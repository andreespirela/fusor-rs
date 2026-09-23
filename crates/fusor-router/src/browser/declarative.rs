//! Public owned route selection, shared by HTML routes and custom routers.
use super::{
    NavigateOptions, NavigationDriver, PreparedNavigation, error,
    navigation::{Browser, BrowserDriver},
};
use crate::{
    AppUrl,
    pattern::{Match, Pattern, path_segments},
};
use fusor::dom::{MountPoint, Scope};
use fusor::{ContextKey, Derived, Owner, OwnerHandle, Signal, batch, derived, signal, untrack};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
use wasm_bindgen::JsValue;

type Factory = dyn Fn(&OwnerHandle, &Match) -> Result<Scope, JsValue>;

/// A lazy branch factory. Unmatched branches are never constructed.
pub struct RouteView {
    pattern: Option<Pattern>,
    render: Box<Factory>,
}
impl RouteView {
    pub fn new(
        pattern: &str,
        render: impl Fn(&OwnerHandle, &Match) -> Result<Scope, JsValue> + 'static,
    ) -> Result<Self, JsValue> {
        Ok(Self {
            pattern: Some(Pattern::new(pattern).map_err(|e| error(&e.to_string()))?),
            render: Box::new(render),
        })
    }
    pub fn fallback(
        render: impl Fn(&OwnerHandle, &Match) -> Result<Scope, JsValue> + 'static,
    ) -> Self {
        Self {
            pattern: None,
            render: Box::new(render),
        }
    }
}
struct Context;
impl ContextKey for Context {
    type Value = BranchContext;
}
struct BranchContext {
    tree: Weak<Tree>,
    prefix: usize,
    children: Rc<RefCell<Vec<Weak<Boundary>>>>,
    initial: AppUrl,
    owner: OwnerHandle,
}
struct NavigationContext;
impl ContextKey for NavigationContext {
    type Value = Weak<Tree>;
}

#[derive(PartialEq)]
struct Identity {
    index: usize,
    matched: Match,
    fallback_path: Option<String>,
}
struct View {
    identity: Identity,
    scope: Scope,
    owner: Owner,
    children: Rc<RefCell<Vec<Weak<Boundary>>>>,
}
struct Boundary {
    parent: OwnerHandle,
    target: MountPoint,
    routes: Vec<RouteView>,
    prefix: usize,
    current: RefCell<Option<Rc<View>>>,
}
struct Tree {
    root: Rc<Boundary>,
    location: Signal<AppUrl>,
    history: RefCell<Option<Rc<Browser<Driver>>>>,
    disposed: Cell<bool>,
    busy: Cell<bool>,
    activating: Cell<bool>,
}

/// Navigation shared by nested routers, independent of the declaring template file.
#[derive(Clone)]
pub struct Navigation(Rc<Tree>, Option<(OwnerHandle, AppUrl)>);
impl Navigation {
    pub fn from_owner(owner: &OwnerHandle) -> Option<Self> {
        if owner.is_disposed() {
            return None;
        }
        owner
            .context::<NavigationContext>()?
            .upgrade()
            .filter(|tree| !tree.disposed.get())
            .map(|tree| {
                Self(
                    tree,
                    owner
                        .context::<Context>()
                        .map(|context| (context.owner.clone(), context.initial.clone())),
                )
            })
    }
    pub fn location(&self) -> Derived<AppUrl> {
        let location = self.0.location.clone();
        let prepared = self.1.clone();
        derived(move || {
            let current = location.get();
            prepared
                .as_ref()
                .filter(|(owner, _)| !owner.is_active() && !owner.is_disposed())
                .map_or(current, |(_, initial)| initial.clone())
        })
    }
    pub fn navigate_url(&self, url: &str, options: NavigateOptions) -> Result<(), JsValue> {
        if self.0.disposed.get() {
            return Err(error("router is disposed"));
        }
        if self.0.busy.get() || self.0.activating.get() {
            return Err(error("reentrant navigation is not supported"));
        }
        let history = self.0.history.borrow().clone();
        if let Some(history) = history {
            history.navigate_url(url, options)
        } else {
            ViewRouter(self.0.clone())
                .navigate(AppUrl::parse(url).map_err(|e| error(&e.to_string()))?)
        }
    }
    pub fn last_error(&self) -> Option<JsValue> {
        self.0
            .history
            .borrow()
            .as_ref()
            .and_then(|browser| browser.error.get())
    }
}

/// Reusable view selector without browser history. Custom routers can stage a
/// transition, publish their own location, then commit; dropping the stage rolls back.
/// Finish a stage synchronously before yielding to the browser: preparation inserts
/// inactive DOM while keeping the previous view alive. Only one stage may exist per tree.
#[derive(Clone)]
pub struct ViewRouter(Rc<Tree>);
impl ViewRouter {
    pub fn mount(
        scope: &mut Scope,
        target: &MountPoint,
        routes: Vec<RouteView>,
        url: AppUrl,
    ) -> Result<Self, JsValue> {
        validate(&routes)?;
        let root = Rc::new(Boundary {
            parent: scope.owner(),
            target: target.clone(),
            routes,
            prefix: 0,
            current: RefCell::new(None),
        });
        let tree = Rc::new(Tree {
            root,
            location: signal(url.clone()),
            history: RefCell::new(None),
            disposed: Cell::new(false),
            busy: Cell::new(false),
            activating: Cell::new(false),
        });
        scope
            .owner()
            .provide::<NavigationContext>(Rc::downgrade(&tree))
            .map_err(|e| error(&e.to_string()))?;
        let pending = Pending::new(tree.clone())?;
        let view = tree.root.prepare(&tree, &url)?;
        view.apply();
        let weak = Rc::downgrade(&tree);
        scope.retain(scope.owner().on_activate(move || {
            if let Some(tree) = weak.upgrade() {
                tree.activate();
            }
        }));
        let weak = Rc::downgrade(&tree);
        scope.retain(scope.owner().on_cleanup(move || {
            if let Some(tree) = weak.upgrade() {
                tree.dispose();
            }
        }));
        scope.retain(RootGuard(tree.clone()));
        drop(pending);
        Ok(Self(tree))
    }
    pub fn navigation(&self) -> Navigation {
        Navigation(self.0.clone(), None)
    }
    pub fn navigate(&self, url: AppUrl) -> Result<(), JsValue> {
        self.prepare_navigation(&url)?.commit();
        Ok(())
    }
}
impl NavigationDriver for ViewRouter {
    fn prepare_navigation(&self, url: &AppUrl) -> Result<Box<dyn PreparedNavigation>, JsValue> {
        if self.0.disposed.get() {
            return Err(error("router is disposed"));
        }
        if self.0.busy.get() || self.0.activating.get() {
            return Err(error("reentrant navigation is not supported"));
        }
        let pending = Pending::new(self.0.clone())?;
        Ok(Box::new(Transaction {
            _pending: pending,
            plan: self.0.root.prepare(&self.0, url)?,
            tree: self.0.clone(),
            url: url.clone(),
        }))
    }
}
struct RootGuard(Rc<Tree>);
impl Drop for RootGuard {
    fn drop(&mut self) {
        self.0.dispose();
    }
}
impl Tree {
    fn activate(&self) {
        struct Reset<'a>(&'a Cell<bool>);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        if self.activating.replace(true) {
            return;
        }
        let _reset = Reset(&self.activating);
        self.root.activate();
    }
    fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }
        self.root.current.take();
        if let Some(router) = self.history.take() {
            router.dispose();
        }
    }
}
struct Driver(Weak<Tree>);
impl NavigationDriver for Driver {
    fn prepare_navigation(&self, url: &AppUrl) -> Result<Box<dyn PreparedNavigation>, JsValue> {
        ViewRouter(
            self.0
                .upgrade()
                .ok_or_else(|| error("router is disposed"))?,
        )
        .prepare_navigation(url)
    }
}
impl BrowserDriver for Driver {
    fn accepts_link(&self, _: &AppUrl) -> bool {
        true // Declarative matching, including fallbacks, owns application paths.
    }
    fn retains_view(&self, _: &AppUrl) -> bool {
        true // Fragment changes still prepare the real tree through the driver.
    }
}
// Only one staged transition can own a tree. A failed or abandoned preparation
// releases this guard, leaving the current view and URL unchanged.
struct Pending(Rc<Tree>);
impl Pending {
    fn new(tree: Rc<Tree>) -> Result<Self, JsValue> {
        if tree.busy.replace(true) {
            return Err(error("router already has a prepared navigation"));
        }
        Ok(Self(tree))
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.0.busy.set(false);
    }
}
struct Transaction {
    _pending: Pending,
    plan: Plan,
    tree: Rc<Tree>,
    url: AppUrl,
}
impl PreparedNavigation for Transaction {
    fn commit(self: Box<Self>) {
        if self.tree.disposed.get() {
            return;
        }
        batch(|| {
            self.plan.apply();
            self.tree.location.set(self.url);
            self.tree.activate();
        });
    }
}
enum Plan {
    Keep(Vec<Plan>),
    Replace(Rc<Boundary>, Option<Rc<View>>),
}
impl Plan {
    fn apply(self) {
        match self {
            Self::Keep(children) => {
                for child in children {
                    child.apply();
                }
            }
            Self::Replace(boundary, view) => {
                let old = boundary.current.replace(view);
                drop(old);
            }
        }
    }
}
impl Boundary {
    fn prepare(self: &Rc<Self>, tree: &Rc<Tree>, url: &AppUrl) -> Result<Plan, JsValue> {
        let segments = path_segments(url).map_err(|e| error(&e.to_string()))?;
        let selected = self
            .routes
            .iter()
            .enumerate()
            .filter_map(|(i, route)| {
                route
                    .pattern
                    .as_ref()?
                    .matches(&segments, self.prefix)
                    .map(|m| (i, m, route.pattern.as_ref().unwrap().priority()))
            })
            .max_by(|a, b| a.2.cmp(&b.2))
            .map(|(index, matched, _)| Identity {
                index,
                matched,
                fallback_path: None,
            })
            .or_else(|| {
                self.routes
                    .iter()
                    .position(|route| route.pattern.is_none())
                    .map(|index| Identity {
                        index,
                        matched: Match {
                            params: Default::default(),
                            consumed: segments.len(),
                        },
                        fallback_path: Some(url.path.clone()),
                    })
            });
        let same = self
            .current
            .borrow()
            .as_ref()
            .is_some_and(|view| selected.as_ref() == Some(&view.identity));
        if same {
            let children = self
                .current
                .borrow()
                .as_ref()
                .unwrap()
                .children
                .borrow()
                .iter()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>();
            return Ok(Plan::Keep(
                children
                    .into_iter()
                    .filter(|c| !c.parent.is_disposed())
                    .map(|child| child.prepare(tree, url))
                    .collect::<Result<_, _>>()?,
            ));
        }
        let Some(identity) = selected else {
            return Ok(Plan::Replace(self.clone(), None));
        };
        let owner = Owner::child(&self.parent);
        let children = Rc::new(RefCell::new(Vec::new()));
        owner
            .handle()
            .provide::<Context>(BranchContext {
                tree: Rc::downgrade(tree),
                prefix: identity.matched.consumed,
                children: children.clone(),
                initial: url.clone(),
                owner: owner.handle(),
            })
            .map_err(|e| error(&e.to_string()))?;
        let mut scope =
            untrack(|| (self.routes[identity.index].render)(&owner.handle(), &identity.matched))?;
        if scope.owner().is_active() || !scope.owner().is_child_of(&owner.handle()) {
            return Err(error(
                "route views must return a prepared child of their supplied owner",
            ));
        }
        scope.attach_at(&self.target)?;
        scope.finish_prepare()?;
        if self.parent.is_active() {
            scope.finish_prepare_subtree()?;
        }
        Ok(Plan::Replace(
            self.clone(),
            Some(Rc::new(View {
                identity,
                scope,
                owner,
                children,
            })),
        ))
    }
    fn activate(&self) {
        let current = self.current.borrow().clone();
        if let Some(view) = current.as_ref() {
            view.owner.commit();
            view.scope.commit();
            let children = view
                .children
                .borrow()
                .iter()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>();
            drop(current);
            for child in children {
                if !child.parent.is_disposed() {
                    child.activate();
                }
            }
        }
    }
}
fn validate(routes: &[RouteView]) -> Result<(), JsValue> {
    for (i, route) in routes.iter().enumerate() {
        if routes[..i]
            .iter()
            .any(|other| match (&route.pattern, &other.pattern) {
                (Some(a), Some(b)) => a.conflicts(b),
                (None, None) => true,
                _ => false,
            })
        {
            return Err(error(
                "Router contains ambiguous routes or multiple fallbacks",
            ));
        }
    }
    Ok(())
}
/// Mount an HTML route selector. Descendants inherit the nearest matched prefix;
/// the first selector establishes browser navigation and owns its cleanup.
pub fn mount_routes(
    scope: &mut Scope,
    target: &MountPoint,
    base: &str,
    routes: Vec<RouteView>,
) -> Result<(), JsValue> {
    validate(&routes)?;
    if let Some(context) = scope.owner().context::<Context>() {
        let tree = context
            .tree
            .upgrade()
            .ok_or_else(|| error("parent router is disposed"))?;
        let boundary = Rc::new(Boundary {
            parent: scope.owner(),
            target: target.clone(),
            routes,
            prefix: context.prefix,
            current: RefCell::new(None),
        });
        let url = if context.owner.is_active() {
            tree.location.get_untracked()
        } else {
            context.initial.clone()
        };
        boundary.prepare(&tree, &url)?.apply();
        context
            .children
            .borrow_mut()
            .retain(|child| child.strong_count() != 0);
        context.children.borrow_mut().push(Rc::downgrade(&boundary));
        let weak = Rc::downgrade(&boundary);
        scope.retain(scope.owner().on_activate(move || {
            if let Some(boundary) = weak.upgrade() {
                boundary.activate();
            }
        }));
        scope.retain(boundary);
        return Ok(());
    }
    let browser = Browser::prepare(&scope.owner(), base, target.parent_element()?, |_, url| {
        let views = ViewRouter::mount(scope, target, routes, url)?;
        Ok(Driver(Rc::downgrade(&views.0)))
    })?;
    browser.before_commit(scope)?;
    let tree = browser
        .driver
        .0
        .upgrade()
        .expect("scope retains the route tree");
    *tree.history.borrow_mut() = Some(browser);
    Ok(())
}

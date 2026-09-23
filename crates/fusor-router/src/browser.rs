//! Owned browser navigation and composable route views.
pub mod declarative;
mod navigation;
use crate::{AppUrl, Location, Route};
use fusor::dom::{Content, Scope};
use fusor::{ContextKey, Derived, OwnerHandle, Signal, batch, derived, signal, untrack};
use navigation::{Browser, BrowserDriver};
use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::{Rc, Weak},
};
use wasm_bindgen::JsValue;
use web_sys::Element;

fn error(message: &str) -> JsValue {
    JsValue::from_str(message)
}

/// Defaults match following a link to new content. Query updates may opt to
/// preserve focus and scroll. Back/forward uses native browser scroll restoration.
#[derive(Clone, Copy, Debug, Default)]
pub struct NavigateOptions {
    pub replace: bool,
    pub keep_focus: bool,
    pub keep_scroll: bool,
}

/// Input to a route constructor. Use `Component::prepare(&context.parent, ...)`
/// to create the view. Location is reactive and belongs to this view instance.
pub struct RouteContext<R: Route> {
    pub parent: OwnerHandle,
    pub location: Derived<Location<R>>,
}
struct View<R: Route> {
    location: Signal<Location<R>>,
    scope: Scope,
}
type Render<R> = dyn Fn(RouteContext<R>) -> Result<Scope, JsValue>;
/// Prepared view work. Dropping it must roll back without disposing the current view.
pub trait PreparedNavigation {
    fn commit(self: Box<Self>);
}

/// A view manager participating in the browser history transaction.
pub trait NavigationDriver {
    fn prepare_navigation(&self, url: &AppUrl) -> Result<Box<dyn PreparedNavigation>, JsValue>;
}

struct Outlet<R: Route> {
    owner: OwnerHandle,
    container: Element,
    render: Box<Render<R>>,
    view: RefCell<Option<View<R>>>,
    location: Signal<Location<R>>,
}

struct RouterContext<R: Route>(PhantomData<R>);
impl<R: Route> ContextKey for RouterContext<R> {
    type Value = RefCell<Weak<Browser<Rc<Outlet<R>>>>>;
}

/// Mount a typed router from Rust. HTML applications normally use Router/Route.
/// Data loading remains in application constructors.
pub fn mount_outlet<R: Route>(
    scope: &mut Scope,
    container: &Element,
    base: &str,
    render: impl Fn(RouteContext<R>) -> Content + 'static,
) -> Result<(), JsValue> {
    let router = Router::prepare(&scope.owner(), base, container.clone(), move |context| {
        let parent = context.parent.clone();
        render(context).prepare(&parent)
    })?;
    router.0.before_commit(scope)?;
    scope.retain(router);
    Ok(())
}

/// Retain this handle for the router's lifetime. Clones share one router. Owner
/// disposal detaches listeners and disposes the current route even if a handle survives.
pub struct Router<R: Route>(Rc<Browser<Rc<Outlet<R>>>>);
impl<R: Route> Clone for Router<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<R: Route> Router<R> {
    /// Mount into an empty element. The renderer must return a prepared child of
    /// the supplied owner; using `Component::mount` here is an error.
    pub fn mount(
        parent: &OwnerHandle,
        base: &str,
        container: Element,
        render: impl Fn(RouteContext<R>) -> Result<Scope, JsValue> + 'static,
    ) -> Result<Self, JsValue> {
        let router = Self::prepare(parent, base, container, render)?;
        router.0.finish_mount()?;
        Ok(router)
    }

    fn prepare(
        parent: &OwnerHandle,
        base: &str,
        container: Element,
        render: impl Fn(RouteContext<R>) -> Result<Scope, JsValue> + 'static,
    ) -> Result<Self, JsValue> {
        if parent.is_disposed() {
            return Err(error("router parent is disposed"));
        }
        if container.child_element_count() != 0
            || container
                .text_content()
                .is_some_and(|text| !text.trim().is_empty())
        {
            return Err(error("router outlet must be empty"));
        }
        let inner = Browser::prepare(parent, base, container.clone(), |owner, url| {
            Ok(Rc::new(Outlet {
                owner: owner.clone(),
                container,
                render: Box::new(render),
                view: RefCell::new(None),
                location: signal(Location::new(url)),
            }))
        })?;
        if parent.context::<RouterContext<R>>().is_none() {
            parent
                .provide::<RouterContext<R>>(RefCell::new(Weak::new()))
                .map_err(|e| error(&e.to_string()))?;
        }
        *parent
            .context::<RouterContext<R>>()
            .expect("provided router context")
            .borrow_mut() = Rc::downgrade(&inner);
        let view = inner
            .driver
            .prepare(inner.driver.location.get_untracked())?;
        *inner.driver.view.borrow_mut() = Some(view);
        Ok(Self(inner))
    }

    /// Obtain the router provided by the nearest containing outlet component.
    /// Lookup is explicit and does not subscribe a reactive dependency.
    pub fn from_owner(owner: &OwnerHandle) -> Option<Self> {
        if owner.is_disposed() {
            return None;
        }
        owner
            .context::<RouterContext<R>>()?
            .borrow()
            .upgrade()
            .filter(|inner| !inner.is_disposed())
            .map(Self)
    }
    pub fn location(&self) -> Location<R> {
        self.0.driver.location.get()
    }
    pub fn last_error(&self) -> Option<JsValue> {
        self.0.error.get()
    }
    pub fn href(&self, route: &R) -> Result<String, JsValue> {
        self.0.base.href(route).map_err(|e| error(&e.to_string()))
    }
    pub fn navigate(&self, route: &R, options: NavigateOptions) -> Result<(), JsValue> {
        self.navigate_url(&self.href(route)?, options)
    }
    /// Resolve a browser URL, enforcing same origin and application base. Unknown
    /// routes reach the renderer's `None` branch; external URLs are rejected.
    pub fn navigate_url(&self, href: &str, options: NavigateOptions) -> Result<(), JsValue> {
        self.0.navigate_url(href, options)
    }
    pub fn dispose(&self) {
        self.0.dispose();
    }
}
impl<R: Route> Outlet<R> {
    fn prepare(&self, location: Location<R>) -> Result<View<R>, JsValue> {
        let location = signal(location);
        let read = location.clone();
        let mut scope = untrack(|| {
            (self.render)(RouteContext {
                parent: self.owner.clone(),
                location: derived(move || read.get()),
            })
        })?;
        if scope.owner().is_active()
            || scope.owner().is_disposed()
            || !scope.owner().is_child_of(&self.owner)
            || scope.root().is_connected()
        {
            return Err(error(
                "route renderer must return a detached Component::prepare child of the supplied parent",
            ));
        }
        scope.attach(&self.container)?;
        scope.finish_prepare()?;
        Ok(View { location, scope })
    }
    fn same_route(&self, next: &Location<R>) -> bool {
        // Unknown URLs are distinct destinations, even though both parse as None.
        self.view.borrow().as_ref().is_some_and(|view| {
            view.location.with_untracked(|old| {
                old.route == next.route && (old.route.is_some() || old.url.path == next.url.path)
            })
        })
    }
    fn commit(&self, next: Location<R>, staged: Option<View<R>>) {
        batch(|| {
            if let Some(view) = staged {
                let old = self.view.replace(Some(view));
                drop(old); // cancel the old route before activating the destination
            }
            let location = self
                .view
                .borrow()
                .as_ref()
                .map(|view| view.location.clone());
            if let Some(location) = location {
                location.set(next.clone());
            }
            self.location.set(next);
            // No RefCell borrow spans activation callbacks.
            self.activate_view();
        });
    }
    fn activate_view(&self) {
        let view = self.view.take();
        if let Some(view) = &view {
            view.scope.commit();
        }
        if !self.owner.is_disposed() {
            *self.view.borrow_mut() = view;
        }
    }
}
impl<R: Route> NavigationDriver for Rc<Outlet<R>> {
    fn prepare_navigation(&self, url: &AppUrl) -> Result<Box<dyn PreparedNavigation>, JsValue> {
        let next = Location::new(url.clone());
        let staged = if self.same_route(&next) {
            None
        } else {
            Some(self.prepare(next.clone())?)
        };
        Ok(Box::new(OutletNavigation {
            outlet: self.clone(),
            next,
            staged,
        }))
    }
}
struct OutletNavigation<R: Route> {
    outlet: Rc<Outlet<R>>,
    next: Location<R>,
    staged: Option<View<R>>,
}
impl<R: Route> PreparedNavigation for OutletNavigation<R> {
    fn commit(self: Box<Self>) {
        if !self.outlet.owner.is_disposed() {
            self.outlet.commit(self.next, self.staged);
        }
    }
}
impl<R: Route> BrowserDriver for Rc<Outlet<R>> {
    fn accepts_link(&self, url: &AppUrl) -> bool {
        R::parse(url).is_some()
    }
    fn retains_view(&self, url: &AppUrl) -> bool {
        self.same_route(&Location::new(url.clone()))
    }
    fn activate(&self) {
        self.activate_view();
    }
    fn dispose(&self) {
        let view = self.view.take();
        drop(view);
    }
}

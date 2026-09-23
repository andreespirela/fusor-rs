//! Shared browser history transaction. Route identity and view ownership stay in drivers.
use super::{NavigateOptions, NavigationDriver, PreparedNavigation, error};
use crate::{AppUrl, BasePath};
use fusor::dom::{Scope, document};
use fusor::{Owner, OwnerHandle, Registration, Signal, batch, signal};
use js_sys::{Object, Reflect};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{
    Element, Event, EventTarget, FocusOptions, History, HtmlAnchorElement, HtmlElement, MouseEvent,
    Url, Window,
};

thread_local! {
    static TAKEN: Cell<bool> = const { Cell::new(false) };
    static SESSION: Cell<u64> = const { Cell::new(0) };
}
struct Lease(Cell<bool>);
impl Lease {
    fn acquire() -> Result<Self, JsValue> {
        if TAKEN.with(|taken| taken.replace(true)) {
            return Err(error("only one fusor router may own browser history"));
        }
        Ok(Self(Cell::new(true)))
    }
    fn release(&self) {
        if self.0.replace(false) {
            TAKEN.with(|taken| taken.set(false));
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.release();
    }
}
struct Listener {
    target: EventTarget,
    name: &'static str,
    callback: Closure<dyn FnMut(Event)>,
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .target
            .remove_event_listener_with_callback(self.name, self.callback.as_ref().unchecked_ref());
    }
}
impl Listener {
    fn new(
        target: EventTarget,
        name: &'static str,
        callback: impl FnMut(Event) + 'static,
    ) -> Result<Self, JsValue> {
        let callback = Closure::wrap(Box::new(callback) as Box<dyn FnMut(Event)>);
        target.add_event_listener_with_callback(name, callback.as_ref().unchecked_ref())?;
        Ok(Self {
            target,
            name,
            callback,
        })
    }
}

// Only browser-specific policy belongs here. The public prepared-navigation
// contract also remains usable by selectors with no browser history.
pub(super) trait BrowserDriver: NavigationDriver + 'static {
    fn accepts_link(&self, url: &AppUrl) -> bool;
    fn retains_view(&self, url: &AppUrl) -> bool;
    fn activate(&self) {}
    fn dispose(&self) {}
}

pub(super) struct Browser<D: BrowserDriver> {
    owner: Owner,
    lease: Lease,
    window: Window,
    history: History,
    pub(super) base: BasePath,
    origin: String,
    session: String,
    epoch: Cell<u32>,
    index: Cell<i32>,
    recovering: Cell<Option<i32>>,
    busy: Cell<bool>,
    disposed: Cell<bool>,
    presentation: Element,
    pub(super) driver: D,
    location: RefCell<AppUrl>,
    pub(super) error: Signal<Option<JsValue>>,
    listeners: RefCell<Vec<Listener>>,
    cleanup: RefCell<Option<Registration>>,
    activation: RefCell<Option<Registration>>,
    original_history: RefCell<Option<JsValue>>,
}

impl<D: BrowserDriver> Browser<D> {
    pub(super) fn prepare(
        parent: &OwnerHandle,
        base: &str,
        presentation: Element,
        make_driver: impl FnOnce(&OwnerHandle, AppUrl) -> Result<D, JsValue>,
    ) -> Result<Rc<Self>, JsValue> {
        if parent.is_disposed() {
            return Err(error("router parent is disposed"));
        }
        let lease = Lease::acquire()?;
        let base = BasePath::new(base).map_err(|e| error(&e.to_string()))?;
        let window = web_sys::window().ok_or_else(|| error("router requires a browser"))?;
        let history = window.history()?;
        let url = Url::new(&window.location().href()?)?;
        let location = relative(&base, &url)?;
        let owner = Owner::child(parent);
        let driver = make_driver(&owner.handle(), location.clone())?;
        let session = SESSION.with(|counter| {
            let n = counter.get() + 1;
            counter.set(n);
            format!("{}-{n}", js_sys::Date::now())
        });
        let inner = Rc::new(Self {
            owner,
            lease,
            window,
            history,
            base,
            origin: url.origin(),
            session,
            epoch: Cell::new(0),
            index: Cell::new(0),
            recovering: Cell::new(None),
            busy: Cell::new(false),
            disposed: Cell::new(false),
            presentation,
            driver,
            location: RefCell::new(location),
            error: signal(None),
            listeners: RefCell::new(Vec::new()),
            cleanup: RefCell::new(None),
            activation: RefCell::new(None),
            original_history: RefCell::new(None),
        });
        let weak = Rc::downgrade(&inner);
        *inner.cleanup.borrow_mut() = Some(parent.on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                inner.dispose();
            }
        }));
        Ok(inner)
    }
    pub(super) fn before_commit(self: &Rc<Self>, scope: &mut Scope) -> Result<(), JsValue> {
        let weak = Rc::downgrade(self);
        scope.before_commit(move || {
            if let Some(inner) = weak.upgrade().filter(|inner| !inner.is_disposed()) {
                inner.finish_mount()?;
            }
            Ok(())
        })
    }
    pub(super) fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    pub(super) fn navigate_url(&self, href: &str, options: NavigateOptions) -> Result<(), JsValue> {
        let result = self.navigate(href, options);
        if let Err(error) = &result {
            self.report(error.clone());
        }
        result
    }
}
fn relative(base: &BasePath, url: &Url) -> Result<AppUrl, JsValue> {
    base.strip(&format!("{}{}{}", url.pathname(), url.search(), url.hash()))
        .map_err(|e| error(&e.to_string()))
}
struct Busy<'a>(&'a Cell<bool>);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl<D: BrowserDriver> Browser<D> {
    pub(super) fn finish_mount(self: &Rc<Self>) -> Result<(), JsValue> {
        if self.disposed.get() {
            return Err(error("router is disposed"));
        }
        self.install_listeners()?;
        let original = self.history.state()?;
        // This runs only after the complete containing component was prepared.
        self.history
            .replace_state_with_url(&self.state(0)?, "", None)?;
        *self.original_history.borrow_mut() = Some(original);
        let weak = Rc::downgrade(self);
        *self.activation.borrow_mut() = Some(self.owner.handle().on_activate(move || {
            if let Some(inner) = weak.upgrade() {
                inner.original_history.borrow_mut().take();
            }
        }));
        self.owner.commit();
        self.driver.activate();
        Ok(())
    }
    fn enter(&self) -> Result<Busy<'_>, JsValue> {
        if self.disposed.get() || !self.owner.handle().is_active() {
            return Err(error("router is not active"));
        }
        if self.recovering.get().is_some() {
            return Err(error(
                "router is restoring browser history after a failed navigation",
            ));
        }
        if self.busy.replace(true) {
            return Err(error("reentrant navigation is not supported"));
        }
        Ok(Busy(&self.busy))
    }
    fn state(&self, index: i32) -> Result<JsValue, JsValue> {
        let state = Object::new();
        let previous = self.history.state()?;
        if previous.is_object() && !previous.is_null() {
            Object::assign(&state, &previous.unchecked_into::<Object>());
        } else if !previous.is_null() && !previous.is_undefined() {
            Reflect::set(&state, &"user".into(), &previous)?;
        }
        let metadata = Object::new();
        Reflect::set(&metadata, &"session".into(), &self.session.clone().into())?;
        Reflect::set(&metadata, &"index".into(), &index.into())?;
        Reflect::set(&metadata, &"epoch".into(), &self.epoch.get().into())?;
        Reflect::set(&state, &"__fusor".into(), &metadata)?;
        Ok(state.into())
    }
    fn history_index(&self) -> Option<i32> {
        let state = self.history.state().ok()?;
        let meta = Reflect::get(&state, &"__fusor".into()).ok()?;
        if Reflect::get(&meta, &"session".into()).ok()?.as_string()? != self.session {
            return None;
        }
        if Reflect::get(&meta, &"epoch".into()).ok()?.as_f64()? != f64::from(self.epoch.get()) {
            return None;
        }
        let index = Reflect::get(&meta, &"index".into()).ok()?.as_f64()?;
        (index.is_finite() && index.fract() == 0.0 && index >= 0.0 && index <= i32::MAX as f64)
            .then_some(index as i32)
    }
    fn navigate(&self, href: &str, options: NavigateOptions) -> Result<(), JsValue> {
        let _busy = self.enter()?;
        let url = Url::new_with_base(href, &self.window.location().href()?)?;
        if url.origin() != self.origin || !url.username().is_empty() || !url.password().is_empty() {
            return Err(error("navigation must stay on the application origin"));
        }
        let next = relative(&self.base, &url)?;
        if *self.location.borrow() == next {
            return Ok(());
        }
        let index = if options.replace {
            self.index.get()
        } else {
            self.index
                .get()
                .checked_add(1)
                .ok_or_else(|| error("history index overflow"))?
        };
        // Preparation and append may fail. Neither changes history or disposes old state.
        let transaction = self.driver.prepare_navigation(&next)?;
        let state = self.state(index)?;
        if options.replace {
            self.history
                .replace_state_with_url(&state, "", Some(&url.href()))?;
        } else {
            self.history
                .push_state_with_url(&state, "", Some(&url.href()))?;
        }
        self.index.set(index);
        self.commit(next.clone(), transaction);
        self.present(&next, options, false);
        Ok(())
    }
    fn commit(&self, next: AppUrl, transaction: Box<dyn PreparedNavigation>) {
        batch(|| {
            *self.location.borrow_mut() = next;
            self.error.set(None);
            transaction.commit();
        });
    }
    fn present(&self, url: &AppUrl, options: NavigateOptions, traversal: bool) {
        if self.disposed.get() {
            return;
        }
        if !options.keep_focus {
            let target = self
                .presentation
                .query_selector("[autofocus]")
                .ok()
                .flatten()
                .or_else(|| self.presentation.query_selector("h1").ok().flatten());
            if let Some(node) = target {
                if let Some(node) = node.dyn_ref::<HtmlElement>() {
                    if !node.has_attribute("tabindex") && node.tab_index() < 0 {
                        let _ = node.set_attribute("tabindex", "-1");
                    }
                    let options = FocusOptions::new();
                    options.set_prevent_scroll(true);
                    let _ = node.focus_with_options(&options);
                }
            }
        }
        if !options.keep_scroll && !traversal {
            if !url.fragment.is_empty() {
                if let Ok(id) = percent_encoding::percent_decode_str(&url.fragment).decode_utf8() {
                    if let Some(target) = self
                        .window
                        .document()
                        .and_then(|d| d.get_element_by_id(&id))
                    {
                        target.scroll_into_view();
                        return;
                    }
                }
            }
            self.window.scroll_to_with_x_and_y(0.0, 0.0);
        }
    }
    fn pop(&self) -> Result<(), JsValue> {
        if self.disposed.get() {
            return Ok(());
        }
        let url = Url::new(&self.window.location().href()?)?;
        let next = match relative(&self.base, &url) {
            Ok(url) => url,
            Err(_) => return self.window.location().reload(),
        };
        let index = self.history_index();
        if let Some(expected) = self.recovering.get() {
            if index == Some(expected) && *self.location.borrow() == next {
                self.recovering.set(None);
                return Ok(());
            }
            // A second traversal overtook restoration. Reload the actual URL;
            // never publish a view for a different address-bar location.
            return self.window.location().reload();
        }
        if *self.location.borrow() == next {
            return Ok(());
        }
        let fragment_only =
            self.location.borrow().path == next.path && self.location.borrow().query == next.query;
        if fragment_only && self.driver.retains_view(&next) {
            let _busy = self.enter()?;
            // Native fragment entries need not carry our state or a unique index.
            // Start a new known segment rather than inventing a traversal delta.
            // Traversing beyond it can fall back to a document navigation.
            self.epoch.set(
                self.epoch
                    .get()
                    .checked_add(1)
                    .ok_or_else(|| error("history epoch overflow"))?,
            );
            self.index.set(0);
            self.history
                .replace_state_with_url(&self.state(0)?, "", None)?;
            let transaction = self.driver.prepare_navigation(&next)?;
            self.commit(next, transaction);
            return Ok(());
        }
        if index.is_none() && !fragment_only {
            return self.window.location().reload();
        }
        let _busy = self.enter()?;
        let result = (|| {
            let transaction = self.driver.prepare_navigation(&next)?;
            self.commit(next.clone(), transaction);
            self.index.set(index.unwrap_or(self.index.get()));
            if !fragment_only {
                self.present(&next, NavigateOptions::default(), true);
            }
            Ok(())
        })();
        if result.is_err() {
            if let Some(index) = index.filter(|index| *index != self.index.get()) {
                self.recovering.set(Some(self.index.get()));
                self.history.go_with_delta(self.index.get() - index)?;
            } else {
                self.window.location().reload()?;
            }
        }
        result
    }
    fn clicked(&self, event: Event) -> Result<(), JsValue> {
        let Some(mouse) = event.dyn_ref::<MouseEvent>() else {
            return Ok(());
        };
        if event.default_prevented()
            || mouse.button() != 0
            || mouse.ctrl_key()
            || mouse.meta_key()
            || mouse.alt_key()
            || mouse.shift_key()
        {
            return Ok(());
        }
        let anchor = event
            .composed_path()
            .iter()
            .find_map(|node| node.dyn_into::<HtmlAnchorElement>().ok());
        let Some(anchor) = anchor else {
            return Ok(());
        };
        if !anchor.has_attribute("data-fusor-link")
            || anchor.has_attribute("download")
            || anchor
                .rel()
                .split_whitespace()
                .any(|part| part.eq_ignore_ascii_case("external"))
        {
            return Ok(());
        }
        let target = if anchor.target().is_empty() {
            document()?
                .query_selector("base[target]")?
                .and_then(|base| base.get_attribute("target"))
                .unwrap_or_default()
        } else {
            anchor.target()
        };
        if !target.is_empty() && !target.eq_ignore_ascii_case("_self") {
            return Ok(());
        }
        let url = Url::new(&anchor.href())?;
        if url.origin() != self.origin {
            return Ok(());
        }
        let Ok(relative) = relative(&self.base, &url) else {
            return Ok(());
        };
        if !self.driver.accepts_link(&relative) {
            return Ok(());
        }
        let fragment = anchor
            .get_attribute("href")
            .is_some_and(|href| href.starts_with('#'))
            || {
                let old = self.location.borrow();
                old.path == relative.path
                    && old.query == relative.query
                    && !relative.fragment.is_empty()
            };
        if fragment {
            return Ok(());
        }
        event.prevent_default();
        self.navigate(&url.href(), NavigateOptions::default())
    }
    fn install_listeners(self: &Rc<Self>) -> Result<(), JsValue> {
        for name in ["popstate", "hashchange"] {
            let weak = Rc::downgrade(self);
            let listener = Listener::new(self.window.clone().into(), name, move |_| {
                if let Some(inner) = weak
                    .upgrade()
                    .filter(|inner| inner.owner.handle().is_active())
                {
                    if let Err(error) = inner.pop() {
                        inner.report(error);
                    }
                }
            })?;
            self.listeners.borrow_mut().push(listener);
        }
        let weak = Rc::downgrade(self);
        let listener = Listener::new(document()?.into(), "click", move |event| {
            if let Some(inner) = weak
                .upgrade()
                .filter(|inner| inner.owner.handle().is_active())
            {
                if let Err(error) = inner.clicked(event) {
                    inner.report(error);
                }
            }
        })?;
        self.listeners.borrow_mut().push(listener);
        Ok(())
    }
    fn report(&self, error: JsValue) {
        web_sys::console::error_1(&error);
        self.error.set(Some(error));
    }
    pub(super) fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }
        self.owner.dispose();
        let listeners = self.listeners.take();
        drop(listeners);
        self.driver.dispose();
        if let Some(original) = self.original_history.take() {
            if self.history_index() == Some(0) {
                if let Err(error) = self.history.replace_state_with_url(&original, "", None) {
                    web_sys::console::error_1(&error);
                }
            }
        }
        self.lease.release();
    }
}
impl<D: BrowserDriver> Drop for Browser<D> {
    fn drop(&mut self) {
        self.dispose();
    }
}

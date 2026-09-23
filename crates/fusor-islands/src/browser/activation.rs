//! One activation attempt. Pending previews own prepared scopes, not visible DOM.
use fusor::{
    Effect,
    dom::{Scope, delivery::PreviewReadiness},
    effect, untrack,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::JsValue;
use web_sys::Element;

pub(super) struct Prepared {
    pub scope: Scope,
    pub preview: Option<Preview>,
}
pub(super) struct Preview {
    pub host: Element,
    pub initial: Element,
    pub readiness: PreviewReadiness,
    attributes: Vec<(&'static str, Option<String>)>,
    props: Element,
    text: String,
}
impl Preview {
    pub fn new(
        host: &Element,
        initial: Element,
        readiness: PreviewReadiness,
        text: &str,
    ) -> Result<Self, JsValue> {
        let props = host
            .query_selector(":scope > script[data-rf-props]")?
            .ok_or_else(|| JsValue::from_str("missing island props"))?;
        let attributes = [
            "id",
            "data-rf-island",
            "data-rf-unit",
            "data-rf-generation",
            "data-rf-schema",
            "data-rf-hash",
            "data-rf-activate",
            "data-rf-prefetch",
        ]
        .into_iter()
        .map(|name| (name, host.get_attribute(name)))
        .collect();
        Ok(Self {
            host: host.clone(),
            initial,
            readiness,
            attributes,
            props,
            text: text.into(),
        })
    }
    fn valid(&self) -> bool {
        self.host.is_connected()
            && self.initial.parent_element().as_ref() == Some(&self.host)
            && self.props.parent_element().as_ref() == Some(&self.host)
            && self.props.get_attribute("type").as_deref() == Some("application/json")
            && self.props.has_attribute("data-rf-props")
            && self.props.text_content().as_deref() == Some(self.text.as_str())
            && self
                .attributes
                .iter()
                .all(|(name, value)| self.host.get_attribute(name) == *value)
    }
}

pub(super) struct Activation {
    prepared: Prepared,
    promise: js_sys::Promise,
    resolve: js_sys::Function,
    reject: js_sys::Function,
    driver: RefCell<Option<Effect>>,
    scheduled: Cell<bool>,
    done: Cell<bool>,
}
impl Activation {
    pub fn new(prepared: Prepared) -> Rc<Self> {
        let mut callbacks = None;
        let promise =
            js_sys::Promise::new(&mut |resolve, reject| callbacks = Some((resolve, reject)));
        let (resolve, reject) = callbacks.expect("Promise executor is synchronous");
        Rc::new(Self {
            prepared,
            promise,
            resolve,
            reject,
            driver: RefCell::new(None),
            scheduled: Cell::new(false),
            done: Cell::new(false),
        })
    }
    pub fn promise(&self) -> js_sys::Promise {
        self.promise.clone()
    }
    pub fn start(self: &Rc<Self>) {
        if self.prepared.preview.is_none() {
            self.finish(self.prepared.scope.try_commit());
            return;
        }
        let weak = Rc::downgrade(self);
        let driver = effect(move || {
            let Some(activation) = weak.upgrade().filter(|activation| !activation.done.get())
            else {
                return;
            };
            let readiness = activation
                .prepared
                .preview
                .as_ref()
                .expect("preview")
                .readiness
                .poll();
            if matches!(readiness, Ok(false)) || activation.scheduled.replace(true) {
                return;
            }
            let weak = weak.clone();
            // Let structural preparation and reactive propagation settle. Recheck
            // readiness and identity before publishing; no user callbacks run in
            // the gap between native inspection and initial DOM replacement.
            wasm_bindgen_futures::spawn_local(async move {
                let Some(activation) = weak.upgrade().filter(|activation| !activation.done.get())
                else {
                    return;
                };
                activation.scheduled.set(false);
                match untrack(|| {
                    activation
                        .prepared
                        .preview
                        .as_ref()
                        .expect("preview")
                        .readiness
                        .poll()
                }) {
                    Ok(false) => {}
                    Ok(true) => activation.finish(activation.publish()),
                    Err(error) => activation.finish(Err(JsValue::from_str(&format!(
                        "initial coherent view failed: {error}"
                    )))),
                }
            });
        });
        if !self.done.get() {
            *self.driver.borrow_mut() = Some(driver);
        }
    }
    fn publish(&self) -> Result<(), JsValue> {
        let preview = self.prepared.preview.as_ref().expect("preview");
        if !preview.valid() || self.prepared.scope.owner().is_disposed() {
            return Err(JsValue::from_str(
                "preview registration changed while loading",
            ));
        }
        preview.host.append_child(self.prepared.scope.root())?;
        // Fallible setup runs while the native fallback is still owned by its
        // host; failure removes the candidate and leaves that fallback intact.
        self.prepared.scope.finish_prepare()?;
        if !preview.valid() {
            return Err(JsValue::from_str(
                "preview registration changed during setup",
            ));
        }
        preview.initial.remove();
        let result = self.prepared.scope.try_commit();
        if result.is_err() && preview.host.is_connected() {
            let _ = preview
                .host
                .insert_before(&preview.initial, Some(self.prepared.scope.root()));
        }
        result
    }
    fn finish(&self, result: Result<(), JsValue>) {
        if self.done.replace(true) {
            return;
        }
        self.driver.borrow_mut().take();
        if let Some(preview) = &self.prepared.preview {
            preview.readiness.close();
        }
        match result {
            Ok(()) => {
                let _ = self.resolve.call0(&JsValue::UNDEFINED);
            }
            Err(error) => {
                self.prepared.scope.dispose();
                if self.prepared.preview.is_some() {
                    self.prepared.scope.root().remove();
                }
                let _ = self.reject.call1(&JsValue::UNDEFINED, &error);
            }
        }
    }
    pub fn dispose(&self) {
        self.finish(Err(JsValue::from_str("island activation was cancelled")));
        self.prepared.scope.dispose();
    }
}
impl Drop for Activation {
    fn drop(&mut self) {
        self.dispose();
    }
}

//! The narrow Wasm entry and typed control surface. All loading and scheduling
//! belong to the shared JavaScript registry, not to a second Rust loader.
use crate::{Entry, Island, RenderMode, UnitWitness};
use fusor::{
    OwnerHandle, Registration,
    dom::{Component, Scope, delivery},
};
use std::{cell::RefCell, collections::BTreeMap, marker::PhantomData, rc::Rc};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::Element;

mod activation;
use activation::{Activation, Prepared, Preview};

type Make = dyn Fn(&Element, &str) -> Result<Prepared, JsValue>;
struct Factory {
    metadata: Entry,
    make: Box<Make>,
}

/// A unit is initialized once, with a separate retained scope for each instance.
/// Registration installs metadata/factories without constructing application state.
#[derive(Default)]
pub struct Unit {
    entries: BTreeMap<String, Rc<Factory>>,
    scopes: RefCell<BTreeMap<String, Rc<Activation>>>,
}
impl Unit {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn entry<D: Island, C: Component>(
        mut self,
        make: impl Fn(OwnerHandle, D::Props) -> C + 'static,
    ) -> Self {
        assert!(
            !self.entries.contains_key(D::NAME),
            "duplicate island entry"
        );
        self.entries.insert(D::NAME.into(), Rc::new(Factory {
            metadata: Entry { unit: D::UNIT.into(), descriptor: D::NAME.into(), props_schema: D::SCHEMA.into(), template_hash: C::TEMPLATE_HASH.into(), mode: D::MODE },
            make: Box::new(move |host, text| {
                let props = crate::decode::<D::Props>(text).map_err(|error| JsValue::from_str(&format!("island props: {error}")))?;
                if host.get_attribute("data-fusor-schema").as_deref() != Some(D::SCHEMA) || host.get_attribute("data-fusor-hash").as_deref() != Some(C::TEMPLATE_HASH) {
                    return Err(JsValue::from_str("island schema/template mismatch"));
                }
                let initial: Vec<Element> = (0..host.child_element_count()).filter_map(|index| host.children().item(index)).filter(|node| !node.has_attribute("data-fusor-props")).collect();
                if initial.len() != 1 { return Err(JsValue::from_str("an island requires exactly one initial component root")); }
                match D::MODE {
                    RenderMode::Attach => delivery::with_root(&initial[0], || C::prepare_component(None, Box::new(|owner| Ok(make(owner, props)))))
                        .map(|scope| Prepared { scope, preview: None }),
                    RenderMode::Preview => {
                        if initial[0].matches("input,textarea,select,[contenteditable]:not([contenteditable=false])")? || initial[0].query_selector("input,textarea,select,[contenteditable]:not([contenteditable=false])")?.is_some() {
                            return Err(JsValue::from_str("editable island previews cannot be replaced"));
                        }
                        let (mut scope, readiness) = delivery::prepare_preview(|| C::prepare_component(None, Box::new(|owner| Ok(make(owner, props)))))?;
                        // Keep the candidate detached, with ordinary owners still
                        // prepared, until its initial coherent regions are ready.
                        let staging = fusor::dom::document()?.create_element("div")?;
                        scope.attach(&staging)?;
                        Ok(Prepared {scope, preview: Some(Preview::new(host, initial[0].clone(), readiness, text)?)})
                    }
                }
            }),
        }));
        self
    }
    pub fn manifest(&self) -> String {
        crate::encode(&UnitWitness {
            version: crate::PROTOCOL_VERSION,
            entries: self
                .entries
                .values()
                .map(|entry| entry.metadata.clone())
                .collect(),
        })
        .expect("metadata is JSON")
    }
    pub fn activate(
        &self,
        descriptor: &str,
        host: &Element,
        props: &str,
        token: &str,
    ) -> Result<js_sys::Promise, JsValue> {
        if let Some(activation) = self.scopes.borrow().get(token) {
            return Ok(activation.promise());
        }
        let entry = self
            .entries
            .get(descriptor)
            .ok_or_else(|| JsValue::from_str("unknown unit entry"))?
            .clone();
        delivery::enable();
        let activation = Activation::new((entry.make)(host, props)?);
        self.scopes
            .borrow_mut()
            .insert(token.to_owned(), activation.clone());
        activation.start();
        Ok(activation.promise())
    }
    pub fn dispose(&self, token: &str) {
        let activation = self.scopes.borrow_mut().remove(token);
        if let Some(activation) = activation {
            activation.dispose();
        }
    }
}

/// Export one unit with no application start function. The expression registers
/// typed factories; component construction happens only in `__fusor_activate`.
#[macro_export]
macro_rules! export {
    ($unit:expr) => {
        // wasm-bindgen rejects multiple start functions, including those emitted
        // by macros/dependencies. Reserve that native compiler slot with a pure
        // engine guard, so application startup cannot run eagerly in a unit.
        #[::wasm_bindgen::prelude::wasm_bindgen(start)]
        pub fn __fusor_delivery_start_guard() {}
        ::std::thread_local! { static __FUSOR_UNIT: ::std::cell::OnceCell<$crate::browser::Unit> = const { ::std::cell::OnceCell::new() }; }
        fn __fusor_unit<R>(read: impl FnOnce(&$crate::browser::Unit) -> R) -> R { __FUSOR_UNIT.with(|unit| read(unit.get_or_init(|| $unit))) }
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn __fusor_manifest() -> ::std::string::String { __fusor_unit(|unit| unit.manifest()) }
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn __fusor_activate(descriptor: &str, host: &$crate::browser::IslandElement, props: &str, token: &str) -> ::std::result::Result<$crate::browser::IslandActivation, ::wasm_bindgen::JsValue> {
            __fusor_unit(|unit| unit.activate(descriptor, host, props, token))
        }
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn __fusor_dispose(token: &str) { __fusor_unit(|unit| unit.dispose(token)); }
    };
}
#[doc(hidden)]
pub type IslandElement = Element;
#[doc(hidden)]
pub type IslandActivation = js_sys::Promise;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IslandStatus {
    Dormant,
    Requested,
    Binding,
    Active,
    Failed,
    Disposed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IslandError {
    Unavailable,
    UnknownInstance,
    DescriptorMismatch,
    InactiveOwner,
    DisposedOwner,
    StaleInstance,
    Cancelled,
    LoadFailed(String),
    BindingFailed(String),
}
impl std::fmt::Display for IslandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for IslandError {}

fn call(name: &str, args: &[JsValue]) -> Result<JsValue, IslandError> {
    let registry = js_sys::Reflect::get(&js_sys::global(), &"__fusor_islands".into())
        .map_err(|_| IslandError::Unavailable)?;
    let function = js_sys::Reflect::get(&registry, &name.into())
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
        .ok_or(IslandError::Unavailable)?;
    let list = js_sys::Array::new();
    for arg in args {
        list.push(arg);
    }
    function.apply(&registry, &list).map_err(decode_error)
}
fn decode_error(value: JsValue) -> IslandError {
    let code = js_sys::Reflect::get(&value, &"code".into())
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default();
    let message = js_sys::Reflect::get(&value, &"message".into())
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| format!("{value:?}"));
    match code.as_str() {
        "unknown-instance" => IslandError::UnknownInstance,
        "descriptor-mismatch" => IslandError::DescriptorMismatch,
        "stale-instance" => IslandError::StaleInstance,
        "cancelled" => IslandError::Cancelled,
        "binding-failed" => IslandError::BindingFailed(message),
        _ => IslandError::LoadFailed(message),
    }
}

/// Lookup performs no import, initialization or activation. The instance token
/// remains tied to this registration even if the DOM ID is later reused.
pub fn get<D: Island>(owner: &OwnerHandle, id: &str) -> Result<IslandRef<D>, IslandError> {
    if owner.is_disposed() {
        return Err(IslandError::DisposedOwner);
    }
    let token = call("lookup", &[id.into(), D::NAME.into(), D::SCHEMA.into()])?
        .as_string()
        .ok_or(IslandError::UnknownInstance)?;
    Ok(IslandRef {
        owner: owner.clone(),
        token,
        marker: PhantomData,
    })
}
pub struct IslandRef<D> {
    owner: OwnerHandle,
    token: String,
    marker: PhantomData<D>,
}
impl<D> Clone for IslandRef<D> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            token: self.token.clone(),
            marker: PhantomData,
        }
    }
}
impl<D: Island> IslandRef<D> {
    pub fn status(&self) -> Result<IslandStatus, IslandError> {
        if self.owner.is_disposed() {
            return Err(IslandError::DisposedOwner);
        }
        match call("status", &[self.token.clone().into()])?
            .as_string()
            .as_deref()
        {
            Some("dormant") => Ok(IslandStatus::Dormant),
            Some("requested") => Ok(IslandStatus::Requested),
            Some("binding") => Ok(IslandStatus::Binding),
            Some("active") => Ok(IslandStatus::Active),
            Some("failed") => Ok(IslandStatus::Failed),
            Some("disposed") => Ok(IslandStatus::Disposed),
            _ => Err(IslandError::StaleInstance),
        }
    }
    pub async fn prefetch(&self) -> Result<(), IslandError> {
        self.request("prefetch").await
    }
    pub async fn activate(&self) -> Result<(), IslandError> {
        self.request("activate").await
    }
    pub async fn retry(&self) -> Result<(), IslandError> {
        self.request("retry").await
    }
    async fn request(&self, action: &str) -> Result<(), IslandError> {
        if self.owner.is_disposed() {
            return Err(IslandError::DisposedOwner);
        }
        if !self.owner.is_active() {
            return Err(IslandError::InactiveOwner);
        }
        let operation = call("request", &[self.token.clone().into(), action.into()])?;
        let cancel = js_sys::Reflect::get(&operation, &"cancel".into())
            .ok()
            .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
            .ok_or(IslandError::Unavailable)?;
        let callback = cancel.clone();
        let registration = self.owner.on_cleanup(move || {
            let _ = callback.call0(&JsValue::UNDEFINED);
        });
        let mut guard = Waiter {
            cancel: Some(cancel),
            _registration: registration,
        };
        let promise = js_sys::Reflect::get(&operation, &"promise".into())
            .ok()
            .and_then(|value| value.dyn_into::<js_sys::Promise>().ok())
            .ok_or(IslandError::Unavailable)?;
        let result = JsFuture::from(promise).await.map_err(decode_error);
        if self.owner.is_disposed() {
            return Err(IslandError::DisposedOwner);
        }
        result?;
        guard.cancel.take();
        Ok(())
    }
}
struct Waiter {
    cancel: Option<js_sys::Function>,
    _registration: Registration,
}
impl Drop for Waiter {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.call0(&JsValue::UNDEFINED);
        }
    }
}

/// Namespaced, ephemeral DOM events. The receiving scope guards its lifetime.
pub fn emit<D: Island, T: serde::Serialize>(
    host: &Element,
    event: &str,
    value: &T,
) -> Result<(), JsValue> {
    let options = web_sys::CustomEventInit::new();
    options.set_bubbles(true);
    options.set_detail(
        &crate::encode(value)
            .map_err(|error| JsValue::from_str(&error.to_string()))?
            .into(),
    );
    let event = web_sys::CustomEvent::new_with_event_init_dict(
        &format!("fusor:{}:{event}", D::NAME),
        &options,
    )?;
    host.dispatch_event(&event)?;
    Ok(())
}

/// Decode an explicit cross-unit message while the receiving scope is active.
/// Payloads cross the boundary as JSON text, preserving Rust integer precision.
pub fn listen<D: Island, T: serde::de::DeserializeOwned + 'static>(
    scope: &mut Scope,
    target: &Element,
    event: &str,
    mut receive: impl FnMut(Result<T, String>) + 'static,
) -> Result<(), JsValue> {
    scope.on(
        target,
        &format!("fusor:{}:{event}", D::NAME),
        move |event| {
            let value = event
                .dyn_into::<web_sys::CustomEvent>()
                .ok()
                .and_then(|event| event.detail().as_string())
                .ok_or_else(|| "island event requires opaque JSON text".to_owned())
                .and_then(|text| crate::decode(&text).map_err(|error| error.to_string()));
            receive(value);
        },
    )
}

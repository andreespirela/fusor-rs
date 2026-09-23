use fusor::{Signal, signal};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local! { static PROPS: RefCell<Option<(fusor::dom::Scope, Signal<JsValue>, Signal<u32>)>> = const { RefCell::new(None) }; }
#[wasm_bindgen]
pub fn property_mount(tag: &str, prepared: bool) -> Result<(), JsValue> {
    property_drop();
    let root = fusor::dom::document()?.create_element(tag)?;
    root.set_id("property-fixture");
    fusor::dom::document()?
        .body()
        .unwrap()
        .append_child(&root)?;
    let mut scope = fusor::dom::Scope::new(root.clone());
    if prepared {
        scope.prepare_owner(None);
    }
    let value = signal(JsValue::from_f64(1.0));
    let other = signal(0u32);
    let read = value.clone();
    scope.property(&root, "someValue", move || read.get())?;
    let update = value.clone();
    scope.on(&root, "ValueChanged", move |event| {
        use wasm_bindgen::JsCast;
        let event: web_sys::CustomEvent = event.unchecked_into();
        update.set(event.detail());
    })?;
    let read = other.clone();
    scope.bind_dom(move || {
        let _ = read.get();
        Ok(())
    })?;
    PROPS.with(|v| *v.borrow_mut() = Some((scope, value, other)));
    Ok(())
}
#[wasm_bindgen]
pub fn property_commit() {
    PROPS.with(|v| v.borrow().as_ref().unwrap().0.commit());
}
#[wasm_bindgen]
pub fn property_set(value: JsValue) {
    let signal = PROPS.with(|v| v.borrow().as_ref().unwrap().1.clone());
    signal.set(value);
}
#[wasm_bindgen]
pub fn property_unrelated() {
    let signal = PROPS.with(|v| v.borrow().as_ref().unwrap().2.clone());
    signal.update(|v| *v += 1);
}
#[wasm_bindgen]
pub fn property_drop() {
    let state = PROPS.with(|v| v.borrow_mut().take());
    drop(state);
}

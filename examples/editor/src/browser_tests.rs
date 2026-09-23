//! Browser acceptance hooks, excluded from normal builds.
use crate::app::Store;
use fusor::dom::application;
use wasm_bindgen::prelude::*;

fn store() -> std::rc::Rc<Store> {
    Store::context(&application::owner().expect("app")).expect("store")
}
#[wasm_bindgen]
pub fn submit_again() {
    let store = store();
    if let Some(session) = store.session.get() {
        session.submit(store.projects.clone());
    }
}
#[wasm_bindgen]
pub fn reset_title(value: String) {
    if let Some(session) = store().session.get() {
        session.title.reset(value);
    }
}
#[wasm_bindgen]
pub fn unmount() -> Result<(), JsValue> {
    application::unmount()
}

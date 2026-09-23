use fusor::{dom::{Component, Scope}, prelude::*};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

pub struct Counter {
    count: Signal<i32>,
}

// Retain the root scope for as long as this embedded UI should exist.
thread_local! {
    static ROOT: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let mut parent = Scope::at("#counter-host")?;
    let child = Counter::prepare(&parent.owner(), |_owner| {
        Ok(Counter { count: signal(0) })
    })?;
    parent.mount_child(":scope", child)?;
    ROOT.with(|root| *root.borrow_mut() = Some(parent));
    Ok(())
}

// Call this from an embedding application when it removes this UI.
#[wasm_bindgen]
pub fn unmount_counter() {
    ROOT.with(|root| { root.borrow_mut().take(); });
}

fusor::template!("web/index.html");

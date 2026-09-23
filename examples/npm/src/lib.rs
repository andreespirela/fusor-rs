mod app;
mod chart;
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn stop() -> Result<(), wasm_bindgen::JsValue> {
    fusor::dom::application::unmount()
}

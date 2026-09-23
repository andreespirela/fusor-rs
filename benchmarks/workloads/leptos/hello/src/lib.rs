use leptos::prelude::*;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(start)]
pub fn main() {
    leptos::mount::mount_to_body(|| view! { <h1>"Hello, world!"</h1> });
}

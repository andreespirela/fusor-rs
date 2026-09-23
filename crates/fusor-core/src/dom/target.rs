//! Native handles are the binding interface. Selectors are a convenience adapter.

use super::{JsValue, Scope};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlInputElement};

/// An element handle, or a selector resolved relative to a scope.
pub trait ElementTarget {
    fn resolve(self, scope: &Scope) -> Result<Element, JsValue>;
}

/// A native input handle, or a selector checked to refer to an input.
/// Passing an unrelated element handle is a Rust type error.
pub trait InputTarget {
    fn resolve_input(self, scope: &Scope) -> Result<HtmlInputElement, JsValue>;
}

impl ElementTarget for &str {
    fn resolve(self, scope: &Scope) -> Result<Element, JsValue> {
        scope.select(self)
    }
}

impl InputTarget for &str {
    fn resolve_input(self, scope: &Scope) -> Result<HtmlInputElement, JsValue> {
        scope
            .select(self)?
            .dyn_into()
            .map_err(|_| JsValue::from_str("fusor: input binding requires an HTML input element"))
    }
}

impl ElementTarget for &String {
    fn resolve(self, scope: &Scope) -> Result<Element, JsValue> {
        self.as_str().resolve(scope)
    }
}

impl InputTarget for &String {
    fn resolve_input(self, scope: &Scope) -> Result<HtmlInputElement, JsValue> {
        self.as_str().resolve_input(scope)
    }
}

impl ElementTarget for &Element {
    fn resolve(self, _: &Scope) -> Result<Element, JsValue> {
        Ok(self.clone())
    }
}

impl ElementTarget for &HtmlInputElement {
    fn resolve(self, _: &Scope) -> Result<Element, JsValue> {
        Ok(self.clone().into())
    }
}

impl InputTarget for &HtmlInputElement {
    fn resolve_input(self, _: &Scope) -> Result<HtmlInputElement, JsValue> {
        Ok(self.clone())
    }
}

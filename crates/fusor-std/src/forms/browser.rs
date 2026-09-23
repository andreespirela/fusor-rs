//! Scope-owned text controls. Composition owns the visible draft until it ends:
//! programmatic writes do not interrupt the IME, and its final text wins. A reset
//! during composition still changes the baseline; the composed draft may be dirty.
use super::TextField;
use fusor::dom::{ElementTarget, Scope};
use std::{cell::Cell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlInputElement, HtmlTextAreaElement};

#[derive(Clone)]
enum Control {
    Input(HtmlInputElement),
    TextArea(HtmlTextAreaElement),
}
impl Control {
    fn resolve(element: &Element) -> Result<Self, JsValue> {
        if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
            let kind = input
                .get_attribute("type")
                .unwrap_or_else(|| "text".into())
                .to_ascii_lowercase();
            if matches!(
                kind.as_str(),
                "text" | "search" | "email" | "url" | "tel" | "password"
            ) {
                return Ok(Self::Input(input.clone()));
            }
        } else if let Some(textarea) = element.dyn_ref::<HtmlTextAreaElement>() {
            return Ok(Self::TextArea(textarea.clone()));
        }
        Err(JsValue::from_str(
            "fusor: bind:field requires a text input (text/search/email/url/tel/password) or textarea",
        ))
    }
    fn value(&self) -> String {
        match self {
            Self::Input(input) => input.value(),
            Self::TextArea(textarea) => textarea.value(),
        }
    }
    fn set_value(&self, value: &str) {
        match self {
            Self::Input(input) => input.set_value(value),
            Self::TextArea(textarea) => textarea.set_value(value),
        }
    }
}

/// Bind the same typed field used by `Form::new`. Rustc checks the field handle;
/// the native target is validated before any listeners or effects are installed.
/// Normal input echoes never assign `.value`, preserving focus and selection.
#[doc(hidden)]
pub fn adopt<T: 'static>(
    scope: &Scope,
    target: impl ElementTarget,
    field: &TextField<T>,
) -> Result<(), JsValue> {
    let control = Control::resolve(&target.resolve(scope)?)?;
    let value = control.value();
    if value != field.raw() {
        field.edit(value);
    }
    Ok(())
}

pub fn bind<T: 'static>(
    scope: &mut Scope,
    target: impl ElementTarget,
    field: TextField<T>,
) -> Result<(), JsValue> {
    let element = target.resolve(scope)?;
    let control = Control::resolve(&element)?;
    let composing = Rc::new(Cell::new(false));
    let active = composing.clone();
    let draft = field.clone();
    let input = control.clone();
    scope.on(&element, "compositionstart", move |_| {
        active.set(true);
        // Reserve an edit even before the first composition input. An older
        // acknowledgment must not normalize this newly composing draft.
        draft.edit(input.value());
    })?;
    let active = composing.clone();
    let draft = field.clone();
    let input = control.clone();
    scope.on(&element, "compositionend", move |_| {
        active.set(false);
        draft.edit(input.value());
    })?;
    let draft = field.clone();
    let input = control.clone();
    scope.on(&element, "input", move |_| draft.edit(input.value()))?;
    let draft = field.clone();
    scope.on(&element, "blur", move |_| draft.touch())?;
    let owner = scope.owner();
    scope.bind_dom(move || {
        if owner.is_disposed() {
            return Ok(());
        }
        let next = field.raw();
        if !composing.get() && control.value() != next {
            control.set_value(&next);
        }
        Ok(())
    })?;
    Ok(())
}

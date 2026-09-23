//! Reuse JS string arguments for native DOM calls, without interning dynamic
//! application values or adding a cache lookup to every Wasm string conversion.
use crate::template::ComponentId;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{Document, Element, EventTarget, NodeList};

// Compare against the live DOM in JavaScript. Returning its old string to Rust
// only to compare it allocates and transcodes a value that no caller needs.
#[wasm_bindgen(
    inline_js = "export function setTextIfChanged(node, value) { if (node.data !== value) node.data = value; } export function setIntegerTextIfChanged(node, number) { const value = '' + number; if (node.data !== value) node.data = value; }"
)]
extern "C" {
    #[wasm_bindgen(js_name = setTextIfChanged)]
    pub(super) fn set_text_if_changed(node: &web_sys::Text, value: &str);
    #[wasm_bindgen(js_name = setIntegerTextIfChanged)]
    pub(super) fn set_integer_text_if_changed(node: &web_sys::Text, value: f64);
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Element, js_name = Element)]
    type StringElement;
    #[wasm_bindgen(method, structural, js_name = getAttribute)]
    fn attribute(this: &StringElement, name: &JsValue) -> Option<String>;
    #[wasm_bindgen(method, structural, js_name = getAttribute)]
    fn attribute_value(this: &StringElement, name: &JsValue) -> JsValue;
    #[wasm_bindgen(method, structural, catch, js_name = setAttribute)]
    fn set_attribute_value(
        this: &StringElement,
        name: &JsValue,
        value: &JsValue,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(extends = Document, js_name = Document)]
    type StringDocument;
    #[wasm_bindgen(method, structural, catch, js_name = querySelectorAll)]
    fn query(this: &StringDocument, selector: &JsValue) -> Result<NodeList, JsValue>;

    #[wasm_bindgen(extends = EventTarget, js_name = EventTarget)]
    type StringTarget;
    #[wasm_bindgen(method, structural, catch, js_name = addEventListener)]
    fn add(this: &StringTarget, name: &JsValue, callback: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(method, structural, catch, js_name = removeEventListener)]
    fn remove(this: &StringTarget, name: &JsValue, callback: &JsValue) -> Result<(), JsValue>;
}

pub(super) enum Attribute {
    Element = 2,
    Key = 3,
}

thread_local! {
    // These names are framework syntax. The cache cannot grow with input data.
    static NAMES: [JsValue; 8] = [
        crate::template::COMPONENT_ATTRIBUTE,
        crate::template::VERSION_ATTRIBUTE,
        crate::template::ELEMENT_ATTRIBUTE,
        "data-rf-key", "click", "input", "change", crate::template::INSTANCE_ATTRIBUTE,
    ].map(JsValue::from_str);
}

pub(super) fn attribute(element: &Element, name: Attribute) -> Option<String> {
    NAMES.with(|names| {
        element
            .unchecked_ref::<StringElement>()
            .attribute(&names[name as usize])
    })
}

pub(super) enum EventName {
    Cached(usize),
    Owned(JsValue),
}

impl From<&str> for EventName {
    fn from(name: &str) -> Self {
        event(name)
    }
}

impl EventName {
    fn with<R>(&self, call: impl FnOnce(&JsValue) -> R) -> R {
        match self {
            Self::Cached(index) => NAMES.with(|names| call(&names[*index])),
            Self::Owned(value) => call(value),
        }
    }
}

pub(super) fn event(name: &str) -> EventName {
    match name {
        "click" => EventName::Cached(4),
        "input" => EventName::Cached(5),
        "change" => EventName::Cached(6),
        _ => EventName::Owned(JsValue::from_str(name)),
    }
}

pub(super) fn add(
    target: &EventTarget,
    name: &EventName,
    callback: &JsValue,
) -> Result<(), JsValue> {
    name.with(|name| target.unchecked_ref::<StringTarget>().add(name, callback))
}

pub(super) fn remove(
    target: &EventTarget,
    name: &EventName,
    callback: &JsValue,
) -> Result<(), JsValue> {
    name.with(|name| {
        target
            .unchecked_ref::<StringTarget>()
            .remove(name, callback)
    })
}

/// Bounded immutable metadata only; no DOM roots, scopes or application values.
pub(super) struct DescriptorStrings {
    component: ComponentId,
    version: u32,
    selector: JsValue,
    schema: JsValue,
    identity: JsValue,
}

thread_local! {
    static DESCRIPTORS: RefCell<VecDeque<Rc<DescriptorStrings>>> = const { RefCell::new(VecDeque::new()) };
}

pub(super) fn descriptor(component: ComponentId, version: u32) -> Rc<DescriptorStrings> {
    let cached = DESCRIPTORS.with(|descriptors| {
        descriptors
            .borrow()
            .iter()
            .rev()
            .find(|entry| entry.component == component && entry.version == version)
            .cloned()
    });
    if let Some(cached) = cached {
        return cached;
    }
    // No registry borrow crosses JS calls, including reentrant native methods.
    let entry = Rc::new(DescriptorStrings {
        component,
        version,
        selector: JsValue::from_str(&format!(
            "[{}=\"{}\"]",
            crate::template::COMPONENT_ATTRIBUTE,
            component
        )),
        schema: JsValue::from_str(&version.to_string()),
        identity: JsValue::from_str(&component.to_string()),
    });
    DESCRIPTORS.with(|descriptors| {
        let mut descriptors = descriptors.borrow_mut();
        if descriptors.len() >= 32 {
            descriptors.pop_front();
        }
        descriptors.push_back(entry.clone());
    });
    entry
}

impl DescriptorStrings {
    pub(super) fn roots(&self, document: &Document) -> Result<NodeList, JsValue> {
        document
            .unchecked_ref::<StringDocument>()
            .query(&self.selector)
    }
    pub(super) fn version_matches(&self, element: &Element) -> bool {
        NAMES.with(|names| {
            element
                .unchecked_ref::<StringElement>()
                .attribute_value(&names[1])
                == self.schema
        })
    }
    #[cfg(feature = "islands")]
    pub(super) fn component_matches(&self, element: &Element) -> bool {
        NAMES.with(|names| {
            element
                .unchecked_ref::<StringElement>()
                .attribute_value(&names[0])
                == self.identity
        })
    }
    pub(super) fn mark_instance(&self, element: &Element) -> Result<(), JsValue> {
        NAMES.with(|names| {
            element
                .unchecked_ref::<StringElement>()
                .set_attribute_value(&names[7], &self.identity)
        })
    }
}

// Generated attribute names are static program metadata, never application
// values. Bound the shared registry; effects retain their immutable name if an
// entry is evicted. No registry borrow crosses the JS string conversion.
thread_local! {
    static STATIC_ATTRIBUTES: RefCell<VecDeque<(&'static str, Rc<JsValue>)>> = const { RefCell::new(VecDeque::new()) };
}

pub(super) fn static_attribute(name: &'static str) -> Rc<JsValue> {
    let found = STATIC_ATTRIBUTES.with(|names| {
        names
            .borrow()
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| Rc::clone(value))
    });
    if let Some(found) = found {
        return found;
    }
    let value = Rc::new(JsValue::from_str(name));
    STATIC_ATTRIBUTES.with(|names| {
        let mut names = names.borrow_mut();
        if names.len() >= 64 {
            names.pop_front();
        }
        names.push_back((name, Rc::clone(&value)));
    });
    value
}

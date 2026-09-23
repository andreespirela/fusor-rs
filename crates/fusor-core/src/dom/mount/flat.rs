//! Keep the protocol scan next to the DOM for shapes with no managed subtrees.
//! The bounded registry owns descriptor metadata and inert certificates, never
//! application nodes. Typed resolution and native bundle binding share its plans.
use super::Scope;
#[cfg(feature = "islands")]
use super::{ElementHandle, Handles, Mounts, Resolution, Slot, TextPosition};
use crate::template::{ElementDescriptor, TemplateDescriptor, TextElementDescriptor, TextId};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[cfg(feature = "islands")]
use wasm_bindgen::JsCast;
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};
use web_sys::{Element, Event};

#[wasm_bindgen(module = "/src/dom/mount/flat.js")]
extern "C" {
    #[wasm_bindgen(js_name = flatPlan)]
    fn create_plan(
        elements: &js_sys::Array,
        tags: &js_sys::Array,
        texts: &js_sys::Array,
        text_elements: &js_sys::Array,
    ) -> JsValue;
    #[cfg(feature = "islands")]
    #[wasm_bindgen(catch, js_name = resolveFlat)]
    fn resolve_flat(plan: &JsValue, root: &Element) -> Result<js_sys::Array, JsValue>;
    #[wasm_bindgen(catch, js_name = resolveBindings)]
    fn resolve_bindings(plan: &JsValue, root: &Element, cached: bool) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_name = bindingText)]
    fn binding_text(nodes: &JsValue, index: u32, value: &str);
    #[wasm_bindgen(js_name = bindingIntegerText)]
    fn binding_integer_text(nodes: &JsValue, index: u32, value: f64);
    #[wasm_bindgen(catch, js_name = bindingSetAttribute)]
    fn binding_set_attribute(
        nodes: &JsValue,
        index: u32,
        name: &JsValue,
        value: &str,
    ) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = bindingRemoveAttribute)]
    fn binding_remove_attribute(nodes: &JsValue, index: u32, name: &JsValue)
    -> Result<(), JsValue>;
    // All target types were validated before the user factory. Returning the
    // pinned handle must not re-check a prototype changed by that factory.
    #[wasm_bindgen(js_name = bindingElement)]
    fn binding_element(nodes: &JsValue, index: u32) -> Element;
}
struct Plan {
    elements: &'static [ElementDescriptor],
    texts: &'static [TextId],
    text_elements: &'static [TextElementDescriptor],
    native: JsValue,
}
thread_local! {
    static PLANS: RefCell<VecDeque<Rc<Plan>>> = const { RefCell::new(VecDeque::new()) };
}

fn plan(descriptor: &TemplateDescriptor) -> Rc<Plan> {
    let found = PLANS.with(|plans| {
        plans
            .borrow()
            .iter()
            .find(|plan| {
                std::ptr::eq(plan.elements, descriptor.elements)
                    && std::ptr::eq(plan.texts, descriptor.texts)
                    && std::ptr::eq(plan.text_elements, descriptor.text_elements)
            })
            .cloned()
    });
    found.unwrap_or_else(|| {
        let elements = descriptor
            .elements
            .iter()
            .map(|element| JsValue::from_str(&element.id.to_string()))
            .collect();
        let tags = descriptor
            .elements
            .iter()
            .map(|element| JsValue::from_str(element.tag))
            .collect();
        let texts = descriptor
            .texts
            .iter()
            .map(|id| JsValue::from_str(&id.to_string()))
            .collect();
        let text_elements = descriptor
            .text_elements
            .iter()
            .flat_map(|text| {
                [
                    JsValue::from_str(&text.id.to_string()),
                    text.host
                        .map_or(JsValue::NULL, |host| JsValue::from_str(&host.to_string())),
                    JsValue::from_str(text.tag),
                ]
            })
            .collect();
        let plan = Rc::new(Plan {
            elements: descriptor.elements,
            texts: descriptor.texts,
            text_elements: descriptor.text_elements,
            native: create_plan(&elements, &tags, &texts, &text_elements),
        });
        PLANS.with(|plans| {
            let mut plans = plans.borrow_mut();
            if plans.len() >= 32 {
                plans.pop_front();
            }
            plans.push_back(plan.clone());
        });
        plan
    })
}

pub(super) fn resolve_bundle(
    descriptor: &TemplateDescriptor,
    root: &Element,
    cached: bool,
) -> Result<JsValue, JsValue> {
    let plan = plan(descriptor);
    resolve_bindings(&plan.native, root, cached)
}

#[cfg(feature = "islands")]
pub(super) fn resolve(
    descriptor: &TemplateDescriptor,
    root: &Element,
) -> Result<Resolution, JsValue> {
    let plan = plan(descriptor);
    let nodes = resolve_flat(&plan.native, root)?;
    let mut handles = Handles::new();
    for (index, element) in descriptor.elements.iter().enumerate() {
        let node = nodes.get(index as u32);
        let handle = if element.tag == "input" {
            ElementHandle::Input(node.dyn_into()?)
        } else {
            ElementHandle::Element(node.dyn_into()?)
        };
        handles.insert(element.id, handle);
    }
    let mut slots = Vec::with_capacity(descriptor.texts.len() + descriptor.text_elements.len());
    for (index, id) in descriptor.texts.iter().enumerate() {
        let offset = (descriptor.elements.len() + index * 3) as u32;
        let text = nodes.get(offset + 2);
        slots.push(Slot {
            id: *id,
            position: TextPosition::Anchored {
                start: nodes.get(offset).dyn_into()?,
                end: nodes.get(offset + 1).dyn_into()?,
            },
            existing: if text.is_null() {
                None
            } else {
                Some(text.dyn_into()?)
            },
        });
    }
    for (index, expected) in descriptor.text_elements.iter().enumerate() {
        let offset = (descriptor.elements.len() + descriptor.texts.len() * 3 + index * 2) as u32;
        let text = nodes.get(offset + 1);
        slots.push(Slot {
            id: expected.id,
            position: TextPosition::Element(nodes.get(offset).dyn_into()?),
            existing: if text.is_null() {
                None
            } else {
                Some(text.dyn_into()?)
            },
        });
    }
    handles.finish();
    Ok((handles, slots, Mounts::new()))
}

impl Scope {
    #[doc(hidden)]
    pub fn bundle_text_value(
        &mut self,
        nodes: &Rc<JsValue>,
        index: u32,
        read: impl Fn() -> crate::dom::text_value::Output + 'static,
    ) -> Result<(), JsValue> {
        use crate::dom::text_value::Output;
        let nodes = Rc::clone(nodes);
        self.bind_dom_infallible(move || match read() {
            Output::String(value) => binding_text(&nodes, index, &value),
            Output::Integer(value) => binding_integer_text(&nodes, index, value),
        })
    }

    #[doc(hidden)]
    pub fn bundle_text_string(
        &mut self,
        nodes: &Rc<JsValue>,
        index: u32,
        read: impl Fn() -> String + 'static,
    ) -> Result<(), JsValue> {
        let nodes = Rc::clone(nodes);
        self.bind_dom_infallible(move || binding_text(&nodes, index, &read()))
    }

    #[doc(hidden)]
    pub fn bundle_attr(
        &mut self,
        nodes: &Rc<JsValue>,
        index: u32,
        name: &'static str,
        read: impl Fn() -> Option<String> + 'static,
    ) -> Result<(), JsValue> {
        let nodes = Rc::clone(nodes);
        let name = crate::dom::strings::static_attribute(name);
        self.bind_dom(move || match read() {
            Some(value) => binding_set_attribute(&nodes, index, &name, &value),
            None => binding_remove_attribute(&nodes, index, &name),
        })
    }

    #[doc(hidden)]
    pub fn bundle_on(
        &mut self,
        nodes: &Rc<JsValue>,
        index: u32,
        event: &str,
        handler: impl FnMut(Event) + 'static,
    ) -> Result<(), JsValue> {
        let target = binding_element(nodes, index);
        self.on(&target, event, handler)
    }
}

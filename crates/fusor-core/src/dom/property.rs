//! Explicit custom-element properties, activated with their owning scope.
use super::*;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(inline_js = r#"
const unset = Symbol('fusor unset property');
const definitions = new WeakMap();
function report(s, error) {
  if (!s.active) return;
  console.error(error);
  s.element.dispatchEvent(new CustomEvent('fusor:error', {detail: error, bubbles: true}));
}
function flush(s) {
  if (!s.active || !s.ready || s.value === unset || Object.is(s.written, s.value)) return;
  const value = s.value;
  s.written = value;
  try {
    if (!Reflect.set(s.element, s.name, value)) throw new TypeError(`Cannot set custom element property ${s.name}`);
  } catch (error) {
    if (s.active && Object.is(s.written, value)) s.written = unset;
    throw error;
  }
}
export function property_create(element, name) {
  return {element, name, value: unset, written: unset, active: false, ready: false, pending: null};
}
export function property_value(s, value) { s.value = value; flush(s); }
export function property_activate(s) {
  s.active = true;
  const registry = s.element.ownerDocument.defaultView.customElements;
  const name = s.element.localName;
  if (registry.get(name)) {
    registry.upgrade(s.element);
    s.ready = true;
    flush(s);
  } else {
    let tags = definitions.get(registry);
    if (!tags) { tags = new Map(); definitions.set(registry, tags); }
    let pending = tags.get(name);
    if (!pending) {
      pending = new Set();
      tags.set(name, pending);
      // Keep an empty entry until definition: repeated mount/drop must not add
      // unbounded callbacks to the registry's never-settled promise.
      registry.whenDefined(name).then(() => {
        tags.delete(name);
        for (const binding of pending) {
          pending.delete(binding);
          binding.pending = null;
          if (!binding.active) continue;
          try {
            registry.upgrade(binding.element);
            binding.ready = true;
            flush(binding);
          } catch (error) { report(binding, error); }
        }
      }).catch(error => {
        tags.delete(name);
        for (const binding of pending) { binding.pending = null; report(binding, error); }
        pending.clear();
      });
    }
    pending.add(s);
    s.pending = pending;
  }
}
export function property_dispose(s) {
  s.active = false;
  if (s.pending) { s.pending.delete(s); s.pending = null; }
  s.element = null;
  s.value = s.written = unset;
}
"#)]
extern "C" {
    fn property_create(element: &Element, name: &str) -> JsValue;
    #[wasm_bindgen(catch)]
    fn property_value(binding: &JsValue, value: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    fn property_activate(binding: &JsValue) -> Result<(), JsValue>;
    fn property_dispose(binding: &JsValue);
}

struct Property(JsValue);
impl Drop for Property {
    fn drop(&mut self) {
        property_dispose(&self.0);
    }
}

impl Scope {
    /// Assign a JavaScript value to a custom element property after activation
    /// and definition. Objects retain identity; equal values skip the setter.
    /// Pending definitions retain neither the element nor Rust state after drop.
    /// A setter can have arbitrary external effects and is not transactional.
    pub fn property<V: Into<JsValue>>(
        &mut self,
        target: impl ElementTarget,
        name: &str,
        read: impl Fn() -> V + 'static,
    ) -> Result<(), JsValue> {
        if self.is_coherent() {
            return Err(JsValue::from_str(
                "fusor: custom properties cannot participate in coherent rendering",
            ));
        }
        let element = target.resolve(self)?;
        if !element.local_name().contains('-') || !is_html(&element) || name.is_empty() {
            return Err(JsValue::from_str(
                "fusor: prop:name requires a custom element and a nonempty property name",
            ));
        }
        let property = Rc::new(Property(property_create(&element, name)));
        let update = property.clone();
        self.bind(move || property_value(&update.0, &read().into()))?;
        let activate = property.clone();
        let registration = self.owner().on_activate(move || {
            if let Err(error) = property_activate(&activate.0) {
                web_sys::console::error_1(&error);
            }
        });
        // Cleanup must clear the JS handle even if another Rust owner retains
        // registrations until the scope itself drops.
        let cleanup = property.clone();
        let registration_cleanup = self
            .owner()
            .on_cleanup(move || property_dispose(&cleanup.0));
        self.retain((property, registration, registration_cleanup));
        Ok(())
    }
}

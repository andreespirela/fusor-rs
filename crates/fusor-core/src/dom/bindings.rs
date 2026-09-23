use super::{ElementTarget, InputTarget, JsValue, Listener, Scope, strings, text_value};
use crate::{Signal, batch, effect};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::closure::Closure;
use web_sys::{Event, EventTarget, Text};

impl Scope {
    pub(super) fn bind(
        &mut self,
        update: impl FnMut() -> Result<(), JsValue> + 'static,
    ) -> Result<(), JsValue> {
        self.bind_internal(update, false)
    }

    /// Generated and standard-library DOM effects join attachment validation.
    /// Native descendants are validated before the first binding writes DOM.
    #[doc(hidden)]
    pub fn bind_dom(
        &mut self,
        update: impl FnMut() -> Result<(), JsValue> + 'static,
    ) -> Result<(), JsValue> {
        self.bind_internal(update, self.hydrating)
    }

    fn bind_internal(
        &mut self,
        mut update: impl FnMut() -> Result<(), JsValue> + 'static,
        deferred: bool,
    ) -> Result<(), JsValue> {
        let first_error = Rc::new(RefCell::new(None));
        let captured = first_error.clone();
        let mut first = true;
        let owner = self.owner();
        let callback = move || {
            if owner.is_disposed() {
                return;
            }
            if let Err(error) = update() {
                if first {
                    *captured.borrow_mut() = Some(error);
                } else {
                    web_sys::console::error_1(&error);
                }
            }
            first = false;
        };
        let binding = if deferred {
            let binding = crate::reactive::prepared_effect(callback);
            let initialize = binding.initializer();
            let errors = first_error.clone();
            self.before_commit(move || {
                initialize();
                errors.take().map_or(Ok(()), Err)
            })?;
            binding
        } else {
            effect(callback)
        };
        if let Some(error) = first_error.take() {
            return Err(error);
        }
        self.effects.push(binding);
        Ok(())
    }

    // Native Text writes use a non-catching host helper and cannot return a
    // JsValue error. Keep the same effect/commit lifecycle without allocating
    // shared first-error storage for a result that is always successful.
    pub(super) fn bind_dom_infallible(
        &mut self,
        mut update: impl FnMut() + 'static,
    ) -> Result<(), JsValue> {
        let owner = self.owner();
        let callback = move || {
            if !owner.is_disposed() {
                update();
            }
        };
        let binding = if self.hydrating {
            let binding = crate::reactive::prepared_effect(callback);
            let initialize = binding.initializer();
            self.before_commit(move || {
                initialize();
                Ok(())
            })?;
            binding
        } else {
            effect(callback)
        };
        self.effects.push(binding);
        Ok(())
    }

    /// Replace a text leaf's content reactively. Values are text, never HTML.
    pub fn text<T: ToString>(
        &mut self,
        target: impl ElementTarget,
        read: impl Fn() -> T + 'static,
    ) -> Result<(), JsValue> {
        let node = target.resolve(self)?;
        self.bind_dom(move || {
            let value = read().to_string();
            if node.text_content().as_deref() != Some(value.as_str()) {
                node.set_text_content(Some(&value));
            }
            Ok(())
        })
    }

    /// Bind a native text node, preserving its identity and all sibling nodes.
    pub fn text_node<T: ToString>(
        &mut self,
        text: &Text,
        read: impl Fn() -> T + 'static,
    ) -> Result<(), JsValue> {
        self.text_node_string(text, move || read().to_string())
    }

    #[doc(hidden)]
    pub fn text_node_string(
        &mut self,
        text: &Text,
        read: impl Fn() -> String + 'static,
    ) -> Result<(), JsValue> {
        let text = text.clone();
        self.bind_dom_infallible(move || {
            let value = read();
            strings::set_text_if_changed(&text, &value);
        })
    }

    #[doc(hidden)]
    pub fn text_node_value(
        &mut self,
        text: &Text,
        read: impl Fn() -> text_value::Output + 'static,
    ) -> Result<(), JsValue> {
        let text = text.clone();
        self.bind_dom_infallible(move || match read() {
            text_value::Output::String(value) => strings::set_text_if_changed(&text, &value),
            text_value::Output::Integer(value) => {
                strings::set_integer_text_if_changed(&text, value)
            }
        })
    }

    /// Bind an attribute. `None` removes it, including boolean attributes.
    pub fn attr(
        &mut self,
        target: impl ElementTarget,
        name: &str,
        read: impl Fn() -> Option<String> + 'static,
    ) -> Result<(), JsValue> {
        let node = target.resolve(self)?;
        let name = name.to_owned();
        self.bind_dom(move || match read() {
            Some(value) => node.set_attribute(&name, &value),
            None => node.remove_attribute(&name),
        })
    }

    pub fn class(
        &mut self,
        target: impl ElementTarget,
        name: &str,
        read: impl Fn() -> bool + 'static,
    ) -> Result<(), JsValue> {
        let node = target.resolve(self)?;
        let name = name.to_owned();
        self.bind_dom(move || {
            node.class_list()
                .toggle_with_force(&name, read())
                .map(|_| ())
        })
    }

    /// Listen to a DOM event. All signal writes in one handler are batched.
    pub fn on(
        &mut self,
        target: impl ElementTarget,
        event: &str,
        handler: impl FnMut(Event) + 'static,
    ) -> Result<(), JsValue> {
        let target: EventTarget = target.resolve(self)?.into();
        let owner = self.owner();
        let handler = RefCell::new(handler);
        let callback = Closure::wrap(Box::new(move |event| {
            if owner.is_active() {
                batch(|| (handler.borrow_mut())(event));
            }
        }) as Box<dyn Fn(Event)>);
        let event = strings::event(event);
        strings::add(&target, &event, callback.as_ref())?;
        self.listeners.push(Listener {
            target,
            event,
            callback,
        });
        Ok(())
    }

    /// Two-way text input binding. Unchanged values preserve the user's cursor.
    pub fn input(
        &mut self,
        target: impl InputTarget,
        value: Signal<String>,
    ) -> Result<(), JsValue> {
        let input = target.resolve_input(self)?;
        let source = value.clone();
        let target = input.clone();
        self.on(&input, "input", move |_| source.set(target.value()))?;
        self.bind_dom(move || {
            let next = value.get();
            if input.value() != next {
                input.set_value(&next);
            }
            Ok(())
        })
    }

    pub fn checked(
        &mut self,
        target: impl InputTarget,
        read: impl Fn() -> bool + 'static,
    ) -> Result<(), JsValue> {
        let input = target.resolve_input(self)?;
        self.bind_dom(move || {
            input.set_checked(read());
            Ok(())
        })
    }

    /// Bind the current input property, including changes after user edits.
    pub fn value(
        &mut self,
        target: impl InputTarget,
        read: impl Fn() -> String + 'static,
    ) -> Result<(), JsValue> {
        let input = target.resolve_input(self)?;
        self.bind_dom(move || {
            let value = read();
            if input.value() != value {
                input.set_value(&value);
            }
            Ok(())
        })
    }

    /// Two-way checkbox binding backed by `Signal<bool>`.
    pub fn checkbox(
        &mut self,
        target: impl InputTarget,
        value: Signal<bool>,
    ) -> Result<(), JsValue> {
        let input = target.resolve_input(self)?;
        let source = value.clone();
        let captured = input.clone();
        self.on(&input, "change", move |_| source.set(captured.checked()))?;
        self.checked(&input, move || value.get())
    }
}

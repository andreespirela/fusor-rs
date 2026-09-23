//! Ordinary Rust guards for external work and effect executions.
use crate::{Effect, effect, untrack};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::Rc,
};

/// Run a callback exactly once when dropped. Combine with an owner registration or
/// [`effect_with_cleanup`] to release external resources.
#[must_use = "retain the guard for the lifetime of the external work"]
pub struct Cleanup(Option<Box<dyn FnOnce()>>);

impl Cleanup {
    pub fn new(callback: impl FnOnce() + 'static) -> Self {
        Self(Some(Box::new(callback)))
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(callback) = self.0.take() {
            untrack(callback);
        }
    }
}

#[derive(Default)]
struct GuardSlot {
    stopped: Cell<bool>,
    value: RefCell<Option<Box<dyn Any>>>,
}

impl GuardSlot {
    fn clear(&self) {
        let value = self.value.take();
        untrack(|| drop(value));
    }
}

/// A reactive execution with a guard released before the next execution and on
/// disposal. Destructors run without dependency tracking or registry borrows.
#[must_use = "retain the effect while its external work should remain active"]
pub struct CleanupEffect {
    effect: Effect,
    slot: Rc<GuardSlot>,
}

impl CleanupEffect {
    pub fn dispose(&self) {
        self.slot.stopped.set(true);
        self.effect.dispose();
        self.slot.clear();
    }
}

impl Drop for CleanupEffect {
    fn drop(&mut self) {
        self.dispose();
    }
}

/// Track reads during setup, retaining its returned Rust value until replacement.
/// Return a [`Cleanup`] for callback-based APIs or any existing RAII guard.
/// Cleanup itself is untracked. A rerun that panics has already released its old
/// guard; Rust's ordinary unwind/abort behavior applies to the setup operation.
pub fn effect_with_cleanup<G: 'static>(mut setup: impl FnMut() -> G + 'static) -> CleanupEffect {
    let slot = Rc::new(GuardSlot::default());
    let captured = slot.clone();
    let effect = effect(move || {
        captured.clear();
        if captured.stopped.get() {
            return;
        }
        let guard = setup();
        if captured.stopped.get() {
            untrack(|| drop(guard));
        } else {
            *captured.value.borrow_mut() = Some(Box::new(guard));
        }
    });
    CleanupEffect { effect, slot }
}

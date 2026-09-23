use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};

type Callback = Box<dyn FnOnce()>;
#[derive(Default)]
struct Cancellation {
    cancelled: Cell<bool>,
    next: Cell<u64>,
    callbacks: RefCell<BTreeMap<u64, Callback>>,
}

/// Cancellation of a single operation. Connect transport cancellation here; stopping
/// polling alone does not stop browser Fetch or undo a server operation.
#[derive(Clone, Default)]
pub struct RequestContext(Rc<Cancellation>);

/// Owner-side cancellation capability, also usable by explicit write adapters.
/// Cancellation stops local work; it cannot establish whether a server committed.
/// Dropping this source cancels its context. Receiving contexts cannot cancel it.
pub struct CancellationSource(Option<RequestContext>);
impl Default for CancellationSource {
    fn default() -> Self {
        Self(Some(RequestContext::default()))
    }
}
impl CancellationSource {
    pub fn context(&self) -> RequestContext {
        self.0.as_ref().expect("live cancellation source").clone()
    }
    pub fn cancel(&self) {
        if let Some(context) = &self.0 {
            context.cancel();
        }
    }
    /// The operation completed. Release the source without signalling cancellation.
    pub fn complete(mut self) {
        self.0.take();
    }
}
impl Drop for CancellationSource {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Unregisters its callback on drop. Keep it alive while the operation is pending.
#[must_use = "retain the cancellation registration while the operation is pending"]
pub struct CancelRegistration {
    context: Weak<Cancellation>,
    id: u64,
}
impl Drop for CancelRegistration {
    fn drop(&mut self) {
        if let Some(context) = self.context.upgrade() {
            let callback = context.callbacks.borrow_mut().remove(&self.id);
            drop(callback);
        }
    }
}
impl RequestContext {
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.get()
    }
    pub fn on_cancel(&self, callback: impl FnOnce() + 'static) -> CancelRegistration {
        let id = self
            .0
            .next
            .get()
            .checked_add(1)
            .expect("cancellation registration overflow");
        self.0.next.set(id);
        if self.is_cancelled() {
            callback();
        } else {
            self.0.callbacks.borrow_mut().insert(id, Box::new(callback));
        }
        CancelRegistration {
            context: Rc::downgrade(&self.0),
            id,
        }
    }
    pub(crate) fn cancel(&self) {
        if self.0.cancelled.replace(true) {
            return;
        }
        let callbacks = self.0.callbacks.take();
        for callback in callbacks.into_values() {
            callback();
        }
    }
}

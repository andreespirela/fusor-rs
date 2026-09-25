use futures_util::future::{AbortHandle, AbortRegistration};
#[cfg(feature = "browser")]
use std::cell::OnceCell;
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
    #[cfg(feature = "browser")]
    controller: OnceCell<web_sys::AbortController>,
}

/// Cancellation of a single operation. Connect transport cancellation here; stopping
/// polling alone does not stop browser Fetch or undo a server operation.
#[derive(Clone, Default)]
pub struct CancellationToken(Rc<Cancellation>);

/// Owner-side cancellation capability, also usable by explicit write adapters.
/// Cancellation stops local work; it cannot establish whether a server committed.
/// Dropping this source cancels its token. Receiving tokens cannot cancel it.
pub struct CancellationSource(Option<CancellationToken>);
impl Default for CancellationSource {
    fn default() -> Self {
        Self(Some(CancellationToken::default()))
    }
}
impl CancellationSource {
    pub fn token(&self) -> CancellationToken {
        self.0.as_ref().expect("live cancellation source").clone()
    }
    pub fn cancel(&self) {
        if let Some(token) = &self.0 {
            token.cancel();
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

/// One in-flight load: the handle that stops polling its future and the source
/// of its token. Dropping it aborts the future, then cancels the token.
pub(crate) struct InFlight {
    abort: AbortHandle,
    source: Option<CancellationSource>,
}
impl InFlight {
    /// The future must be wrapped in `Abortable` with the returned registration.
    pub(crate) fn start() -> (Self, CancellationToken, AbortRegistration) {
        let (abort, registration) = AbortHandle::new_pair();
        let source = CancellationSource::default();
        let token = source.token();
        let in_flight = Self {
            abort,
            source: Some(source),
        };
        (in_flight, token, registration)
    }
    /// The load finished. Release it without signalling cancellation.
    pub(crate) fn complete(mut self) {
        if let Some(source) = self.source.take() {
            source.complete();
        }
    }
}
impl Drop for InFlight {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

/// Unregisters its callback on drop. Keep it alive while the operation is pending.
#[must_use = "retain the cancellation registration while the operation is pending"]
pub struct CancelRegistration {
    token: Weak<Cancellation>,
    id: u64,
}
impl Drop for CancelRegistration {
    fn drop(&mut self) {
        if let Some(token) = self.token.upgrade() {
            let callback = token.callbacks.borrow_mut().remove(&self.id);
            drop(callback);
        }
    }
}
impl CancellationToken {
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
            token: Rc::downgrade(&self.0),
            id,
        }
    }
    /// A browser `AbortSignal` that aborts when this token is cancelled. Pass it to
    /// any Fetch; it lives as long as the token, so it also covers body reads.
    #[cfg(feature = "browser")]
    pub fn abort_signal(&self) -> Result<web_sys::AbortSignal, wasm_bindgen::JsValue> {
        if let Some(controller) = self.0.controller.get() {
            return Ok(controller.signal());
        }
        let controller = web_sys::AbortController::new()?;
        if self.is_cancelled() {
            controller.abort();
        }
        let signal = controller.signal();
        let _ = self.0.controller.set(controller);
        Ok(signal)
    }
    pub(crate) fn cancel(&self) {
        if self.0.cancelled.replace(true) {
            return;
        }
        // Stop the transport before running cleanup callbacks.
        #[cfg(feature = "browser")]
        if let Some(controller) = self.0.controller.get() {
            controller.abort();
        }
        let callbacks = self.0.callbacks.take();
        for callback in callbacks.into_values() {
            callback();
        }
    }
}

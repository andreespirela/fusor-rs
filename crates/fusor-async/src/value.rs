//! Read-only declarations evaluated by a coherent renderer, including while a
//! descendant's ordinary DOM owner is still prepared.
use crate::{CancellationToken, cancellation::InFlight};
use derive_where::derive_where;
use fusor::{
    OwnerHandle, Registration,
    coherence::{Attempt, BoundaryLifetime, ReadLease},
    versions::Versions,
};
use futures_util::future::{Abortable, FutureExt, LocalBoxFuture};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::{Rc, Weak},
};

thread_local! { static NEXT: Cell<u64> = const { Cell::new(0) }; }

/// A scoped, typed result. Pending is an ordinary value, never control flow via
/// panic. The HTML compiler evaluates an `Await` subtree for `Ready`.
pub enum AsyncRead<T> {
    Pending,
    Ready(Rc<T>),
}

enum State<T, E> {
    Idle,
    Pending,
    Ready(Rc<T>),
    Error(Rc<E>),
}
type Load<K, T, E> = dyn Fn(K, CancellationToken) -> LocalBoxFuture<'static, Result<T, E>>;

struct Inner<K, T, E> {
    id: u64,
    owner: OwnerHandle,
    boundary: RefCell<Option<(u64, BoundaryLifetime)>>,
    key: Box<dyn Fn() -> K>,
    selected: RefCell<Option<(K, Versions)>>,
    state: RefCell<State<T, E>>,
    generation: Cell<u64>,
    retry: Cell<u64>,
    request: RefCell<Option<InFlight>>,
    cleanup: RefCell<Option<Registration>>,
    load: Rc<Load<K, T, E>>,
    spawn: Box<dyn Fn(LocalBoxFuture<'static, ()>)>,
}

impl<K, T, E> Inner<K, T, E> {
    fn cancel_work(&self) {
        self.generation.set(
            self.generation
                .get()
                .checked_add(1)
                .expect("read generation overflow"),
        );
        let work = self.request.take();
        if matches!(*self.state.borrow(), State::Pending) {
            *self.state.borrow_mut() = State::Idle;
        }
        drop(work);
    }
    /// Cancel pending work and return to Idle. The retired state is returned so
    /// callers drop its payload only after finishing their own updates.
    fn reset(&self) -> State<T, E> {
        self.cancel_work();
        self.state.replace(State::Idle)
    }
    fn is_current(&self, generation: u64) -> bool {
        !self.owner.is_disposed() && self.generation.get() == generation
    }
}
impl<K, T, E> Drop for Inner<K, T, E> {
    fn drop(&mut self) {
        self.request.get_mut().take();
    }
}

struct Lease<K, T, E>(Weak<Inner<K, T, E>>);
impl<K, T, E> ReadLease for Lease<K, T, E> {
    fn cancel(&self) {
        if let Some(inner) = self.0.upgrade() {
            inner.cancel_work();
        }
    }
}

/// A declaration of one read. All changing request inputs belong in `key`.
/// Loader execution is untracked and uses the supplied local executor. Reads
/// participate when reached by a boundary, not merely when declared.
#[derive_where(Clone)]
pub struct AsyncValue<K, T, E>(Rc<Inner<K, T, E>>);

impl<K: Clone + PartialEq + 'static, T: 'static, E: std::fmt::Display + 'static>
    AsyncValue<K, T, E>
{
    pub fn new<F: Future<Output = Result<T, E>> + 'static>(
        owner: &OwnerHandle,
        key: impl Fn() -> K + 'static,
        load: impl Fn(K, CancellationToken) -> F + 'static,
        spawn: impl Fn(LocalBoxFuture<'static, ()>) + 'static,
    ) -> Self {
        let id = NEXT.with(|next| {
            let id = next.get().checked_add(1).expect("read id overflow");
            next.set(id);
            id
        });
        let inner = Rc::new(Inner {
            id,
            owner: owner.clone(),
            boundary: RefCell::new(None),
            key: Box::new(key),
            selected: RefCell::new(None),
            state: RefCell::new(State::Idle),
            generation: Cell::new(0),
            retry: Cell::new(0),
            request: RefCell::new(None),
            cleanup: RefCell::new(None),
            load: Rc::new(move |key, cancel| Box::pin(load(key, cancel))),
            spawn: Box::new(spawn),
        });
        let weak = Rc::downgrade(&inner);
        let cleanup = owner.on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                drop(inner.reset());
            }
        });
        *inner.cleanup.borrow_mut() = Some(cleanup);
        Self(inner)
    }

    /// Renderer integration. Calling this does not commit the component owner.
    #[doc(hidden)]
    pub fn read(&self, attempt: &Attempt) -> Result<AsyncRead<T>, String> {
        let inner = &self.0;
        if inner.owner.is_disposed() {
            return Err("async read owner was disposed".into());
        }
        inner.adopt_boundary(attempt)?;
        let key = inner.select_key();
        inner.clear_error_on_retry(attempt);
        attempt.register(inner.id, Rc::new(Lease(Rc::downgrade(inner))));
        if matches!(*inner.state.borrow(), State::Idle) {
            inner.start_load(key, attempt);
        }
        inner.outcome(attempt)
    }
}

impl<K: Clone + PartialEq + 'static, T: 'static, E: std::fmt::Display + 'static> Inner<K, T, E> {
    fn adopt_boundary(&self, attempt: &Attempt) -> Result<(), String> {
        let boundary = attempt.boundary_id();
        let (changed, live) = self
            .boundary
            .borrow()
            .as_ref()
            .map_or((true, false), |(id, lifetime)| {
                (*id != boundary, lifetime.is_live())
            });
        if changed && live {
            return Err(
                "one AsyncValue cannot participate in different live async boundaries".into(),
            );
        }
        if changed {
            // A parent may retain a declaration while a conditional Await is
            // removed and remounted. Never retain the disposed boundary or let
            // its request publish into the new view.
            let retired = self.reset();
            self.selected.take();
            self.boundary
                .replace(Some((boundary, attempt.boundary_lifetime())));
            drop(retired);
        }
        Ok(())
    }
    /// Capture the current key; a different key or changed inputs restart the read.
    fn select_key(&self) -> K {
        let (key, versions) = Versions::capture(|| (self.key)());
        let compatible = self
            .selected
            .borrow()
            .as_ref()
            .is_some_and(|(old, inputs)| old == &key && inputs.same(&versions));
        if !compatible {
            let retired = self.reset();
            self.selected.replace(Some((key.clone(), versions)));
            drop(retired);
        }
        key
    }
    fn clear_error_on_retry(&self, attempt: &Attempt) {
        let retry = attempt.retry_generation();
        if self.retry.replace(retry) != retry && matches!(*self.state.borrow(), State::Error(_)) {
            drop(self.state.replace(State::Idle));
        }
    }
    fn start_load(self: &Rc<Self>, key: K, attempt: &Attempt) {
        let generation = self.generation.get();
        let (request, token, registration) = InFlight::start();
        *self.request.borrow_mut() = Some(request);
        *self.state.borrow_mut() = State::Pending;
        let weak = Rc::downgrade(self);
        let load = self.load.clone();
        let notify = attempt.notifier();
        let work = async move {
            let result = load(key, token).await;
            let Some(inner) = weak.upgrade().filter(|inner| inner.is_current(generation)) else {
                return;
            };
            if let Some(request) = inner.request.take() {
                request.complete();
            }
            drop(inner.state.replace(match result {
                Ok(value) => State::Ready(Rc::new(value)),
                Err(error) => State::Error(Rc::new(error)),
            }));
            notify();
        };
        (self.spawn)(Box::pin(Abortable::new(work, registration).map(drop)));
    }
    fn outcome(&self, attempt: &Attempt) -> Result<AsyncRead<T>, String> {
        // Release the state borrow before running the error's Display code.
        let error = match &*self.state.borrow() {
            State::Ready(value) => return Ok(AsyncRead::Ready(value.clone())),
            State::Error(error) => error.clone(),
            State::Idle | State::Pending => {
                attempt.pending();
                return Ok(AsyncRead::Pending);
            }
        };
        Err(error.to_string())
    }
}

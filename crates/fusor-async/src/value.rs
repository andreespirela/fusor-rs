//! Read-only declarations evaluated by a coherent renderer, including while a
//! descendant's ordinary DOM owner is still prepared.
use crate::{CancellationSource, RequestContext};
use fusor::{
    OwnerHandle, Registration,
    coherence::{Attempt, BoundaryLifetime, ReadLease},
    versions::Versions,
};
use futures_util::future::{AbortHandle, Abortable, LocalBoxFuture};
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
struct Work {
    abort: AbortHandle,
    cancellation: Option<CancellationSource>,
}
impl Drop for Work {
    fn drop(&mut self) {
        self.abort.abort();
    }
}
type Load<K, T, E> = dyn Fn(K, RequestContext) -> LocalBoxFuture<'static, Result<T, E>>;

struct Inner<K, T, E> {
    id: u64,
    owner: OwnerHandle,
    boundary: RefCell<Option<(u64, BoundaryLifetime)>>,
    key: Box<dyn Fn() -> K>,
    selected: RefCell<Option<(K, Versions)>>,
    state: RefCell<State<T, E>>,
    generation: Cell<u64>,
    retry: Cell<u64>,
    request: RefCell<Option<Work>>,
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
pub struct AsyncValue<K, T, E>(Rc<Inner<K, T, E>>);
impl<K, T, E> Clone for AsyncValue<K, T, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<K: Clone + PartialEq + 'static, T: 'static, E: std::fmt::Display + 'static>
    AsyncValue<K, T, E>
{
    pub fn new<F: Future<Output = Result<T, E>> + 'static>(
        owner: &OwnerHandle,
        key: impl Fn() -> K + 'static,
        load: impl Fn(K, RequestContext) -> F + 'static,
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
            load: Rc::new(move |key, context| Box::pin(load(key, context))),
            spawn: Box::new(spawn),
        });
        let weak = Rc::downgrade(&inner);
        let cleanup = owner.on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                inner.cancel_work();
                let old = inner.state.replace(State::Idle);
                drop(old);
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
        let boundary = attempt.boundary_id();
        let (changed, live) = inner
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
            inner.cancel_work();
            let old = inner.state.replace(State::Idle);
            inner.selected.take();
            inner
                .boundary
                .replace(Some((boundary, attempt.boundary_lifetime())));
            drop(old);
        }
        let (key, versions) = Versions::capture(|| (inner.key)());
        let selected = inner.selected.borrow().clone();
        let compatible = selected
            .as_ref()
            .is_some_and(|(old, inputs)| old == &key && inputs.same(&versions));
        if !compatible {
            inner.cancel_work();
            let old = inner.state.replace(State::Idle);
            inner.selected.replace(Some((key.clone(), versions)));
            drop(old);
        }
        if inner.retry.get() != attempt.retry_generation() {
            inner.retry.set(attempt.retry_generation());
            if matches!(*inner.state.borrow(), State::Error(_)) {
                let old = inner.state.replace(State::Idle);
                drop(old);
            }
        }
        attempt.register(inner.id, Rc::new(Lease(Rc::downgrade(inner))));
        if matches!(*inner.state.borrow(), State::Idle) {
            let generation = inner.generation.get();
            let cancellation = CancellationSource::default();
            let context = cancellation.context();
            let (abort, registration) = AbortHandle::new_pair();
            *inner.request.borrow_mut() = Some(Work {
                abort,
                cancellation: Some(cancellation),
            });
            *inner.state.borrow_mut() = State::Pending;
            let weak = Rc::downgrade(inner);
            let load = inner.load.clone();
            let notify = attempt.notifier();
            (inner.spawn)(Box::pin(async move {
                let work = async move {
                    let result = load(key, context).await;
                    if let Some(inner) = weak.upgrade().filter(|inner| {
                        !inner.owner.is_disposed() && inner.generation.get() == generation
                    }) {
                        if let Some(mut work) = inner.request.take() {
                            work.cancellation
                                .take()
                                .expect("pending request")
                                .complete();
                        }
                        let old = inner.state.replace(match result {
                            Ok(value) => State::Ready(Rc::new(value)),
                            Err(error) => State::Error(Rc::new(error)),
                        });
                        drop(old);
                        notify();
                    }
                };
                let _ = Abortable::new(work, registration).await;
            }));
        }
        let error = match &*inner.state.borrow() {
            State::Ready(value) => return Ok(AsyncRead::Ready(value.clone())),
            State::Error(error) => Some(error.clone()),
            State::Idle | State::Pending => {
                attempt.pending();
                return Ok(AsyncRead::Pending);
            }
        };
        Err(error.expect("error state").to_string())
    }
}

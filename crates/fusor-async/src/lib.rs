//! Reactive, latest-key-wins data reads with explicit ownership.
//!
//! The core accepts a local task spawner. Enable `browser` for the browser
//! executor and cancellable GET adapter. There is no implicit cache or retry.
//! Only the synchronous key function tracks signals, never the async loader.
mod cancellation;
mod value;
pub use cancellation::{CancelRegistration, CancellationSource, RequestContext};
pub use fusor::coherence::{AsyncBoundary, BoundaryStatus};
pub use value::{AsyncRead, AsyncValue};
#[cfg(feature = "browser")]
pub mod browser;

use fusor::{Effect, OwnerHandle, Registration, Signal, batch, effect, signal, untrack};
use futures_util::future::{AbortHandle, Abortable, LocalBoxFuture};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};

/// A successful value and the key that actually produced it.
#[derive(Debug)]
pub struct Data<K, T> {
    pub key: K,
    pub value: Rc<T>,
}
impl<K: Clone, T> Clone for Data<K, T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            value: self.value.clone(),
        }
    }
}

/// Previous data is explicitly labelled; it is never passed off as a new key's result.
#[derive(Debug)]
pub enum ResourceState<K, T, E> {
    Idle,
    Loading {
        key: K,
        previous: Option<Data<K, T>>,
    },
    Ready(Data<K, T>),
    Error {
        key: K,
        error: Rc<E>,
        previous: Option<Data<K, T>>,
    },
    Disposed,
}
impl<K: Clone, T, E> Clone for ResourceState<K, T, E> {
    fn clone(&self) -> Self {
        match self {
            Self::Idle => Self::Idle,
            Self::Disposed => Self::Disposed,
            Self::Loading { key, previous } => Self::Loading {
                key: key.clone(),
                previous: previous.clone(),
            },
            Self::Ready(data) => Self::Ready(data.clone()),
            Self::Error {
                key,
                error,
                previous,
            } => Self::Error {
                key: key.clone(),
                error: error.clone(),
                previous: previous.clone(),
            },
        }
    }
}
impl<K, T, E> ResourceState<K, T, E> {
    pub fn data(&self) -> Option<&Data<K, T>> {
        match self {
            Self::Ready(data) => Some(data),
            Self::Loading { previous, .. } | Self::Error { previous, .. } => previous.as_ref(),
            _ => None,
        }
    }
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }
}

type Loader<K, T, E> = dyn Fn(K, RequestContext) -> LocalBoxFuture<'static, Result<T, E>>;
type Spawner = dyn Fn(LocalBoxFuture<'static, ()>);
struct Request {
    abort: AbortHandle,
    context: RequestContext,
}
impl Request {
    fn cancel(self) {
        self.abort.abort();
        self.context.cancel();
    }
}
struct Inner<K, T, E> {
    owner: OwnerHandle,
    state: Signal<ResourceState<K, T, E>>,
    key: RefCell<Option<K>>,
    generation: Cell<u64>,
    disposed: Cell<bool>,
    request: RefCell<Option<Request>>,
    subscription: RefCell<Option<Effect>>,
    registrations: RefCell<Vec<Registration>>,
    load: Rc<Loader<K, T, E>>,
    spawn: Rc<Spawner>,
}
impl<K, T, E> Drop for Inner<K, T, E> {
    fn drop(&mut self) {
        if let Some(request) = self.request.get_mut().take() {
            request.cancel();
        }
    }
}

/// A shared handle to one owned read, with no `Clone` bound on data or errors.
/// Dropping its last handle cancels the read and detaches its key subscription.
/// Cloned handles cannot extend the owner's lifetime.
pub struct Resource<K, T, E>(Rc<Inner<K, T, E>>);
impl<K, T, E> Clone for Resource<K, T, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<K: Clone + PartialEq + 'static, T: 'static, E: 'static> Resource<K, T, E> {
    /// `spawn` must schedule the future on a local executor. The loader is first
    /// called when the scheduled future is polled after owner activation.
    /// Return `None` from `key` to disable loading and clear previous data.
    pub fn new<F: Future<Output = Result<T, E>> + 'static>(
        owner: &OwnerHandle,
        key: impl Fn() -> Option<K> + 'static,
        load: impl Fn(K, RequestContext) -> F + 'static,
        spawn: impl Fn(LocalBoxFuture<'static, ()>) + 'static,
    ) -> Self {
        let inner = Rc::new(Inner {
            owner: owner.clone(),
            state: signal(ResourceState::Idle),
            key: RefCell::new(None),
            generation: Cell::new(0),
            disposed: Cell::new(false),
            request: RefCell::new(None),
            subscription: RefCell::new(None),
            registrations: RefCell::new(Vec::new()),
            load: Rc::new(move |key, context| Box::pin(load(key, context))),
            spawn: Rc::new(spawn),
        });
        let weak = Rc::downgrade(&inner);
        let cleanup = owner.on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                dispose(&inner);
            }
        });
        inner.registrations.borrow_mut().push(cleanup);
        if !inner.disposed.get() {
            let weak = Rc::downgrade(&inner);
            let subscription = effect(move || {
                let next = key();
                if let Some(inner) = weak.upgrade() {
                    untrack(|| change(&inner, next, false));
                }
            });
            if inner.disposed.get() {
                subscription.dispose();
            } else {
                *inner.subscription.borrow_mut() = Some(subscription);
            }
            let weak = Rc::downgrade(&inner);
            // Register only while prepared: an active owner already started in the effect.
            if !owner.is_active() {
                let activation = owner.on_activate(move || {
                    if let Some(inner) = weak.upgrade() {
                        let key = inner.key.borrow().clone();
                        change(&inner, key, true);
                    }
                });
                inner.registrations.borrow_mut().push(activation);
            }
        }
        Self(inner)
    }
    pub fn get(&self) -> ResourceState<K, T, E> {
        self.0.state.get()
    }
    pub fn with<R>(&self, read: impl FnOnce(&ResourceState<K, T, E>) -> R) -> R {
        self.0.state.with(read)
    }
    /// Reload the current key. No-op for a disabled or disposed resource.
    pub fn refresh(&self) {
        let key = self.0.key.borrow().clone();
        untrack(|| change(&self.0, key, true));
    }
    /// Permanently stop this resource, even while its owner remains alive.
    pub fn dispose(&self) {
        dispose(&self.0);
    }
}

fn invalidate<K, T, E>(inner: &Inner<K, T, E>) -> u64 {
    let generation = inner
        .generation
        .get()
        .checked_add(1)
        .expect("resource generation overflow");
    inner.generation.set(generation);
    let request = inner.request.take();
    if let Some(request) = request {
        request.cancel();
    }
    generation
}
fn dispose<K, T, E>(inner: &Inner<K, T, E>) {
    if inner.disposed.replace(true) {
        return;
    }
    batch(|| {
        invalidate(inner);
        let subscription = inner.subscription.take();
        drop(subscription);
        publish(inner, ResourceState::Disposed);
    });
}

// Prepare the complete state before borrowing, notify (or queue in a batch),
// then retire payloads outside the borrow. Caller-written update closures keep
// their non-reentrant contract; framework-owned replacement does not drop there.
fn publish<K, T, E>(inner: &Inner<K, T, E>, next: ResourceState<K, T, E>) {
    let retired = inner.state.update(|state| std::mem::replace(state, next));
    drop(retired);
}
fn change<K: Clone + PartialEq + 'static, T: 'static, E: 'static>(
    inner: &Rc<Inner<K, T, E>>,
    next: Option<K>,
    force: bool,
) {
    if inner.disposed.get() || inner.owner.is_disposed() {
        return;
    }
    if !force && *inner.key.borrow() == next {
        return;
    }
    let retired_key = inner.key.replace(next.clone());
    batch(|| {
        let generation = invalidate(inner);
        if inner.disposed.get() || inner.generation.get() != generation {
            return;
        }
        let Some(key) = next else {
            publish(inner, ResourceState::Idle);
            return;
        };
        if !inner.owner.is_active() {
            return;
        }
        let previous = inner.state.with_untracked(|state| state.data().cloned());
        let (abort, registration) = AbortHandle::new_pair();
        let context = RequestContext::default();
        *inner.request.borrow_mut() = Some(Request {
            abort,
            context: context.clone(),
        });
        publish(
            inner,
            ResourceState::Loading {
                key: key.clone(),
                previous: previous.clone(),
            },
        );
        let weak = Rc::downgrade(inner);
        let load = inner.load.clone();
        (inner.spawn)(Box::pin(async move {
            let work = async move {
                let result = load(key.clone(), context).await;
                if let Some(inner) = weak.upgrade().filter(|i| current(i, generation)) {
                    // Remove completed handles before notifying subscribers. Notifications
                    // may immediately dispose this resource or start another generation.
                    inner.request.take();
                    let next = match result {
                        Ok(value) => ResourceState::Ready(Data {
                            key,
                            value: Rc::new(value),
                        }),
                        Err(error) => ResourceState::Error {
                            key,
                            error: Rc::new(error),
                            previous,
                        },
                    };
                    publish(&inner, next);
                    // On success, the unused previous data is retired here,
                    // after Ready has been published, never in an update closure.
                }
            };
            let _ = Abortable::new(work, registration).await;
        }));
    });
    // Key destructors see the completed transition, including cancellation and
    // scheduling. Cancellation callbacks retain their existing pre-publication order.
    drop(retired_key);
}
fn current<K, T, E>(inner: &Inner<K, T, E>, generation: u64) -> bool {
    !inner.disposed.get() && inner.owner.is_active() && inner.generation.get() == generation
}

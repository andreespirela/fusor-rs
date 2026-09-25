//! Deterministic helpers for application tests. No browser or wall-clock sleeps.
//! These control framework futures, not the browser's event loop or foreign JS.
use fusor::{Cleanup, OwnerHandle, Registration};
use fusor_async::CancellationToken;
use futures_channel::oneshot;
use futures_executor::LocalPool;
use futures_util::{future::LocalBoxFuture, task::LocalSpawnExt};
use std::{
    cell::Cell, cell::RefCell, collections::VecDeque, future::Future, rc::Rc, time::Duration,
};

/// Single-threaded executor. Pass [`Self::spawner`] to a resource or query client.
#[derive(Default)]
pub struct TestExecutor(RefCell<LocalPool>);

impl TestExecutor {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn spawner(&self) -> impl Fn(LocalBoxFuture<'static, ()>) + 'static {
        let spawner = self.0.borrow().spawner();
        move |future| {
            spawner
                .spawn_local(future)
                .expect("test executor was dropped")
        }
    }
    /// Poll ready work until every remaining task is pending. Never advances time.
    /// Do not call recursively from a task running on this executor.
    pub fn run_until_stalled(&self) {
        self.0.borrow_mut().run_until_stalled();
    }
}

/// Monotonic manually advanced time for explicit freshness/retention policies.
#[derive(Clone, Default)]
pub struct TestClock(Rc<Cell<Duration>>);
impl TestClock {
    pub fn now(&self) -> Duration {
        self.0.get()
    }
    pub fn advance(&self, duration: Duration) {
        self.0.set(
            self.now()
                .checked_add(duration)
                .expect("test clock overflow"),
        );
    }
    pub fn reader(&self) -> impl Fn() -> Duration + 'static {
        let clock = self.clone();
        move || clock.now()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RequestCounts {
    pub started: usize,
    pub completed: usize,
    pub cancelled: usize,
    /// Futures not yet dropped, including cancelled tasks awaiting executor polling.
    pub live: usize,
}

struct Requests<K, T, E> {
    queue: RefCell<VecDeque<PendingRequest<K, T, E>>>,
    counts: Cell<RequestCounts>,
}

/// A cloneable loader with explicit completion order and observable cancellation.
pub struct ControlledLoader<K, T, E>(Rc<Requests<K, T, E>>);
impl<K, T, E> Clone for ControlledLoader<K, T, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<K, T, E> Default for ControlledLoader<K, T, E> {
    fn default() -> Self {
        Self(Rc::new(Requests {
            queue: RefCell::new(VecDeque::new()),
            counts: Cell::new(RequestCounts::default()),
        }))
    }
}
impl<K: 'static, T: 'static, E: 'static> ControlledLoader<K, T, E> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn counts(&self) -> RequestCounts {
        self.0.counts.get()
    }
    pub fn next_request(&self) -> Option<PendingRequest<K, T, E>> {
        self.0.queue.borrow_mut().pop_front()
    }
    pub fn load(
        &self,
        key: K,
        cancel: CancellationToken,
    ) -> impl Future<Output = Result<T, E>> + 'static + use<K, T, E> {
        let (sender, receiver) = oneshot::channel();
        self.0.queue.borrow_mut().push_back(PendingRequest {
            key,
            cancel: cancel.clone(),
            sender,
        });
        let mut counts = self.counts();
        counts.started += 1;
        counts.live += 1;
        self.0.counts.set(counts);
        let requests = self.0.clone();
        let cleanup = Cleanup::new(move || {
            let mut counts = requests.counts.get();
            counts.live -= 1;
            requests.counts.set(counts);
        });
        let requests = self.0.clone();
        let cancellation = cancel.on_cancel(move || {
            let mut counts = requests.counts.get();
            counts.cancelled += 1;
            requests.counts.set(counts);
        });
        let requests = self.0.clone();
        async move {
            let _cleanup = cleanup;
            let _cancellation = cancellation;
            let value = receiver.await.expect("complete the test request or cancel its resource before dropping the pending request");
            let mut counts = requests.counts.get();
            counts.completed += 1;
            requests.counts.set(counts);
            value
        }
    }
}

/// One requested key. Retain this handle to complete requests in any order.
pub struct PendingRequest<K, T, E> {
    pub key: K,
    cancel: CancellationToken,
    sender: oneshot::Sender<Result<T, E>>,
}
impl<K, T, E> PendingRequest<K, T, E> {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
    /// Returns the supplied result if the future has already been dropped.
    /// Successfully queuing a result does not mean a disposed owner can publish it.
    pub fn complete(self, result: Result<T, E>) -> Result<(), Result<T, E>> {
        self.sender.send(result)
    }
}

/// Observe a lifetime without extending it. Retain the probe to count cleanup.
pub struct OwnerProbe {
    owner: OwnerHandle,
    cleanups: Rc<Cell<usize>>,
    _registration: Registration,
}
impl OwnerProbe {
    pub fn new(owner: &OwnerHandle) -> Self {
        let cleanups = Rc::new(Cell::new(0));
        let captured = cleanups.clone();
        let registration = owner.on_cleanup(move || captured.set(captured.get() + 1));
        Self {
            owner: owner.clone(),
            cleanups,
            _registration: registration,
        }
    }
    pub fn is_active(&self) -> bool {
        self.owner.is_active()
    }
    pub fn is_disposed(&self) -> bool {
        self.owner.is_disposed()
    }
    pub fn cleanup_count(&self) -> usize {
        self.cleanups.get()
    }
}

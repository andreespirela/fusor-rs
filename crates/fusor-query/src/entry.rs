use super::*;
use fusor::{Effect, effect};
use fusor_async::{Data, Resource, ResourceState};
use std::cell::Cell;

pub(super) struct Entry<K, T, E> {
    owner: Owner,
    key: K,
    request_key: Signal<Option<K>>,
    resource: Resource<K, T, E>,
    pub state: Signal<QueryState<K, T, E>>,
    cached: RefCell<Option<Data<K, T>>>,
    updated: Cell<Option<Duration>>,
    invalid: Cell<bool>,
    pub observers: Cell<usize>,
    pub unused_since: Cell<Duration>,
    clock: Rc<dyn Fn() -> Duration>,
    watch: RefCell<Option<Effect>>,
}

impl<K: Clone + Ord + 'static, T: 'static, E: 'static> Entry<K, T, E> {
    pub fn new(client: &Inner<K, T, E>, key: K) -> Rc<Self> {
        let owner = Owner::child(&client.owner.handle());
        let request_key = signal(None);
        let input = request_key.clone();
        let load = client.load.clone();
        let spawn = client.spawn.clone();
        let resource = Resource::new(
            &owner.handle(),
            move || input.get(),
            move |key, context| load(key, context),
            move |future| spawn(future),
        );
        let entry = Rc::new(Self {
            owner,
            key,
            request_key,
            resource,
            state: signal(QueryState::Idle),
            cached: RefCell::new(None),
            updated: Cell::new(None),
            invalid: Cell::new(true),
            observers: Cell::new(0),
            unused_since: Cell::new((client.clock)()),
            clock: client.clock.clone(),
            watch: RefCell::new(None),
        });
        let weak = Rc::downgrade(&entry);
        let watch = effect(move || {
            if let Some(entry) = weak.upgrade() {
                let state = entry.resource.get();
                untrack(|| entry.publish(state));
            }
        });
        *entry.watch.borrow_mut() = Some(watch);
        entry.owner.commit();
        entry
    }

    fn publish(&self, state: ResourceState<K, T, E>) {
        let state = match state {
            ResourceState::Ready(data) => {
                let previous = self.cached.replace(Some(data.clone()));
                drop(previous);
                self.updated.set(Some((self.clock)()));
                self.invalid.set(false);
                QueryState::Ready(data)
            }
            ResourceState::Loading { key, .. } => QueryState::Loading {
                key,
                previous: self.cached.borrow().clone(),
            },
            ResourceState::Error { key, error, .. } => {
                self.invalid.set(true);
                QueryState::Error {
                    key,
                    error,
                    previous: self.cached.borrow().clone(),
                }
            }
            ResourceState::Idle => self
                .cached
                .borrow()
                .clone()
                .map(QueryState::Ready)
                .unwrap_or(QueryState::Idle),
            ResourceState::Disposed => {
                let previous = self.cached.take();
                drop(previous);
                QueryState::Disposed
            }
        };
        let previous = self
            .state
            .update(|current| std::mem::replace(current, state));
        drop(previous);
    }

    pub fn acquire(&self, freshness: Freshness) {
        self.observers.set(self.observers.get() + 1);
        let fresh = !self.invalid.get()
            && self.updated.get().is_some_and(|at| match freshness {
                Freshness::Forever => true,
                Freshness::For(duration) => (self.clock)().saturating_sub(at) < duration,
            });
        if !fresh && !self.resource.with(|s| s.is_loading()) {
            self.start();
        }
    }

    fn start(&self) {
        if self.request_key.get_untracked().is_some() {
            self.resource.refresh();
        } else {
            self.request_key.set(Some(self.key.clone()));
        }
    }

    pub fn release(&self) {
        let remaining = self
            .observers
            .get()
            .checked_sub(1)
            .expect("query subscription released once");
        self.observers.set(remaining);
        if remaining == 0 {
            self.unused_since.set((self.clock)());
            // Disable the resource to cancel its generation; cached data is held
            // separately and can survive cancelled revalidation.
            self.request_key.set(None);
        }
    }

    pub fn invalidate(&self) {
        self.invalid.set(true);
        if self.observers.get() != 0 {
            self.start();
        }
    }

    pub fn expired(&self, now: Duration, retention: Duration) -> bool {
        self.observers.get() == 0 && now.saturating_sub(self.unused_since.get()) >= retention
    }

    pub fn dispose(&self) {
        self.owner.dispose();
    }
}

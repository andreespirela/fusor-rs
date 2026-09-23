use super::*;
use fusor::{Effect, effect};
use std::cell::Cell;

struct Observer<K: Clone + Ord + 'static, T: 'static, E: 'static> {
    client: QueryClient<K, T, E>,
    owner: OwnerHandle,
    disposed: Cell<bool>,
    entry: RefCell<Option<Rc<Entry<K, T, E>>>>,
    key: RefCell<Option<K>>,
    state: Signal<QueryState<K, T, E>>,
    wake: Signal<()>,
    effect: RefCell<Option<Effect>>,
    registrations: RefCell<Vec<Registration>>,
}

impl<K: Clone + Ord + 'static, T: 'static, E: 'static> Observer<K, T, E> {
    fn release(&self) {
        let entry = self.entry.take();
        if let Some(entry) = entry {
            entry.release();
        }
    }
    fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }
        untrack(|| {
            batch(|| {
                self.effect.take();
                self.release();
                let previous = self
                    .state
                    .update(|state| std::mem::replace(state, QueryState::Disposed));
                drop(previous);
            })
        });
    }
}
impl<K: Clone + Ord + 'static, T: 'static, E: 'static> Drop for Observer<K, T, E> {
    fn drop(&mut self) {
        self.dispose();
    }
}

/// A read-only handle to one owner-bound subscription. Last-handle drop detaches
/// it; owner disposal detaches it even if handles remain in application state.
pub struct Query<K: Clone + Ord + 'static, T: 'static, E: 'static>(Rc<Observer<K, T, E>>);
impl<K: Clone + Ord + 'static, T: 'static, E: 'static> Clone for Query<K, T, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<K: Clone + Ord + 'static, T: 'static, E: 'static> Query<K, T, E> {
    pub(super) fn new(
        client: QueryClient<K, T, E>,
        owner: &OwnerHandle,
        key: impl Fn() -> Option<K> + 'static,
    ) -> Self {
        let inner = Rc::new(Observer {
            client,
            owner: owner.clone(),
            disposed: Cell::new(false),
            entry: RefCell::new(None),
            key: RefCell::new(None),
            state: signal(QueryState::Idle),
            wake: signal(()),
            effect: RefCell::new(None),
            registrations: RefCell::new(Vec::new()),
        });
        let weak = Rc::downgrade(&inner);
        let cleanup = owner.on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                inner.dispose();
            }
        });
        inner.registrations.borrow_mut().push(cleanup);
        if inner.disposed.get() {
            return Self(inner);
        }
        let weak = Rc::downgrade(&inner);
        let watcher = effect(move || {
            let Some(inner) = weak.upgrade().filter(|i| !i.disposed.get()) else {
                return;
            };
            inner.wake.get();
            if !inner.client.0.alive.get() {
                inner.dispose();
                return;
            }
            let next = key();
            if !inner.owner.is_active() {
                return;
            }
            untrack(|| {
                if *inner.key.borrow() != next || inner.entry.borrow().is_none() {
                    inner.release();
                    if inner.disposed.get()
                        || !inner.owner.is_active()
                        || !inner.client.0.alive.get_untracked()
                    {
                        return;
                    }
                    *inner.key.borrow_mut() = next.clone();
                    let entry = next.as_ref().and_then(|key| inner.client.acquire(key));
                    if inner.disposed.get()
                        || !inner.owner.is_active()
                        || !inner.client.0.alive.get_untracked()
                    {
                        if let Some(entry) = entry {
                            entry.release();
                        }
                    } else {
                        *inner.entry.borrow_mut() = entry;
                    }
                }
            });
            if !inner.client.0.alive.get_untracked() {
                inner.dispose();
                return;
            }
            let entry = inner.entry.borrow().clone();
            let state = if let Some(entry) = entry {
                entry.state.get()
            } else if let Some(key) = next {
                QueryState::Capacity { key }
            } else {
                QueryState::Idle
            };
            if !inner.disposed.get() {
                untrack(|| {
                    let previous = inner
                        .state
                        .update(|current| std::mem::replace(current, state));
                    drop(previous);
                });
            }
        });
        if inner.disposed.get() {
            watcher.dispose();
        } else {
            *inner.effect.borrow_mut() = Some(watcher);
        }
        let weak = Rc::downgrade(&inner);
        let activation = owner.on_activate(move || {
            if let Some(inner) = weak.upgrade() {
                inner.wake.update(|_| ());
            }
        });
        inner.registrations.borrow_mut().push(activation);
        Self(inner)
    }
    pub fn get(&self) -> QueryState<K, T, E> {
        self.0.state.get()
    }
    pub fn with<R>(&self, read: impl FnOnce(&QueryState<K, T, E>) -> R) -> R {
        self.0.state.with(read)
    }
    pub fn dispose(&self) {
        self.0.dispose();
    }
    /// Explicitly refresh this key, or retry a capacity-limited subscription.
    pub fn refresh(&self) {
        if self.0.disposed.get() || !self.0.owner.is_active() {
            return;
        }
        untrack(|| {
            let key = self.0.key.borrow().clone();
            if self.0.entry.borrow().is_some() {
                if let Some(key) = key {
                    self.0.client.invalidate(&key);
                }
            } else {
                self.0.wake.update(|_| ());
            }
        });
    }
}

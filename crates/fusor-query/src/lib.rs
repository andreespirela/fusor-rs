//! Shared reads with explicit freshness, retention and ownership.
//!
//! Each [`QueryClient`] is a typed query definition: it binds one loader to its
//! key space. Cloning it shares a cache; creating another client creates a distinct
//! query identity, even with identical Rust key/value types. Provide cloned clients
//! through typed owner context. No global registry, automatic retries or mutations.
mod entry;
mod observer;
mod state;
pub use observer::Query;
pub use state::{CacheInfo, Freshness, QueryOptions, QueryState};

use entry::Entry;
use fusor::{Owner, OwnerHandle, Registration, Signal, batch, signal, untrack};
use fusor_async::CancellationToken;
use futures_util::future::LocalBoxFuture;
use std::{cell::RefCell, collections::BTreeMap, future::Future, rc::Rc, time::Duration};

type Loader<K, T, E> = dyn Fn(K, CancellationToken) -> LocalBoxFuture<'static, Result<T, E>>;
type Spawner = dyn Fn(LocalBoxFuture<'static, ()>);
type Entries<K, T, E> = BTreeMap<K, Rc<Entry<K, T, E>>>;
struct Inner<K: Clone + Ord + 'static, T: 'static, E: 'static> {
    owner: Owner,
    alive: Signal<bool>,
    options: QueryOptions,
    entries: RefCell<Entries<K, T, E>>,
    load: Rc<Loader<K, T, E>>,
    spawn: Rc<Spawner>,
    clock: Rc<dyn Fn() -> Duration>,
    cleanup: RefCell<Option<Registration>>,
}

/// One loader, one typed key space, and one application/session lifetime.
/// Consumer owners only own subscriptions. The last consumer leaving cancels
/// unfinished work; completed data may remain until eviction or client disposal.
pub struct QueryClient<K: Clone + Ord + 'static, T: 'static, E: 'static>(Rc<Inner<K, T, E>>);
impl<K: Clone + Ord + 'static, T: 'static, E: 'static> Clone for QueryClient<K, T, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<K: Clone + Ord + 'static, T: 'static, E: 'static> QueryClient<K, T, E> {
    /// `clock` must be monotonic. `spawn` schedules local futures without blocking.
    /// The client's owner is a child of `parent`; a retained clone cannot revive it.
    pub fn new<F: Future<Output = Result<T, E>> + 'static>(
        parent: &OwnerHandle,
        options: QueryOptions,
        load: impl Fn(K, CancellationToken) -> F + 'static,
        spawn: impl Fn(LocalBoxFuture<'static, ()>) + 'static,
        clock: impl Fn() -> Duration + 'static,
    ) -> Self {
        let inner = Rc::new(Inner {
            owner: Owner::child(parent),
            alive: signal(true),
            options,
            entries: RefCell::new(BTreeMap::new()),
            load: Rc::new(move |key, request| Box::pin(load(key, request))),
            spawn: Rc::new(spawn),
            clock: Rc::new(clock),
            cleanup: RefCell::new(None),
        });
        let weak = Rc::downgrade(&inner);
        let cleanup = inner.owner.handle().on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                batch(|| {
                    inner.alive.set(false);
                    let entries = inner.entries.take();
                    for entry in entries.into_values() {
                        entry.dispose();
                    }
                });
            }
        });
        *inner.cleanup.borrow_mut() = Some(cleanup);
        inner.owner.commit();
        Self(inner)
    }

    /// Observe a reactive key. Loading waits for both client and view activation.
    /// Cloning the returned handle shares one subscription, not an extra observer.
    pub fn observe(
        &self,
        owner: &OwnerHandle,
        key: impl Fn() -> Option<K> + 'static,
    ) -> Query<K, T, E> {
        Query::new(self.clone(), owner, key)
    }

    /// Mark a key stale and refresh it immediately if it has active observers.
    /// Returns false if it is absent. Inactive entries reload when next observed.
    pub fn invalidate(&self, key: &K) -> bool {
        untrack(|| {
            let entry = self.0.entries.borrow().get(key).cloned();
            if let Some(entry) = entry {
                entry.invalidate();
                true
            } else {
                false
            }
        })
    }

    /// Release expired inactive entries. Retention is lazy, also checked whenever
    /// a new subscription selects a key. No background timers or hidden refetches.
    pub fn collect(&self) -> usize {
        untrack(|| {
            let now = (self.0.clock)();
            let mut entries = self.0.entries.borrow_mut();
            let keys: Vec<_> = entries
                .iter()
                .filter(|(_, e)| e.expired(now, self.0.options.retention))
                .map(|(k, _)| k.clone())
                .collect();
            let removed: Vec<_> = keys.iter().filter_map(|key| entries.remove(key)).collect();
            drop(entries);
            let count = removed.len();
            for entry in removed {
                entry.dispose();
            }
            count
        })
    }

    /// Counts describe this cache, without tracking a reactive dependency.
    pub fn info(&self) -> CacheInfo {
        let entries = self.0.entries.borrow();
        CacheInfo {
            entries: entries.len(),
            observers: entries.values().map(|e| e.observers.get()).sum(),
        }
    }

    /// End this identity permanently, cancelling reads and clearing cached data.
    /// Use a new client for a new login/tenant. Already cloned application values
    /// remain ordinary Rust values and cannot be retroactively revoked.
    pub fn dispose(&self) {
        untrack(|| self.0.owner.dispose());
    }

    fn acquire(&self, key: &K) -> Option<Rc<Entry<K, T, E>>> {
        self.collect();
        if !self.0.alive.get_untracked() {
            return None;
        }
        let existing = self.0.entries.borrow().get(key).cloned();
        let entry = if let Some(entry) = existing {
            entry
        } else {
            let removed = {
                let mut entries = self.0.entries.borrow_mut();
                if entries.len() >= self.0.options.capacity.get() {
                    let oldest = entries
                        .iter()
                        .filter(|(_, e)| e.observers.get() == 0)
                        .min_by_key(|(_, e)| e.unused_since.get())
                        .map(|(key, _)| key.clone());
                    let oldest = oldest?;
                    entries.remove(&oldest)
                } else {
                    None
                }
            };
            if let Some(entry) = removed {
                entry.dispose();
            }
            if !self.0.alive.get_untracked() {
                return None;
            }
            let entry = Entry::new(&self.0, key.clone());
            self.0
                .entries
                .borrow_mut()
                .insert(key.clone(), entry.clone());
            entry
        };
        entry.acquire(self.0.options.freshness);
        Some(entry)
    }
}

#[cfg(feature = "browser")]
pub mod browser {
    //! Browser executor and monotonic performance clock. Enable the async crate's
    //! `browser` feature separately when using its Fetch adapter.
    use super::*;
    pub fn client<K, T, E, F>(
        parent: &OwnerHandle,
        options: QueryOptions,
        load: impl Fn(K, CancellationToken) -> F + 'static,
    ) -> QueryClient<K, T, E>
    where
        K: Clone + Ord + 'static,
        T: 'static,
        E: 'static,
        F: Future<Output = Result<T, E>> + 'static,
    {
        let performance = web_sys::window()
            .and_then(|w| w.performance())
            .expect("query client requires browser performance clock");
        QueryClient::new(
            parent,
            options,
            load,
            wasm_bindgen_futures::spawn_local,
            move || Duration::from_secs_f64(performance.now() / 1000.0),
        )
    }
}

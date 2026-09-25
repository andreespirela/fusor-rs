//! Browser executor bindings: `read` and `resource` run loads with `spawn_local`.
use crate::{AsyncValue, CancellationToken, Resource};
use fusor::OwnerHandle;
use std::future::Future;

/// Declare a read participating in the nearest generated coherent region.
pub fn read<K, T, E, F>(
    owner: &OwnerHandle,
    key: impl Fn() -> K + 'static,
    load: impl Fn(K, CancellationToken) -> F + 'static,
) -> AsyncValue<K, T, E>
where
    K: Clone + PartialEq + 'static,
    T: 'static,
    E: std::fmt::Display + 'static,
    F: Future<Output = Result<T, E>> + 'static,
{
    AsyncValue::new(owner, key, load, wasm_bindgen_futures::spawn_local)
}

/// Create a [`Resource`] whose loads run with `wasm_bindgen_futures::spawn_local`.
pub fn resource<K, T, E, F>(
    owner: &OwnerHandle,
    key: impl Fn() -> Option<K> + 'static,
    load: impl Fn(K, CancellationToken) -> F + 'static,
) -> Resource<K, T, E>
where
    K: Clone + PartialEq + 'static,
    T: 'static,
    E: 'static,
    F: Future<Output = Result<T, E>> + 'static,
{
    Resource::new(owner, key, load, wasm_bindgen_futures::spawn_local)
}

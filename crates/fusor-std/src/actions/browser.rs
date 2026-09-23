//! Browser scheduling only; request serialization and outcome classification
//! belong to the application transport.
use super::{Action, Outcome, RequestContext, SavePolicy};
use fusor::OwnerHandle;
use std::{future::Future, rc::Rc};

pub fn action<C: 'static, T: 'static, E: 'static, F: Future<Output = Outcome<T, E>> + 'static>(
    owner: &OwnerHandle,
    policy: SavePolicy,
    load: impl Fn(Rc<C>, RequestContext) -> F + 'static,
) -> Action<C, T, E> {
    Action::new(owner, policy, load, wasm_bindgen_futures::spawn_local)
}

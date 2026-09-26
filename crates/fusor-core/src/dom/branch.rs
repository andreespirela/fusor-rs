//! Retained, wrapper-free structural branches. Same-case data changes never
//! replace the branch's owner; a different case is prepared before replacement.
use super::{JsValue, MountPoint, Scope};
use crate::{OwnerHandle, Signal, signal, untrack};

impl Scope {
    #[doc(hidden)]
    pub fn branch_at<T: Clone + PartialEq + 'static>(
        &mut self,
        target: &MountPoint,
        read: impl Fn() -> (usize, T) + 'static,
        prepare: impl Fn(usize, Signal<T>, &OwnerHandle) -> Result<Scope, JsValue> + 'static,
    ) -> Result<(), JsValue> {
        let mut hydrating = self.is_hydrating();
        let target = target.clone();
        let parent = self.owner();
        let mut current: Option<(usize, Signal<T>, Scope)> = None;
        self.bind(move || {
            let (key, data) = read();
            untrack(|| {
                if let Some((old, value, _)) = &current {
                    if *old == key {
                        value.set(data);
                        return Ok(());
                    }
                }
                let initial = std::mem::take(&mut hydrating);
                let hydration = if initial {
                    let marker = target
                        .start
                        .next_sibling()
                        .ok_or_else(|| JsValue::from_str("missing server branch marker"))?;
                    if marker.node_value().as_deref() != Some(&format!("fusor:branch:{key}")) {
                        return Err(JsValue::from_str(
                            "server branch differs from browser branch",
                        ));
                    }
                    Some(MountPoint {
                        start: marker,
                        end: target.end.clone(),
                    })
                } else {
                    None
                };
                let value = signal(data);
                let child = super::children::with_hydration(hydration.clone(), || {
                    prepare(key, value.clone(), &parent)
                })?;
                child.finish_prepare()?;
                if let Some(hydration) = hydration {
                    hydration
                        .start
                        .parent_node()
                        .unwrap()
                        .remove_child(&hydration.start)?;
                }
                if !initial {
                    child.attach_fragment(&target)?;
                }
                let old = current.replace((key, value, child));
                drop(old);
                current.as_ref().expect("inserted branch").2.try_commit()?;
                Ok(())
            })
        })
    }
}

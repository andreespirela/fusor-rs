//! Retained, wrapper-free structural branches. Same-case data changes never
//! replace the branch's owner; a different case is prepared before replacement.
use super::{JsValue, MountPoint, Scope, component::Retained, hydration};
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
        let mut current: Retained<(usize, Signal<T>)> = Retained::default();
        self.bind(move || {
            let (case, data) = read();
            untrack(|| {
                if let Some((shown, value)) = current.key() {
                    if *shown == case {
                        value.set(data);
                        return Ok(());
                    }
                }
                // The server marks the case it rendered just after `start`;
                // the branch's own range runs from there to `end`.
                let server = if std::mem::take(&mut hydrating) {
                    let marker = target
                        .start
                        .next_sibling()
                        .ok_or_else(|| JsValue::from_str("missing server branch marker"))?;
                    if marker.node_value().as_deref() != Some(&format!("fusor:branch:{case}")) {
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
                let child = hydration::with_range(server.clone(), || {
                    prepare(case, value.clone(), &parent)
                })?;
                let adopted = server.is_some();
                current.replace((case, value), child, |child| {
                    if adopted {
                        Ok(())
                    } else {
                        child.attach_fragment(&target)
                    }
                })?;
                if let Some(server) = server {
                    server
                        .start
                        .parent_node()
                        .expect("server branch marker follows an anchor")
                        .remove_child(&server.start)?;
                }
                Ok(())
            })
        })
    }
}

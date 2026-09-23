//! Retained child instances. Construction runs once per identity, untracked.
use super::{ElementTarget, JsValue, Scope, TemplateComponent};
use crate::{OwnerHandle, untrack};
use std::cell::Cell;
use web_sys::Node;

thread_local! {
    static MOUNT_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Guards recursive generated mounts, including cycles across HTML modules.
#[doc(hidden)]
pub struct MountGuard;

impl MountGuard {
    pub fn enter() -> Result<Self, JsValue> {
        MOUNT_DEPTH.with(|depth| {
            if depth.get() >= 128 {
                return Err(JsValue::from_str(
                    "fusor: component nesting exceeds 128; check for recursive component tags",
                ));
            }
            depth.set(depth.get() + 1);
            Ok(Self)
        })
    }
}

impl Drop for MountGuard {
    fn drop(&mut self) {
        MOUNT_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

impl Scope {
    /// Append an insertion range to a container. This scope owns the anchors;
    /// callers retain and dispose any child views mounted inside the range.
    pub fn mount_point(&mut self, container: &web_sys::Element) -> Result<MountPoint, JsValue> {
        let document = super::document()?;
        let start: Node = document.create_comment("fusor:mount").into();
        let end: Node = document.create_comment("fusor:end").into();
        container.append_child(&start)?;
        if let Err(error) = container.append_child(&end) {
            let _ = container.remove_child(&start);
            return Err(error);
        }
        let point = MountPoint { start, end };
        struct Anchors(MountPoint);
        impl Drop for Anchors {
            fn drop(&mut self) {
                for node in [&self.0.start, &self.0.end] {
                    if let Some(parent) = node.parent_node() {
                        let _ = parent.remove_child(node);
                    }
                }
            }
        }
        self.retain(Anchors(point.clone()));
        Ok(point)
    }

    /// Attach a prepared native-root or fragment view without activating it.
    /// Retain this scope and finish preparation before committing the view.
    pub fn attach_at(&mut self, target: &MountPoint) -> Result<(), JsValue> {
        target.validate()?;
        if self.fragment.is_some() {
            self.attach_fragment(target)
        } else {
            target.attach(self)
        }
    }

    /// Own one template child in an empty host. An unchanged identity retains
    /// DOM and local state. `None` disposes the child. Construction and mounting
    /// are untracked; pass signals as inputs to update a retained instance.
    /// Failed replacements leave the previous instance alive.
    pub fn component<C, K>(
        &mut self,
        target: impl ElementTarget,
        identity: impl Fn() -> Option<K> + 'static,
        make: impl Fn() -> Result<C, JsValue> + 'static,
    ) -> Result<(), JsValue>
    where
        C: TemplateComponent,
        K: PartialEq + 'static,
    {
        self.component_with(target, identity, move |_| make())
    }

    /// Owner-aware variant of [`Self::component`]. The factory runs after the
    /// child's HTML descriptor has been validated and before its work starts.
    pub fn component_with<C, K>(
        &mut self,
        target: impl ElementTarget,
        identity: impl Fn() -> Option<K> + 'static,
        make: impl Fn(OwnerHandle) -> Result<C, JsValue> + 'static,
    ) -> Result<(), JsValue>
    where
        C: TemplateComponent,
        K: PartialEq + 'static,
    {
        let container = target.resolve(self)?;
        let parent = self.owner();
        let mut current: Option<(K, Scope)> = None;
        let mut hydrating = self.is_hydrating();
        self.bind(move || {
            let next = identity();
            untrack(|| {
                let initial = std::mem::take(&mut hydrating);
                if initial && container.child_element_count() != u32::from(next.is_some()) {
                    return Err(JsValue::from_str("server child identity/shape mismatch"));
                }
                let Some(key) = next else {
                    current.take();
                    return Ok(());
                };
                if current
                    .as_ref()
                    .is_some_and(|(previous, _)| previous == &key)
                {
                    return Ok(());
                }
                #[cfg(feature = "islands")]
                let mut child = if initial {
                    super::delivery::with_root(
                        &container.first_element_child().expect("validated child"),
                        || C::prepare(&parent, &make),
                    )?
                } else {
                    C::prepare(&parent, &make)?
                };
                #[cfg(not(feature = "islands"))]
                let mut child = C::prepare(&parent, &make)?;
                if !initial {
                    child.attach(&container)?;
                }
                child.remove_on_drop = true;
                child.finish_prepare()?;
                current = Some((key, child));
                current.as_ref().expect("just inserted").1.commit();
                Ok(())
            })
        })
    }
}

/// An owned sibling range, created by a compiled template or Scope::mount_point.
/// The anchors remain stable across view replacements.
#[derive(Clone)]
pub struct MountPoint {
    pub(super) start: Node,
    pub(super) end: Node,
}

impl MountPoint {
    pub(super) fn validate(&self) -> Result<(), JsValue> {
        let parent = self
            .start
            .parent_node()
            .ok_or_else(|| JsValue::from_str("detached component anchor"))?;
        if !self
            .end
            .parent_node()
            .is_some_and(|node| node.is_same_node(Some(&parent)))
        {
            return Err(JsValue::from_str(
                "component anchors have different parents",
            ));
        }
        let mut cursor = self.start.next_sibling();
        while let Some(node) = cursor {
            if node.is_same_node(Some(&self.end)) {
                return Ok(());
            }
            cursor = node.next_sibling();
        }
        Err(JsValue::from_str("component anchors are out of order"))
    }

    pub(super) fn hydrated_root(&self) -> Result<Option<web_sys::Element>, JsValue> {
        self.validate()?;
        let next = self.start.next_sibling().expect("validated anchors");
        if next.is_same_node(Some(&self.end)) {
            return Ok(None);
        }
        if next.node_type() != Node::ELEMENT_NODE
            || !next
                .next_sibling()
                .is_some_and(|node| node.is_same_node(Some(&self.end)))
        {
            return Err(JsValue::from_str(
                "server component identity/shape mismatch",
            ));
        }
        use wasm_bindgen::JsCast;
        Ok(Some(next.unchecked_into()))
    }
    fn attach(&self, child: &mut Scope) -> Result<(), JsValue> {
        let parent = self
            .end
            .parent_node()
            .ok_or_else(|| JsValue::from_str("detached component anchor"))?;
        if !self
            .start
            .parent_node()
            .is_some_and(|node| node.is_same_node(Some(&parent)))
        {
            return Err(JsValue::from_str(
                "component anchors have different parents",
            ));
        }
        parent.insert_before(child.root(), Some(&self.end))?;
        child.remove_on_drop = true;
        Ok(())
    }
}

impl Scope {
    /// Compiler entry point for a wrapper-free retained component. Preparation
    /// and fallible browser setup complete before the previous scope is dropped.
    #[doc(hidden)]
    pub fn component_at<C, K>(
        &mut self,
        target: &MountPoint,
        identity: impl Fn() -> Option<K> + 'static,
        make: impl Fn(OwnerHandle) -> Result<C, JsValue> + 'static,
    ) -> Result<(), JsValue>
    where
        C: TemplateComponent,
        K: PartialEq + 'static,
    {
        self.component_at_with_children(target, identity, make, super::Children::default())
    }

    #[doc(hidden)]
    pub fn component_at_with_children<C, K>(
        &mut self,
        target: &MountPoint,
        identity: impl Fn() -> Option<K> + 'static,
        make: impl Fn(OwnerHandle) -> Result<C, JsValue> + 'static,
        children: super::Children,
    ) -> Result<(), JsValue>
    where
        C: TemplateComponent,
        K: PartialEq + 'static,
    {
        let mut hydrating = self.is_hydrating();
        let target = target.clone();
        let parent = self.owner();
        let mut current: Option<(K, Scope)> = None;
        self.bind(move || {
            let next = identity();
            untrack(|| {
                let initial = std::mem::take(&mut hydrating);
                let root = if initial {
                    target.hydrated_root()?
                } else {
                    None
                };
                if initial && root.is_some() != next.is_some() {
                    return Err(JsValue::from_str(
                        "server component identity/shape mismatch",
                    ));
                }
                let Some(key) = next else {
                    current.take();
                    return Ok(());
                };
                if current
                    .as_ref()
                    .is_some_and(|(previous, _)| previous == &key)
                {
                    return Ok(());
                }
                #[cfg(feature = "islands")]
                let mut child = if let Some(root) = &root {
                    super::delivery::with_root(root, || {
                        children.with(|| C::prepare(&parent, &make))
                    })?
                } else {
                    children.with(|| C::prepare(&parent, &make))?
                };
                #[cfg(not(feature = "islands"))]
                let mut child = children.with(|| C::prepare(&parent, &make))?;
                if !initial {
                    target.attach(&mut child)?;
                }
                child.remove_on_drop = true;
                child.finish_prepare()?;
                current = Some((key, child));
                current.as_ref().expect("just inserted").1.commit();
                Ok(())
            })
        })
    }
}

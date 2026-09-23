//! Compiler-owned child fragments. Factories capture lexical state; placement
//! supplies the lifetime. The temporary staging element is never mounted.
use super::{JsValue, MountPoint, Scope};
use crate::OwnerHandle;
use std::{cell::RefCell, rc::Rc};
use web_sys::Node;

#[derive(Clone, Default)]
#[doc(hidden)]
pub struct Children(Option<Rc<Factory>>);
type Factory = dyn Fn(&OwnerHandle) -> Result<Scope, JsValue>;
thread_local! {
    static INCOMING: RefCell<Children> = RefCell::new(Children::default());
    static HYDRATING: RefCell<Option<MountPoint>> = const { RefCell::new(None) };
}

impl Children {
    pub fn new(make: impl Fn(&OwnerHandle) -> Result<Scope, JsValue> + 'static) -> Self {
        Self(Some(Rc::new(make)))
    }
    pub fn take() -> Self {
        INCOMING.with(|value| std::mem::take(&mut *value.borrow_mut()))
    }
    pub fn with<R>(&self, run: impl FnOnce() -> R) -> R {
        struct Restore(Children);
        impl Drop for Restore {
            fn drop(&mut self) {
                INCOMING.with(|value| *value.borrow_mut() = self.0.clone());
            }
        }
        let _restore = Restore(INCOMING.with(|value| value.replace(self.clone())));
        run()
    }
    pub(super) fn prepare(&self, parent: &OwnerHandle) -> Result<Option<Scope>, JsValue> {
        self.0.as_ref().map(|make| make(parent)).transpose()
    }
}

pub(super) fn take_hydration() -> Option<MountPoint> {
    HYDRATING.with(|value| value.take())
}
pub(super) fn with_hydration<R>(point: Option<MountPoint>, run: impl FnOnce() -> R) -> R {
    struct Restore(Option<MountPoint>);
    impl Drop for Restore {
        fn drop(&mut self) {
            HYDRATING.with(|value| *value.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(HYDRATING.with(|value| value.replace(point)));
    run()
}

impl Scope {
    #[doc(hidden)]
    pub fn children_at(&mut self, target: &MountPoint, children: &Children) -> Result<(), JsValue> {
        let child = with_hydration(self.is_hydrating().then(|| target.clone()), || {
            children.prepare(&self.owner())
        })?;
        if let Some(child) = child {
            if !self.is_hydrating() {
                child.attach_fragment(target)?;
            }
            child.finish_prepare()?;
            child.try_commit()?;
            self.children.push(child);
        } else if self.is_hydrating() && !target.is_empty()? {
            return Err(JsValue::from_str(
                "server children differ from browser children",
            ));
        }
        Ok(())
    }
    pub(super) fn attach_fragment(&self, target: &MountPoint) -> Result<(), JsValue> {
        target.validate()?;
        let parent = target.end.parent_node().expect("validated target");
        let fragment = self
            .fragment
            .as_ref()
            .ok_or_else(|| JsValue::from_str("expected a children fragment"))?;
        for node in fragment.nodes()? {
            parent.insert_before(&node, Some(&target.end))?;
        }
        Ok(())
    }
}

impl MountPoint {
    /// Native container of this owned insertion range.
    pub fn parent_element(&self) -> Result<web_sys::Element, JsValue> {
        self.validate()?;
        self.end
            .parent_element()
            .ok_or_else(|| JsValue::from_str("mount range has no element parent"))
    }

    pub(super) fn is_empty(&self) -> Result<bool, JsValue> {
        self.validate()?;
        Ok(self
            .start
            .next_sibling()
            .is_some_and(|node| node.is_same_node(Some(&self.end))))
    }
    pub(super) fn nodes(&self) -> Result<Vec<Node>, JsValue> {
        self.validate()?;
        let mut result = Vec::new();
        let mut current = Some(self.start.clone());
        while let Some(node) = current {
            let end = node.is_same_node(Some(&self.end));
            current = node.next_sibling();
            result.push(node);
            if end {
                break;
            }
        }
        Ok(result)
    }
    pub(super) fn contains(&self, target: &Node) -> bool {
        self.nodes()
            .is_ok_and(|nodes| nodes.iter().any(|node| node.contains(Some(target))))
    }
    pub(super) fn remove(&self) {
        if let Ok(nodes) = self.nodes() {
            for node in nodes {
                #[cfg(feature = "islands")]
                {
                    use wasm_bindgen::JsCast;
                    if let Some(element) = node.dyn_ref::<web_sys::Element>() {
                        super::delivery::dispose_tree(element);
                    }
                }
                if let Some(parent) = node.parent_node() {
                    let _ = parent.remove_child(&node);
                }
            }
        }
    }
}

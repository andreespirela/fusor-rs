//! Compiler-owned child fragments. Factories capture lexical state; placement
//! supplies the lifetime. The temporary staging element is never mounted.
use super::{JsValue, MountPoint, Scope, hydration, scoped};
use crate::OwnerHandle;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Default)]
#[doc(hidden)]
pub struct Children(Option<Rc<Factory>>);
type Factory = dyn Fn(&OwnerHandle) -> Result<Scope, JsValue>;
thread_local! {
    static INCOMING: RefCell<Children> = RefCell::new(Children::default());
}

impl Children {
    pub fn new(make: impl Fn(&OwnerHandle) -> Result<Scope, JsValue> + 'static) -> Self {
        Self(Some(Rc::new(make)))
    }
    /// Generated prepare takes the children its component tag supplied.
    pub fn take() -> Self {
        INCOMING.take()
    }
    /// Supply these children to the component that `run` prepares next.
    pub fn with<R>(&self, run: impl FnOnce() -> R) -> R {
        scoped(&INCOMING, self.clone(), run)
    }
    pub(super) fn prepare(&self, parent: &OwnerHandle) -> Result<Option<Scope>, JsValue> {
        self.0.as_ref().map(|make| make(parent)).transpose()
    }
}

impl Scope {
    #[doc(hidden)]
    pub fn children_at(&mut self, target: &MountPoint, children: &Children) -> Result<(), JsValue> {
        let child = hydration::with_range(self.is_hydrating().then(|| target.clone()), || {
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
}

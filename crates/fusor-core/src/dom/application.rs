//! The single managed application root. Application state remains an ordinary
//! Rust value; this adapter retains its scope and owns startup/teardown.
use super::{Component, JsValue, Scope};
use crate::OwnerHandle;
use std::{cell::RefCell, rc::Rc};

#[derive(Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Starting,
    Running,
    Stopping,
}
#[derive(Default)]
struct Application {
    phase: Phase,
    scope: Option<Rc<Scope>>,
}
thread_local! { static APP: RefCell<Application> = RefCell::new(Application::default()); }

struct Transition(bool);
impl Drop for Transition {
    fn drop(&mut self) {
        if self.0 {
            let scope = APP.with(|app| app.borrow_mut().scope.take());
            drop(scope);
            APP.with(|app| app.borrow_mut().phase = Phase::Idle);
        }
    }
}

/// Start a developer-defined root, retaining it before owned work activates.
/// The typed low-level counterpart of generated `<App>` startup. Duplicate/reentrant starts fail.
pub fn mount<C: Component>(
    make: impl FnOnce(OwnerHandle) -> Result<C, JsValue>,
) -> Result<(), JsValue> {
    mount_scope(|| C::prepare_component(None, Box::new(make)))
}

/// Retain and activate a compiler-prepared application scope. The preparation
/// closure runs only after the single-root startup guard has been acquired.
#[doc(hidden)]
pub fn mount_scope(prepare: impl FnOnce() -> Result<Scope, JsValue>) -> Result<(), JsValue> {
    APP.with(|app| {
        let mut app = app.borrow_mut();
        if app.phase != Phase::Idle {
            return Err(JsValue::from_str(
                "fusor: application already mounted or changing lifecycle state",
            ));
        }
        app.phase = Phase::Starting;
        Ok(())
    })?;
    let mut transition = Transition(true);
    let scope = Rc::new(prepare()?);
    APP.with(|app| app.borrow_mut().scope = Some(scope.clone()));
    scope.try_commit()?;
    APP.with(|app| app.borrow_mut().phase = Phase::Running);
    transition.0 = false;
    Ok(())
}

/// Weak access to the retained root, useful to embedding and test adapters.
pub fn owner() -> Option<OwnerHandle> {
    APP.with(|app| app.borrow().scope.as_ref().map(|scope| scope.owner()))
}

/// Dispose the managed application. Idle teardown is a no-op; teardown during
/// startup or cleanup is rejected. Callbacks run outside the application borrow.
pub fn unmount() -> Result<(), JsValue> {
    let scope = APP.with(|app| {
        let mut app = app.borrow_mut();
        match app.phase {
            Phase::Idle => Ok(None),
            Phase::Running => {
                app.phase = Phase::Stopping;
                Ok(app.scope.take())
            }
            _ => Err(JsValue::from_str("fusor: reentrant application teardown")),
        }
    })?;
    let _transition = Transition(true);
    drop(scope);
    Ok(())
}

#![cfg(feature = "actions")]
use fusor::{Owner, effect};
use fusor_std::actions::{Action, AdmissionError, Outcome, SavePolicy, Status};
use fusor_test::{ControlledLoader, TestExecutor};
use std::{cell::Cell, rc::Rc};

#[test]
fn direct_actions_reject_busy_and_effect_can_dispatch_on_completion() {
    let owner = Owner::new();
    let executor = TestExecutor::new();
    let backend = ControlledLoader::<Rc<String>, Outcome<(), ()>, ()>::new();
    let loader = backend.clone();
    let action = Action::new(
        &owner.handle(),
        SavePolicy::RejectWhilePending,
        move |command, context| {
            let future = loader.load(command, context);
            async move { future.await.unwrap() }
        },
        executor.spawner(),
    );
    assert_eq!(
        action.dispatch("first".into()).unwrap_err().reason,
        AdmissionError::NotActive
    );
    owner.commit();
    let first = action.dispatch("first".into()).unwrap();
    let busy = action.dispatch("second".into()).unwrap_err();
    assert_eq!(busy.reason, AdmissionError::Busy);
    assert_eq!(busy.command, "second");
    assert_eq!(action.state().submission.unwrap().id, first);
    let dispatched = Rc::new(Cell::new(false));
    let flag = dispatched.clone();
    let captured = action.clone();
    let _effect = effect(move || {
        if captured.state().status == Status::Accepted && !flag.replace(true) {
            captured.dispatch("next".into()).unwrap();
        }
    });
    executor.run_until_stalled();
    backend
        .next_request()
        .unwrap()
        .complete(Ok(Outcome::Accepted(())))
        .unwrap();
    executor.run_until_stalled();
    assert!(dispatched.get());
    assert!(action.pending());
    let next = backend.next_request().unwrap();
    assert_eq!(*next.key, "next");
    assert!(!next.is_cancelled());
    next.complete(Ok(Outcome::Unknown(()))).unwrap();
    executor.run_until_stalled();
    assert_eq!(
        action.dispatch("retry".into()).unwrap_err().reason,
        AdmissionError::Unresolved
    );
    action.reconcile().unwrap();
    assert_eq!(action.state().status, Status::Idle);
}

#[test]
fn disposing_from_pending_notification_never_starts_transport() {
    let owner = Rc::new(Owner::new());
    owner.commit();
    let executor = TestExecutor::new();
    let started = Rc::new(Cell::new(false));
    let flag = started.clone();
    let action = Action::new(
        &owner.handle(),
        SavePolicy::RejectWhilePending,
        move |_: Rc<()>, _| {
            flag.set(true);
            async { Outcome::<(), ()>::Accepted(()) }
        },
        executor.spawner(),
    );
    let captured = action.clone();
    let parent = owner.clone();
    let _effect = effect(move || {
        if captured.pending() {
            parent.dispose();
        }
    });
    action.dispatch(()).unwrap();
    executor.run_until_stalled();
    assert!(!started.get());
    assert_eq!(action.state().status, Status::Disposed);
}

#[test]
fn cancellation_source_completion_does_not_signal_abort() {
    use fusor_std::actions::RequestContext;
    // The shared cancellation context is compatible with native read adapters.
    fn context(_: RequestContext) {}
    let source = fusor_async::CancellationSource::default();
    let finished = source.context();
    context(finished.clone());
    source.complete();
    assert!(!finished.is_cancelled());
    let source = fusor_async::CancellationSource::default();
    let cancelled = source.context();
    drop(source);
    assert!(cancelled.is_cancelled());
}

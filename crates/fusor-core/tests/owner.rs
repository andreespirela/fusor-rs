use fusor::Owner;
use std::{cell::Cell, rc::Rc};

#[test]
fn ancestors_gate_activation_and_handles_do_not_keep_owners_alive() {
    let parent = Owner::new();
    let child = Owner::child(&parent.handle());
    let count = Rc::new(Cell::new(0));
    let captured = count.clone();
    let _start = child
        .handle()
        .on_activate(move || captured.set(captured.get() + 1));
    child.commit();
    assert!(!child.handle().is_active());
    assert_eq!(count.get(), 0);
    parent.commit();
    parent.commit();
    child.commit();
    assert_eq!(count.get(), 1);
    let handle = parent.handle();
    drop(parent);
    assert!(handle.is_disposed());
    assert!(child.handle().is_disposed());
}

#[test]
fn entire_subtree_is_invalid_before_cleanup_and_callbacks_can_register_callbacks() {
    let parent = Owner::new();
    let child = Owner::child(&parent.handle());
    let child_handle = child.handle();
    let parent_handle = parent.handle();
    let count = Rc::new(Cell::new(0));
    let captured = count.clone();
    let _cleanup = parent.handle().on_cleanup(move || {
        assert!(child_handle.is_disposed());
        let _late = parent_handle.on_cleanup(move || captured.set(2));
    });
    parent.dispose();
    parent.dispose();
    assert_eq!(count.get(), 2);
}

#[test]
fn dropped_registrations_and_failed_preparations_do_not_start_work() {
    let owner = Owner::new();
    let count = Rc::new(Cell::new(0));
    let captured = count.clone();
    drop(owner.handle().on_activate(move || captured.set(100)));
    let captured = count.clone();
    let _cleanup = owner.handle().on_cleanup(move || captured.set(1));
    owner.dispose();
    owner.commit();
    assert_eq!(count.get(), 1);
    assert!(Owner::child(&owner.handle()).handle().is_disposed());
}

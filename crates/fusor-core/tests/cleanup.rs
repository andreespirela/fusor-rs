use fusor::{Cleanup, CleanupEffect, Owner, effect_with_cleanup, signal};
use std::{cell::RefCell, rc::Rc};

#[test]
fn cleanup_precedes_setup_and_its_reads_do_not_subscribe() {
    let input = signal(0);
    let unrelated = signal(0);
    let events = Rc::new(RefCell::new(Vec::new()));
    let effect = effect_with_cleanup({
        let input = input.clone();
        let unrelated = unrelated.clone();
        let events = events.clone();
        move || {
            let value = input.get();
            events.borrow_mut().push(("setup", value));
            let events = events.clone();
            let unrelated = unrelated.clone();
            Cleanup::new(move || {
                unrelated.get();
                events.borrow_mut().push(("cleanup", value));
            })
        }
    });
    input.set(1);
    unrelated.set(1);
    effect.dispose();
    effect.dispose();
    drop(effect);
    assert_eq!(
        *events.borrow(),
        [("setup", 0), ("cleanup", 0), ("setup", 1), ("cleanup", 1)]
    );
}

#[test]
fn disposing_during_setup_releases_the_new_guard() {
    let stored: Rc<RefCell<Option<CleanupEffect>>> = Rc::new(RefCell::new(None));
    let input = signal(false);
    let drops = signal(0);
    let effect = effect_with_cleanup({
        let input = input.clone();
        let stored = stored.clone();
        let drops = drops.clone();
        move || {
            if input.get() {
                stored.borrow().as_ref().unwrap().dispose();
            }
            let drops = drops.clone();
            Cleanup::new(move || drops.update(|n| *n += 1))
        }
    });
    *stored.borrow_mut() = Some(effect);
    input.set(true);
    assert_eq!(drops.get(), 2);
    input.set(false);
    assert_eq!(drops.get(), 2);
    stored.borrow_mut().take();
}

#[test]
fn guarded_callbacks_are_inactive_before_commit_and_after_disposal() {
    let owner = Owner::new();
    let input = signal(0);
    let mut callback = owner.handle().guarded({
        let input = input.clone();
        move |n| input.set(n)
    });
    assert!(callback(1).is_none());
    owner.commit();
    assert!(callback(2).is_some());
    owner.dispose();
    assert!(callback(3).is_none());
    assert_eq!(input.get(), 2);
}

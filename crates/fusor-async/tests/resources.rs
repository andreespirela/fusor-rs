use fusor::{Owner, effect, signal};
use fusor_async::{Resource, ResourceState};
use futures_channel::oneshot;
use futures_executor::LocalPool;
use futures_util::{future::LocalBoxFuture, task::LocalSpawnExt};
use std::{
    cell::{Cell, RefCell},
    future::{Future, pending},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

fn spawner(pool: &LocalPool) -> impl Fn(LocalBoxFuture<'static, ()>) + 'static {
    let spawn = pool.spawner();
    move |future| spawn.spawn_local(future).unwrap()
}

#[test]
fn starts_only_after_ancestors_commit_and_deduplicates_equal_keys() {
    let mut pool = LocalPool::new();
    let parent = Owner::new();
    let child = Owner::child(&parent.handle());
    let key = signal(1);
    let calls = Rc::new(Cell::new(0));
    let r = Resource::new(
        &child.handle(),
        {
            let key = key.clone();
            move || Some(key.get())
        },
        {
            let calls = calls.clone();
            move |key, _| {
                calls.set(calls.get() + 1);
                async move { Ok::<_, ()>(key * 2) }
            }
        },
        spawner(&pool),
    );
    child.commit();
    pool.run_until_stalled();
    assert_eq!(calls.get(), 0);
    key.set(2);
    parent.commit();
    pool.run_until_stalled();
    assert_eq!(calls.get(), 1);
    assert_eq!(*r.get().data().unwrap().value, 4);
    key.update(|_| {});
    pool.run_until_stalled();
    assert_eq!(calls.get(), 1);
    r.refresh();
    pool.run_until_stalled();
    assert_eq!(calls.get(), 2);
}

#[test]
fn changed_keys_abort_old_reads_and_previous_data_keeps_its_original_key() {
    let mut pool = LocalPool::new();
    let owner = Owner::new();
    owner.commit();
    let key = signal(Some(1));
    let pending = Rc::new(RefCell::new(Vec::new()));
    let cancelled = Rc::new(Cell::new(0));
    let r = Resource::new(
        &owner.handle(),
        {
            let key = key.clone();
            move || key.get()
        },
        {
            let pending = pending.clone();
            let cancelled = cancelled.clone();
            move |key, context| {
                let (tx, rx) = oneshot::channel::<Result<i32, &'static str>>();
                pending.borrow_mut().push((key, tx));
                let cancelled = cancelled.clone();
                let guard = context.on_cancel(move || cancelled.set(cancelled.get() + 1));
                async move {
                    let _guard = guard;
                    rx.await.unwrap()
                }
            }
        },
        spawner(&pool),
    );
    pool.run_until_stalled();
    pending.borrow_mut().remove(0).1.send(Ok(10)).unwrap();
    pool.run_until_stalled();
    key.set(Some(2));
    pool.run_until_stalled();
    let old = pending.borrow_mut().remove(0).1;
    assert!(
        matches!(r.get(), ResourceState::Loading { key: 2, previous: Some(data) } if data.key == 1)
    );
    key.set(Some(3));
    assert_eq!(cancelled.get(), 1); // transport cancellation is synchronous
    old.send(Ok(20)).unwrap(); // a response already queued still cannot publish
    pool.run_until_stalled();
    pending
        .borrow_mut()
        .remove(0)
        .1
        .send(Err("offline"))
        .unwrap();
    pool.run_until_stalled();
    assert!(
        matches!(r.get(), ResourceState::Error { key: 3, previous: Some(data), .. } if data.key == 1)
    );
    key.set(None);
    pool.run_until_stalled();
    assert!(matches!(r.get(), ResourceState::Idle));
}

#[test]
fn future_that_changes_its_key_during_final_poll_cannot_publish_stale_output() {
    let mut pool = LocalPool::new();
    let owner = Owner::new();
    owner.commit();
    let key = signal(1);
    let r = Resource::new(
        &owner.handle(),
        {
            let key = key.clone();
            move || Some(key.get())
        },
        {
            let key = key.clone();
            move |value, _| {
                let key = key.clone();
                async move {
                    if value == 1 {
                        key.set(2);
                    }
                    Ok::<_, ()>(value)
                }
            }
        },
        spawner(&pool),
    );
    let observed = Rc::new(RefCell::new(Vec::new()));
    let _watch = effect({
        let r = r.clone();
        let observed = observed.clone();
        move || {
            if let ResourceState::Ready(data) = r.get() {
                observed.borrow_mut().push(*data.value);
            }
        }
    });
    pool.run_until_stalled();
    assert_eq!(*observed.borrow(), vec![2]);
}

struct DropFuture(Rc<Cell<usize>>);
impl Future for DropFuture {
    type Output = Result<(), ()>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for DropFuture {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn disposal_is_terminal_and_releases_future_captures_on_next_poll() {
    let mut pool = LocalPool::new();
    let owner = Owner::new();
    owner.commit();
    let drops = Rc::new(Cell::new(0));
    let r = Resource::new(
        &owner.handle(),
        || Some(()),
        {
            let drops = drops.clone();
            move |_, _| DropFuture(drops.clone())
        },
        spawner(&pool),
    );
    pool.run_until_stalled();
    drop(owner);
    assert!(matches!(r.get(), ResourceState::Disposed));
    r.refresh();
    assert_eq!(drops.get(), 0);
    pool.run_until_stalled();
    assert_eq!(drops.get(), 1);
}

#[test]
fn last_handle_drop_cancels_even_when_owner_survives_and_failed_mount_never_calls_loader() {
    let mut pool = LocalPool::new();
    let owner = Owner::new();
    let calls = Rc::new(Cell::new(0));
    let r = Resource::new(
        &owner.handle(),
        || Some(()),
        {
            let calls = calls.clone();
            move |_, _| {
                calls.set(calls.get() + 1);
                pending::<Result<(), ()>>()
            }
        },
        spawner(&pool),
    );
    drop(owner);
    pool.run_until_stalled();
    assert_eq!(calls.get(), 0);
    assert!(matches!(r.get(), ResourceState::Disposed));
    let owner = Owner::new();
    owner.commit();
    let drops = Rc::new(Cell::new(0));
    let r = Resource::new(
        &owner.handle(),
        || Some(()),
        {
            let drops = drops.clone();
            move |_, _| DropFuture(drops.clone())
        },
        spawner(&pool),
    );
    pool.run_until_stalled();
    drop(r);
    pool.run_until_stalled();
    assert_eq!(drops.get(), 1);
}

#[test]
fn loaders_are_untracked_and_data_errors_need_not_be_clone() {
    struct Value;
    struct Error;
    let mut pool = LocalPool::new();
    let owner = Owner::new();
    owner.commit();
    let unrelated = signal(0);
    let calls = Rc::new(Cell::new(0));
    let r = Resource::new(
        &owner.handle(),
        || Some(()),
        {
            let unrelated = unrelated.clone();
            let calls = calls.clone();
            move |_, _| {
                unrelated.get();
                calls.set(calls.get() + 1);
                async { Ok::<_, Error>(Value) }
            }
        },
        spawner(&pool),
    );
    pool.run_until_stalled();
    unrelated.set(1);
    pool.run_until_stalled();
    assert_eq!(calls.get(), 1);
    let _snapshot = r.get();
}

#[test]
fn cancellation_callback_can_dispose_resource_without_reentrant_borrows() {
    let mut pool = LocalPool::new();
    let owner = Owner::new();
    owner.commit();
    let resource = Rc::new(RefCell::new(None::<Resource<u32, (), ()>>));
    let weak = Rc::downgrade(&resource);
    let r = Resource::new(
        &owner.handle(),
        || Some(1),
        move |_, context| {
            let weak = weak.clone();
            let guard = context.on_cancel(move || {
                if let Some(slot) = weak.upgrade() {
                    slot.borrow().as_ref().unwrap().dispose();
                }
            });
            async move {
                let _guard = guard;
                pending().await
            }
        },
        spawner(&pool),
    );
    *resource.borrow_mut() = Some(r.clone());
    pool.run_until_stalled();
    r.refresh();
    pool.run_until_stalled();
    assert!(matches!(r.get(), ResourceState::Disposed));
}

#[test]
fn retired_resource_payloads_observe_complete_transitions() {
    struct Payload(&'static str, Box<dyn Fn(&'static str)>);
    impl Drop for Payload {
        fn drop(&mut self) {
            (self.1)(self.0);
        }
    }
    let mut pool = LocalPool::new();
    let owner = Owner::new();
    owner.commit();
    let key = signal(Some(1));
    let slot = Rc::new(RefCell::new(None::<Resource<i32, Payload, Payload>>));
    let log = Rc::new(RefCell::new(Vec::new()));
    let payload = |name| {
        let slot = Rc::downgrade(&slot);
        let log = log.clone();
        Payload(
            name,
            Box::new(move |name| {
                if let Some(slot) = slot.upgrade() {
                    slot.borrow().as_ref().unwrap().with(|state| {
                        let phase = match state {
                            ResourceState::Idle => "idle",
                            ResourceState::Loading { .. } => "loading",
                            ResourceState::Ready(_) => "ready",
                            ResourceState::Error { .. } => "error",
                            ResourceState::Disposed => "disposed",
                        };
                        log.borrow_mut().push((name, phase));
                    });
                }
            }),
        )
    };
    let requests = Rc::new(RefCell::new(Vec::new()));
    let r = Resource::new(
        &owner.handle(),
        {
            let key = key.clone();
            move || key.get()
        },
        {
            let requests = requests.clone();
            move |_, _| {
                let (tx, rx) = oneshot::channel();
                requests.borrow_mut().push(tx);
                async move { rx.await.unwrap() }
            }
        },
        spawner(&pool),
    );
    *slot.borrow_mut() = Some(r.clone());
    let mut complete = |result| {
        pool.run_until_stalled();
        assert!(requests.borrow_mut().remove(0).send(result).is_ok());
        pool.run_until_stalled();
    };
    complete(Ok(payload("first")));
    key.set(Some(2));
    complete(Err(payload("failure")));
    r.refresh();
    assert_eq!(*log.borrow(), [("failure", "loading")]);
    complete(Ok(payload("second")));
    assert_eq!(log.borrow().last(), Some(&("first", "ready")));
    key.set(None);
    assert_eq!(log.borrow().last(), Some(&("second", "idle")));
    key.set(Some(3));
    complete(Ok(payload("third")));
    r.dispose();
    assert_eq!(log.borrow().last(), Some(&("third", "disposed")));
}

#[test]
fn retired_key_can_refresh_after_the_transition_releases_its_borrow() {
    #[derive(Clone)]
    struct Key(i32, Rc<dyn Fn()>);
    impl PartialEq for Key {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl Drop for Key {
        fn drop(&mut self) {
            (self.1)();
        }
    }
    let mut pool = LocalPool::new();
    let owner = Owner::new();
    owner.commit();
    let slot = Rc::new(RefCell::new(None::<Resource<Key, (), ()>>));
    let armed = Rc::new(Cell::new(false));
    let read = Rc::new({
        let slot = Rc::downgrade(&slot);
        let armed = armed.clone();
        move || {
            if armed.replace(false) {
                let slot = slot.upgrade().unwrap();
                let resource = slot.borrow();
                let resource = resource.as_ref().unwrap();
                resource.with(|state| assert!(state.is_loading()));
                resource.refresh();
            }
        }
    });
    let key = signal(1);
    let resource = Resource::new(
        &owner.handle(),
        {
            let key = key.clone();
            move || Some(Key(key.get(), read.clone()))
        },
        |_, _| pending(),
        spawner(&pool),
    );
    *slot.borrow_mut() = Some(resource);
    pool.run_until_stalled();
    armed.set(true);
    key.set(2);
    assert!(!armed.get());
    pool.run_until_stalled();
}

#[test]
fn completed_reads_never_cancel_their_token() {
    let mut pool = LocalPool::new();
    let owner = Owner::new();
    owner.commit();
    let key = signal(1);
    let cancelled = Rc::new(Cell::new(0));
    // Keep every registration alive so a late cancellation would be observed.
    let registrations = Rc::new(RefCell::new(Vec::new()));
    let r = Resource::new(
        &owner.handle(),
        {
            let key = key.clone();
            move || Some(key.get())
        },
        {
            let cancelled = cancelled.clone();
            let registrations = registrations.clone();
            move |key, cancel| {
                let cancelled = cancelled.clone();
                let registration = cancel.on_cancel(move || cancelled.set(cancelled.get() + 1));
                registrations.borrow_mut().push(registration);
                async move {
                    if key == 3 {
                        pending::<()>().await;
                    }
                    Ok::<_, ()>(key)
                }
            }
        },
        spawner(&pool),
    );
    pool.run_until_stalled();
    assert_eq!(*r.get().data().unwrap().value, 1);
    r.refresh();
    pool.run_until_stalled();
    key.set(2);
    pool.run_until_stalled();
    assert_eq!(*r.get().data().unwrap().value, 2);
    assert_eq!(cancelled.get(), 0);
    // A pending read is still cancelled, so the counter does observe cancellation.
    key.set(3);
    pool.run_until_stalled();
    r.refresh();
    assert_eq!(cancelled.get(), 1);
    pool.run_until_stalled();
    r.dispose();
    assert_eq!(cancelled.get(), 2);
    assert_eq!(registrations.borrow().len(), 5);
}

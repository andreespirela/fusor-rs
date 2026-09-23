use fusor::{Owner, signal};
use fusor_async::{Resource, ResourceState};
use fusor_test::{ControlledLoader, OwnerProbe, TestExecutor};

#[test]
fn application_controls_completion_and_disposal_without_sleeps() {
    let owner = Owner::new();
    let probe = OwnerProbe::new(&owner.handle());
    let executor = TestExecutor::new();
    let loader = ControlledLoader::<u32, String, ()>::new();
    let selected = signal(Some(1));
    let resource = Resource::new(
        &owner.handle(),
        {
            let selected = selected.clone();
            move || selected.get()
        },
        {
            let loader = loader.clone();
            move |key, request| loader.load(key, request)
        },
        executor.spawner(),
    );
    executor.run_until_stalled();
    assert!(!probe.is_active());
    assert_eq!(loader.counts().started, 0);
    owner.commit();
    executor.run_until_stalled();
    let first = loader.next_request().unwrap();
    selected.set(Some(2));
    executor.run_until_stalled();
    let second = loader.next_request().unwrap();
    second.complete(Ok("new".into())).unwrap();
    executor.run_until_stalled();
    assert!(first.is_cancelled());
    assert!(first.complete(Ok("late".into())).is_err());
    executor.run_until_stalled();
    assert_eq!(&*resource.get().data().unwrap().value, "new");
    assert_eq!(loader.counts().completed, 1);

    resource.refresh();
    executor.run_until_stalled();
    // A successfully queued result is distinct from a publishable result.
    loader
        .next_request()
        .unwrap()
        .complete(Ok("queued".into()))
        .unwrap();
    owner.dispose();
    executor.run_until_stalled();
    assert!(matches!(resource.get(), ResourceState::Disposed));
    assert_eq!(probe.cleanup_count(), 1);
    assert!(probe.is_disposed());
    assert_eq!(loader.counts().cancelled, 2);
    assert_eq!(loader.counts().completed, 1);
    assert_eq!(loader.counts().live, 0);
}

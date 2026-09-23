//! Creating an application, and what the read-only commands do to one that has
//! not been prepared yet.
mod support;

use std::fs;
use support::{Fixture, failure, success};

#[test]
fn scaffolding_and_read_only_commands_run_without_rust() {
    let fixture = Fixture::new();
    let help = success(&mut fixture.cli(&fixture.root, &[]));
    assert!(String::from_utf8_lossy(&help.stdout).contains("fusor new my-app"));

    let application = fixture.scaffold("starter");
    let before = fs::read(application.join("Cargo.toml")).unwrap();
    // Doctor reports problems; it never resolves a lockfile or edits anything.
    let error = failure(fixture.cli(&application, &["doctor"]).env("PATH", ""));
    assert!(error.contains("Cargo.lock is missing"), "{error}");
    assert_eq!(before, fs::read(application.join("Cargo.toml")).unwrap());
    assert!(!application.join("Cargo.lock").exists());
}

#[test]
fn a_frozen_check_refuses_to_resolve_a_missing_lockfile() {
    let fixture = Fixture::new();
    let application = fixture.scaffold("unlocked");
    let error = failure(&mut fixture.cli(&application, &["check", "--frozen"]));
    assert!(error.contains("fusor install"), "{error}");
    assert!(!application.join("Cargo.lock").exists());
}

#[test]
fn new_refuses_an_occupied_destination() {
    let fixture = Fixture::new();
    fixture.scaffold("taken");
    let error = failure(&mut fixture.cli(&fixture.root, &["new", "taken", "--skip-install"]));
    assert!(error.contains("destination already exists"), "{error}");
}

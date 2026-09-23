//! The full install/check/build lifecycle of an independent application.
//!
//! Ignored by default: this compiles Wasm, so it needs the installed target
//! and a matching wasm-bindgen. CI runs it explicitly.
mod support;

use std::fs;
use support::{Fixture, failure, success};

#[test]
#[ignore = "requires the installed Wasm target and a matching wasm-bindgen"]
fn a_failed_install_is_resumable_and_a_frozen_build_leaves_the_lock_alone() {
    let fixture = Fixture::new();
    let application = fixture.scaffold("recovery");

    // A missing tool fails installation, but the lockfile it resolved first is
    // kept, so the retry does not start over.
    let error = failure(
        fixture
            .cli(&application, &["install", "--offline"])
            .env("FUSOR_WASM_BINDGEN", fixture.root.join("missing-bindgen")),
    );
    assert!(error.contains("wasm-bindgen"), "{error}");
    assert!(application.join("Cargo.lock").is_file());

    success(&mut fixture.cli(&application, &["install", "--frozen"]));
    let lock = fs::read(application.join("Cargo.lock")).unwrap();
    success(&mut fixture.cli(&application, &["check", "--frozen"]));
    success(&mut fixture.cli(&application, &["build", "--debug", "--frozen"]));

    assert_eq!(lock, fs::read(application.join("Cargo.lock")).unwrap());
    assert!(application.join("dist/.fusor-output.json").is_file());
    // A production build must not leave development output behind.
    assert!(!application.join(".fusor/dev").exists());
}

#[test]
#[cfg(unix)]
#[ignore = "requires prepared Wasm tools and localhost binding; npm failures are controlled without Node"]
fn an_interrupted_npm_restore_invalidates_its_stamp_and_retries() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let application = fixture.scaffold("npm-recovery");
    fixture.lock(&application);
    fs::write(
        application.join("package.json"),
        r#"{"private":true,"devDependencies":{"esbuild":"0.28.2"}}"#,
    )
    .unwrap();
    fs::write(
        application.join("package-lock.json"),
        r#"{"lockfileVersion":3}"#,
    )
    .unwrap();

    // A stand-in npm that records its calls, refuses to run while a completed
    // stamp exists, and can be told to fail partway through.
    let fake_bin = fixture.root.join("fake-bin");
    fs::create_dir(&fake_bin).unwrap();
    let npm = fake_bin.join("npm");
    fs::write(
        &npm,
        r#"#!/bin/sh
[ "$1" = ci ] || exit 41
[ ! -e .fusor/npm-install.json ] || exit 42
printf 'ci\n' >> npm-invocations
mkdir -p node_modules/esbuild
printf partial > node_modules/esbuild/partial
[ "$FUSOR_TEST_NPM_FAIL" != 1 ] || exit 43
printf ready > node_modules/esbuild/ready
"#,
    )
    .unwrap();
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let command = |args: &[&str]| {
        let mut command = fixture.cli(&application, args);
        command.env("PATH", &path);
        command
    };

    let stamp = application.join(".fusor/npm-install.json");
    let ready = application.join("node_modules/esbuild/ready");
    success(&mut command(&["install", "--frozen"]));
    assert!(stamp.is_file() && ready.is_file());

    // Explicit installation repairs a partial package despite a matching stamp.
    fs::remove_file(&ready).unwrap();
    success(&mut command(&["install", "--frozen"]));
    assert!(ready.is_file());

    fs::remove_dir_all(application.join("node_modules")).unwrap();
    // Stop dev right after preparation, without leaving a server running.
    fs::write(
        application.join("src/lib.rs"),
        "compile_error!(\"stop after npm preparation\");",
    )
    .unwrap();
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    drop(listener);

    let error =
        failure(command(&["dev", "--frozen", "--port", &port]).env("FUSOR_TEST_NPM_FAIL", "1"));
    assert!(error.contains("JavaScript preparation failed"), "{error}");
    assert!(!stamp.exists(), "an interrupted install leaves no stamp");
    assert!(application.join("node_modules/esbuild/partial").is_file());

    let error = failure(&mut command(&["dev", "--frozen", "--port", &port]));
    assert!(error.contains("stop after npm preparation"), "{error}");
    assert!(stamp.is_file() && ready.is_file());
    let calls = fs::read_to_string(application.join("npm-invocations")).unwrap();
    assert_eq!(calls.lines().count(), 4);

    // A warm dev run skips installation once the completed stamp matches.
    let error =
        failure(command(&["dev", "--frozen", "--port", &port]).env("FUSOR_TEST_NPM_FAIL", "1"));
    assert!(error.contains("stop after npm preparation"), "{error}");
    assert_eq!(
        calls,
        fs::read_to_string(application.join("npm-invocations")).unwrap()
    );
}

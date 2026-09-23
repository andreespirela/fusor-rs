//! npm installs only from the committed lockfile; builds never resolve new
//! versions.
use crate::{
    context::Context,
    error::{Error, Result},
    layout,
    process::checked,
    workspace::Project,
};
use std::{fs, path::Path, process::Command};

pub(crate) fn npm(cx: &Context, root: &Path, args: &[&str]) -> Result {
    let executable = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let mut command = Command::new(executable);
    command
        .args(args)
        .args(["--no-audit", "--no-fund"])
        .current_dir(root);
    if cx.quiet {
        command
            .args(["--loglevel", "error"])
            .stdout(std::process::Stdio::null());
    }
    if cx.offline {
        command.arg("--offline").env("npm_config_offline", "true");
    }
    checked(&mut command).map_err(|error| {
        Error::tooling(format!("JavaScript preparation failed: {error}"))
            .remedy("install Node.js and npm, then run `fusor install`")
    })
}

/// `explicit` always restores the tree, which is how a partially removed
/// package is repaired. Otherwise the recorded stamp can skip npm.
pub(crate) fn prepare(cx: &Context, project: &Project, explicit: bool, initial: bool) -> Result {
    let manifest = project.root.join("package.json");
    if !manifest.is_file() {
        return Ok(());
    }
    let pending = project.root.join(layout::PENDING_JAVASCRIPT);
    let initial = initial
        || fs::read_to_string(&pending).is_ok_and(|version| version == env!("CARGO_PKG_VERSION"));
    let lock = project.root.join("package-lock.json");
    if !lock.is_file() {
        if !initial || cx.locked {
            return Err(Error::project("package-lock.json is missing").remedy(
                "run `fusor add javascript` for the initial resolution, then commit the lockfile",
            ));
        }
        npm(cx, &project.root, &["install", "--package-lock-only"])?;
    }

    let manifest_bytes = fs::read(&manifest)?;
    let lock_bytes = fs::read(&lock)?;
    let signature = serde_json::to_vec(&(manifest_bytes, &lock_bytes))?;
    let stamp = project.root.join(layout::NPM_STAMP);
    let installed = project.root.join("node_modules").is_dir();
    if !explicit && installed && fs::read(&stamp).ok().as_deref() == Some(signature.as_slice()) {
        cx.reporter
            .note("node_modules matches the recorded installation; skipping npm");
        return Ok(());
    }
    cx.reporter.step(match (explicit, installed) {
        (true, true) => "Installing JavaScript dependencies from package-lock.json ...",
        (false, true) => {
            "Installing JavaScript dependencies; the recorded installation no longer matches ..."
        }
        _ => "Installing JavaScript dependencies ...",
    });

    // An interrupted installation must not be mistaken later for a completed
    // one.
    match fs::remove_file(&stamp) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    npm(cx, &project.root, &["ci"])?;
    if fs::read(&lock)? != lock_bytes {
        return Err(
            Error::project("npm rewrote package-lock.json during `npm ci`")
                .remedy("inspect the lockfile before continuing"),
        );
    }
    if let Some(parent) = stamp.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(stamp, signature)?;
    if pending.is_file() {
        fs::remove_file(pending)?;
    }
    Ok(())
}

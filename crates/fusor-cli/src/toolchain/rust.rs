//! Never installs Rust or changes the default toolchain. It will install the
//! channel a project's own `rust-toolchain.toml` asks for.
use crate::{
    context::Context,
    error::{Error, Result},
    layout,
    process::{checked, rustup},
};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Before anything slow. Only `may_install` callers honor a pinned channel.
pub(crate) fn preflight(cx: &Context, directory: &Path, may_install: bool) -> Result {
    let attempt = || {
        rustup("rustc")
            .arg("--version")
            .current_dir(directory)
            .output()
    };
    let output = attempt();
    if output.as_ref().is_ok_and(|out| out.status.success()) {
        return Ok(());
    }
    if may_install && !cx.offline {
        if let Some(channel) = declared_channel(directory)? {
            cx.reporter
                .step(format!("Installing Rust toolchain {channel} ..."));
            install_toolchain(cx, directory, &channel)?;
            if attempt().is_ok_and(|out| out.status.success()) {
                return Ok(());
            }
        }
    }
    let detail = output
        .map(|out| String::from_utf8_lossy(&out.stderr).trim().to_owned())
        .unwrap_or_default();
    let message = if detail.is_empty() {
        "Rust is not available for this project".to_owned()
    } else {
        format!("Rust is not available for this project: {detail}")
    };
    Err(Error::tooling(message).remedy(format!(
        "install Rust from https://rustup.rs with your platform's build prerequisites, or run `rustup toolchain install {}`, then `fusor install`",
        layout::RUST_VERSION
    )))
}

/// `RUSTUP_TOOLCHAIN` means the caller has already chosen.
fn declared_channel(directory: &Path) -> Result<Option<String>> {
    if env::var_os("RUSTUP_TOOLCHAIN").is_some() {
        return Ok(None);
    }
    let file = directory.join("rust-toolchain.toml");
    if !file.is_file() {
        return Ok(None);
    }
    let has_rustup = rustup("rustup")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !has_rustup {
        return Ok(None);
    }
    let document: toml::Value = fs::read_to_string(&file)?.parse()?;
    Ok(document
        .get("toolchain")
        .and_then(|toolchain| toolchain.get("channel"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned))
}

fn install_toolchain(cx: &Context, directory: &Path, channel: &str) -> Result {
    let mut install = rustup("rustup");
    if cx.quiet {
        install.arg("--quiet");
    }
    checked(
        install
            .args([
                "toolchain",
                "install",
                channel,
                "--profile",
                "minimal",
                "--no-self-update",
            ])
            .current_dir(directory),
    )
}

/// Checks for the standard library itself; rustup can list a target whose
/// files are gone.
pub(crate) fn target_ready(root: &Path) -> Result<bool> {
    let output = rustup("rustc")
        .args(["--print", "target-libdir", "--target", layout::TARGET])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Ok(false);
    }
    let directory = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    Ok(fs::read_dir(directory).is_ok_and(|files| {
        files
            .filter_map(std::result::Result::ok)
            .any(|file| file.file_name().to_string_lossy().starts_with("libcore-"))
    }))
}

pub(crate) fn install_target(cx: &Context, root: &Path) -> Result {
    if target_ready(root)? {
        return Ok(());
    }
    if cx.offline {
        return Err(
            Error::tooling(format!("the {} target is not installed", layout::TARGET))
                .remedy("run `fusor install` while online"),
        );
    }
    cx.reporter
        .step(format!("Installing the {} target ...", layout::TARGET));
    let mut install = rustup("rustup");
    if cx.quiet {
        install.arg("--quiet");
    }
    checked(
        install
            .args(["target", "add", layout::TARGET])
            .current_dir(root),
    )
}

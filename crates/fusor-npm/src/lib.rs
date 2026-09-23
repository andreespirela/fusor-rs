//! Internal browser module bundling used by the fusor CLI.
//!
//! Applications do not depend on this crate or call it from build.rs. A nonempty
//! compiler module manifest opts the browser CLI into Node; Rust-only builds and
//! native Cargo checks never launch it. Dependencies must already be installed.
use serde_json::{Value, json};
use std::{env, error::Error, fs, path::Path, process::Command};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
const TOOL: &str = include_str!("tool.mjs");

/// Bundle compiler-discovered JavaScript modules and a wasm-bindgen entry into
/// the caller's unpublished generation. No dependencies are installed or fetched.
/// The returned graph records authored inputs and generated stylesheet paths.
#[doc(hidden)]
pub fn bundle(
    root: &Path,
    entry: &Path,
    modules: &Value,
    public_path: &str,
    release: bool,
) -> Result<Value> {
    let modules = modules
        .as_array()
        .ok_or("invalid JavaScript module manifest")?;
    if modules.is_empty() {
        return Ok(json!({ "inputs": [], "styles": [] }));
    }
    let directory = entry.parent().ok_or("JavaScript entry has no parent")?;
    fs::create_dir_all(directory)?;
    let script = directory.join("fusor-javascript-tool.mjs");
    let manifest = directory.join("fusor-javascript-modules.json");
    fs::write(&script, TOOL)?;
    fs::write(&manifest, serde_json::to_vec(modules)?)?;
    let mut command = Command::new(env::var_os("FUSOR_NODE").unwrap_or_else(|| "node".into()));
    command
        .current_dir(root)
        .arg(&script)
        .arg(root)
        .arg(entry)
        .arg(&manifest)
        .arg(public_path);
    if release {
        command.arg("--release");
    }
    let execution = command.output();
    let _ = fs::remove_file(&script);
    let _ = fs::remove_file(&manifest);
    let result = execution
        .map_err(|error| format!("JavaScript modules need Node.js 22 or newer: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "Fusor JavaScript bundling failed:\n{}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    if !result.stderr.is_empty() {
        eprintln!("{}", String::from_utf8_lossy(&result.stderr));
    }
    Ok(serde_json::from_slice(&fs::read(
        directory.join("javascript-bundle.json"),
    )?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_only_builds_do_not_need_a_package_directory_or_node() {
        let output = bundle(
            Path::new("/nonexistent-fusor-application"),
            Path::new("/nonexistent-fusor-output/app.js"),
            &json!([]),
            "/__fusor/unused/pkg",
            false,
        )
        .unwrap();
        assert_eq!(output, json!({ "inputs": [], "styles": [] }));
    }
}

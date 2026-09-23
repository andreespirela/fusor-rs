//! Artifact paths come from Cargo's JSON messages, not guessed profile
//! directories, so a changed profile cannot silently publish a stale site.
use super::{declarations, diagnostics::Diagnostics};
use crate::{
    context::Context,
    error::{Error, Result},
    layout,
    process::cargo,
    workspace::Project,
};
use fusor_build::app::ArtifactManifest;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Stdio},
};

#[derive(Clone, Copy)]
pub(crate) enum Mode {
    Check,
    Build { release: bool },
}

pub(crate) struct Compilation {
    pub manifest: ArtifactManifest,
    pub wasm: Option<PathBuf>,
    /// The native renderer of an islands application.
    pub executable: Option<PathBuf>,
}

pub(crate) fn compile(cx: &Context, project: &Project, mode: Mode) -> Result<Compilation> {
    let mut command = cargo();
    command.current_dir(&project.workspace).arg(match mode {
        Mode::Check => "check",
        Mode::Build { .. } => "build",
    });
    if let Some(delivery) = &project.config.delivery {
        command.args(["--bin", delivery.binary.as_deref().unwrap_or(&project.name)]);
    } else {
        command.args(["--lib", "--target", layout::TARGET]);
    }
    command.arg("--message-format=json");
    if matches!(mode, Mode::Build { release: true }) {
        command.arg("--release");
    }
    project.flags(cx, &mut command);

    let mut child = command.stdout(Stdio::piped()).spawn().map_err(|error| {
        Error::tooling(format!("could not run cargo: {error}"))
            .remedy("install Rust from https://rustup.rs")
    })?;
    let stdout = child.stdout.take().expect("Cargo stdout was piped");
    let mut diagnostics = Diagnostics::default();
    // Always reap Cargo, even after a malformed message; a compiler left
    // running would block the next build on the lock file.
    let streamed = read_messages(stdout, project, &mut diagnostics);
    let status = reap(&mut child, streamed.is_err())?;
    let streamed = streamed?;
    if !status.success() {
        return Err(Error::compile(
            "Rust compilation failed; see the compiler diagnostics above",
        ));
    }
    let manifest = streamed.artifact.ok_or_else(|| {
        Error::project("the application emitted no Fusor artifact")
            .remedy("call fusor_build::compile_app() from the package's build.rs")
    })?;
    declarations::write(project, &manifest)?;
    Ok(Compilation {
        manifest,
        wasm: streamed.wasm,
        executable: streamed.executable,
    })
}

#[derive(Default)]
struct Streamed {
    artifact: Option<ArtifactManifest>,
    wasm: Option<PathBuf>,
    executable: Option<PathBuf>,
}

fn read_messages(
    stdout: ChildStdout,
    project: &Project,
    diagnostics: &mut Diagnostics,
) -> Result<Streamed> {
    let mut streamed = Streamed::default();
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            // A build script may print to stdout. Forward it verbatim.
            eprintln!("{line}");
            continue;
        };
        let selected = message["package_id"].as_str() == Some(&project.id);
        match message["reason"].as_str() {
            Some("build-script-executed") => {
                if let Some(path) = artifact_path(&message) {
                    let manifest = ArtifactManifest::read(Path::new(path))?;
                    // A diagnostic in a dependency still points at its HTML.
                    diagnostics.register(&manifest, &project.workspace)?;
                    if selected {
                        streamed.artifact = Some(manifest);
                    }
                }
            }
            Some("compiler-artifact") if selected => {
                if let Some(path) = message["executable"].as_str() {
                    streamed.executable = Some(path.into());
                }
                for file in message["filenames"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if Path::new(file)
                        .extension()
                        .is_some_and(|name| name == "wasm")
                    {
                        streamed.wasm = Some(file.into());
                    }
                }
            }
            Some("compiler-message") => diagnostics.print(&message["message"], &project.workspace),
            _ => {}
        }
    }
    Ok(streamed)
}

fn artifact_path(message: &Value) -> Option<&str> {
    message["env"].as_array()?.iter().find_map(|pair| {
        (pair[0].as_str() == Some("FUSOR_ARTIFACT_MANIFEST"))
            .then(|| pair[1].as_str())
            .flatten()
    })
}

fn reap(child: &mut Child, failed: bool) -> Result<std::process::ExitStatus> {
    if failed {
        let _ = child.kill();
    }
    Ok(child.wait()?)
}

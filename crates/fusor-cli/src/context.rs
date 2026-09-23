//! Global flags after their interactions are settled, so the engine reads plain
//! booleans.
use crate::{
    cli::{Action, Cli, Color},
    reporter::{Reporter, Verbosity},
};
use std::path::PathBuf;

#[derive(Clone, Default)]
pub(crate) struct Context {
    pub reporter: Reporter,
    pub manifest_path: Option<PathBuf>,
    pub package: Option<String>,
    pub offline: bool,
    pub locked: bool,
    pub features: Vec<String>,
    pub color: Color,
    pub verbose: bool,
    pub quiet: bool,
    /// Set only by `dev`, which publishes to `.fusor/dev`.
    pub output: Option<PathBuf>,
}

impl Context {
    pub fn new(cli: &Cli) -> Self {
        let offline = cli.offline
            || cli.frozen
            || std::env::var("CARGO_NET_OFFLINE")
                .is_ok_and(|value| value == "true" || value == "1");
        // A check or build that silently updated Cargo.lock would not be
        // reproducible.
        let locked = cli.locked
            || cli.frozen
            || matches!(
                cli.command,
                Some(
                    Action::Dev { .. }
                        | Action::Build { .. }
                        | Action::Check
                        | Action::Expand { .. }
                )
            );
        let verbosity = match (cli.quiet, cli.verbose) {
            (true, _) => Verbosity::Quiet,
            (_, true) => Verbosity::Verbose,
            _ => Verbosity::Normal,
        };
        Self {
            reporter: Reporter::new(verbosity, cli.color),
            manifest_path: cli.manifest_path.clone(),
            package: cli.package.clone(),
            offline,
            locked,
            features: cli.features.clone(),
            color: cli.color,
            verbose: cli.verbose,
            quiet: cli.quiet,
            output: None,
        }
    }

    /// Re-enter discovery for a known manifest: after scaffolding, on a
    /// rebuild, or for a delivery unit. The dev output override is dropped.
    pub fn for_manifest(&self, manifest: PathBuf, package: Option<String>) -> Self {
        Self {
            manifest_path: Some(manifest),
            package,
            output: None,
            ..self.clone()
        }
    }
}

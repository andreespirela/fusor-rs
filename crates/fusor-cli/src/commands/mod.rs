pub(crate) mod add;
pub(crate) mod build;
pub(crate) mod check;
pub(crate) mod dev;
pub(crate) mod doctor;
pub(crate) mod expand;
pub(crate) mod install;
pub(crate) mod new;
pub(crate) mod preview;
pub(crate) mod repo;

use crate::{
    cli::{Action, RepoAction},
    context::Context,
    error::Result,
    toolchain,
    workspace::{Project, select},
};
use std::path::Path;

pub(crate) fn dispatch(cx: &Context, action: Action) -> Result {
    match action {
        // `preview` and `doctor` must run on a host with build output but no
        // Rust, so these never resolve an application through Cargo.
        Action::New {
            path,
            framework_path,
            javascript,
            skip_install,
            // `new` has no prompts; --yes is accepted so scripts keep working.
            yes: _,
        } => new::run(
            cx,
            &path,
            framework_path.as_deref(),
            javascript,
            skip_install,
        ),
        Action::Preview {
            directory,
            port,
            open,
        } => preview::run(cx, directory.as_deref(), port, open),
        Action::Doctor => doctor::run(cx),
        Action::Add {
            capability,
            dry_run,
        } => add::run(cx, capability, dry_run),
        Action::Repo {
            command: RepoAction::Check,
        } => repo::check(cx),

        // These name the application in any failure, which matters in a
        // workspace.
        Action::Install => {
            let project = application(cx, Prepare::Toolchain)?;
            install::run(cx, &project).map_err(|error| error.context(&project.name))
        }
        Action::Check => {
            let project = application(cx, Prepare::None)?;
            check::run(cx, &project).map_err(|error| error.context(&project.name))
        }
        Action::Build { debug, site: true } => {
            toolchain::rust::preflight(cx, Path::new("."), false)?;
            build::site(cx, debug)
        }
        Action::Build { debug, site: false } => {
            let project = application(cx, Prepare::None)?;
            build::single(cx, &project, debug).map_err(|error| error.context(&project.name))
        }
        Action::Expand { module } => {
            let project = application(cx, Prepare::None)?;
            expand::run(cx, &project, &module).map_err(|error| error.context(&project.name))
        }
        Action::Dev {
            port,
            open,
            site: true,
        } => {
            toolchain::rust::preflight(cx, Path::new("."), true)?;
            dev::site(cx, port, open)
        }
        Action::Dev {
            port,
            open,
            site: false,
        } => {
            let project = application(cx, Prepare::Toolchain)?;
            dev::run(cx, project, port, open)
        }
    }
}

/// Only commands the user runs to prepare may install a missing Rust
/// toolchain. `check` and `build` report it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Prepare {
    None,
    Toolchain,
}

fn application(cx: &Context, prepare: Prepare) -> Result<Project> {
    let located = select::from_filesystem(cx)?.map(|candidate| candidate.manifest);
    let directory = located
        .as_deref()
        .and_then(Path::parent)
        .unwrap_or(Path::new("."));
    toolchain::rust::preflight(cx, directory, prepare == Prepare::Toolchain)?;
    Project::discover_at(cx, located.as_deref())
}

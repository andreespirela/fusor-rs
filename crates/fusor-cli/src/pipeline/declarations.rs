//! TypeScript declarations come from the Rust artifact, so even `fusor check`
//! writes them without Node.
use crate::{
    error::{Error, Result},
    layout,
    workspace::Project,
};
use fusor_build::app::ArtifactManifest;
use std::{fs, path::Path};

/// Lists what we wrote last time, so removals never touch files we do not own.
const INDEX: &str = ".generated.json";

pub(crate) fn write(project: &Project, artifact: &ArtifactManifest) -> Result {
    let directory = project.root.join(layout::TYPES);
    let index = directory.join(INDEX);
    if artifact.javascript.is_empty() && !index.is_file() {
        return Ok(());
    }
    fs::create_dir_all(&directory)?;
    let previous: Vec<String> = fs::read(&index)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let mut names = Vec::new();
    for module in &artifact.javascript {
        let name = &module.declaration_name;
        if !is_declaration_name(name) || names.contains(name) {
            return Err(Error::internal(format!(
                "the compiler emitted an invalid or duplicate declaration name {name:?}"
            )));
        }
        write_if_changed(&directory.join(name), &fs::read(&module.declaration)?)?;
        names.push(name.clone());
    }
    for name in previous {
        if is_declaration_name(&name) && !names.contains(&name) {
            let _ = fs::remove_file(directory.join(name));
        }
    }
    write_if_changed(&index, &serde_json::to_vec_pretty(&names)?)
}

/// These come from the compiler, but they become filesystem paths.
fn is_declaration_name(name: &str) -> bool {
    let path = Path::new(name);
    name.ends_with(".d.ts")
        && path.components().count() == 1
        && matches!(
            path.components().next(),
            Some(std::path::Component::Normal(_))
        )
}

/// Editors watch this directory and reload on any write.
fn write_if_changed(path: &Path, contents: &[u8]) -> Result {
    if fs::read(path).ok().as_deref() != Some(contents) {
        fs::write(path, contents)?;
    }
    Ok(())
}

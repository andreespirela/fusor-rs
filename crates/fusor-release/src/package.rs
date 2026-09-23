//! The binaries must report the expected version and the archive must unpack
//! before it is announced.
use crate::{Result, archive, workspace_root, workspace_version};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const BINARIES: [&str; 2] = ["fusor", "cargo-fusor"];

pub fn run(target: &str, binary_directory: Option<&Path>, output: &Path) -> Result {
    let root = workspace_root();
    let version = workspace_version()?;
    let windows = target.contains("windows");
    let suffix = if windows { ".exe" } else { "" };
    let binaries = binary_directory
        .map(Path::to_owned)
        .unwrap_or_else(|| root.join("target").join(target).join("release"));

    let name = format!("fusor-{version}-{target}");
    fs::create_dir_all(output)?;
    let stage = output.join(format!(".staging-{name}"));
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage)?;

    for binary in BINARIES {
        let source = binaries.join(format!("{binary}{suffix}"));
        verify_version(&source, &version)?;
        fs::copy(&source, stage.join(format!("{binary}{suffix}")))?;
    }
    fs::copy(root.join("LICENSE"), stage.join("LICENSE"))?;
    fs::copy(
        root.join("crates/fusor-cli/README.md"),
        stage.join("README.md"),
    )?;
    fs::write(stage.join("build.json"), build_metadata(&version, target)?)?;

    let archive_path = output.join(format!(
        "{name}{}",
        if windows { ".zip" } else { ".tar.gz" }
    ));
    if windows {
        archive::zip(&stage, &name, &archive_path)?;
        archive::verify_zip(&archive_path, entry_count(&stage)?)?;
    } else {
        archive::tar_gz(&stage, &name, &archive_path)?;
        run_from_archive(&archive_path, &name, suffix)?;
    }
    write_checksum(&archive_path)?;
    fs::remove_dir_all(&stage)?;
    println!("{}", archive_path.display());
    Ok(())
}

fn verify_version(binary: &Path, version: &str) -> Result {
    let output = Command::new(binary).arg("--version").output()?;
    let reported = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if reported != format!("fusor {version}") {
        return Err(format!(
            "{}: reports {reported:?}, expected fusor {version}",
            binary.display()
        )
        .into());
    }
    Ok(())
}

fn build_metadata(version: &str, target: &str) -> Result<String> {
    let rustc = Command::new("rustc").arg("--version").output()?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "version": version,
        "target": target,
        "rustc": String::from_utf8_lossy(&rustc.stdout).trim(),
    }))? + "\n")
}

/// Also proves the executable bit survived.
fn run_from_archive(archive_path: &Path, name: &str, suffix: &str) -> Result {
    let unpacked = archive_path.with_extension("unpacked");
    let _ = fs::remove_dir_all(&unpacked);
    fs::create_dir_all(&unpacked)?;
    let file = fs::File::open(archive_path)?;
    tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(&unpacked)?;
    for binary in BINARIES {
        let executable = unpacked.join(name).join(format!("{binary}{suffix}"));
        let status = Command::new(&executable)
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .status()?;
        if !status.success() {
            return Err(format!("{} --help failed from the archive", executable.display()).into());
        }
    }
    fs::remove_dir_all(&unpacked)?;
    Ok(())
}

fn entry_count(stage: &Path) -> Result<usize> {
    Ok(fs::read_dir(stage)?.count())
}

/// `sha256sum`-compatible.
fn write_checksum(archive_path: &Path) -> Result<PathBuf> {
    let name = archive_path
        .file_name()
        .ok_or("the archive has no filename")?
        .to_string_lossy()
        .into_owned();
    let digest = format!("{:x}", Sha256::digest(fs::read(archive_path)?));
    let checksum = archive_path.with_file_name(format!("{name}.sha256"));
    fs::write(&checksum, format!("{digest}  {name}\n"))?;
    Ok(checksum)
}

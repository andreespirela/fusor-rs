use crate::error::{Error, Result};
use std::process::Command;

/// A rustup proxy must never install a toolchain behind the user's back.
pub(crate) fn cargo() -> Command {
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.env("RUSTUP_AUTO_INSTALL", "0");
    command
}

pub(crate) fn rustup(program: &str) -> Command {
    let mut command = Command::new(program);
    command.env("RUSTUP_AUTO_INSTALL", "0");
    command
}

/// The program's own output has already reached the terminal, so the error
/// only names it.
pub(crate) fn checked(command: &mut Command) -> Result {
    let status = command.status().map_err(|error| {
        Error::tooling(format!(
            "could not run {}: {error}",
            command.get_program().to_string_lossy()
        ))
    })?;
    if !status.success() {
        return Err(Error::tooling(format!(
            "{} failed with {status}",
            command.get_program().to_string_lossy()
        )));
    }
    Ok(())
}

use std::{env, process::ExitCode};

fn main() -> ExitCode {
    match fusor_cli::run(env::args_os()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => error.report(),
    }
}

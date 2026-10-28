mod cli;
mod services;

use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

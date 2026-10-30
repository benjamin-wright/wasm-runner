use std::process::ExitCode;

fn main() -> ExitCode {
    match wasm_runner::execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

use std::process::ExitCode;

fn main() -> ExitCode {
    match kasane::run_without_plugins() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

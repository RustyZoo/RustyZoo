mod audio;
mod boot;
mod cli;
mod findcmd;
mod input;
mod layout;
mod probe;
mod run;
mod simcmd;
mod textcmd;
mod window;

use std::io::{self, Write};
use std::process::ExitCode;

use cli::{CliError, USAGE};

fn main() -> ExitCode {
    match cli::run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage) => {
            // A closed standard error is not worth a panic, which `eprintln!` would raise.
            let _ = writeln!(io::stderr(), "{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failed(err)) => {
            let _ = writeln!(io::stderr(), "rustyzoo: {err}");
            ExitCode::FAILURE
        }
    }
}

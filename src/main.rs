// On Windows no console opens when the game is started from the file manager; a command typed in
// a terminal writes to that terminal's console (`console::attach`).
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod audio;
mod boot;
mod cli;
#[cfg(windows)]
mod console;
mod findcmd;
mod input;
mod layout;
mod probe;
mod run;
mod simcmd;
mod textcmd;
mod window;

use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

use sdl3::messagebox::{MessageBoxFlag, show_simple_message_box};

use cli::{CliError, USAGE};

fn main() -> ExitCode {
    #[cfg(windows)]
    console::attach();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let boxed = in_a_box(&args);
    match cli::run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage) => {
            // A closed standard error is not worth a panic, which `eprintln!` would raise.
            let _ = writeln!(io::stderr(), "{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failed(err)) => {
            let _ = writeln!(io::stderr(), "rustyzoo: {err}");
            if boxed {
                show_box(&err.to_string());
            }
            ExitCode::FAILURE
        }
    }
}

/// Whether an error is also shown in a message box: a program opened from the file manager, with
/// no arguments, has no visible error output.
fn in_a_box(args: &[OsString]) -> bool {
    args.is_empty()
}

fn show_box(message: &str) {
    if let Err(e) = show_simple_message_box(MessageBoxFlag::ERROR, "RustyZoo", message, None) {
        let _ = writeln!(io::stderr(), "rustyzoo: warning: no message box: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::in_a_box;

    #[test]
    fn only_a_start_without_arguments_shows_errors_in_a_box() {
        assert!(in_a_box(&[]));
        assert!(!in_a_box(&["run".into()]));
        assert!(!in_a_box(&["--help".into()]));
        assert!(!in_a_box(&["Saved Games/a.zoo".into()]));
    }
}

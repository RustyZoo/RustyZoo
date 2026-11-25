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

use sdl3::messagebox::{
    ButtonData, ClickedButton, MessageBoxButtonFlag, MessageBoxFlag, show_message_box,
    show_simple_message_box,
};
use sdl3::video::Window;

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

/// Asks whether to send a crash report: `Some(true)` for Yes, `Some(false)` for No, `None` when the
/// box cannot be shown or is closed without an answer.
fn ask_crash_report(question: &str) -> Option<bool> {
    ask(MessageBoxFlag::ERROR, question)
}

/// Asks whether to share anonymous statistics, like [`ask_crash_report`].
fn ask_statistics(question: &str) -> Option<bool> {
    ask(MessageBoxFlag::INFORMATION, question)
}

/// A box with Yes and No and no button chosen beforehand: neither Return nor Escape answers.
fn ask(icon: MessageBoxFlag, question: &str) -> Option<bool> {
    let buttons = [
        ButtonData {
            flags: MessageBoxButtonFlag::NOTHING,
            button_id: 1,
            text: "Yes",
        },
        ButtonData {
            flags: MessageBoxButtonFlag::NOTHING,
            button_id: 0,
            text: "No",
        },
    ];
    let title = "RustyZoo";
    match show_message_box(icon, &buttons, title, question, None::<&Window>, None) {
        Ok(ClickedButton::CustomButton(button)) => Some(button.button_id == 1),
        Ok(ClickedButton::CloseButton) => None,
        Err(e) => {
            let _ = writeln!(io::stderr(), "rustyzoo: warning: no message box: {e}");
            None
        }
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

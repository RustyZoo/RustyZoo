//! What the window tells through its events, and whether it is asleep.

use rustyzoo::app::Command;
use sdl3::event::{Event, WindowEvent};
use sdl3::video::WindowFlags;

use crate::input;

/// Whether the window is hidden or minimised, or full screen without the focus (main loop 3.2),
/// from its flags. The flags are read, not followed
/// through the events: a restore that comes as another event cannot leave the loop asleep.
pub fn asleep(flags: WindowFlags) -> bool {
    flags.intersects(WindowFlags::HIDDEN | WindowFlags::MINIMIZED)
        || (flags.contains(WindowFlags::FULLSCREEN) && !focused(flags))
}

/// Whether the window has the input focus, from its flags (read, like [`asleep`]). A hidden or
/// minimised window has none.
pub fn focused(flags: WindowFlags) -> bool {
    flags.contains(WindowFlags::INPUT_FOCUS)
}

/// What the window tells through its events.
#[derive(Default)]
pub struct State {
    /// The system asks to draw the window again.
    pub exposed: bool,
    /// Alt+Enter was released.
    pub toggle: bool,
}

impl State {
    /// Turns an event into a command or a change of state.
    pub fn note(&mut self, event: Event, commands: &mut Vec<Command>) {
        if let Some(command) = input::command(&event) {
            commands.push(command);
        }
        self.toggle |= input::alt_enter(&event);
        match event {
            Event::Quit { .. }
            | Event::Window {
                win_event: WindowEvent::CloseRequested,
                ..
            } => commands.push(Command::Quit),
            Event::Window {
                win_event: WindowEvent::Exposed,
                ..
            } => self.exposed = true,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_flags_decide_whether_the_loop_blocks() {
        let window = |win_event| Event::Window {
            timestamp: 0,
            window_id: 1,
            win_event,
        };
        let (mut state, mut commands) = (State::default(), Vec::new());
        assert!(asleep(WindowFlags::MINIMIZED));
        assert!(asleep(WindowFlags::HIDDEN | WindowFlags::INPUT_FOCUS));
        assert!(
            !asleep(WindowFlags::MAXIMIZED),
            "a restore may come as Maximized"
        );
        assert!(
            !asleep(WindowFlags::empty()),
            "inactive but visible keeps running"
        );
        let full = WindowFlags::FULLSCREEN;
        assert!(
            asleep(full),
            "inactive in full screen blocks like minimised"
        );
        assert!(!asleep(full | WindowFlags::INPUT_FOCUS));
        assert!(focused(WindowFlags::INPUT_FOCUS | WindowFlags::MAXIMIZED));
        assert!(!focused(WindowFlags::empty()) && !focused(WindowFlags::MINIMIZED));
        state.note(window(WindowEvent::FocusLost), &mut commands);
        assert!(!state.exposed && commands.is_empty());
        state.note(window(WindowEvent::Exposed), &mut commands);
        assert!(state.exposed);
        state.note(window(WindowEvent::CloseRequested), &mut commands);
        state.note(Event::Quit { timestamp: 0 }, &mut commands);
        assert_eq!(commands, [Command::Quit, Command::Quit]);
    }
}

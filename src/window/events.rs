//! What the window tells through its events, and whether it is asleep.

use rustyzoo::app::{Command, Mouse};
use rustyzoo::text::to_cp1252;
use sdl3::event::{Event, WindowEvent};
use sdl3::mouse::MouseButton;
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

/// Whether the maximise button of the title bar was pressed on a windowed game, from the flags
/// of the window (read, like [`asleep`]): the game then goes to full screen (input 3.4).
pub fn maximised(flags: WindowFlags, fullscreen: bool) -> bool {
    !fullscreen && flags.contains(WindowFlags::MAXIMIZED)
}

/// A mouse message in points of the window, kept until the loop knows where the picture is.
#[derive(Debug, PartialEq)]
pub enum Raw {
    Move(f32, f32),
    Down(f32, f32),
    Up(f32, f32),
    Wheel(i32),
}

/// What the window tells through its events.
#[derive(Default)]
pub struct State {
    /// The system asks to draw the window again.
    pub exposed: bool,
    /// Alt+Enter was released.
    pub toggle: bool,
    /// The game has an interface: the mouse, the keys it reads and the typed characters become
    /// commands.
    pub interface: bool,
    /// The mouse messages of the left button, the pointer and the wheel, in order.
    pub mouse: Vec<Raw>,
}

impl State {
    /// Turns an event into a command or a change of state.
    pub fn note(&mut self, event: Event, commands: &mut Vec<Command>) {
        if let Some(command) = input::command(&event) {
            commands.push(command);
        }
        self.toggle |= input::alt_enter(&event);
        if self.interface {
            commands.extend(input::interface_command(&event));
            match event {
                Event::MouseMotion { x, y, .. } => self.mouse.push(Raw::Move(x, y)),
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => self.mouse.push(Raw::Down(x, y)),
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => self.mouse.push(Raw::Up(x, y)),
                Event::MouseWheel { integer_y, .. } if integer_y != 0 => {
                    self.mouse.push(Raw::Wheel(integer_y))
                }
                Event::TextInput { ref text, .. } => {
                    commands.extend(to_cp1252(text).into_iter().map(Command::Char))
                }
                _ => {}
            }
        }
        match event {
            Event::Quit { .. }
            | Event::Window {
                win_event: WindowEvent::CloseRequested,
                ..
            } => commands.push(Command::Quit),
            // A resize changes where the picture goes: draw it again, even if the game paints
            // nothing.
            Event::Window {
                win_event:
                    WindowEvent::Exposed | WindowEvent::Resized(..) | WindowEvent::PixelSizeChanged(..),
                ..
            } => self.exposed = true,
            _ => {}
        }
    }
}

impl State {
    /// Turns the mouse messages noted so far into commands. `place` gives the pixel of the game
    /// under a point of the window. A button message first moves the pointer to its position.
    pub fn mouse_commands<E>(
        &mut self,
        mut place: impl FnMut((f32, f32)) -> Result<(i32, i32), E>,
        commands: &mut Vec<Command>,
    ) -> Result<(), E> {
        for raw in std::mem::take(&mut self.mouse) {
            match raw {
                Raw::Move(x, y) => {
                    let (px, py) = place((x, y))?;
                    commands.push(Command::Mouse(Mouse::Move(px, py)));
                }
                Raw::Down(x, y) | Raw::Up(x, y) => {
                    let (px, py) = place((x, y))?;
                    commands.push(Command::Mouse(Mouse::Move(px, py)));
                    let down = matches!(raw, Raw::Down(..));
                    commands.push(Command::Mouse(if down { Mouse::Down } else { Mouse::Up }));
                }
                Raw::Wheel(n) => commands.push(Command::Mouse(Mouse::Wheel(n))),
            }
        }
        Ok(())
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
        assert!(maximised(
            WindowFlags::MAXIMIZED | WindowFlags::RESIZABLE,
            false
        ));
        assert!(!maximised(WindowFlags::RESIZABLE, false), "just resized");
        assert!(
            !maximised(WindowFlags::MAXIMIZED, true),
            "already full screen"
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
        for resize in [
            WindowEvent::Resized(5, 5),
            WindowEvent::PixelSizeChanged(0, -1),
        ] {
            state.exposed = false;
            state.note(window(resize), &mut commands);
            assert!(state.exposed, "a resize draws the picture again");
        }
        state.note(window(WindowEvent::CloseRequested), &mut commands);
        state.note(Event::Quit { timestamp: 0 }, &mut commands);
        assert_eq!(commands, [Command::Quit, Command::Quit]);
    }

    #[test]
    fn with_an_interface_the_mouse_the_keys_and_the_text_become_commands() {
        let button = |mouse_btn, x, y| Event::MouseButtonDown {
            timestamp: 0,
            window_id: 1,
            which: 0,
            mouse_btn,
            clicks: 1,
            x,
            y,
        };
        let mut state = State::default();
        let mut commands = Vec::new();
        state.note(button(MouseButton::Left, 3.0, 4.0), &mut commands);
        assert!(
            state.mouse.is_empty() && commands.is_empty(),
            "no interface"
        );
        state.interface = true;
        state.note(button(MouseButton::Left, 3.0, 4.0), &mut commands);
        state.note(button(MouseButton::Right, 9.0, 9.0), &mut commands);
        state.note(
            Event::MouseWheel {
                timestamp: 0,
                window_id: 1,
                which: 0,
                x: 0.0,
                y: 1.0,
                direction: sdl3::mouse::MouseWheelDirection::Normal,
                mouse_x: 0.0,
                mouse_y: 0.0,
                integer_x: 0,
                integer_y: -1,
            },
            &mut commands,
        );
        state.note(
            Event::TextInput {
                timestamp: 0,
                window_id: 1,
                text: "A\u{e9}\u{4e2d}".into(),
            },
            &mut commands,
        );
        assert_eq!(
            commands,
            [Command::Char(65), Command::Char(233), Command::Char(63)]
        );
        // The points of the window become pixels of the game when the loop knows where it is.
        let mut commands = Vec::new();
        let place = |(x, y): (f32, f32)| Ok::<_, ()>((x as i32 * 2, y as i32 * 2));
        state.mouse_commands(place, &mut commands).unwrap();
        assert_eq!(
            commands,
            [
                Command::Mouse(Mouse::Move(6, 8)),
                Command::Mouse(Mouse::Down),
                Command::Mouse(Mouse::Wheel(-1)),
            ]
        );
        assert!(state.mouse.is_empty());
    }
}

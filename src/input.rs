//! The keyboard and the mouse of the map view (input 4): what an SDL event asks of the game, as
//! a [`Command`]. Keys are read by position (scancode), so the named keys do not depend on the
//! layout, as the virtual keys of the original. The rest, the edge scroll band and its timing,
//! is the core's: the pointer only tells where it is.

use rustyzoo::app::Command;
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::{Mod, Scancode};

/// The command of one event, if it asks for one. Every key-down acts, a repeat included (7).
pub fn command(event: &Event) -> Option<Command> {
    match *event {
        Event::KeyDown {
            scancode: Some(key),
            keymod,
            ..
        } => key_command(key, keymod),
        // The pointer, in points of the window.
        Event::MouseMotion { x, y, .. } => Some(Command::Pointer(Some((x as i32, y as i32)))),
        // An inactive window does not scroll (4.2).
        Event::Window {
            win_event: WindowEvent::MouseLeave | WindowEvent::FocusLost,
            ..
        } => Some(Command::Pointer(None)),
        _ => None,
    }
}

fn key_command(key: Scancode, keymod: Mod) -> Option<Command> {
    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
    let alt = keymod.intersects(Mod::LALTMOD | Mod::RALTMOD);
    if key == Scancode::Pause {
        return Some(Command::Pause);
    }
    // With Alt the keys are system keys (4.1).
    if alt {
        return None;
    }
    // The keypad types digits with Num Lock on and is the arrows and Home to Page Down off.
    let key = match (key, keymod.contains(Mod::NUMMOD)) {
        (_, true) => key,
        (Scancode::Kp4, _) => Scancode::Left,
        (Scancode::Kp6, _) => Scancode::Right,
        (Scancode::Kp8, _) => Scancode::Up,
        (Scancode::Kp2, _) => Scancode::Down,
        (Scancode::Kp7, _) => Scancode::Home,
        (Scancode::Kp1, _) => Scancode::End,
        (Scancode::Kp9, _) => Scancode::PageUp,
        (Scancode::Kp3, _) => Scancode::PageDown,
        _ => key,
    };
    Some(match (key, ctrl) {
        (Scancode::Left, true) => Command::Rotate(false),
        (Scancode::Right, true) => Command::Rotate(true),
        (Scancode::Up, true) => Command::Zoom(true),
        (Scancode::Down, true) => Command::Zoom(false),
        (Scancode::Left, false) => Command::Scroll(-1, 0),
        (Scancode::Right, false) => Command::Scroll(1, 0),
        (Scancode::Up, false) => Command::Scroll(0, -1),
        (Scancode::Down, false) => Command::Scroll(0, 1),
        (Scancode::Home, _) => Command::Scroll(-1, -1),
        (Scancode::End, _) => Command::Scroll(-1, 1),
        (Scancode::PageUp, _) => Command::Scroll(1, -1),
        (Scancode::PageDown, _) => Command::Scroll(1, 1),
        (Scancode::Space, false) => Command::Pause,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Scancode, keymod: Mod) -> Option<Command> {
        command(&Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            keycode: None,
            scancode: Some(key),
            keymod,
            repeat: false,
            which: 0,
            raw: 0,
        })
    }

    fn window(win_event: WindowEvent) -> Event {
        Event::Window {
            timestamp: 0,
            window_id: 1,
            win_event,
        }
    }

    #[test]
    fn the_eight_scroll_keys_step_one_way_and_ignore_shift() {
        let none = Mod::NOMOD;
        for (key, dir) in [
            (Scancode::Left, (-1, 0)),
            (Scancode::Right, (1, 0)),
            (Scancode::Up, (0, -1)),
            (Scancode::Down, (0, 1)),
            (Scancode::Home, (-1, -1)),
            (Scancode::End, (-1, 1)),
            (Scancode::PageUp, (1, -1)),
            (Scancode::PageDown, (1, 1)),
        ] {
            let scroll = Some(Command::Scroll(dir.0, dir.1));
            assert_eq!(press(key, none), scroll, "{key:?}");
            assert_eq!(press(key, Mod::LSHIFTMOD), scroll, "{key:?} with Shift");
        }
        // Home to Page Down ignore Ctrl.
        assert_eq!(
            press(Scancode::End, Mod::RCTRLMOD),
            Some(Command::Scroll(-1, 1))
        );
    }

    #[test]
    fn ctrl_arrows_rotate_and_zoom() {
        let ctrl = Mod::LCTRLMOD;
        assert_eq!(press(Scancode::Left, ctrl), Some(Command::Rotate(false)));
        assert_eq!(press(Scancode::Right, ctrl), Some(Command::Rotate(true)));
        assert_eq!(press(Scancode::Up, ctrl), Some(Command::Zoom(true)));
        assert_eq!(press(Scancode::Down, ctrl), Some(Command::Zoom(false)));
    }

    #[test]
    fn pause_and_space_toggle_the_pause_and_alt_makes_system_keys() {
        assert_eq!(press(Scancode::Pause, Mod::NOMOD), Some(Command::Pause));
        assert_eq!(press(Scancode::Space, Mod::NOMOD), Some(Command::Pause));
        assert_eq!(press(Scancode::Left, Mod::LALTMOD), None);
        assert_eq!(press(Scancode::Space, Mod::RALTMOD), None);
        assert_eq!(press(Scancode::A, Mod::NOMOD), None);
        assert_eq!(press(Scancode::Escape, Mod::NOMOD), None, "O4");
    }

    #[test]
    fn the_keypad_scrolls_only_with_num_lock_off() {
        assert_eq!(
            press(Scancode::Kp4, Mod::NOMOD),
            Some(Command::Scroll(-1, 0))
        );
        assert_eq!(
            press(Scancode::Kp3, Mod::NOMOD),
            Some(Command::Scroll(1, 1))
        );
        assert_eq!(
            press(Scancode::Kp8, Mod::LCTRLMOD),
            Some(Command::Zoom(true))
        );
        assert_eq!(press(Scancode::Kp4, Mod::NUMMOD), None);
    }

    #[test]
    fn the_pointer_follows_the_mouse_and_goes_when_the_window_is_left_or_inactive() {
        let motion = |x, y| Event::MouseMotion {
            timestamp: 0,
            window_id: 1,
            which: 0,
            mousestate: sdl3::mouse::MouseState::from_sdl_state(0),
            x,
            y,
            xrel: 0.0,
            yrel: 0.0,
        };
        assert_eq!(
            command(&motion(7.9, 1020.0)),
            Some(Command::Pointer(Some((7, 1020))))
        );
        for lost in [WindowEvent::MouseLeave, WindowEvent::FocusLost] {
            assert_eq!(command(&window(lost)), Some(Command::Pointer(None)));
        }
        assert_eq!(command(&window(WindowEvent::Exposed)), None);
    }
}

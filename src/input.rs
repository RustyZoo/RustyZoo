//! The keyboard and the mouse of the map view (input 4): what an SDL event asks of the game, as
//! a [`Command`]. Keys are read by position (scancode), so the named keys do not depend on the
//! layout, as the virtual keys of the original. The rest, the edge scroll band and its timing,
//! is the core's: the pointer only tells where it is.

use rustyzoo::app::{Command, Key};
use sdl3::event::Event;
use sdl3::keyboard::{Mod, Scancode};

/// The command of one event, if it asks for one. Every key-down acts, a repeat included (7).
pub fn command(event: &Event) -> Option<Command> {
    match *event {
        Event::KeyDown {
            scancode: Some(key),
            keymod,
            ..
        } => key_command(key, keymod),
        _ => None,
    }
}

/// The command for the interface of one key-down: the keys its widgets read. Alt makes system
/// keys (4.1). The map view's own keys are [`command`]'s.
pub fn interface_command(event: &Event) -> Option<Command> {
    let Event::KeyDown {
        scancode: Some(key),
        keymod,
        ..
    } = *event
    else {
        return None;
    };
    if keymod.intersects(Mod::LALTMOD | Mod::RALTMOD) {
        return None;
    }
    let key = match key {
        Scancode::Return | Scancode::KpEnter => Key::Enter,
        Scancode::Escape => Key::Escape,
        Scancode::Tab => Key::Tab,
        Scancode::Backspace => Key::Backspace,
        Scancode::Delete => Key::Delete,
        Scancode::Left => Key::Left,
        Scancode::Right => Key::Right,
        Scancode::Up => Key::Up,
        Scancode::Down => Key::Down,
        Scancode::Home => Key::Home,
        Scancode::End => Key::End,
        Scancode::PageUp => Key::PageUp,
        Scancode::PageDown => Key::PageDown,
        Scancode::Y => Key::Y,
        Scancode::N => Key::N,
        _ => return None,
    };
    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
    let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
    Some(Command::Key(key, ctrl, shift))
}

/// Whether the event is the release of Enter with Alt held: the switch between window and full
/// screen (3.4). The key-down of Alt+Enter is a system key and does nothing else (4.1).
pub fn alt_enter(event: &Event) -> bool {
    matches!(
        *event,
        Event::KeyUp { scancode: Some(Scancode::Return | Scancode::KpEnter), keymod, .. }
            if keymod.intersects(Mod::LALTMOD | Mod::RALTMOD)
    )
}

fn key_command(key: Scancode, keymod: Mod) -> Option<Command> {
    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
    let alt = keymod.intersects(Mod::LALTMOD | Mod::RALTMOD);
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
        (Scancode::G, true) => Command::Grid,
        (Scancode::Left, false) => Command::Scroll(-1, 0),
        (Scancode::Right, false) => Command::Scroll(1, 0),
        (Scancode::Up, false) => Command::Scroll(0, -1),
        (Scancode::Down, false) => Command::Scroll(0, 1),
        (Scancode::Home, _) => Command::Scroll(-1, -1),
        (Scancode::End, _) => Command::Scroll(-1, 1),
        (Scancode::PageUp, _) => Command::Scroll(1, -1),
        (Scancode::PageDown, _) => Command::Scroll(1, 1),
        // Ctrl+Space still types a space; Ctrl+Pause is another key.
        (Scancode::Pause, false) => Command::Pause,
        (Scancode::Space, _) => Command::Space,
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

    #[test]
    fn alt_enter_is_the_release_of_enter_with_alt_held() {
        let up = |key, keymod| {
            alt_enter(&Event::KeyUp {
                timestamp: 0,
                window_id: 1,
                keycode: None,
                scancode: Some(key),
                keymod,
                repeat: false,
                which: 0,
                raw: 0,
            })
        };
        assert!(up(Scancode::Return, Mod::LALTMOD));
        assert!(up(Scancode::KpEnter, Mod::RALTMOD | Mod::NUMMOD));
        assert!(!up(Scancode::Return, Mod::NOMOD));
        assert!(!up(Scancode::Return, Mod::LCTRLMOD));
        assert!(!up(Scancode::Space, Mod::LALTMOD));
        assert_eq!(
            press(Scancode::Return, Mod::LALTMOD),
            None,
            "key-down does nothing"
        );
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
    fn ctrl_g_toggles_the_grid() {
        assert_eq!(press(Scancode::G, Mod::LCTRLMOD), Some(Command::Grid));
        assert_eq!(press(Scancode::G, Mod::RCTRLMOD), Some(Command::Grid));
        assert_eq!(press(Scancode::G, Mod::NOMOD), None);
        assert_eq!(press(Scancode::G, Mod::LCTRLMOD | Mod::LALTMOD), None);
    }

    #[test]
    fn pause_and_space_toggle_the_pause_and_alt_makes_system_keys() {
        assert_eq!(press(Scancode::Pause, Mod::NOMOD), Some(Command::Pause));
        assert_eq!(press(Scancode::Space, Mod::NOMOD), Some(Command::Space));
        assert_eq!(press(Scancode::Space, Mod::LCTRLMOD), Some(Command::Space));
        assert_eq!(press(Scancode::Pause, Mod::LALTMOD), None);
        assert_eq!(press(Scancode::Pause, Mod::LCTRLMOD), None);
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
    fn the_interface_reads_its_keys_with_ctrl_and_shift_and_not_with_alt() {
        let press = |key, keymod| {
            interface_command(&Event::KeyDown {
                timestamp: 0,
                window_id: 1,
                keycode: None,
                scancode: Some(key),
                keymod,
                repeat: false,
                which: 0,
                raw: 0,
            })
        };
        assert_eq!(
            press(Scancode::Return, Mod::NOMOD),
            Some(Command::Key(Key::Enter, false, false))
        );
        assert_eq!(
            press(Scancode::Left, Mod::LSHIFTMOD | Mod::RCTRLMOD),
            Some(Command::Key(Key::Left, true, true))
        );
        assert_eq!(
            press(Scancode::Y, Mod::NOMOD),
            Some(Command::Key(Key::Y, false, false))
        );
        assert_eq!(press(Scancode::Return, Mod::LALTMOD), None);
        assert_eq!(press(Scancode::F1, Mod::NOMOD), None);
    }
}

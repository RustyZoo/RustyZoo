//! The game state of the map view and its pass: the map, the staged picture, the camera, the
//! timers, the calendar and the pause flag, driven by [`Command`]s. Before a game is started (the
//! main menu) there is no map: the pass runs the timers and the interface, and the menu acts on
//! what the player selects. No window and no input
//! events: a shell turns what the player does into commands and presents the frames that
//! [`App::pass`] paints, so a windowed run and a scripted one share every line of this.
//!
//! One pass applies the commands first (in the original they arrive between two passes, main
//! loop 3.1), then runs the timers in their order: the draw, the interface (the camera's edge
//! scroll), the simulation (only the calendar, so far; the pause stops it, main loop 3.3).
//!
//! With an [`Interface`] the tree is painted over the map: the map view element draws the staged
//! map into its rectangle, and a due draw paints when the camera moved or the interface changed
//! (a timed element hid itself, an animation showed another frame, ...).
//! The keys and the edge scroll of the map view then act only while the map view is shown and no
//! other element has the keyboard focus. Without one, the frame is the map alone.

use std::fmt;
use std::str::FromStr;

use crate::Framebuffer;
use crate::camera::{Camera, Settings};
use crate::menu::Menu;
use crate::music::Music;
use crate::render::{MapRenderer, RenderError, Staged, check_size};
use crate::timing::{Calendar, Pass, Timers};
use crate::ui::interface::Interface;
use crate::zoo::Zoo;

/// A key the interface reads, by the name a script gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Y,
    N,
}

const KEY_NAMES: [(&str, Key); 15] = [
    ("enter", Key::Enter),
    ("escape", Key::Escape),
    ("tab", Key::Tab),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("left", Key::Left),
    ("right", Key::Right),
    ("up", Key::Up),
    ("down", Key::Down),
    ("home", Key::Home),
    ("end", Key::End),
    ("pageup", Key::PageUp),
    ("pagedown", Key::PageDown),
    ("y", Key::Y),
    ("n", Key::N),
];

impl Key {
    fn name(self) -> &'static str {
        KEY_NAMES.iter().find(|k| k.1 == self).map_or("", |k| k.0)
    }
}

/// What the mouse does over the window, in window coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mouse {
    /// The pointer moved here.
    Move(i32, i32),
    /// The left button went down or up at the pointer.
    Down,
    Up,
    /// The wheel turned by this many notches (positive: away from the player).
    Wheel(i32),
}

/// What the player does (input 4). The text form is the script format of a headless run:
/// `scroll DX DY`, `pointer X Y`, `pointer none`, `rotate cw`, `rotate ccw`, `zoom in`,
/// `zoom out`, `fullscreen on`, `fullscreen off`, `pause`, `quit` for the map view, `skip` (any key
/// or button press, which stops an intro movie), and for the
/// interface `mouse X Y`, `down`, `up`, `wheel N`, `key NAME [ctrl] [shift]` (NAME: enter, escape,
/// tab, backspace, delete, left, right, up, down, home, end, pageup, pagedown, y, n) and
/// `char C` (a byte, as a number from 0 to 255 or as one character that is not a digit).
/// A window reads `fullscreen` in a script as a request to switch, and makes the command itself
/// when the window has switched, so a log replays it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// One key-down message of a scroll key: the direction on each axis, -1, 0 or 1.
    Scroll(i32, i32),
    /// The pointer is at this position in the window, or has left it (or the window is not
    /// active): the edge scroll of the interface pass reads it.
    Pointer(Option<(i32, i32)>),
    /// A quarter turn, clockwise or not.
    Rotate(bool),
    /// Zoom in or out.
    Zoom(bool),
    /// The window is full screen (true) or windowed (false) now.
    Fullscreen(bool),
    /// Toggle the pause.
    Pause,
    /// Close the game.
    Quit,
    Mouse(Mouse),
    /// A key of the interface, with Ctrl and Shift held or not.
    Key(Key, bool, bool),
    /// A character typed, as a Windows-1252 byte.
    Char(u8),
    /// A key or a mouse button was pressed: stops the intro movie that plays. Ignored by the game.
    Skip,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Command::Scroll(x, y) => write!(f, "scroll {x} {y}"),
            Command::Pointer(Some((x, y))) => write!(f, "pointer {x} {y}"),
            Command::Pointer(None) => f.write_str("pointer none"),
            Command::Rotate(cw) => write!(f, "rotate {}", if cw { "cw" } else { "ccw" }),
            Command::Zoom(zoom_in) => write!(f, "zoom {}", if zoom_in { "in" } else { "out" }),
            Command::Fullscreen(on) => write!(f, "fullscreen {}", if on { "on" } else { "off" }),
            Command::Pause => f.write_str("pause"),
            Command::Quit => f.write_str("quit"),
            Command::Mouse(Mouse::Move(x, y)) => write!(f, "mouse {x} {y}"),
            Command::Mouse(Mouse::Down) => f.write_str("down"),
            Command::Mouse(Mouse::Up) => f.write_str("up"),
            Command::Mouse(Mouse::Wheel(n)) => write!(f, "wheel {n}"),
            Command::Key(key, ctrl, shift) => {
                write!(f, "key {}", key.name())?;
                if ctrl {
                    f.write_str(" ctrl")?;
                }
                if shift {
                    f.write_str(" shift")?;
                }
                Ok(())
            }
            Command::Char(byte) => write!(f, "char {byte}"),
            Command::Skip => f.write_str("skip"),
        }
    }
}

impl FromStr for Command {
    type Err = String;

    fn from_str(text: &str) -> Result<Command, String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let number = |w: &str| w.parse::<i32>().map_err(|_| format!("not a number: {w:?}"));
        Ok(match words.as_slice() {
            ["scroll", x, y] => {
                let (x, y) = (number(x)?, number(y)?);
                if !(-1..=1).contains(&x) || !(-1..=1).contains(&y) {
                    return Err("scroll takes directions from -1 to 1".into());
                }
                Command::Scroll(x, y)
            }
            ["pointer", "none"] => Command::Pointer(None),
            ["pointer", x, y] => Command::Pointer(Some((number(x)?, number(y)?))),
            ["rotate", "cw"] => Command::Rotate(true),
            ["rotate", "ccw"] => Command::Rotate(false),
            ["zoom", "in"] => Command::Zoom(true),
            ["zoom", "out"] => Command::Zoom(false),
            ["fullscreen", "on"] => Command::Fullscreen(true),
            ["fullscreen", "off"] => Command::Fullscreen(false),
            ["pause"] => Command::Pause,
            ["quit"] => Command::Quit,
            ["skip"] => Command::Skip,
            ["mouse", x, y] => Command::Mouse(Mouse::Move(number(x)?, number(y)?)),
            ["down"] => Command::Mouse(Mouse::Down),
            ["up"] => Command::Mouse(Mouse::Up),
            ["wheel", n] => Command::Mouse(Mouse::Wheel(number(n)?)),
            ["key", name, mods @ ..] => {
                let key = KEY_NAMES
                    .iter()
                    .find(|k| k.0 == *name)
                    .ok_or_else(|| format!("not a key: {name:?}"))?
                    .1;
                let (mut ctrl, mut shift) = (false, false);
                for m in mods {
                    match *m {
                        "ctrl" => ctrl = true,
                        "shift" => shift = true,
                        _ => return Err(format!("not a key modifier: {m:?}")),
                    }
                }
                Command::Key(key, ctrl, shift)
            }
            ["char", c] => match (c.parse::<u8>(), c.as_bytes()) {
                (Ok(byte), _) => Command::Char(byte),
                (_, [byte]) if byte.is_ascii() && !byte.is_ascii_digit() => Command::Char(*byte),
                _ => return Err(format!("not a character: {c:?}")),
            },
            _ => return Err(format!("not a command: {text:?}")),
        })
    }
}

/// A pass in which no timer fired.
const IDLE: Pass = Pass {
    draw: false,
    interface: None,
    step: None,
};

/// What the camera looked like when a frame was painted: the frame changes with nothing else.
type Look = ((i32, i32), i32, u8);

/// A zoo being played.
struct Game {
    zoo: Zoo,
    staged: Staged,
    camera: Camera,
    calendar: Calendar,
    paused: bool,
}

pub struct App<'a> {
    renderer: MapRenderer<'a>,
    /// The zoo being played; `None` in the main menu.
    game: Option<Game>,
    timers: Timers,
    quit: bool,
    pointer: Option<(i32, i32)>,
    frame: Framebuffer,
    /// The interface drawn over the map, if the game has one.
    ui: Option<Interface>,
    /// The map as the staged picture draws it for `painted`, when there is an interface: the map
    /// view copies it, so a frame painted for the interface alone draws no map again.
    map: Framebuffer,
    /// The look of the map in `map` (`frame` without an interface); `None` before the first.
    painted: Option<Look>,
    /// What the last pass did: the commands it applied and the timers it fired.
    applied: Vec<Command>,
    fired: Pass,
    /// Whether the last pass painted a frame.
    drew: bool,
    /// What the interface did in the last pass (the lines of [`Interface::take_events`]) and the
    /// sounds it asked for.
    ui_events: Vec<String>,
    ui_sounds: Vec<&'static str>,
    /// The camera when the last draw fired, before the interface moved it; `None` without a game.
    drawn_at: Option<Look>,
    /// The main menu, while the player is in it.
    menu: Option<Menu>,
    /// What the menu did and has not been logged yet, one line each (the lines of its first
    /// show open the log of the first pass), and what it did in the last pass.
    menu_events: Vec<String>,
    menu_lines: Vec<String>,
    /// The menu music, until the game has none (`noMenuMusic`) or it is stopped.
    music: Option<Music>,
    /// Whether the interface update of the last pass changed the music's volume or stopped it.
    music_changed: bool,
}

impl<'a> App<'a> {
    /// A game on `zoo` seen through a window of `size` pixels, with the camera the map stores.
    pub fn new(
        renderer: MapRenderer<'a>,
        zoo: Zoo,
        settings: Settings,
        timers: Timers,
        size: (u32, u32),
    ) -> Result<App<'a>, RenderError> {
        let mut app = App::empty(renderer, timers, size)?;
        let camera = Camera::open(&zoo.view, (size.0 as i32, size.1 as i32), settings);
        let staged = app.renderer.stage(&zoo, camera.rotation, false)?;
        app.drawn_at = Some((camera.scroll, camera.zoom, camera.rotation));
        app.game = Some(Game {
            zoo,
            staged,
            camera,
            calendar: Calendar::new(),
            paused: false,
        });
        Ok(app)
    }

    /// The main menu seen through a window of `size` pixels: no game yet, the `menu` acting on
    /// what is selected in `ui`, and the menu music (if any) playing at full level.
    pub fn menu(
        renderer: MapRenderer<'a>,
        timers: Timers,
        size: (u32, u32),
        mut ui: Interface,
        mut menu: Menu,
        music: Option<Music>,
    ) -> Result<App<'a>, RenderError> {
        let mut app = App::empty(renderer, timers, size)?;
        app.menu_events = menu.show(&mut ui);
        app.set_interface(ui);
        app.menu = Some(menu);
        app.music = music;
        Ok(app)
    }

    fn empty(
        renderer: MapRenderer<'a>,
        timers: Timers,
        size: (u32, u32),
    ) -> Result<App<'a>, RenderError> {
        check_size(&[0, 0, i64::from(size.0), i64::from(size.1)])?;
        Ok(App {
            renderer,
            game: None,
            timers,
            quit: false,
            pointer: None,
            frame: Framebuffer::new(size.0, size.1),
            ui: None,
            map: Framebuffer::new(0, 0),
            painted: None,
            applied: Vec::new(),
            fired: IDLE,
            drew: false,
            ui_events: Vec::new(),
            ui_sounds: Vec::new(),
            drawn_at: None,
            menu: None,
            menu_events: Vec::new(),
            menu_lines: Vec::new(),
            music: None,
            music_changed: false,
        })
    }

    /// Puts the interface around the game. The window is the size it was resolved for.
    pub fn set_interface(&mut self, ui: Interface) {
        self.map = Framebuffer::new(self.frame.width, self.frame.height);
        self.painted = None;
        self.ui = Some(ui);
    }

    /// The interface, if the game has one.
    pub fn interface(&mut self) -> Option<&mut Interface> {
        self.ui.as_mut()
    }

    /// Whether the keys and the edge scroll of the map view act: always without an interface,
    /// else while the map view is shown and no other element has the keyboard focus.
    fn map_keys(&self) -> bool {
        self.ui
            .as_ref()
            .is_none_or(Interface::map_view_has_the_keys)
    }

    /// One pass at the clock reading `now` (ms) with the commands that came before it. Returns
    /// the frame when this pass painted one: a due draw paints only when the camera has moved
    /// since the last frame. A `Quit` ends the game before the rest of the pass: the commands
    /// after it and the timers do not run (main loop 3.1), and neither does any later pass.
    pub fn pass(&mut self, now: u64, commands: &[Command]) -> Option<&Framebuffer> {
        (self.applied, self.fired, self.drew) = (Vec::new(), IDLE, false);
        self.menu_lines.clear();
        (self.ui_events, self.ui_sounds) = (Vec::new(), Vec::new());
        self.music_changed = false;
        for &command in commands {
            if self.quit {
                return None;
            }
            self.applied.push(command);
            self.apply(command);
        }
        if self.quit {
            return None;
        }
        if let Some(ui) = &mut self.ui {
            self.ui_events.extend(ui.take_events());
            self.ui_sounds.extend(ui.take_sounds());
        }
        self.menu_acts();
        if self.quit {
            self.menu_lines = std::mem::take(&mut self.menu_events);
            return None;
        }
        self.fired = self.timers.pass(now, 1.0);
        if self.fired.draw {
            self.drawn_at = self.look();
            // The animations advance at the drawn frame (ui layout 5.3).
            let paused = self.paused();
            if let Some(ui) = &mut self.ui {
                ui.animate(now, paused);
            }
            self.drew = self.paint();
        }
        if let Some(period) = self.fired.interface {
            if self.map_keys()
                && let Some(game) = &mut self.game
            {
                game.camera.edge_scroll(&game.zoo, now, self.pointer);
            }
            if let Some(ui) = &mut self.ui {
                ui.update(period);
                if let Some(menu) = &mut self.menu {
                    self.menu_events.extend(menu.update(ui, period));
                }
                self.ui_events.extend(ui.take_events());
                self.ui_sounds.extend(ui.take_sounds());
            }
            if let Some(music) = &mut self.music {
                self.music_changed = music.update(period);
            }
        }
        if let Some(game) = &mut self.game
            && let Some(step) = self.fired.step
            && !game.paused
        {
            game.calendar.advance(step);
        }
        self.menu_lines = std::mem::take(&mut self.menu_events);
        self.drew.then_some(&self.frame)
    }

    /// The game starts with this menu music playing: it fades out from the first interface update
    /// (sound specification 6.3). The caller plays it and follows [`App::music`].
    pub fn start_game(&mut self, mut music: Option<Music>) {
        if let Some(music) = &mut music {
            music.start_fade();
        }
        self.music = music;
    }

    /// The menu music and whether the last pass changed its volume or stopped it.
    pub fn music(&self) -> Option<(&Music, bool)> {
        self.music.as_ref().map(|m| (m, self.music_changed))
    }

    /// Milliseconds from `now` to the next time a pass fires a timer: 0 when one is due.
    pub fn next_due(&self, now: u64) -> u64 {
        self.timers.next_due(now)
    }

    fn apply(&mut self, command: Command) {
        let keys = self.map_keys();
        match (command, &mut self.game) {
            (Command::Pointer(at), _) => self.pointer = at,
            (Command::Quit, _) => self.quit = true,
            (Command::Mouse(mouse), _) => {
                if let Some(ui) = &mut self.ui {
                    ui.mouse(mouse);
                }
            }
            (Command::Scroll(x, y), Some(game)) if keys => {
                game.camera.key_scroll(&game.zoo, (x, y));
            }
            (Command::Rotate(clockwise), Some(game)) if keys => {
                let before = game.camera.clone();
                game.camera.rotate(&game.zoo, None, clockwise);
                // Restage from the new side. If that fails the view stays as it was.
                match self.renderer.stage(&game.zoo, game.camera.rotation, false) {
                    Ok(staged) => game.staged = staged,
                    Err(_) => game.camera = before,
                }
            }
            (Command::Zoom(zoom_in), Some(game)) if keys => {
                game.camera.zoom(&game.zoo, None, zoom_in);
            }
            (Command::Fullscreen(on), Some(game)) => game.camera.set_fullscreen(&game.zoo, on),
            (Command::Pause, Some(game)) if keys => game.paused = !game.paused,
            // The map view's keys while it does not have them or there is no game, and the keys
            // and characters, which the widgets that take them (text fields, dialogs) read in
            // later issues.
            _ => {}
        }
    }

    /// The menu acts on what the player selected in the last pass.
    fn menu_acts(&mut self) {
        let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) else {
            return;
        };
        for id in ui.take_selected() {
            let (lines, exit) = menu.selected(ui, id);
            self.menu_events.extend(lines);
            self.quit |= exit;
        }
    }

    /// Paints the window of the view if it differs from the last frame: the camera moved or the
    /// interface changed.
    fn paint(&mut self) -> bool {
        let look = self.look();
        let moved = self.painted != look;
        if !moved && !self.ui.as_ref().is_some_and(Interface::dirty) {
            return false;
        }
        // The one place that draws the map; the zoom of the camera applies here.
        let target = match self.ui {
            Some(_) => &mut self.map,
            None => &mut self.frame,
        };
        if moved && let Some(game) = &self.game {
            game.staged.draw(
                target,
                game.camera.scroll.0,
                game.camera.scroll.1,
                game.camera.zoom,
            );
            self.painted = look;
        }
        if let Some(ui) = &mut self.ui {
            let map = &self.map;
            let game = self.game.is_some();
            ui.paint(&mut self.frame, &mut |frame, rect| {
                if game {
                    copy_rect(frame, map, rect);
                }
            });
        }
        true
    }

    /// The camera as it looks now; `None` without a game.
    fn look(&self) -> Option<Look> {
        self.game
            .as_ref()
            .map(|g| (g.camera.scroll, g.camera.zoom, g.camera.rotation))
    }

    /// The camera of the game; `None` in the menu.
    pub fn camera(&self) -> Option<&Camera> {
        self.game.as_ref().map(|g| &g.camera)
    }

    /// The calendar of the game; `None` in the menu.
    pub fn calendar(&self) -> Option<&Calendar> {
        self.game.as_ref().map(|g| &g.calendar)
    }

    pub fn paused(&self) -> bool {
        self.game.as_ref().is_some_and(|g| g.paused)
    }

    /// Whether the player asked to close the game.
    pub fn quit(&self) -> bool {
        self.quit
    }

    /// The sounds the interface asked for in the last pass, by file name without the extension
    /// (`uiclick`, `sounds/uiclick.wav`): the shell plays them.
    pub fn sounds(&self) -> &[&'static str] {
        &self.ui_sounds
    }

    /// The timers that fired in the last pass.
    pub fn fired(&self) -> Pass {
        self.fired
    }

    /// The first line of a log: the size of the window, which the frames depend on.
    pub fn log_header(&self) -> String {
        format!("size {}x{}", self.frame.width, self.frame.height)
    }

    /// The lines of the last pass, each led by the pass index and the clock: one per command it
    /// applied, in the text form of a script, then one per thing the interface did (`ui hover 7111`,
    /// `ui press ...`, `ui sound uiclick`, `ui select`, `ui show`, `ui hide`, `ui drag`), then one per thing the main menu did
    /// (`menu continue enabled NAME`, `menu continue disabled`, `menu version V` when it appears, `menu exit`),
    /// then one per timer that fired, in the order draw,
    /// interface (then the music, if the update changed its volume or stopped it), step. A draw
    /// tells the hash of the frame it painted (`-` if it painted none) and the camera it drew
    /// with (none without a game), a step the new date (none without a game). A pass that did nothing has no line.
    /// A log, with the size of the window, is all a replay needs: the commands and the clock of
    /// each pass that did something (`run --replay`). A windowed run prints the same lines.
    pub fn log_lines(&self, pass: u64, now: u64) -> Vec<String> {
        let mut lines = Vec::new();
        let lead = format!("pass {pass} t={now}");
        lines.extend(self.applied.iter().map(|c| format!("{lead} {c}")));
        lines.extend(self.ui_events.iter().map(|e| format!("{lead} ui {e}")));
        lines.extend(self.menu_lines.iter().map(|e| format!("{lead} menu {e}")));
        if self.fired.draw {
            let hash = match self.drew {
                true => format!("{:016x}", frame_hash(&self.frame)),
                false => "-".into(),
            };
            lines.push(match self.drawn_at {
                Some(((x, y), zoom, rotation)) => {
                    format!("{lead} draw {hash} view={x},{y} rotation={rotation} zoom={zoom}")
                }
                None => format!("{lead} draw {hash}"),
            });
        }
        if let Some(period) = self.fired.interface {
            lines.push(format!("{lead} interface {period}"));
            if let Some(music) = self.music.as_ref().filter(|_| self.music_changed) {
                lines.push(format!("{lead} {}", music.log_line()));
            }
        }
        if let Some(step) = self.fired.step
            && let Some(game) = &self.game
        {
            let d = game.calendar.date();
            let paused = if game.paused { " paused" } else { "" };
            lines.push(format!(
                "{lead} step {step} {:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}{paused}",
                d.year, d.month, d.day, d.hour, d.minute, d.second, d.millisecond
            ));
        }
        lines
    }
}

/// Copies the rectangle (left, top, width, height) of `from` to the same place of `to`, clipped.
fn copy_rect(to: &mut Framebuffer, from: &Framebuffer, (x, y, w, h): (i32, i32, i32, i32)) {
    let left = x.max(0) as usize;
    let right =
        (i64::from(x) + i64::from(w)).clamp(0, i64::from(to.width.min(from.width))) as usize;
    for row in
        y.max(0)..(i64::from(y) + i64::from(h)).min(i64::from(to.height.min(from.height))) as i32
    {
        let range = (row as usize * to.width as usize + left) * 4
            ..(row as usize * to.width as usize + right) * 4;
        if range.start < range.end {
            to.pixels[range.clone()].copy_from_slice(&from.pixels[range]);
        }
    }
}

/// FNV-1a over the pixels of a frame.
pub fn frame_hash(frame: &Framebuffer) -> u64 {
    frame.pixels.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests;

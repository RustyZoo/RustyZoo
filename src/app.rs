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
use crate::menu::{Menu, Request, Start};
use crate::music::Music;
use crate::render::{MapRenderer, RenderError, Staged, check_size};
use crate::text::to_cp1252;
use crate::timing::{Calendar, Pass, Timers};
use crate::ui::interface::{Action, Buttons, Interface};
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
    pub fn name(self) -> &'static str {
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
    /// The left button went down for the second time of a double click, which the system
    /// reports instead of a second `Down`.
    Double,
    /// The wheel turned by this many notches (positive: away from the player).
    Wheel(i32),
}

/// What the player does (input 4). The text form is the script format of a headless run:
/// `scroll DX DY`, `pointer X Y`, `pointer none`, `rotate cw`, `rotate ccw`, `zoom in`,
/// `zoom out`, `grid` (toggle the tile grid), `fullscreen on`, `fullscreen off`, `pause`, `space`, `quit` (end the run), `close` (the
/// window is closed) for the map view, `skip` (any key
/// or button press, which stops an intro movie), and for the
/// interface `mouse X Y`, `down`, `up`, `double` (the second press of a double click), `wheel N`, `key NAME [ctrl] [shift]` (NAME: enter, escape,
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
    /// Ctrl+G: switch the tile grid on or off.
    Grid,
    /// The window is full screen (true) or windowed (false) now.
    Fullscreen(bool),
    /// The Pause key: toggles the pause while the map view is shown, whatever has the focus.
    Pause,
    /// The space bar: presses the pause button, so it toggles the pause only while the map view
    /// is shown and no element has the keyboard focus (a field types the space).
    Space,
    /// End the run at once.
    Quit,
    /// The player closed the window: the program ends at once from the menu, and asks first in a
    /// game (main menu 9.3).
    Close,
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
            Command::Grid => f.write_str("grid"),
            Command::Fullscreen(on) => write!(f, "fullscreen {}", if on { "on" } else { "off" }),
            Command::Pause => f.write_str("pause"),
            Command::Space => f.write_str("space"),
            Command::Quit => f.write_str("quit"),
            Command::Close => f.write_str("close"),
            Command::Mouse(Mouse::Move(x, y)) => write!(f, "mouse {x} {y}"),
            Command::Mouse(Mouse::Down) => f.write_str("down"),
            Command::Mouse(Mouse::Up) => f.write_str("up"),
            Command::Mouse(Mouse::Double) => f.write_str("double"),
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
            ["grid"] => Command::Grid,
            ["fullscreen", "on"] => Command::Fullscreen(true),
            ["fullscreen", "off"] => Command::Fullscreen(false),
            ["pause"] => Command::Pause,
            ["space"] => Command::Space,
            ["quit"] => Command::Quit,
            ["close"] => Command::Close,
            ["skip"] => Command::Skip,
            ["mouse", x, y] => Command::Mouse(Mouse::Move(number(x)?, number(y)?)),
            ["down"] => Command::Mouse(Mouse::Down),
            ["up"] => Command::Mouse(Mouse::Up),
            ["double"] => Command::Mouse(Mouse::Double),
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

/// What the camera and the grid looked like when a frame was painted: the frame changes with nothing else.
type Look = ((i32, i32), i32, u8, bool);

/// A zoo being played.
struct Game {
    zoo: Zoo,
    staged: Staged,
    camera: Camera,
    calendar: Calendar,
    paused: bool,
    /// The tile grid is drawn (Ctrl+G).
    grid: bool,
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
    ui_actions: Vec<Action>,
    /// How many of the actions the menu has read.
    acted: usize,
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
    /// Whether the last pass started the menu music again, from its beginning.
    music_restarted: bool,
    /// The scroll settings a game opens its camera with, and whether the window is full screen.
    settings: Settings,
    fullscreen: bool,
    /// The question that is open and what it leads to.
    asked: Option<Asked>,
    /// Whether a fade of the menu music has begun in this run (only the first one waits), and
    /// whether a game opens without the main panel.
    faded: bool,
    bare: bool,
}

/// What an open question leads to when it is answered No: stopping the game for the menu, or
/// ending the program (main menu 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Leave,
    Exit,
}

/// The layout of the question boxes with a message, the main panel and the options panel, and the
/// buttons of the options panel that leave the game.
const QUESTION: i32 = 45000;
const MAIN_PANEL: i32 = 2;
const OPTIONS_PANEL: i32 = 5;
const MAIN_MENU_BUTTON: i32 = 1503;
const EXIT_BUTTON: i32 = 1504;
/// The questions: save before starting a new zoo, before exiting.
const ASK_LEAVE: i32 = 153;
const ASK_EXIT: i32 = 151;

impl<'a> App<'a> {
    /// A game on `zoo` seen through a window of `size` pixels, with the camera the map stores.
    pub fn new(
        renderer: MapRenderer<'a>,
        zoo: Zoo,
        settings: Settings,
        timers: Timers,
        size: (u32, u32),
    ) -> Result<App<'a>, RenderError> {
        let mut app = App::empty(renderer, timers, size, settings)?;
        app.open_game(zoo)?;
        Ok(app)
    }

    /// The main menu seen through a window of `size` pixels: no game yet, the `menu` acting on
    /// what is selected in `ui`, and the menu music (if any) playing at full level.
    pub fn menu(
        renderer: MapRenderer<'a>,
        settings: Settings,
        timers: Timers,
        size: (u32, u32),
        mut ui: Interface,
        mut menu: Menu,
        music: Option<Music>,
    ) -> Result<App<'a>, RenderError> {
        let mut app = App::empty(renderer, timers, size, settings)?;
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
        settings: Settings,
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
            ui_actions: Vec::new(),
            acted: 0,
            drawn_at: None,
            menu: None,
            menu_events: Vec::new(),
            menu_lines: Vec::new(),
            music: None,
            music_changed: false,
            music_restarted: false,
            settings,
            fullscreen: false,
            asked: None,
            faded: false,
            bare: false,
        })
    }

    /// Opens a game on `zoo`: its stored camera, the first draw, and a simulation whose next step is
    /// the cap, as after the original's blocking load (main loop 3.5). A failure leaves the app
    /// as it was.
    fn open_game(&mut self, zoo: Zoo) -> Result<(), RenderError> {
        let size = (self.frame.width as i32, self.frame.height as i32);
        let mut camera = Camera::open(&zoo.view, size, self.settings);
        if self.fullscreen {
            camera.set_fullscreen(&zoo, true);
        }
        let staged = self.renderer.stage(&zoo, camera.rotation, false)?;
        self.drawn_at = Some((camera.scroll, camera.zoom, camera.rotation, false));
        self.painted = None;
        self.timers.forget_simulation();
        self.game = Some(Game {
            zoo,
            staged,
            camera,
            calendar: Calendar::new(),
            paused: false,
            grid: false,
        });
        Ok(())
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

    /// Whether a question is open.
    fn modal(&self) -> bool {
        self.ui.as_ref().is_some_and(Interface::modal_open)
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
        (self.ui_events, self.ui_sounds, self.ui_actions) = (Vec::new(), Vec::new(), Vec::new());
        self.acted = 0;
        (self.music_changed, self.music_restarted) = (false, false);
        if let Some(ui) = &mut self.ui {
            ui.set_time(now);
        }
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
            self.ui_actions.extend(ui.take_actions());
        }
        self.acts();
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
                ui.update(now);
                if let Some(menu) = &mut self.menu {
                    self.menu_events.extend(menu.update(ui, now));
                }
                self.ui_events.extend(ui.take_events());
                self.ui_sounds.extend(ui.take_sounds());
                self.ui_actions.extend(ui.take_actions());
            }
            if let Some(music) = &mut self.music {
                self.music_changed = music.update(period);
            }
            self.acts();
        }
        let modal = self.modal();
        if let Some(game) = &mut self.game
            && let Some(step) = self.fired.step
            && !(game.paused || modal)
        {
            game.calendar.advance(step);
        }
        self.menu_lines = std::mem::take(&mut self.menu_events);
        self.drew.then_some(&self.frame)
    }

    /// The game starts with this menu music playing: it fades out from the first interface update
    /// (sound specification 6.3); the main panel shows with the map view. The caller plays the
    /// music and follows [`App::music`].
    pub fn start_game(&mut self, mut music: Option<Music>) {
        if let Some(music) = &mut music {
            music.start_fade(!self.faded);
        }
        self.faded = true;
        self.music = music;
        if let Some(ui) = &mut self.ui
            && !self.bare
        {
            ui.show(MAIN_PANEL);
        }
    }

    /// The games of this app show the map view alone, without the main panel.
    pub fn bare(&mut self) {
        self.bare = true;
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
        match command {
            Command::Close => return self.close(),
            Command::Key(Key::Escape, ..)
                if self.game.is_some() && self.ui.is_some() && self.map_keys() =>
            {
                return self.escape();
            }
            Command::Fullscreen(on) => self.fullscreen = on,
            _ => {}
        }
        let keys = self.map_keys();
        let shown = self.ui.as_ref().is_none_or(Interface::map_view_shown);
        let modal = self.modal();
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
            (Command::Grid, Some(game)) if keys => game.grid = !game.grid,
            (Command::Fullscreen(on), Some(game)) => game.camera.set_fullscreen(&game.zoo, on),
            // The Pause key works through a question too, but the question gives the flag back as
            // it was, so it is left alone there.
            (Command::Pause, Some(game)) if shown && !modal => game.paused = !game.paused,
            (Command::Space, Some(game)) if keys => game.paused = !game.paused,
            // The keys and characters go to the focused field or list.
            (Command::Key(key, _, shift), _) => {
                if let Some(ui) = &mut self.ui {
                    ui.key(key, shift);
                }
            }
            (Command::Char(byte), _) => {
                if let Some(ui) = &mut self.ui {
                    ui.char(byte);
                }
            }
            // The map view's keys while it does not have them or there is no game.
            _ => {}
        }
    }

    /// The menu and the game act on what the player did since the last look: the answers to the
    /// questions, the actions of the lists and the cash field, the buttons selected.
    fn acts(&mut self) {
        let Some(ui) = &mut self.ui else { return };
        let selected = ui.take_selected();
        let actions: Vec<Action> = self.ui_actions[self.acted..].to_vec();
        self.acted = self.ui_actions.len();
        for action in actions {
            match action {
                Action::Answer(code) => self.answered(code),
                action if self.game.is_none() => {
                    if let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) {
                        let lines = menu.action(self.renderer.vfs(), ui, action);
                        self.menu_events.extend(lines);
                    }
                    self.requested();
                }
                _ => {}
            }
        }
        for id in selected {
            match id {
                MAIN_MENU_BUTTON | EXIT_BUTTON if self.game.is_some() => {
                    let kind = if id == EXIT_BUTTON {
                        Asked::Exit
                    } else {
                        Asked::Leave
                    };
                    self.ask(kind);
                }
                _ if self.game.is_none() => {
                    if let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) {
                        let (lines, exit) = menu.selected(self.renderer.vfs(), ui, id);
                        self.menu_events.extend(lines);
                        self.quit |= exit;
                    }
                    self.requested();
                }
                _ => {}
            }
        }
        // What the menu did to the lists and the fields may have made more to tell.
        if let Some(ui) = &mut self.ui {
            self.ui_events.extend(ui.take_events());
        }
    }

    /// What the menu's last button asked of the game: open a file, or tell the player something.
    fn requested(&mut self) {
        match self.menu.as_mut().and_then(Menu::take_request) {
            Some(Request::Open(start)) => {
                let Some(menu) = &self.menu else { return };
                let zoo = menu.load(&start.path, self.renderer.catalogue());
                match zoo.and_then(|zoo| self.open_game(zoo).map_err(|e| e.to_string())) {
                    Ok(()) => self.begin(&start),
                    Err(why) => self.say(&why),
                }
            }
            Some(Request::Message(text)) => self.say(&text),
            None => {}
        }
    }

    /// A game was opened from the menu: the menu layouts are hidden, the main panel and the map
    /// view show, the menu music starts to fade and the simulation runs from the next pass (main
    /// menu 8.1). A freeform start also updates the music once with 2000 ms, which counts as one of
    /// the updates the first fade waits and moves no later one (8.3).
    fn begin(&mut self, start: &Start) {
        if let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) {
            menu.hide(ui, start.forget_last);
        }
        self.menu_events
            .push(format!("game start {} {}", start.label, start.path));
        let music = self.music.take();
        self.start_game(music);
        if start.freeform
            && let Some(music) = &mut self.music
        {
            music.update(2000);
        }
    }

    /// Tells the player something in a question box with an OK button.
    fn say(&mut self, text: &str) {
        self.menu_events.push(format!("message {text}"));
        if let Some(ui) = &mut self.ui {
            ui.ask(QUESTION, &to_cp1252(text), Buttons::OK);
        }
    }

    /// Asks whether to save before leaving; without an interface to ask with, leaves.
    fn ask(&mut self, kind: Asked) {
        let (id, name) = match kind {
            Asked::Leave => (ASK_LEAVE, "leave"),
            Asked::Exit => (ASK_EXIT, "exit"),
        };
        let asked = self.ui.as_mut().is_some_and(|ui| {
            let text = to_cp1252(&ui.string(id));
            ui.ask(QUESTION, &text, Buttons::YES_NO_CANCEL)
        });
        match asked {
            true => {
                self.asked = Some(kind);
                self.menu_events.push(format!("game ask {name}"));
            }
            false => self.proceed(kind),
        }
    }

    /// The answer to the question that was asked: Yes would save first, which there is nothing to
    /// do yet (saved games come later), so the question closes and the game goes on; No leaves;
    /// Cancel stays.
    fn answered(&mut self, code: i32) {
        let Some(kind) = self.asked.take() else {
            return;
        };
        match code {
            6 => self.menu_events.push("game save unavailable".to_string()),
            7 => self.proceed(kind),
            _ => {}
        }
    }

    fn proceed(&mut self, kind: Asked) {
        match kind {
            Asked::Leave => self.stop_game(),
            Asked::Exit => {
                self.menu_events.push("game exit".to_string());
                self.quit = true;
            }
        }
    }

    /// The window is closed: from the menu the program ends at once; in a game it asks first. A
    /// question that is open is answered Cancel before (input 4.7).
    fn close(&mut self) {
        if self.game.is_none() || self.ui.is_none() {
            self.quit = true;
            return;
        }
        if let Some(ui) = &mut self.ui
            && ui.modal_open()
        {
            ui.answer(2);
            // The answer is not for the new question: what else is waiting stays.
            let waiting = ui.take_actions();
            self.ui_actions
                .extend(waiting.into_iter().filter(|a| *a != Action::Answer(2)));
            self.asked = None;
        }
        self.ask(Asked::Exit);
    }

    /// Escape, with the map view having the keys: the options panel, if open, closes, else the question that
    /// leads to the menu (input 4.6).
    fn escape(&mut self) {
        if let Some(ui) = &mut self.ui {
            // The main panel is part of the game screen: only the panels opened from it close.
            if let Some(i) = ui.tree.find(OPTIONS_PANEL)
                && ui.tree.elements[i].state & crate::ui::layout::HIDDEN == 0
            {
                ui.tree.hide(i);
                ui.touch();
                return;
            }
        }
        self.ask(Asked::Leave);
    }

    /// Stops the game and shows the main menu again: Continue is checked, and the music, if it is
    /// not playing at full level, starts from its beginning (main menu 9.2). Without a menu to
    /// go back to, the run ends.
    fn stop_game(&mut self) {
        let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) else {
            self.menu_events.push("game exit".to_string());
            self.quit = true;
            return;
        };
        self.game = None;
        (self.painted, self.drawn_at) = (None, None);
        for id in [OPTIONS_PANEL, MAIN_PANEL] {
            if let Some(i) = ui.tree.find(id) {
                ui.tree.hide(i);
            }
        }
        self.menu_events.push("game stop".to_string());
        let shown = menu.show(ui);
        self.menu_events.extend(shown);
        if !self.music.as_ref().is_some_and(Music::playing) {
            self.music = menu.music();
            self.music_restarted = self.music.is_some();
            if self.music_restarted {
                self.menu_events.push("music restart".to_string());
            }
        }
    }

    /// Whether the last pass started the menu music again: the player plays it from the start.
    pub fn music_restarted(&self) -> bool {
        self.music_restarted
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
                game.grid,
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
            .map(|g| (g.camera.scroll, g.camera.zoom, g.camera.rotation, g.grid))
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
        self.modal() || self.game.as_ref().is_some_and(|g| g.paused)
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

    /// The actions the interface ran in the last pass, for the game's code.
    pub fn ui_actions(&self) -> &[Action] {
        &self.ui_actions
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
                Some(((x, y), zoom, rotation, _)) => {
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
            let paused = if self.paused() { " paused" } else { "" };
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

#[cfg(test)]
mod start_tests;

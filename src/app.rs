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
use crate::build::terrain::{MINUS, PLUS};
use crate::build::{BULLDOZER, Build};
use crate::camera::{Camera, Settings};
use crate::menu::{Menu, Request, Start};
use crate::money::{DOLLAR_KEY, DONATIONS, Money, SYMBOL, Symbol, Tone};
use crate::music::Music;
use crate::place::{self, Class, Outcome, Pending, Tints};
use crate::render::{Extra, MapRenderer, Mark, RenderError, Staged, check_size};
use crate::status::{Finances, STATUS_BUTTON};
use crate::terrain::{Tool, habit};
use crate::text::to_cp1252;
use crate::timing::{Calendar, Pass, Timers};
use crate::ui::interface::{Action, Buttons, Interface};
use crate::ui::layout::installed_expansions;
use crate::world::World;
use crate::zoo::{Entity, Zoo};

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
    /// The right button went up at the pointer.
    Right,
}

/// What the player does (input 4). The text form is the script format of a headless run:
/// `scroll DX DY`, `pointer X Y`, `pointer none`, `rotate cw`, `rotate ccw`, `zoom in`,
/// `zoom out`, `grid` (toggle the tile grid), `fullscreen on`, `fullscreen off`, `pause`, `space`, `quit` (end the run), `close` (the
/// window is closed) for the map view, `skip` (any key
/// or button press, which stops an intro movie), and for the
/// interface `mouse X Y`, `down`, `up`, `double` (the second press of a double click), `right` (the right button went up), `wheel N`, `key NAME [ctrl] [shift]` (NAME: enter, escape,
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
            Command::Mouse(Mouse::Right) => f.write_str("right"),
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
            ["right"] => Command::Mouse(Mouse::Right),
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

/// How far right of the pointer the cost of the pending object is written, in pixels.
const COST_OFFSET: i32 = 23;

/// What the camera and the grid looked like when a frame was painted: the frame changes with nothing else.
type Look = ((i32, i32), i32, u8, bool);

/// A zoo being played.
struct Game {
    world: World,
    staged: Staged,
    camera: Camera,
    calendar: Calendar,
    paused: bool,
    /// The tile grid is drawn (Ctrl+G; the terrain tools force it on), and what the player chose
    /// with Ctrl+G, which the grid goes back to when the tools are left (input 4.8).
    grid: bool,
    choice: bool,
    /// The cash and the accounts, and what of them the log and the status bar showed last.
    money: Money,
    told: Option<i64>,
    bar: Option<(String, Tone)>,
    /// What the Finances table of the zoo status window holds.
    finances: Finances,
    /// The building panels and the tool they choose.
    build: Build,
    /// The pending object: the tool at the place under the pointer, while the pointer (as the
    /// last move left it) is over the map view and a tool is chosen.
    pending: Option<Pending>,
    /// The gate tool's pending gate (`pending` is its object), while it is over a fence piece.
    gate: Option<place::gate::Gate>,
    /// The fence piece the gate tool last judged under the pointer (placing 6.4: the pending gate
    /// is made again only when the piece under the pointer changes).
    gate_over: Option<u32>,
    /// The pieces of the path or fence drag in progress, the start first (empty without a drag).
    run: Vec<Pending>,
    hover: bool,
    tints: Tints,
    /// Which terrain types are water.
    water: Vec<bool>,
    /// The smart fences test is on (`[UI] smartFences`).
    smart: bool,
    /// The transient items the staged picture was made with.
    extra: Extra,
    /// The terrain tool, the highlights it asks the map to draw (they go to the staging with the
    /// transient items) and what the terrain panel showed last.
    terrain: Tool,
    marks: Vec<Mark>,
    /// The tile a pending fence selects.
    selection: Option<Mark>,
    shown: Option<terrain::Shown>,
    /// The amount next to the pointer the log told last.
    pointed: Option<i64>,
    /// `[UI] confirmSell`, read when the game opens.
    confirm: bool,
    /// What the bulldozer would remove now and the removal that is under way.
    dozer: bulldozer::Dozer,
    sweep: Option<bulldozer::Sweep>,
    /// The new exhibits that wait for a name, the first one's dialog being open or not.
    naming: Vec<u32>,
    naming_open: bool,
    /// The new tank whose objects the player is asked about (the question is open).
    refused: Option<u32>,
    /// The pieces of the drag after the one that closed an exhibit, bought once it is named.
    rest: Vec<Pending>,
}

impl Game {
    /// What the cost text at the pointer shows: the drag's, else the pending object's, else, in
    /// the paint tab, minus what painting the brush costs while the pointer is over the map. A
    /// gate shows its price as it is, not as a charge (placing 4.5).
    fn cost(&self) -> Option<i64> {
        if self.build.bulldozing() {
            return self.dozer_amount();
        }
        if self.gate.as_ref().is_some_and(|g| g.hidden) {
            return None;
        }
        match self.run.is_empty() {
            true => self
                .pending
                .as_ref()
                .map(|p| match self.gate {
                    Some(_) => p.amount().saturating_neg(),
                    None => p.amount(),
                })
                .or_else(|| self.terrain.pointer_amount().filter(|_| self.hover)),
            false => Some(place::run_amount(&self.run)),
        }
    }
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
    /// What the map view's tool and the world did in the last pass, one line each (`tool down
    /// 12,40`, `world stage`).
    tool_lines: Vec<String>,
    world_lines: Vec<String>,
    /// What the exhibits did in the last pass, and the messages about gates that wait to be shown.
    exhibit_lines: Vec<String>,
    exhibit_messages: Vec<(u32, i32)>,
    /// What the terrain tool did in the last pass.
    terrain_lines: Vec<String>,
    /// What the cash and the accounts did and has not been logged yet, and what they did in the
    /// last pass.
    money_events: Vec<String>,
    money_lines: Vec<String>,
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
    /// A question of the bulldozer: any answer goes back to it.
    Sweep,
    /// The objects a new tank does not allow: Yes sells and moves them, anything else undoes
    /// the operation.
    Tank,
}

/// The layout of the question boxes with a message, the main panel and the options panel, and the
/// buttons of the options panel that leave the game.
const QUESTION: i32 = 45000;
const MAIN_PANEL: i32 = 2;
/// The cash on the status bar.
const STATUS_MONEY: i32 = 1016;
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
        app.open_game(zoo, Money::default())?;
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
            tool_lines: Vec::new(),
            world_lines: Vec::new(),
            exhibit_lines: Vec::new(),
            exhibit_messages: Vec::new(),
            terrain_lines: Vec::new(),
            money_events: Vec::new(),
            money_lines: Vec::new(),
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
    fn open_game(&mut self, zoo: Zoo, money: Money) -> Result<(), RenderError> {
        let size = (self.frame.width as i32, self.frame.height as i32);
        let mut camera = Camera::open(&zoo.view, size, self.settings);
        if self.fullscreen {
            camera.set_fullscreen(&zoo, true);
        }
        let brush = (zoo.view.brush & 0xff) as usize;
        let mut world = World::new(zoo, |e| self.renderer.shape(e));
        let staged = self.renderer.stage(&world.zoo, camera.rotation, false)?;
        self.drawn_at = Some((camera.scroll, camera.zoom, camera.rotation, false));
        self.painted = None;
        self.timers.forget_simulation();
        let types = crate::terrain::types(self.renderer.vfs());
        let water = place::water(&types);
        world.set_water(water.clone());
        world.set_tank_config(crate::exhibit::TankConfig::load(self.renderer.vfs()));
        let exhibits = world.exhibits().list().len();
        // The terrain brush starts at the size the file stores (terrain editing 4.1); a size
        // outside 1 to 5 leaves the default.
        let mut terrain = Tool::new(
            types.clone(),
            crate::terrain::elevation_cost(self.renderer.vfs().ini()),
        );
        terrain.set_size(brush);
        self.game = Some(Game {
            world,
            staged,
            camera,
            calendar: Calendar::new(),
            paused: false,
            grid: false,
            choice: false,
            money,
            told: None,
            bar: None,
            finances: Finances::default(),
            build: Build::new(installed_expansions(self.renderer.vfs())).with_exhibits(exhibits),
            pending: None,
            gate: None,
            gate_over: None,
            run: Vec::new(),
            hover: false,
            tints: Tints::load(self.renderer.vfs()),
            water,
            smart: place::smart_fences(self.renderer.vfs().ini()),
            extra: Extra::default(),
            terrain,
            marks: Vec::new(),
            selection: None,
            shown: None,
            pointed: None,
            confirm: place::confirm_sell(self.renderer.vfs().ini()),
            dozer: bulldozer::Dozer::default(),
            sweep: None,
            naming: Vec::new(),
            naming_open: false,
            refused: None,
            rest: Vec::new(),
        });
        if let Some(ui) = &mut self.ui {
            Build::first_tabs(ui);
        }
        Ok(())
    }

    /// Gives the game the cash and the accounts of its file (or those a run sets).
    pub fn set_money(&mut self, money: Money) {
        if let Some(game) = &mut self.game {
            game.money = money;
        }
    }

    /// Puts the interface around the game. The window is the size it was resolved for.
    pub fn set_interface(&mut self, ui: Interface) {
        self.map = Framebuffer::new(self.frame.width, self.frame.height);
        self.painted = None;
        self.ui = Some(ui);
        if let Some(game) = &mut self.game {
            game.finances = Finances::default();
        }
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
        (self.tool_lines, self.world_lines) = (Vec::new(), Vec::new());
        self.exhibit_lines = Vec::new();
        self.terrain_lines = Vec::new();
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
            self.terrain_help();
            self.restage();
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
                game.camera.edge_scroll(&game.world.zoo, now, self.pointer);
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
            && game.calendar.advance(step)
        {
            game.money.month_change();
            self.money_events.push(format!(
                "month {} year {}",
                game.money.month, game.money.year
            ));
        }
        self.tell_money(now);
        self.menu_lines = std::mem::take(&mut self.menu_events);
        self.drew.then_some(&self.frame)
    }

    /// Tells what the cash did since it was last told: the amount in whole units for the log,
    /// and the status bar, which shows it with its colour at every change.
    fn tell_money(&mut self, now: u64) {
        if let Some(game) = &mut self.game {
            let (units, tone) = game.money.bar();
            if game.told != Some(units) {
                game.told = Some(units);
                self.money_events.push(format!("cash {units}"));
            }
            if let Some(ui) = &mut self.ui {
                let text = Symbol::new(ui.string(SYMBOL)).text(units);
                let shown = Some((text, tone));
                if game.bar != shown {
                    let (text, _) = shown.as_ref().expect("just made");
                    ui.set_text(STATUS_MONEY, &to_cp1252(text));
                    ui.set_fore_color(STATUS_MONEY, tone.rgb());
                    self.money_events
                        .push(format!("bar {text} {}", tone.name()));
                    game.bar = shown;
                }
                let updated = self.fired.interface.is_some();
                let lines = game.finances.refresh(&game.money, ui, now, updated);
                self.money_events.extend(lines);
            }
        }
        self.money_lines = std::mem::take(&mut self.money_events);
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
                return self.cancel(true);
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
            (Command::Mouse(mouse), game) => {
                // A press or release that belongs to the map view is logged with the tile it is
                // on, and the terrain tool, if it is the map view's tool, acts on the mouse.
                let on_map = self.ui.as_mut().is_some_and(|ui| ui.mouse(mouse));
                let phase = match mouse {
                    Mouse::Down | Mouse::Double => Some("down"),
                    Mouse::Up => Some("up"),
                    _ => None,
                };
                if on_map && mouse == Mouse::Right {
                    return self.cancel(false);
                }
                if let (true, Mouse::Down | Mouse::Double, Some(game)) =
                    (on_map, mouse, game.as_mut())
                {
                    // The next operation starts a new undo list.
                    game.world.begin();
                    if let Some(ui) = &self.ui {
                        let tile = game.camera.tile_under(&game.world.zoo, ui.pointer());
                        let (x, y, _) = game.camera.pick(&game.world.zoo, ui.pointer());
                        game.build.press(self.renderer.catalogue(), tile, (x, y));
                        game.build.sweep_press(tile, ui.pointer());
                        if game.build.dragging() {
                            game.selection = None;
                        }
                    }
                }
                // A release that does not end a press on the map ends a drag without buying.
                if let (false, Mouse::Up, Some(game)) = (on_map, mouse, game.as_mut()) {
                    game.build.end_drag();
                    game.build.end_sweep();
                    game.run.clear();
                }
                if let (true, Some(phase), Some(game), Some(ui)) =
                    (on_map, phase, game.as_mut(), &self.ui)
                {
                    let tile = game.camera.tile_under(&game.world.zoo, ui.pointer());
                    let tile = tile.map_or("none".to_string(), |(x, y)| format!("{x},{y}"));
                    self.tool_lines.push(format!("tool {phase} {tile}"));
                }
                if let Mouse::Move(..) = mouse {
                    if let Some(game) = game.as_mut() {
                        game.hover = on_map;
                    }
                    self.aim(true);
                } else if on_map && mouse == Mouse::Up {
                    self.release();
                }
                self.terrain_mouse(mouse, on_map);
            }
            (Command::Scroll(x, y), Some(game)) if keys => {
                game.camera.key_scroll(&game.world.zoo, (x, y));
            }
            (Command::Rotate(clockwise), Some(game)) if keys => {
                let before = game.camera.clone();
                game.camera.rotate(&game.world.zoo, None, clockwise);
                // Restage from the new side. If that fails the view stays as it was.
                let extra = Extra {
                    marks: game.marks.clone(),
                    ..Extra::default()
                };
                match self
                    .renderer
                    .stage_with(&game.world.zoo, game.camera.rotation, false, &extra)
                {
                    Ok(staged) => {
                        game.staged = staged;
                        game.extra = extra;
                    }
                    Err(_) => game.camera = before,
                }
            }
            (Command::Zoom(zoom_in), Some(game)) if keys => {
                game.camera.zoom(&game.world.zoo, None, zoom_in);
            }
            (Command::Grid, Some(game)) if keys => {
                game.grid = !game.grid;
                game.choice = game.grid;
            }
            (Command::Fullscreen(on), Some(game)) => {
                game.camera.set_fullscreen(&game.world.zoo, on)
            }
            // The Pause key works through a question too, but the question gives the flag back as
            // it was, so it is left alone there.
            (Command::Pause, Some(game)) if shown && !modal => game.paused = !game.paused,
            (Command::Space, Some(game)) if keys => game.paused = !game.paused,
            // The brush keys press the plus and minus buttons of the terrain panel (input 10).
            (Command::Char(b'+' | b'-'), Some(_)) if keys => {
                if let (Some(ui), Command::Char(key)) = (&mut self.ui, command) {
                    ui.press_button(if key == b'+' { PLUS } else { MINUS });
                }
            }
            // The Delete key presses the clear button, the bulldozer (input 10).
            (Command::Key(Key::Delete, ..), Some(_)) if keys => {
                if let Some(ui) = &mut self.ui {
                    ui.press_button(BULLDOZER);
                }
            }
            // The keys and characters go to the focused field or list.
            (Command::Key(key, _, shift), _) => {
                if let Some(ui) = &mut self.ui {
                    ui.key(key, shift);
                }
            }
            // The Z key presses the button of the zoo status window (input 10).
            (Command::Char(b'z' | b'Z'), Some(_)) if keys => {
                if let Some(ui) = &mut self.ui {
                    ui.press_button(STATUS_BUTTON);
                }
            }
            // The `$` key donates (input 4.8).
            (Command::Char(b'$'), Some(game)) if keys && !game.paused => {
                game.money.refund(DONATIONS, DOLLAR_KEY);
            }
            (Command::Char(byte), _) => {
                if let Some(ui) = &mut self.ui {
                    ui.char(byte);
                }
            }
            // The map view's keys while it does not have them or there is no game.
            _ => {}
        }
        // The view turned or zoomed under the pointer: the pending object is another one.
        if matches!(command, Command::Rotate(_) | Command::Zoom(_)) {
            self.aim(false);
        }
    }

    /// Works out the pending object from the tool, the panel's rotation step and the pointer, and
    /// has the next draw show it. With `log`, a move over the map tells it: `tool pending TYPE
    /// X,Y q N valid`, the tile under the snapped point, the quadrant of the point in it and the
    /// verdict (`valid`, `overwrite`, `highlight` or `invalid`).
    fn aim(&mut self, log: bool) {
        self.aim_pending(log);
        self.dozer_aim();
    }

    /// The pending object part of [`App::aim`].
    fn aim_pending(&mut self, log: bool) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let cat = self.renderer.catalogue();
        let before = game.pending.take();
        let habits = |e: &Entity| habit(cat, e);
        let dirt = place::dirt(&game.world, game.terrain.types(), &habits);
        let (step, view) = (game.build.step(), game.camera.rotation);
        let at = |ty, point| {
            place::pending(
                &game.world,
                cat,
                &game.water,
                &dirt,
                game.smart,
                ty,
                step,
                view,
                point,
            )
        };
        let point = game.camera.pick(&game.world.zoo, ui.pointer());
        game.pending = game
            .build
            .tool()
            .filter(|_| game.hover)
            .and_then(|ty| at(ty, (point.0, point.1)));
        // The gate tool's pending gate sits on the fence piece under the pointer.
        // It is judged when the piece changes, so a gate just placed shows nothing until the
        // pointer has been over another piece.
        // It is the entity the pixel pick finds, widened as the bulldozer's (placing 6.4).
        let hovered = game
            .staged
            .pick(
                game.camera.view_point(ui.pointer()),
                place::bulldozer::WIDEN,
                |id| game.world.find(id).is_some(),
            )
            .and_then(|id| game.world.find(id));
        let over = hovered
            .and_then(|at| place::gate::over(&game.world, at))
            .filter(|_| game.build.gating() && game.hover);
        if over != game.gate_over || over.is_none() {
            game.gate_over = over;
            game.gate = hovered
                .filter(|_| over.is_some())
                .and_then(|at| place::gate::pending(&game.world, cat, &game.money, at));
        }
        if let Some(g) = &game.gate {
            game.pending = Some(g.pending.clone());
        }
        // A pending fence selects the tile under the pointer, before snapping; a fence drag in
        // progress does not draw it (placing 4.5, 6.2 items 1 and 2).
        let fence = game
            .build
            .tool()
            .and_then(|ty| cat.types.get(ty))
            .is_some_and(|t| Class::of(t) == Class::Fence);
        game.selection = (fence && game.hover && !game.build.dragging())
            .then(|| {
                let (x, y) = (point.0.div_euclid(64), point.1.div_euclid(64));
                let tile = game
                    .world
                    .tile_index(i32::try_from(x).ok()?, i32::try_from(y).ok()?)?;
                Some(Mark {
                    tile,
                    color: crate::terrain::SELECTION,
                })
            })
            .flatten();
        // A drag lays a piece on each tile of its L; the pieces stay while the pointer is away.
        if let (Some(ty), Some(p), true) = (game.build.tool(), &game.pending, game.build.dragging())
        {
            let points = game.build.run(p.tile, (point.0, point.1), point.2);
            let mut run: Vec<Pending> = points.iter().filter_map(|&p| at(ty, p)).collect();
            place::afford(&mut run, &game.money);
            game.run = run;
        } else if !game.build.dragging() {
            game.run.clear();
        }
        if let (true, Some(p)) = (log, &game.pending) {
            let ty = &cat.types[p.ty];
            self.tool_lines.push(format!(
                "tool pending {}/{}/{} {},{} q {} {}",
                ty.class,
                String::from_utf8_lossy(&ty.type_name),
                String::from_utf8_lossy(&ty.subtype),
                p.tile.0,
                p.tile.1,
                p.quadrant,
                p.verdict.word(),
            ));
        }
        // The cost text follows the pointer: the frame is painted again.
        if before.is_some() || game.pending.is_some() {
            ui.touch();
        }
    }

    /// A release on the map (the view sees only one that ends a press on it): the pieces of a path
    /// drag, or else the pending object, are bought in order. A valid place gets its object, which
    /// is paid and logged (`world add objects/foliage/doumpalm 12,40 id 14908 quadrants 1`, with
    /// the number of quadrants the object holds; a path on water
    /// logs `world paint 12,40 3` first, one on another path `world remove paths/... 12,40 id 7`;
    /// a fence is told with its side, `world add fences/... 12,40 side 2 id 8`, and the piece it
    /// replaces `world remove fences/... 12,40 side 2 id 7`).
    /// One sound is asked for the release: the placement sound if anything was placed, else the
    /// refusal sound; a type that turns by itself turns once; the last message of a piece that
    /// could not be placed shows in the message line (`tool message 10311`), and one that gives no
    /// message when nothing at all was placed logs `tool refused`; and a new pending
    /// object is made (placing 6.1 and 6.2).
    fn release(&mut self) {
        let Some(game) = &mut self.game else {
            return;
        };
        if game.build.bulldozing() {
            return self.bulldoze_release();
        }
        if game.build.gating() {
            return self.gate_release();
        }
        let dragging = game.build.dragging();
        game.build.end_drag();
        let run = std::mem::take(&mut game.run);
        let pieces = match dragging && !run.is_empty() {
            true => run,
            false => game.pending.clone().into_iter().collect(),
        };
        self.buy(pieces, false);
    }

    /// Buys the pieces in order (`release`); `resumed` when they are what a closing piece left
    /// of the drag, whose sound and turn were done. A piece that makes an exhibit stops the
    /// buying: its name is asked first, the rest of the pieces come after the answer.
    fn buy(&mut self, pieces: Vec<Pending>, resumed: bool) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        if pieces.is_empty() {
            return;
        }
        game.world
            .set_exhibit_words([0, 1, 2].map(|k| ui.string(exhibits::WORD + k)));
        let renderer = &self.renderer;
        let cat = renderer.catalogue();
        let (mut first, mut refused) = (None, None);
        for (n, p) in pieces.iter().enumerate() {
            let ty = &cat.types[p.ty];
            let name = format!(
                "{}/{}/{}",
                ty.class,
                String::from_utf8_lossy(&ty.type_name),
                String::from_utf8_lossy(&ty.subtype)
            );
            let (x, y) = p.tile;
            let fence = Class::of(ty) == Class::Fence;
            let at = game.world.tile_index(x, y);
            let old = match ty.class {
                "fences" | "tankwall" => at.and_then(|t| game.world.edge(t, p.facing as usize)),
                "paths" => at.and_then(|t| game.world.path(t)),
                _ => None,
            };
            let old = old.and_then(|o| game.world.zoo.entities[o].common.as_ref().map(|c| c.id));
            // A fence is told with its side.
            let tile = match fence {
                true => format!("{x},{y} side {}", p.facing),
                false => format!("{x},{y}"),
            };
            let water = &game.water;
            let colors = renderer.default_colors(ty);
            let outcome = place::place(
                &mut game.world,
                &mut game.money,
                ty,
                p,
                water,
                colors,
                |e| renderer.shape(e),
            );
            match outcome {
                Outcome::Placed(id) => {
                    if let Some(kind) = p.paint {
                        self.world_lines.push(format!("paint {x},{y} {kind}"));
                    }
                    if let Some(old) = old {
                        let line = format!("remove {name} {tile} id {old}");
                        self.world_lines.push(line);
                    }
                    // An object that takes quadrants says how many it holds.
                    let held = match Class::of(ty) == Class::Fixed {
                        true => game.world.find(id).map(|e| game.world.held(e)),
                        false => None,
                    };
                    let held = held.map_or(String::new(), |n| format!(" quadrants {n}"));
                    let line = format!("add {name} {tile} id {id}{held}");
                    self.world_lines.push(line);
                    first = first.or(Some(p.ty));
                    if !game.world.waiting().is_empty() {
                        game.rest = pieces[n + 1..].to_vec();
                        break;
                    }
                }
                Outcome::Refused(message) => refused = message.or(refused),
            }
        }
        match first {
            _ if resumed => {}
            Some(ty) => {
                self.ui_sounds.push("place");
                if game.build.placed(&cat.types[ty]) {
                    self.tool_lines
                        .push(format!("tool turn {}", game.build.step()));
                }
            }
            None => self.ui_sounds.push("noplace"),
        }
        match refused {
            Some(id) => {
                ui.message(&to_cp1252(&ui.string(id)));
                self.tool_lines.push(format!("tool message {id}"));
            }
            // A refusal without a message (ground that is not flat, a quadrant off the zoo).
            None if first.is_none() => self.tool_lines.push("tool refused".into()),
            None => {}
        }
        self.exhibits_told();
        self.aim(false);
    }

    /// The menu and the game act on what the player did since the last look: the answers to the
    /// questions, the actions of the lists and the cash field, the buttons selected.
    fn acts(&mut self) {
        let Some(ui) = &mut self.ui else { return };
        let selected = ui.take_selected();
        self.build(None);
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
                _ if self.game.is_some() => self.build(Some(id)),
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

    /// The building panels notice what the player did to them: the panel shown or hidden, and
    /// the button `selected`, if any.
    fn build(&mut self, selected: Option<i32>) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        let told = self.tool_lines.len();
        game.build.sync(ui, vfs, cat, &mut self.tool_lines);
        if let Some(id) = selected {
            game.build.selected(ui, vfs, cat, id, &mut self.tool_lines);
            self.terrain_pressed(id);
        }
        self.terrain_sync();
        // Every change of the tool or the step tells a line.
        if self.tool_lines.len() != told {
            self.aim(false);
        }
    }

    /// What the menu's last button asked of the game: open a file, or tell the player something.
    fn requested(&mut self) {
        match self.menu.as_mut().and_then(Menu::take_request) {
            Some(Request::Open(start)) => {
                let Some(menu) = &self.menu else { return };
                let zoo = menu.load(&start.path, self.renderer.catalogue());
                let opened = zoo
                    .and_then(|(zoo, money)| self.open_game(zoo, money).map_err(|e| e.to_string()));
                match opened {
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
        if let Some(game) = &mut self.game {
            game.money.start(start.cash, start.reset);
            for (name, amount) in [("set", start.cash.set), ("add", start.cash.add)] {
                if let Some(amount) = amount {
                    self.money_events.push(format!("{name} {amount}"));
                }
            }
        }
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
            // The bulldozer and the tanks ask their own questions.
            Asked::Sweep | Asked::Tank => return,
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
        if self.game.as_ref().is_some_and(|g| g.naming_open) {
            return self.named();
        }
        let Some(kind) = self.asked.take() else {
            return;
        };
        if kind == Asked::Sweep {
            return self.sweep_answered(code);
        }
        if kind == Asked::Tank {
            return self.tank_answered(code);
        }
        match code {
            6 => self.menu_events.push("game save unavailable".to_string()),
            7 => self.proceed(kind),
            _ => {}
        }
    }

    fn proceed(&mut self, kind: Asked) {
        match kind {
            Asked::Sweep | Asked::Tank => {}
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
            if let Some(game) = &mut self.game {
                game.sweep = None;
            }
            if self.game.as_ref().is_some_and(|g| g.refused.is_some()) {
                self.tank_no();
            }
            self.named();
        }
        self.ask(Asked::Exit);
    }

    /// Escape, with the map view having the keys, or the right button released over the map
    /// (`ask` is false): the first of these that applies acts (input 4.6): the tool of the
    /// building panels is dropped, then the open panels close, and only Escape then asks the
    /// question that leads to the menu.
    fn cancel(&mut self, ask: bool) {
        if let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) {
            if game.build.cancel(ui, &mut self.tool_lines) {
                self.terrain_sync();
                return self.aim(false);
            }
            // The main panel is part of the game screen: only the panels opened from it close.
            if let Some(i) = ui.tree.find(OPTIONS_PANEL)
                && ui.tree.elements[i].state & crate::ui::layout::HIDDEN == 0
            {
                ui.tree.hide(i);
                ui.touch();
                return;
            }
        }
        if ask {
            self.ask(Asked::Leave);
        }
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
        for id in [
            OPTIONS_PANEL,
            MAIN_PANEL,
            crate::build::CONSTRUCTION,
            crate::build::OBJECTS,
        ] {
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
            // The cost of the pending object or of the brush, next to the pointer (placing 4.5,
            // terrain editing 7.3).
            if let Some(amount) = self.game.as_ref().and_then(Game::cost) {
                let text = place::cost_text(ui.string(SYMBOL), amount);
                ui.pointer_text(&mut self.frame, &to_cp1252(&text), COST_OFFSET);
            }
        }
        true
    }

    /// Stages the map again when an edit changed the world since the last draw: the picture is
    /// the final state at once. A map that cannot be staged keeps the old picture.
    fn restage(&mut self) {
        let dirty = match &mut self.game {
            Some(game) => game.world.take_dirty(),
            None => return,
        };
        // An edit may change what the place under the pointer is worth.
        if dirty {
            self.aim(false);
        }
        let Some(game) = &mut self.game else { return };
        // The pending object and the terrain highlights are transient items: the picture is made
        // again when they changed.
        let mut want = match game.run.is_empty() {
            true => game
                .pending
                .as_ref()
                .map_or_else(Extra::default, |p| p.extra(&game.tints)),
            false => place::extra_of(&game.run, &game.tints),
        };
        want.marks.clone_from(&game.marks);
        want.marks.extend(game.selection);
        game.dozer_extra(&mut want);
        if !dirty && want == game.extra {
            return;
        }
        match self
            .renderer
            .stage_with(&game.world.zoo, game.camera.rotation, false, &want)
        {
            Ok(staged) => {
                game.staged = staged;
                self.painted = None;
                if dirty {
                    self.world_lines.push("stage".to_string());
                }
            }
            Err(why) => self.world_lines.push(format!("stage failed: {why}")),
        }
        game.extra = want;
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

    /// The cash and the accounts of the game; `None` in the menu.
    pub fn money(&self) -> Option<&Money> {
        self.game.as_ref().map(|g| &g.money)
    }

    pub fn paused(&self) -> bool {
        self.modal() || self.game.as_ref().is_some_and(|g| g.paused)
    }

    /// Whether the player asked to close the game.
    pub fn quit(&self) -> bool {
        self.quit
    }

    /// The sounds the interface and the placing of objects asked for in the last pass, by file
    /// name without the extension (`uiclick`, `place`, `noplace`, `sounds/uiclick.wav`): the shell plays them.
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
    /// then `tool down X,Y` or `tool up X,Y` for a press or release on the map view (the tile, or `none`),
    /// then `world stage` when an edit made the draw stage the map again,
    /// then `terrain enter paint|modify`, `terrain brush SIZE X,Y`, `terrain size N`, `terrain type N`,
    /// `terrain paint N`, `terrain cost N`, `terrain accept N`, `terrain cancel N`, `terrain leave`
    /// and `terrain short` for what the terrain tool did,
    /// then `money cash N` (whole units) when the cash changed, `money bar TEXT COLOUR` when the
    /// status bar was written, `money set N` and `money add N` when a start set or added cash and
    /// `money month M year Y` at a month change, `money cell ID RRGGBB TEXT` for each cell of the
    /// Finances table that changed when the table was written,
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
        lines.extend(self.tool_lines.iter().map(|e| format!("{lead} {e}")));
        lines.extend(self.world_lines.iter().map(|e| format!("{lead} world {e}")));
        lines.extend(
            self.exhibit_lines
                .iter()
                .map(|e| format!("{lead} exhibit {e}")),
        );
        lines.extend(
            self.terrain_lines
                .iter()
                .map(|e| format!("{lead} terrain {e}")),
        );
        lines.extend(self.money_lines.iter().map(|e| format!("{lead} money {e}")));
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

mod bulldozer;
mod exhibits;
mod gates;
mod terrain;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod start_tests;

#[cfg(test)]
mod terrain_tests;

#[cfg(test)]
mod bulldozer_tests;

#[cfg(test)]
mod money_tests;

//! The game state of the map view and its pass: the map, the staged picture, the camera, the
//! timers, the simulation and the pause flag, driven by [`Command`]s. Before a game is started (the
//! main menu) there is no map: the pass runs the timers and the interface, and the menu acts on
//! what the player selects. No window and no input
//! events: a shell turns what the player does into commands and presents the frames that
//! [`App::pass`] paints, so a windowed run and a scripted one share every line of this.
//!
//! One pass applies the commands first (in the original they arrive between two passes, main
//! loop 3.1), then runs the timers in their order: the draw, the interface (the camera's edge
//! scroll), the simulation (the managers of [`Sim`]; the pause stops it, main loop 3.3).
//!
//! With an [`Interface`] the tree is painted over the map: the map view element draws the staged
//! map into its rectangle, and a due draw paints when the camera moved or the interface changed
//! (a timed element hid itself, an animation showed another frame, ...).
//! The keys and the edge scroll of the map view then act only while the map view is shown and no
//! other element has the keyboard focus. Without one, the frame is the map alone.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use crate::Framebuffer;
use crate::behaviour::{Library, Timings};
use crate::build::terrain::{MINUS, PLUS};
use crate::build::{BULLDOZER, Build};
use crate::camera::{Camera, Settings, zoom_view};
use crate::config::Config;
use crate::exhibit::rating;
use crate::menu::{self, Menu, Request, Start};
use crate::money::{DOLLAR_KEY, DONATIONS, Money, SYMBOL, Symbol, Tone};
use crate::music::{Music, positional};
use crate::panel::building as panel_building;
use crate::panel::{animal, exhibit, guest, staff};
use crate::place::{self, Class, Outcome, Pending, Tints};
use crate::render::order::project;
use crate::render::{Extra, Form, MapRenderer, Mark, RenderError, Shown, Staged, check_size};
use crate::scenario::Scenario;
use crate::sim::Sim;
use crate::status::{Finances, STATUS_BUTTON, Words, awards::Awards};
use crate::terrain::{Tool, habit};
use crate::text::format::{self, Arg};
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
    S,
    L,
}

const KEY_NAMES: [(&str, Key); 17] = [
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
    ("s", Key::S),
    ("l", Key::L),
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
/// `zoom out`, `grid` (toggle the tile grid), `devmode` (Ctrl+D), `fullscreen on`, `fullscreen off`, `pause`, `space`, `quit` (end the run), `close` (the
/// window is closed) for the map view, `skip` (any key
/// or button press, which stops an intro movie), and for the
/// interface `mouse X Y`, `down`, `up`, `double` (the second press of a double click), `right` (the right button went up), `wheel N`, `key NAME [ctrl] [shift]` (NAME: enter, escape,
/// tab, backspace, delete, left, right, up, down, home, end, pageup, pagedown, y, n, s, l; Ctrl+S saves and Ctrl+L loads) and
/// `char C` (a byte, as a number from 0 to 255 or as one character that is not a digit).
/// `save FILE` writes the game to a file and `load FILE` opens a saved game as Continue does (the
/// file name runs to the end of the line; the name is kept for the rest of the run).
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
    /// Ctrl+D: switch the developer mode on or off, if the ini allows it (main loop 5.2).
    DevMode,
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
    /// `award add ID`: the zoo holds the award (`awards.md` 4.1).
    Award(i32),
    Save(&'static str),
    Load(&'static str),
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
            Command::DevMode => f.write_str("devmode"),
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
            Command::Award(id) => write!(f, "award add {id}"),
            Command::Save(file) => write!(f, "save {file}"),
            Command::Load(file) => write!(f, "load {file}"),
        }
    }
}

impl FromStr for Command {
    type Err = String;

    fn from_str(text: &str) -> Result<Command, String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let number = |w: &str| w.parse::<i32>().map_err(|_| format!("not a number: {w:?}"));
        let file = |word: &str| match text.trim().strip_prefix(word).map(str::trim) {
            Some(file) if !file.is_empty() => Ok(&*Box::leak(file.to_string().into_boxed_str())),
            _ => Err(format!("{word} takes a file name: {text:?}")),
        };
        Ok(match words.as_slice() {
            ["award", "add", id] => Command::Award(number(id)?),
            ["save", ..] => Command::Save(file("save")?),
            ["load", ..] => Command::Load(file("load")?),
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
            ["devmode"] => Command::DevMode,
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

/// A sound a unit's animation began: the member of the file (behaviours 5.2), the place of the
/// unit in the view space, and the distance term and the pan of that place from the centre of the
/// map view (sound 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitSound {
    pub id: u32,
    pub file: String,
    /// The voice of the type and key, and the base term of the sound (sound 4.4, 9.1).
    pub voice: String,
    pub base: i32,
    pub at: (i64, i64),
    pub distance: i32,
    pub pan: i32,
}

/// A zoo being played.
struct Game {
    world: World,
    staged: Staged,
    camera: Camera,
    paused: bool,
    /// The tile grid is drawn (Ctrl+G; the terrain tools force it on), and what the player chose
    /// with Ctrl+G, which the grid goes back to when the tools are left (input 4.8).
    grid: bool,
    choice: bool,
    /// The cash and the accounts, and what of them the log and the status bar showed last.
    money: Money,
    told: Option<i64>,
    bar: Option<(String, Tone)>,
    /// The three bars of the status bar as last written.
    bars: Option<[i32; 3]>,
    /// What the Finances table of the zoo status window holds.
    finances: Finances,
    /// Whether the status window was shown at the last look, for the Awards page.
    awards_page: Awards,
    /// The building panels and the tool they choose.
    build: Build,
    /// The pending object: the tool at the place under the pointer, while the pointer (as the
    /// last move left it) is over the map view and a tool is chosen.
    pending: Option<Pending>,
    /// The pending staff member made when its item was chosen (staff 4.2): the type and the
    /// counters it drew; the hire gives them to the unit.
    made: Option<(usize, crate::unit::Staff)>,
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
    /// The Undo button is enabled.
    undoable: bool,
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
    /// The exhibit panel and the recommendations plaque.
    panel: crate::panel::exhibit::Panel,
    /// The animal panel and the animal selected.
    animal: crate::panel::animal::Panel,
    /// The staff panel and the staff member selected.
    staff: crate::panel::staff::Panel,
    /// The guest panel and the guest selected.
    guest: crate::panel::guest::Panel,
    /// The building panels and the building selected.
    building: crate::panel::building::Panel,
    /// The unit the player selected, and the one it carries (unit placing 8).
    selected: Option<u32>,
    carry: Option<place::unit::Carry>,
    /// The building it carries, when what it carries is one (unit placing 8.7).
    held: Option<crate::building::Held>,
}

impl Game {
    /// The animation and frame each unit holds, for the picture.
    fn shown(&self) -> Vec<Shown> {
        (self.world.units.iter())
            .filter(|u| !u.animation.name.is_empty())
            .map(|u| Shown {
                id: u.id,
                animation: u.animation.name.clone(),
                frame: u.animation.shown(),
            })
            .chain(self.world.units.gate_frames().map(|(id, frame)| Shown {
                id,
                animation: "idle".into(),
                frame,
            }))
            .collect()
    }

    /// The Pause key and the space bar.
    fn pause(&mut self) {
        self.paused = !self.paused;
        self.world.set_paused(self.paused);
    }

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
                .map(|p| match &self.gate {
                    Some(g) => g.amount(),
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
    /// Whether the pointer is over the map view after the last pass (see [`App::cursor`]).
    on_map: bool,
    /// Whether it is over an editable text field.
    on_text: bool,
    /// Whether the last pass began a long operation: rotating or zooming a map, loading a game.
    busy: bool,
    /// Told when a long operation begins, so the window can show the busy cursor around it.
    busy_hook: Option<Box<dyn FnMut() + 'a>>,
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
    /// What the information panels did in the last pass.
    panel_lines: Vec<String>,
    /// What the terrain tool did in the last pass.
    terrain_lines: Vec<String>,
    /// What the cash and the accounts did and has not been logged yet, and what they did in the
    /// last pass.
    money_events: Vec<String>,
    money_lines: Vec<String>,
    /// The managers of the simulation and the `sim` lines of the last pass.
    sim: Sim,
    sim_lines: Vec<String>,
    /// The developer mode and speed changes of the last pass (`dev on`, `dev speed 1.1`).
    dev_lines: Vec<String>,
    unit_lines: Vec<String>,
    animal_lines: Vec<String>,
    guest_lines: Vec<String>,
    staff_lines: Vec<String>,
    building_lines: Vec<String>,
    /// The sounds the units' animations began in the last pass.
    unit_sounds: Vec<UnitSound>,
    /// The sounds of a type's own that a selection or a pick-up asked for in this pass, not placed
    /// in the view: they join the unit sounds at its end (unit placing 8.1, 8.2).
    type_sounds: Vec<UnitSound>,
    /// The frames and delays of the animations the units played.
    timings: Timings,
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
    /// Whether the end dialog of a scenario is open.
    ending: bool,
    /// The save dialog, while it is open, and the list of saved games, while it is shown.
    saving: Option<saves::Saving>,
    listing: Option<saves::Listing>,
    /// The folder of the settings a game without the menu writes `[user] lastfile` to (none: not
    /// written), and that file: the menu keeps both for itself.
    user: Option<PathBuf>,
    last_file: Option<PathBuf>,
    /// Whether a fade of the menu music has begun in this run (only the first one waits), and
    /// whether a game opens without the main panel.
    faded: bool,
    bare: bool,
    /// What the scenario's triggers changed in the interface.
    marks: crate::scenario::Marks,
    scenario_lines: Vec<String>,
}

/// The layouts of the game screen that stopping or ending a game hides.
const GAME_LAYOUTS: [i32; 5] = [
    OPTIONS_PANEL,
    MAIN_PANEL,
    crate::build::CONSTRUCTION,
    crate::build::OBJECTS,
    crate::menu::FREEFORM_LAYOUT,
];

/// What an open question leads to when it is answered No: stopping the game for the menu, or
/// ending the program (main menu 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Leave,
    Exit,
    /// Load Game: the list of saved games follows.
    Load,
    /// A question of the bulldozer: any answer goes back to it.
    Sweep,
    /// The objects a new tank does not allow: Yes sells and moves them, anything else undoes
    /// the operation.
    Tank,
}

/// The string that starts the default name of a guest.
const GUEST_NAME: i32 = 9507;
/// The layout of the question boxes with a message, the main panel and the options panel, and the
/// buttons of the options panel that leave the game.
const QUESTION: i32 = 45000;
const MAIN_PANEL: i32 = 2;
/// The cash on the status bar, and its bars of animal happiness, guest happiness and zoo rating.
const STATUS_MONEY: i32 = 1016;
const STATUS_ANIMALS: i32 = 1011;
const STATUS_GUESTS: i32 = 1013;
const STATUS_ZOO: i32 = 1015;
/// The date on the status bar (its tooltip is the string of the same number), and the strings of
/// the patterns of the date and of the weekday in the tooltip.
const STATUS_DATE: i32 = 1030;
const DATE_PATTERN: i32 = 22012;
const DAY_PATTERN: i32 = 22013;
const OPTIONS_PANEL: i32 = 5;
const MAIN_MENU_BUTTON: i32 = 1503;
const EXIT_BUTTON: i32 = 1504;
/// The questions: save before starting a new zoo, before exiting.
const ASK_LEAVE: i32 = 153;
const ASK_EXIT: i32 = 151;
const ASK_LOAD: i32 = 152;
/// The Load Game and Save Game buttons of the options panel.
const LOAD_BUTTON: i32 = 1501;
const SAVE_BUTTON: i32 = 1502;

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
        let mut sim = Sim::new(renderer.vfs().ini());
        match renderer.vfs().read("economy.cfg") {
            Ok(economy) => sim.set_economy(&economy),
            Err(why) => eprintln!("rustyzoo: warning: economy.cfg: {why}; built-in settings used"),
        }
        let last_file = Config::parse(renderer.vfs().ini())
            .ok()
            .and_then(|ini| menu::last_path(&ini, renderer.vfs().root()));
        Ok(App {
            renderer,
            game: None,
            timers,
            quit: false,
            pointer: None,
            on_map: true,
            on_text: false,
            busy: false,
            busy_hook: None,
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
            panel_lines: Vec::new(),
            terrain_lines: Vec::new(),
            money_events: Vec::new(),
            money_lines: Vec::new(),
            sim,
            sim_lines: Vec::new(),
            dev_lines: Vec::new(),
            unit_lines: Vec::new(),
            animal_lines: Vec::new(),
            guest_lines: Vec::new(),
            staff_lines: Vec::new(),
            building_lines: Vec::new(),
            unit_sounds: Vec::new(),
            type_sounds: Vec::new(),
            timings: Timings::default(),
            music: None,
            music_changed: false,
            music_restarted: false,
            settings,
            fullscreen: false,
            asked: None,
            ending: false,
            saving: None,
            listing: None,
            user: None,
            last_file,
            faded: false,
            bare: false,
            marks: Default::default(),
            scenario_lines: Vec::new(),
        })
    }

    /// Opens a game on `zoo`: its stored camera, the first draw, and a simulation whose next step is
    /// the cap, as after the original's blocking load (main loop 3.5). A failure leaves the app
    /// as it was.
    fn open_game(&mut self, zoo: Zoo, money: Money) -> Result<(), RenderError> {
        self.open_game_as(zoo, money, false)
    }

    /// Opens a file the game read ([`App::open_game`]). A saved game (`saved`) whose scenario cannot
    /// be read is refused before anything changes: the load fails with the old-version message
    /// (research 7.2).
    fn open_zoo(&mut self, zoo: Zoo, money: Money, saved: bool) -> Result<(), String> {
        let unread = zoo
            .scenario_file()
            .filter(|_| saved)
            .is_some_and(|file| Scenario::prepare(self.renderer.vfs(), &file).is_none());
        if unread {
            return Err(crate::zoo::ZooError::TooOld(zoo.version).to_string());
        }
        self.open_game_as(zoo, money, saved)
            .map_err(|e| e.to_string())
    }

    /// [`App::open_game`] for a saved game when `saved`: the scenario it names builds the research
    /// tree and its research section is read (research 7.2, before the entities' own steps).
    fn open_game_as(&mut self, zoo: Zoo, money: Money, saved: bool) -> Result<(), RenderError> {
        let file = zoo.scenario_file().filter(|_| saved);
        let research = saved.then(|| (file.clone(), zoo.research().to_vec()));
        let goals = (zoo.scenario_states(), zoo.version >= 86);
        let size = (self.frame.width as i32, self.frame.height as i32);
        let mut camera = Camera::open(&zoo.view, size, self.settings);
        if self.fullscreen {
            camera.set_fullscreen(&zoo, true);
        }
        let brush = (zoo.view.brush & 0xff) as usize;
        let language = crate::strings::Language::of(self.renderer.vfs().root());
        let mut world = World::with_language(zoo, |e| self.renderer.shape(e), language);
        let staged = self.renderer.stage(&world.zoo, camera.rotation, false)?;
        self.drawn_at = Some((camera.scroll, camera.zoom, camera.rotation, false));
        self.painted = None;
        self.timers.forget_simulation();
        self.scenario_clean();
        self.sim
            .start_game(world.zoo.tiles.len(), world.zoo.clocks.clone());
        world.units.set_period(self.sim.ai.status_period);
        let tree = research.and_then(|(file, section)| self.loaded_research(file?, &section));
        world.units.set_ini(self.renderer.vfs().ini());
        if let Some(ui) = &self.ui {
            world.units.guest_name = ui.string(GUEST_NAME);
            world.units.names = self.type_names(ui);
            world.name_loaded_entities(self.renderer.catalogue());
        }
        let types = crate::terrain::types(self.renderer.vfs());
        let water = place::water(&types);
        world.set_water(water.clone());
        world.units.class = crate::animal::Class {
            reset_interval: self.sim.ai.animal_reset_interval,
            ..crate::animal::Class::load(self.renderer.vfs())
        };
        world.set_decay(crate::world::Decay::load(self.renderer.vfs()));
        self.sim.load(&mut world, self.renderer.catalogue());
        self.sim.fences_loaded(&world, self.renderer.catalogue());
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
            paused: false,
            grid: false,
            choice: false,
            money,
            told: None,
            bar: None,
            bars: None,
            finances: Finances::default(),
            awards_page: Awards::default(),
            build: Build::new(installed_expansions(self.renderer.vfs()))
                .with_exhibits(exhibits)
                .unlocked(
                    self.renderer.catalogue(),
                    &scenario_ui::reward_ini(&self.menu, self.renderer.vfs()),
                ),
            pending: None,
            made: None,
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
            undoable: false,
            dozer: bulldozer::Dozer::default(),
            sweep: None,
            naming: Vec::new(),
            naming_open: false,
            refused: None,
            rest: Vec::new(),
            panel: Default::default(),
            animal: Default::default(),
            staff: Default::default(),
            guest: Default::default(),
            building: Default::default(),
            selected: None,
            carry: None,
            held: None,
        });
        // The scenario resumes first (its triggers, then the goal states), then the research
        // section completes the programs over their availability, then the ini rewards (7.3).
        if let Some(scenario) = file.and_then(|f| Scenario::prepare(self.renderer.vfs(), &f)) {
            self.sim.resume_scenario(scenario, &goals.0, goals.1);
            self.scenario_avail(false);
        }
        if let (Some(tree), Some(game)) = (tree, &mut self.game) {
            let ini = Config::parse(self.renderer.vfs().ini()).unwrap_or_default();
            game.build.start_research(self.renderer.catalogue(), &tree);
            game.build
                .after_triggers(self.renderer.catalogue(), &ini, &[]);
            self.sim.set_research(tree);
        }
        if let Some(ui) = &mut self.ui {
            Build::first_tabs(ui);
        }
        Ok(())
    }

    /// The rating check that ends every load (zoo status 16).
    fn loaded_check(&mut self) {
        if let Some(game) = &mut self.game {
            let cat = self.renderer.catalogue();
            self.sim.loaded(&game.world, cat, &mut game.money);
        }
    }

    /// Gives the game the cash and the accounts of its file (or those a run sets); the load's
    /// rating check is made again with them.
    pub fn set_money(&mut self, money: Money) {
        if let Some(game) = &mut self.game {
            game.money = money;
        }
        self.loaded_check();
    }

    /// Puts the interface around the game. The window is the size it was resolved for.
    pub fn set_interface(&mut self, mut ui: Interface) {
        self.map = Framebuffer::new(self.frame.width, self.frame.height);
        self.painted = None;
        crate::usertext::serve(&mut ui, self.renderer.catalogue());
        let names = self.type_names(&ui);
        self.ui = Some(ui);
        if let Some(game) = &mut self.game {
            game.finances = Finances::default();
            game.world.units.names = names;
            game.awards_page = Awards::default();
        }
    }

    /// The text of the name id of each type of the catalogue, as `ui` shows it: the units name
    /// their new animals, staff and objects with it.
    fn type_names(&self, ui: &Interface) -> std::collections::HashMap<i32, String> {
        (self.renderer.catalogue().types.iter())
            .filter(|t| t.name_id != 0)
            .map(|t| (t.name_id, ui.string(t.name_id)))
            .collect()
    }

    /// The interface, if the game has one.
    pub fn interface(&mut self) -> Option<&mut Interface> {
        self.ui.as_mut()
    }

    /// Whether a question is open or the list of saved games is shown: the game stands still.
    fn modal(&self) -> bool {
        self.question_open() || self.listing_open()
    }

    /// Whether a question is open.
    fn question_open(&self) -> bool {
        self.ui.as_ref().is_some_and(Interface::modal_open)
    }

    /// Whether the keys and the edge scroll of the map view act: always without an interface,
    /// else while the map view is shown and no other element has the keyboard focus.
    fn map_keys(&self) -> bool {
        !self.listing_open()
            && self
                .ui
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
        self.panel_lines = Vec::new();
        self.terrain_lines = Vec::new();
        self.dev_lines = Vec::new();
        (self.ui_events, self.ui_sounds, self.ui_actions) = (Vec::new(), Vec::new(), Vec::new());
        self.acted = 0;
        self.busy = false;
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
        self.on_map = self.ui.as_mut().is_none_or(Interface::pointer_on_map);
        self.on_text = self.ui.as_mut().is_some_and(Interface::pointer_on_text);
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
        self.fired = self.timers.pass(now, self.timers.speed.factor());
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
            self.follow();
            if self.map_keys()
                && !self.following()
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
        let mut month_end = false;
        self.scenario_sync();
        if let Some(game) = &mut self.game
            && let Some(step) = self.fired.step
            && !(game.paused || modal)
        {
            let data = Library {
                catalogue: self.renderer.catalogue(),
                vfs: self.renderer.vfs(),
                timings: &self.timings,
            };
            let report = self.sim.step_game(
                &data,
                self.renderer.catalogue(),
                &mut game.world,
                &mut game.money,
                step,
                |_, _, _| {},
            );
            month_end = report.month_end;
            if report.month_end {
                self.money_events.push(format!(
                    "month {} year {}",
                    game.money.month, game.money.year
                ));
            }
            if report.water_moved {
                self.exhibits_told();
            }
            self.status_told(report.bars);
            self.research_done();
            self.scenario_over();
        }
        self.scenario_effects();
        self.undo_button();
        self.panel_pass(now);
        self.tell_money(now);
        self.menu_lines = std::mem::take(&mut self.menu_events);
        self.sim_lines = self.sim.take_lines();
        // The state hash at each month change, so that two runs of a game can be compared by it.
        if month_end && let Some(game) = &self.game {
            let hash = crate::sim::hash::state(&self.sim, &game.world, &game.money);
            self.sim_lines.push(format!("hash {hash:016x}"));
        }
        self.unit_lines =
            (self.game.as_mut()).map_or_else(Vec::new, |g| g.world.units.take_lines());
        self.animal_told();
        self.unit_sounds = self.unit_sounds_placed();
        self.unit_sounds.append(&mut self.type_sounds);
        let sounds = self
            .unit_sounds
            .iter()
            .map(|s| format!("sound {} {}", s.id, s.file));
        self.unit_lines.extend(sounds);
        self.drew.then_some(&self.frame)
    }

    /// Takes the sounds the units' animations began and places each one: at the unit's position
    /// in the view space of the camera, heard from the centre of the map view (sound 5).
    fn unit_sounds_placed(&mut self) -> Vec<UnitSound> {
        let Some(game) = &mut self.game else {
            return Vec::new();
        };
        let camera = &game.camera;
        let (w, h) = (i64::from(camera.size.0), i64::from(camera.size.1));
        let listener = (
            i64::from(camera.scroll.0) + w / 2,
            i64::from(camera.scroll.1) + h / 2,
        );
        let sounds = game.world.units.take_sounds();
        let world = &game.world;
        (sounds.into_iter())
            .filter_map(|(id, heard)| {
                let c = world.zoo.entities[world.find(id)?].common.as_ref()?;
                let view = project(camera.rotation, c.x, c.y, c.z);
                let at = zoom_view(camera.zoom, view);
                let (distance, pan) = positional(at, listener);
                Some(UnitSound {
                    id,
                    file: heard.file,
                    voice: heard.voice,
                    base: heard.base,
                    at,
                    distance,
                    pan,
                })
            })
            .collect()
    }

    /// Writes the three bars (when they changed), and the date and the money text at each refresh, and
    /// shows the zoo status' messages in the message line; a donation names its amount.
    fn status_told(&mut self, bars: Option<[i32; 3]>) {
        let messages = self.sim.take_messages();
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        if bars.is_some() {
            let date = self.sim.calendar().date();
            let text = to_cp1252(&format::date(&ui.string(DATE_PATTERN), &date));
            ui.set_text(STATUS_DATE, &text);
            let day = format::date(&ui.string(DAY_PATTERN), &date);
            let tip = format::expand(
                &Words::new(|_| String::new()),
                &ui.template(STATUS_DATE),
                &[Arg::Text(&day)],
            );
            ui.set_string(STATUS_DATE, tip);
            let (units, _) = game.money.bar();
            ui.set_text(
                STATUS_MONEY,
                &to_cp1252(&Symbol::new(ui.string(SYMBOL)).text(units)),
            );
        }
        if let Some(bars) = bars.filter(|b| game.bars != Some(*b)) {
            game.bars = Some(bars);
            for (id, value) in [STATUS_ANIMALS, STATUS_GUESTS, STATUS_ZOO]
                .into_iter()
                .zip(bars)
            {
                ui.set_status(id, value);
            }
            self.money_events
                .push(format!("bars {} {} {}", bars[0], bars[1], bars[2]));
        }
        for (id, amount) in messages {
            let text = ui.string(id as i32);
            let text = match amount {
                Some(units) => {
                    text.replacen("%s", &Symbol::new(ui.string(SYMBOL)).text(units.into()), 1)
                }
                None => text,
            };
            ui.message(&to_cp1252(&text));
        }
    }

    /// Takes the animals' log lines of the last pass and shows their messages, with the animal's
    /// name in them, in the message line.
    fn animal_told(&mut self) {
        let Some(game) = &mut self.game else {
            self.animal_lines.clear();
            self.guest_lines.clear();
            self.staff_lines.clear();
            self.building_lines.clear();
            return;
        };
        self.animal_lines = game.world.units.take_animal_lines();
        self.ui_sounds.extend(game.world.units.take_cheers());
        self.guest_lines = game.world.units.take_guest_lines();
        self.staff_lines = game.world.units.take_staff_lines();
        self.world_lines.extend(game.world.units.take_world_lines());
        self.building_lines = game.world.units.take_building_lines();
        for (message, id, names) in game.world.units.take_messages() {
            let own = (game.world.find(id))
                .and_then(|at| game.world.zoo.entities[at].common.as_ref())
                .map_or("", |c| &c.name);
            if let Some(ui) = &mut self.ui {
                let names = if names.is_empty() {
                    vec![own.into()]
                } else {
                    names
                };
                let text = (names.iter()).fold(ui.string(message), |t, n| t.replacen("%s", n, 1));
                ui.message(&to_cp1252(&text));
            }
        }
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
        if matches!(command, Command::Rotate(_) | Command::Zoom(_)) && self.game.is_some() {
            self.start_busy();
        }
        match command {
            Command::Close => return self.close(),
            Command::Key(Key::Escape, ..) if self.listing_open() && !self.question_open() => {
                return self.close_list();
            }
            Command::Key(Key::Escape, ..)
                if self.game.is_some() && self.ui.is_some() && self.map_keys() =>
            {
                return self.cancel(true);
            }
            Command::Fullscreen(on) => self.fullscreen = on,
            Command::Load(file) => {
                self.load_saved(file);
                return;
            }
            Command::Save(file) => return self.save_game(file),
            Command::Award(id) => {
                if let Some(game) = &mut self.game {
                    crate::award::add(&mut game.world.zoo.awards, id);
                }
                return;
            }
            Command::Key(Key::S, true, _) if self.game.is_some() && self.map_keys() => {
                return self.save_key();
            }
            Command::Key(Key::L, true, _) if self.map_keys() => return self.load_pressed(),
            _ => {}
        }
        let keys = self.map_keys();
        let following = self.following();
        let shown = self.ui.as_ref().is_none_or(Interface::map_view_shown);
        let modal = self.modal();
        let listing = self.listing_open();
        match (command, &mut self.game) {
            (Command::Pointer(at), _) => self.pointer = at,
            (Command::Quit, _) => self.quit = true,
            (Command::Mouse(mouse), game) => {
                // A press or release that belongs to the map view is logged with the tile it is
                // on, and the terrain tool, if it is the map view's tool, acts on the mouse.
                let on_map = self.ui.as_mut().is_some_and(|ui| ui.mouse(mouse)) && !listing;
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
            (Command::Scroll(x, y), Some(game)) if keys && !following => {
                game.camera.key_scroll(&game.world.zoo, (x, y));
            }
            (Command::Rotate(clockwise), Some(game)) if keys => {
                let before = game.camera.clone();
                game.camera.rotate(&game.world.zoo, None, clockwise);
                // Restage from the new side. If that fails the view stays as it was.
                let extra = Extra {
                    marks: game.marks.clone(),
                    shown: game.shown(),
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
            (Command::DevMode, Some(_)) if keys => {
                if self.timers.speed.toggle() {
                    let on = self.timers.speed.developer();
                    self.dev_lines.push(if on { "on" } else { "off" }.into());
                    self.painted = None;
                }
            }
            // The speed keys go before the interface's characters (input 3.2, main loop 5.2).
            (Command::Char(byte @ (b'.' | b',' | b'/')), Some(_))
                if keys && self.timers.speed.key(byte) =>
            {
                self.dev_lines
                    .push(format!("speed {}", self.timers.speed.factor()));
            }
            (Command::Fullscreen(on), Some(game)) => {
                game.camera.set_fullscreen(&game.world.zoo, on)
            }
            // The Pause key works through a question too, but the question gives the flag back as
            // it was, so it is left alone there.
            (Command::Pause, Some(game)) if shown && !modal => game.pause(),
            (Command::Space, Some(game)) if keys => game.pause(),
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
            // The Backspace key presses the undo button (input 10).
            (Command::Key(Key::Backspace, ..), Some(_)) if keys => {
                if let Some(ui) = &mut self.ui {
                    ui.press_button(undo::UNDO);
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
            // The O key presses the Scenario button (input 10).
            (Command::Char(b'o' | b'O'), Some(_)) if keys => {
                if let Some(ui) = &mut self.ui {
                    ui.press_button(scenario::BUTTON);
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
        self.assign_aim();
    }

    /// The pending object part of [`App::aim`].
    fn aim_pending(&mut self, log: bool) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let cat = self.renderer.catalogue();
        game.build.set_month(self.sim.calendar().date().month);
        let before = game.pending.take();
        let habits = |e: &Entity| habit(cat, e);
        let dirt = place::dirt(&game.world, game.terrain.types(), &habits);
        let (step, view) = (game.build.step(), game.camera.rotation);
        let october = game.build.october();
        let point = game.camera.pick(&game.world.zoo, ui.pointer());
        let aim = place::unit::Aim {
            point: (point.0, point.1),
            row: i64::from(game.camera.scroll.1) + i64::from(ui.pointer().1),
            zoom: game.camera.zoom,
        };
        let at = |ty, point: (i64, i64)| {
            if cat.types.get(ty).is_some_and(place::unit::is_unit) {
                return place::unit::pending(&game.world, cat, &game.water, ty, view, aim, october);
            }
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
        game.pending = game
            .build
            .tool()
            .filter(|_| game.hover)
            .and_then(|ty| at(ty, (point.0, point.1)));
        // The unit the player carries follows the pointer with the same checks (8.3).
        // (A unit that was taken from the zoo meanwhile, by an undo, is no longer carried.)
        if (game.carry.as_ref())
            .is_some_and(|c| game.held.is_none() && game.world.find(c.id).is_none())
        {
            game.carry = None;
        }
        if let Some(carry) = game.carry.as_ref().filter(|_| game.hover) {
            let water = &game.water;
            game.pending = match game.held {
                // A building held (8.7) is judged as a new one, facing as the view keeps it.
                Some(_) => place::moved(
                    &game.world,
                    cat,
                    water,
                    &dirt,
                    game.smart,
                    carry.ty,
                    place::unit::turned(carry, view),
                    (point.0, point.1),
                ),
                None => place::unit::carried(&game.world, cat, water, carry, view, aim),
            };
        }
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
    /// logs `world paint 12,40 3` first; a fence is told with its side, `world add fences/...
    /// 12,40 side 2 id 8`; a piece that takes the place of another keeps its id when it is a path
    /// or a fence of the same type, `world retype paths/... 12,40 id 7`, and otherwise logs the
    /// old one first, `world remove fences/... 12,40 side 2 id 7`).
    /// One sound is asked for the release: the placement sound if anything was placed, else the
    /// refusal sound; a type that turns by itself turns once; the last message of a piece that
    /// could not be placed shows in the message line (`tool message 10311`), and one that gives no
    /// message when nothing at all was placed logs `tool refused`; and a new pending
    /// object is made (placing 6.1 and 6.2).
    fn release(&mut self) {
        if let Some(id) = self.assigning() {
            return self.assign_release(id);
        }
        let Some(game) = &mut self.game else {
            return;
        };
        if game.build.bulldozing() {
            return self.bulldoze_release();
        }
        if game.build.gating() {
            return self.gate_release();
        }
        if game.carry.is_some() {
            return self.drop_carried();
        }
        let cat = self.renderer.catalogue();
        if (game.build.tool().and_then(|t| cat.types.get(t))).is_some_and(place::unit::is_unit) {
            return self.adopt();
        }
        if game.build.tool().is_none() && game.terrain.mode().is_none() {
            return match self.pick_or_select() {
                true => (),
                false => self.select(),
            };
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
            // The animals of the exhibit react to an object placed (habitat rating 9.4); a fence
            // piece runs the pair too and decides nothing by its value.
            let placed = p.verdict != place::Verdict::Invalid && game.money.can_pay(p.price);
            let mut before = match placed {
                false => Vec::new(),
                true => {
                    let rand = &mut || self.sim.rng.rand();
                    rating::entity(&mut game.world, cat, at, rating::reads_neighbours(ty), rand)
                }
            };
            let outcome = place::place(
                &mut game.world,
                &mut game.money,
                ty,
                p,
                water,
                colors,
                |e| renderer.shape(e),
            );
            if let Outcome::Placed(id) = outcome {
                crate::building::built(&mut game.world, ty, id, self.sim.calendar().elapsed_ms());
                let (habitat, name) = rating::object(ty);
                let change = (!fence).then_some(rating::Change::Object {
                    habitat,
                    name,
                    sign: 1,
                });
                let rand = &mut || self.sim.rng.rand();
                rating::again(&game.world, &mut before, at, rating::reads_neighbours(ty));
                rating::react(&mut game.world, cat, before, change, rand);
            }
            match outcome {
                Outcome::Placed(id) => {
                    // A new or replaced fence piece is healthy and draws its wear interval.
                    if fence {
                        let rng = &mut self.sim.rng;
                        game.world
                            .heal(renderer.catalogue(), id, &mut || rng.rand());
                    }
                    if let Some(kind) = p.paint {
                        self.world_lines.push(format!("paint {x},{y} {kind}"));
                    }
                    // A piece that keeps its id only takes the new type.
                    if old == Some(id) {
                        self.world_lines
                            .push(format!("retype {name} {tile} id {id}"));
                        first = first.or(Some(p.ty));
                        continue;
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
        // The list of saved games was closed by its own Back button, or by a game that started.
        if self.listing.is_some() && !ui.shown(crate::menu::FREEFORM_LAYOUT) {
            self.listing = None;
        }
        let selected = ui.take_selected();
        self.build(None);
        let actions: Vec<Action> = self.ui_actions[self.acted..].to_vec();
        self.acted = self.ui_actions.len();
        self.scenario_pressed(&selected, &actions);
        for action in actions {
            match action {
                Action::Answer(code) => self.answered(code),
                action if self.listing_open() => self.listed(action),
                action if self.game.is_none() => {
                    if let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) {
                        let lines = menu.action(self.renderer.vfs(), ui, action);
                        self.menu_events.extend(lines);
                    }
                    self.requested();
                }
                Action::Row(exhibit::LIST, row) if self.game.is_some() => self.panel_row(row),
                Action::Row(exhibit::ANIMALS, row) if self.game.is_some() => {
                    self.panel_animal(row);
                }
                Action::Edited(exhibit::NAME) if self.game.is_some() => self.panel_rename(),
                Action::Edited(animal::NAME) if self.game.is_some() => self.animal_rename(),
                Action::Row(staff::LIST, row) if self.game.is_some() => self.staff_row(row),
                Action::Row(staff::DUTIES, row) if self.game.is_some() => self.staff_duty(row),
                Action::Edited(staff::NAME) if self.game.is_some() => self.staff_rename(),
                Action::Edited(guest::NAME) if self.game.is_some() => self.guest_rename(),
                Action::Edited(
                    panel_building::NAME | panel_building::PLAIN_NAME | panel_building::FILTER_NAME,
                ) if self.game.is_some() => {
                    self.building_rename();
                }
                Action::Edited(panel_building::PRICE) if self.game.is_some() => {
                    self.building_type_price();
                }
                _ => {}
            }
        }
        for id in selected {
            match id {
                SAVE_BUTTON if self.game.is_some() => self.save_pressed(),
                scenario::BUTTON if self.game.is_some() => self.objectives(),
                LOAD_BUTTON if self.game.is_some() => self.ask(Asked::Load),
                crate::menu::SAVED_LOAD if self.listing_open() => self.load_listed(),
                crate::menu::LOAD if self.game.is_none() => self.open_list(),
                MAIN_MENU_BUTTON | EXIT_BUTTON if self.game.is_some() => {
                    let kind = if id == EXIT_BUTTON {
                        Asked::Exit
                    } else {
                        Asked::Leave
                    };
                    self.ask(kind);
                }
                undo::UNDO if self.game.is_some() => self.undo(),
                animal::EXHIBIT if self.game.is_some() => self.animal_exhibit(),
                exhibit::WALL_DOWN | exhibit::WALL_UP if self.game.is_some() => {
                    self.tank_wall(id == exhibit::WALL_UP);
                }
                exhibit::GROUND_DOWN | exhibit::GROUND_UP if self.game.is_some() => {
                    self.tank_ground(id == exhibit::GROUND_UP);
                }
                exhibit::SALT | exhibit::FRESH if self.game.is_some() => {
                    self.tank_fill(id == exhibit::SALT);
                }
                exhibit::DRAIN if self.game.is_some() => self.tank_drain(),
                exhibit::TANK_TAB if self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.panel.clock.force = true;
                    }
                }
                staff::FIRE if self.game.is_some() => self.staff_fire(),
                staff::PICK_UP if self.game.is_some() => self.staff_pick_up(),
                guest::PICK_UP if self.game.is_some() => self.guest_pick_up(),
                panel_building::UP | panel_building::DOWN if self.game.is_some() => {
                    self.building_price(id == panel_building::UP);
                }
                panel_building::SELL | panel_building::PLAIN_SELL | panel_building::FILTER_SELL
                    if self.game.is_some() =>
                {
                    self.building_sell();
                }
                panel_building::PICK_UP if self.game.is_some() => self.building_pick_up(),
                t if panel_building::TABS.contains(&t) && self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.building.clock.force = true;
                    }
                }
                staff::REMOVE | staff::REMOVE_ALL if self.game.is_some() => {
                    self.staff_remove(id == staff::REMOVE_ALL);
                }
                t if staff::TABS.contains(&t) && self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.staff.clock.force = true;
                    }
                }
                t if guest::TABS.contains(&t) && self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.guest.clock.force = true;
                    }
                }
                t if animal::TABS.contains(&t) && self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.animal.clock.force = true;
                    }
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
        game.build.set_month(self.sim.calendar().date().month);
        game.build.set_hire(self.sim.hire_male);
        game.build.sync(ui, vfs, cat, &mut self.tool_lines);
        if let Some(id) = selected {
            game.build.selected(ui, vfs, cat, id, &mut self.tool_lines);
            self.terrain_pressed(id);
        }
        self.terrain_sync();
        self.make_pending();
        // Every change of the tool or the step tells a line. Choosing an item ends a move (8.6).
        if self.tool_lines.len() != told {
            let b = self.game.as_ref().map(|g| &g.build);
            let tool = b.is_some_and(|b| b.tool().is_some() || b.bulldozing() || b.gating());
            if tool && self.game.as_ref().is_some_and(|g| g.carry.is_some()) {
                return self.put_back();
            }
            self.aim(false);
        }
    }

    /// Makes the pending staff member of each staff item chosen since the last call: its two
    /// counters are drawn now (staff 4.2), the last one made is the pending one.
    pub(super) fn make_pending(&mut self) {
        let Some(game) = &mut self.game else { return };
        let cat = self.renderer.catalogue();
        for ty in game.build.take_made() {
            let t = &cat.types[ty];
            if let Some(kind) = crate::unit::Kind::of_class(t.class) {
                game.made = Some((ty, crate::staff::create(Some(t), kind, &mut self.sim.rng)));
            }
        }
    }

    /// What the menu's last button asked of the game: open a file, or tell the player something.
    fn requested(&mut self) {
        match self.menu.as_mut().and_then(Menu::take_request) {
            Some(Request::Open(start)) => {
                self.start_busy();
                let Some(menu) = &self.menu else { return };
                let zoo = menu.load(&start.path, self.renderer.catalogue());
                self.open(&start, zoo);
            }
            Some(Request::Message(text)) => self.say(&text),
            None => {}
        }
    }

    /// Opens the game that was read, or tells why it was not. Whether it opened.
    fn open(&mut self, start: &Start, read: Result<(Zoo, Money), String>) -> bool {
        // A saved game loads its research section; a start ignores it (research 7.4).
        let saved = start.scenario.is_none();
        let opened = read.and_then(|(zoo, money)| self.open_zoo(zoo, money, saved));
        match opened {
            Ok(()) => {
                self.loaded_check();
                self.begin(start);
                true
            }
            Err(why) => {
                self.say(&why);
                false
            }
        }
    }

    /// `load FILE` of a script: a saved game opens as a Continue does. Whether it opened.
    fn load_saved(&mut self, file: &str) -> bool {
        self.start_busy();
        let catalogue = self.renderer.catalogue();
        let read = std::fs::read(file).map_err(|e| format!("{file}: {e}"));
        let read = read.and_then(|bytes| {
            Zoo::parse_money(&bytes)
                .and_then(|(zoo, money)| Ok((zoo.checked(catalogue)?, money)))
                .map_err(|e| format!("{file}: {e}"))
        });
        let start = Start {
            path: file.to_string(),
            label: "load".to_string(),
            forget_last: false,
            freeform: false,
            cash: Default::default(),
            reset: false,
            reset_fences: false,
            scenario: None,
        };
        self.open(&start, read)
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
        if start.forget_last {
            self.sim.restart_calendar();
        }
        if let Some(file) = &start.scenario {
            // The research tree and its picks come before the fence repairs of the start (7.4).
            let rng = &mut self.sim.rng;
            if let Some(tree) =
                crate::research::Tree::of_scenario(self.renderer.vfs(), file, &mut || rng.rand())
            {
                if let Some(game) = &mut self.game {
                    game.build.start_research(self.renderer.catalogue(), &tree);
                }
                self.sim.set_research(tree);
            }
        }
        if let Some(game) = &mut self.game {
            game.money.start(start.cash, start.reset);
            if start.reset {
                crate::guest::arrive_all(&mut game.world, self.sim.calendar().date());
                let now = self.sim.calendar().elapsed_ms();
                game.world.reset_filters(now);
                for id in crate::building::reset_all(&mut game.world, now) {
                    self.sim.remove(id);
                }
            }
            if start.forget_last {
                // A scenario or freeform map is a new game: the set-up rating check.
                let cat = self.renderer.catalogue();
                self.sim
                    .new_game(&mut game.world, cat, &mut game.money, start.reset_fences);
            }
            for (name, amount) in [("set", start.cash.set), ("add", start.cash.add)] {
                if let Some(amount) = amount {
                    self.money_events.push(format!("{name} {amount}"));
                }
            }
        }
        if let Some(file) = &start.scenario
            && let Some(scenario) = Scenario::prepare(self.renderer.vfs(), file)
        {
            self.sim.start_scenario(scenario);
        }
        if start.scenario.is_some() {
            // The triggers set the availability over the tree, the ini rewards come last (7.4).
            self.scenario_avail(true);
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
            Asked::Load => (ASK_LOAD, "load"),
            // The bulldozer and the tanks ask their own questions.
            Asked::Sweep | Asked::Tank => return,
        };
        // The questions ask only when saving is allowed (save games 3).
        if !self.can_save() {
            return self.proceed(kind);
        }
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

    /// The answer to the question that was asked: Yes saves first and goes on if it saved, No goes
    /// on, Cancel stays.
    fn answered(&mut self, code: i32) {
        if self.ending {
            return self.end_answered(code);
        }
        if let Some(saving) = self.saving.take() {
            return self.save_answered(code, saving);
        }
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
            6 => self.begin_save(Some(kind)),
            7 => self.proceed(kind),
            _ => {}
        }
    }

    fn proceed(&mut self, kind: Asked) {
        match kind {
            Asked::Sweep | Asked::Tank => {}
            Asked::Leave => self.stop_game(),
            Asked::Load => self.open_list(),
            Asked::Exit => {
                self.menu_events.push("game exit".to_string());
                self.quit = true;
            }
        }
    }

    /// The window is closed: from the menu the program ends at once; in a game it asks first. A
    /// question that is open is answered Cancel before (input 4.7).
    fn close(&mut self) {
        // The scenario is over and its result stored: there is nothing left to save.
        if self.game.is_none() || self.ui.is_none() || self.ending {
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
            (self.asked, self.saving) = (None, None);
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
        if self.game.as_ref().is_some_and(|g| g.carry.is_some()) {
            return self.put_back();
        }
        if self.end_assign() {
            return;
        }
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
        self.marks.clean(ui);
        (self.painted, self.drawn_at) = (None, None);
        for id in GAME_LAYOUTS {
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
            if self.timers.speed.developer() {
                ui.corner_text(&mut self.frame, b"developer mode");
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
        want.shown = game.shown();
        want.hidden.extend(game.carry.as_ref().map(|c| c.id));
        want.selected = game.selected;
        let animal = game
            .selected
            .and_then(|id| match &game.world.units.get(id)?.part {
                crate::unit::Part::Animal(a) => Some((a.egg, a.crated)),
                _ => None,
            });
        want.form = match animal {
            Some((true, _)) => Form::Egg,
            Some((_, true)) => Form::Crate,
            _ => Form::Plain,
        };
        want.faces.clone_from(&game.world.units.reactions);
        want.marks.extend(game.selection);
        game.dozer_extra(&mut want);
        want.marks.extend(game.assign_marks());
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

    /// The folder of the settings that a game without the menu writes its last file to.
    pub fn set_user_folder(&mut self, dir: Option<PathBuf>) {
        self.user = dir;
    }

    /// Makes this a test run: the generator is seeded from `seed` and the seconds of its later
    /// seedings are `seed + n` (main loop 12). Call it before the first pass.
    pub fn set_seed(&mut self, seed: u32) {
        self.sim.set_seed(seed);
        self.load_units();
    }

    /// Makes a replay: the seconds of the generator's seedings are `seeds`, the ones the log of the
    /// session told. Call it before the first pass.
    pub fn set_seeds(&mut self, seeds: Vec<u32>) {
        self.sim.set_seeds(seeds);
        self.load_units();
    }

    /// The animals of an open game are created again, so that their draws follow the new seeding
    /// as they do when the seed is known from the start.
    fn load_units(&mut self) {
        if let Some(game) = &mut self.game {
            self.sim.load(&mut game.world, self.renderer.catalogue());
        }
    }

    /// The calendar of the game; `None` in the menu.
    pub fn calendar(&self) -> Option<&Calendar> {
        self.game.as_ref().map(|_| self.sim.calendar())
    }

    /// The cash and the accounts of the game; `None` in the menu.
    pub fn money(&self) -> Option<&Money> {
        self.game.as_ref().map(|g| &g.money)
    }

    /// The cursor to show, by its number without the 50 of the alternate set ([`crate::cursor`]):
    /// over the interface the text cursor on an editable field, else the default; over the map
    /// view the terrain tool's, the bulldozer's, the drag's, the pending object's, the hover's
    /// (an exhibit under the assign tool, a fence piece under the gate tool, or, with no tool, a
    /// selectable entity), else the normal one.
    pub fn cursor(&self) -> u32 {
        use crate::cursor as c;
        if !self.on_map {
            return if self.on_text { c::TEXT } else { c::DEFAULT };
        }
        let Some(game) = &self.game else {
            return c::NORMAL;
        };
        if let Some(tool) = game.terrain.cursor() {
            return tool;
        }
        let over =
            game.staff.over.is_some() || (self.assigning().is_none() && self.over_selectable());
        let b = &game.build;
        match () {
            _ if b.bulldozing() => c::BULLDOZER,
            _ if b.dragging() || !game.run.is_empty() => c::DRAG,
            // The gate tool's pending gate is the hover, not a pending object.
            _ if b.gating() => {
                if game.gate.is_some() {
                    c::HOVER
                } else {
                    c::NORMAL
                }
            }
            _ if game.pending.is_some() => c::PLACING,
            _ if over => c::HOVER,
            _ => c::NORMAL,
        }
    }

    /// Whether the last pass began a long operation, which shows the busy cursor (string tables
    /// 6.1): rotating or zooming a map, loading a game from the interface or a script. A save
    /// never does.
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// Sets what is called when a long operation begins.
    pub fn on_busy(&mut self, hook: impl FnMut() + 'a) {
        self.busy_hook = Some(Box::new(hook));
    }

    fn start_busy(&mut self) {
        self.busy = true;
        if let Some(hook) = &mut self.busy_hook {
            hook();
        }
    }

    pub fn paused(&self) -> bool {
        self.modal() || self.game.as_ref().is_some_and(|g| g.paused)
    }

    /// Whether the player asked to close the game.
    pub fn quit(&self) -> bool {
        self.quit
    }

    /// The sounds the units' animations began in the last pass, each placed in the view: the
    /// shell plays them as positional one-shots.
    pub fn unit_sounds(&self) -> &[UnitSound] {
        &self.unit_sounds
    }

    /// The sounds the interface, the placing of objects and the animals' reactions to a change
    /// of their exhibit asked for in the last pass, by file name without the extension
    /// (`uiclick`, `place`, `noplace`, `smile`, `frown`, `exhibit`, `sounds/uiclick.wav`): the shell plays
    /// them.
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

    /// How many awards the zoo holds (none without a game).
    pub fn awards(&self) -> usize {
        self.game.as_ref().map_or(0, |g| g.world.zoo.awards.len())
    }

    /// The lines of the last pass, each led by the pass index and the clock: one per command it
    /// applied, in the text form of a script, then one per thing the interface did (`ui hover 7111`,
    /// `ui press ...`, `ui sound uiclick`, `ui select`, `ui show`, `ui hide`, `ui drag`), then one per thing the main menu did
    /// (`menu continue enabled NAME`, `menu continue disabled`, `menu version V` when it appears, `menu exit`),
    /// then `tool down X,Y` or `tool up X,Y` for a press or release on the map view (the tile, or `none`),
    /// then `world stage` when an edit made the draw stage the map again,
    /// then `panel exhibit ...`, `panel status ...`, `panel donations ...`, `panel animals ...`,
    /// `panel plaque ...` and `panel line ...` for what the exhibit panel and the recommendations
    /// plaque show, when it changed (see the usage of `run`),
    /// then `terrain enter paint|modify`, `terrain brush SIZE X,Y`, `terrain size N`, `terrain type N`,
    /// `terrain paint N`, `terrain cost N`, `terrain accept N`, `terrain cancel N`, `terrain leave`,
    /// `terrain short` and `terrain undo N` (the price of the tiles an undo gave back) for what the
    /// terrain tool did, and `tool undo N` with `world remove|add|retype|skip ... undo` for each
    /// action an undo took back,
    /// then `money cash N` (whole units) when the cash changed, `money bar TEXT COLOUR` when the
    /// status bar was written, `money set N` and `money add N` when a start set or added cash and
    /// `money month M year Y` at a month change, `money cell ID RRGGBB TEXT` for each cell of the
    /// Finances table that changed when the table was written,
    /// then `dev on` or `dev off` when Ctrl+D switched the developer mode and `dev speed F` for each
    /// speed key it acted on (the factor as the shortest decimal that reads back),
    /// then one per timer that fired, in the order draw,
    /// interface (then the music, if the update changed its volume or stopped it), step. A draw
    /// tells the hash of the frame it painted (`-` if it painted none) and the camera it drew
    /// with (none without a game), a step the new date (none without a game),
    /// then what the managers did: `sim seed N` for each seeding of the generator (the
    /// wall clock's seconds, or `--seed`'s), which a replay uses again, `sim step MS`
    /// for a step the game ran and `sim month M Y` when it acted on the month flag of the one
    /// before, then `unit add ID KIND X,Y`, `unit remove ID`, `unit status ID`, `unit tile ID X,Y`,
    /// `unit enter ID EXHIBIT`, `unit face ID DIRECTION`, `unit abort ID` and
    /// `unit gate ID open|close GATE` for the units of the world. A pass that did nothing has no line.
    /// A log, with the size of the window, is all a replay needs: the commands and the clock of
    /// each pass that did something (`run --replay`). A windowed run prints the same lines.
    pub fn log_lines(&self, pass: u64, now: u64) -> Vec<String> {
        let mut lines = Vec::new();
        let lead = format!("pass {pass} t={now}");
        lines.extend(self.applied.iter().map(|c| format!("{lead} {c}")));
        // One count line per pass that gave an award: the awards the zoo holds after it.
        let given = self
            .applied
            .iter()
            .filter(|c| matches!(c, Command::Award(_)));
        lines.extend(given.map(|_| format!("{lead} awards {}", self.awards())));
        lines.extend(self.ui_events.iter().map(|e| format!("{lead} ui {e}")));
        lines.extend(self.menu_lines.iter().map(|e| format!("{lead} menu {e}")));
        lines.extend(self.tool_lines.iter().map(|e| format!("{lead} {e}")));
        lines.extend(self.world_lines.iter().map(|e| format!("{lead} world {e}")));
        lines.extend(
            self.exhibit_lines
                .iter()
                .map(|e| format!("{lead} exhibit {e}")),
        );
        lines.extend(self.panel_lines.iter().map(|e| format!("{lead} panel {e}")));
        lines.extend(
            self.terrain_lines
                .iter()
                .map(|e| format!("{lead} terrain {e}")),
        );
        lines.extend(self.money_lines.iter().map(|e| format!("{lead} money {e}")));
        lines.extend(self.dev_lines.iter().map(|e| format!("{lead} dev {e}")));
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
            && self.game.is_some()
        {
            let d = self.sim.calendar().date();
            let paused = if self.paused() { " paused" } else { "" };
            lines.push(format!(
                "{lead} step {step} {:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}{paused}",
                d.year, d.month, d.day, d.hour, d.minute, d.second, d.millisecond
            ));
        }
        lines.extend(self.sim_lines.iter().map(|e| format!("{lead} sim {e}")));
        lines.extend(self.unit_lines.iter().map(|e| format!("{lead} unit {e}")));
        lines.extend(
            self.animal_lines
                .iter()
                .map(|e| format!("{lead} animal {e}")),
        );
        lines.extend(self.guest_lines.iter().map(|e| format!("{lead} guest {e}")));
        lines.extend(self.staff_lines.iter().map(|e| format!("{lead} staff {e}")));
        lines.extend((self.building_lines.iter()).map(|e| format!("{lead} building {e}")));
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

mod adopt;
mod building_panel;
mod bulldozer;
mod carry;
mod exhibits;
mod gates;
mod guest_panel;
mod panel;
mod research;
mod saves;
mod scenario;
mod scenario_ui;
mod staff_panel;
mod tank_panel;
mod terrain;
mod undo;

#[cfg(test)]
mod adopt_tests;
#[cfg(test)]
mod tank_panel_tests;

#[cfg(test)]
mod carry_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod avail_tests;

#[cfg(test)]
mod replay_tests;

#[cfg(test)]
mod start_tests;

#[cfg(test)]
mod saves_tests;

#[cfg(test)]
mod scenario_tests;

#[cfg(test)]
mod terrain_tests;

#[cfg(test)]
mod bulldozer_tests;

#[cfg(test)]
mod money_tests;

#[cfg(test)]
mod undo_tests;

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

use std::path::PathBuf;

use crate::Framebuffer;
use crate::behaviour::{Library, Timings};
use crate::build::terrain::{MINUS, PLUS};
use crate::build::{BULLDOZER, Build};
use crate::camera::{Camera, Settings, zoom_view};
use crate::cheat::Subject;
use crate::config::Config;
use crate::exhibit::rating;
use crate::menu::{self, Menu, Request, Start};
use crate::money::{DOLLAR_KEY, DONATIONS, Money, SYMBOL, Symbol, Tone};
use crate::music::{Music, positional};
use crate::panel::building as panel_building;
use crate::panel::marketing as marketing_panel;
use crate::panel::research as research_panel;
use crate::panel::{animal, exhibit, guest, recovery, show, staff};
use crate::place::{self, Class, Outcome, Pending, Tints};
use crate::render::order::project;
use crate::render::{
    Extra, Form, Frames, MapRenderer, Mark, RenderError, Shown, Staged, check_size,
};
use crate::scenario::Scenario;
use crate::sim::Sim;
pub use crate::soundscape::SceneLoop;
use crate::soundscape::Soundscape;
use crate::status::{Finances, STATUS_BUTTON, Words, awards::Awards};
use crate::terrain::{Tool, habit};
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
#[cfg(test)]
use crate::timing::Calendar;
use crate::timing::{Pass, Timers};
use crate::ui::interface::{Action, Buttons, Interface};
use crate::ui::layout::{expansion_entries, installed_expansions};
use crate::world::World;
use crate::zoo::{Entity, Zoo};
use ids::*;
pub use options::Display;

/// Binds `game` (and `ui`) of the zoo being played, or returns from the method without a game.
/// A macro, not a method: the caller keeps borrowing the other fields of the app (the renderer).
macro_rules! playing {
    ($app:ident, $game:ident) => {
        let Some($game) = &mut $app.game else {
            return;
        };
    };
    ($app:ident, $game:ident, $ui:ident) => {
        let (Some($game), Some($ui)) = (&mut $app.game, &mut $app.ui) else {
            return;
        };
    };
}

pub use command::{Command, Key, Mouse};
pub use told::UnitSound;

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
    /// What the marketing part of the zoo status window holds.
    funding: crate::panel::marketing::Funding,
    /// Whether the status window was shown at the last look, for the Awards page.
    awards_page: Awards,
    /// The tabs of the zoo status window.
    window: zoo_window::Window,
    /// The building panels and the tool they choose.
    build: Build,
    /// The pending object and the drag that buys it.
    placing: pending::Placing,
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
    /// The new exhibits that wait for a name, and what the player is asked about them.
    naming: exhibits::Naming,
    /// The exhibit panel and the recommendations plaque.
    panel: crate::panel::exhibit::Panel,
    /// The animal panel and the animal selected.
    animal: crate::panel::animal::Panel,
    /// The staff panel and the staff member selected.
    staff: crate::panel::staff::Panel,
    /// The recovery team panel and the helicopter selected.
    recovery: crate::panel::recovery::Panel,
    /// The guest panel and the guest selected.
    guest: crate::panel::guest::Panel,
    /// The research panel.
    research: crate::panel::research::Panel,
    /// The building panels and the building selected.
    building: crate::panel::building::Panel,
    /// The show panel (Marine Mania).
    show: crate::panel::show::Panel,
    /// The unit the player selected, and the one it carries (unit placing 8).
    selected: Option<u32>,
    carry: Option<place::unit::Carry>,
    /// The building it carries, when what it carries is one (unit placing 8.7).
    held: Option<crate::building::Held>,
    /// The loops the date hook holds for the sleigh and the biplane: the flight, the file and the
    /// base term (ambients 6). The game's end and a file load drop them with the game.
    vehicles: Vec<(u32, &'static str, i32)>,
    /// The global term of the sound rules, from the ini file (sound 4.3).
    global: i32,
    /// What the hide/show buttons hide, and Shift (map drawing 15).
    view: view_buttons::View,
    /// The minimap of the main bar (map drawing 10), made at the first pass with an interface.
    minimap: Option<crate::minimap::Minimap>,
}

impl Game {
    /// The world position `(x, y)` of the selected entity, whatever its class (an animal, a guest,
    /// a staff member, a building, a gate or a filter), if any: the reference of a rotation or a
    /// zoom (camera 8.1 step 1), followed or not. A unit being carried is not selected.
    fn selected_point(&self) -> Option<(i32, i32)> {
        let id = self.selected?;
        let c = self.world.zoo.entities[self.world.find(id)?]
            .common
            .as_ref()?;
        Some((c.x, c.y))
    }

    /// Sets or clears the pause flag (the buttons are the app's, `main_buttons`).
    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.world.set_paused(paused);
    }

    /// What the cost text at the pointer shows: the drag's, else the pending object's, else, in
    /// the paint tab, minus what painting the brush costs while the pointer is over the map. A
    /// gate shows its price as it is, not as a charge (placing 4.5).
    fn cost(&self) -> Option<i64> {
        if self.build.bulldozing() {
            return self.dozer_amount();
        }
        if self.placing.gate.as_ref().is_some_and(|g| g.hidden) {
            return None;
        }
        match self.placing.run.is_empty() {
            true => self
                .placing
                .pending
                .as_ref()
                .map(|p| match &self.placing.gate {
                    Some(g) => g.amount(),
                    None => p.amount(),
                })
                .or_else(|| self.terrain.pointer_amount().filter(|_| self.placing.hover)),
            false => Some(place::run_amount(&self.placing.run)),
        }
    }
}

pub struct App<'a> {
    renderer: MapRenderer<'a>,
    /// The zoo being played; `None` in the main menu.
    game: Option<Game>,
    /// The name cheats; its flags last as long as the program.
    cheats: crate::cheat::Cheats,
    /// The show panel's tab of the program's first showing (info-panels 19.3 point 4).
    show_tab: Option<i32>,
    /// The animal, guest and staff list windows (information panels 20.1).
    lists: crate::panel::unitlist::Lists,
    timers: Timers,
    quit: bool,
    /// The paused message (string 400) is on the message line (input 4.5).
    pause_said: bool,
    pointer: Option<(i32, i32)>,
    /// The window is active (no `Pointer(None)` since the last `Pointer(Some)`).
    active: bool,
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
    /// The rectangles of the map that a staging changed since it was painted for `painted`
    /// (`Some` when there was a staging, even one that changed nothing in the window).
    stale: Option<Vec<[i32; 4]>>,
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
    /// What the last pass did, one line each, by part of the game.
    lines: log::Lines,
    /// The messages about gates that wait to be shown.
    exhibit_messages: Vec<(u32, i32)>,
    /// What the cash and the accounts did and has not been logged yet.
    money_events: Vec<String>,
    /// The managers of the simulation.
    sim: Sim,
    /// The sounds the units' animations began in the last pass.
    unit_sounds: Vec<UnitSound>,
    /// The loops the tank filters play now, placed in the view (tank staff 7.3 point 7).
    loop_sounds: Vec<UnitSound>,
    /// The sounds of a type's own that a selection or a pick-up asked for in this pass, not placed
    /// in the view: they join the unit sounds at its end (unit placing 8.1, 8.2).
    type_sounds: Vec<UnitSound>,
    /// The frames and delays of the animations the units played.
    timings: Timings,
    /// The main menu, while the player is in it.
    menu: Option<Menu>,
    /// What the menu did and has not been logged yet, one line each (the lines of its first
    /// show open the log of the first pass).
    menu_events: Vec<String>,
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
    /// The menu is not shown yet ([`App::held_menu`]).
    menu_held: bool,
    /// The warning about user-made types is open (main menu 3.5 point 6.4).
    warned: bool,
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
    /// The game options window and the help window.
    options: options::Options,
    help: help::Help,
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
    /// The restart notice of the options window: its answer deselects the Options button.
    Restart,
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
        ui: Interface,
        menu: Menu,
        music: Option<Music>,
    ) -> Result<App<'a>, RenderError> {
        let mut app = App::held_menu(renderer, settings, timers, size, ui, menu, music)?;
        app.show_menu();
        Ok(app)
    }

    /// [`App::menu`], but the menu is not shown yet: [`App::start_up`] shows it unless the game
    /// of the command line opens first (main menu 3.5 point 4).
    pub fn held_menu(
        renderer: MapRenderer<'a>,
        settings: Settings,
        timers: Timers,
        size: (u32, u32),
        ui: Interface,
        menu: Menu,
        music: Option<Music>,
    ) -> Result<App<'a>, RenderError> {
        let mut app = App::empty(renderer, timers, size, settings)?;
        app.set_interface(ui);
        app.menu = Some(menu);
        app.menu_held = true;
        app.music = music;
        app.options_start();
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
        let options = options::Options::new(renderer.vfs().ini());
        let last_file = Config::parse(renderer.vfs().ini())
            .ok()
            .and_then(|ini| menu::last_path(&ini, renderer.vfs().root()));
        Ok(App {
            renderer,
            game: None,
            cheats: Default::default(),
            show_tab: None,
            lists: Default::default(),
            timers,
            quit: false,
            pause_said: false,
            pointer: None,
            active: true,
            on_map: true,
            on_text: false,
            busy: false,
            busy_hook: None,
            frame: Framebuffer::new(size.0, size.1),
            ui: None,
            map: Framebuffer::new(0, 0),
            painted: None,
            stale: None,
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
            exhibit_messages: Vec::new(),
            money_events: Vec::new(),
            sim,
            lines: Default::default(),
            unit_sounds: Vec::new(),
            loop_sounds: Vec::new(),
            type_sounds: Vec::new(),
            timings: Timings::default(),
            music: None,
            music_changed: false,
            music_restarted: false,
            settings,
            fullscreen: false,
            asked: None,
            warned: false,
            menu_held: false,
            ending: false,
            saving: None,
            listing: None,
            user: None,
            last_file,
            faded: false,
            bare: false,
            marks: Default::default(),
            options,
            help: Default::default(),
        })
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
        let question = ui.tree.picture(self.renderer.vfs(), QUESTION_PICTURE);
        ui.set_question_picture(question);
        let names = self.type_names(&ui);
        self.ui = Some(ui);
        if let Some(game) = &mut self.game {
            game.finances = Finances::default();
            game.funding = Default::default();
            game.world.units.names = names;
            game.awards_page = Awards::default();
            game.window.fresh();
        }
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
    fn run_pass(&mut self, now: u64, commands: &[Command]) -> Option<&Framebuffer> {
        (self.applied, self.fired, self.drew) = (Vec::new(), IDLE, false);
        self.lines.begin_pass();
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
            self.lines.menu = std::mem::take(&mut self.menu_events);
            return None;
        }
        // A zoom or a rotation validates the objects again (guest buildings 11.1.1).
        if let Some(game) = &mut self.game
            && game.camera.take_viewed()
        {
            crate::building::dead::revalidate(&mut game.world, self.renderer.catalogue());
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
            // After the picture, as the original does: the tint shows from the next draw.
            if self.active {
                self.hover_aim();
            }
        }
        if let Some(period) = self.fired.interface {
            self.scroll_view(now);
            self.pause_message();
            self.minimap_pass(now);
            if let Some(ui) = &mut self.ui {
                if let Some(game) = &self.game {
                    ui.entities_found(|id| game.world.find(id).is_some());
                }
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
        // What the start triggers left is done before the frame's step, as the start routine does it,
        // so the step of the start frame runs; the answer of a modal popup is no start: its pass goes
        // on after the step (scenarios 5.2, 8.5).
        if !self.marks.waiting() {
            self.scenario_effects();
        }
        let modal = self.modal();
        let held = self.scenario_held();
        let mut month_end = false;
        self.scenario_sync();
        if let Some(game) = &mut self.game
            && let Some(step) = self.fired.step
            && !(game.paused || modal || held)
        {
            let data = Library {
                catalogue: self.renderer.catalogue(),
                vfs: self.renderer.vfs(),
                timings: &self.timings,
            };
            // The session flags go in with the step, and come back as the rules left them.
            game.world.units.cheat = self.cheats.flags;
            game.world.units.hearing = game.hearing();
            game.world.units.eggs_hatch_fast = self.cheats.eggs_hatch_fast;
            let (season, cat) = (&mut self.cheats.season, self.renderer.catalogue());
            let (vehicles, mut trio) = (&mut game.vehicles, false);
            let report = self.sim.step_hooked(crate::sim::Step {
                data: &data,
                cat,
                world: &mut game.world,
                money: &mut game.money,
                ms: step,
                update: &mut |_, _, _| {},
                created: &mut |sim, world| {
                    trio = world.units.cheat.lions_tigers_bears;
                    crate::cheat::guest_flights(season, &data, cat, sim, world, vehicles)
                },
            });
            self.cheats.flags = game.world.units.cheat;
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
            if report.arrived {
                self.guest_created(trio);
            }
            self.research_done();
        }
        self.scenario_effects();
        // The end is decided at the end of the pass: its dialog comes after every popup of the pass
        // was answered (scenarios 8.5 point 5).
        if !self.scenario_held() && !self.question_open() {
            self.scenario_over();
        }
        self.undo_button();
        self.panel_pass(now);
        self.tell_money(now);
        self.lines.menu = std::mem::take(&mut self.menu_events);
        self.lines.sim = self.sim.take_lines();
        // The state hash at each month change, so that two runs of a game can be compared by it.
        if month_end && let Some(game) = &self.game {
            let hash = crate::sim::hash::state(&self.sim, &game.world, &game.money);
            self.lines.sim.push(format!("hash {hash:016x}"));
        }
        self.lines.unit =
            (self.game.as_mut()).map_or_else(Vec::new, |g| g.world.units.take_lines());
        self.animal_told();
        self.unit_sounds = self.unit_sounds_placed();
        self.loop_sounds = self.loop_sounds_placed();
        self.unit_sounds.append(&mut self.type_sounds);
        let ambient = self.ambient_placed();
        self.unit_sounds.extend(ambient);
        let sounds = self
            .unit_sounds
            .iter()
            .map(|s| format!("sound {} {}", s.id, s.file));
        self.lines.unit.extend(sounds);
        self.drew.then_some(&self.frame)
    }
}

mod actions;
mod adopt;
mod apply;
mod building_panel;
mod bulldozer;
mod carry;
mod cheat;
mod cheat_guest;
mod cheat_staff;
mod command;
mod exhibits;
mod facts;
mod gates;
mod guest_panel;
mod help;
mod history;
mod ids;
mod kinds;
mod log;
mod main_buttons;
mod minimap_bar;
mod options;
mod paint;
mod view_buttons;
pub use paint::{frame_hash, frame_hashes};
#[cfg(test)]
mod cheat_staff_tests;
mod panel;
mod pending;
mod questions;
mod recovery_panel;
mod research;
mod saves;
mod scenario;
mod scenario_ui;
mod show_panel;
mod staff_panel;
mod start;
mod start_up;
mod state;
#[cfg(test)]
pub(crate) use start::continued_save;
mod tank_panel;
mod terrain;
mod told;
mod undo;
mod unit_lists;
mod zoo_window;

#[cfg(test)]
mod adopt_tests;
#[cfg(test)]
mod avail_tests;
#[cfg(test)]
mod carry_tests;
#[cfg(test)]
mod cheat_guest_tests;
#[cfg(test)]
mod cheat_season_tests;
#[cfg(test)]
mod cheat_tests;
#[cfg(test)]
mod colour_tab_tests;
#[cfg(test)]
mod hover_tests;
#[cfg(test)]
mod random_use_tests;
#[cfg(test)]
mod recovery_panel_tests;
#[cfg(test)]
mod tank_panel_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod replay_tests;

#[cfg(test)]
mod first_run_tests;
#[cfg(test)]
mod main_buttons_tests;
#[cfg(test)]
mod start_tests;
#[cfg(test)]
mod start_up_tests;

#[cfg(test)]
mod saves_tests;

#[cfg(test)]
mod scenario_tests;

#[cfg(test)]
mod tutorial_tests;

#[cfg(test)]
mod tutorial_full_tests;

#[cfg(test)]
mod award_tests;

#[cfg(test)]
mod terrain_tests;

#[cfg(test)]
mod bulldozer_tests;
#[cfg(test)]
mod facts_tests;
#[cfg(test)]
mod minimap_bar_tests;
#[cfg(test)]
mod options_tests;
#[cfg(test)]
mod unit_lists_tests;
#[cfg(test)]
mod view_buttons_tests;

#[cfg(test)]
mod filter_tests;
#[cfg(test)]
mod show_panel_tests;

#[cfg(test)]
mod marketing_tests;
#[cfg(test)]
mod money_tests;
#[cfg(test)]
mod zoo_window_tests;

#[cfg(test)]
mod undo_tests;

#[cfg(test)]
mod ambient_tests;
#[cfg(test)]
mod developer_tests;

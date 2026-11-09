//! Opening a game: from the file the menu or a script read, with its camera, interface and
//! scenario, and the start of a game from the menu.

use super::*;
use crate::status::completed::Completed;

impl App<'_> {
    /// Opens a game on `zoo`: its stored camera, the first draw, and a simulation whose next step is
    /// the cap, as after the original's blocking load (main loop 3.5). A failure leaves the app
    /// as it was.
    ///
    /// This is a bare map: the start hook of the date hooks does not run (cheats 6.4 point 6).
    pub(super) fn open_game(&mut self, zoo: Zoo, money: Money) -> Result<(), RenderError> {
        self.open_game_as(zoo, money, false)
    }

    /// Opens a file the game read ([`App::open_game`]). A saved game (`saved`) whose scenario cannot
    /// be read is refused before anything changes: the load fails with the old-version message
    /// (research 7.2).
    pub(super) fn open_zoo(&mut self, zoo: Zoo, money: Money, saved: bool) -> Result<(), String> {
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

    /// The `[UI] userAttenuation` global term of the sound rules.
    fn global_term(&self) -> i32 {
        crate::config::Config::parse(self.renderer.vfs().ini())
            .map_or(0, |ini| crate::music::global_term(&ini))
    }

    /// The build panel of a game that opens, with the rewards its ini unlocks.
    fn new_build(&self, stored: u32, exhibits: usize) -> Build {
        Build::new(installed_expansions(self.renderer.vfs()))
            .with_selector(expansion_entries(self.renderer.vfs()), stored)
            .with_exhibits(exhibits)
            .unlocked(
                self.renderer.catalogue(),
                &scenario_ui::reward_ini(&self.menu, self.renderer.vfs()),
            )
    }

    /// [`App::open_game`] for a saved game when `saved`: the scenario it names builds the research
    /// tree and its research section is read (research 7.2, before the entities' own steps). A
    /// saved game whose scenario block loads runs the start hook after the resume triggers
    /// (cheats 6.3 point 2, 6.4).
    pub(super) fn open_game_as(
        &mut self,
        zoo: Zoo,
        money: Money,
        saved: bool,
    ) -> Result<(), RenderError> {
        // Every file load stops the loops of the sleigh and the biplane (cheats 6.4, 6.5.1).
        self.cheats.season.loops_stopped();
        let file = zoo.scenario_file().filter(|_| saved);
        let research = saved.then(|| (file.clone(), zoo.research().to_vec()));
        let goals = (zoo.scenario_states(), zoo.version >= 86);
        let stored = zoo.expansion;
        let size = (self.frame.width as i32, self.frame.height as i32);
        let mut camera = Camera::open(&zoo.view, size, self.settings);
        if self.fullscreen {
            camera.set_fullscreen(&zoo, None, true);
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
        let soundscape = Soundscape::load(self.renderer.vfs(), file.as_deref());
        self.sim.set_soundscape(soundscape);
        world.units.set_period(self.sim.ai.status_period);
        self.set_up_loaded(file.as_deref(), saved, &world.zoo);
        let mut tree = research.and_then(|(file, section)| self.loaded_research(file?, &section));
        // The animals are cut by the types as the file leaves them, the programs lengthen them
        // after (save games 15.5). Their availability follows the scenario's resume.
        let applied = tree
            .as_mut()
            .map(|t| t.apply(self.renderer.catalogue(), &mut world.research));
        let (flags, section_life) = applied.map_or((None, Vec::new()), |(f, l)| (Some(f), l));
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
        self.sim
            .fences_loaded(&mut world, self.renderer.catalogue());
        world.set_tank_config(crate::exhibit::TankConfig::load(self.renderer.vfs()));
        world.units.show = crate::show::Params::load(self.renderer.vfs());
        let exhibits = world.exhibits().list().len();
        // The terrain brush starts at the size the file stores (terrain editing 4.1); a size
        // outside 1 to 5 leaves the default.
        let mut terrain = Tool::new(
            types.clone(),
            crate::terrain::elevation_cost(self.renderer.vfs().ini()),
        );
        terrain.set_size(brush);
        self.ui.iter_mut().for_each(Interface::clear_messages);
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
            funding: Default::default(),
            awards_page: Awards::default(),
            window: Default::default(),
            build: self.new_build(stored, exhibits),
            placing: pending::Placing {
                pending: None,
                made: None,
                used: None,
                gate: None,
                gate_over: None,
                run: Vec::new(),
                hover: false,
                tints: Tints::load(self.renderer.vfs()),
            },
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
            naming: Default::default(),
            panel: Default::default(),
            animal: Default::default(),
            staff: Default::default(),
            recovery: Default::default(),
            guest: Default::default(),
            research: Default::default(),
            building: Default::default(),
            show: Default::default(),
            selected: None,
            carry: None,
            held: None,
            vehicles: Vec::new(),
            global: self.global_term(),
            view: Default::default(),
        });
        // The game screen's set-up: running, the pause button shown; the zoom buttons follow the
        // zoom of the file (camera 8.4 point 5, ui layout 7.11 point 4).
        self.pause_to(false);
        self.zoom_check();
        self.view_sync();
        let scenario = file.and_then(|f| Scenario::prepare(self.renderer.vfs(), &f));
        // The scenario resumes first (its triggers, then the goal states), then the research
        // section completes the programs over their availability, then the ini rewards (7.3).
        let mut forced = false;
        if let Some(scenario) = scenario {
            self.sim.resume_scenario(scenario, &goals.0, goals.1);
            // The section replaces a forced research for the types (7.3), not for the animals.
            if let Some(s) = self.sim.scenario() {
                forced = !s.gui().take_forced().is_empty();
            }
            self.scenario_avail();
            // The start hook follows the resume triggers; a saved game without a scenario block,
            // with an empty or a failed one does not run it (cheats 6.3 point 2, 6.4 points 5 to 7).
            self.game_started();
            self.sim.add_scenario_ambients();
        }
        // The lookup of the recorded rewards follows the open's triggers (entity types 10.4).
        self.look_up_unlocks();
        if let (Some(tree), Some(flags), Some(game)) = (tree, flags, &mut self.game) {
            let ini = scenario_ui::reward_ini(&self.menu, self.renderer.vfs());
            let life = tree.loaded_life(self.renderer.catalogue(), forced, section_life);
            crate::animal::lengthen(&mut game.world, &life);
            game.build.start_research(self.renderer.catalogue(), &flags);
            // The resumed scenario sets the selector over the file's value if it has a rule for it.
            game.build.set_scenario(tree.off, tree.selection);
            game.world.units.upgrades = research::upgrades(&tree);
            game.build
                .after_triggers(self.renderer.catalogue(), &ini, &[]);
            game.window.completed = Completed::replay(&tree);
            self.sim.set_research(tree);
        }
        if let Some(ui) = &mut self.ui {
            Build::first_tabs(ui);
            zoo_window::reset_tabs(ui);
        }
        // The load sets every exhibit's stored name again (cheats 2.2).
        let names = self
            .game
            .as_ref()
            .filter(|_| saved)
            .map(|g| g.world.loaded_names());
        for name in names.unwrap_or_default() {
            self.name_hook(Subject::Exhibit, &name);
        }
        Ok(())
    }

    /// What a file just read sets over the defaults. The resume of a scenario sets them up again
    /// (economy, marketing), the game section then restores the price as saved, whatever the range
    /// (zoo status 3.1, 11). The marketing word of the file comes over the set-up's level; a load
    /// with no scenario block and a bare map keep the configuration of the game before
    /// (marketing 3.2, 3.3, 7.2).
    fn set_up_loaded(&mut self, file: Option<&str>, saved: bool, zoo: &Zoo) {
        if saved {
            if let Some(file) = file {
                self.scenario_economy(file);
                self.marketing_set_up(file);
            }
            self.sim.status.price = zoo.clocks.status.price;
        }
        if let Some(word) = zoo.marketing() {
            self.sim.marketing_mut().load(word);
        }
    }

    /// The rating check that ends every load (zoo status 16).
    pub(super) fn loaded_check(&mut self) {
        if let Some(game) = &mut self.game {
            let cat = self.renderer.catalogue();
            self.sim.loaded(&game.world, cat, &mut game.money);
        }
    }

    /// Opens the game that was read, or tells why it was not. Whether it opened.
    pub(super) fn open(&mut self, start: &Start, read: Result<(Zoo, Money), String>) -> bool {
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
    pub(super) fn load_saved(&mut self, file: &str) -> bool {
        self.start_busy();
        let catalogue = self.renderer.catalogue();
        let read = std::fs::read(file).map_err(|e| format!("{file}: {e}"));
        let read = read.and_then(|bytes| {
            Zoo::parse_money_in(&bytes, catalogue).map_err(|e| format!("{file}: {e}"))
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

    /// `scenario FILE` of a script: the menu starts the scenario as its Play button does. Without a
    /// menu (a run of a map) there is nothing to start it from.
    pub(super) fn play_scenario(&mut self, file: &str) {
        let Some(menu) = &mut self.menu else { return };
        menu.start_file(self.renderer.vfs(), file);
        self.requested();
    }

    /// The economy file the scenario `file` names under `[start] economy` comes over the settings
    /// (zoo status 3.1); a file that cannot be read is a warning.
    pub(super) fn scenario_economy(&mut self, file: &str) {
        let vfs = self.renderer.vfs();
        let name = Config::parse(&vfs.read(file).unwrap_or_default())
            .ok()
            .and_then(|c| {
                let name = c.section("start")?.get("economy")?;
                Some(String::from_utf8_lossy(name).into_owned())
            });
        let Some(name) = name else { return };
        match vfs.read(&name) {
            Ok(economy) => self.sim.scenario_economy(&economy),
            Err(why) => {
                eprintln!("rustyzoo: warning: {name}: {why}; the economy settings are kept")
            }
        }
    }

    /// A game was opened from the menu: the menu layouts are hidden, the main panel and the map
    /// view show, the menu music starts to fade and the simulation runs from the next pass (main
    /// menu 8.1). A freeform start also updates the music once with 2000 ms, which counts as one of
    /// the updates the first fade waits and moves no later one (8.3).
    pub(super) fn begin(&mut self, start: &Start) {
        if let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) {
            menu.hide(ui, start.forget_last);
        }
        self.menu_events
            .push(format!("game start {} {}", start.label, start.path));
        if start.forget_last {
            self.sim.restart_calendar();
        }
        if let Some(file) = &start.scenario {
            self.scenario_economy(file);
            let soundscape = Soundscape::load(self.renderer.vfs(), Some(file.as_str()));
            self.sim.set_soundscape(soundscape);
            self.marketing_set_up(file);
            // The research tree and its picks come before the fence repairs of the start (7.4).
            let rng = &mut self.sim.rng;
            if let Some(mut tree) =
                crate::research::Tree::of_scenario(self.renderer.vfs(), file, &mut || rng.rand())
            {
                if let Some(game) = &mut self.game {
                    let cat = self.renderer.catalogue();
                    let (flags, life) = tree.apply(cat, &mut game.world.research);
                    // A free program changes the animals the map placed (7.4).
                    crate::animal::lengthen(&mut game.world, &life);
                    game.build.start_research(cat, &flags);
                    // A start sets the raw expansion first, then the scenario's rules (research 3.5).
                    let value = tree.selection.unwrap_or(tree.expansion);
                    game.build.set_scenario(tree.off, Some(value));
                    game.world.units.upgrades = research::upgrades(&tree);
                    game.window.completed = Completed::replay(&tree);
                }
                self.sim.set_research(tree);
            } else if let Some(game) = &mut self.game {
                game.build.set_scenario([false; 4], Some(0));
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
                let cat = self.renderer.catalogue();
                self.sim.draw_use_all(&mut game.world, cat);
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
            self.scenario_forced();
            self.scenario_avail();
            // The start hook follows the start triggers and the research tree (cheats 6.3 point 2);
            // the ini rewards come after it.
            self.game_started();
            self.sim.add_scenario_ambients();
            self.scenario_rewards();
            self.look_up_unlocks();
        }
        let music = self.music.take();
        self.start_game(music);
        if start.freeform
            && let Some(music) = &mut self.music
        {
            music.update(2000);
        }
    }

    /// The animals of an open game are created again, so that their draws follow the new seeding
    /// as they do when the seed is known from the start.
    pub(super) fn load_units(&mut self) {
        if let Some(game) = &mut self.game {
            self.sim.load(&mut game.world, self.renderer.catalogue());
        }
    }
}

/// The file `source` of the install `vfs` opened as Continue does, with the interface the
/// window builds, and saved before any step.
#[cfg(test)]
pub(crate) fn continued_save(
    vfs: &crate::vfs::Vfs,
    cat: &crate::entity::Catalogue,
    source: &[u8],
) -> Result<Vec<u8>, String> {
    let size = (800, 600);
    let (zoo, money) = Zoo::parse_money_in(source, cat).map_err(|e| e.to_string())?;
    let renderer = crate::render::MapRenderer::new(vfs, cat);
    let ini = Config::parse(vfs.ini()).unwrap_or_default();
    // The window's first game is the file itself, in a process that has played nothing: the AI
    // clock held is 0 (zoo format 6.4).
    let mut first = zoo.clone();
    first.clocks.ai_ticks = 0;
    let mut app = App::new(
        renderer,
        first,
        Settings::from_config(&ini),
        Timers::from_config(&ini),
        size,
    )
    .map_err(|e| e.to_string())?;
    // The window's interface, as `load_interface` of the program builds it (none when the install has none).
    if let Ok(tables) = crate::strings::StringTables::open(vfs.root()) {
        let mut tree =
            crate::ui::layout::Tree::load(vfs, "ui/gamescrn.lyt", installed_expansions(vfs));
        let mut fonts = crate::text::Fonts::default();
        tree.resolve(size.0 as i32, size.1 as i32, |font| {
            let (face, points) = font.map_or(("Arial", "8"), |(face, points)| {
                let string = |n: i32| u32::try_from(n).map_or("", |n| tables.string(n));
                (string(face), string(points))
            });
            fonts.get(face, points).line_height()
        });
        if tree.failure.is_none() {
            app.set_interface(Interface::new(tree, Box::new(tables)));
        }
    }
    let start = Start {
        path: String::new(),
        label: "load".to_string(),
        forget_last: false,
        freeform: false,
        cash: Default::default(),
        reset: false,
        reset_fences: false,
        scenario: None,
    };
    if !app.open(&start, Ok((zoo, money))) {
        return Err("the file did not open".to_string());
    }
    app.saved_bytes()
}

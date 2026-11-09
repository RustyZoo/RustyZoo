//! The game options window (ui layout 16.1 to 16.6): its start-up values, what each control does
//! and writes, the video check, and the notice that asks for a restart. The window is layout 5.
//!
//! The settings are read from `Options::ini`, a copy of the ini file the changes are written to
//! as well as to the settings of the user folder, so "the value in the ini" is always the latest.

use super::*;
use crate::timing::ini_whole;
use crate::ui::layout::DISABLED;

pub(super) const OPTIONS_BUTTON: i32 = 1004;
const HELP_BUTTON: i32 = 1509;
const FILE_TAB: i32 = 1506;
const SHORT: i32 = 1520;
const LONG: i32 = 1521;
const DELAY: i32 = 1523;
const DURATION: i32 = 1525;
const SCROLL: i32 = 1515;
const MUSIC: i32 = 1513;
const VOLUME: i32 = 1511;
/// The resolution buttons and the sizes they ask for.
const RESOLUTIONS: [(i32, (i32, i32)); 3] = [
    (1517, (800, 600)),
    (1518, (1024, 768)),
    (1519, (1280, 1024)),
];
const WINDOW_MODE: i32 = 1587;
const FULL_SCREEN: i32 = 1588;
/// The terrain blending buttons, the first being level 0.
const TERRAIN: i32 = 1590;
const HIGH: i32 = 1539;
const LOW: i32 = 1546;
const BIT16: i32 = 1551;
const BIT8: i32 = 1552;
const COLORED: i32 = 1555;
const MONO: i32 = 1556;
/// The strings: the restart notice, and the refusals to leave full screen.
const RESTART: i32 = 11700;
const TOO_SMALL: i32 = 22007;
const TOO_SHALLOW: i32 = 22008;
/// The panels that showing the window hides (16.1 point 3.1), besides the tool panels
/// ([`Build::hide_panels`]): the zoo status, show, research, hiring and developer panels, the
/// plaques, and the exhibit, animal, guest and staff panels.
const HIDDEN_PANELS: [i32; 12] = [
    14,
    803,
    10,
    34,
    27,
    28,
    show::LAYOUT,
    research_panel::LAYOUT,
    exhibit::LAYOUT,
    animal::LAYOUT,
    guest::LAYOUT,
    staff::LAYOUT,
];

/// The desktop of the display: width, height and bits per pixel.
pub type Display = (i32, i32, i32);

/// What the options window keeps between two looks.
pub(crate) struct Options {
    /// The ini file with the changes made since the start.
    ini: Config,
    started: bool,
    /// The rows that need a game's map view have been set.
    mapped: bool,
    shown: bool,
    /// `loadHalfAnims` and `use8BitSound` when the window was shown (16.1 point 3.5).
    remembered: (i32, i32),
    /// `menuMusicMaxAttenuation`, read once (16.2 point 1).
    max_attenuation: i32,
    /// The global term of the sound rules (sound 4.3).
    pub(super) global: i32,
    alternate: bool,
    display: Option<Display>,
    /// The mode and display the video check last looked at.
    checked: Option<(bool, Option<Display>)>,
    /// The mode the player asked the window for with a button, until the shell takes it.
    request: Option<bool>,
}

impl Options {
    pub(super) fn new(ini: &[u8]) -> Options {
        let ini = Config::parse(ini).unwrap_or_default();
        Options {
            started: false,
            mapped: false,
            shown: false,
            remembered: (0, 0),
            max_attenuation: -1000,
            global: crate::music::global_term(&ini),
            alternate: crate::cursor::alternate(&ini),
            display: None,
            checked: None,
            request: None,
            ini,
        }
    }

    fn get(&self, section: &str, key: &str, default: i32) -> i32 {
        ini_whole(&self.ini, section, key).unwrap_or(default)
    }

    /// `[advanced] loadHalfAnims` and `use8BitSound`.
    fn restart_keys(&self) -> (i32, i32) {
        (
            self.get("advanced", "loadHalfAnims", 0),
            self.get("advanced", "use8BitSound", 0),
        )
    }
}

/// The slider position of the map scroll speed `v` (16.2).
fn scroll_position(v: i32) -> i64 {
    (f64::from(v) * 0.008333333333333333 * 100.0) as i64
}

/// The scroll step of the slider position `s` (16.4 point 4).
fn scroll_step(s: i32) -> i32 {
    8 - (f64::from(s) * 0.01 * -120.0) as i32
}

/// The terrain preset of a level (16.5): blending, then `normal`, `click` and `drag`.
fn preset(level: i32) -> (bool, i32, i32, i32) {
    match level {
        0 => (false, 0, 0, 0),
        1 => (true, 3, 4, 4),
        2 => (true, 2, 1, 3),
        _ => (true, 0, 0, 0),
    }
}

impl App<'_> {
    /// Writes `key` of `section` to the settings, to the copy of the ini file and to the user
    /// folder; with a `default`, a value the ini already has (or, with no key, the default) is
    /// left alone.
    fn put(&mut self, section: &str, key: &str, value: i32, default: Option<i32>) {
        if default.is_some_and(|d| self.options.get(section, key, d) == value) {
            return;
        }
        let text = value.to_string();
        self.options.ini.set(section, key, &text);
        match (&mut self.menu, &self.user) {
            (Some(menu), _) => menu.write_back(section, key, &text),
            (None, Some(dir)) => {
                if let Err(e) = crate::userdir::save(dir, section, key, &text) {
                    eprintln!("rustyzoo: warning: {e}");
                }
            }
            _ => {}
        }
    }

    /// The controls take the values of the ini file, once (16.2): each is set without running its
    /// action, then what the value asks for is applied. The rows from the map scroll speed on wait
    /// for a game, whose map view they act on (point 6); the window cannot be opened before.
    pub(super) fn options_start(&mut self) {
        if self.ui.is_none() {
            return;
        }
        let o = &self.options;
        let get = |section, key, default| o.get(section, key, default);
        let (help, delay) = (get("UI", "helpType", 0), get("UI", "tooltipDelay", 1));
        let duration = get("UI", "tooltipDuration", 5000);
        if !std::mem::replace(&mut self.options.started, true) {
            if let Some(ui) = &mut self.ui {
                ui.pick(if help == 0 { SHORT } else { LONG });
                ui.set_slider(DELAY, (i64::from(delay) - 1) / 10);
                ui.set_slider(DURATION, (i64::from(duration) - 500) / 100);
            }
            self.tooltip_delay((i64::from(delay) - 1) / 10);
            self.tooltip_duration((i64::from(duration) - 500) / 100);
            self.help_build();
        }
        if self.game.is_none() || std::mem::replace(&mut self.options.mapped, true) {
            return;
        }
        let o = &self.options;
        let get = |section, key, default| o.get(section, key, default);
        let scroll = get("UI", "mouseScrollX", 32);
        let (max, music) = (
            get("UI", "menuMusicMaxAttenuation", -1000),
            get("UI", "menuMusicAttenuation", -1000),
        );
        let volume = get("UI", "userAttenuation", 0);
        let level = get("advanced", "level", 2);
        let (half, eight) = o.restart_keys();
        let colored = get("UI", "useAlternateCursors", 0);
        self.options.max_attenuation = max;
        if let Some(ui) = &mut self.ui {
            ui.set_slider(SCROLL, scroll_position(scroll));
            ui.set_slider(MUSIC, 100 - (i64::from(music) - i64::from(max)) / 50);
            ui.set_slider(VOLUME, (100 - i64::from(volume) / 15).clamp(0, 100));
            ui.pick(if half == 0 { HIGH } else { LOW });
            ui.pick(if eight == 0 { BIT16 } else { BIT8 });
            ui.pick(if colored == 0 { COLORED } else { MONO });
            if (0..4).contains(&level) {
                ui.pick(TERRAIN + level);
            }
        }
        self.set_scroll(scroll);
        self.put("UI", "mouseScrollY", scroll, Some(32));
        self.put("UI", "mouseScrollX", scroll, Some(32));
        self.options.global = crate::music::user_term(volume);
        if (0..4).contains(&level) {
            self.terrain_level(level, true);
        }
    }

    /// The tooltip's delay for the slider position `s` (16.2): `10 s + 1` ms.
    fn tooltip_delay(&mut self, s: i64) {
        let Some(ui) = &mut self.ui else { return };
        let mut settings = ui.settings();
        settings.tooltip_delay = u32::try_from((10 * s + 1).max(0)).unwrap_or(u32::MAX);
        ui.set_settings(settings);
    }

    /// The tooltip's duration for the slider position `s`: `100 s + 500` ms.
    fn tooltip_duration(&mut self, s: i64) {
        if let Some(ui) = &mut self.ui {
            ui.set_tooltip_duration(i32::try_from(100 * s + 500).unwrap_or(i32::MAX));
        }
    }

    /// Both scroll steps of the camera become `step`, now and for the games that open later.
    fn set_scroll(&mut self, step: i32) {
        self.settings.mouse = step;
        if let Some(game) = &mut self.game {
            game.camera.settings.mouse = step;
        }
    }

    /// The terrain preset of `level` (16.5): the renderer blends or not, the map is drawn again
    /// at once, and the four keys are written.
    fn terrain_level(&mut self, level: i32, start: bool) {
        let (blend, normal, click, drag) = preset(level);
        if self.renderer.set_blending(blend)
            && let Some(game) = &mut self.game
        {
            game.world.touch();
        }
        let keys = [
            ("level", level, 2),
            ("drag", drag, 3),
            ("click", click, 1),
            ("normal", normal, 2),
        ];
        for (key, value, default) in keys {
            self.put("advanced", key, value, start.then_some(default));
        }
    }

    /// The window shown or hidden since the last look (16.1), and a change of mode or display.
    pub(super) fn options_pass(&mut self) {
        self.options_start();
        let Some(ui) = self.ui.as_mut().filter(|_| self.game.is_some()) else {
            return;
        };
        let shown = ui.shown(OPTIONS_PANEL);
        if shown && !self.options.shown {
            ui.set_button(BULLDOZER, false, true);
            Build::hide_panels(ui);
            for id in HIDDEN_PANELS {
                ui.set_hidden(id, true);
            }
            let enabled = ui
                .tree
                .find(FILE_TAB)
                .is_some_and(|i| ui.tree.elements[i].state & DISABLED == 0);
            if enabled {
                ui.select_member(FILE_TAB);
            }
            let width = i32::try_from(self.frame.width).unwrap_or(0);
            if let Some((id, _)) = RESOLUTIONS.iter().find(|(_, (w, _))| *w == width) {
                ui.pick(*id);
            }
            self.options.checked = None;
            self.options.remembered = self.options.restart_keys();
        } else if !shown && self.options.shown {
            ui.set_button(OPTIONS_BUTTON, false, true);
            if self.options.restart_keys() != self.options.remembered {
                self.notice(RESTART);
            }
        }
        self.options.shown = shown;
        let look = (self.fullscreen, self.options.display);
        if self.options.checked != Some(look) {
            self.options.checked = Some(look);
            self.video_check();
        }
    }

    /// A question box with an OK button and the exclamation picture.
    fn notice(&mut self, string: i32) {
        let Some(ui) = &mut self.ui else { return };
        let text = to_cp1252(&ui.string(string));
        self.menu_events
            .push(format!("options notice {}", ui.string(string)));
        if ui.ask(QUESTION, &text, Buttons::OK) {
            research::exclaim(ui, self.renderer.vfs());
        }
    }

    /// Which resolutions and which of the two mode buttons the display allows (16.3 point 3).
    fn video_check(&mut self) {
        let (w, h) = (self.frame.width as i32, self.frame.height as i32);
        let (dw, dh, db) = self.options.display.unwrap_or((i32::MAX, i32::MAX, 32));
        let full = self.fullscreen;
        let Some(ui) = &mut self.ui else { return };
        for (id, (rw, rh)) in RESOLUTIONS {
            // In full screen the game scales its picture to any display that is large enough.
            let fits = match full {
                true => dw >= rw && dh >= rh,
                false => dw > rw && dh > rh,
            };
            ui.set_enabled(id, fits);
        }
        if full {
            ui.set_enabled(WINDOW_MODE, w < dw && h < dh && db > 8);
        }
        ui.reveal(WINDOW_MODE, full);
        ui.reveal(FULL_SCREEN, !full);
    }

    /// One pass (see `run_pass`). A pass that ends the program saves the main volume as
    /// `[UI] userAttenuation` (sound 4.3).
    pub fn pass(&mut self, now: u64, commands: &[Command]) -> Option<&Framebuffer> {
        let drew = self.run_pass(now, commands).is_some();
        if self.quit {
            self.put("UI", "userAttenuation", self.options.global, Some(0));
        }
        drew.then_some(&self.frame)
    }

    /// The desktop of the display, as the shell sees it (16.3).
    pub fn set_display(&mut self, display: Option<Display>) {
        self.options.display = display;
    }

    /// The mode a button of the window asked for, once: the shell switches and tells the game.
    pub fn take_mode_request(&mut self) -> Option<bool> {
        self.options.request.take()
    }

    /// The global term of the sound rules, from the main volume (sound 4.3).
    pub fn user_term(&self) -> i32 {
        self.options.global
    }

    /// Whether the alternate cursors are on.
    pub fn alternate_cursors(&self) -> bool {
        self.options.alternate
    }

    /// Switches the alternate cursors (Ctrl+C, the buttons): the ini is written and the matching
    /// button selected (16.6 point 3). The shell builds the cursors again.
    pub fn set_alternate_cursors(&mut self, on: bool) {
        if std::mem::replace(&mut self.options.alternate, on) == on {
            return;
        }
        self.put("UI", "useAlternateCursors", i32::from(on), None);
        if let Some(ui) = &mut self.ui {
            ui.pick(if on { MONO } else { COLORED });
        }
    }

    /// What the player did in the unit lists and the options window since the last look.
    pub(super) fn windows_acted(&mut self) {
        self.lists_acted();
        self.options_acted();
    }

    /// The buttons and sliders of the window that the player used since the last look.
    pub(super) fn options_acted(&mut self) {
        let Some(ui) = &mut self.ui else { return };
        let buttons = [
            SHORT,
            LONG,
            HIGH,
            LOW,
            BIT16,
            BIT8,
            COLORED,
            MONO,
            WINDOW_MODE,
            FULL_SCREEN,
        ];
        let ids: Vec<i32> = (buttons.into_iter())
            .chain([HELP_BUTTON])
            .chain(RESOLUTIONS.map(|r| r.0))
            .chain(TERRAIN..TERRAIN + 4)
            .collect();
        let pressed = ui.take_selected_of(&ids);
        let moved: Vec<(i32, i32)> = (self.ui_actions[self.acted..].iter())
            .filter_map(|a| match *a {
                Action::Scroll(id, s) => Some((id, s)),
                _ => None,
            })
            .collect();
        let rows: Vec<i32> = (self.ui_actions[self.acted..].iter())
            .filter_map(|a| match *a {
                Action::Row(help::LIST, row) => Some(row),
                _ => None,
            })
            .collect();
        for id in pressed {
            self.option_pressed(id);
        }
        // A row of the help window's list: its topic.
        for row in rows {
            if self.game.is_some() {
                self.help_show(usize::try_from(row).unwrap_or(usize::MAX));
            }
        }
        for (id, s) in moved {
            self.option_moved(id, s);
        }
    }

    fn option_pressed(&mut self, id: i32) {
        let restart = |this: &mut Self, key: &str, value: i32| {
            if this.options.get("advanced", key, 0) != value {
                this.put("advanced", key, value, None);
            }
        };
        match id {
            SHORT | LONG => {
                self.put("UI", "helpType", i32::from(id == LONG), None);
                if let Some(ui) = &mut self.ui {
                    let mut settings = ui.settings();
                    settings.long_help = id == LONG;
                    ui.set_settings(settings);
                }
            }
            HIGH | LOW => restart(self, "loadHalfAnims", i32::from(id == LOW)),
            BIT16 | BIT8 => restart(self, "use8BitSound", i32::from(id == BIT8)),
            COLORED | MONO => self.set_alternate_cursors(id == MONO),
            WINDOW_MODE | FULL_SCREEN => self.mode_pressed(id == FULL_SCREEN),
            HELP_BUTTON if self.game.is_some() => self.help_open(true),
            t if (TERRAIN..TERRAIN + 4).contains(&t) => self.terrain_level(t - TERRAIN, false),
            _ => {
                // The sizes are kept for the next start: the game keeps its size while it runs.
                let Some((_, (w, h))) = RESOLUTIONS.into_iter().find(|r| r.0 == id) else {
                    return;
                };
                self.put("user", "screenwidth", w, None);
                self.put("user", "screenheight", h, None);
                self.options.checked = None;
            }
        }
    }

    /// Window Mode or Full Screen: the switch is asked of the shell, only if the display is in
    /// the other state; to a window it is refused when the desktop is not larger than the game
    /// or has 8 bits per pixel (16.3 point 5).
    pub(super) fn mode_pressed(&mut self, full: bool) {
        if full == self.fullscreen {
            return;
        }
        self.start_busy();
        let (w, h) = (self.frame.width as i32, self.frame.height as i32);
        if let (false, Some((dw, dh, db))) = (full, self.options.display) {
            if db < 9 {
                return self.notice(TOO_SHALLOW);
            }
            if dw <= w || dh <= h {
                return self.notice(TOO_SMALL);
            }
        }
        self.options.request = Some(full);
    }

    fn option_moved(&mut self, id: i32, s: i32) {
        match id {
            VOLUME => {
                self.options.global = match s {
                    0 => 5000,
                    s => (100 - s).saturating_mul(15),
                };
                if let Some(game) = &mut self.game {
                    game.global = self.options.global;
                }
                if let Some(music) = &mut self.music {
                    music.set_global(self.options.global);
                    self.music_changed = true;
                }
            }
            MUSIC => {
                let value = self
                    .options
                    .max_attenuation
                    .saturating_add((100 - s).saturating_mul(50));
                self.put("UI", "menuMusicAttenuation", value, None);
            }
            DELAY => self.tooltip_delay(s.into()),
            DURATION => self.tooltip_duration(s.into()),
            SCROLL => {
                let step = scroll_step(s);
                self.set_scroll(step);
                self.put("UI", "mouseScrollX", step, None);
                self.put("UI", "mouseScrollY", step, None);
            }
            _ => {}
        }
    }
}

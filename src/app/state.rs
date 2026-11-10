//! What the app tells its shell: the music, the sounds, the cursor, the flags of the last pass.

use super::*;

impl<'a> App<'a> {
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

    /// Whether the last pass started the menu music again: the player plays it from the start.
    pub fn music_restarted(&self) -> bool {
        self.music_restarted
    }

    /// The camera of the game; `None` in the menu.
    #[cfg(test)]
    pub(crate) fn camera(&self) -> Option<&Camera> {
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

    /// The calendar of the game; `None` in the menu.
    #[cfg(test)]
    pub(crate) fn calendar(&self) -> Option<&Calendar> {
        self.game.as_ref().map(|_| self.sim.calendar())
    }

    /// The cash and the accounts of the game; `None` in the menu.
    #[cfg(test)]
    pub(crate) fn money(&self) -> Option<&Money> {
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
            _ if b.dragging() || !game.placing.run.is_empty() => c::DRAG,
            // The gate tool's pending gate is the hover, not a pending object.
            _ if b.gating() => {
                if game.placing.gate.is_some() {
                    c::HOVER
                } else {
                    c::NORMAL
                }
            }
            _ if game.placing.pending.is_some() => c::PLACING,
            _ if over => c::HOVER,
            _ => c::NORMAL,
        }
    }

    /// Whether the last pass began a long operation, which shows the busy cursor (string tables
    /// 6.1): rotating or zooming a map, loading a game from the interface or a script. A save
    /// never does.
    #[cfg(test)]
    pub(crate) fn busy(&self) -> bool {
        self.busy
    }

    /// Sets what is called when a long operation begins.
    pub fn on_busy(&mut self, hook: impl FnMut() + 'a) {
        self.busy_hook = Some(Box::new(hook));
    }

    pub(super) fn start_busy(&mut self) {
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

    /// The loops the tank filters play now, each placed in the view: the shell keeps one voice for
    /// each, starts it, moves it and stops it (sound 4.2, 8.2).
    pub fn loop_sounds(&self) -> &[UnitSound] {
        &self.loop_sounds
    }

    /// The loops of the soundscape, the world's and the crowd's (sound 8.1): the shell keeps one
    /// voice for each, tries to start it once, and follows its volume.
    pub fn scene_loops(&self) -> Vec<SceneLoop> {
        self.sim.scene_loops()
    }

    /// The sounds the interface, the placing of objects and the animals' reactions to a change
    /// of their exhibit asked for in the last pass, by file name without the extension
    /// (`uiclick`, `place`, `noplace`, `smile`, `frown`, `exhibit`, `sounds/uiclick.wav`): the shell plays
    /// them.
    pub fn sounds(&self) -> &[&'static str] {
        &self.ui_sounds
    }

    /// The actions the interface ran in the last pass, for the game's code.
    #[cfg(test)]
    pub(crate) fn ui_actions(&self) -> &[Action] {
        &self.ui_actions
    }

    /// The timers that fired in the last pass.
    pub fn fired(&self) -> Pass {
        self.fired
    }

    /// How many awards the zoo holds (none without a game).
    pub(crate) fn awards(&self) -> usize {
        self.game.as_ref().map_or(0, |g| g.world.zoo.awards.len())
    }
}

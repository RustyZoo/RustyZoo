//! The log of a pass: its first line and the lines of each pass.

use super::*;

/// What each part of the game did in the last pass, one line each, as the log leads them with the
/// part's word (`tool down 12,40`, `world stage`, `dev on`, `dev speed 1.1`).
#[derive(Default)]
pub(super) struct Lines {
    pub(super) menu: Vec<String>,
    pub(super) tool: Vec<String>,
    pub(super) world: Vec<String>,
    pub(super) exhibit: Vec<String>,
    pub(super) panel: Vec<String>,
    pub(super) terrain: Vec<String>,
    pub(super) money: Vec<String>,
    pub(super) dev: Vec<String>,
    pub(super) sim: Vec<String>,
    pub(super) unit: Vec<String>,
    pub(super) animal: Vec<String>,
    pub(super) guest: Vec<String>,
    pub(super) staff: Vec<String>,
    pub(super) building: Vec<String>,
    pub(super) scenario: Vec<String>,
}

impl Lines {
    /// The start of a pass: the lines the pass itself fills are forgotten (the others are taken
    /// from their managers at its end).
    pub(super) fn begin_pass(&mut self) {
        self.menu.clear();
        (self.tool, self.world) = (Vec::new(), Vec::new());
        self.exhibit = Vec::new();
        self.panel = Vec::new();
        self.terrain = Vec::new();
        self.dev = Vec::new();
    }
}

impl App<'_> {
    /// The first line of a log: the size of the window, which the frames depend on.
    #[cfg(test)]
    pub(crate) fn log_header(&self) -> String {
        format!("size {}x{}", self.frame.width, self.frame.height)
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
        self.log_lines_with(pass, now, |frame| format!("{:016x}", frame_hash(frame)))
    }

    /// [`App::log_lines`] with the text of the frame hash given by `hash` (called once at most):
    /// a caller that hashes in a thread puts a placeholder there.
    pub(crate) fn log_lines_with(
        &self,
        pass: u64,
        now: u64,
        hash: impl FnOnce(&Framebuffer) -> String,
    ) -> Vec<String> {
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
        lines.extend(self.lines.menu.iter().map(|e| format!("{lead} menu {e}")));
        lines.extend(self.lines.tool.iter().map(|e| format!("{lead} {e}")));
        lines.extend(self.lines.world.iter().map(|e| format!("{lead} world {e}")));
        lines.extend(
            self.lines
                .exhibit
                .iter()
                .map(|e| format!("{lead} exhibit {e}")),
        );
        lines.extend(self.lines.panel.iter().map(|e| format!("{lead} panel {e}")));
        lines.extend(
            self.lines
                .terrain
                .iter()
                .map(|e| format!("{lead} terrain {e}")),
        );
        lines.extend(self.lines.money.iter().map(|e| format!("{lead} money {e}")));
        lines.extend(self.lines.dev.iter().map(|e| format!("{lead} dev {e}")));
        if self.fired.draw {
            let hash = match self.drew {
                true => hash(&self.frame),
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
        lines.extend(self.lines.sim.iter().map(|e| format!("{lead} sim {e}")));
        lines.extend(self.lines.unit.iter().map(|e| format!("{lead} unit {e}")));
        lines.extend(
            self.lines
                .animal
                .iter()
                .map(|e| format!("{lead} animal {e}")),
        );
        lines.extend(self.lines.guest.iter().map(|e| format!("{lead} guest {e}")));
        lines.extend(self.lines.staff.iter().map(|e| format!("{lead} staff {e}")));
        lines.extend((self.lines.building.iter()).map(|e| format!("{lead} building {e}")));
        lines
    }
}

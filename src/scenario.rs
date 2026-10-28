//! The scenario: the file read when a scenario or freeform map is chosen, its goals and the goal
//! manager that evaluates them every 1.5 s of simulated time (scenarios 3, 4.1 to 4.4, 5.1).
//!
//! The goal list is the timer, the goals of the file's `[goals]`, then those of each `[start]
//! extragoals` file. A goal is simple, complex (all children) or chain (children in order).
//! The rules a simple goal reads are in [`rules`]: the accounts, the zoo status, the exhibits, the
//! entities and the interface (rules 10 and 11); the research, awards and shows rules read 0. The
//! triggers are stubs but for trigger 2 (popups and elements): a trigger logs `scenario trigger`
//! and does nothing else. The objectives list (4.5) and the end (5.4, 9) are here; saving comes
//! with a later issue.
//!
//! Trigger 2 (scenarios 7.1) and the interface rules 10 and 11 reach the interface through
//! [`gui::Gui`]: a trigger leaves an [`Effect`] for the application, which tells back which
//! elements are shown and which were pressed. Popups and blocks are read when the goal is made.
//!
//! The log (family `sim`): `scenario start FILE GOALS` when a scenario is prepared, `scenario
//! trigger WHO STATE TRULEA TRULEB TARGA TARGB` for each trigger fired with a non-zero `trulea`,
//! and `scenario end won|lost`; the application adds `scenario popup LAYOUT modal|open`,
//! `scenario hint LAYOUT` and `scenario element ID disable|enable|hide|show|press` for what it
//! did to the interface.

use crate::config::{Config, Section, to_int};
use crate::vfs::Vfs;

pub use apply::Marks;
pub use gui::{Effect, Gui};
pub use popup::{Hint, Op, Popup};
pub use rules::Game;

mod apply;
mod gui;
mod popup;
mod rules;

#[cfg(test)]
mod tests;

/// The question layout a trigger 2 of `truleb` 11 shows.
const QUESTION: i32 = 45000;
/// The milliseconds the manager gathers between two passes.
const PERIOD: u32 = 1500;
/// Goals one scenario holds, children included, and how deep goals nest: a hostile file whose
/// sections name each other must not grow the list without bound.
const MAX_GOALS: u32 = 4096;
const MAX_DEPTH: u32 = 16;

/// What the rules read from the game. Without a `game` the rules that read the zoo give 0.
#[derive(Default)]
pub struct Env<'a> {
    /// The accounts' month index (1 to 12) and year index (0 to 19).
    pub month: i32,
    pub year: i32,
    pub game: Option<Game<'a>>,
    cache: rules::Cache,
}

impl<'a> Env<'a> {
    pub fn new(month: i32, year: i32, game: Option<Game<'a>>) -> Env<'a> {
        Env {
            month,
            year,
            game,
            cache: Default::default(),
        }
    }

    /// The month index plus 12 times the year index: 1 on the first day of a scenario.
    fn total_months(&self) -> i32 {
        self.month.saturating_add(self.year.saturating_mul(12))
    }
}

/// Whether the manager runs, or how the scenario ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    On,
    Won,
    Lost,
}

/// What a goal does when its state changes: `trulea`, `truleb`, `targa`, `targb`. Only trigger 2
/// does anything; the others are stubs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Trigger {
    v: [i32; 4],
    /// What trigger 2 leaves for the interface.
    effect: Option<Box<Effect>>,
}

impl Trigger {
    fn read(cfg: &Config, name: &[u8]) -> Trigger {
        let s = cfg.section(name);
        let v = ["trulea", "truleb", "targa", "targb"].map(|k| int(s, k));
        let effect = (v[0] == 2).then(|| effect(cfg, v)).flatten().map(Box::new);
        Trigger { v, effect }
    }

    fn fire(&self, who: &str, state: &str, log: &mut Vec<String>, gui: &Gui) {
        if self.v[0] != 0 {
            let [a, b, c, d] = self.v;
            log.push(format!("scenario trigger {who} {state} {a} {b} {c} {d}"));
        }
        self.run(true, gui);
    }

    /// The trigger runs, fresh or again after a load (7).
    fn run(&self, fresh: bool, gui: &Gui) {
        let effect = self.effect.as_deref().cloned();
        if let Some(effect) = effect.and_then(|e| if fresh { Some(e) } else { e.quiet() }) {
            gui.push(effect);
        }
    }
}

/// Trigger 2 (7.1): `truleb` says what, `targa` and `targb` which element or block and which popup.
fn effect(cfg: &Config, [_, b, a, p]: [i32; 4]) -> Option<Effect> {
    let popup = || Popup::read(cfg, p);
    let one = |op| Effect::Set {
        ops: vec![(a, op)],
        popup: popup(),
        always: false,
    };
    let block = |op| Effect::Set {
        ops: popup::ids(cfg, a).into_iter().map(|i| (i, op)).collect(),
        popup: popup(),
        always: false,
    };
    Some(match b {
        0 => Effect::Popup(Popup::read(cfg, a)?),
        1 | 2 => Effect::Press(a, popup()),
        3 | 4 => one(Op::Disable),
        5 | 6 => one(Op::Enable),
        7 | 8 => one(Op::Hide),
        9 | 10 => one(Op::Show),
        11 => Effect::Popup(Popup {
            layout: QUESTION,
            short: [a, 0],
            ok: true,
            modal: p > 0,
            ..Popup::default()
        }),
        12 => block(Op::Disable),
        13 => block(Op::Enable),
        14 => block(Op::Hide),
        15 => block(Op::Show),
        16 => Effect::Set {
            ops: popup::entries(cfg, a),
            popup: popup(),
            always: true,
        },
        _ => return None,
    })
}

/// A line of the objectives list (4.5): the string of its text, the values that go into it and
/// the state of its goal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: i32,
    pub values: [i32; 3],
    pub state: u8,
}

/// A simple goal: one rule compared with `value` by `kind`.
#[derive(Clone, Debug)]
struct Simple {
    /// `text` and `hidden`: the goal shows a line when it has a text and is not hidden.
    text: i32,
    hidden: bool,
    /// `rulea`, `ruleb`, `arga`, `argb`.
    rule: [i32; 4],
    kind: i32,
    value: i32,
    sticky: bool,
    trigger: Trigger,
}

#[derive(Clone, Debug)]
enum Body {
    Timer(i32),
    Simple(Simple),
    Complex(Vec<Goal>),
    Chain(Vec<Goal>),
}

/// One entry of the list. The state is 0 in progress, 1 completed, 2 failed, 3 completed and over.
#[derive(Clone, Debug)]
pub struct Goal {
    name: String,
    state: u8,
    optional: bool,
    /// The string id of the goal's line (`text`); 0 for none.
    text: i32,
    body: Body,
}

fn int(s: Option<&Section>, key: &str) -> i32 {
    s.and_then(|s| s.get(key)).and_then(to_int).unwrap_or(0)
}

impl Simple {
    /// The state the rule value gives (4.2), or none for a type that is not 0 to 8.
    fn judge(&self, r: i32) -> Option<u8> {
        let (met, yes, no) = match self.kind {
            0..=8 => {
                let met = match self.kind % 3 {
                    0 => r >= self.value,
                    1 => r < self.value,
                    _ => r == self.value,
                };
                match self.kind / 3 {
                    0 => (met, 1, 0),
                    1 => (met, 2, 0),
                    _ => (met, 1, 2),
                }
            }
            _ => return None,
        };
        Some(if met { yes } else { no })
    }
}

impl Goal {
    fn make(cfg: &Config, name: &[u8], depth: u32, left: &mut u32) -> Option<Goal> {
        if depth > MAX_DEPTH || *left == 0 {
            return None;
        }
        *left -= 1;
        // A name with no section makes the empty simple goal, which completes at the first pass.
        let s = cfg.section(name);
        let children = |left: &mut u32| -> Vec<Goal> {
            let names = s.map_or(&[][..], |s| s.get_all("child"));
            let made = names.iter().map(|n| Goal::make(cfg, n, depth + 1, left));
            made.flatten().collect()
        };
        let body = match int(s, "rtype") {
            1 => Body::Complex(children(left)),
            2 => Body::Chain(children(left)),
            _ => Body::Simple(Simple {
                text: int(s, "text"),
                hidden: int(s, "hidden") != 0,
                rule: ["rulea", "ruleb", "arga", "argb"].map(|k| int(s, k)),
                kind: int(s, "type"),
                value: int(s, "value"),
                sticky: int(s, "sticky") != 0,
                trigger: Trigger::read(cfg, name),
            }),
        };
        Some(Goal {
            name: String::from_utf8_lossy(name).into_owned(),
            state: 0,
            optional: int(s, "optional") != 0,
            text: int(s, "text"),
            body,
        })
    }

    /// Registers the elements this goal's rule 10 reads whether they are shown (8.2).
    fn watch(&self, gui: &Gui) {
        match &self.body {
            Body::Timer(_) => {}
            Body::Simple(s) => {
                if s.rule[0] == 10 && s.rule[1] == 0 {
                    gui.watch(s.rule[2]);
                }
            }
            Body::Complex(c) | Body::Chain(c) => c.iter().for_each(|g| g.watch(gui)),
        }
    }

    /// The lines this goal adds to the objectives list: its own, or its children's.
    fn lines(&self, out: &mut Vec<Line>) {
        match &self.body {
            Body::Timer(_) => {}
            Body::Simple(s) if s.text != 0 && !s.hidden => out.push(Line {
                text: s.text,
                values: [s.value, s.rule[2], s.rule[3]],
                state: self.state,
            }),
            Body::Simple(_) => {}
            Body::Complex(c) | Body::Chain(c) => c.iter().for_each(|c| c.lines(out)),
        }
    }

    /// The state the manager and the groups see: 1 for an optional goal.
    fn reported(&self) -> u8 {
        if self.optional { 1 } else { self.state }
    }

    /// One update (4.2, 4.4). The timer is updated by the manager, which knows the other goals.
    fn update(&mut self, env: &Env, states: &[u8], gui: &Gui, log: &mut Vec<String>) {
        let new = match &mut self.body {
            Body::Timer(_) => return,
            Body::Simple(s) => {
                if s.sticky && self.state != 0 {
                    return;
                }
                s.judge(rules::eval(&s.rule, s.value, env, states, gui))
            }
            Body::Complex(children) => {
                for c in children.iter_mut() {
                    c.update(env, states, gui, log);
                }
                let of = |c: &Goal| c.reported();
                Some(if children.iter().any(|c| of(c) == 2) {
                    2
                } else if children.iter().any(|c| of(c) == 0) {
                    0
                } else {
                    1
                })
            }
            Body::Chain(children) => {
                let mut state = 1;
                for c in children.iter_mut() {
                    c.update(env, states, gui, log);
                    if c.reported() != 1 {
                        state = c.reported();
                        break;
                    }
                }
                Some(state)
            }
        };
        match (new, &self.body) {
            // Only a simple goal runs a trigger; a group just takes its state.
            (Some(s @ 1..=3), Body::Simple(simple)) if s != self.state => {
                self.state = s;
                simple.trigger.fire(&self.name, &s.to_string(), log, gui);
            }
            (Some(s), _) => self.state = s,
            (None, _) => {}
        }
    }
}

/// How a scenario ended (5.4): the file, the result and the types a win unlocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ended {
    pub file: String,
    pub won: bool,
    pub unlocks: Vec<i32>,
}

/// The seven tutorial files (9.1), in the order of their chain.
const TUTORIALS: [&str; 7] = [
    "scn01a", "scn01", "scn02", "scn19", "scn27", "scn28", "scn28b",
];

/// The name of a tutorial among the seven (9.1), from a scenario file path, or "".
fn tutorial(file: &str) -> &'static str {
    let name = file.replace('\\', "/").to_ascii_lowercase();
    TUTORIALS
        .into_iter()
        .find(|t| name == format!("scenario/{t}/{t}.scn"))
        .unwrap_or("")
}

/// Whether `file` is one of the seven tutorial files (9.1), listed or not.
pub fn is_tutorial(file: &str) -> bool {
    !tutorial(file).is_empty()
}

/// The file of the tutorial that follows the tutorial `file` (9.1), given the expansion flags
/// (bit 1 the first expansion, bit 2 the second). None for any other file or without a next one.
pub fn next_tutorial(file: &str, expansions: i32) -> Option<String> {
    let next = match tutorial(file) {
        "scn01a" => "scn01",
        "scn01" => "scn02",
        "scn02" if expansions & 1 != 0 => "scn19",
        "scn02" | "scn19" if expansions & 2 != 0 => "scn27",
        "scn27" => "scn28",
        "scn28" => "scn28b",
        _ => return None,
    };
    Some(format!("scenario/{next}/{next}.scn"))
}

/// The string of the end dialog's text (9.1) for the scenario `file` and the expansion flags
/// (bit 1 the first expansion, bit 2 the second), plus 1 when lost.
pub fn end_text(file: &str, won: bool, expansions: i32) -> i32 {
    let name = file.replace('\\', "/").to_ascii_lowercase();
    let base = match name.as_str() {
        "scenario/scn01/scn01.scn" | "scenario/scn01a/scn01a.scn" => 16104,
        "scenario/scn27/scn27.scn" | "scenario/scn28/scn28.scn" => 16104,
        "scenario/scn02/scn02.scn" if expansions != 0 => 16104,
        "scenario/scn19/scn19.scn" if expansions & 2 != 0 => 16104,
        "scenario/scn02/scn02.scn" | "scenario/scn19/scn19.scn" => 16102,
        "scenario/scn28b/scn28b.scn" => 16102,
        _ => 16100,
    };
    base + i32::from(!won)
}

/// A scenario being played.
#[derive(Debug)]
pub struct Scenario {
    file: String,
    /// `[desc]`: the string of the name, the name of the story's text, the pictures of the three
    /// goal states (in progress, completed, failed).
    name: i32,
    story: Vec<u8>,
    icons: [String; 3],
    /// `[duration] text`: the string of the timer's line.
    timer_text: i32,
    /// `[completion] unlock`: the types a win makes available.
    unlocks: Vec<i32>,
    /// Whether the end was told.
    told: bool,
    goals: Vec<Goal>,
    /// The state each goal of the list reports, kept in step with the goals for rule 9.
    reported: Vec<u8>,
    /// The sections `[start] triggers` names, fired at the start.
    start: Vec<(String, Trigger)>,
    gathered: u32,
    run: Run,
    gui: Gui,
}

impl Scenario {
    /// Reads and prepares the scenario `file` (4.1). `None` when the file cannot be read. An
    /// extra file that cannot be read is skipped.
    pub fn prepare(vfs: &Vfs, file: &str) -> Option<Scenario> {
        let read = |name: &str| Config::parse(&vfs.read(name).ok()?).ok();
        let main = read(file)?;
        // An `extragoals` value is a pattern: the text before its last dot is the prefix, the dot
        // and the rest the suffix. A value with no dot, or ending in one, lists nothing.
        let extra = |s: Option<&Section>| -> Vec<Config> {
            let values = s.map_or(&[][..], |s| s.get_all("extragoals"));
            let names = values.iter().flat_map(|v| {
                let value = String::from_utf8_lossy(v);
                match value.rfind('.') {
                    Some(dot) if dot + 1 < value.len() => vfs.listed(&value[..dot], &value[dot..]),
                    _ => Vec::new(),
                }
            });
            names.filter_map(|n| read(&n)).collect()
        };
        Some(Scenario::build(file, &main, &extra(main.section("start"))))
    }

    fn build(file: &str, main: &Config, extras: &[Config]) -> Scenario {
        let mut left = MAX_GOALS;
        let months = int(main.section("duration"), "nummonths");
        let timer = Goal {
            name: "timer".to_string(),
            // No limit stated: nothing to wait for.
            state: u8::from(months == 0),
            optional: false,
            text: 0,
            body: Body::Timer(months),
        };
        let mut goals = vec![timer];
        for cfg in std::iter::once(main).chain(extras) {
            let names = cfg.section("goals").map_or(&[][..], |s| s.get_all("goal"));
            goals.extend(
                names
                    .iter()
                    .filter_map(|n| Goal::make(cfg, n, 0, &mut left)),
            );
        }
        let names = main
            .section("start")
            .map_or(&[][..], |s| s.get_all("triggers"));
        let start = names.iter().map(|n| {
            let trigger = Trigger::read(main, n);
            (String::from_utf8_lossy(n).into_owned(), trigger)
        });
        // The elements rule 10 reads are known from the start, so the first sync tells them.
        let gui = Gui::default();
        goals.iter().for_each(|g| g.watch(&gui));
        let desc = main.section("desc");
        let icon = |k: &str| {
            let name = desc.and_then(|d| d.get(k)).unwrap_or_default();
            crate::vfs::name_from_bytes(name)
        };
        let unlock = main
            .section("completion")
            .map_or(&[][..], |s| s.get_all("unlock"));
        Scenario {
            file: file.to_string(),
            reported: goals.iter().map(Goal::reported).collect(),
            name: int(desc, "name"),
            story: desc
                .and_then(|d| d.get("story"))
                .unwrap_or_default()
                .to_vec(),
            icons: ["iconp", "iconc", "iconf"].map(icon),
            timer_text: int(main.section("duration"), "text"),
            unlocks: unlock.iter().filter_map(|v| to_int(v)).collect(),
            told: false,
            goals,
            start: start.collect(),
            gathered: 0,
            run: Run::On,
            gui,
        }
    }

    /// What the scenario and the interface tell one another.
    pub fn gui(&self) -> &Gui {
        &self.gui
    }

    pub fn run(&self) -> Run {
        self.run
    }

    /// The scenario file.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The string id of the name and the name of the text of the story (`[desc]`).
    pub fn title(&self) -> (i32, &[u8]) {
        (self.name, &self.story)
    }

    /// The picture names of a goal in progress, completed and failed.
    pub fn icons(&self) -> &[String; 3] {
        &self.icons
    }

    /// The lines of the objectives list (4.5): the timer's, when it has a text, then the goals'.
    pub fn lines(&self) -> Vec<Line> {
        let mut out = Vec::new();
        if let Some(Goal {
            body: Body::Timer(months),
            state,
            ..
        }) = self.goals.first()
            && self.timer_text != 0
        {
            out.push(Line {
                text: self.timer_text,
                values: [*months, 0, 0],
                state: *state,
            });
        }
        self.goals.iter().for_each(|g| g.lines(&mut out));
        out
    }

    /// How the scenario ended, once, when it has (5.4).
    pub fn ended(&mut self) -> Option<Ended> {
        let won = match self.run {
            Run::On => return None,
            Run::Won => true,
            Run::Lost => false,
        };
        let told = std::mem::replace(&mut self.told, true);
        (!told).then(|| Ended {
            file: self.file.clone(),
            won,
            unlocks: if won {
                self.unlocks.clone()
            } else {
                Vec::new()
            },
        })
    }

    /// The start triggers fire, in the order listed.
    pub fn start(&self, log: &mut Vec<String>) {
        log.push(format!("scenario start {} {}", self.file, self.goals.len()));
        for (name, trigger) in &self.start {
            trigger.fire(name, "start", log, &self.gui);
        }
    }

    /// The timer is the first goal; the others are updated in list order.
    fn update_goal(&mut self, at: usize, env: &Env, log: &mut Vec<String>) {
        let total = env.total_months();
        let gui = &self.gui;
        // The line a hint of -1 shows: the first goal in progress that has one (8.3).
        let first = self.goals.iter().find(|g| g.reported() == 0 && g.text > 0);
        gui.set_goal_text(first.map_or(0, |g| g.text));
        let (timer, rest) = self.goals.split_first_mut().expect("the timer");
        let seen = &mut self.reported;
        if at > 0 {
            rest[at - 1].update(env, seen, gui, log);
            seen[at] = rest[at - 1].reported();
            return;
        }
        if let Body::Timer(months) = timer.body
            && timer.state == 0
            && (0..total).contains(&months)
        {
            timer.state = 3;
            seen[0] = 3;
            for (i, g) in rest.iter_mut().enumerate() {
                g.update(env, seen, gui, log);
                seen[i + 1] = g.reported();
            }
            if rest.iter().any(|g| !matches!(g.reported(), 1 | 3)) {
                timer.state = 2;
            }
            seen[0] = timer.reported();
        }
    }

    /// A step of `ms` milliseconds (5.1): from 1500 ms gathered the pass updates every goal and
    /// reads its state; a failed or finished goal, or none in progress, ends the scenario.
    pub fn update(&mut self, ms: u32, env: &Env, log: &mut Vec<String>) {
        if self.run != Run::On {
            return;
        }
        self.gathered = self.gathered.saturating_add(ms);
        if self.gathered < PERIOD {
            return;
        }
        self.gathered = 0;
        let (mut open, mut over) = (false, false);
        for at in 0..self.goals.len() {
            self.update_goal(at, env, log);
            match self.goals[at].reported() {
                0 => open = true,
                2 | 3 => {
                    over = true;
                    break;
                }
                _ => {}
            }
        }
        if over || !open {
            for at in 0..self.goals.len() {
                self.update_goal(at, env, log);
            }
            let won = self.goals.iter().all(|g| matches!(g.reported(), 1 | 3));
            self.run = if won { Run::Won } else { Run::Lost };
            log.push(format!("scenario end {}", if won { "won" } else { "lost" }));
        }
    }
}

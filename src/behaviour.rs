//! The behaviour sets of the unit types: the lists of calls (`fPlay(stand)`, `fWalk(0,0)`, ...)
//! that make a unit act, and the ambient tables that pick a set from the unit's happiness.
//!
//! The original parses a call when it becomes current and never rejects one: a call whose name
//! it does not know or whose arguments are wrong fails when run, and a failed call ends its
//! set. This module parses every set when the definition file loads, which is the same as long
//! as nothing that fails when run is refused:
//!
//! * a name of the 53 of the program, and the three misspelt names of the shipped data
//!   (`fFollow`, `fPlayWith`, `fPlayWithSoud`), become an [`Op`]; the three names, and every
//!   call whose arguments break the rule of its operation, become [`Op::Fail`], with a warning
//!   for the arguments;
//! * any other name is a [`BehaviourError`] that names the section and the call.
//!
//! The runner of the sets is in [`run`], and the data of the shipped game for it in [`library`].

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::config::{Config, to_int};
use crate::vfs::Vfs;

mod library;
mod run;
pub use library::{Library, Timings};
pub use run::{Animation, Data, Env, NoData, Note, Runner, Timing};

/// How many of [`NAMES`] the program knows.
pub const KNOWN: usize = 53;

/// The 53 operations of the program, then the three names of the shipped data it does not know
/// (`fFollow`, `fPlayWith`, `fPlayWithSoud`).
#[rustfmt::skip]
pub const NAMES: [&str; 56] = [
    "fWalk", "fSwim", "fTrot", "fRun", "fSpray", "fKeeperRoutine", "fGuestRoutine",
    "fGuideRoutine", "fMaintRoutine", "fFaceTowardFood", "fFaceTowardTarget", "fShowPredator",
    "fHidePredator", "fEnterBuilding", "fExitBuilding", "fReturnToBuilding", "fDustBall",
    "fKillPrey", "fCaughtPrey", "fDie", "fTargetBuilding", "fHatch", "fEndTrick",
    "fPlaySetTank", "fPlay", "fSetBase", "fPlayWithBase", "fMoveEntityDelta",
    "fMoveEntitySpecial", "fPlayTime", "fPlaySound", "fPlaySpecial", "fPlaySp", "fPlaySetProb",
    "fPlaySetTerrain", "fPlayReverse", "fPlayPingPong", "fPingPong", "fPlayWithSound",
    "fPlaySFX", "fPlayStopAtEnd", "fPlaySet", "fMoveSlow", "fMoveMedium", "fMoveFast", "fMove",
    "fMoveReverse", "fMoveWithSound", "fFaceDirection", "fFly", "fFlyReverse", "fKeepControl",
    "fHalt", "fFollow", "fPlayWith", "fPlayWithSoud",
];

/// How a play call plays its animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Play {
    Forward,
    Reverse,
    /// Forward, then reversed.
    PingPong,
    /// Forward, and the last frame stays.
    StopAtEnd,
}

/// A number of a call: written as a number (read leniently), or as the name of a characteristic
/// of the unit's type (`cWalkRandomlyTime1`, `cRunRandomlyTime3`, `cInformGuestTime`, ...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Number {
    Int(i32),
    Characteristic(String),
}

/// The 3D target of a move: only swimmers and flyers use it. Any other first argument (`0`,
/// `keeperfood`, `otherfood`, `mother`, `prey`, `keeper`) is not interpreted by the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    None,
    Level,
    Surface,
    Dive,
    Rise,
    Floor,
    Climb,
    Fly,
}

/// The animation, speed and animation speed of a move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gait {
    /// The type's slow, medium or fast move animation and rates.
    Slow,
    Medium,
    Fast,
    /// `fFly`: a flying unit's move.
    Fly,
    /// `fMove` and the like: the call names the animation, the speed and the percentage of the
    /// animation speed (100 if absent), and perhaps a sound.
    Own {
        anim: String,
        speed: Number,
        percent: Number,
        sound: Option<String>,
    },
}

/// Which unit class a routine call hands to its goals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Routine {
    Keeper,
    Guest,
    Guide,
    Maint,
}

/// One call of a set, parsed.
#[rustfmt::skip]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// One or more animations, in order.
    Play(Play, Vec<String>),
    /// An animation with a sound named in the type's `[Sounds]`.
    PlayWithSound {
        anim: String,
        sound: String,
    },
    PlayTime {
        anim: String,
        time: Number,
    },
    SetBase(String),
    PlayWithBase {
        base: String,
        anims: Vec<String>,
    },
    PlaySet(String),
    /// Set names with weights that add up to 100.
    PlaySetProb(Vec<(String, i32)>),
    /// A set for land and one for water, and, with four arguments, one for the surface of a tank
    /// and one under its water.
    PlaySetTerrain {
        land: String,
        water: String,
        tank: Option<(String, String)>,
    },
    PlaySetTank {
        land: String,
        tank: String,
    },
    Move {
        gait: Gait,
        reverse: bool,
        target: Target,
        time: Number,
    },
    Routine(Routine),
    FaceFood, FaceTarget, FaceDirection, Halt, KeepControl, Spray, Die, Hatch, DustBall, KillPrey,
    CaughtPrey, ShowPredator, HidePredator, EnterBuilding, ExitBuilding, ReturnToBuilding,
    TargetBuilding, EndTrick,
    /// A call of the shows (`fMoveEntityDelta`, `fMoveEntitySpecial`, `fPlaySpecial`, `fPlaySp`,
    /// `fPlaySound`), which M6 does not run; the arguments as written.
    Show(Vec<String>),
    /// A name the program does not know, or arguments that break the rule of the operation: the
    /// call fails when it runs and ends its set.
    Fail,
}

/// A call and the name it was written with (one of [`NAMES`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub name: &'static str,
    pub op: Op,
}

/// One row of an ambient table: the set for a happiness from `lo` to `hi`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub lo: i32,
    pub hi: i32,
    pub set: String,
}

/// The four ambient tables of a type file, in this order.
pub const TABLES: [&str; 4] = [
    "AmbientAnims",
    "AmbientAnimsWater",
    "AmbientAnimsUnderwater",
    "AmbientAnimsSurface",
];

/// The sets and ambient tables of one definition file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Behaviours {
    /// The sets by section name (`f\BehaviorSet\bHappy`); a section without an `f` value is a
    /// set without calls, which counts as absent when a set is looked up.
    sets: BTreeMap<Vec<u8>, Vec<Call>>,
    /// The rows of the tables of [`TABLES`]; empty for a table that is absent or invalid.
    pub tables: [Vec<Row>; 4],
    /// One line each for a call whose arguments make it fail and for an invalid table.
    pub warnings: Vec<String>,
}

/// A call whose name is not one the program interprets, or that has no name.
#[derive(Debug, PartialEq, Eq)]
pub struct BehaviourError {
    pub section: String,
    /// The position of the call among the `f` values of the section, from 1.
    pub call: usize,
    pub text: String,
}

impl BehaviourError {
    /// The line of the call in the file `bytes` was read from, from 1.
    pub fn line(&self, bytes: &[u8]) -> Option<usize> {
        let mut section = None;
        let mut seen = 0;
        for (n, line) in bytes.split(|&b| b == b'\n').enumerate() {
            let line = line.trim_ascii_start();
            if let Some(header) = line.strip_prefix(b"[") {
                let end = header.iter().position(|&b| b == b']')?;
                section = Some(header[..end].trim_ascii());
            } else if section == Some(self.section.as_bytes())
                && let Some(equals) = line.iter().position(|&b| b == b'=')
                && line[..equals].trim_ascii() == b"f"
            {
                seen += 1;
                if seen == self.call {
                    return Some(n + 1);
                }
            }
        }
        None
    }
}

impl fmt::Display for BehaviourError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "section [{}], call {}: unknown operation in {}",
            self.section, self.call, self.text
        )
    }
}

impl Error for BehaviourError {}

/// The name and the arguments of a call text (specification 3.2). The name is `None` when the
/// text has no `(` or its only `(` is its last byte.
fn split_call(text: &[u8]) -> (Option<&[u8]>, Vec<String>) {
    let Some(open) = text.iter().position(|&b| b == b'(') else {
        return (None, Vec::new());
    };
    let name = (open + 1 < text.len()).then(|| &text[..open]);
    let rest = &text[open + 1..];
    let Some(rest) = rest.strip_suffix(b")") else {
        return (name, Vec::new());
    };
    let rest: Vec<u8> = rest
        .iter()
        .copied()
        .filter(|&b| b != b' ' && b != b'"')
        .collect();
    // Every token is appended before the end is tested: `f()` has one empty argument. A trailing
    // comma adds nothing.
    let mut args: Vec<String> = rest
        .split(|&b| b == b',')
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    if rest.ends_with(b",") {
        args.pop();
    }
    (name, args)
}

/// A number of a call argument, read like C `atol`: white space, a sign, decimal digits up to
/// the first other character, 0 if there are none. The `0x` rule of the configuration reader
/// does not apply: `0x64` is 0.
fn atol(text: &str) -> i32 {
    let text = text.trim_ascii_start();
    let (negative, digits) = match text.as_bytes().split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, text.as_bytes()),
    };
    let number = digits
        .iter()
        .map_while(|&b| char::from(b).to_digit(10))
        .fold(0i32, |n, d| n.wrapping_mul(10).wrapping_add(d as i32));
    if negative {
        number.wrapping_neg()
    } else {
        number
    }
}

fn number(text: &str) -> Number {
    match text {
        "cWalkRandomlyTime1" | "cWalkRandomlyTime2" | "cWalkRandomlyTime3"
        | "cRunRandomlyTime1" | "cRunRandomlyTime2" | "cRunRandomlyTime3" | "cInformGuestTime" => {
            Number::Characteristic(text.to_string())
        }
        _ => Number::Int(atol(text)),
    }
}

fn target(text: &str) -> Target {
    match text {
        "level" => Target::Level,
        "surface" => Target::Surface,
        "dive" => Target::Dive,
        "rise" => Target::Rise,
        "floor" => Target::Floor,
        "climb" => Target::Climb,
        "fly" => Target::Fly,
        _ => Target::None,
    }
}

/// The operation `name` (one of [`NAMES`]) with these arguments; the reason when they break the
/// rule of the operation.
fn build(name: &str, args: &[String]) -> Result<Op, &'static str> {
    let n = args.len();
    let first = || args.first().cloned().unwrap_or_default();
    let want = |ok: bool, reason| if ok { Ok(()) } else { Err(reason) };
    let to_move = |gait: Gait, reverse: bool| Op::Move {
        gait,
        reverse,
        target: target(&args[0]),
        time: number(&args[1]),
    };
    Ok(match name {
        "fPlay" | "fPlayReverse" | "fPlayPingPong" | "fPingPong" | "fPlayStopAtEnd" => {
            want(n >= 1, "no animation")?;
            let how = match name {
                "fPlay" => Play::Forward,
                "fPlayReverse" => Play::Reverse,
                "fPlayStopAtEnd" => Play::StopAtEnd,
                _ => Play::PingPong,
            };
            Op::Play(how, args.to_vec())
        }
        "fPlayWithSound" | "fPlaySFX" => {
            want(n >= 1, "no animation")?;
            Op::PlayWithSound {
                anim: first(),
                sound: args[n - 1].clone(),
            }
        }
        "fPlayTime" => {
            want(n == 2, "needs an animation and a time")?;
            Op::PlayTime {
                anim: first(),
                time: number(&args[1]),
            }
        }
        "fSetBase" => {
            want(n == 1, "needs one animation")?;
            Op::SetBase(first())
        }
        "fPlayWithBase" => {
            want(n >= 2, "needs a base and an animation")?;
            Op::PlayWithBase {
                base: first(),
                anims: args[1..].to_vec(),
            }
        }
        "fPlaySet" => {
            want(n == 1, "needs one set")?;
            Op::PlaySet(first())
        }
        "fPlaySetProb" => {
            want(
                n >= 2 && n.is_multiple_of(2),
                "needs pairs of a set and a weight",
            )?;
            let rows: Vec<(String, i32)> = args
                .chunks(2)
                .map(|p| (p[0].clone(), atol(&p[1])))
                .collect();
            let sum: i64 = rows.iter().map(|r| i64::from(r.1)).sum();
            want(sum == 100, "the weights do not add up to 100")?;
            Op::PlaySetProb(rows)
        }
        "fPlaySetTerrain" => {
            want(n == 2 || n == 4, "needs 2 or 4 sets")?;
            Op::PlaySetTerrain {
                land: first(),
                water: args[1].clone(),
                tank: (n == 4).then(|| (args[2].clone(), args[3].clone())),
            }
        }
        "fPlaySetTank" => {
            want(n == 2, "needs 2 sets")?;
            Op::PlaySetTank {
                land: first(),
                tank: args[1].clone(),
            }
        }
        "fWalk" | "fSwim" | "fMoveSlow" | "fTrot" | "fMoveMedium" | "fRun" | "fMoveFast"
        | "fFly" | "fFlyReverse" => {
            want(n == 2, "needs a target and a time")?;
            let gait = match name {
                "fWalk" | "fSwim" | "fMoveSlow" => Gait::Slow,
                "fTrot" | "fMoveMedium" => Gait::Medium,
                "fRun" | "fMoveFast" => Gait::Fast,
                _ => Gait::Fly,
            };
            to_move(gait, name == "fFlyReverse")
        }
        "fMove" | "fMoveReverse" | "fMoveWithSound" => {
            let sound = name == "fMoveWithSound";
            want(
                n == 4 + usize::from(sound) || n == 5 + usize::from(sound),
                "wrong number of arguments",
            )?;
            let percent = args
                .get(4 + usize::from(sound))
                .map_or(Number::Int(100), |p| number(p));
            to_move(
                Gait::Own {
                    anim: args[2].clone(),
                    speed: number(&args[3]),
                    percent,
                    sound: sound.then(|| args[4].clone()),
                },
                name == "fMoveReverse",
            )
        }
        "fKeeperRoutine" => Op::Routine(Routine::Keeper),
        "fGuestRoutine" => Op::Routine(Routine::Guest),
        "fGuideRoutine" => Op::Routine(Routine::Guide),
        "fMaintRoutine" => Op::Routine(Routine::Maint),
        "fFaceTowardFood" => Op::FaceFood,
        "fFaceTowardTarget" => Op::FaceTarget,
        "fFaceDirection" => {
            want(n == 1 && !args[0].is_empty(), "needs one direction")?;
            Op::FaceDirection
        }
        "fHalt" => Op::Halt,
        "fKeepControl" => Op::KeepControl,
        "fSpray" => Op::Spray,
        "fDie" => Op::Die,
        "fHatch" => Op::Hatch,
        "fDustBall" => Op::DustBall,
        "fKillPrey" => Op::KillPrey,
        "fCaughtPrey" => Op::CaughtPrey,
        "fShowPredator" => Op::ShowPredator,
        "fHidePredator" => Op::HidePredator,
        "fEnterBuilding" => Op::EnterBuilding,
        "fExitBuilding" => Op::ExitBuilding,
        "fReturnToBuilding" => Op::ReturnToBuilding,
        "fTargetBuilding" => Op::TargetBuilding,
        "fEndTrick" => Op::EndTrick,
        "fMoveEntityDelta" | "fMoveEntitySpecial" | "fPlaySpecial" | "fPlaySp" | "fPlaySound" => {
            Op::Show(args.to_vec())
        }
        // fFollow, fPlayWith, fPlayWithSoud.
        _ => Op::Fail,
    })
}

/// Parses one call text. `None` when the name is not in [`NAMES`]; otherwise the call and, when
/// the arguments make it fail, the reason.
pub fn parse_call(text: &[u8]) -> Option<(Call, Option<&'static str>)> {
    let (name, args) = split_call(text);
    let name = NAMES.iter().find(|n| Some(n.as_bytes()) == name)?;
    Some(match build(name, &args) {
        Ok(op) => (Call { name, op }, None),
        Err(why) => (Call { name, op: Op::Fail }, Some(why)),
    })
}

impl Behaviours {
    /// Reads the sets and tables of a definition file. Stops at the first call with an unknown
    /// name.
    pub fn parse(config: &Config) -> Result<Behaviours, BehaviourError> {
        let mut found = Behaviours::default();
        for (name, section) in config.sections() {
            if name.starts_with(b"BehaviorSet\\")
                || name.windows(13).any(|w| w == b"\\BehaviorSet\\")
            {
                let mut calls = Vec::new();
                for (i, text) in section.get_all("f").iter().enumerate() {
                    let section = || String::from_utf8_lossy(name).into_owned();
                    let (call, defect) = parse_call(text).ok_or_else(|| BehaviourError {
                        section: section(),
                        call: i + 1,
                        text: String::from_utf8_lossy(text).into_owned(),
                    })?;
                    if let Some(why) = defect {
                        found.warnings.push(format!(
                            "section [{}], call {}: {} fails when run: {why}",
                            section(),
                            i + 1,
                            String::from_utf8_lossy(text)
                        ));
                    }
                    calls.push(call);
                }
                found.sets.insert(name.to_vec(), calls);
            }
        }
        for (table, name) in found.tables.iter_mut().zip(TABLES) {
            let Some(section) = config.section(name) else {
                continue;
            };
            let (bounds, sets) = (section.get_all("a"), section.get_all("b"));
            // Valid with at least one row and twice as many bounds as sets.
            if sets.is_empty() || bounds.len() != 2 * sets.len() {
                found
                    .warnings
                    .push(format!("table [{name}] is not valid, ignored"));
                continue;
            }
            let bound = |i: usize| to_int(&bounds[i]).unwrap_or(0);
            *table = sets
                .iter()
                .enumerate()
                .map(|(i, set)| Row {
                    lo: bound(2 * i),
                    hi: bound(2 * i + 1),
                    set: String::from_utf8_lossy(set).into_owned(),
                })
                .collect();
        }
        Ok(found)
    }

    /// The sets in ascending order of their section name.
    pub fn sets(&self) -> impl Iterator<Item = (&[u8], &[Call])> {
        self.sets.iter().map(|(n, c)| (n.as_slice(), c.as_slice()))
    }

    /// The first of `prefixes` (a subtype name, or empty for the unprefixed form) that has a
    /// set `name` with at least one call.
    pub fn find(&self, prefixes: &[&[u8]], name: &[u8]) -> Option<&[Call]> {
        prefixes.iter().find_map(|prefix| {
            let mut section = prefix.to_vec();
            if !prefix.is_empty() {
                section.push(b'\\');
            }
            section.extend_from_slice(b"BehaviorSet\\");
            section.extend_from_slice(name);
            self.sets
                .get(&section)
                .filter(|c| !c.is_empty())
                .map(Vec::as_slice)
        })
    }
}

/// What the data holds: the numbers of the specification's survey.
#[derive(Debug, Default)]
pub struct Survey {
    /// Files with at least one set or table.
    pub files: usize,
    pub sets: usize,
    pub calls: usize,
    pub tables: usize,
    /// Calls of the names the program does not know, and calls whose arguments make them fail.
    pub unknown: usize,
    pub defects: usize,
    /// Calls per name.
    pub counts: BTreeMap<&'static str, usize>,
    /// The warnings of every file, each with its file name.
    pub warnings: Vec<String>,
}

/// Reads every copy (also those another archive hides) of every `.ai`, `.uca`, `.ucb` and `.ucs`
/// file of the installation. The message names the file, the section and the line of the first
/// call with an unknown name, or the file that cannot be read.
pub fn survey(vfs: &Vfs) -> Result<Survey, String> {
    let mut survey = Survey::default();
    for member in vfs.list(None) {
        if ![".ai", ".uca", ".ucb", ".ucs"]
            .iter()
            .any(|e| member.ends_with(e))
        {
            continue;
        }
        for bytes in vfs.read_copies(&member).map_err(|e| e.to_string())? {
            let config = Config::parse(&bytes).map_err(|e| format!("{member}: {e}"))?;
            let found = Behaviours::parse(&config).map_err(|e| {
                let at = e
                    .line(&bytes)
                    .map_or_else(String::new, |n| format!(" line {n}"));
                format!("{member}{at}: {e}")
            })?;
            survey
                .warnings
                .extend(found.warnings.iter().map(|w| format!("{member}: {w}")));
            let tables = found.tables.iter().filter(|t| !t.is_empty()).count();
            survey.files += usize::from(found.sets().next().is_some() || tables > 0);
            survey.tables += tables;
            for (_, set) in found.sets() {
                survey.sets += 1;
                survey.calls += set.len();
                for call in set {
                    *survey.counts.entry(call.name).or_default() += 1;
                    if call.op == Op::Fail {
                        survey.unknown += usize::from(!NAMES[..KNOWN].contains(&call.name));
                        survey.defects += usize::from(NAMES[..KNOWN].contains(&call.name));
                    }
                }
            }
        }
    }
    Ok(survey)
}

#[cfg(test)]
mod tests;

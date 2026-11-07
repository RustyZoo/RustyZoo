//! The loops of the soundscape (sound 8.1): the world loop, which plays for the whole game, and
//! the crowd loop, which follows the number of guests with a hysteresis and cross-fades from one
//! file to the next over ten seconds of simulation. The loops draw no random number. The shell
//! plays them and keeps the original's bug: a loop is tried once, at its start, and a start whose
//! sum is over 6,000 never happens (sound 4.2).

use crate::config::{Config, to_int};
use crate::vfs::name_from_bytes;

/// The fade term a crowd loop starts with and the old one ends with (sound 8.1).
const FADE_FROM: i32 = 4500;
/// The length of a cross-fade, in milliseconds of simulation steps.
const FADE_MS: u32 = 10_000;
/// The base term of a crowd loop whose `...atten` key is missing.
const CROWD_ATTEN: i32 = 1500;

/// A loop the shell has to keep: the voice (a new one for every start), the file and the sum of
/// the base and the fade terms, to which the shell adds the global one (sound 4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneLoop {
    pub voice: String,
    pub file: String,
    pub terms: i32,
}

/// The crowd loop's level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Quiet,
    Small,
    Medium,
    Large,
}

impl Level {
    const NAMES: [&'static str; 4] = ["quiet", "small", "medium", "large"];
    /// The `...atten` keys; the shipped file names the third one `mediumatten`, which is not read.
    const ATTEN: [&'static str; 4] = ["quietatten", "smallatten", "medatten", "largeatten"];

    /// The level a zoo of `guests` guests moves to from `from`, if it moves (sound 8.1).
    fn next(from: Option<Level>, guests: i32) -> Option<Level> {
        use Level::*;
        let to = match from {
            None => return Some(if guests <= 14 { Quiet } else { Small }),
            Some(Quiet) => (guests >= 21).then_some(Small),
            Some(_) if guests <= 14 => Some(Quiet),
            Some(Small) => (guests >= 86).then_some(Medium),
            Some(Medium) if guests >= 161 => Some(Large),
            Some(Medium) => (guests < 75).then_some(Small),
            Some(Large) => (guests < 150).then_some(Medium),
        };
        to.filter(|&to| Some(to) != from)
    }
}

/// A file and its base term; `None` when the file is not named.
type Tune = Option<(String, i32)>;

/// The files of the two loops: one world file, four crowd files by level.
#[derive(Debug, Default)]
pub(super) struct Files {
    world: Tune,
    crowd: [Tune; 4],
}

impl Files {
    /// Reads `[crowdsound]` of the crowd file and `[worldsound]` of the world file. A file that
    /// is not named plays nothing; a missing `...atten` key is 1,500 for a crowd loop and 0 for
    /// the world loop, a value that is not a number reads 0.
    pub(super) fn parse(crowd: &[u8], world: &[u8]) -> Files {
        let tune = |config: &Config, section: &str, name: &str, atten: &str, default: i32| {
            let section = config.section(section)?;
            let file = name_from_bytes(section.get(name)?);
            let atten = section
                .get(atten)
                .map_or(default, |v| to_int(v).unwrap_or(0));
            (!file.is_empty()).then_some((file, atten))
        };
        let crowd = Config::parse(crowd).unwrap_or_default();
        let world = Config::parse(world).unwrap_or_default();
        Files {
            world: tune(&world, "worldsound", "name", "atten", 0),
            crowd: std::array::from_fn(|i| {
                let (name, atten) = (Level::NAMES[i], Level::ATTEN[i]);
                tune(&crowd, "crowdsound", name, atten, CROWD_ATTEN)
            }),
        }
    }
}

/// A loop being played.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Playing {
    voice: String,
    file: String,
    base: i32,
    fade: i32,
}

/// The loops of a game.
#[derive(Debug, Default)]
pub(super) struct Loops {
    files: Files,
    /// The first update made the world loop.
    started: bool,
    world: Option<Playing>,
    /// The crowd's level, and the two slots of its loops: the newest is `slot`.
    level: Option<Level>,
    crowd: [Option<Playing>; 2],
    slot: usize,
    /// Milliseconds of the cross-fade under way.
    fading: Option<u32>,
    /// The voices made so far, so that every start has its own.
    serial: u32,
}

impl Loops {
    pub(super) fn new(files: Files) -> Loops {
        Loops {
            files,
            ..Loops::default()
        }
    }

    /// One simulation step of `step` ms with `guests` guests: the world loop is made at the first,
    /// a cross-fade under way advances (and nothing else changes meanwhile), else the crowd moves
    /// to another level when the guests ask. What happened goes to `log`.
    pub(super) fn update(&mut self, step: u32, guests: i32, log: &mut Vec<String>) {
        if !self.started {
            self.started = true;
            self.world = self.files.world.clone().map(|(file, base)| {
                log.push(format!("soundscape loop world {file}"));
                Playing {
                    voice: "world".to_string(),
                    file,
                    base,
                    fade: 0,
                }
            });
        }
        if let Some(ms) = self.fading {
            self.cross_fade(ms.saturating_add(step));
        } else if let Some(level) = Level::next(self.level, guests) {
            self.change(level, log);
        }
    }

    /// The crowd moves to `level`: its loop starts in the other slot with the whole fade term.
    fn change(&mut self, level: Level, log: &mut Vec<String>) {
        let slot = if self.level.is_some() {
            1 - self.slot
        } else {
            0
        };
        let i = level as usize;
        self.crowd[slot] = self.files.crowd[i].clone().map(|(file, base)| {
            log.push(format!("soundscape loop crowd {} {file}", Level::NAMES[i]));
            self.serial = self.serial.wrapping_add(1);
            Playing {
                voice: format!("crowd {}", self.serial),
                file,
                base,
                fade: FADE_FROM,
            }
        });
        (self.level, self.slot, self.fading) = (Some(level), slot, Some(0));
    }

    /// The cross-fade has lasted `ms`: the new loop's fade goes down to 0 as the old one's goes up
    /// to 4,500, then the old one is stopped.
    fn cross_fade(&mut self, ms: u32) {
        let out = (i64::from(FADE_FROM) * i64::from(ms.min(FADE_MS)) / i64::from(FADE_MS)) as i32;
        if let Some(old) = &mut self.crowd[1 - self.slot] {
            old.fade = out;
        }
        if let Some(new) = &mut self.crowd[self.slot] {
            new.fade = FADE_FROM - out;
        }
        self.fading = (ms < FADE_MS).then_some(ms);
        if self.fading.is_none() {
            self.crowd[1 - self.slot] = None;
        }
    }

    /// The loops playing now.
    pub(super) fn playing(&self) -> Vec<SceneLoop> {
        let all = [&self.world, &self.crowd[0], &self.crowd[1]];
        (all.into_iter().flatten())
            .map(|p| SceneLoop {
                voice: p.voice.clone(),
                file: p.file.clone(),
                terms: p.base.saturating_add(p.fade),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CROWD: &str = "[crowdsound]\nquiet=q.wav\nsmall=s.wav\nsmallatten=100\nmedium=m.wav\n\
        medatten=-50\nlarge=l.wav\nlargeatten=x\n";
    const WORLD: &str = "[worldsound]\nname=w.wav\natten=2000\n";

    fn loops() -> Loops {
        Loops::new(Files::parse(CROWD.as_bytes(), WORLD.as_bytes()))
    }

    /// The loops after one step per `(ms, guests)`, as `voice terms` pairs.
    fn run(l: &mut Loops, steps: &[(u32, i32)]) -> Vec<String> {
        let mut log = Vec::new();
        steps.iter().for_each(|&(ms, n)| l.update(ms, n, &mut log));
        log
    }

    fn terms(l: &Loops) -> Vec<(String, i32)> {
        l.playing().into_iter().map(|p| (p.file, p.terms)).collect()
    }

    fn level(guests: &[i32]) -> Option<Level> {
        guests
            .iter()
            .fold(None, |from, &n| Level::next(from, n).or(from))
    }

    #[test]
    fn the_files_are_read_with_their_attenuations_and_the_defaults() {
        let f = Files::parse(CROWD.as_bytes(), WORLD.as_bytes());
        assert_eq!(f.world, Some(("w.wav".to_string(), 2000)));
        let crowd: Vec<_> = f.crowd.iter().map(|t| t.clone().unwrap().1).collect();
        // The quiet one has no key: 1,500; the medium one is read from `medatten`; text is 0.
        assert_eq!(crowd, [1500, 100, -50, 0]);
        // `mediumatten`, as the shipped file spells it, is not read.
        let shipped = CROWD.replace("medatten", "mediumatten");
        let f = Files::parse(shipped.as_bytes(), b"");
        assert_eq!(f.crowd[2].as_ref().unwrap().1, 1500);
        // A name that is empty, a missing section or file, or text that is not a file: nothing.
        for bad in [
            "",
            "[worldsound]\nname=\natten=0\n",
            "[x]\nname=a.wav\n",
            "\u{0}\u{1}",
        ] {
            let f = Files::parse(bad.as_bytes(), bad.as_bytes());
            assert!(
                f.world.is_none() && f.crowd.iter().all(Option::is_none),
                "{bad:?}"
            );
        }
        // An atten key without a name does nothing; a huge value is kept as the integer it makes.
        let f = Files::parse(
            b"[crowdsound]\nquietatten=5\n",
            b"[worldsound]\nname=a\natten=99999999999\n",
        );
        assert!(f.crowd[0].is_none());
        assert_eq!(f.world.unwrap().1, to_int(b"99999999999").unwrap());
    }

    #[test]
    fn the_level_climbs_one_step_at_a_time_with_a_hysteresis() {
        use Level::*;
        // No loop yet: 14 guests are quiet, 15 are small.
        assert_eq!(
            [level(&[14]), level(&[15]), level(&[0]), level(&[-3])],
            [Some(Quiet), Some(Small), Some(Quiet), Some(Quiet)]
        );
        // Quiet stays up to 20, small starts at 21; 86 medium; 161 large.
        assert_eq!(level(&[0, 20]), Some(Quiet));
        assert_eq!(level(&[0, 21]), Some(Small));
        assert_eq!(level(&[15, 85]), Some(Small));
        assert_eq!(level(&[15, 86]), Some(Medium));
        // A big crowd arriving at once climbs one level per change.
        assert_eq!(Level::next(Some(Quiet), 1000), Some(Small));
        assert_eq!(Level::next(Some(Small), 1000), Some(Medium));
        assert_eq!(Level::next(Some(Medium), 1000), Some(Large));
        assert_eq!(Level::next(Some(Large), 1000), None);
        // Down: large below 150, medium below 75, quiet from any level at 14 or fewer.
        assert_eq!(Level::next(Some(Large), 150), None);
        assert_eq!(Level::next(Some(Large), 149), Some(Medium));
        assert_eq!(Level::next(Some(Medium), 75), None);
        assert_eq!(Level::next(Some(Medium), 74), Some(Small));
        assert_eq!(Level::next(Some(Small), 15), None);
        for from in [Small, Medium, Large] {
            assert_eq!(Level::next(Some(from), 14), Some(Quiet), "{from:?}");
            assert_eq!(Level::next(Some(from), i32::MIN), Some(Quiet), "{from:?}");
        }
        assert_eq!(Level::next(Some(Quiet), i32::MAX), Some(Small));
    }

    #[test]
    fn the_world_loop_plays_from_the_first_step_with_its_base_term() {
        let mut l = loops();
        let log = run(&mut l, &[(0, 0)]);
        assert_eq!(log[0], "soundscape loop world w.wav");
        assert_eq!(terms(&l)[0], ("w.wav".to_string(), 2000));
        // Once: later steps neither log it again nor change it.
        assert_eq!(run(&mut l, &[(66, 0)]), Vec::<String>::new());
        // No world file, no loop; before any step there is none either.
        let mut none = Loops::new(Files::parse(CROWD.as_bytes(), b""));
        assert!(none.playing().is_empty());
        run(&mut none, &[(1, 0)]);
        assert_eq!(none.playing().len(), 1);
    }

    #[test]
    fn the_crowd_loop_starts_with_the_whole_fade_term_and_fades_in_over_ten_seconds() {
        let mut l = loops();
        let log = run(&mut l, &[(66, 0)]);
        assert_eq!(log[1], "soundscape loop crowd quiet q.wav");
        assert_eq!(terms(&l)[1], ("q.wav".to_string(), 1500 + 4500));
        // Half way: the proportion is truncated.
        run(&mut l, &[(5000, 0)]);
        assert_eq!(terms(&l)[1], ("q.wav".to_string(), 1500 + 2250));
        run(&mut l, &[(3, 0)]);
        assert_eq!(terms(&l)[1].1, 1500 + 4500 - 2251);
        // The end of it, however long the step: no fade left.
        run(&mut l, &[(u32::MAX, 0)]);
        assert_eq!(terms(&l)[1], ("q.wav".to_string(), 1500));
        // A step of 0 ms and one of the exact length.
        let mut l = loops();
        run(&mut l, &[(0, 0), (0, 0)]);
        assert_eq!(terms(&l)[1].1, 6000);
        run(&mut l, &[(10_000, 0)]);
        assert_eq!(terms(&l)[1].1, 1500);
    }

    #[test]
    fn a_change_cross_fades_in_the_other_slot_and_no_change_starts_meanwhile() {
        let mut l = loops();
        run(&mut l, &[(0, 0), (10_000, 0)]);
        // 21 guests: small starts at 4,500 + 100, quiet goes up from 1,500.
        let log = run(&mut l, &[(0, 21)]);
        assert_eq!(log, ["soundscape loop crowd small s.wav"]);
        assert_eq!(
            terms(&l),
            [
                ("w.wav".into(), 2000),
                ("q.wav".into(), 1500),
                ("s.wav".into(), 4600)
            ]
        );
        run(&mut l, &[(4000, 21)]);
        // Quiet: 4,500 * 0.4 = 1,800 more; small: 2,700 less.
        assert_eq!(
            terms(&l)[1..],
            [("q.wav".into(), 3300), ("s.wav".into(), 1900)]
        );
        // 200 guests wanting medium and large change nothing until the fade is over.
        assert!(run(&mut l, &[(1000, 200), (1000, 200), (3999, 200)]).is_empty());
        let log = run(&mut l, &[(1, 200)]);
        assert!(log.is_empty());
        assert_eq!(terms(&l)[1..], [("s.wav".into(), 100)]);
        // The next update moves on one level; the voices of the two starts differ.
        let voice = |l: &Loops| {
            l.playing()
                .iter()
                .map(|p| p.voice.clone())
                .collect::<Vec<_>>()
        };
        let before = voice(&l);
        let log = run(&mut l, &[(0, 200)]);
        assert_eq!(log, ["soundscape loop crowd medium m.wav"]);
        let after = voice(&l);
        assert!(
            !before.contains(&after[2]) && after[1] == before[1],
            "{before:?} {after:?}"
        );
    }

    #[test]
    fn a_level_without_a_file_is_silent_and_fades_the_old_loop_out() {
        let mut l = Loops::new(Files::parse(b"[crowdsound]\nquiet=q.wav\n", b""));
        run(&mut l, &[(0, 0), (10_000, 0), (0, 50)]);
        assert_eq!(terms(&l), [("q.wav".into(), 1500)]);
        run(&mut l, &[(5000, 50)]);
        assert_eq!(terms(&l), [("q.wav".into(), 1500 + 2250)]);
        run(&mut l, &[(5000, 50)]);
        assert!(terms(&l).is_empty());
        // A crowd with no file at all never logs or plays, whatever the steps.
        let mut l = Loops::new(Files::default());
        assert!(run(&mut l, &[(0, 0), (u32::MAX, i32::MAX), (7, i32::MIN)]).is_empty());
        assert!(l.playing().is_empty());
    }
}

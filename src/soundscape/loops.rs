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
/// The crowd files before the crowd file is read (sound 4.4).
const CROWD_FILES: [&str; 4] = [
    "sounds/quiet.wav",
    "sounds/crowds.wav",
    "sounds/crowdm.wav",
    "sounds/crowdl.wav",
];

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

/// A file and its base term; `None` when it does not load.
type Tune = Option<(String, i32)>;

/// The files of the two loops: one world file, four crowd files by level.
#[derive(Debug, Default)]
pub(super) struct Files {
    world: Tune,
    crowd: [Tune; 4],
}

impl Files {
    /// Reads `[crowdsound]` of the crowd file and `[worldsound]` of the world file (sound 4.4). A
    /// crowd level has its default file at 1,500 until its name key replaces the file, and only
    /// then is its `...atten` key read; the world loop needs its name key (`atten` 0 when missing).
    /// A number that is not one reads 0. A file that cannot be read has the defaults.
    pub(super) fn parse(crowd: &[u8], world: &[u8]) -> Files {
        let crowd = Config::parse(crowd).unwrap_or_default();
        let world = Config::parse(world).unwrap_or_default();
        let number = |v: &[u8]| to_int(v).unwrap_or(0);
        let world = world.section("worldsound").and_then(|s| {
            let atten = s.get("atten").map_or(0, number);
            Some((name_from_bytes(s.get("name")?), atten))
        });
        let crowd = std::array::from_fn(|i| {
            let section = crowd.section("crowdsound");
            let named = section.and_then(|s| Some((s, name_from_bytes(s.get(Level::NAMES[i])?))));
            Some(match named {
                Some((s, file)) => (file, s.get(Level::ATTEN[i]).map_or(CROWD_ATTEN, number)),
                None => (CROWD_FILES[i].to_string(), CROWD_ATTEN),
            })
        });
        Files { world, crowd }
    }

    /// Drops what does not load: an empty name, or a file `loads` refuses.
    pub(super) fn loaded(mut self, loads: &dyn Fn(&str) -> bool) -> Files {
        let keep = |t: Tune| t.filter(|(file, _)| !file.is_empty() && loads(file));
        self.crowd = self.crowd.map(keep);
        self.world = keep(self.world);
        self
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
    world: Option<Playing>,
    /// The crowd's level, and the two slots of its loops.
    level: Option<Level>,
    crowd: [Option<Playing>; 2],
    /// The slot the next loop starts in.
    toggle: usize,
    /// The cross-fade under way: milliseconds from 0 to 10,000, and whether it counts up (the new
    /// loop is in the second slot) or down.
    fading: Option<(u32, bool)>,
    /// The voices made so far, so that every start has its own.
    serial: u32,
    /// Log lines of what was made before the first step.
    pending: Vec<String>,
}

impl Loops {
    /// The loops of a game just started: the world loop plays from now (sound 8.1).
    pub(super) fn new(files: Files) -> Loops {
        let world = files.world.clone().map(|(file, base)| Playing {
            voice: "world".to_string(),
            file,
            base,
            fade: 0,
        });
        Loops {
            pending: (world.iter())
                .map(|w| format!("soundscape loop world {}", w.file))
                .collect(),
            files,
            world,
            ..Loops::default()
        }
    }

    /// One simulation step of `step` ms with `guests` guests: a cross-fade under way advances, then,
    /// when none is, the crowd moves to another level if the guests ask. What happened goes to `log`.
    pub(super) fn update(&mut self, step: u32, guests: i32, log: &mut Vec<String>) {
        log.append(&mut self.pending);
        if let Some((ms, up)) = self.fading {
            self.cross_fade(ms, up, step.max(1));
        }
        if self.fading.is_none()
            && let Some(level) = Level::next(self.level, guests)
        {
            self.change(level, log);
        }
    }

    /// The crowd moves to `level`. Its loop, if its file loads, starts in the next slot with the
    /// whole fade term and a cross-fade begins; if not, nothing starts and the old loop plays on.
    fn change(&mut self, level: Level, log: &mut Vec<String>) {
        self.level = Some(level);
        let i = level as usize;
        let Some((file, base)) = self.files.crowd[i].clone() else {
            return;
        };
        log.push(format!("soundscape loop crowd {} {file}", Level::NAMES[i]));
        self.serial = self.serial.wrapping_add(1);
        let slot = self.toggle;
        self.crowd[slot] = Some(Playing {
            voice: format!("crowd {}", self.serial),
            file,
            base,
            fade: FADE_FROM,
        });
        self.toggle = 1 - slot;
        // The first slot counts down from the end, the second up from 0.
        self.fading = Some(if slot == 0 {
            (FADE_MS, false)
        } else {
            (0, true)
        });
    }

    /// The cross-fade is at `ms` and moves by `step`: each slot that plays takes its fade term, the
    /// first `trunc(p * 4500)`, the second `trunc((1 - p) * 4500)`, with `p = ms * 0.0001` in
    /// single precision. At the end the loop of the slot that fades out is stopped.
    fn cross_fade(&mut self, ms: u32, up: bool, step: u32) {
        let ms = if up {
            ms.saturating_add(step).min(FADE_MS)
        } else {
            ms.saturating_sub(step)
        };
        let p = ms as f32 * 0.0001f32;
        let terms = [
            (p * FADE_FROM as f32) as i32,
            ((1.0 - p) * FADE_FROM as f32) as i32,
        ];
        for (slot, term) in self.crowd.iter_mut().zip(terms) {
            if let Some(playing) = slot {
                playing.fade = term;
            }
        }
        self.fading = (ms != if up { FADE_MS } else { 0 }).then_some((ms, up));
        if self.fading.is_none() {
            self.crowd[usize::from(!up)] = None;
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

    fn loops_of(crowd: &str, world: &str) -> Loops {
        Loops::new(Files::parse(crowd.as_bytes(), world.as_bytes()).loaded(&|_| true))
    }

    fn loops() -> Loops {
        loops_of(CROWD, WORLD)
    }

    /// The log after one step per `(ms, guests)`.
    fn run(l: &mut Loops, steps: &[(u32, i32)]) -> Vec<String> {
        let mut log = Vec::new();
        steps.iter().for_each(|&(ms, n)| l.update(ms, n, &mut log));
        log
    }

    fn terms(l: &Loops) -> Vec<(String, i32)> {
        l.playing().into_iter().map(|p| (p.file, p.terms)).collect()
    }

    /// The terms of the crowd loops only, by file.
    fn crowd(l: &Loops) -> Vec<(String, i32)> {
        (l.playing().into_iter())
            .filter(|p| p.voice != "world")
            .map(|p| (p.file, p.terms))
            .collect()
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
        let atten: Vec<_> = f.crowd.iter().map(|t| t.clone().unwrap().1).collect();
        // The quiet one has no atten key: 1,500; the medium one is read from `medatten`; text is 0.
        assert_eq!(atten, [1500, 100, -50, 0]);
        // `mediumatten`, as the shipped file spells it, is not read.
        let shipped = CROWD.replace("medatten", "mediumatten");
        let f = Files::parse(shipped.as_bytes(), b"");
        assert_eq!(f.crowd[2], Some(("m.wav".to_string(), 1500)));
        // An atten key without its name key is not read; a huge value is the integer it makes.
        let f = Files::parse(
            b"[crowdsound]\nquietatten=5\n",
            b"[worldsound]\nname=a\natten=99999999999\n",
        );
        assert_eq!(f.crowd[0], Some(("sounds/quiet.wav".to_string(), 1500)));
        assert_eq!(f.world.unwrap().1, to_int(b"99999999999").unwrap());
        // No world name key, no section, no file or text that is not a file: no world loop.
        for bad in [
            "",
            "[worldsound]\natten=5\n",
            "[x]\nname=a.wav\n",
            "\u{0}\u{1}",
        ] {
            assert!(Files::parse(b"", bad.as_bytes()).world.is_none(), "{bad:?}");
        }
    }

    #[test]
    fn a_crowd_level_without_a_name_key_plays_its_default_file() {
        // No file, no section, or a level without its key: the default file at 1,500.
        let default = [
            "sounds/quiet.wav",
            "sounds/crowds.wav",
            "sounds/crowdm.wav",
            "sounds/crowdl.wav",
        ];
        for text in ["", "[x]\n", "[crowdsound]\nsmall=s.wav\n"] {
            let f = Files::parse(text.as_bytes(), b"").loaded(&|_| true);
            let want = |i: usize| Some((default[i].to_string(), 1500));
            assert_eq!(f.crowd[0], want(0), "{text:?}");
            assert_eq!(f.crowd[2], want(2), "{text:?}");
            assert_eq!(f.crowd[3], want(3), "{text:?}");
        }
        let mut l = loops_of("", "");
        run(&mut l, &[(0, 0)]);
        assert_eq!(crowd(&l), [("sounds/quiet.wav".to_string(), 6000)]);
        // A file that is not there plays nothing.
        let none = Files::parse(b"", b"").loaded(&|f| f != "sounds/quiet.wav");
        assert!(none.crowd[0].is_none() && none.crowd[1].is_some());
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
    fn the_world_loop_is_there_from_the_creation_and_logged_at_the_first_step() {
        let mut l = loops();
        assert_eq!(terms(&l), [("w.wav".to_string(), 2000)]);
        let log = run(&mut l, &[(0, 0)]);
        assert_eq!(log[0], "soundscape loop world w.wav");
        // Once: later steps neither log it again nor change it.
        assert_eq!(run(&mut l, &[(66, 0)]), Vec::<String>::new());
        assert_eq!(terms(&l)[0], ("w.wav".to_string(), 2000));
        // No world file, no loop.
        assert!(loops_of(CROWD, "").playing().is_empty());
        assert!(
            Loops::new(Files::parse(b"", WORLD.as_bytes()).loaded(&|_| false))
                .playing()
                .is_empty()
        );
    }

    #[test]
    fn the_crowd_loop_starts_with_the_whole_fade_term_and_fades_in_over_ten_seconds() {
        let mut l = loops();
        let log = run(&mut l, &[(66, 0)]);
        assert_eq!(log[1], "soundscape loop crowd quiet q.wav");
        // The change sets no fade: the loop starts at 4,500 with its base term 1,500.
        assert_eq!(crowd(&l), [("q.wav".to_string(), 6000)]);
        run(&mut l, &[(5000, 0)]);
        assert_eq!(crowd(&l), [("q.wav".to_string(), 1500 + 2250)]);
        // The end of it, however long the step: no fade left.
        run(&mut l, &[(u32::MAX, 0)]);
        assert_eq!(crowd(&l), [("q.wav".to_string(), 1500)]);
        // The exact length.
        let mut l = loops();
        run(&mut l, &[(0, 0), (10_000, 0)]);
        assert_eq!(crowd(&l)[0].1, 1500);
    }

    #[test]
    fn the_cross_fade_terms_follow_the_single_precision_chain_of_each_slot() {
        let mut l = loops();
        run(&mut l, &[(0, 0)]);
        // The first change is in the first slot and counts down from 10,000: 3 ms in, p = 0.9997.
        run(&mut l, &[(3, 0)]);
        assert_eq!(crowd(&l), [("q.wav".to_string(), 1500 + 4498)]);
        run(&mut l, &[(9997, 0)]);
        // The second change is in the second slot and counts up from 0.
        run(&mut l, &[(0, 21)]);
        assert_eq!(
            crowd(&l),
            [
                ("q.wav".to_string(), 1500),
                ("s.wav".to_string(), 100 + 4500)
            ]
        );
        run(&mut l, &[(3, 21)]);
        // Not 4,500 less the old one: the new loop is at 4,498 and the old at 1.
        assert_eq!(
            crowd(&l),
            [
                ("q.wav".to_string(), 1501),
                ("s.wav".to_string(), 100 + 4498)
            ]
        );
        run(&mut l, &[(4997, 21)]);
        assert_eq!(
            crowd(&l),
            [
                ("q.wav".to_string(), 1500 + 2250),
                ("s.wav".to_string(), 100 + 2250)
            ]
        );
        run(&mut l, &[(5000, 21)]);
        assert_eq!(crowd(&l), [("s.wav".to_string(), 100)]);
    }

    #[test]
    fn a_step_of_0_ms_advances_a_cross_fade_by_1_ms() {
        let mut l = loops();
        run(&mut l, &[(0, 0), (0, 0)]);
        assert_eq!(crowd(&l), [("q.wav".to_string(), 1500 + 4499)]);
        // 10,000 steps of nothing end it; it never sticks.
        let mut l = loops();
        run(&mut l, &[(0, 0)]);
        run(&mut l, &vec![(0, 0); 10_000]);
        assert_eq!(crowd(&l), [("q.wav".to_string(), 1500)]);
    }

    #[test]
    fn a_change_can_start_in_the_step_that_ends_a_cross_fade() {
        let mut l = loops();
        run(&mut l, &[(0, 0)]);
        // The step that ends the first fade starts the small loop in the second slot.
        let log = run(&mut l, &[(10_000, 21)]);
        assert_eq!(log, ["soundscape loop crowd small s.wav"]);
        assert_eq!(
            crowd(&l),
            [("q.wav".to_string(), 1500), ("s.wav".to_string(), 4600)]
        );
        // No change starts meanwhile: 200 guests wait for the end of the fade.
        assert!(run(&mut l, &[(1000, 200), (8999, 200)]).is_empty());
        let log = run(&mut l, &[(1, 200)]);
        assert_eq!(log, ["soundscape loop crowd medium m.wav"]);
        // The voices are new for every start, the old ones go on.
        let voices: Vec<_> = l.playing().into_iter().map(|p| p.voice).collect();
        assert_eq!(voices.len(), 3, "{voices:?}");
        assert_eq!(
            voices
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn a_crowd_file_that_does_not_load_starts_no_cross_fade() {
        // The small and the medium files are not there; the quiet one is.
        let files = Files::parse(CROWD.as_bytes(), b"").loaded(&|f| f == "q.wav" || f == "l.wav");
        let mut l = Loops::new(files);
        run(&mut l, &[(0, 0), (10_000, 0)]);
        // Small does not load: the quiet loop plays on, at its fade, with no cross-fade.
        let log = run(&mut l, &[(0, 21)]);
        assert!(log.is_empty());
        assert_eq!(crowd(&l), [("q.wav".to_string(), 1500)]);
        assert!(l.fading.is_none());
        run(&mut l, &[(5000, 21)]);
        assert_eq!(crowd(&l), [("q.wav".to_string(), 1500)]);
        // The level did change: the next one up is medium from small (86), not small from quiet.
        assert_eq!(l.level, Some(Level::Small));
        run(&mut l, &[(0, 86)]);
        assert_eq!(l.level, Some(Level::Medium));
        // A level that loads later takes the slot the failed ones left (`largeatten` is text: 0).
        run(&mut l, &[(0, 161)]);
        assert_eq!(crowd(&l)[1], ("l.wav".to_string(), 4500));
        assert_eq!(l.crowd[1].as_ref().map(|p| p.file.as_str()), Some("l.wav"));
        // A crowd with no file at all never logs or plays, whatever the steps.
        let mut l = Loops::new(Files::default());
        assert!(run(&mut l, &[(0, 0), (u32::MAX, i32::MAX), (7, i32::MIN)]).is_empty());
        assert!(l.playing().is_empty());
    }
}

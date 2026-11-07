//! The ambient one-shots of a game (sound 8.1): every simulation step draws a random point near
//! the centre of the view for each of the two groups, the crowd's voices and the world's birds, and
//! once per second of steps each group rolls whether it plays one sound there. The world loop and
//! the crowd loop are in [`loops`].

mod loops;

use std::collections::BTreeMap;

use crate::config::{Config, to_int};
use crate::sim::Rng;
use crate::vfs::{Vfs, name_from_bytes};
use loops::{Files, Loops};

pub use loops::SceneLoop;

/// The file of each group when the scenario names none (sound 8.1).
const CROWD_FILE: &str = "crowdsnd.cfg";
const WORLD_FILE: &str = "worldsnd.cfg";
/// The files of the loops: the crowd's, and the world's when the scenario names none (sound 4.4).
const CROWD_LOOPS: &str = "crowd.cfg";
const WORLD_LOOP: &str = "worldlp.cfg";
/// The level the world group plays at (sound 8.1).
const WORLD_LEVEL: i32 = 50;
/// The group accumulates this many milliseconds of steps per roll.
const SECOND: u32 = 1000;

/// One group of an ambient-levels file: the chance and the sounds with their weights.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Group {
    chance: u32,
    sounds: Vec<(String, i32)>,
}

impl Group {
    /// The list of files, which names the sound group the entries belong to.
    fn files(&self) -> Vec<String> {
        self.sounds.iter().map(|(f, _)| f.clone()).collect()
    }
}

/// An `[ambientlevels]` file: the groups with the range of levels each one serves.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Ambients(Vec<(i32, i32, Group)>);

impl Ambients {
    /// Reads a file. A group without two values, or without a section, serves no level; a
    /// number that is not one reads 0 and the sounds take the weights in file order, 0 when there
    /// are fewer weights.
    pub(crate) fn parse(bytes: &[u8]) -> Ambients {
        let Ok(file) = Config::parse(bytes) else {
            return Ambients::default();
        };
        let Some(levels) = file.section("ambientlevels") else {
            return Ambients::default();
        };
        let number = |v: &[u8]| to_int(v).unwrap_or(0);
        let values = levels.get_all("value");
        let mut groups = Vec::new();
        for (i, name) in levels.get_all("group").iter().enumerate() {
            let (Some(lo), Some(hi)) = (values.get(2 * i), values.get(2 * i + 1)) else {
                continue;
            };
            let (lo, hi) = (number(lo), number(hi));
            let group = file.section(name).map_or_else(Group::default, |s| {
                let probs = s.get_all("prob");
                Group {
                    // Without the key the slot keeps the group's upper level (sound 8.1 point 8).
                    chance: s.get("chance").map_or(hi, number) as u32,
                    sounds: (s.get_all("sound").iter().enumerate())
                        .map(|(i, f)| (name_from_bytes(f), probs.get(i).map_or(0, |p| number(p))))
                        .collect(),
                }
            });
            groups.push((lo, hi, group));
        }
        Ambients(groups)
    }

    /// Gives every group the chance and the weights of the first group made for the same list of
    /// files, `made` holding those first ones (sound 8.1 point 4).
    fn shared(&mut self, made: &mut BTreeMap<Vec<String>, Group>) {
        for (_, _, group) in &mut self.0 {
            *group = made
                .entry(group.files())
                .or_insert_with(|| group.clone())
                .clone();
        }
    }

    /// The first group whose range holds `level`, both ends included.
    fn group(&self, level: i32) -> Option<&Group> {
        let found = self
            .0
            .iter()
            .find(|(lo, hi, _)| (*lo..=*hi).contains(&level));
        found.map(|(_, _, g)| g)
    }
}

/// A one-shot to play: the member, and the place as an offset in view pixels from the centre of
/// the map view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shot {
    pub file: String,
    /// The sound group's list of files and the entry: each entry of a group is its own voice
    /// (sound 8.1 point 9).
    pub voice: String,
    pub dx: i32,
    pub dy: i32,
}

/// The crowd group's level for `guests` guests: -100 below 10, 10 below 50, 40 below 100, else 90.
fn crowd_level(guests: i32) -> i32 {
    match guests {
        ..10 => -100,
        10..50 => 10,
        50..100 => 40,
        _ => 90,
    }
}

/// The soundscape of the game: the two groups' files, what each has accumulated, and the
/// one-shots a step asked for.
#[derive(Debug, Default)]
pub(crate) struct Soundscape {
    crowd: Ambients,
    world: Ambients,
    /// Milliseconds accumulated by the sound group, which is shared by every group that lists the same
    /// files (sound 8.1 point 4).
    clocks: BTreeMap<Vec<String>, u32>,
    shots: Vec<Shot>,
    loops: Loops,
}

impl Soundscape {
    /// The soundscape of two files: the crowd's groups are made first, then the world's, each in
    /// file order, and a group listing files that an earlier one listed takes its chance and weights.
    fn new(mut crowd: Ambients, mut world: Ambients) -> Soundscape {
        let mut made = BTreeMap::new();
        crowd.shared(&mut made);
        world.shared(&mut made);
        Soundscape {
            crowd,
            world,
            ..Soundscape::default()
        }
    }

    /// The soundscape of a game whose scenario file is `scenario` (a saved game without one, or a
    /// bare map, has none): the files `[start]` names, else the default ones, read through `read`.
    pub(crate) fn from_files(
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
        scenario: Option<&str>,
    ) -> Soundscape {
        let start = scenario.and_then(read).and_then(|b| Config::parse(&b).ok());
        let named = |key: &str, default: &str| {
            let name = start.as_ref().and_then(|c| c.section("start")?.get(key));
            name.map_or_else(|| default.to_string(), name_from_bytes)
        };
        let ambients = |name: String| Ambients::parse(&read(&name).unwrap_or_default());
        let file = |name: String| read(&name).unwrap_or_default();
        let mut scape = Soundscape::new(
            ambients(named("crowdAmbients", CROWD_FILE)),
            ambients(named("worldAmbients", WORLD_FILE)),
        );
        let (crowd, world) = (
            file(CROWD_LOOPS.to_string()),
            file(named("worldConfig", WORLD_LOOP)),
        );
        scape.loops = Loops::new(Files::parse(&crowd, &world));
        scape
    }

    /// [`Soundscape::from_files`] on the files of `vfs`; a file that cannot be read is a warning
    /// and a group without sounds.
    pub(crate) fn load(vfs: &Vfs, scenario: Option<&str>) -> Soundscape {
        let read = |name: &str| match vfs.read(name) {
            Ok(bytes) => Some(bytes),
            Err(why) => {
                eprintln!("rustyzoo: warning: {name}: {why}; no sounds from it");
                None
            }
        };
        Soundscape::from_files(&read, scenario)
    }

    /// One simulation step of `step` ms with `guests` guests: the crowd's point then the world's
    /// (four draws each, the y draws first), then the crowd's play and the world's (sound 8.1).
    pub(crate) fn update(&mut self, step: u32, guests: i32, rng: &mut Rng, log: &mut Vec<String>) {
        let mut point = || {
            let (y1, y2) = (rng.rand() % 300, rng.rand() % 300);
            let (x1, x2) = (rng.rand() % 400, rng.rand() % 400);
            ((x1 as i32 - x2 as i32), (y1 as i32 - y2 as i32))
        };
        let points = [point(), point()];
        log.push(format!(
            "soundscape points crowd {},{} world {},{}",
            points[0].0, points[0].1, points[1].0, points[1].1
        ));
        let Soundscape {
            crowd,
            world,
            clocks,
            shots,
            loops,
        } = self;
        loops.update(step, guests, log);
        let plays = [
            ("crowd", crowd.group(crowd_level(guests)), points[0]),
            ("world", world.group(WORLD_LEVEL), points[1]),
        ];
        for (name, group, (dx, dy)) in plays {
            let Some(group) = group else { continue };
            let files = group.files();
            let clock = clocks.entry(files).or_insert(0);
            *clock = clock.wrapping_add(step);
            if *clock <= SECOND {
                continue;
            }
            *clock = (*clock - 1) % SECOND + 1;
            if group.chance == 0 || rng.pick(group.chance) != 0 {
                log.push(format!("soundscape {name} roll no"));
                continue;
            }
            let draw = (rng.rand() % 100) as i32;
            let mut sum = 0i32;
            let found = group.sounds.iter().enumerate().find(|(_, (_, prob))| {
                sum = sum.wrapping_add(*prob);
                draw < sum
            });
            match found {
                Some((entry, (file, _))) => {
                    log.push(format!("soundscape {name} roll {draw} {file}"));
                    shots.push(Shot {
                        file: file.clone(),
                        voice: format!("ambient {entry} {}", group.files().join(" ")),
                        dx,
                        dy,
                    });
                }
                None => log.push(format!("soundscape {name} roll {draw} none")),
            }
        }
    }

    /// The loops playing now (sound 8.1).
    pub(crate) fn loops(&self) -> Vec<SceneLoop> {
        self.loops.playing()
    }

    /// The one-shots the steps since the last call asked for.
    pub(crate) fn take_shots(&mut self) -> Vec<Shot> {
        std::mem::take(&mut self.shots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORLD: &str = "[ambientlevels]\ngroup = happy\nvalue = -100\nvalue = 100\n\n[happy]\nchance = 100\n\
        sound = a.wav\nprob = 40\nsound = b.wav\nprob = 40\nsound = c.wav\nprob = 20\n";
    const CROWD: &str = "[ambientlevels]\ngroup = none\nvalue = -100\nvalue = 0\ngroup = small\nvalue = 1\nvalue = 20\n\
        group = big\nvalue = 21\nvalue = 100\n\n[none]\nchance = 0\n\n[small]\nchance = 1\nsound = v.wav\nprob = 100\n\n\
        [big]\nchance = 1\nsound = v.wav\nprob = 100\n";

    fn scape(crowd: &str, world: &str) -> Soundscape {
        Soundscape::new(
            Ambients::parse(crowd.as_bytes()),
            Ambients::parse(world.as_bytes()),
        )
    }

    /// The generator after `n` draws from the seed.
    fn after(seed: u32, n: usize) -> Rng {
        let mut rng = Rng::default();
        rng.seed(seed);
        (0..n).for_each(|_| _ = rng.rand());
        rng
    }

    fn seeded(seed: u32) -> Rng {
        after(seed, 0)
    }

    #[test]
    fn a_step_draws_eight_numbers_for_the_two_points_and_none_for_a_group_without_a_level() {
        let mut s = scape("", "");
        let (mut rng, mut log) = (seeded(7), Vec::new());
        s.update(67, 0, &mut rng, &mut log);
        assert_eq!(rng.state(), after(7, 8).state());
        // y from the first two draws of a point, x from the last two; the crowd's point first.
        let mut r = seeded(7);
        let mut d = || r.rand() as i32;
        let (y1, y2, x1, x2) = (d() % 300, d() % 300, d() % 400, d() % 400);
        let crowd = format!("{},{}", x1 - x2, y1 - y2);
        assert!(
            log[0].starts_with(&format!("soundscape points crowd {crowd} world ")),
            "{log:?}"
        );
        assert!(s.take_shots().is_empty());
    }

    #[test]
    fn a_group_rolls_once_when_its_sum_passes_a_second_and_keeps_the_rest() {
        let mut s = scape("", WORLD);
        let (mut rng, mut log) = (seeded(1), Vec::new());
        let clock = |s: &Soundscape| s.clocks.values().copied().collect::<Vec<_>>();
        // 1000 ms is not above a second: no roll, the 8 draws only.
        s.update(1000, 0, &mut rng, &mut log);
        assert_eq!((rng.state(), clock(&s)), (after(1, 8).state(), vec![1000]));
        // 5400 more: one roll (chance 100: one draw, a second when it hits), the sum stays 1 to 1000.
        let before = rng.state();
        s.update(5400, 0, &mut rng, &mut log);
        assert_eq!(clock(&s), vec![400]);
        assert!(log.iter().any(|l| l.contains("world roll")), "{log:?}");
        assert_ne!(rng.state(), before);
        // A step of 0 never rolls; the clock wraps instead of failing.
        let mut s = scape("", WORLD);
        s.update(0, 0, &mut rng, &mut log);
        assert_eq!(s.clocks.values().copied().collect::<Vec<_>>(), vec![0]);
        s.update(u32::MAX, 0, &mut rng, &mut log);
        s.update(u32::MAX, 0, &mut rng, &mut log);
        assert!(s.clocks.values().all(|&c| (1..=1000).contains(&c)));
    }

    #[test]
    fn the_roll_hits_one_time_in_chance_and_the_second_draw_picks_by_cumulative_weight() {
        // The draws a roll takes from seed 1 after the eight of the points.
        let mut probe = after(1, 8);
        let first = probe.rand();
        let second = probe.rand() % 100;
        let (mut rng, mut log) = (seeded(1), Vec::new());
        let mut s = scape("", WORLD);
        s.update(1001, 0, &mut rng, &mut log);
        let line = log.last().unwrap();
        if first.is_multiple_of(100) {
            let want = match second {
                0..40 => "a.wav",
                40..80 => "b.wav",
                _ => "c.wav",
            };
            assert_eq!(line, &format!("soundscape world roll {second} {want}"));
            assert_eq!(rng.state(), after(1, 10).state());
            assert_eq!(s.take_shots()[0].file, want);
        } else {
            assert_eq!(line, "soundscape world roll no");
            assert_eq!(rng.state(), after(1, 9).state());
            assert!(s.take_shots().is_empty());
        }
    }

    #[test]
    fn a_hit_picks_the_first_sound_whose_running_sum_passes_the_draw() {
        // Chance 1 always hits; weights 10 and 5: a draw of 15 or more plays nothing.
        let world = "[ambientlevels]\ngroup = g\nvalue = 0\nvalue = 100\n[g]\nchance = 1\nsound = a.wav\nprob = 10\nsound = b.wav\nprob = 5\n";
        let mut seen = std::collections::BTreeSet::new();
        for seed in 1..40 {
            let (mut rng, mut log) = (seeded(seed), Vec::new());
            let mut s = scape("", world);
            s.update(1001, 0, &mut rng, &mut log);
            let line = log.last().unwrap().clone();
            let r = {
                let mut p = after(seed, 8);
                p.rand();
                p.rand() % 100
            };
            let want = match r {
                0..10 => format!("soundscape world roll {r} a.wav"),
                10..15 => format!("soundscape world roll {r} b.wav"),
                _ => format!("soundscape world roll {r} none"),
            };
            assert_eq!(line, want);
            seen.insert(want.rsplit(' ').next().unwrap().to_string());
        }
        assert_eq!(seen.len(), 3, "{seen:?}");
    }

    #[test]
    fn chance_zero_and_a_missing_group_draw_nothing_and_accumulate_nothing() {
        // No guests: the crowd level -100 is the group with chance 0, which rolls and draws no more.
        let mut s = scape(CROWD, "");
        let (mut rng, mut log) = (seeded(3), Vec::new());
        s.update(5000, 0, &mut rng, &mut log);
        assert_eq!(rng.state(), after(3, 8).state());
        assert!(log.iter().any(|l| l == "soundscape crowd roll no"));
        // A level no group serves: no accumulation.
        let mut s = scape(&CROWD.replace("value = 100", "value = 80"), "");
        s.update(5000, 100, &mut rng, &mut log);
        assert!(s.clocks.is_empty());
    }

    #[test]
    fn the_crowd_level_follows_the_guest_count() {
        let levels: Vec<i32> = [-5, 0, 9, 10, 49, 50, 99, 100, i32::MAX]
            .map(crowd_level)
            .to_vec();
        assert_eq!(levels, [-100, -100, -100, 10, 10, 40, 40, 90, 90]);
    }

    #[test]
    fn groups_that_list_the_same_files_share_one_clock() {
        let mut s = scape(CROWD, "");
        let (mut rng, mut log) = (seeded(1), Vec::new());
        // 20 guests: small; 60 guests: big. Both list v.wav, so the second step adds to the first's sum.
        s.update(600, 20, &mut rng, &mut log);
        s.update(600, 60, &mut rng, &mut log);
        assert_eq!(s.clocks[&vec!["v.wav".to_string()]], 200);
    }

    #[test]
    fn groups_that_list_the_same_files_share_the_chance_and_weights_of_the_first() {
        let crowd = "[ambientlevels]\ngroup = small\nvalue = 1\nvalue = 20\ngroup = medium\nvalue = 21\nvalue = 60\n\
            group = big\nvalue = 61\nvalue = 100\n[small]\nchance = 100\nsound = a.wav\nprob = 10\nsound = b.wav\nprob = 90\n\
            [medium]\nchance = 30\nsound = a.wav\nprob = 90\nsound = b.wav\nprob = 10\n\
            [big]\nchance = 15\nsound = a.wav\nprob = 50\nsound = b.wav\nprob = 50\n";
        let s = scape(crowd, "");
        for level in [10, 40, 90] {
            let g = s.crowd.group(level).unwrap();
            assert_eq!(g.chance, 100, "{level}");
            assert_eq!(
                g.sounds,
                [("a.wav".to_string(), 10), ("b.wav".to_string(), 90)]
            );
        }
        // 60 guests is level 40, the medium group: it rolls with small's chance and weights, on one clock.
        for seed in 1..30 {
            let (mut rng, mut log) = (seeded(seed), Vec::new());
            let mut s = scape(crowd, "");
            s.update(1001, 60, &mut rng, &mut log);
            let mut p = after(seed, 8);
            let hit = p.rand().is_multiple_of(100);
            let draw = p.rand() % 100;
            let want = match (hit, draw) {
                (false, _) => "soundscape crowd roll no".to_string(),
                (_, 0..10) => format!("soundscape crowd roll {draw} a.wav"),
                _ => format!("soundscape crowd roll {draw} b.wav"),
            };
            assert_eq!(log.last().unwrap(), &want, "seed {seed}");
        }
        // The first group made keeps its own when the world file lists the same files after the crowd's.
        let world = "[ambientlevels]\ngroup = w\nvalue = 0\nvalue = 100\n[w]\nchance = 7\nsound = a.wav\nprob = 1\n\
            sound = b.wav\nprob = 1\n";
        let s = scape(crowd, world);
        let w = s.world.group(50).unwrap();
        assert_eq!((w.chance, w.sounds[1].1), (100, 90));
        // A world file first listed in no crowd group keeps its own.
        let s = scape("", world);
        assert_eq!(s.world.group(50).unwrap().chance, 7);
    }

    #[test]
    fn a_group_without_a_chance_key_takes_its_upper_level_as_chance() {
        let file =
            "[ambientlevels]\ngroup = g\nvalue = 21\nvalue = 50\n[g]\nsound = a.wav\nprob = 100\n";
        assert_eq!(
            Ambients::parse(file.as_bytes()).group(30).unwrap().chance,
            50
        );
        // The key there wins, even 0; a negative upper level is the unsigned number it makes.
        let keyed = file.replace("[g]\n", "[g]\nchance = 0\n");
        assert_eq!(
            Ambients::parse(keyed.as_bytes()).group(30).unwrap().chance,
            0
        );
        let neg = file
            .replace("value = 50", "value = -1")
            .replace("value = 21", "value = -5");
        assert_eq!(
            Ambients::parse(neg.as_bytes()).group(-3).unwrap().chance,
            u32::MAX
        );
        // It rolls one in 50: the draw mod 50 decides.
        for seed in 1..40 {
            let (mut rng, mut log) = (seeded(seed), Vec::new());
            let mut s = scape("", &file.replace("value = 21", "value = 0"));
            s.update(1001, 0, &mut rng, &mut log);
            let hit = after(seed, 8).rand().is_multiple_of(50);
            assert_eq!(
                log.last().unwrap() != "soundscape world roll no",
                hit,
                "seed {seed}"
            );
        }
    }

    #[test]
    fn a_file_is_read_by_its_ranges_chances_and_weights_whatever_it_holds() {
        let a = Ambients::parse(CROWD.as_bytes());
        assert_eq!(a.group(-100).map(|g| g.chance), Some(0));
        assert_eq!(a.group(0).map(|g| g.sounds.len()), Some(0));
        assert_eq!(a.group(1).unwrap().sounds, [("v.wav".to_string(), 100)]);
        assert_eq!(a.group(20).unwrap().chance, 1);
        assert_eq!(a.group(21).unwrap().chance, 1);
        assert_eq!(a.group(101), None);
        assert_eq!(a.group(-101), None);
        // Missing: no file, no section, a group without its two values or its section, fewer weights
        // than sounds, a negative or huge chance and weight, text for a number.
        for bad in [
            "",
            "[ambientlevels]\n",
            "[x]\ngroup = a\n",
            "[ambientlevels]\ngroup = a\nvalue = 1\n",
        ] {
            assert_eq!(Ambients::parse(bad.as_bytes()).group(1), None, "{bad:?}");
        }
        let odd = "[ambientlevels]\ngroup = a\nvalue = -5\nvalue = x\ngroup = b\nvalue = 0\nvalue = 9\n\
            [b]\nchance = -1\nsound = f.wav\nsound = g.wav\nprob = 99999999999\n";
        let a = Ambients::parse(odd.as_bytes());
        let b = a.group(5).unwrap();
        assert_eq!(b.chance, u32::MAX);
        assert_eq!(b.sounds[1].1, 0);
        // A group with no section serves its range and plays nothing.
        assert_eq!(a.group(-5).unwrap().chance, 0);
    }

    #[test]
    fn the_scenario_names_the_files_and_the_default_ones_serve_otherwise() {
        let files = |name: &str| -> Option<Vec<u8>> {
            Some(match name {
                "s.scn" => b"[start]\nworldAmbients=w2.cfg\n".to_vec(),
                "w2.cfg" => WORLD.replace("a.wav", "z.wav").into_bytes(),
                "worldsnd.cfg" => WORLD.as_bytes().to_vec(),
                "crowdsnd.cfg" => CROWD.as_bytes().to_vec(),
                _ => return None,
            })
        };
        let key = |s: &Soundscape| s.world.group(50).unwrap().sounds[0].0.clone();
        assert_eq!(key(&Soundscape::from_files(&files, Some("s.scn"))), "z.wav");
        assert_eq!(key(&Soundscape::from_files(&files, None)), "a.wav");
        // A scenario that cannot be read, or has no `[start]`, leaves the defaults.
        assert_eq!(
            key(&Soundscape::from_files(&files, Some("missing.scn"))),
            "a.wav"
        );
        // The loops: the scenario's world file, else `worldlp.cfg`; `crowd.cfg` always.
        let loops = |scenario: Option<&str>| {
            let files = |name: &str| -> Option<Vec<u8>> {
                Some(match name {
                    "s.scn" => b"[start]\nworldConfig=w.cfg\n".to_vec(),
                    "w.cfg" => b"[worldsound]\nname=scn.wav\natten=7\n".to_vec(),
                    "worldlp.cfg" => b"[worldsound]\nname=def.wav\n".to_vec(),
                    "crowd.cfg" => b"[crowdsound]\nquiet=q.wav\n".to_vec(),
                    _ => return None,
                })
            };
            let mut s = Soundscape::from_files(&files, scenario);
            s.update(0, 0, &mut seeded(1), &mut Vec::new());
            s.loops()
                .into_iter()
                .map(|l| (l.file, l.terms))
                .collect::<Vec<_>>()
        };
        let crowd = ("q.wav".to_string(), 6000);
        assert_eq!(
            loops(Some("s.scn")),
            [("scn.wav".to_string(), 7), crowd.clone()]
        );
        assert_eq!(loops(None), [("def.wav".to_string(), 0), crowd]);
        assert!(Soundscape::from_files(&|_| None, None).loops().is_empty());
        // A named file that is missing is a group without sounds, not a failure.
        let none = Soundscape::from_files(&|_| None, Some("s.scn"));
        assert_eq!(none.world.group(50), None);
    }
}

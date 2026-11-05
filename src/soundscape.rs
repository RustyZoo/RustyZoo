//! The ambient one-shots of a game (sound 8.1): every simulation step draws a random point near
//! the centre of the view for each of the two groups, the crowd's voices and the world's birds, and
//! once per second of steps each group rolls whether it plays one sound there. The loops of the
//! soundscape are not made yet.

use std::collections::BTreeMap;

use crate::config::{Config, to_int};
use crate::sim::Rng;
use crate::vfs::{Vfs, name_from_bytes};

/// The file of each group when the scenario names none (sound 8.1).
const CROWD_FILE: &str = "crowdsnd.cfg";
const WORLD_FILE: &str = "worldsnd.cfg";
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
            let group = file.section(name).map_or_else(Group::default, |s| {
                let probs = s.get_all("prob");
                Group {
                    chance: s.get("chance").map_or(0, number) as u32,
                    sounds: (s.get_all("sound").iter().enumerate())
                        .map(|(i, f)| (name_from_bytes(f), probs.get(i).map_or(0, |p| number(p))))
                        .collect(),
                }
            });
            groups.push((number(lo), number(hi), group));
        }
        Ambients(groups)
    }

    /// The first group whose range holds `level`, both ends included.
    fn group(&self, level: i32) -> Option<&Group> {
        let found = self.0.iter().find(|(lo, hi, _)| (*lo..=*hi).contains(&level));
        found.map(|(_, _, g)| g)
    }
}

/// A one-shot to play: the member, and the place as an offset in view pixels from the centre of
/// the map view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shot {
    pub file: String,
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
}

impl Soundscape {
    /// The soundscape of a game whose scenario file is `scenario` (a saved game without one, or a
    /// bare map, has none): the files `[start]` names, else the default ones, read through `read`.
    pub(crate) fn from_files(read: &dyn Fn(&str) -> Option<Vec<u8>>, scenario: Option<&str>) -> Soundscape {
        let start = scenario.and_then(read).and_then(|b| Config::parse(&b).ok());
        let named = |key: &str, default: &str| {
            let name = start.as_ref().and_then(|c| c.section("start")?.get(key));
            name.map_or_else(|| default.to_string(), name_from_bytes)
        };
        let ambients = |name: String| Ambients::parse(&read(&name).unwrap_or_default());
        Soundscape {
            crowd: ambients(named("crowdAmbients", CROWD_FILE)),
            world: ambients(named("worldAmbients", WORLD_FILE)),
            ..Soundscape::default()
        }
    }

    /// [`Soundscape::from_files`] on the files of `vfs`; a file that cannot be read is a warning
    /// and a group without sounds.
    pub(crate) fn load(vfs: &Vfs, scenario: Option<&str>) -> Soundscape {
        let read = |name: &str| match vfs.read(name) {
            Ok(bytes) => Some(bytes),
            Err(why) => {
                eprintln!("rustyzoo: warning: {name}: {why}; no ambient sounds from it");
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
        } = self;
        let plays = [
            ("crowd", crowd.group(crowd_level(guests)), points[0]),
            ("world", world.group(WORLD_LEVEL), points[1]),
        ];
        for (name, group, (dx, dy)) in plays {
            let Some(group) = group else { continue };
            let files = group.sounds.iter().map(|(f, _)| f.clone()).collect();
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
            let found = group.sounds.iter().find(|(_, prob)| {
                sum = sum.wrapping_add(*prob);
                draw < sum
            });
            match found {
                Some((file, _)) => {
                    log.push(format!("soundscape {name} roll {draw} {file}"));
                    shots.push(Shot {
                        file: file.clone(),
                        dx,
                        dy,
                    });
                }
                None => log.push(format!("soundscape {name} roll {draw} none")),
            }
        }
    }

    /// The one-shots the steps since the last call asked for.
    pub(crate) fn take_shots(&mut self) -> Vec<Shot> {
        std::mem::take(&mut self.shots)
    }
}

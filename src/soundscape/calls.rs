//! The calls of the ambient creatures (ambients 6.4): each type names an `[ambientlevels]` file
//! (`cSoundName`) and every move of a creature plays its group at the level 50. The sound groups
//! are the manager's: one per list of file names, with its accumulator, chance and weights, kept
//! for the whole run of the program, so every creature of a type (and every type listing the same
//! files) rolls on the same clock and has one voice per entry.

use std::collections::BTreeMap;

use super::{Ambients, Group, Roll, Shot};
use crate::sim::Rng;

/// The level a creature's call plays at.
const LEVEL: i32 = 50;

/// The groups the creatures made, the clock of each, and the calls waiting to be placed.
#[derive(Debug, Default)]
pub(crate) struct Calls {
    /// The file of each name read once; a name that does not read has no group.
    files: BTreeMap<String, Ambients>,
    /// The first group made for each list of files (sound 8.1 point 4).
    made: BTreeMap<Vec<String>, Group>,
    clocks: BTreeMap<Vec<String>, u32>,
    shots: Vec<Shot>,
}

impl Calls {
    /// The creature whose type names the file `name` moved by `step` ms to the map position `at`:
    /// its group plays (ambients 6.4 points 2 and 3). An empty name has no group.
    pub(crate) fn play(
        &mut self,
        name: &str,
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
        at: (i32, i32),
        step: u32,
        rng: &mut Rng,
    ) {
        if name.is_empty() {
            return;
        }
        if !self.files.contains_key(name) {
            let mut file = Ambients::parse(&read(name).unwrap_or_default());
            file.shared(&mut self.made);
            self.files.insert(name.into(), file);
        }
        let Some(group) = self.files[name].group(LEVEL) else {
            return;
        };
        let clock = self.clocks.entry(group.files()).or_insert(0);
        if let Roll::Play(entry, _) = group.roll(clock, step, rng) {
            let mut shot = group.shot(entry, 0, 0);
            shot.at = Some(at);
            self.shots.push(shot);
        }
    }

    /// The calls the steps since the last call asked for.
    pub(crate) fn take(&mut self) -> Vec<Shot> {
        std::mem::take(&mut self.shots)
    }
}

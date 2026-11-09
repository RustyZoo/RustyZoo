//! What the runner reads of the data: the [`Data`] trait and the values it returns.

use super::super::{Call, Number, Row};
use crate::entity::EntityType;
use crate::zoo::Entity;

/// The frames of an animation and the time each lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub count: u32,
    pub delay: u32,
}

/// What the runner reads of the data, for the unit of an entity.
pub trait Data {
    /// The set `name` the unit starts: its calls, if it has any (behaviours 3.1).
    fn set(&self, unit: &Entity, name: &str) -> Option<&[Call]>;
    /// The rows of the unit's ambient table number `table` of [`super::super::TABLES`].
    fn ambient(&self, unit: &Entity, table: usize) -> &[Row];
    /// The frames and delay of one of the unit's animations; `None` if it has none of that name.
    fn timing(&self, unit: &Entity, animation: &str) -> Option<Timing>;
    /// The value of a number of a call (behaviours 5.8).
    fn number(&self, unit: &Entity, number: &Number) -> i32;
    /// The sound the unit's `[Sounds]` section names with this key.
    fn sound(&self, unit: &Entity, key: &str) -> Option<Heard>;
    /// A whole number of `[Characteristics/Integers]` of the unit class file `ztunit.ai`, shared by
    /// all units (pathfinding 6.5); `None` when the file or the key is missing.
    fn unit_class(&self, _key: &str) -> Option<i32> {
        None
    }
    /// The number of sprite files of the base animation set of an ambient type; 0 when it has
    /// none or it does not load (ambients 3.2).
    fn set_files(&self, _ty: &EntityType) -> usize {
        0
    }
    /// The rotor sound a helicopter type loads: the member of `cLoopSoundName` and
    /// `cLoopSoundAtten`; `None` when the file cannot be read (staff 7.11.2 point 3).
    fn loop_sound(&self, _unit: &Entity) -> Option<(String, i32)> {
        None
    }
    /// The bytes of the data file `name`; `None` when there is none.
    fn read(&self, _name: &str) -> Option<Vec<u8>> {
        None
    }
}

/// No data: no set, no animation.
pub(crate) struct NoData;

impl Data for NoData {
    fn set(&self, _: &Entity, _: &str) -> Option<&[Call]> {
        None
    }
    fn ambient(&self, _: &Entity, _: usize) -> &[Row] {
        &[]
    }
    fn timing(&self, _: &Entity, _: &str) -> Option<Timing> {
        None
    }
    fn number(&self, _: &Entity, number: &Number) -> i32 {
        match number {
            Number::Int(n) => *n,
            Number::Characteristic(_) => 0,
        }
    }
    fn sound(&self, _: &Entity, _: &str) -> Option<Heard> {
        None
    }
}

/// A sound of a `[Sounds]` key: the member of the file (behaviours 5.2), the base term `B` (the
/// second value of the key, sound 4.4) and the name of its voice: each key of each type has one
/// (sound 9.1).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Heard {
    pub file: String,
    pub base: i32,
    pub voice: String,
}

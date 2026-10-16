//! The data of the shipped game for the runner: the sets and tables of the catalogue and the
//! frames of the animations, read when a unit first plays one.

use std::cell::RefCell;
use std::collections::HashMap;

use super::{Call, Data, Heard, Number, Row, Timing};
use crate::anim::AnimSet;
use crate::entity::Catalogue;
use crate::sprite::Sprite;
use crate::vfs::Vfs;
use crate::zoo::Entity;

/// The timings read so far, by type (its position in the catalogue) and animation name; `None`
/// for an animation the type lacks or whose files cannot be read.
#[derive(Default)]
pub struct Timings(RefCell<HashMap<usize, HashMap<String, Option<Timing>>>>);

/// The catalogue and the files, with the timings they filled.
pub struct Library<'a> {
    pub catalogue: &'a Catalogue,
    pub vfs: &'a Vfs,
    pub timings: &'a Timings,
}

impl Library<'_> {
    /// The position in the catalogue of the type of a unit.
    fn at(&self, unit: &Entity) -> Option<usize> {
        let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
        let (name, sub) = (bytes(&unit.type_name), bytes(&unit.subtype));
        self.catalogue.position(&unit.class, &name, &sub)
    }

    /// The frames and delay of the first sprite file of the animation (all the files of a set
    /// have the same, so the timing does not depend on the view).
    fn read(&self, at: usize, animation: &str) -> Option<Timing> {
        let ty = &self.catalogue.types[at];
        let member = ty
            .animations()
            .into_iter()
            .find(|a| a.name == animation)?
            .member?;
        let set = AnimSet::load(self.vfs, &member).ok()?;
        let file = Sprite::parse(&self.vfs.read(set.files.first()?).ok()?).ok()?;
        Some(Timing {
            count: file.frames.len() as u32,
            delay: file.delay_ms,
        })
    }
}

impl Data for Library<'_> {
    fn set(&self, unit: &Entity, name: &str) -> Option<&[Call]> {
        let ty = &self.catalogue.types[self.at(unit)?];
        self.catalogue.behaviour_set(ty, name.as_bytes())
    }

    fn ambient(&self, unit: &Entity) -> &[Row] {
        match self.at(unit) {
            Some(at) => self.catalogue.ambient(&self.catalogue.types[at], 0),
            None => &[],
        }
    }

    fn timing(&self, unit: &Entity, animation: &str) -> Option<Timing> {
        let at = self.at(unit)?;
        if let Some(known) = self
            .timings
            .0
            .borrow()
            .get(&at)
            .and_then(|t| t.get(animation))
        {
            return *known;
        }
        let timing = self.read(at, animation);
        (self.timings.0.borrow_mut().entry(at))
            .or_default()
            .insert(animation.to_string(), timing);
        timing
    }

    fn number(&self, unit: &Entity, number: &Number) -> i32 {
        match number {
            Number::Int(n) => *n,
            Number::Characteristic(name) => (self.at(unit))
                .and_then(|at| crate::build::number(&self.catalogue.types[at], name))
                .map_or(0, |n| n as i32),
        }
    }

    fn sound(&self, unit: &Entity, key: &str) -> Option<Heard> {
        let ty = &self.catalogue.types[self.at(unit)?];
        let values = ty.config.as_deref()?.section("Sounds")?.get_all(key);
        let base = values
            .get(1)
            .map_or(0, |v| crate::config::to_int(v).unwrap_or(0));
        Some(Heard {
            file: ty.sound_member(values.first()?),
            base,
            voice: format!(
                "{}/{}/{key}",
                ty.class,
                String::from_utf8_lossy(&ty.type_name)
            ),
        })
    }
}

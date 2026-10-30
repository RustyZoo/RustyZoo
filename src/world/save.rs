//! The state a saved game takes from the live world and gives back (save games 7, 9): the
//! thoughts, and the statistics and last service of each exhibit.

use super::World;
use crate::building::Kind;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::goal;
use crate::guest::thought::Thought;
use crate::place::bulldozer::type_of;
use crate::render::tank_kind;
use crate::unit::Part;
use crate::zoo::{Common, Entity, Zoo, animal_new, fresh};
use crate::{animal, guest};
use std::collections::HashSet;

impl World {
    /// Takes the thoughts of the file, in file order, as the original's load does: one whose
    /// thinker or object is not an entity of the zoo is dropped; the exhibit is the one at the
    /// stored tile, if there is a tile.
    pub(super) fn load_thoughts(&mut self) {
        let ids: HashSet<u32> = (self.zoo.entities.iter())
            .filter_map(|e| e.common.as_ref().map(|c| c.id))
            .collect();
        for &[text, thinker, object, x, y] in self.zoo.thoughts() {
            let known = |id: u32| id == 0 || ids.contains(&id);
            if !known(thinker) || !known(object) {
                continue;
            }
            let tile = (x as i32 != -1 && y as i32 != -1)
                .then(|| self.tile_index(x as i32, y as i32))
                .flatten();
            let exhibit = tile
                .and_then(|t| self.exhibits.region(t))
                .filter(|&r| r != GROUNDS && r != EXTERIOR);
            self.units.thoughts.push_back(Thought {
                text: text as i32,
                thinker,
                object: Some(object).filter(|&o| o != 0),
                exhibit,
                gate: tile,
            });
        }
    }

    /// The zoo as a file stores it: without the entities `known` does not find (the original never
    /// makes them, so it never saves them or their thoughts), with the live thoughts and the live
    /// names, statistics and last service of the exhibits. An exhibit is matched with the record that
    /// names its tile; a record whose tile is in no exhibit keeps what it stored.
    pub fn saved(&self, known: impl Fn(&Entity) -> bool) -> Zoo {
        let mut zoo = self.zoo.clone();
        zoo.drop_unknown(known);
        let ids: HashSet<u32> = (zoo.entities.iter())
            .filter_map(|e| e.common.as_ref().map(|c| c.id))
            .collect();
        let tile = |t: Option<usize>| match t {
            Some(t) => [t as u32 % zoo.width, t as u32 / zoo.width],
            None => [u32::MAX; 2],
        };
        let thoughts = (self.units.thoughts.iter())
            .filter(|t| t.thinker == 0 || ids.contains(&t.thinker))
            .filter(|t| t.object.is_none_or(|o| ids.contains(&o)))
            .map(|t| {
                let [x, y] = tile(t.gate);
                [t.text as u32, t.thinker, t.object.unwrap_or(0), x, y]
            })
            .collect();
        zoo.set_thoughts(thoughts);
        let records = self.zoo.habitat_tiles.iter().zip(&mut zoo.habitat_info);
        for (&(x, y), info) in records {
            let exhibit = (self.tile_index(x, y))
                .and_then(|t| self.exhibits.region(t))
                .and_then(|id| self.exhibits.get(id));
            let Some(e) = exhibit else { continue };
            info.name.clone_from(&e.name);
            // Kept last month, this month, total; stored this month, last month, total.
            let order = |v: [u32; 3]| [v[1], v[0], v[2]];
            let (d, u) = (e.stats.donations, e.stats.upkeep);
            let bits = |v: [f32; 3]| order(v.map(f32::to_bits));
            let words = [bits(d), bits(u), order(e.stats.count)];
            info.stats = words.concat().try_into().unwrap_or(info.stats);
            info.viewing = e.stats.viewing;
            info.serviced = e.rating.serviced as u32;
        }
        zoo
    }

    /// Finishes a zoo from [`World::saved`] for writing, from the types of `cat`: the record of a
    /// transient type keeps no body (zoo format 6.2, the original refuses a file where it has one;
    /// a fence keeps its figures), an animal gets the record of its live state over the one it was
    /// read with (or the record of a new one, save games 11.3), a guest or a building made in play
    /// gets the record of a new one, a guest's record takes the live guest's fields, and a staff
    /// member's record takes its live state.
    pub fn finish_save(&self, zoo: &mut Zoo, cat: &Catalogue) {
        // A save writes the install language, whatever the file loaded said (localisation 10).
        zoo.locale = self.language.number;
        let now = crate::timing::file_time(zoo.clocks.calendar);
        for e in &mut zoo.entities {
            let (Some(ty), Some(c)) = (type_of(cat, e), e.common.as_mut()) else {
                continue;
            };
            if !matches!(e.class.as_str(), "fences" | "tankwall") && tank_kind(ty).transient {
                e.common = None;
                continue;
            }
            match e.class.as_str() {
                "animals" => {
                    let k = animal::Keys::of(ty, &self.units.class).researched(ty, &self.research);
                    let class = &self.units.class;
                    let new = || {
                        let mut a = animal_new(k.initial_happiness, k.max_hits, now);
                        a.female = matches!(e.subtype.as_str(), "f" | "p");
                        // The counters that are not drawn take their creation values.
                        (a.life, a.baby) = (
                            Some(k.time_death.wrapping_sub(1)),
                            k.baby_to_adult.wrapping_sub(1),
                        );
                        (a.interval, a.mate_delay) = (
                            k.reproduction_interval,
                            Some(class.mate_pick_delay.wrapping_sub(1)),
                        );
                        a.boxed = class.boxed.wrapping_sub(1);
                        a
                    };
                    let base = c.animal.take().map_or_else(new, |a| *a);
                    let live = self.units.get(c.id).filter(|u| u.started);
                    c.animal = Some(Box::new(match live.map(|u| &u.part) {
                        Some(Part::Animal(a)) => a.record(base),
                        _ => base,
                    }));
                }
                "guests" => {
                    if c.raw.is_new() {
                        let record = fresh::guest_new(guest::Keys::of(ty).initial_happiness, now);
                        c.guest.get_or_insert_with(|| Box::new(record));
                    }
                    self.live_guest(c);
                }
                "building" if c.raw.is_new() => {
                    let price = Kind::of(ty, self.units.prices.default).price;
                    c.building
                        .get_or_insert_with(|| fresh::building_new(price, now));
                }
                _ => {}
            }
            // The current upgrade of a building is the live one (research 6.3).
            if let (Some(b), true) = (
                c.building.as_mut(),
                self.units.buildings.contains_key(&c.id),
            ) {
                b.upgrade = self.units.upgrade_index.get(&c.id).copied().unwrap_or(0);
            }
            if matches!(
                e.class.as_str(),
                "staff" | "keeper" | "maint" | "tour" | "helicopter"
            ) {
                let base = c.staff.take().map_or_else(Default::default, |s| *s);
                let live = self.units.get(c.id).filter(|u| u.started);
                c.staff = Some(Box::new(match live.map(|u| &u.part) {
                    Some(Part::Staff(s)) => {
                        let tiles = (s.assigned.iter())
                            .filter_map(|&x| crate::staff::gate_tile(self, x))
                            .collect();
                        s.record(
                            base,
                            tiles,
                            crate::staff::duty_kinds(cat),
                            e.class == "tour",
                        )
                    }
                    _ => base,
                }));
            }
            if matches!(
                e.class.as_str(),
                "animals" | "guests" | "staff" | "keeper" | "maint" | "tour" | "helicopter"
            ) {
                let cost = cat
                    .guest_non_path_cost
                    .unwrap_or(fresh::DEFAULT_NON_PATH_COST);
                self.unit_made(c, &e.class, cost);
                self.live_way(c, cat, ty);
            }
        }
    }

    /// The unit block of a unit made in play (save games 11.2, 11.3). A unit that has had its first
    /// turn (`typed`, set by the first turn; a staff member is started at hiring, before it) writes
    /// its live goal and job numbers and is on the ground, and a guest its live non-path cost; one
    /// that has not writes the values of a new one, a guest's cost being its type's. A loaded unit
    /// keeps its stored block: the engine does not take the stored goal back at load.
    fn unit_made(&self, c: &mut Common, class: &str, cost: u32) {
        if !c.raw.is_new() {
            return;
        }
        let guest = (class == "guests").then_some(cost);
        let mut block = c.unit.take().map_or_else(|| fresh::unit_new(guest), |b| *b);
        if let Some(u) = self.units.get(c.id).filter(|u| u.typed) {
            let record = &u.goal.record;
            block.target.active = goal::active_goal(record.task, u.kind);
            block.target.active_sub = i32::from(record.job);
            block.move_kind = 2;
            if guest.is_some() {
                block.w4 = u.mover.non_path_cost;
            }
        }
        c.unit = Some(Box::new(block));
    }

    /// A started unit writes its live path (with its levels, -1 for none), destination, pending
    /// set name and behaviour set (the chain of calls it runs), not the ones its block was read
    /// with (behaviours 4.7, zoo format 6.14).
    fn live_way(&self, c: &mut Common, cat: &Catalogue, ty: &EntityType) {
        let Some(u) = self.units.get(c.id).filter(|u| u.started) else {
            return;
        };
        let Some(block) = c.unit.as_deref_mut() else {
            return;
        };
        let width = self.zoo.width;
        block.records = (u.path.iter().enumerate())
            .map(|(i, &t)| {
                let level = u.levels.get(i).map_or(u32::MAX, |&l| l as u32);
                [t as u32 % width, t as u32 / width, level]
            })
            .collect();
        block.p3 = u.destination.map_or([-1; 3], |(x, y, z)| [x, y, z]);
        block.pending = u.runner.queued().unwrap_or_default().to_string();
        let sets = |name: &str| cat.behaviour_set(ty, name.as_bytes());
        if let Some(set) = u.runner.saved(&block.behaviour, sets) {
            block.behaviour = set;
        }
    }

    /// A guest record takes the live guest's fields (a guest made in play has its drawn countdowns
    /// and its start values here, save games 11.3). A guest the game never made, which has no
    /// arrival date, keeps the record it has.
    fn live_guest(&self, c: &mut Common) {
        let unit = self.units.get(c.id);
        let live = unit.and_then(|u| match &u.part {
            Part::Guest(g) if g.arrived.is_some() => Some((g, u.goal.wandered)),
            _ => None,
        });
        if let (Some(base), Some((g, wandered))) = (c.guest.as_deref().copied(), live) {
            c.guest = Some(Box::new(g.record(&base, wandered)));
        }
    }
}

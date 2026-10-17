//! The megatiles and the surroundings check (guests 6.11, 12): the map cut in squares of 5 x 5
//! tiles whose guest count, esthetic value for each of the four guest types and stink are
//! counted again every 7.5 seconds, as the zoo status does at each step.
//!
//! The values come from the scenery objects and buildings (class `objects` or `building`) the
//! tiles hold: an object that has an `[EstheticBonus]` for a guest type or a `cStink` adds
//! `value / area` for each quadrant it holds, where `area` is its footprint in quadrants, so it
//! counts once in total. Dung stinks; litter and trash cans are ugly.

use super::{Guest, HAPPINESS, Keys};
use crate::entity::Catalogue;
use crate::status::zoo::{GUESTS, bonus};
use crate::world::World;
use std::collections::HashMap;

/// The side of a megatile in tiles, and the clock past which every megatile is counted again.
pub const SIDE: usize = 5;
const PERIOD: u32 = 7499;
/// The thoughts of the check: a beautiful area, a crowd, a smell.
const BEAUTIFUL: i32 = 10103;
const CROWDED: i32 = 10104;
const SMELLY: i32 = 10241;

/// What one megatile holds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Cell {
    pub guests: u32,
    /// By guest type: man, woman, boy, girl.
    pub esthetic: [f32; 4],
    pub stink: f32,
}

/// The megatiles of the map in rows, and the clock of the last count.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mega {
    clock: u32,
    cells: Vec<Cell>,
}

impl Mega {
    /// The megatile that holds tile `t` of a map `width` tiles wide.
    pub fn at(&self, width: usize, t: usize) -> Option<&Cell> {
        let row = (t / width.max(1) / SIDE) * width.div_ceil(SIDE);
        self.cells.get(row + t % width.max(1) / SIDE)
    }
}

/// The megatile manager's step: the clock grows by `step`, and past 7499 ms (or with no count
/// yet for this map) every megatile is counted again.
pub fn update(world: &mut World, cat: &Catalogue, step: u32) {
    let width = world.zoo.width as usize;
    let want = width.div_ceil(SIDE) * (world.zoo.tiles.len() / width.max(1)).div_ceil(SIDE);
    let m = &mut world.units.mega;
    m.clock = m.clock.saturating_add(step);
    if m.clock <= PERIOD && m.cells.len() == want {
        return;
    }
    m.clock = 0;
    world.units.mega.cells = count(world, cat, want);
}

/// 12: the guests and the scenery of every tile, added to the tile's megatile.
fn count(world: &World, cat: &Catalogue, cells: usize) -> Vec<Cell> {
    let width = world.zoo.width as usize;
    let mut out = vec![Cell::default(); cells];
    let cell_of = |t: usize| (t / width / SIDE) * width.div_ceil(SIDE) + t % width / SIDE;
    for (at, e) in world.zoo.entities.iter().enumerate() {
        if let (Some(t), "guests") = (super::view::tile_of(world, at), e.class.as_str()) {
            out[cell_of(t)].guests += 1;
        }
    }
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    // (bonus, stink) of an entity, shared by its quadrants.
    let mut known: HashMap<usize, Option<([f32; 4], f32)>> = HashMap::new();
    for t in 0..world.zoo.tiles.len() {
        for q in 0..4 {
            let Some(e) = world.quadrant(t, q) else {
                continue;
            };
            let found = *known.entry(e).or_insert_with(|| {
                let entity = &world.zoo.entities[e];
                let scenery = ["objects", "building"].contains(&entity.class.as_str());
                let ty = cat.find(
                    &entity.class,
                    &bytes(&entity.type_name),
                    &bytes(&entity.subtype),
                );
                let area = world.shape(e).map_or(1, |s| s.footprint.0 * s.footprint.1);
                let area = area.max(1) as f32;
                (ty.filter(|_| scenery)).map(|ty| {
                    let stink = crate::build::number(ty, "cStink").unwrap_or(0.0);
                    (bonus(ty).map(|b| b / area), stink / area)
                })
            });
            if let Some((bonus, stink)) = found {
                let cell = &mut out[cell_of(t)];
                cell.esthetic.iter_mut().zip(bonus).for_each(|(v, b)| *v += b);
                cell.stink += stink;
            }
        }
    }
    out
}

/// 6.11: the check of a guest of the type with name id `k.name_id` in megatile `cell`; the
/// thoughts it gives, in order.
pub fn check(g: &mut Guest, k: &Keys, cell: &Cell) -> Vec<i32> {
    let [beauty, happy, stink, sick, _] = k.env;
    let mut thoughts = Vec::new();
    let own = GUESTS.iter().position(|&id| id == k.name_id);
    if own.is_some_and(|i| cell.esthetic[i] >= beauty as f32) {
        g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(happy);
        thoughts.push(BEAUTIFUL);
    }
    let (limit, change) = k.view.crowd;
    if (cell.guests.saturating_mul(10) / 25) as i64 >= i64::from(limit) {
        g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(change);
        thoughts.push(CROWDED);
    }
    if cell.stink >= stink as f32 {
        if !g.sick {
            g.sick = true;
            g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(sick);
            thoughts.push(SMELLY);
        }
    } else {
        g.sick = false;
    }
    thoughts
}

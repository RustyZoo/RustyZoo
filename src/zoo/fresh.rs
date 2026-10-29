//! The class fields of an entity made in play, which has no body read from a file (zoo format 6.7
//! to 6.13), in the form of version 106. They come from the model where the entity has the figures
//! and are the values of a new one elsewhere: what the unit keeps while it runs is written by the
//! sub-issues of the animal and guest records; a staff member is written from its record.

use super::upgrade::NEVER;
use super::write::Out;
use super::{
    BuildingRecord, Common, Entity, Fence, Filter, GuestRecord, StaffRecord, Target, UnitBlock,
};

/// The fields of a filter after its scenery part (zoo format 6.8): the health, the three upkeep
/// figures, the build and service dates and the months count.
pub(super) fn filter(f: &Filter) -> Vec<u8> {
    let mut o = Out(Vec::new());
    o.u32(f.health);
    f.upkeep.iter().for_each(|&n| o.u32(n));
    [f.built, f.serviced]
        .iter()
        .for_each(|d| o.bytes(&d.to_le_bytes()));
    o.u32(f.months as u32);
    o.0
}

/// The fields before the unit block or the entity base, and the fields after them.
pub(super) fn parts(e: &Entity, c: &Common, now: u64) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (mut pre, mut post) = (Out(Vec::new()), Out(Vec::new()));
    match e.class.as_str() {
        // The fence figures are written from the model for every fence.
        "objects" | "food" | "paths" | "fences" | "tankwall" => {}
        "ambient" => pre.bytes(&[0; 8]),
        "filter" => post.bytes(&filter(&c.filter.unwrap_or(Filter {
            built: now,
            serviced: now,
            months: 1,
            ..Filter::default()
        }))),
        // An empty list of pieces and one word.
        "rubble" => post.bytes(&[0; 8]),
        "building" => building(
            &mut post,
            &c.building.clone().unwrap_or(building_new(0.0, now)),
        ),
        "animals" => {
            let new = || super::animal::animal_new(60, 500, now);
            let a = c.animal.as_deref().cloned().unwrap_or_else(new);
            super::animal::write(&mut pre, &a)?;
        }
        "guests" => {
            let record = c.guest.as_deref().copied();
            pre.bytes(&guest(None, &record.unwrap_or(guest_new(40, now)))?);
        }
        "staff" | "keeper" | "maint" | "tour" | "helicopter" => {
            let new = StaffRecord::default();
            super::staff::write(
                &mut pre,
                e.class.as_bytes(),
                c.staff.as_deref().unwrap_or(&new),
            );
        }
        _ => {
            return Err(format!(
                "{}/{}: no body to write for an entity that was not read from a file",
                e.class, e.type_name
            ));
        }
    }
    Ok((pre.0, post.0))
}

/// The four values of a fence or tank wall record, and the portal flag of a tank wall.
pub(super) fn fence(class: &str, f: &Fence) -> Vec<u8> {
    let mut o = Out(Vec::new());
    [f.strength, f.life as u32, f.timers[0], f.timers[1]]
        .iter()
        .for_each(|&n| o.u32(n));
    if class == "tankwall" {
        o.bytes(&[f.portal.into(), 0]);
    }
    o.0
}

/// The non-path cost of a guest type when the unit class file gives none (pathfinding 6.5).
pub(crate) const DEFAULT_NON_PATH_COST: u32 = 7;

/// The unit block of a unit made in play, before it has run (save games 11.3): no goal, no target,
/// no movement in progress, the counters a new unit has and the call flag of a unit made in play.
/// `guest` is the non-path cost of a guest, which is its word 230; the other units write 1 there.
pub(crate) fn unit_new(guest: Option<u32>) -> UnitBlock {
    UnitBlock {
        slot: 255,
        need: 255,
        w4: guest.unwrap_or(1),
        w7: 4,
        p1: [-1; 3],
        p3: [-1; 3],
        target: Target {
            target: [-1; 3],
            sub_target: [-1; 3],
            active: -1,
            active_sub: -1,
            ..Target::default()
        },
        move_kind: -1,
        pending_flag: 1,
        ..UnitBlock::default()
    }
}

/// The record of a new guest building: no figure yet, the type's default price (save games 11.3).
pub(crate) fn building_new(price: f32, now: u64) -> BuildingRecord {
    BuildingRecord {
        guests: [0; 3],
        income: [0; 3],
        upkeep: Some(0),
        upgrade: 0,
        price: price.to_bits(),
        users: Vec::new(),
        users_now: 0,
        fill: 0,
        built: now,
    }
}

/// The figures of a guest building.
fn building(o: &mut Out, b: &BuildingRecord) {
    o.u32(b.users_now);
    o.u32(b.guests[2]);
    o.u32(b.income[2]);
    o.u32(b.price);
    o.u32(b.users.len() as u32);
    for &(unit, slot) in &b.users {
        o.u32(unit);
        o.u32(slot);
    }
    o.u32(b.fill);
    for n in [b.income[1], b.income[0], b.guests[1], b.guests[0]] {
        o.u32(n);
    }
    o.bytes(&b.built.to_le_bytes());
    o.u32(b.upkeep.unwrap_or(0));
    // The index of the current upgrade and the empty list of members.
    o.u32(b.upgrade);
    o.u32(0);
}

/// Writes the index of the current upgrade into the building fields `post` of version 106 (they
/// come from a body of version 70 or later, the oldest the writer upgrades): it follows the users and the figures.
pub(super) fn set_upgrade(post: &mut [u8], index: u32) {
    let users = post.get(16..20).and_then(|n| <[u8; 4]>::try_from(n).ok());
    let at = users.map(|n| 52 + 8 * u32::from_le_bytes(n) as usize);
    if let Some(word) = at.and_then(|at| post.get_mut(at..at + 4)) {
        word.copy_from_slice(&index.to_le_bytes());
    }
}

/// A new guest (save games 11.3) with no live state: the type's initial happiness, every need at 0,
/// no item, no flag.
pub(crate) fn guest_new(happiness: i32, now: u64) -> GuestRecord {
    GuestRecord {
        values: [happiness, 0, 0, 0, 0],
        countdowns: [0; 11],
        arrived: now,
        favourite: -1,
        sick: false,
        saw_animal: 0,
        female: false,
        child: false,
        items: [0; 5],
        leave: false,
        afraid: false,
        gift: false,
        wandered: false,
        stand: false,
        in_group: false,
        listening: false,
        guide: 0,
    }
}

/// Where the item slots start in a guest record of version 106, and how long the tail after the
/// last list is (zoo format 6.12).
const ITEMS_AT: usize = 89;
const TAIL: usize = 42;

/// The guest record (zoo format 6.12) of version 106: the record `kept` as read with the fields of
/// the model written over it, or, for a new guest, the fields the model lacks as a new guest has
/// them (the five increment factors 1, the lists empty).
pub(super) fn guest(kept: Option<Vec<u8>>, g: &GuestRecord) -> Result<Vec<u8>, String> {
    let mut b = kept.unwrap_or_else(|| {
        let mut o = Out(vec![0; 21]);
        (0..5).for_each(|_| o.u32(1));
        // Female, child, the ten countdowns, the count of items (the slots are put in below), the
        // list of seen things, a word and an empty string, and the list of pairs.
        o.bytes(&[0; 8 + 40 + 4 + 4 + 4 + 4 + 4]);
        o.bytes(&[0; TAIL]);
        let at = o.0.len() - TAIL + 26;
        o.0[at..at + 8].copy_from_slice(&NEVER.to_le_bytes());
        o.0
    });
    let short = || "the guest record is short".to_string();
    let (head, tail) = (ITEMS_AT + 4, b.len().checked_sub(TAIL).ok_or_else(short)?);
    let n = b.get(ITEMS_AT..head).ok_or_else(short)?;
    // checked: on a 32-bit target `4 * count` overflows for a wild count.
    let count = u32::from_le_bytes(n.try_into().unwrap()) as usize;
    let end = count.checked_mul(4).and_then(|c| c.checked_add(head));
    let end = end.filter(|&e| e <= tail).ok_or_else(short)?;
    // Five item slots, the way the original writes them.
    let items = g.items.iter().flat_map(|n| n.to_le_bytes());
    b.splice(ITEMS_AT..end, [5, 0, 0, 0].into_iter().chain(items));
    let tail = b.len() - TAIL;
    let word = |b: &mut [u8], at: usize, n: u32| b[at..at + 4].copy_from_slice(&n.to_le_bytes());
    for (i, v) in g.values.iter().enumerate() {
        word(&mut b, 4 * i, *v as u32);
    }
    b[20] = g.sick.into();
    word(&mut b, 41, g.female.into());
    word(&mut b, 45, g.child.into());
    // The ten countdowns of the block: all but the chase one, which follows the lists.
    let others = (g.countdowns.iter().enumerate()).filter_map(|(i, &n)| (i != 6).then_some(n));
    for (i, n) in others.enumerate() {
        word(&mut b, 49 + 4 * i, n as u32);
    }
    word(&mut b, tail, g.countdowns[6] as u32);
    b[tail + 4] = g.leave.into();
    b[tail + 5] = g.afraid.into();
    word(&mut b, tail + 6, g.saw_animal as u32);
    b[tail + 11] = g.gift.into();
    b[tail + 12..tail + 20].copy_from_slice(&g.arrived.to_le_bytes());
    word(&mut b, tail + 20, g.favourite as u32);
    b[tail + 24] = g.wandered.into();
    b[tail + 25] = g.stand.into();
    b[tail + 36] = g.in_group.into();
    b[tail + 37] = g.listening.into();
    word(&mut b, tail + 38, g.guide);
    Ok(b)
}

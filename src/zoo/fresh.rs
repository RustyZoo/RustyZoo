//! The class fields of an entity made in play, which has no body read from a file (zoo format 6.7
//! to 6.13), in the form of version 106. They come from the model where the entity has the figures
//! and are the values of a new one elsewhere: what the unit keeps while it runs is written by the
//! sub-issues of the animal and guest records; a staff member is written from its record.

use super::upgrade::NEVER;
use super::write::Out;
use super::{BuildingRecord, Common, Entity, Fence, GuestRecord, StaffRecord, Target, UnitBlock};

/// The fields before the unit block or the entity base, and the fields after them.
pub(super) fn parts(e: &Entity, c: &Common, now: u64) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (mut pre, mut post) = (Out(Vec::new()), Out(Vec::new()));
    match e.class.as_str() {
        // The fence figures are written from the model for every fence.
        "objects" | "food" | "paths" | "fences" | "tankwall" => {}
        "ambient" => pre.bytes(&[0; 8]),
        "filter" => {
            // Health, the three upkeep figures, built and serviced, and the word that is 1 when absent.
            post.u32(c.filter.map_or(0, |f| f.health));
            post.bytes(&[0; 12]);
            [now, now].iter().for_each(|d| post.bytes(&d.to_le_bytes()));
            post.u32(1);
        }
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

/// The unit block of a unit made in play, before it has run (save games 11.3): no goal, no target and
/// no movement in progress, the counters a new unit has. A guest's word 230 is 0: the table gives it
/// for animals and staff only.
pub(crate) fn unit_new(guest: bool) -> UnitBlock {
    UnitBlock {
        slot: 255,
        need: 255,
        w4: u32::from(!guest),
        w7: 4,
        p3: [-1; 3],
        target: Target {
            target: [-1; 3],
            active: -1,
            active_sub: -1,
            ..Target::default()
        },
        move_kind: -1,
        ..UnitBlock::default()
    }
}

/// The record of a new guest building: no figure yet, the type's default price (save games 11.3).
pub(crate) fn building_new(price: f32, now: u64) -> BuildingRecord {
    BuildingRecord {
        guests: [0; 3],
        income: [0; 3],
        upkeep: Some(0),
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
    // The unknown word and the empty list of members.
    o.bytes(&[0; 8]);
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
    let end = head + 4 * u32::from_le_bytes(n.try_into().unwrap()) as usize;
    if end > tail {
        return Err(short());
    }
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

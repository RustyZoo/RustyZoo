//! The class fields of an entity made in play, which has no body read from a file (zoo format 6.7
//! to 6.13), in the form of version 106. They come from the model where the entity has the figures
//! and are the values of a new one elsewhere: what the unit keeps while it runs is written by the
//! sub-issues of the animal, guest and staff records.

use super::upgrade::NEVER;
use super::write::Out;
use super::{AnimalRecord, BuildingRecord, Common, Entity, Fence, GuestRecord};

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
        "animals" => animal(
            &mut pre,
            c.animal.as_deref().unwrap_or(&animal_new(60, 500)),
            now,
        ),
        "guests" => {
            let record = c.guest.as_deref().copied();
            pre.bytes(&guest(None, &record.unwrap_or(guest_new(40, now)))?);
        }
        "staff" | "keeper" | "maint" | "tour" | "helicopter" => {
            // The own fields of the class, then the common staff block: all 0, the lists empty.
            let own = match e.class.as_str() {
                "keeper" => 21,
                "maint" => 18,
                "tour" => 13,
                "helicopter" => 12,
                _ => 0,
            };
            pre.bytes(&vec![0; own + 16]);
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

/// A new animal (save games 11.3): the type's initial happiness and full health, not hungry, no
/// energy, no counter running.
pub(crate) fn animal_new(happiness: i32, health: i32) -> AnimalRecord {
    AnimalRecord {
        happiness,
        hunger: 0,
        health,
        energy: 0,
        dung: 0,
        female: false,
        counters: [None; 16],
        breath: None,
        at_minimum_health: false,
        very_unhappy: false,
        keeper_visited: false,
        mourning: false,
        dying: false,
        crated: false,
        egg: false,
        thirsty: false,
        rampaging: false,
        rampage: 0,
    }
}

/// The animal record (zoo format 6.11): what the model holds, 0 or a date elsewhere.
fn animal(o: &mut Out, a: &AnimalRecord, now: u64) {
    let c = |i: usize| a.counters[i].unwrap_or(0);
    for n in [a.happiness, a.hunger, a.health, a.energy] {
        o.i32(n);
    }
    // At minimum health, sleeping, eating, very unhappy and one more; then 25 unknown bytes.
    o.bytes(&[a.at_minimum_health.into(), 0, 0, a.very_unhappy.into(), 0]);
    o.bytes(&[0; 25]);
    o.string(if a.female { "Female" } else { "Male" }).ok();
    o.u32(0);
    for i in [0, 2, 3, 5, 6, 7, 8, 9, 10, 11] {
        o.i32(c(i));
    }
    o.bytes(&[0; 8]);
    o.i32(c(12));
    o.i32(a.dung);
    o.bytes(&[a.keeper_visited.into(), 0, 0]);
    o.i32(c(4));
    o.bytes(&[0; 6 + 12]);
    [now, now].iter().for_each(|d| o.bytes(&d.to_le_bytes()));
    o.bytes(&[0; 2 + 8 + 12]);
    o.bytes(&[a.dying.into(), a.mourning.into()]);
    o.i32(c(14));
    o.u32(0); // not escaped
    o.bytes(&[0; 9]);
    o.bytes(&[a.crated.into(), a.thirsty.into(), 0, a.rampaging.into()]);
    o.bytes(&[0; 4]);
    o.i32(a.rampage);
    o.bytes(&[0, a.egg.into(), 0, 0, 0, 0]);
    o.i32(a.breath.unwrap_or(0));
    o.i32(c(1));
    // No home exhibit.
    o.bytes(&[0xff; 8]);
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

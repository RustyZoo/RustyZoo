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
        "building" => building(&mut post, c.building.as_ref(), now),
        "animals" => animal(&mut pre, c.animal.as_deref().unwrap_or(&animal_new()), now),
        "guests" => guest(&mut pre, c.guest.as_deref().unwrap_or(&guest_new(now))),
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

/// The figures of a guest building: none yet for a new one.
fn building(o: &mut Out, b: Option<&BuildingRecord>, now: u64) {
    let none = BuildingRecord {
        guests: [0; 3],
        income: [0; 3],
        upkeep: Some(0),
        price: 0,
        users: Vec::new(),
        fill: 0,
        built: now,
    };
    let b = b.unwrap_or(&none);
    o.u32(0); // users now
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

/// A new animal: unhurt and content, no counter running.
fn animal_new() -> AnimalRecord {
    AnimalRecord {
        happiness: 100,
        hunger: 50,
        health: 100,
        energy: 100,
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

/// A new guest: happy, rested, and not hungry or thirsty.
fn guest_new(now: u64) -> GuestRecord {
    GuestRecord {
        values: [50, 0, 0, 100, 0],
        countdowns: [0; 11],
        arrived: now,
        favourite: -1,
        sick: false,
        saw_animal: 0,
        female: false,
        child: false,
    }
}

/// The guest record (zoo format 6.12).
fn guest(o: &mut Out, g: &GuestRecord) {
    g.values.iter().for_each(|&n| o.i32(n));
    o.u8(g.sick.into());
    o.bytes(&[0; 20]);
    o.u32(g.female.into());
    o.u32(g.child.into());
    // The ten countdowns of the block: all but the chase one.
    for (i, &n) in g.countdowns.iter().enumerate() {
        if i != 6 {
            o.i32(n);
        }
    }
    // The item slots and an unknown list, both empty; an unknown word and an empty string.
    o.bytes(&[0; 16]);
    o.u32(0); // the list of pairs
    o.i32(g.countdowns[6]);
    o.bytes(&[0, 0]);
    o.i32(g.saw_animal);
    o.bytes(&[0, 0]);
    o.bytes(&g.arrived.to_le_bytes());
    o.i32(g.favourite);
    o.bytes(&[0, 0]);
    o.bytes(&NEVER.to_le_bytes());
    // Two bytes, the in-group and listening flags, and the guide id.
    o.bytes(&[0; 2 + 1 + 1 + 4]);
}

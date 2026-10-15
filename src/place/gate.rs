//! The gate tool (placing 6.4): over a fence piece of an exhibit the pending object is the gate
//! of that fence type at the piece's place, and a click moves the exhibit's gate there.
//! Valid places are green where a keeper passes and in the highlight colour elsewhere.

use super::bulldozer::type_of;
use super::{LIFE, NO_FUNDS, Outcome, Pending, STRENGTH, Verdict};
use crate::build::{cost, number};
use crate::entity::Catalogue;
use crate::money::{CONSTRUCTION, Money};
use crate::world::World;

/// The pending gate and what a click does with it: the fence piece (an entity id) that becomes
/// the gate of the exhibit.
#[derive(Debug, Clone, PartialEq)]
pub struct Gate {
    pub pending: Pending,
    pub piece: u32,
    pub exhibit: u32,
    /// The cost text is hidden: a quadrant of the tank tile holds an object (placing 6.4 item 4).
    pub hidden: bool,
}

/// The id of the entity at position `at` when it is a fence piece, the hovered entity of the
/// gate tool (the entity the pixel pick finds, placing 6.4).
pub fn over(world: &World, at: usize) -> Option<u32> {
    world.slot(at)?;
    Some(world.zoo.entities[at].common.as_ref()?.id)
}

/// The pending gate for the entity at position `at`, or none when it is no fence piece of an
/// exhibit fence type with a gate. The place is valid when [`World::gate_for`] finds an
/// exhibit, no object stands on the tile of a tank and the cash covers the price (placing 6.4):
/// nothing for a piece above the decayed life of its type, else the price of the gate type, and
/// tinted by [`World::keeper_passes`]. An invalid place keeps no message, except for the cash,
/// which shows 10002; an object on a tank tile also hides the cost text.
pub fn pending(world: &World, cat: &Catalogue, money: &Money, at: usize) -> Option<Gate> {
    let piece = &world.zoo.entities[at];
    let (tile, facing) = world.slot(at)?;
    let c = piece.common.as_ref()?;
    let old = type_of(cat, piece)?;
    let ty = cat.position(&piece.class, &bytes(&piece.type_name), b"g")?;
    let life = c
        .fence
        .map_or_else(|| number(old, "cLife").unwrap_or(LIFE), |f| f.life as f32);
    let price = match life > number(old, "cDecayedLife").unwrap_or(0.0) {
        true => 0.0,
        false => cost(&cat.types[ty]),
    };
    let exhibit = world.gate_for(at);
    let hidden = exhibit.is_some() && world.tank_holds_object(at);
    let exhibit = exhibit.filter(|_| !hidden);
    let afford = money.can_pay(price);
    let width = world.zoo.width as usize;
    let (lx, ly) = (c.x - 64 * c.x.div_euclid(64), c.y - 64 * c.y.div_euclid(64));
    let pending = Pending {
        ty,
        x: c.x,
        y: c.y,
        z: c.z,
        facing: facing as u32,
        tile: ((tile % width) as i32, (tile / width) as i32),
        quadrant: (ly / 32 * 2 + lx / 32).clamp(0, 3) as usize,
        verdict: match (exhibit.is_some() && afford, world.keeper_passes(at)) {
            (true, true) => Verdict::Valid,
            (true, false) => Verdict::Highlight,
            _ => Verdict::Invalid,
        },
        message: (exhibit.is_some() && !afford).then_some(NO_FUNDS),
        price,
        paint: None,
        blocking: Vec::new(),
    };
    Some(Gate {
        pending,
        piece: c.id,
        exhibit: exhibit.unwrap_or(0),
        hidden,
    })
}

/// The click on the pending gate (placing 6.4): a valid place moves the exhibit's gate to the
/// piece, made healthy, and books the price as construction; any other place does nothing, and
/// shows the message of the place if it has one.
pub fn place(world: &mut World, money: &mut Money, cat: &Catalogue, gate: &Gate) -> Outcome {
    let p = &gate.pending;
    if p.verdict == Verdict::Invalid {
        return Outcome::Refused(p.message);
    }
    // The cash is tested again at the click.
    if !money.can_pay(p.price) {
        return Outcome::Refused(Some(NO_FUNDS));
    }
    let ty = &cat.types[p.ty];
    let life = number(ty, "cLife").unwrap_or(LIFE) as u32;
    let strength = number(ty, "cStrength").unwrap_or(STRENGTH) as u32;
    if !world.move_gate(gate.exhibit, gate.piece, life, strength) {
        return Outcome::Refused(None);
    }
    money.charge(CONSTRUCTION, p.price);
    Outcome::Placed(gate.piece)
}

fn bytes(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u8).collect()
}

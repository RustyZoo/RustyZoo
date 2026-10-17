//! The animation and the sounds of the gates a unit opens and closes (movement 6.2).
//!
//! A gate has an "open" flag and a "playing" flag. Opening plays its `idle<slope>` animation
//! forward with the type's `cOpenSound`, closing plays it reversed with `cCloseSound`. When the
//! animation ends the same one is set again without sound, reversed for an open gate and forward
//! for a closed one, so an open gate rests on its last frame and a closed one on its first.
//! A gate that is closed and not playing needs no state.

use std::collections::HashMap;

use super::{Kind, Unit, Units};
use crate::behaviour::{Data, Heard, Timing};
use crate::entity::Catalogue;
use crate::render::corner_elevation;
use crate::world::World;

/// The state of one gate.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gate {
    open: bool,
    playing: bool,
    /// The animation runs reversed: `frame` counts from the end of the file.
    reverse: bool,
    frame: u32,
    elapsed: u32,
    /// The frames of the animation, as the last advance saw them.
    count: u32,
    /// The sound of the entry that just started waits for the next advance.
    fresh: bool,
}

impl Gate {
    /// The frame to draw.
    fn shown(&self) -> u32 {
        match self.reverse {
            true => self.count.saturating_sub(1).saturating_sub(self.frame),
            false => self.frame,
        }
    }
}

/// The gates that are open or playing, by the id of the piece.
pub type Gates = HashMap<u32, Gate>;

impl Units {
    /// The gate `id` opens: its animation plays forward from the start, with the open sound.
    pub fn open_gate(&mut self, id: u32) {
        self.play_gate(id, true);
    }

    /// The gate `id` closes: its animation plays reversed from the start, with the close sound.
    pub fn close_gate(&mut self, id: u32) {
        self.play_gate(id, false);
    }

    fn play_gate(&mut self, id: u32, open: bool) {
        let count = self.gates.get(&id).map_or(0, |g| g.count);
        let gate = Gate {
            open,
            playing: true,
            reverse: !open,
            count,
            fresh: true,
            ..Gate::default()
        };
        self.gates.insert(id, gate);
    }

    /// A keeper gives up the exhibit it was going to: it closes the gate it remembers and forgets
    /// it (movement 6.2, last point).
    pub fn release_gate(&mut self, unit: &mut Unit) {
        if unit.kind == Kind::Keeper
            && let Some(id) = unit.gate.take()
        {
            self.close_gate(id);
        }
    }

    /// [`Units::release_gate`] for the unit `id` of the table.
    pub fn release_gate_of(&mut self, id: u32) {
        let gate = (self.get_mut(id))
            .filter(|u| u.kind == Kind::Keeper)
            .and_then(|u| u.gate.take());
        gate.into_iter().for_each(|g| self.close_gate(g));
    }

    /// The frame each open or playing gate shows, by the id of the piece.
    pub fn gate_frames(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.gates.iter().map(|(&id, g)| (id, g.shown()))
    }
}

/// The animation of the gate at `at`: `idle`, with the slope of the edge it stands on (sprite
/// drawing 3.6).
fn animation(world: &World, at: usize) -> &'static str {
    let Some(c) = world.zoo.entities[at].common.as_ref() else {
        return "idle";
    };
    let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
    let Some(tile) = tile.and_then(|t| world.zoo.tiles.get(t)) else {
        return "idle";
    };
    let [first, second] = match c.rotation % 8 / 2 {
        0 => [7, 1],
        1 => [1, 3],
        2 => [3, 5],
        _ => [5, 7],
    };
    match corner_elevation(tile, second).cmp(&corner_elevation(tile, first)) {
        std::cmp::Ordering::Greater => "idle30p",
        std::cmp::Ordering::Less => "idle30n",
        std::cmp::Ordering::Equal => "idle",
    }
}

/// The sound the type of the gate at `at` gives to opening or closing, with its attenuation.
fn heard(world: &World, cat: &Catalogue, at: usize, open: bool) -> Option<Heard> {
    let e = &world.zoo.entities[at];
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let ty = &cat.types[cat.position(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))?];
    let (key, atten) = match open {
        true => ("cOpenSound", "cOpenSoundAtten"),
        false => ("cCloseSound", "cCloseSoundAtten"),
    };
    let config = ty.config.as_deref()?;
    let read = |prefix: &[u8]| {
        let section = config.section([prefix, b"Characteristics/Integers"].concat())?;
        section.get(key).filter(|v| !v.is_empty())
    };
    let value = read(&[&ty.subtype[..], b"/"].concat()).or_else(|| read(b""))?;
    let base = crate::build::number(ty, atten).map_or(0, |n| n as i32);
    Some(Heard {
        file: ty.sound_member(value),
        base,
        voice: format!(
            "{}/{}/{key}",
            ty.class,
            String::from_utf8_lossy(&ty.type_name)
        ),
    })
}

/// The turn of the entity at `at` for the gates: if it is a gate that is open or playing, its
/// animation advances by `step` milliseconds, and the sound of an entry that just started is
/// handed to the app.
pub fn advance(world: &mut World, cat: &Catalogue, data: &dyn Data, at: usize, step: u32) {
    if world.units.gates.is_empty() {
        return;
    }
    let Some(id) = world.zoo.entities[at].common.as_ref().map(|c| c.id) else {
        return;
    };
    let Some(mut gate) = world.units.gates.get(&id).copied() else {
        return;
    };
    let name = animation(world, at);
    let timing = data.timing(&world.zoo.entities[at], name);
    let Timing { count, delay } = timing.unwrap_or(Timing { count: 0, delay: 0 });
    gate.count = count;
    if std::mem::take(&mut gate.fresh) {
        let sound = heard(world, cat, at, gate.open);
        world.units.sounds.extend(sound.map(|s| (id, s)));
    }
    if gate.playing {
        gate.elapsed = gate.elapsed.saturating_add(step);
        let mut ended = delay == 0 || count == 0;
        while !ended && gate.elapsed >= delay {
            gate.elapsed -= delay;
            gate.frame += 1;
            ended = gate.frame >= count;
        }
        if ended {
            // The same animation is set again without sound: reversed when the gate is open.
            (gate.playing, gate.reverse) = (false, gate.open);
            (gate.frame, gate.elapsed) = (0, 0);
        }
    }
    match gate.open || gate.playing {
        true => world.units.gates.insert(id, gate),
        false => world.units.gates.remove(&id),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviour::NoData;
    use crate::config::Config;
    use crate::entity::EntityType;
    use crate::unit::testing::{centre, entity, world_with};
    use crate::zoo::Entity;

    /// A gate whose animation has four frames of 100 ms.
    struct Frames;

    impl Data for Frames {
        fn set(&self, e: &Entity, n: &str) -> Option<&[crate::behaviour::Call]> {
            NoData.set(e, n)
        }
        fn ambient(&self, e: &Entity, t: usize) -> &[crate::behaviour::Row] {
            NoData.ambient(e, t)
        }
        fn timing(&self, _: &Entity, name: &str) -> Option<Timing> {
            (name == "idle").then_some(Timing {
                count: 4,
                delay: 100,
            })
        }
        fn number(&self, e: &Entity, n: &crate::behaviour::Number) -> i32 {
            NoData.number(e, n)
        }
        fn sound(&self, e: &Entity, k: &str) -> Option<Heard> {
            NoData.sound(e, k)
        }
    }

    fn gate_world() -> (World, Catalogue) {
        let mut gate = entity("fences", "iron", 7, centre((2, 2)), 0);
        gate.subtype = "g".into();
        let w = world_with((6, 6), |z| z.entities.push(gate));
        let mut cat = Catalogue::default();
        let ini = "[g/Characteristics/Integers]\ncOpenSound=sounds/open.wav\ncOpenSoundAtten=1600\n\
                   cCloseSound=close\n";
        cat.push(EntityType {
            class: "fences",
            type_name: b"iron".to_vec(),
            subtype: b"g".to_vec(),
            file: None,
            config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
        (w, cat)
    }

    /// Steps the gate by 100 ms `n` times and returns the frame it shows after each.
    fn run(w: &mut World, cat: &Catalogue, n: usize) -> Vec<Option<u32>> {
        (0..n)
            .map(|_| {
                advance(w, cat, &Frames, 0, 100);
                w.units.gate_frames().next().map(|(_, f)| f)
            })
            .collect()
    }

    #[test]
    fn an_opening_gate_plays_forward_with_its_sound_and_rests_on_its_last_frame() {
        let (mut w, cat) = gate_world();
        w.units.open_gate(7);
        assert_eq!(
            run(&mut w, &cat, 6),
            [1, 2, 3, 3, 3, 3].map(Some),
            "the last frame is held"
        );
        let heard = w.units.take_sounds();
        assert_eq!(heard.len(), 1, "one sound, at the start");
        assert_eq!(heard[0].0, 7);
        assert_eq!(
            (heard[0].1.file.as_str(), heard[0].1.base),
            ("sounds/open.wav", 1600)
        );
    }

    #[test]
    fn a_closing_gate_plays_reversed_with_its_sound_and_rests_closed() {
        let (mut w, cat) = gate_world();
        w.units.open_gate(7);
        run(&mut w, &cat, 6);
        w.units.take_sounds();
        w.units.close_gate(7);
        assert_eq!(
            run(&mut w, &cat, 5),
            [Some(2), Some(1), Some(0), None, None]
        );
        let heard = w.units.take_sounds();
        assert_eq!(heard.len(), 1);
        assert!(heard[0].1.file.ends_with("close.wav") && heard[0].1.base == 0);
        // Closing a closed gate plays it again all the same.
        w.units.close_gate(7);
        assert_eq!(run(&mut w, &cat, 1), [Some(2)]);
    }

    #[test]
    fn a_keeper_gives_up_its_gate_but_a_guest_does_not() {
        for (kind, closes) in [(Kind::Keeper, true), (Kind::Guest, false)] {
            let (mut w, _) = gate_world();
            let mut unit = Unit::new(1, kind);
            unit.gate = Some(7);
            w.units.release_gate(&mut unit);
            assert_eq!(w.units.gate_frames().count(), usize::from(closes));
            assert_eq!(unit.gate.is_none(), closes);
        }
    }

    #[test]
    fn a_world_without_open_gates_and_a_type_without_sound_are_silent() {
        let (mut w, cat) = gate_world();
        advance(&mut w, &cat, &Frames, 0, 100);
        assert_eq!(w.units.gate_frames().count(), 0);
        w.units.open_gate(7);
        advance(&mut w, &Catalogue::default(), &NoData, 0, 100);
        assert!(w.units.take_sounds().is_empty());
    }

    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn the_gates_of_the_game_have_a_timing_and_the_expansion_ones_a_sound() {
        use crate::behaviour::{Library, Timings};
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
        let cat = Catalogue::load(&vfs).unwrap();
        let timings = Timings::default();
        let data = Library {
            catalogue: &cat,
            vfs: &vfs,
            timings: &timings,
        };
        let (mut timed, mut sounds) = (0, 0);
        for ty in &cat.types {
            if ty.class != "fences" || !ty.subtype.starts_with(b"g") {
                continue;
            }
            let e = crate::unit::testing::entity("fences", "x", 1, (0, 0), 0);
            let e = Entity {
                type_name: String::from_utf8_lossy(&ty.type_name).into(),
                subtype: String::from_utf8_lossy(&ty.subtype).into(),
                ..e
            };
            let t = data.timing(&e, "idle");
            timed += usize::from(t.is_some_and(|t| t.count > 1 && t.delay > 0));
            let w = world_with((6, 6), |z| z.entities.push(e));
            sounds += usize::from(heard(&w, &cat, 0, true).is_some());
        }
        assert!(
            timed > 0 && sounds > 0,
            "{timed} timed, {sounds} with sound"
        );
    }
}

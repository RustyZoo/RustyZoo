//! The hooks of a unit's turn, the game's [`Standard`] ones and the turn itself.

use super::{Ctx, Kind, Part, Unit, movement, swim};
use crate::behaviour::{Data, Env, Flow, Note};
use crate::world::World;

/// The work of a unit's turn that later issues fill. Each gets the unit (taken out of the table
/// for the turn), the world, the managers' context and the unit's position in the entity list.
pub(crate) trait Hooks {
    /// The unit's first turn, before `start`, for every unit: the characteristics of its type.
    fn typed(&mut self, _: &mut Unit, _: &mut World, _at: usize) {}
    /// The unit's first turn, before anything else (simulation 8.2: the creation of its state).
    fn start(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The events read at the start of the turn (`Unit::events`), before the move.
    fn events(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The move by the step (movement 3); not run while suspended.
    fn movement(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The status variables (the needs); at a status tick, not run while suspended.
    fn needs(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The conditional checks; at a status tick, right after the needs.
    fn checks(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The animation, advanced by `advance` milliseconds (sprite drawing 6.2); it runs even when
    /// the unit is suspended.
    fn animate(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize, _advance: u32) {}
    /// The behaviour set (behaviours 4.6); not run while suspended or waiting.
    fn behaviour(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The goals (simulation 6.6); not run while suspended or waiting.
    fn goals(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
}

/// The hooks of the game: the move, the animation and the behaviour set from the data, the
/// animals' status work and the goals.
pub(crate) struct Standard<'a> {
    pub data: &'a dyn Data,
}

impl Standard<'_> {
    /// The calls of a use set that reach the building (guest buildings 6.4.1).
    fn building_calls(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        for enter in [true, false] {
            let ran = if enter {
                unit.runner.take_enter()
            } else {
                unit.runner.take_exit()
            };
            if ran {
                crate::goal::use_call(unit, world, ctx, at, self.data, enter);
            }
        }
    }

    /// Tells the log and the app what the unit's animation and runner noted.
    fn tell(&self, unit: &mut Unit, world: &mut World, at: usize) {
        self.force(unit, world, at);
        for note in std::mem::take(&mut unit.animation.notes) {
            match note {
                // The routine set ends in the step it starts and starts again in the next one.
                Note::Set(name) if name == "bZooRoutine" => {}
                Note::Set(name) => world.units.lines.push(format!("set {} {name}", unit.id)),
                Note::Anim(name) => world.units.lines.push(format!("anim {} {name}", unit.id)),
                Note::Sound(key) => {
                    let sound = self.data.sound(&world.zoo.entities[at], &key);
                    world.units.sounds.extend(sound.map(|s| (unit.id, s)));
                }
            }
        }
    }

    /// The crossing animation an escape walk asked for (animals 11.3, `forceAnim`) becomes the
    /// unit's base animation, when its type has it.
    fn force(&self, unit: &mut Unit, world: &World, at: usize) {
        let Part::Animal(a) = &mut unit.part else {
            return;
        };
        let name = a.force.take();
        if let Some(name) = name.filter(|n| self.data.timing(&world.zoo.entities[at], n).is_some())
        {
            unit.animation.base_to(name);
            unit.animation.cut();
        }
    }
}

impl Hooks for Standard<'_> {
    fn movement(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        lower_walk(unit);
        let before = movement::here(world, at);
        movement::step(unit, world, ctx.step, at);
        movement::reanimate(unit, world, self.data, at);
        if let Some(new) = movement::here(world, at).filter(|&t| Some(t) != before) {
            crate::animal::escape::entered(unit, world, ctx, new);
            crate::animal::escape::trample(unit, world, ctx, new);
        }
        self.force(unit, world, at);
    }

    fn animate(&mut self, unit: &mut Unit, world: &mut World, _: &mut Ctx, at: usize, ms: u32) {
        let entity = &world.zoo.entities[at];
        unit.animation
            .advance(ms, &|name| self.data.timing(entity, name));
        self.tell(unit, world, at);
        crate::staff::heli::rotor(unit, ms);
    }

    fn behaviour(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        let took = unit.kind == Kind::Animal
            && crate::animal::escape::chose(unit, world, ctx, at, self.data);
        // An egg, a crate or a darted animal chooses nothing (behaviours 6.4): only the set that
        // hatches the egg or shakes the crate runs.
        let held = matches!(&unit.part, Part::Animal(a) if a.egg || a.crated || a.tranq);
        let key = (!took && !held).then(|| ambient_key(unit));
        unit.runner.place = swim::place(unit, world, at);
        swim::nesting(unit, world, at);
        // The terrain mode follows the animal where an ambient set is chosen (tank fauna 9.4).
        let choosing = key.is_some() && !unit.runner.running() && unit.runner.queued().is_none();
        let record = &unit.goal.record;
        unit.runner.talking =
            unit.kind == Kind::Guide && record.task == crate::goal::TALK && !record.done;
        let mut env = Env {
            data: self.data,
            unit: &world.zoo.entities[at],
            rng: ctx.rng,
        };
        if let Some(name) = unit.use_set.take() {
            unit.runner
                .start(&mut unit.animation, &env, &name, false, true, true);
        }
        if unit.kind == Kind::Heli {
            crate::staff::heli::idle_set(unit, &env);
        }
        unit.runner.step(&mut unit.animation, &mut env, key);
        self.building_calls(unit, world, ctx, at);
        if choosing {
            swim::add_to_map(unit, world, at);
        }
        // A move call is the unit's to run (movement 8.2); when it is done the set goes on in
        // the same step.
        lower_walk(unit);
        while let Some(go) = unit.runner.take_move() {
            let flow = movement::call(unit, world, self.data, ctx.cat, ctx.rng, at, &go);
            if flow == Flow::Running {
                break;
            }
            unit.runner.answer(flow);
            let mut env = Env {
                data: self.data,
                unit: &world.zoo.entities[at],
                rng: ctx.rng,
            };
            unit.runner.resume(&mut unit.animation, &mut env);
            self.building_calls(unit, world, ctx, at);
            lower_walk(unit);
        }
        if let Some(to) = unit.sent.take() {
            crate::goal::send(unit, world, ctx, at, self.data, to);
        }
        if unit.runner.take_face() {
            crate::animal::food::face(unit, world, at);
        }
        if unit.runner.take_target() {
            face_target(unit, world, at);
        }
        if unit.runner.take_halt() {
            unit.moving = false;
        }
        if unit.runner.take_die() {
            crate::animal::dirt::die(unit, world, ctx, at);
        }
        if unit.runner.take_hatch() {
            crate::animal::breed::hatch(unit, world, at, self.data);
        }
        for kill in [false, true] {
            if unit.runner.take_meal(kill) {
                crate::goal::meal(unit, world, ctx, at, self.data, kill);
            }
        }
        if unit.runner.take_end_trick() {
            crate::building::leave::post_own(world, unit.id, crate::show::event::FINISHED);
        }
        self.tell(unit, world, at);
    }

    fn typed(&mut self, unit: &mut Unit, world: &mut World, at: usize) {
        movement::read_type(unit, self.data, &world.zoo.entities[at]);
        if unit.kind == Kind::Heli {
            crate::staff::heli::read_type(unit, self.data, &world.zoo.entities[at]);
        }
    }
    fn events(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        crate::guest::reaction::react(unit, world, ctx, at, self.data);
        crate::show::cue(unit, world, ctx, at, self.data);
    }
    fn start(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        crate::animal::start(unit, world, ctx, at);
        swim::add_to_map(unit, world, at);
        crate::staff::start(unit, world, ctx, at);
    }
    fn needs(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        crate::animal::needs(unit, world, ctx, at);
        crate::guest::needs(unit, world, ctx, at);
    }
    fn checks(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        // The checks that skip an animal in a show read it as it is now (shows 18.2, 25).
        refresh_in_show(unit, world, ctx.cat, at);
        crate::animal::checks(unit, world, ctx, at, self.data);
        crate::guest::checks(unit, world, ctx, at, self.data);
        crate::staff::checks(unit, world, ctx, at, self.data);
        // The checks end with the ambient choice, with the happiness of that moment; a darted
        // animal chooses nothing (behaviours 6.4).
        let asked = match &mut unit.part {
            Part::Animal(a) => std::mem::take(&mut a.ambient) && !a.tranq,
            _ => false,
        };
        if asked {
            refresh_in_show(unit, world, ctx.cat, at);
            crate::animal::escape::ambient_sets(unit, world, self.data, ctx.rng, at);
            if !crate::animal::escape::chose(unit, world, ctx, at, self.data) {
                let key = ambient_key(unit);
                let choosing = !unit.runner.running() && unit.runner.queued().is_none();
                unit.runner.place = swim::place(unit, world, at);
                swim::nesting(unit, world, at);
                let mut env = Env {
                    data: self.data,
                    unit: &world.zoo.entities[at],
                    rng: ctx.rng,
                };
                unit.runner.ambient(&mut unit.animation, &mut env, key);
                if choosing {
                    swim::add_to_map(unit, world, at);
                }
            }
            self.tell(unit, world, at);
        }
    }
    fn goals(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        crate::goal::update(unit, world, ctx, at, self.data);
        self.tell(unit, world, at);
    }
}

/// An accepted set start lowers the move handler's started flag and touches nothing else of the
/// walk (movement 8.3): the next move call takes its first-run branch with the path the unit
/// still has.
fn lower_walk(unit: &mut Unit) {
    if unit.runner.take_started() {
        unit.walking = false;
    }
}

/// The animal's `in_show` as it is now, for the checks that read the field (shows 18.2, 32.3).
fn refresh_in_show(unit: &mut Unit, world: &World, cat: &crate::entity::Catalogue, at: usize) {
    if !matches!(unit.part, Part::Animal(_)) {
        return;
    }
    let shown = crate::show::in_show_now(world, cat, unit, at);
    if let Part::Animal(a) = &mut unit.part {
        a.in_show = shown;
    }
}

/// The key a unit's ambient row is chosen by: an animal's happiness, 0 for the others (behaviours
/// 6.2).
fn ambient_key(unit: &Unit) -> i32 {
    match &unit.part {
        Part::Animal(a) => a.happiness,
        _ => 0,
    }
}

/// `fFaceTowardTarget` (behaviours 5.5): a guest or a staff member turns at once to its own target
/// tile, a keeper's own tile (staff 7.4) or, for a guest, the scenery object of its goal. Nothing
/// for a tile that is none; an animal is left as it is.
fn face_target(unit: &Unit, world: &mut World, at: usize) {
    let tile = match &unit.part {
        Part::Staff(s) => s.target,
        Part::Guest(_) => {
            let record = &unit.goal.record;
            let scenery = matches!(record.task, crate::goal::SCENERY | crate::goal::DO_SCENERY);
            world
                .find(record.target.id)
                .filter(|_| scenery)
                .and_then(|i| crate::guest::view::tile_of(world, i))
        }
        Part::Animal(_) => None,
    };
    let width = world.zoo.width as usize;
    if let Some(t) = tile {
        let to = ((t % width) as i32, (t / width) as i32);
        crate::animal::food::turn(unit, world, at, to);
    }
}

/// One turn of the unit at position `at` of the entity list (simulation 6.1); nothing for an
/// entity that is not a unit.
pub(crate) fn update(hooks: &mut impl Hooks, world: &mut World, ctx: &mut Ctx, at: usize) {
    let Some(id) = (world.zoo.entities.get(at)).and_then(|e| e.common.as_ref().map(|c| c.id))
    else {
        return;
    };
    let Some(slot) = world.units.get_mut(id) else {
        return;
    };
    // The unit leaves the table for its turn, so that the hooks can use the world freely.
    let mut unit = std::mem::replace(slot, Unit::new(id, slot.kind));
    turn(hooks, &mut unit, world, ctx, at);
    if let Some(slot) = world.units.get_mut(id) {
        *slot = unit;
    }
}

fn turn(hooks: &mut impl Hooks, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    let step = ctx.step;
    // An animal loaded with the map is started already, but reads its type like the others.
    if !unit.typed {
        unit.typed = true;
        hooks.typed(unit, world, at);
    }
    if !unit.started {
        unit.started = true;
        hooks.start(unit, world, ctx, at);
    }
    unit.events = ctx
        .events
        .listen(unit.id, unit.kind.channel())
        .cloned()
        .collect();
    hooks.events(unit, world, ctx, at);
    if !unit.suspended() {
        hooks.movement(unit, world, ctx, at);
    }
    // A moving unit's animation runs at its speed percentage; it runs even when suspended.
    let advance = match unit.moving {
        true => step.wrapping_mul(unit.anim_speed.into()) / 100,
        false => step,
    };
    hooks.animate(unit, world, ctx, at, advance);
    if !unit.suspended() {
        let period = world.units.period;
        unit.status_timer = unit.status_timer.wrapping_add(step as i32);
        if unit.status_timer > period {
            unit.status_timer = if period < 1 {
                0
            } else {
                unit.status_timer % period
            };
            world.units.lines.push(format!("status {}", unit.id));
            hooks.needs(unit, world, ctx, at);
            hooks.checks(unit, world, ctx, at);
        }
        if unit.wait > 0 {
            unit.wait = unit.wait.wrapping_sub(step as i32);
        } else {
            hooks.behaviour(unit, world, ctx, at);
            hooks.goals(unit, world, ctx, at);
        }
    }
    unit.events.clear();
}

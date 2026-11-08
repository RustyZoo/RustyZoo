//! The pending object under the pointer and its purchase: the tool placed at the pointer, the
//! release that buys it, and the building panels that choose the tool.

use super::*;

/// What the tool places at the pointer, and the drag that buys it.
pub(super) struct Placing {
    /// The pending object: the tool at the place under the pointer, while the pointer (as the
    /// last move left it) is over the map view and a tool is chosen.
    pub(super) pending: Option<Pending>,
    /// The pending staff member made when its item was chosen (staff 4.2): the type and the
    /// counters it drew; the hire gives them to the unit.
    pub(super) made: Option<(usize, crate::unit::Staff)>,
    /// The random use time drawn when the pending building was made (guest buildings 10.1 point
    /// 2.5): the type and the time; the building placed keeps it.
    pub(super) used: Option<(usize, u32)>,
    /// The gate tool's pending gate (`pending` is its object), while it is over a fence piece.
    pub(super) gate: Option<place::gate::Gate>,
    /// The fence piece the gate tool last judged under the pointer (placing 6.4: the pending gate
    /// is made again only when the piece under the pointer changes).
    pub(super) gate_over: Option<u32>,
    /// The pieces of the path or fence drag in progress, the start first (empty without a drag).
    pub(super) run: Vec<Pending>,
    pub(super) hover: bool,
    pub(super) tints: Tints,
}

/// The pending object of type `index` is a building: its random use time is drawn (guest
/// buildings 10.1 point 2.5).
fn pending_use(placing: &mut Placing, sim: &mut Sim, ty: &crate::entity::EntityType, index: usize) {
    if ty.class == "building" {
        placing.used = sim.use_time(ty).map(|time| (index, time));
    }
}

impl App<'_> {
    /// Works out the pending object from the tool, the panel's rotation step and the pointer, and
    /// has the next draw show it. With `log`, a move over the map tells it: `tool pending TYPE
    /// X,Y q N valid`, the tile under the snapped point, the quadrant of the point in it and the
    /// verdict (`valid`, `overwrite`, `highlight` or `invalid`).
    pub(super) fn aim(&mut self, log: bool) {
        self.aim_pending(log);
        self.dozer_aim();
        self.assign_aim();
    }

    /// The pending object part of [`App::aim`].
    pub(super) fn aim_pending(&mut self, log: bool) {
        playing!(self, game, ui);
        let cat = self.renderer.catalogue();
        game.build.set_month(self.sim.calendar().date().month);
        let before = game.placing.pending.take();
        let habits = |e: &Entity| habit(cat, e);
        let dirt = place::dirt(&game.world, game.terrain.types(), &habits);
        let (step, view) = (game.build.step(), game.camera.rotation);
        let october = game.build.october();
        let point = game.camera.pick(&game.world.zoo, ui.pointer());
        let aim = place::unit::Aim {
            point: (point.0, point.1),
            row: i64::from(game.camera.scroll.1) + i64::from(ui.pointer().1),
            zoom: game.camera.zoom,
        };
        let at = |ty, point: (i64, i64)| {
            if cat.types.get(ty).is_some_and(place::unit::is_unit) {
                return place::unit::pending(&game.world, cat, &game.water, ty, view, aim, october);
            }
            place::pending(
                &game.world,
                cat,
                &game.water,
                &dirt,
                game.smart,
                ty,
                step,
                view,
                point,
            )
        };
        game.placing.pending = game
            .build
            .tool()
            .filter(|_| game.placing.hover)
            .and_then(|ty| at(ty, (point.0, point.1)));
        // The unit the player carries follows the pointer with the same checks (8.3).
        // (A unit that was taken from the zoo meanwhile, by an undo, is no longer carried.)
        if (game.carry.as_ref())
            .is_some_and(|c| game.held.is_none() && game.world.find(c.id).is_none())
        {
            game.carry = None;
        }
        if let Some(carry) = game.carry.as_ref().filter(|_| game.placing.hover) {
            let water = &game.water;
            game.placing.pending = match game.held {
                // A building held (8.7) is judged as a new one, facing as the view keeps it.
                Some(_) => place::moved(
                    &game.world,
                    cat,
                    water,
                    &dirt,
                    game.smart,
                    carry.ty,
                    place::unit::turned(carry, view),
                    (point.0, point.1),
                ),
                None => place::unit::carried(&game.world, cat, water, carry, view, aim),
            };
        }
        // The gate tool's pending gate sits on the fence piece under the pointer.
        // It is judged when the piece changes, so a gate just placed shows nothing until the
        // pointer has been over another piece.
        // It is the entity the pixel pick finds, widened as the bulldozer's (placing 6.4).
        let hovered = game
            .staged
            .pick(
                game.camera.view_point(ui.pointer()),
                place::bulldozer::WIDEN,
                |id| game.world.find(id).is_some(),
            )
            .and_then(|id| game.world.find(id));
        let over = hovered
            .and_then(|at| place::gate::over(&game.world, at))
            .filter(|_| game.build.gating() && game.placing.hover);
        if over != game.placing.gate_over || over.is_none() {
            game.placing.gate_over = over;
            game.placing.gate = hovered
                .filter(|_| over.is_some())
                .and_then(|at| place::gate::pending(&game.world, cat, &game.money, at));
        }
        if let Some(g) = &game.placing.gate {
            game.placing.pending = Some(g.pending.clone());
        }
        // A pending fence selects the tile under the pointer, before snapping; a fence drag in
        // progress does not draw it (placing 4.5, 6.2 items 1 and 2).
        let fence = game
            .build
            .tool()
            .and_then(|ty| cat.types.get(ty))
            .is_some_and(|t| Class::of(t) == Class::Fence);
        game.selection = (fence && game.placing.hover && !game.build.dragging())
            .then(|| {
                let (x, y) = (point.0.div_euclid(64), point.1.div_euclid(64));
                let tile = game
                    .world
                    .tile_index(i32::try_from(x).ok()?, i32::try_from(y).ok()?)?;
                Some(Mark {
                    tile,
                    color: crate::terrain::SELECTION,
                })
            })
            .flatten();
        // A drag lays a piece on each tile of its L; the pieces stay while the pointer is away.
        if let (Some(ty), Some(p), true) = (
            game.build.tool(),
            &game.placing.pending,
            game.build.dragging(),
        ) {
            let points = game.build.run(p.tile, (point.0, point.1), point.2);
            let mut run: Vec<Pending> = points.iter().filter_map(|&p| at(ty, p)).collect();
            place::afford(&mut run, &game.money);
            game.placing.run = run;
        } else if !game.build.dragging() {
            game.placing.run.clear();
        }
        if let (true, Some(p)) = (log, &game.placing.pending) {
            let ty = &cat.types[p.ty];
            self.lines.tool.push(format!(
                "tool pending {}/{}/{} {},{} q {} {}",
                ty.class,
                String::from_utf8_lossy(&ty.type_name),
                String::from_utf8_lossy(&ty.subtype),
                p.tile.0,
                p.tile.1,
                p.quadrant,
                p.verdict.word(),
            ));
        }
        // The cost text follows the pointer: the frame is painted again.
        if before.is_some() || game.placing.pending.is_some() {
            ui.touch();
        }
    }

    /// A release on the map (the view sees only one that ends a press on it): the pieces of a path
    /// drag, or else the pending object, are bought in order. A valid place gets its object, which
    /// is paid and logged (`world add objects/foliage/doumpalm 12,40 id 14908 quadrants 1`, with
    /// the number of quadrants the object holds; a path on water
    /// logs `world paint 12,40 3` first; a fence is told with its side, `world add fences/...
    /// 12,40 side 2 id 8`; a piece that takes the place of another keeps its id when it is a path
    /// or a fence of the same type, `world retype paths/... 12,40 id 7`, and otherwise logs the
    /// old one first, `world remove fences/... 12,40 side 2 id 7`).
    /// One sound is asked for the release: the placement sound if anything was placed, else the
    /// refusal sound; a type that turns by itself turns once; the last message of a piece that
    /// could not be placed shows in the message line (`tool message 10311`), and one that gives no
    /// message when nothing at all was placed logs `tool refused`; and a new pending
    /// object is made (placing 6.1 and 6.2).
    pub(super) fn release(&mut self) {
        if let Some(id) = self.assigning() {
            return self.assign_release(id);
        }
        playing!(self, game);
        if game.build.bulldozing() {
            return self.bulldoze_release();
        }
        if game.build.gating() {
            return self.gate_release();
        }
        if game.carry.is_some() {
            return self.drop_carried();
        }
        let cat = self.renderer.catalogue();
        if (game.build.tool().and_then(|t| cat.types.get(t))).is_some_and(place::unit::is_unit) {
            return self.adopt();
        }
        if game.build.tool().is_none() && game.terrain.mode().is_none() {
            return match self.pick_or_select() {
                true => (),
                false => self.select(),
            };
        }
        let dragging = game.build.dragging();
        game.build.end_drag();
        let run = std::mem::take(&mut game.placing.run);
        let pieces = match dragging && !run.is_empty() {
            true => run,
            false => game.placing.pending.clone().into_iter().collect(),
        };
        self.buy(pieces, false);
    }

    /// Buys the pieces in order (`release`); `resumed` when they are what a closing piece left
    /// of the drag, whose sound and turn were done. A piece that makes an exhibit stops the
    /// buying: its name is asked first, the rest of the pieces come after the answer.
    pub(super) fn buy(&mut self, pieces: Vec<Pending>, resumed: bool) {
        playing!(self, game, ui);
        if pieces.is_empty() {
            return;
        }
        game.world
            .set_exhibit_words([0, 1, 2].map(|k| ui.string(exhibits::WORD + k)));
        let renderer = &self.renderer;
        let cat = renderer.catalogue();
        let (mut first, mut refused) = (None, None);
        for (n, p) in pieces.iter().enumerate() {
            let ty = &cat.types[p.ty];
            let name = format!(
                "{}/{}/{}",
                ty.class,
                String::from_utf8_lossy(&ty.type_name),
                String::from_utf8_lossy(&ty.subtype)
            );
            let (x, y) = p.tile;
            let fence = Class::of(ty) == Class::Fence;
            let at = game.world.tile_index(x, y);
            let old = match ty.class {
                "fences" | "tankwall" => at.and_then(|t| game.world.edge(t, p.facing as usize)),
                "paths" => at.and_then(|t| game.world.path(t)),
                _ => None,
            };
            let old = old.and_then(|o| game.world.zoo.entities[o].common.as_ref().map(|c| c.id));
            // A fence is told with its side.
            let tile = match fence {
                true => format!("{x},{y} side {}", p.facing),
                false => format!("{x},{y}"),
            };
            let water = &game.water;
            let colors = renderer.default_colors(ty);
            // The animals of the exhibit react to an object placed (habitat rating 9.4); a fence
            // piece runs the pair too and decides nothing by its value.
            let placed = p.verdict != place::Verdict::Invalid && game.money.can_pay(p.price);
            let mut before = match placed {
                false => Vec::new(),
                true => {
                    let rand = &mut || self.sim.rng.rand();
                    rating::entity(&mut game.world, cat, at, rating::reads_neighbours(ty), rand)
                }
            };
            let outcome = place::place(
                &mut game.world,
                &mut game.money,
                ty,
                p,
                water,
                colors,
                |e| renderer.shape(e),
            );
            if let Outcome::Placed(id) = outcome {
                crate::building::built(&mut game.world, ty, id, self.sim.calendar().elapsed_ms());
                let kept = game.placing.used.take_if(|u| u.0 == p.ty).map(|u| u.1);
                self.sim.keep_use(&mut game.world, cat, id, kept);
                crate::show::added(&mut game.world, cat, id);
                let (habitat, name) = rating::object(ty);
                let change = (!fence).then_some(rating::Change::Object {
                    habitat,
                    name,
                    sign: 1,
                });
                let rand = &mut || self.sim.rng.rand();
                rating::again(&game.world, &mut before, at, rating::reads_neighbours(ty));
                rating::react(&mut game.world, cat, before, change, rand);
                // The click makes the next pending object of the type (it draws its time).
                pending_use(&mut game.placing, &mut self.sim, ty, p.ty);
            }
            match outcome {
                Outcome::Placed(id) => {
                    // A new or replaced fence piece is healthy and draws its wear interval.
                    if fence {
                        game.world.heal(cat, id, &mut || self.sim.rng.rand());
                    }
                    if let Some(kind) = p.paint {
                        self.lines.world.push(format!("paint {x},{y} {kind}"));
                    }
                    // A piece that keeps its id only takes the new type, healthy: it draws its start.
                    if old == Some(id) {
                        self.sim.draw_start(&game.world, id);
                        let (sim, r) = (&mut self.sim, &self.renderer);
                        exhibits::retyped(sim, &mut game.world, (r, &self.timings), id);
                        self.lines
                            .world
                            .push(format!("retype {name} {tile} id {id}"));
                        first = first.or(Some(p.ty));
                        continue;
                    }
                    if let Some(old) = old {
                        let line = format!("remove {name} {tile} id {old}");
                        self.lines.world.push(line);
                    }
                    // An object that takes quadrants says how many it holds.
                    let held = match Class::of(ty) == Class::Fixed {
                        true => game.world.find(id).map(|e| game.world.held(e)),
                        false => None,
                    };
                    let held = held.map_or(String::new(), |n| format!(" quadrants {n}"));
                    let line = format!("add {name} {tile} id {id}{held}");
                    self.lines.world.push(line);
                    // The DRT base makes its helicopter (unit placing 7.4).
                    if ty.class == "building" {
                        let built = crate::timing::file_time(self.sim.calendar().elapsed_ms());
                        let (rng, shape) = (&mut self.sim.rng, |e: &_| renderer.shape(e));
                        let made = crate::building::birth::births(
                            &mut game.world,
                            cat,
                            id,
                            rng,
                            built,
                            shape,
                        );
                        for heli in made.into_iter().filter_map(|h| game.world.find(h)) {
                            let e = &game.world.zoo.entities[heli];
                            let c = e.common.as_ref().expect("added");
                            self.lines.world.push(format!(
                                "add {}/{}/{} {},{} id {} name={}",
                                e.class,
                                e.type_name,
                                e.subtype,
                                c.x.div_euclid(64),
                                c.y.div_euclid(64),
                                c.id,
                                c.name
                            ));
                        }
                    }
                    first = first.or(Some(p.ty));
                    if !game.world.waiting().is_empty() {
                        game.naming.rest = pieces[n + 1..].to_vec();
                        break;
                    }
                }
                Outcome::Refused(message) => refused = message.or(refused),
            }
        }
        match first {
            _ if resumed => {}
            Some(ty) => {
                self.ui_sounds.push("place");
                if game.build.placed(&cat.types[ty]) {
                    self.lines
                        .tool
                        .push(format!("tool turn {}", game.build.step()));
                }
            }
            None => self.ui_sounds.push("noplace"),
        }
        match refused {
            Some(id) => {
                ui.message(&to_cp1252(&ui.string(id)), kinds::REFUSAL);
                self.lines.tool.push(format!("tool message {id}"));
            }
            // A refusal without a message (ground that is not flat, a quadrant off the zoo).
            None if first.is_none() => self.lines.tool.push("tool refused".into()),
            None => {}
        }
        self.exhibits_told();
        self.aim(false);
    }

    /// The building panels notice what the player did to them: the panel shown or hidden, and
    /// the button `selected`, if any.
    pub(super) fn build(&mut self, selected: Option<i32>) {
        playing!(self, game, ui);
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        let told = self.lines.tool.len();
        game.build.set_month(self.sim.calendar().date().month);
        game.build.set_hire(self.sim.hire_male);
        game.build.sync(ui, vfs, cat, &mut self.lines.tool);
        if let Some(id) = selected {
            game.build.selected(ui, vfs, cat, id, &mut self.lines.tool);
            self.terrain_pressed(id);
        }
        self.terrain_sync();
        self.make_pending();
        // Every change of the tool or the step tells a line. Choosing an item ends a move (8.6).
        if self.lines.tool.len() != told {
            let b = self.game.as_ref().map(|g| &g.build);
            let tool = b.is_some_and(|b| b.tool().is_some() || b.bulldozing() || b.gating());
            if tool && self.game.as_ref().is_some_and(|g| g.carry.is_some()) {
                return self.put_back();
            }
            self.aim(false);
        }
    }

    /// Makes the pending staff member of each staff item chosen since the last call: its two
    /// counters are drawn now (staff 4.2), the last one made is the pending one.
    pub(super) fn make_pending(&mut self) {
        playing!(self, game);
        let cat = self.renderer.catalogue();
        for ty in game.build.take_made() {
            let t = &cat.types[ty];
            pending_use(&mut game.placing, &mut self.sim, t, ty);
            if let Some(kind) = crate::unit::Kind::of_class(t.class) {
                game.placing.made =
                    Some((ty, crate::staff::create(Some(t), kind, &mut self.sim.rng)));
            }
        }
    }
}

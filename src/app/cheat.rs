//! The name hook (cheats 2): the text of a name set on an animal, a guest, a staff member or an
//! exhibit is offered to the cheats before the name is stored.

use super::App;
use super::cheat_guest::palette_set;
use crate::behaviour::Data;
use crate::cheat::{Cheat, Named, Season, Subject, WITCH};
use crate::entity::Catalogue;
use crate::flight;
use crate::sim::Sim;
use crate::text::from_cp1252;
use crate::world::World;

impl App<'_> {
    /// Runs the cheat whose text is exactly `text` in `class`, if there is one.
    pub(super) fn name_hook(&mut self, class: Subject, text: &str) {
        self.hook(class, &Named::default(), None, text);
    }

    /// As [`App::name_hook`] for the entity `who` that was named, whose type the conditions of
    /// the rows look at.
    fn hook(&mut self, class: Subject, named: &Named, who: Option<u32>, text: &str) {
        match self.cheats.find_named(class, named, text) {
            Some(Cheat::ResearchCategory(name)) => self.complete_category(name),
            Some(cheat) => {
                self.guest_cheat(cheat);
                self.exhibit_and_staff_cheat(cheat, who);
            }
            None => {}
        }
    }

    /// The text in the name field `id` as typed, before the visible-character test.
    pub(super) fn typed_name(&self, id: i32) -> Option<String> {
        let bytes = self.ui.as_ref()?.text(id)?;
        Some(bytes.iter().map(|&b| from_cp1252(b)).collect())
    }

    /// The name hook of the entity `selected` whose name field `id` was just edited, in the class
    /// of the entity (`fallback` when there is none). A game must be open.
    pub(super) fn entity_hook(&mut self, fallback: Subject, id: i32, selected: Option<u32>) {
        let Some(text) = self.typed_name(id).filter(|_| self.game.is_some()) else {
            return;
        };
        let cat = self.renderer.catalogue();
        let found = self.game.as_ref().and_then(|g| {
            let e = &g.world.zoo.entities[g.world.find(selected?)?];
            let ty = crate::place::bulldozer::type_of(cat, e);
            let first = |s: &str| s.bytes().next().unwrap_or(0);
            let (class, subtype) = match e.class.as_str() {
                "animals" => (Subject::Animal, first(&e.subtype)),
                // A guest's sex is the first letter of its type's name.
                "guests" => (Subject::Guest, first(&e.type_name)),
                "keeper" => (Subject::Keeper, first(&e.subtype)),
                "maint" => (Subject::Worker, first(&e.subtype)),
                "tour" => (Subject::Guide, first(&e.subtype)),
                _ => return None,
            };
            let name_id = ty.map_or(0, |t| t.name_id);
            Some((class, Named { subtype, name_id }))
        });
        let (class, named) = found.unwrap_or((fallback, Named::default()));
        self.hook(class, &named, selected, &text);
    }

    /// The rest of the hook of a guest created this step (cheats 6): the seasonal items of the
    /// game date and the yellow brick path. The flying objects went with the creation itself
    /// ([`guest_flights`]).
    pub(super) fn guest_created(&mut self) {
        let date = self.sim.calendar().date();
        let Some(game) = &self.game else { return };
        let trio = game.world.units.cheat.lions_tigers_bears;
        let set = self.cheats.season.guest_created(date.month, date.day, trio);
        self.make_available(&set);
    }

    /// The start hook (cheats 6.3): the seasonal items and the yellow brick path are unavailable
    /// until a hook makes them available. On the computer's 31 October (UTC) the permanent witch
    /// comes once per run, every guest and every recolourable building takes the Halloween colours
    /// and the jack-o'-lantern is available again.
    pub(super) fn game_started(&mut self) {
        let real = (self.sim.real_day)() == (10, 31);
        let cat = self.renderer.catalogue();
        let Some(game) = &mut self.game else { return };
        let (set, fly) = self.cheats.season.game_started(real);
        if let Some(fly) = fly {
            self.sim.note(fly.line());
            // The permanent witch makes no creature: the witch type spawns every 30 s (ambients 7.4).
            self.sim.spawns.add(WITCH);
            self.sim.spawns.period(WITCH, 30);
        }
        if real {
            palette_set(&mut game.world, cat, [10, 2, 11], [7, 5]);
        }
        self.make_available(&set);
    }

    /// Sets the availability of the types of each name id, and makes the open list again.
    pub(super) fn make_available(&mut self, set: &[(i32, bool)]) {
        playing!(self, game);
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        match &mut self.ui {
            Some(ui) => {
                let mut log = Vec::new();
                for &(id, on) in set {
                    game.build.set_available(ui, vfs, cat, id, on, &mut log);
                }
                log.into_iter().for_each(|line| self.sim.note(line));
            }
            None => game.build.start_research(cat, set),
        }
    }
}

/// The flying objects of a guest created on the game date (cheats 6.2 steps 1 to 3, ambients 3.1),
/// run by the guest creation of the step: each is told in the log and made, so that it moves in
/// this very step and its three draws come before those of the entities. The sleigh and the
/// biplane come back with their loop (flight, file, base term) for the game to hold. A game
/// without the type makes no flight and says so in the log (ambients 11.1).
pub(super) fn guest_flights(
    season: &mut Season,
    data: &dyn Data,
    cat: &Catalogue,
    sim: &mut Sim,
    world: &mut World,
) -> Vec<(u32, &'static str, i32)> {
    let date = sim.calendar().date();
    let mut loops = Vec::new();
    for fly in season.flights(date.month, date.day) {
        sim.note(fly.line());
        let Some(id) = fly.type_id() else { continue };
        let Some(ty) = flight::ambient(cat, id) else {
            sim.note(format!("flight {id} missing"));
            continue;
        };
        sim.spawns.add(id);
        let made = flight::launch(world, cat, ty, data.set_files(ty), &mut sim.rng);
        if let (Some(flight), Some((file, base))) = (made, fly.loop_sound()) {
            loops.push((flight, file, base));
        }
    }
    loops
}

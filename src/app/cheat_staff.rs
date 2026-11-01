//! The effects of the exhibit, animal, maintenance worker and keeper name cheats (cheats 3, 4.1,
//! 4.4, 4.5).

use super::App;
use crate::build::{number, plain};
use crate::cheat::{Cheat, GRIZZLY};
use crate::entity::Catalogue;
use crate::money::CONSTRUCTION_REFUND;
use crate::panel::exhibit::animals_exhibit;
use crate::place::bulldozer::type_of;
use crate::rubble::{Kit, PIECE_NAME, turn_to_rubble};
use crate::world::World;
use crate::zoo::Entity;

/// What the foliage cheat books, as a construction refund, and adds to the cash.
const FOLIAGE_BONUS: f32 = 300.0;
/// The `cExpansionID` of Dinosaur Digs.
const DIGS: f32 = 1.0;

impl App<'_> {
    /// Runs a cheat of the exhibit, animal, worker and keeper rows; `who` is the entity that was
    /// named, if any.
    pub(super) fn exhibit_and_staff_cheat(&mut self, cheat: Cheat, who: Option<u32>) {
        let flags = &mut self.cheats.flags;
        match cheat {
            Cheat::RatingBonus => flags.rating_bonus = true,
            Cheat::DonationDouble => flags.donation_double = true,
            Cheat::ShopIncomeDouble => flags.shop_income_double = true,
            Cheat::BreedingBoost => flags.breeding_boost = true,
            Cheat::MarketingDouble => flags.marketing_double = true,
            Cheat::HealChangesSpecies => flags.heal_changes_species = true,
            Cheat::KeeperSpeedDouble => {
                flags.keeper_speed_double = true;
                self.double_speed(who);
            }
            Cheat::BearsBreakFences => {
                flags.bears_bash = true;
                self.bears_escape_check();
            }
            Cheat::Unlock(name) => self.unlock(name),
            Cheat::DinosaursVanish => self.dinosaurs_vanish(),
            Cheat::FoliageClearBonus => self.foliage_clear(),
            Cheat::FencesRepair => self.fences_repair(),
            Cheat::BuildingsToRubble => self.buildings_to_rubble(),
            _ => {}
        }
        // The rules the world reads see the flags at once, not at the next step.
        if let Some(game) = &mut self.game {
            game.world.units.cheat = self.cheats.flags;
        }
    }

    /// Makes the types with this name id available, as a research program does.
    fn unlock(&mut self, name: i32) {
        let Some(game) = &mut self.game else { return };
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        game.build.start_research(cat, &[(name, true)]);
        if let Some(ui) = &mut self.ui {
            let mut log = Vec::new();
            game.build.refill(true, ui, vfs, cat, &mut log);
            log.into_iter().for_each(|line| self.sim.note(line));
        }
    }

    /// The named keeper's own speed, doubled at once.
    fn double_speed(&mut self, who: Option<u32>) {
        let unit = who.and_then(|id| self.game.as_mut()?.world.units.get_mut(id));
        if let Some(unit) = unit {
            unit.speed = unit.speed.saturating_mul(2);
        }
    }

    /// Every exhibit with a grizzly bear is rated again and gets its escape check flag, so that
    /// the next check lets the bears out.
    fn bears_escape_check(&mut self) {
        let cat = self.renderer.catalogue();
        if let Some(game) = &mut self.game {
            bears_look(&mut game.world, cat);
        }
    }

    /// Every animal of Dinosaur Digs goes at once: no refund, no question, no message.
    fn dinosaurs_vanish(&mut self) {
        let Some(game) = &mut self.game else { return };
        let cat = self.renderer.catalogue();
        let digs = |e: &Entity| {
            e.class == "animals"
                && type_of(cat, e).is_some_and(|t| plain(t, "cExpansionID") == Some(DIGS))
        };
        let zoo = &game.world.zoo;
        let gone: Vec<u32> = zoo
            .entities
            .iter()
            .filter(|e| digs(e))
            .filter_map(|e| e.common.as_ref().map(|c| c.id))
            .collect();
        for id in gone {
            game.world.forget_undo(id);
            game.world.unrecorded(|w| w.remove(id));
        }
    }

    /// Every foliage item is taken out with no refund and no question; then the bonus is booked
    /// as a construction refund and goes into the cash, even when no foliage stood.
    fn foliage_clear(&mut self) {
        let Some(game) = &mut self.game else { return };
        let cat = self.renderer.catalogue();
        let foliage = |e: &Entity| {
            e.class == "objects"
                && type_of(cat, e).is_some_and(|t| number(t, "cFoliage") == Some(1.0))
        };
        let zoo = &game.world.zoo;
        let ids: Vec<u32> = zoo
            .entities
            .iter()
            .filter(|e| foliage(e))
            .filter_map(|e| e.common.as_ref().map(|c| c.id))
            .collect();
        for id in ids {
            game.world.unrecorded(|w| w.remove(id));
        }
        game.money.refund(CONSTRUCTION_REFUND, FOLIAGE_BONUS);
    }

    /// Every rubbleable building turns to rubble (cheats 4.4): no refund, no message.
    fn buildings_to_rubble(&mut self) {
        let Some(game) = &mut self.game else { return };
        let template = (self.ui.as_ref()).map_or(String::new(), |u| u.string(PIECE_NAME));
        let renderer = &self.renderer;
        let shape = |e: &Entity| renderer.shape(e);
        let mut kit = Kit {
            cat: renderer.catalogue(),
            rng: &mut self.sim.rng,
            template: &template,
            shape: &shape,
        };
        rubble_buildings(&mut game.world, &mut kit);
    }

    /// Every fence piece that is broken or at or below its decayed life is made healthy.
    fn fences_repair(&mut self) {
        let Some(game) = &mut self.game else { return };
        let cat = self.renderer.catalogue();
        self.sim.repair_fences(&mut game.world, cat);
    }
}

/// Every building whose type has `cRubbleable` 1 (the default) turns to rubble, each once.
pub(super) fn rubble_buildings(world: &mut World, kit: &mut Kit) {
    let rubbleable = |e: &&Entity| {
        e.class == "building"
            && type_of(kit.cat, e).is_some_and(|t| number(t, "cRubbleable").unwrap_or(1.0) == 1.0)
    };
    let ids: Vec<u32> = (world.zoo.entities.iter())
        .filter(rubbleable)
        .filter_map(|e| Some(e.common.as_ref()?.id))
        .collect();
    for id in ids {
        turn_to_rubble(world, kit, id);
    }
}

/// Every exhibit with a grizzly bear is rated again and gets its escape check flag.
pub(super) fn bears_look(world: &mut World, cat: &Catalogue) {
    let bear =
        |e: &Entity| e.class == "animals" && type_of(cat, e).is_some_and(|t| t.name_id == GRIZZLY);
    let homes: Vec<u32> = (world.zoo.entities.iter().enumerate())
        .filter(|(_, e)| bear(e))
        .filter_map(|(at, _)| animals_exhibit(world, at))
        .collect();
    for x in homes {
        world.exhibits_mut().mark(x);
        if let Some(e) = world.exhibits_mut().get_mut(x) {
            e.rating.escape = true;
        }
    }
}

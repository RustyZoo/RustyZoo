//! The zoo status of a step, run by the game manager (zoo status 4).

use super::{Sim, guest};
use crate::entity::Catalogue;
use crate::money::Money;
use crate::status::zoo::Sums;
use crate::world::World;

impl Sim {
    /// The zoo status of a step: the due checks in the original's order, the donation and the
    /// bars. The generator is seeded again inside the finance check, after the other checks.
    /// `created` runs inside the guest creation, once the guest is in the world (ambients 3.1).
    pub(super) fn zoo_status(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        money: &mut Money,
        step: u32,
        month_end: bool,
        created: &mut dyn FnMut(&mut Sim, &mut World),
    ) -> Option<[i32; 3]> {
        let due = self.status.due(step, month_end);
        if due.rating {
            let sums = Sums::of(world, cat, money.cash, self.research_share());
            self.status
                .rating_check(sums, self.calendar.elapsed_ms(), money, &mut self.lines);
        }
        if due.message {
            let (say, log) = (&mut self.messages, &mut self.lines);
            self.status.message_check(money.cash, say, log);
        }
        if due.new_guest {
            let occupied = guest::occupied(world, cat);
            // The effective rating: a cheat adds to it (zoo status 8).
            let bonus = world.units.cheat.rating_points(self.marketing.benefit());
            if let Some(advice) = self.status.new_guest_check(occupied, bonus, &mut self.rng) {
                let s = &self.status;
                let arrival = guest::Arrival {
                    price: s.price,
                    multiple: s.settings.admission_multiple,
                    guests: s.sums.guests,
                    max_guests: self.ai.max_guests,
                    date: self.calendar.date(),
                    reset: self.ai.guest_reset_interval,
                    real_halloween: (self.real_day)() == (10, 31),
                };
                if guest::arrive(world, cat, &mut self.rng, money, &arrival) {
                    self.arrived = true;
                    created(self, world);
                    if advice {
                        self.status.advise(&mut self.messages, &mut self.lines);
                    }
                }
            }
        }
        if month_end {
            self.reseed();
            let date = self.calendar.date();
            self.lines
                .push(format!("month {} {}", date.month, date.year));
            // The endowment goes before the accounts move on, the wages and the upkeep after, in
            // the new month's column.
            self.status.endowment(&mut self.rng, money, &mut self.lines);
            money.month_change();
            world.exhibits_mut().enter_new_month();
            crate::show::new_month(world, cat);
            crate::month::charge(world, cat, money, &mut self.lines);
        }
        let (say, log) = (&mut self.messages, &mut self.lines);
        self.status.donation(&mut self.rng, money, say, log);
        self.status.bars(step)
    }
}

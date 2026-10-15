//! The zoo status of the game manager (zoo status): the settings of the economy file, the sums, the
//! zoo rating, the advice messages, the month-end finance check, the donation when the cash is
//! low, the adult admission price and the three bars of the status bar.
//!
//! [`Status`] keeps the three timers, the last sums and the rating; the [`crate::sim::Sim`] step
//! calls its checks in the original's order. Missing so far: guests (their state is the guest
//! issue's, so their counts stay 0), research (its share is 0), the new guest check, the
//! escapes, wages and building upkeep (the staff and building issues), the scenario's own
//! economy file.
//!
//! The log lines: `rating N` at each rating check, `message ID` for each advice message and
//! `donation AMOUNT` for each donation.

use crate::build::{cost, number};
use crate::config::{Config, to_float, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::rating;
use crate::money::{self, Money};
use crate::sim::Rng;
use crate::timing::ini_value;
use crate::unit::Part;
use crate::world::World;
use crate::zoo::{Entity, StatusRecord};
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;

/// The classes whose entities are objects of the zoo: they count for the zoo value and the
/// esthetics.
const OBJECTS: [&str; 8] = [
    "objects", "building", "fences", "tankwall", "paths", "food", "rubble", "filter",
];
/// The guest type ids whose esthetic bonus is summed: man, woman, boy, girl.
const GUESTS: [i32; 4] = [9503, 9504, 9505, 9506];
/// What a broken fence and a decayed one take off the bonus of a guest type.
const BROKEN: f32 = 45.0;
const DECAYED: f32 = 11.0;
/// An advice message is posted only above this many guests.
const MANY_GUESTS: i32 = 10;
/// The cash below which the money messages and the donation start (the original compares whole
/// units for the messages, a decimal for the donation).
const LOW_CASH: i32 = 1000;
const GAME_HOUR_MS: i64 = 3_600_000;
const GAME_DAY_MS: i64 = 24 * GAME_HOUR_MS;
/// The status bar is rewritten when its counter passes this many milliseconds.
const BAR_PERIOD: u32 = 1001;

/// The settings of the economy file with the built-in values (zoo status 3.2). Only the keys this
/// module uses are kept.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Periods of the three checks, in milliseconds.
    pub rating: i32,
    pub message: i32,
    pub new_guest: i32,
    pub species_available: i32,
    /// The fractions of the animals or guests that make an advice message, in the order
    /// protestors, sick, hungry, thirsty, restroom, gift, tired, litter.
    pub percent: [f32; 8],
    pub high_guest_happiness: i32,
    pub low_guest_happiness: i32,
    pub donations_available: i32,
    pub donation_low: i32,
    pub donation_high: i32,
    pub donation_chance: i32,
    pub membership_join_factor: i32,
    pub endowment_low: i32,
    pub endowment_high: i32,
    pub price_min: f32,
    pub price_max: f32,
}

/// The keys of [`Settings::percent`].
const PERCENT_KEYS: [&str; 8] = [
    "cPctProtestors",
    "cPctSick",
    "cPctHungry",
    "cPctThirsty",
    "cPctBathroom",
    "cPctSouvenir",
    "cPctTired",
    "cPctTrash",
];

impl Settings {
    /// The values the program has before it reads a file.
    pub fn builtin() -> Settings {
        Settings {
            rating: 10_000,
            message: 10_000,
            new_guest: 10_000,
            species_available: 44,
            percent: [0.5; 8],
            high_guest_happiness: 90,
            low_guest_happiness: 0,
            donations_available: 3,
            donation_low: 10_000,
            donation_high: 20_000,
            donation_chance: 0,
            membership_join_factor: 10,
            endowment_low: 10_000,
            endowment_high: 20_000,
            price_min: 0.0,
            price_max: 100.0,
        }
    }

    /// Reads an economy file over the settings: a key that is missing leaves its value, names
    /// ignore case, numbers are read as the game's files are. The two prices are swapped when
    /// the maximum is below the minimum.
    pub fn apply(&mut self, file: &Config) {
        let int = |section: &str, key: &str, value: &mut i32| {
            if let Some(n) = ini_value(file, section, key).and_then(to_int) {
                *value = n;
            }
        };
        let decimal = |key: &str, value: &mut f32| {
            if let Some(v) = ini_value(file, "characteristics", key) {
                *value = to_float(v);
            }
        };
        int("checks", "rating", &mut self.rating);
        int("checks", "message", &mut self.message);
        int("checks", "newguest", &mut self.new_guest);
        let c = "characteristics";
        int(c, "cSpeciesAvailable", &mut self.species_available);
        for (key, value) in PERCENT_KEYS.iter().zip(&mut self.percent) {
            decimal(key, value);
        }
        int(
            c,
            "cHighAvgGuestHappyThreshold",
            &mut self.high_guest_happiness,
        );
        int(
            c,
            "cLowAvgGuestHappyThreshold",
            &mut self.low_guest_happiness,
        );
        int(c, "cDonationsAvail", &mut self.donations_available);
        int(c, "cDonationLow", &mut self.donation_low);
        int(c, "cDonationHigh", &mut self.donation_high);
        int(c, "cDonationChance", &mut self.donation_chance);
        int(c, "cMembershipJoinFactor", &mut self.membership_join_factor);
        int(c, "cEndowmentGiftLow", &mut self.endowment_low);
        int(c, "cEndowmentGiftHigh", &mut self.endowment_high);
        decimal("cMinAdultAdmissionPrice", &mut self.price_min);
        decimal("cMaxAdultAdmissionPrice", &mut self.price_max);
        if self.price_max < self.price_min {
            std::mem::swap(&mut self.price_min, &mut self.price_max);
        }
    }
}

/// What the rating check counts (zoo status 5).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sums {
    pub animals: i32,
    pub sick: i32,
    pub animal_happiness: i32,
    pub guests: i32,
    pub guest_happiness: i32,
    /// The guests whose hunger, thirst, restroom need or tiredness is above the limit, who want a
    /// gift, who hold litter.
    pub hungry: i32,
    pub thirsty: i32,
    pub restroom: i32,
    pub tired: i32,
    pub gift: i32,
    pub litter: i32,
    pub species: i32,
    pub zoo_value: i64,
    pub esthetics: f32,
    /// The share of research programs completed, in percent.
    pub research: i32,
}

/// The bonus of the type for the four guest types: its `[EstheticBonus]` list holds a guest id,
/// then a bonus, and so on; an id it lacks gives 0.
fn bonus(ty: &EntityType) -> [f32; 4] {
    let list: Vec<i32> = (ty.config.as_deref())
        .and_then(|c| c.section("EstheticBonus"))
        .map(|s| s.get_all("v").iter().filter_map(|v| to_int(v)).collect())
        .unwrap_or_default();
    GUESTS.map(|id| {
        let at = list.chunks(2).find(|p| p[0] == id);
        at.and_then(|p| p.get(1)).map_or(0.0, |&b| b as f32)
    })
}

impl Sums {
    /// The sums over the entities of `world` with `cash` in hand and `research` percent of the
    /// research done. The animals' state is read from the units; the guests' is not there yet.
    pub fn of(world: &World, catalogue: &Catalogue, cash: f32, research: i32) -> Sums {
        let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
        let mut sums = Sums {
            zoo_value: cash as i64,
            research,
            ..Sums::default()
        };
        let mut esthetic = 0.0f32;
        for (at, e) in world.zoo.entities.iter().enumerate() {
            let animal = e.class == "animals";
            if !animal && !OBJECTS.contains(&e.class.as_str()) {
                continue;
            }
            let ty = catalogue.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype));
            sums.zoo_value += ty.map_or(0, |ty| cost(ty) as i64);
            if animal {
                sums.animals += 1;
                let part = e.common.as_ref().and_then(|c| world.units.get(c.id));
                if let Some(Part::Animal(a)) = part.map(|u| &u.part) {
                    sums.animal_happiness = sums.animal_happiness.wrapping_add(a.happiness);
                    sums.sick += i32::from(a.at_minimum_health);
                }
            } else if let Some(ty) = ty {
                let wall = world.shape(at).is_some_and(|s| s.wall);
                esthetic += bonus(ty).map(|b| piece(ty, e, wall, b)).iter().sum::<f32>();
            }
        }
        let tiles = &world.zoo.tiles;
        let occupied = (0..tiles.len())
            .filter(|&t| (0..4).any(|q| world.quadrant(t, q).is_some()))
            .count();
        sums.esthetics = if occupied > 0 {
            esthetic / occupied as f32
        } else {
            esthetic
        };
        // The species of every exhibit as its tiles hold them now (a list not yet revised is
        // revised first).
        let keys: BTreeSet<String> = (world.exhibits().list().iter())
            .flat_map(|x| rating::input(world, catalogue, x.id).species)
            .map(|l| l.key)
            .collect();
        sums.species = keys.len() as i32;
        sums
    }

    /// `numerator / count` as a whole number, 0 with no count.
    fn average(sum: i32, count: i32) -> i32 {
        if count == 0 { 0 } else { sum / count }
    }

    pub fn animal_average(&self) -> i32 {
        Sums::average(self.animal_happiness, self.animals)
    }

    pub fn guest_average(&self) -> i32 {
        Sums::average(self.guest_happiness, self.guests)
    }
}

/// The bonus `b` of a piece of the type for a guest type: a fence takes nothing when it is a
/// member of the zoo wall, and loses [`BROKEN`] when it is broken and [`DECAYED`] when its life
/// is at or below the `cDecayedLife` of its type.
fn piece(ty: &EntityType, e: &Entity, wall: bool, b: f32) -> f32 {
    let Some(life) = (matches!(e.class.as_str(), "fences" | "tankwall"))
        .then(|| e.common.as_ref()?.fence.as_ref().map(|f| f.life))
        .flatten()
    else {
        return b;
    };
    if wall {
        return 0.0;
    }
    let decayed = number(ty, "cDecayedLife").unwrap_or(0.0);
    b - if life < 1 { BROKEN } else { 0.0 } - if life as f32 <= decayed { DECAYED } else { 0.0 }
}

/// The zoo status: timers, the last sums, the rating and the counters the files save.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub settings: Settings,
    /// Milliseconds since the rating, message and new guest checks.
    pub timers: [u32; 3],
    pub bar_ms: u32,
    pub sums: Sums,
    pub rating: i32,
    /// How many times the cash fell low enough for a donation.
    pub given: f32,
    pub price: f32,
    /// The game time of the last animal escape, in milliseconds; a year before the game began.
    pub escape_ms: i64,
}

impl Default for Status {
    fn default() -> Status {
        Status {
            settings: Settings::builtin(),
            timers: [0; 3],
            bar_ms: 0,
            sums: Sums::default(),
            rating: 0,
            given: 0.0,
            price: 49.0,
            escape_ms: -366 * GAME_DAY_MS,
        }
    }
}

/// The checks a step runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Due {
    pub rating: bool,
    pub message: bool,
    pub new_guest: bool,
}

/// What a check says: the text number and, for a donation, its amount in whole units.
pub type Say = (u32, Option<i32>);

impl Status {
    /// The status of a game on a file that stores `record`, with the built-in settings and then
    /// the `economy` file over them (3.1).
    pub fn start(record: &StatusRecord, economy: Option<&Config>) -> Status {
        let mut status = Status {
            timers: record.timers,
            rating: record.rating as i32,
            given: record.given,
            ..Status::default()
        };
        if let Some(file) = economy {
            status.settings.apply(file);
        }
        status.price = status.limit(record.price);
        status
    }

    /// The price held between the limits (11).
    pub fn limit(&self, price: f32) -> f32 {
        let (min, max) = (self.settings.price_min, self.settings.price_max);
        if price.min(max) <= min {
            min
        } else {
            price.min(max)
        }
    }

    /// Adds a step to the timers and says which checks are due (4): the rating check also when
    /// the month flag is set. The bar counter is [`Status::bars`]'.
    pub fn due(&mut self, step: u32, month_flag: bool) -> Due {
        let over = |timer: &mut u32, period: i32| {
            *timer = timer.saturating_add(step);
            i64::from(*timer) > i64::from(period)
        };
        let (rating, message, new_guest) = {
            let s = &self.settings;
            (s.rating, s.message, s.new_guest)
        };
        Due {
            rating: over(&mut self.timers[0], rating) || month_flag,
            message: over(&mut self.timers[1], message),
            new_guest: over(&mut self.timers[2], new_guest),
        }
    }

    /// The rating of the sums (6.1), less the escape `penalty` when it is above 0.
    pub fn compute(&self, sums: &Sums, penalty: i64) -> i32 {
        let s = &self.settings;
        let mut r = 0i64;
        if sums.animals > 0 {
            r = i64::from((sums.animals - sums.sick) * 15 / sums.animals);
        }
        if sums.species > 0 && s.species_available > 0 {
            r += i64::from(sums.species.min(s.species_available) * 10 / s.species_available);
        }
        r += i64::from((sums.animal_average() + 100) * 25 / 200);
        r += i64::from((sums.guest_average() + 100) * 25 / 200);
        r += (sums.zoo_value.clamp(0, 30_000) * 10 / 300) / 100;
        r += i64::from(sums.research) * 5 / 100;
        let e = (sums.esthetics * 10.0).clamp(0.0, 100.0);
        r += i64::from((e * 0.1f32) as i32);
        if penalty > 0 {
            r -= penalty;
        }
        r.clamp(0, 100) as i32
    }

    /// The rating check (6): the sums again, the rating, the statistics, the timer back to 0.
    /// `now` is the game time in milliseconds, `loose` whether an animal is loose, `escape` the
    /// ini's change and time of the escape penalty.
    pub fn rating_check(
        &mut self,
        sums: Sums,
        money: &mut Money,
        (now, loose): (i64, bool),
        escape: (i32, i32),
        log: &mut Vec<String>,
    ) {
        if loose {
            self.escape_ms = now;
        }
        let days = (now - self.escape_ms) / GAME_DAY_MS;
        let penalty = i64::from(escape.0) - i64::from(escape.1) * days;
        self.rating = self.compute(&sums, penalty);
        for (category, value) in [
            (money::ZOO_VALUE, sums.zoo_value as f32),
            (money::RATING, self.rating as f32),
            (money::ANIMAL_HAPPINESS, sums.animal_average() as f32),
            (money::GUEST_HAPPINESS, sums.guest_average() as f32),
            (money::GUESTS, sums.guests as f32),
        ] {
            money.set_statistic(category, value);
        }
        self.sums = sums;
        self.timers[0] = 0;
        log.push(format!("rating {}", self.rating));
    }

    /// The message check (7): the advice lines for the last sums and the cash.
    pub fn message_check(&mut self, cash: f32, say: &mut Vec<Say>, log: &mut Vec<String>) {
        self.timers[1] = 0;
        let (s, p) = (&self.sums, &self.settings.percent);
        let over = |count: i32, fraction: f32, number: i32| count as f32 * fraction < number as f32;
        let many = s.guests > MANY_GUESTS;
        let guest = s.guest_average();
        let c = cash as i32;
        let table = [
            (over(s.animals, p[0], s.sick), 10009),
            (many && over(s.animals, p[1], s.sick), 10008),
            (many && over(s.guests, p[2], s.hungry), 10011),
            (many && over(s.guests, p[3], s.thirsty), 10012),
            (many && over(s.guests, p[4], s.restroom), 10013),
            (many && over(s.guests, p[5], s.gift), 10014),
            (many && over(s.guests, p[6], s.tired), 10015),
            (many && over(s.guests, p[7], s.litter), 10016),
            (many && guest >= self.settings.high_guest_happiness, 10021),
            (many && guest <= self.settings.low_guest_happiness, 10022),
            ((1..=LOW_CASH).contains(&c), 10018),
            (c <= 0, 10019),
            (c < 0, 10020),
        ];
        for (_, id) in table.into_iter().filter(|(on, _)| *on) {
            say.push((id, None));
            log.push(format!("message {id}"));
        }
    }

    /// The finance check (9) from the endowment on, the generator having been seeded again: each
    /// member may leave, the rest pay the gift, then the accounts move to the next month.
    pub fn finance_check(&mut self, rng: &mut Rng, money: &mut Money) {
        let members = money.total[money::MEMBERS] as i32;
        if members > 0 {
            let factor = self.settings.membership_join_factor;
            let left = (0..members)
                .filter(|_| self.rating < (rng.rand() % 101) as i32 - factor)
                .count() as i32;
            money.add_count(money::MEMBERS, -(left as f32));
            money.add_count(money::MEMBERS_LEFT, left as f32);
            let (low, high) = (self.settings.endowment_low, self.settings.endowment_high);
            let draw = (rng.rand() % 101) as f32;
            let gift = low as f32 + draw * (high - low).max(0) as f32 * 0.01;
            money.refund(money::ENDOWMENT, gift * (members - left) as f32);
        }
        money.month_change();
    }

    /// The donation (10): at every step with the cash below 1000 and the chance, the next of
    /// the foundations gives, then they run out.
    pub fn donation(
        &mut self,
        rng: &mut Rng,
        money: &mut Money,
        say: &mut Vec<Say>,
        log: &mut Vec<String>,
    ) {
        if money.cash >= LOW_CASH as f32 || !rng.status_chance(self.settings.donation_chance) {
            return;
        }
        self.given += 1.0;
        let s = &self.settings;
        if self.given == (s.donations_available + 1) as f32 {
            say.push((15003, None));
        } else if self.given <= s.donations_available as f32 {
            let range = s.donation_high.saturating_sub(s.donation_low);
            let amount = match range > 0 {
                true => s
                    .donation_low
                    .saturating_add((rng.rand() % range as u32) as i32),
                false => s.donation_low,
            };
            money.refund(money::DONATIONS, amount as f32);
            say.push((15002, Some(amount)));
            log.push(format!("donation {amount}"));
        }
    }

    /// The bars (12): once the counter passes 1001 ms, the animal and guest happiness and the
    /// rating, each 0 to 100; an empty zoo shows 0 for its happiness.
    pub fn bars(&mut self, step: u32) -> Option<[i32; 3]> {
        self.bar_ms = self.bar_ms.saturating_add(step);
        if self.bar_ms <= BAR_PERIOD {
            return None;
        }
        self.bar_ms %= BAR_PERIOD;
        let s = &self.sums;
        let bar = |count: i32, average: i32| match count {
            0 => 0,
            _ => (average + 100) * 100 / 200,
        };
        Some([
            bar(s.animals, s.animal_average()),
            bar(s.guests, s.guest_average()),
            self.rating,
        ])
    }
}

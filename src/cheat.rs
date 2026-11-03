//! The name cheats (cheats 1 to 6, 8): a name set on an animal, a guest, a staff member or an
//! exhibit is first compared, whole and case-sensitive, with the cheat texts of its class; a
//! match runs the cheat. The texts are game text, so the table keeps only the 64-bit FNV-1a hash
//! of each, and a typed name is hashed and compared.

/// The kind of thing that was named: a cheat belongs to one class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Subject {
    Animal,
    Guest,
    Keeper,
    Worker,
    Guide,
    Exhibit,
}

/// What a row asks of the named thing besides its class (cheats 2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cond {
    Any,
    /// The subtype letter of its type: the sex of a staff member, `m` or `f`.
    Subtype(u8),
    /// The name id of its type.
    NameId(i32),
}

/// The thing a name was set on, as far as the conditions look.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Named {
    pub subtype: u8,
    pub name_id: i32,
}

impl Cond {
    fn holds(self, n: &Named) -> bool {
        match self {
            Cond::Any => true,
            Cond::Subtype(s) => n.subtype == s,
            Cond::NameId(id) => n.name_id == id,
        }
    }
}

/// The session flags (cheats 2.4): they belong to the program, are not saved and are not cleared
/// by a load or a new game. The step hands them to the rules that read them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Flags {
    /// `rating_bonus_30`: +30 to the effective rating of the guest arrival check.
    pub rating_bonus: bool,
    /// `donation_double`: what a guest donates is doubled.
    pub donation_double: bool,
    /// `shop_income_double`: what a charging building earns per customer is doubled.
    pub shop_income_double: bool,
    /// `bears_break_fences`: the grizzly bears bash fences.
    pub bears_bash: bool,
    /// `breeding_boost`: the next check of a bear lets it breed regardless of happiness.
    pub breeding_boost: bool,
    /// `keeper_speed_double`: the scientists walk twice as fast.
    pub keeper_speed_double: bool,
    /// `marketing_double_a` and `_b`: the marketing benefit is doubled.
    pub marketing_double: bool,
    /// `heal_changes_species`: a healed animal sometimes turns into another species.
    pub heal_changes_species: bool,
    /// A species walk saw a lion, a tiger and a grizzly bear in one exhibit (habitat rating 9.1
    /// point 4); the guest creation hook reads it ([`Season`]).
    pub lions_tigers_bears: bool,
}

/// The name id of the grizzly bear (cheats 4.1), the species the bears' cheats act on.
pub(crate) const GRIZZLY: i32 = 5016;
/// The name ids of the species the breeding cheat works for (cheats 4.5).
pub(crate) const BREEDERS: [i32; 2] = [5016, 5017];
/// The name id of the scientist, the keeper type of Dinosaur Digs (cheats 4.5).
pub(crate) const SCIENTIST: i32 = 9551;

/// The name ids of the lion and the tiger; with the grizzly they make the exhibit that holds all
/// three unlock the yellow brick path.
pub(crate) const LION: i32 = 5006;
pub(crate) const TIGER: i32 = 5007;

impl Flags {
    /// What the effective rating of the guest arrival check gains from the flags (zoo status 8).
    pub(crate) fn rating_points(&self) -> i32 {
        if self.rating_bonus { 30 } else { 0 }
    }
}

/// What a matching name does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cheat {
    /// `research_cat_NN`: completes every program of the research categories with this name id,
    /// as a plain completion (research 6.8).
    ResearchCategory(i32),
    // The exhibit names (cheats 3).
    RatingBonus,
    DonationDouble,
    ShopIncomeDouble,
    /// `unlock_animal_*`: the types with this name id become available.
    Unlock(i32),
    // The animal names (cheats 4.1).
    BearsBreakFences,
    DinosaursVanish,
    // The maintenance workers' names (cheats 4.4).
    FoliageClearBonus,
    FencesRepair,
    // The keepers' names (cheats 4.5).
    BreedingBoost,
    KeeperSpeedDouble,
    MarketingDouble,
    HealChangesSpecies,
    // The guest-name cheats (cheats 4.2), apart from the research ones.
    /// `palette_set_N`: every guest takes the shirt, trousers (male) and skirt (female) palettes,
    /// and every recolourable building the two it lists.
    Palette {
        guests: [u8; 3],
        buildings: [u8; 2],
    },
    /// `guests_flee_birds`: every guest decides to leave (the birds are skipped, cheats 10 O3).
    GuestsLeave,
    /// `fences_break`: every fence piece that can deteriorate breaks.
    FencesBreak,
    /// `guests_sick_recolour`: every guest is recoloured and, unless busy, made sick.
    GuestsSick,
    /// `unlock_all_scenarios`: every scenario result becomes 0, "unlocked by a cheat".
    UnlockScenarios,
    /// `eggs_hatch_fast`: session flag, an egg is laid with a hatch counter of 5.
    EggsHatchFast,
    /// `cars_to_rubble`: the cars turn to rubble and donations and cash gain 5500.
    CarsToRubble,
    /// `buildings_to_rubble`: every rubbleable building turns to rubble.
    BuildingsToRubble,
    /// `guest_spreads_sickness` and its kin: the named guest passes the effect of `bit` (one of
    /// [`spread`]) to the guests on its tile. All but the first need a man to be named.
    Spread(u8),
}

/// One row of the table: the hash of the exact text, the class it applies to, and the cheat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    pub hash: u64,
    pub class: Subject,
    pub cond: Cond,
    pub cheat: Cheat,
}

/// FNV-1a, 64 bits, over the bytes of a text.
pub(crate) fn hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The research category cheats, all of the guest class: the hash of the text, the name id of
/// the category.
const RESEARCH: [(u64, i32); 9] = [
    (8_217_426_204_780_126_983, 23201),
    (4_046_273_041_572_703_573, 23202),
    (270_688_140_449_273_432, 23203),
    (2_398_357_839_397_979_168, 23204),
    (13_736_647_030_866_780_753, 23205),
    (18_239_091_784_213_413_462, 23206),
    (8_197_895_324_385_658_030, 23207),
    (4_506_853_816_632_816_919, 23213),
    (15_232_229_614_914_872_326, 23214),
];

// Cheats 3, 4.1, 4.4, 4.5: the exhibit, animal, worker and keeper rows, as the hash of the text,
// the condition and the cheat, in the order of the spec's tables.
const EXHIBIT: [(u64, Cheat); 7] = [
    (6_905_518_711_216_013_203, Cheat::RatingBonus),
    (9_915_814_017_009_738_727, Cheat::DonationDouble),
    (17_584_139_552_020_099_469, Cheat::ShopIncomeDouble),
    (1_235_722_152_005_907_628, Cheat::Unlock(5049)),
    (3_730_055_665_491_434_908, Cheat::Unlock(5046)),
    (14_204_585_667_529_478_428, Cheat::Unlock(5047)),
    (15_253_207_984_280_970_367, Cheat::Unlock(5332)),
];
const ANIMALS: [(u64, Cond, Cheat); 2] = [
    (
        15_012_478_273_526_456_181,
        Cond::NameId(GRIZZLY),
        Cheat::BearsBreakFences,
    ),
    (
        3_252_289_375_139_617_263,
        Cond::NameId(5300),
        Cheat::DinosaursVanish,
    ),
];
const MALE: Cond = Cond::Subtype(b'm');
const WORKERS: [(u64, Cond, Cheat); 3] = [
    (8_393_534_450_899_611_877, MALE, Cheat::FoliageClearBonus),
    (8_468_693_752_337_878_622, MALE, Cheat::FencesRepair),
    (2_810_728_018_963_006_213, MALE, Cheat::BuildingsToRubble),
];
const KEEPERS: [(u64, Cond, Cheat); 5] = [
    (16_097_477_701_174_231_208, MALE, Cheat::BreedingBoost),
    (
        6_564_901_469_703_517_242,
        Cond::NameId(SCIENTIST),
        Cheat::KeeperSpeedDouble,
    ),
    (
        3_802_782_351_409_478_448,
        Cond::Subtype(b'f'),
        Cheat::MarketingDouble,
    ),
    (6_179_931_578_302_753_876, MALE, Cheat::MarketingDouble),
    (
        6_698_033_928_044_733_853,
        Cond::NameId(SCIENTIST),
        Cheat::HealChangesSpecies,
    ),
];

/// The rows of cheats 3 and 4.1 to 4.5.
fn exhibit_and_staff_rows() -> Vec<Entry> {
    let row = |class, cond, (hash, cheat)| Entry {
        hash,
        class,
        cond,
        cheat,
    };
    let exhibit = EXHIBIT.iter().map(|&r| row(Subject::Exhibit, Cond::Any, r));
    let of = |class, rows: &[(u64, Cond, Cheat)]| -> Vec<Entry> {
        let rows = rows.iter();
        rows.map(|&(hash, cond, cheat)| row(class, cond, (hash, cheat)))
            .collect()
    };
    let mut rows: Vec<Entry> = exhibit.collect();
    rows.extend(of(Subject::Animal, &ANIMALS));
    rows.extend(of(Subject::Worker, &WORKERS));
    rows.extend(of(Subject::Keeper, &KEEPERS));
    rows
}

/// The name ids of the seasonal items the date hooks make available (cheats 6): the holiday
/// tree, the jack-o'-lantern, the snowman and the yellow brick path.
pub(crate) const HOLIDAY_TREE: i32 = 7077;
pub(crate) const JACK_O_LANTERN: i32 = 7078;
pub(crate) const SNOWMAN: i32 = 7079;
pub(crate) const YELLOW_BRICK: i32 = 9111;

/// The date hooks' flags of the program run (cheats 6): not saved, not cleared by a load or a new
/// game. The flying objects of the hooks (the sleigh, the witch, the biplane) are not made yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Season {
    /// The permanent Halloween flag (a start on the real 31 October): while up, the jack-o'-lantern
    /// step of the guest hook is skipped. Never lowered.
    pub(crate) halloween: bool,
    /// The snowman and the holiday tree are available for 25 December.
    snow: bool,
    /// The jack-o'-lantern is available for 31 October.
    pumpkin: bool,
    /// The yellow brick path was made available; never lowered.
    brick: bool,
}

impl Season {
    /// The hook of a guest created on the game date `month`/`day` (cheats 6); `trio` is
    /// [`Flags::lions_tigers_bears`]. The availability it sets, in order: the items of the day
    /// on the first creation of that day and not again after, the same items unavailable again on
    /// the first creation of another day (the jack-o'-lantern only while the permanent Halloween flag
    /// is down), and the yellow brick path once ever.
    pub(crate) fn guest_created(&mut self, month: u32, day: u32, trio: bool) -> Vec<(i32, bool)> {
        let mut set = Vec::new();
        if (month, day) == (12, 25) {
            if !self.snow {
                self.snow = true;
                set.extend([(SNOWMAN, true), (HOLIDAY_TREE, true)]);
            }
        } else if self.snow {
            self.snow = false;
            set.extend([(SNOWMAN, false), (HOLIDAY_TREE, false)]);
        }
        if self.halloween {
            // Neither available nor taken back, and the day flag is left alone (cheats 6.2.4).
        } else if (month, day) == (10, 31) {
            if !self.pumpkin {
                self.pumpkin = true;
                set.push((JACK_O_LANTERN, true));
            }
        } else if self.pumpkin {
            self.pumpkin = false;
            set.push((JACK_O_LANTERN, false));
        }
        if trio && !self.brick {
            self.brick = true;
            set.push((YELLOW_BRICK, true));
        }
        set
    }

    /// The start of a game (cheats 6): the four items become unavailable. The day flags stay as
    /// they are, as in the original: a game started on the day of the previous one keeps the
    /// items unavailable until the date leaves and comes back.
    pub(crate) fn game_started() -> [(i32, bool); 4] {
        [HOLIDAY_TREE, JACK_O_LANTERN, SNOWMAN, YELLOW_BRICK].map(|id| (id, false))
    }
}

/// The bits of a guest's contagion byte (cheats 4.2 E), not saved.
pub mod spread {
    pub(crate) const SICKNESS: u8 = 2;
    pub(crate) const SCARE_WOMEN: u8 = 4;
    pub(crate) const SCARE_CHILDREN: u8 = 8;
    pub(crate) const YELLOW: u8 = 16;
    pub(crate) const RED: u8 = 32;
    /// The shirt palettes of the two painting bits.
    pub(crate) const YELLOW_SHIRT: u8 = 9;
    pub(crate) const RED_SHIRT: u8 = 1;
}

// The guest-name cheats other than the research ones (cheats 4.2): the hash of the text and the
// cheat, all of the guest class.
const fn palette(guests: [u8; 3], buildings: [u8; 2]) -> Cheat {
    Cheat::Palette { guests, buildings }
}
const GUEST_NAME: [(u64, Cheat); 17] = [
    (14_892_135_562_109_524_636, palette([8, 0, 0], [21, 16])),
    (3_657_051_404_038_028_442, palette([10, 1, 0], [7, 11])),
    (10_919_150_525_679_640_940, palette([14, 5, 0], [14, 19])),
    (12_275_381_737_552_333_213, palette([0, 6, 1], [23, 4])),
    (16_190_317_490_462_175_820, palette([15, 0, 3], [0, 22])),
    (17_712_366_998_183_474_644, palette([9, 1, 4], [15, 12])),
    (3_655_295_719_408_693_313, Cheat::GuestsLeave),
    (11_567_513_591_041_160_000, Cheat::FencesBreak),
    (11_901_318_460_961_946_479, Cheat::GuestsSick),
    (2_002_055_578_063_764_932, Cheat::UnlockScenarios),
    (18_144_294_961_631_575_098, Cheat::EggsHatchFast),
    (12_631_782_597_625_929_863, Cheat::CarsToRubble),
    (7_335_757_701_157_520_529, Cheat::Spread(spread::SICKNESS)),
    (
        16_981_303_017_011_423_432,
        Cheat::Spread(spread::SCARE_WOMEN),
    ),
    (
        1_789_208_750_258_418_929,
        Cheat::Spread(spread::SCARE_CHILDREN),
    ),
    (2_414_308_895_739_376_539, Cheat::Spread(spread::YELLOW)),
    (13_280_212_346_771_292_184, Cheat::Spread(spread::RED)),
];

/// The cheats the program knows. Session flags (cheats 2.4) live here too: they belong to the
/// program, are not saved and are not cleared by a load or a new game.
#[derive(Debug, Clone)]
pub(crate) struct Cheats {
    table: Vec<Entry>,
    pub flags: Flags,
    // The session flags of the guest-name cheats.
    /// `eggs_hatch_fast`.
    pub eggs_hatch_fast: bool,
    /// The date hooks' flags.
    pub season: Season,
}

impl Default for Cheats {
    fn default() -> Self {
        let research = RESEARCH.iter().map(|&(hash, name)| Entry {
            hash,
            class: Subject::Guest,
            cond: Cond::Any,
            cheat: Cheat::ResearchCategory(name),
        });
        let guest = GUEST_NAME.iter().map(|&(hash, cheat)| Entry {
            hash,
            class: Subject::Guest,
            cond: Cond::Any,
            cheat,
        });
        let mut table: Vec<Entry> = research.chain(guest).collect();
        table.extend(exhibit_and_staff_rows());
        Cheats {
            table,
            flags: Flags::default(),
            eggs_hatch_fast: false,
            season: Season::default(),
        }
    }
}

impl Cheats {
    /// The cheat of this class whose text is exactly `text`, for a thing no condition holds on:
    /// no trimming, no case folding. The first row in table order wins.
    #[cfg(test)]
    pub(crate) fn find(&self, class: Subject, text: &str) -> Option<Cheat> {
        self.find_named(class, &Named::default(), text)
    }

    /// As [`Cheats::find`], for the thing `named`: a row whose condition fails is skipped.
    pub(crate) fn find_named(&self, class: Subject, named: &Named, text: &str) -> Option<Cheat> {
        let hash = hash(text);
        let row = self
            .table
            .iter()
            .find(|e| e.class == class && e.hash == hash && e.cond.holds(named));
        row.map(|e| e.cheat)
    }

    /// Adds a row after the others.
    #[cfg(test)]
    pub(crate) fn push(&mut self, class: Subject, text: &str, cheat: Cheat) {
        self.push_when(class, Cond::Any, text, cheat);
    }

    /// Adds a row with a condition after the others.
    #[cfg(test)]
    pub(crate) fn push_when(&mut self, class: Subject, cond: Cond, text: &str, cheat: Cheat) {
        let hash = hash(text);
        self.table.push(Entry {
            hash,
            class,
            cond,
            cheat,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_fnv_1a() {
        assert_eq!(hash(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash("a"), 12_638_187_200_555_641_996);
    }

    #[test]
    fn a_text_matches_whole_and_case_sensitive_in_its_class_only() {
        let mut c = Cheats::default();
        c.table.clear();
        c.push(Subject::Guest, "Zed Zed", Cheat::ResearchCategory(9));
        let find = |class, text| c.find(class, text);
        assert_eq!(
            find(Subject::Guest, "Zed Zed"),
            Some(Cheat::ResearchCategory(9))
        );
        for no in [
            "zed zed",
            "Zed Zed ",
            " Zed Zed",
            "Zed Ze",
            "Zed Zed Zed",
            "",
        ] {
            assert_eq!(find(Subject::Guest, no), None, "{no:?}");
        }
        assert_eq!(find(Subject::Animal, "Zed Zed"), None);
    }

    #[test]
    fn the_table_holds_the_guest_cheats_on_distinct_texts() {
        let c = Cheats::default();
        let guests = c.table.iter().filter(|e| e.class == Subject::Guest);
        let mut hashes: Vec<u64> = guests.map(|e| e.hash).collect();
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(hashes.len(), 9 + GUEST_NAME.len());
    }

    #[test]
    fn the_exhibit_and_staff_rows_are_distinct_and_in_their_classes() {
        let c = Cheats::default();
        let count = |class| c.table.iter().filter(|e| e.class == class).count();
        let counts = [
            Subject::Exhibit,
            Subject::Animal,
            Subject::Worker,
            Subject::Keeper,
        ]
        .map(count);
        assert_eq!(counts, [7, 2, 3, 5]);
        assert_eq!(count(Subject::Guide), 0);
        let mut seen: Vec<(Subject, u64, Cheat)> = Vec::new();
        for e in &c.table {
            assert!(!seen.iter().any(|&(k, h, _)| (k, h) == (e.class, e.hash)));
            seen.push((e.class, e.hash, e.cheat));
        }
    }

    #[test]
    fn the_rows_carry_the_conditions_of_the_spec() {
        let c = Cheats::default();
        let rows = |class| -> Vec<(Cond, Cheat)> {
            let of = c.table.iter().filter(|e| e.class == class);
            of.map(|e| (e.cond, e.cheat)).collect()
        };
        let (male, female) = (Cond::Subtype(b'm'), Cond::Subtype(b'f'));
        assert_eq!(
            rows(Subject::Keeper),
            [
                (male, Cheat::BreedingBoost),
                (Cond::NameId(SCIENTIST), Cheat::KeeperSpeedDouble),
                (female, Cheat::MarketingDouble),
                (male, Cheat::MarketingDouble),
                (Cond::NameId(SCIENTIST), Cheat::HealChangesSpecies),
            ]
        );
        assert_eq!(
            rows(Subject::Worker),
            [
                (male, Cheat::FoliageClearBonus),
                (male, Cheat::FencesRepair),
                (male, Cheat::BuildingsToRubble)
            ]
        );
        assert_eq!(
            rows(Subject::Animal),
            [
                (Cond::NameId(GRIZZLY), Cheat::BearsBreakFences),
                (Cond::NameId(5300), Cheat::DinosaursVanish),
            ]
        );
        let unlocks: Vec<i32> = rows(Subject::Exhibit)
            .iter()
            .filter_map(|&(_, c)| {
                if let Cheat::Unlock(n) = c {
                    Some(n)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(unlocks, [5049, 5046, 5047, 5332]);
        assert!(rows(Subject::Exhibit).iter().all(|r| r.0 == Cond::Any));
    }

    #[test]
    fn the_rating_bonus_is_thirty_points_once_the_flag_is_up() {
        let mut flags = Flags::default();
        assert_eq!(flags.rating_points(), 0);
        flags.rating_bonus = true;
        assert_eq!(flags.rating_points(), 30);
    }

    #[test]
    fn a_condition_that_fails_skips_the_row() {
        let mut c = Cheats::default();
        c.table.clear();
        c.push_when(
            Subject::Keeper,
            Cond::Subtype(b'f'),
            "Zed",
            Cheat::MarketingDouble,
        );
        c.push_when(
            Subject::Keeper,
            Cond::NameId(7),
            "Zed",
            Cheat::BreedingBoost,
        );
        let named = |subtype, name_id| Named { subtype, name_id };
        let find = |n: Named| c.find_named(Subject::Keeper, &n, "Zed");
        assert_eq!(find(named(b'f', 0)), Some(Cheat::MarketingDouble));
        assert_eq!(find(named(b'm', 7)), Some(Cheat::BreedingBoost));
        // The first row whose condition holds wins.
        assert_eq!(find(named(b'f', 7)), Some(Cheat::MarketingDouble));
        for no in [
            named(b'm', 0),
            named(0, 0),
            named(b'F', -7),
            named(0, i32::MAX),
        ] {
            assert_eq!(find(no), None, "{no:?}");
        }
        assert_eq!(c.find(Subject::Keeper, "Zed"), None);
    }

    const SNOW: [(i32, bool); 2] = [(SNOWMAN, true), (HOLIDAY_TREE, true)];

    #[test]
    fn christmas_day_makes_the_snow_items_available_once_and_the_next_day_takes_them_back() {
        let mut s = Season::default();
        assert_eq!(s.guest_created(12, 25, false), SNOW);
        // Every other creation of the day does nothing.
        assert_eq!(s.guest_created(12, 25, false), []);
        assert_eq!(s.guest_created(12, 25, false), []);
        // The first creation of another day takes them back, once.
        let back = [(SNOWMAN, false), (HOLIDAY_TREE, false)];
        assert_eq!(s.guest_created(12, 26, false), back);
        assert_eq!(s.guest_created(12, 27, false), []);
        // The same day of another month is no Christmas.
        assert_eq!(s.guest_created(11, 25, false), []);
        assert_eq!(s.guest_created(1, 25, false), []);
    }

    #[test]
    fn the_permanent_halloween_flag_skips_the_jack_o_lantern_step_and_its_day_flag() {
        // Up and 7078 on a day other than 31 October: not taken back.
        let mut s = Season {
            pumpkin: true,
            halloween: true,
            ..Season::default()
        };
        assert_eq!(s.guest_created(11, 1, false), []);
        assert!(s.pumpkin);
        // Up on 31 October: not made available, and the day flag is not read or written.
        s.pumpkin = false;
        assert_eq!(s.guest_created(10, 31, false), []);
        assert!(!s.pumpkin);
        // The other steps still run.
        assert_eq!(s.guest_created(12, 25, false), SNOW);
        // Down again: the day flag was left alone, so 31 October makes it available.
        s.halloween = false;
        let set = s.guest_created(10, 31, false);
        assert_eq!(set.last(), Some(&(JACK_O_LANTERN, true)));
    }

    #[test]
    fn the_last_day_of_october_makes_the_jack_o_lantern_available_for_the_day() {
        let mut s = Season::default();
        assert_eq!(s.guest_created(10, 30, false), []);
        assert_eq!(s.guest_created(10, 31, false), [(JACK_O_LANTERN, true)]);
        assert_eq!(s.guest_created(10, 31, false), []);
        assert_eq!(s.guest_created(11, 1, false), [(JACK_O_LANTERN, false)]);
        assert_eq!(s.guest_created(11, 1, false), []);
        // Both days are independent: Christmas does not lower the pumpkin.
        assert_eq!(s.guest_created(10, 31, false), [(JACK_O_LANTERN, true)]);
        assert_eq!(
            s.guest_created(12, 25, false),
            [SNOW[0], SNOW[1], (JACK_O_LANTERN, false)]
        );
    }

    #[test]
    fn the_yellow_brick_path_comes_once_per_program_run_and_after_the_trio_only() {
        let mut s = Season::default();
        assert_eq!(s.guest_created(3, 3, false), []);
        assert_eq!(s.guest_created(3, 3, true), [(YELLOW_BRICK, true)]);
        assert_eq!(s.guest_created(3, 3, true), []);
        // A start takes it away and nothing gives it back.
        assert_eq!(Season::game_started()[3], (YELLOW_BRICK, false));
        assert_eq!(s.guest_created(3, 4, true), []);
    }

    #[test]
    fn a_start_makes_the_four_items_unavailable_and_keeps_the_day_flags() {
        let ids: Vec<i32> = Season::game_started().iter().map(|a| a.0).collect();
        assert_eq!(ids, [7077, 7078, 7079, 9111]);
        assert!(Season::game_started().iter().all(|a| !a.1));
        // A game restarted on the same day: the items stay unavailable until the date moves.
        let mut s = Season::default();
        s.guest_created(12, 25, false);
        assert_eq!(s.guest_created(12, 25, false), []);
        assert_eq!(
            s.guest_created(12, 24, false),
            [(SNOWMAN, false), (HOLIDAY_TREE, false)]
        );
        assert_eq!(s.guest_created(12, 25, false), SNOW);
    }
}

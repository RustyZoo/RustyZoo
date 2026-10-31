//! The name cheats (cheats 1, 2, 8): a name set on an animal, a guest, a staff member or an
//! exhibit is first compared, whole and case-sensitive, with the cheat texts of its class; a
//! match runs the cheat. The texts are game text, so the table keeps only the 64-bit FNV-1a hash
//! of each, and a typed name is hashed and compared.

/// The kind of thing that was named: a cheat belongs to one class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    Animal,
    Guest,
    Staff,
    Exhibit,
}

/// What a matching name does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cheat {
    /// `research_cat_NN`: completes every program of the research categories with this name id,
    /// as a plain completion (research 6.8).
    ResearchCategory(i32),
    // The guest-name cheats (cheats 4.2), apart from the research ones.
    /// `palette_set_N`: every guest takes the shirt, trousers (male) and skirt (female) palettes,
    /// and every recolourable building the two it lists.
    Palette { guests: [u8; 3], buildings: [u8; 2] },
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
    /// `cars_to_rubble`: the car scenery goes and donations and cash gain 5500.
    CarsToRubble,
    /// `guest_spreads_sickness` and its kin: the named guest passes the effect of `bit` (one of
    /// [`spread`]) to the guests on its tile. All but the first need a man to be named.
    Spread(u8),
}

/// One row of the table: the hash of the exact text, the class it applies to, and the cheat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub hash: u64,
    pub class: Subject,
    pub cheat: Cheat,
}

/// FNV-1a, 64 bits, over the bytes of a text.
pub fn hash(text: &str) -> u64 {
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

/// The bits of a guest's contagion byte (cheats 4.2 E), not saved.
pub mod spread {
    pub const SICKNESS: u8 = 2;
    pub const SCARE_WOMEN: u8 = 4;
    pub const SCARE_CHILDREN: u8 = 8;
    pub const YELLOW: u8 = 16;
    pub const RED: u8 = 32;
    /// The shirt palettes of the two painting bits.
    pub const YELLOW_SHIRT: u8 = 9;
    pub const RED_SHIRT: u8 = 1;
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
pub struct Cheats {
    table: Vec<Entry>,
    // The session flags of the guest-name cheats.
    /// `eggs_hatch_fast`.
    pub eggs_hatch_fast: bool,
}

impl Default for Cheats {
    fn default() -> Self {
        let research = RESEARCH.iter().map(|&(hash, name)| Entry {
            hash,
            class: Subject::Guest,
            cheat: Cheat::ResearchCategory(name),
        });
        let guest = GUEST_NAME.iter().map(|&(hash, cheat)| Entry {
            hash,
            class: Subject::Guest,
            cheat,
        });
        Cheats {
            table: research.chain(guest).collect(),
            eggs_hatch_fast: false,
        }
    }
}

impl Cheats {
    /// The cheat of this class whose text is exactly `text`: no trimming, no case folding. The
    /// first row in table order wins.
    pub fn find(&self, class: Subject, text: &str) -> Option<Cheat> {
        let hash = hash(text);
        let row = self
            .table
            .iter()
            .find(|e| e.class == class && e.hash == hash);
        row.map(|e| e.cheat)
    }

    /// Adds a row after the others.
    #[cfg(test)]
    pub fn push(&mut self, class: Subject, text: &str, cheat: Cheat) {
        let hash = hash(text);
        self.table.push(Entry { hash, class, cheat });
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
        let mut hashes: Vec<u64> = c.table.iter().map(|e| e.hash).collect();
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(hashes.len(), 9 + GUEST_NAME.len());
        assert!(c.table.iter().all(|e| e.class == Subject::Guest));
    }
}

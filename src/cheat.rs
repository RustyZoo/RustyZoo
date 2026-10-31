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

/// The cheats the program knows. Session flags (cheats 2.4) will live here too: they belong to the
/// program, are not saved and are not cleared by a load or a new game.
#[derive(Debug, Clone)]
pub struct Cheats {
    table: Vec<Entry>,
}

impl Default for Cheats {
    fn default() -> Self {
        let research = RESEARCH.iter().map(|&(hash, name)| Entry {
            hash,
            class: Subject::Guest,
            cheat: Cheat::ResearchCategory(name),
        });
        Cheats {
            table: research.collect(),
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
    fn the_table_holds_the_nine_research_categories_on_distinct_texts() {
        let c = Cheats::default();
        let mut hashes: Vec<u64> = c.table.iter().map(|e| e.hash).collect();
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(hashes.len(), 9);
        assert!(c.table.iter().all(|e| e.class == Subject::Guest));
    }
}

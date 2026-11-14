//! The class keys of `animals.ai` and the keys of one species, read from the game files.

use std::collections::HashMap;
use std::sync::Arc;

use super::*;

/// The class keys of `animals.ai` (animals 3.4), read once for every animal type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub periods: [i32; COUNTERS],
    pub death_change: i32,
    pub keeper_arrives: i32,
    pub keeper_arrives_amphibious: i32,
    /// `cZapCheck`: the period of the zap counter, which creation does not draw.
    pub zap: i32,
    /// `cUIThreshold`, `cDirtChanceChance` and `cDirtChance` (animals 4.5).
    pub ui_threshold: i32,
    pub dirt_chance_chance: i32,
    pub dirt_chance: i32,
    /// The terrain type of trampled ground: 16, or 0 when the terrain table has fewer than 17.
    pub trample: u8,
    /// `AnimalResetInterval` of the ini file (0 for never); the hosts set it.
    pub reset_interval: i32,
    /// `cDeathChance`, the fallback of the species key (animals-needs 10.2).
    pub death_chance: i32,
    /// `cMatePickDelay`: the reproduction checks a monogamous animal waits before it looks for
    /// another mate (animals-needs 11.2).
    pub mate_pick_delay: i32,
    /// `cBoxedCheck`: the period of the crate counter, which creation does not draw.
    pub boxed: i32,
    /// `cRattleChance`: the percent chance a crate check shakes the crate (unit placing 9.2).
    pub rattle: i32,
    /// `cDefenseRadius`: the tiles within which two preattack animals fight (animals 20.4).
    pub defense_radius: i32,
}

impl Default for Class {
    /// The program's values for a missing file or key.
    fn default() -> Class {
        Class::read(&|_, fallback| fallback)
    }
}

impl Class {
    /// The keys of `animals.ai`; a file that is missing or unreadable keeps the defaults. The
    /// terrain types are those of the install.
    pub fn load(vfs: &Vfs) -> Class {
        let file = vfs
            .read("animals.ai")
            .ok()
            .and_then(|b| Config::parse(&b).ok());
        let class = Class::read(&|key, fallback| {
            (file.as_ref())
                .and_then(|c| c.section("Characteristics/Integers")?.get(key))
                .and_then(to_int)
                .unwrap_or(fallback)
        });
        let trample = if crate::terrain::types(vfs).len() > TRAMPLED.into() {
            TRAMPLED
        } else {
            0
        };
        Class { trample, ..class }
    }

    pub(super) fn read(int: &dyn Fn(&str, i32) -> i32) -> Class {
        Class {
            periods: PERIODS.map(|(key, period)| {
                if key.is_empty() {
                    period
                } else {
                    int(key, period)
                }
            }),
            death_change: int("cDeathChange", -21),
            keeper_arrives: int("cKeeperArrivesChange", 100),
            keeper_arrives_amphibious: int("cKeeperArrivesChangeAmphibious", 100),
            zap: int("cZapCheck", 5),
            ui_threshold: int("cUIThreshold", 50),
            dirt_chance_chance: int("cDirtChanceChance", 50),
            dirt_chance: int("cDirtChance", 10),
            trample: TRAMPLED,
            reset_interval: 0,
            death_chance: int("cDeathChance", 99),
            mate_pick_delay: int("cMatePickDelay", 3),
            boxed: int("cBoxedCheck", 2),
            rattle: int("cRattleChance", 10),
            defense_radius: int("cDefenseRadius", 2),
        }
    }
}

/// Declares [`Keys`]: the keys of one species with the program's default for an absent one.
macro_rules! keys {
    ($($field:ident: $key:literal = $default:expr,)*) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub(crate) struct Keys {
            $(pub $field: i32,)*
            /// `cKeeperArrivesChange`, which falls back to the class file's.
            pub keeper_arrives: i32,
            /// `cDirtChance` and `cDeathChance`, which fall back to the class file's.
            pub dirt_chance: i32,
            pub death_chance: i32,
            /// `cUnderwater` 1 and `cOnlyUnderwater` 0.
            pub amphibious: bool,
            /// `cSwims`, `cUnderwater` and `cOnlyUnderwater` of the type.
            pub swims: bool,
            /// `cHeliRecovery` is 1: the helicopter and the scientist recover the species.
            pub heli_recovery: bool,
            pub underwater: bool,
            pub only_underwater: bool,
            /// The species prefers salt water: its value for terrain 10 is at least that of
            /// terrain 9 (tank fauna 11). The keys' reading leaves it out; [`Keys::of`] sets it.
            pub salt: bool,
        }

        impl Keys {
            /// The keys through `int`, which gives the value of a key or the fallback it is handed.
            pub(crate) fn read(int: &dyn Fn(&str, i32) -> i32, class: &Class) -> Keys {
                let amphibious = int("cUnderwater", 0) == 1 && int("cOnlyUnderwater", 0) == 0;
                Keys {
                    $($field: int($key, $default),)*
                    keeper_arrives: match amphibious {
                        true => int("cKeeperArrivesChangeAmphibious", class.keeper_arrives_amphibious),
                        false => int("cKeeperArrivesChange", class.keeper_arrives),
                    },
                    dirt_chance: int("cDirtChance", class.dirt_chance),
                    death_chance: int("cDeathChance", class.death_chance),
                    amphibious,
                    swims: int("cSwims", 0) == 1,
                    heli_recovery: int("cHeliRecovery", 0) == 1,
                    underwater: int("cUnderwater", 0) == 1,
                    only_underwater: int("cOnlyUnderwater", 0) == 1,
                    salt: false,
                }
            }
        }
    };
}

keys! {
    initial_happiness: "cInitialHappiness" = 60,
    max_hits: "cMaxHits" = 500,
    max_energy: "cMaxEnergy" = 100,
    min_dirty: "cMinDirty" = 0,
    max_dirty: "cMaxDirty" = 100,
    habitat_preference: "cHabitatPreference" = 70,
    pct_habitat: "cPctHabitat" = 10,
    happy_habitat: "cHappyHabitatChange" = 20,
    angry_habitat: "cAngryHabitatChange" = -10,
    very_angry_habitat: "cVeryAngryHabitatChange" = -20,
    captivity: "cCaptivity" = 5,
    number_min: "cNumberAnimalsMin" = 1,
    number_max: "cNumberAnimalsMax" = 10,
    habitat_size: "cHabitatSize" = 0,
    number_min_change: "cNumberMinChange" = -5,
    number_max_change: "cNumberMaxChange" = -5,
    density: "cAnimalDensity" = 30,
    all_crowded_change: "cAllCrowdedChange" = -10,
    crowd: "cCrowd" = 40,
    crowd_change: "cCrowdHappinessChange" = -1,
    energy_threshold: "cEnergyThreshold" = 70,
    hunger_threshold: "cHungerThreshold" = 100,
    drink: "cDrinkWaterChance" = 0,
    chase: "cChaseAnimalChance" = 0,
    other_angry: "cOtherAnimalAngryChange" = -30,
    rampage_threshold: "cRampageThreshold" = -100,
    rampage_chance: "cRampageChance" = 0,
    rampage_min: "cRampageTimeMin" = 0,
    rampage_max: "cRampageTimeMax" = 0,
    test_fence: "cTestFenceChance" = 0,
    fence_offset: "cTestFenceOffset" = 0,
    fence_drop: "cTestFenceChanceDrop" = 0,
    fence_min: "cMinTestFenceChance" = 0,
    zap_change: "cZapHappinessChange" = -1,
    hunger_increment: "cHungerIncrement" = 10,
    hungry_health: "cHungryHealthChange" = 5,
    no_food_change: "cNoFoodChange" = -20,
    food_kind: "cKeeperFoodType" = 0,
    bite: "cKeeperFoodUnitsEaten" = 20,
    food_value: "cFoodUnitValue" = 1,
    keep_moving: "cKeepMoving" = 0,
    skip_happiness: "cSkipTrickHappiness" = -100,
    skip_chance: "cSkipTrickChance" = 0,
    energy_increment: "cEnergyIncrement" = 15,
    dirty_increment: "cDirtyIncrement" = 15,
    dirty_threshold: "cDirtyThreshold" = 70,
    small_dung: "cSmallZoodoo" = 0,
    giant_dung: "cGiantZoodoo" = 0,
    dino_dung: "cDinoZoodoo" = 0,
    poo_impact: "cPooWaterImpact" = 1,
    pct_hits: "cPctHits" = 20,
    sick_change: "cSickChange" = -20,
    other_sick_change: "cOtherAnimalSickChange" = -10,
    sick_chance: "cSickChance" = 10,
    sick_random: "cSickRandomChance" = 10,
    sick_time: "cSickTime" = 5,
    time_death: "cTimeDeath" = 5001,
    building_use: "cBuildingUseChance" = 0,
    reproduction_chance: "cReproductionChance" = 10,
    reproduction_interval: "cReproductionInterval" = 70,
    happy_reproduce: "cHappyReproduceThreshold" = 0,
    mating_type: "cMatingType" = 0,
    no_mate_change: "cNoMateChange" = -19,
    offspring: "cOffspring" = 1,
    baby_born_change: "cBabyBornChange" = 60,
    baby_to_adult: "cBabyToAdult" = 100,
    breath_threshold: "cBreathThreshold" = 100,
    breath_increment: "cBreathIncrement" = 0,
    murky: "cMurkyWaterThreshold" = 60,
    very_murky: "cVeryMurkyWaterThreshold" = 20,
    extremely_murky: "cExtremelyMurkyWaterThreshold" = 1,
    murky_change: "cMurkyWaterChange" = -5,
    very_murky_change: "cVeryMurkyWaterChange" = -10,
    extremely_murky_change: "cExtremelyMurkyWaterChange" = -15,
    murky_health: "cMurkyWaterHealthChange" = -5,
    very_murky_health: "cVeryMurkyWaterHealthChange" = -10,
    extremely_murky_health: "cExtremelyMurkyWaterHealthChange" = -15,
    depth_min: "cDepthMin" = 0,
    depth_max: "cDepthMax" = 0,
    depth_change: "cDepthChange" = -5,
    salinity_change: "cSalinityChange" = -5,
    salinity_health: "cSalinityHealthChange" = -5,
    enter_water: "cEnterWaterChance" = 40,
    enter_tank: "cEnterTankChance" = 0,
    enter_land: "cEnterLandChance" = 40,
    water_needed: "cWaterNeeded" = 10,
    underwater_needed: "cUnderwaterNeeded" = 10,
    land_needed: "cLandNeeded" = 1,
    lays_eggs: "cLaysEggs" = 0,
    stay_sick: "cStaySick" = 0,
    time_to_hatch: "cTimeToHatch" = 0,
    min_hatch_hits: "cMinHatchHits" = 0,
}

impl Keys {
    /// The keys of an animal type: the section of its subtype, of the default subtype, then
    /// the plain one (entity types 6.2).
    pub(crate) fn of(ty: &EntityType, class: &Class) -> Keys {
        let Some(config) = ty.config.as_deref() else {
            return Keys::read(&|_, fallback| fallback, class);
        };
        let default = (config.section("Global")).and_then(|g| g.get("DefaultSubtype"));
        let int = |key: &str, fallback: i32| {
            [&ty.subtype[..], default.unwrap_or_default(), b""]
                .into_iter()
                .find_map(|prefix| {
                    let name = match prefix.is_empty() {
                        true => b"Characteristics/Integers".to_vec(),
                        false => [prefix, b"/Characteristics/Integers"].concat(),
                    };
                    config.section(name)?.get(key).and_then(to_int)
                })
                .unwrap_or(fallback)
        };
        let mut keys = Keys::read(&int, class);
        keys.salt = prefers_salt(config);
        keys
    }

    /// The keys with the changes of the completed research programs (research 6.4). The amphibious
    /// keeper-arrives key is another key and stays.
    pub(crate) fn researched(mut self, ty: &EntityType, over: &Overlay) -> Keys {
        if !self.amphibious {
            self.keeper_arrives = over.int(ty, "cKeeperArrivesChange", self.keeper_arrives);
        }
        self.food_value = over.int(ty, "cFoodUnitValue", self.food_value);
        self.reproduction_chance = over.int(ty, "cReproductionChance", self.reproduction_chance);
        self.sick_chance = over.int(ty, "cSickChance", self.sick_chance);
        self.time_death = over.int(ty, "cTimeDeath", self.time_death);
        self.number_max = over.int(ty, "cNumberAnimalsMax", self.number_max);
        self
    }
}

/// The keys of the species read so far, by definition file and subtype, with the class keys
/// they were read with: [`Keys::of`] reads some eighty keys, and every needs check asks for them.
#[derive(Debug, Default)]
pub(crate) struct KeysRead(pub(super) std::cell::RefCell<Read>);

type Read = (
    Option<Class>,
    HashMap<usize, (Arc<Config>, Vec<(Vec<u8>, Keys)>)>,
);

impl KeysRead {
    /// [`Keys::of`], read once for each type. The entry holds the type's file, so that no other
    /// file takes its address.
    pub(crate) fn of(&self, ty: &EntityType, class: &Class) -> Keys {
        let Some(config) = &ty.config else {
            return Keys::of(ty, class);
        };
        let mut read = self.0.borrow_mut();
        let (with, types) = &mut *read;
        if with.as_ref() != Some(class) {
            (*with, *types) = (Some(class.clone()), HashMap::new());
        }
        let (_, subtypes) = (types.entry(Arc::as_ptr(config) as usize))
            .or_insert_with(|| (config.clone(), Vec::new()));
        if let Some((_, keys)) = subtypes.iter().find(|(s, _)| *s == ty.subtype) {
            return keys.clone();
        }
        let keys = Keys::of(ty, class);
        subtypes.push((ty.subtype.clone(), keys.clone()));
        keys
    }
}

/// Whether the species' compatible terrain (`cCompatibleTerrain`, pairs of a terrain and a wish)
/// values salt water (terrain 10) at least as much as fresh water (terrain 9); the last pair that
/// names a terrain counts.
pub(super) fn prefers_salt(config: &Config) -> bool {
    let values = config.section("cCompatibleTerrain");
    let numbers: Vec<i32> = (values.map_or(&[][..], |s| s.get_all("v")).iter())
        .map(|v| to_int(v).unwrap_or(0))
        .collect();
    let wish = |terrain: i32| {
        (numbers.as_chunks::<2>().0.iter().rev())
            .find(|p| p[0] == terrain)
            .map_or(0, |p| p[1])
    };
    wish(10) >= wish(9)
}

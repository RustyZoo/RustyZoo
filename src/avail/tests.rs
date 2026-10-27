use super::*;
use crate::config::Config;
use std::sync::Arc;

/// A type of this name id and startup value (`None`: the key is missing).
fn ty(class: &'static str, name: &str, id: i32, startup: Option<i32>) -> EntityType {
    let ini = startup.map_or(String::new(), |v| {
        format!("[Global]\nAvailableAtStartup={v}\n")
    });
    EntityType {
        class,
        type_name: name.into(),
        subtype: b"f".to_vec(),
        file: None,
        config: Some(Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.types = vec![
        ty("animals", "lion", 5006, Some(0)),
        ty("animals", "lion", 5006, Some(0)),
        ty("animals", "gnu", 5007, None),
        ty("objects", "lion", 5006, Some(0)),
        ty("animals", "mine", 19001, Some(0)),
        ty("animals", "none", 0, Some(0)),
    ];
    cat
}

fn flags(a: &Availability, cat: &Catalogue) -> Vec<bool> {
    (0..cat.types.len() + 1).map(|i| a.is(cat, i)).collect()
}

#[test]
fn the_flag_starts_from_the_catalogue_and_a_user_type_is_always_available() {
    let (cat, a) = (catalogue(), Availability::default());
    assert_eq!(
        flags(&a, &cat),
        [false, false, true, false, true, false, false]
    );
}

#[test]
fn a_name_id_with_high_bits_is_not_a_user_type() {
    let mut cat = Catalogue::default();
    cat.types = vec![ty("animals", "mine", (1 << 24) | 19001, Some(0))];
    let a = Availability::default();
    assert!(!a.is(&cat, 0), "only the start-up flag applies");
}

#[test]
fn a_name_id_sets_every_subtype_and_a_wrong_id_sets_none() {
    let (cat, mut a) = (catalogue(), Availability::default());
    assert!(a.set(&cat, 5006, true));
    assert_eq!(
        flags(&a, &cat),
        [true, true, true, true, true, false, false]
    );
    assert!(!a.set(&cat, 5006, true), "no change");
    assert!(a.set(&cat, 5006, false));
    assert!(
        a.set(&cat, 5007, false),
        "a type available at the start can be cleared"
    );
    assert_eq!(
        flags(&a, &cat),
        [false, false, false, false, true, false, false]
    );
    assert!(!a.set(&cat, 19001, false), "a user type stays available");
    for id in [-1, 5008, i32::MAX, i32::MIN] {
        assert!(!a.set(&cat, id, true), "{id}");
    }
    assert_eq!(
        flags(&a, &cat),
        [false, false, false, false, true, false, false]
    );
}

#[test]
fn a_name_id_of_0_sets_the_types_without_a_name() {
    let (cat, mut a) = (catalogue(), Availability::default());
    assert!(a.set(&cat, 0, true));
    assert_eq!(
        flags(&a, &cat),
        [false, false, true, false, true, true, false]
    );
    assert!(a.set(&cat, 0, false));
    assert!(!a.set(&cat, -1, true), "a negative id names no type");
    assert_eq!(
        flags(&a, &cat),
        [false, false, true, false, true, false, false]
    );
}

fn rewards(cat: &Catalogue, ini: &str) -> (Availability, bool) {
    let mut a = Availability::default();
    let changed = a.unlock(cat, &Config::parse(ini.as_bytes()).unwrap());
    (a, changed)
}

#[test]
fn a_reward_unlocks_the_class_and_type_name_of_the_first_type_with_the_id() {
    let cat = catalogue();
    let (a, changed) = rewards(&cat, "[user]\nunlockCount=1\nunlockEntity0=5006\n");
    assert!(changed);
    // The objects type with the same id is not of the class and type name of the first one.
    assert_eq!(
        flags(&a, &cat),
        [true, true, true, false, true, false, false]
    );
    let (a, _) = rewards(
        &cat,
        "[USER]\nUNLOCKCOUNT = 2\nunlockentity0=5007\nunlockEntity1=5006\n",
    );
    assert_eq!(flags(&a, &cat)[..3], [true, true, true]);
}

#[test]
fn bad_rewards_unlock_nothing() {
    let cat = catalogue();
    for ini in [
        "",
        "[user]\nunlockEntity0=5006\n",
        "[user]\nunlockCount=0\nunlockEntity0=5006\n",
        "[user]\nunlockCount=-1\nunlockEntity0=5006\n",
        "[user]\nunlockCount=x\nunlockEntity0=5006\n",
        "[user]\nunlockCount=1\n",
        "[user]\nunlockCount=1\nunlockEntity0=0\n",
        "[user]\nunlockCount=1\nunlockEntity0=-5006\n",
        "[user]\nunlockCount=1\nunlockEntity0=2147483647\n",
        "[user]\nunlockCount=1\nunlockEntity0=\n",
        "[user]\nunlockCount=1\nunlockEntity1=5006\n",
    ] {
        let (a, changed) = rewards(&cat, ini);
        assert!(!changed, "{ini:?}");
        assert_eq!(flags(&a, &cat)[..2], [false, false], "{ini:?}");
    }
    // A huge count reads the entries that exist and stops at the bound.
    let (a, changed) = rewards(&cat, "[user]\nunlockCount=2147483647\nunlockEntity1=5006\n");
    assert!(changed);
    assert!(a.is(&cat, 0));
    let (a, _) = rewards(
        &cat,
        "[user]\nunlockCount=2147483647\nunlockEntity20000=5006\n",
    );
    assert!(!a.is(&cat, 0), "beyond the bound");
}

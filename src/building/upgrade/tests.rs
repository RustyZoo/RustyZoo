use super::*;
use crate::entity::EntityType;
use crate::unit::testing::world;

fn units() -> Units {
    world((2, 2), |_, _| {}).units
}

fn up(name: i32, upkeep: i32) -> Upgrade {
    Upgrade {
        name,
        icon: String::new(),
        upkeep,
        adult: 5,
        child: 3,
    }
}

fn kind(name_id: i32) -> Kind {
    let ty = EntityType {
        class: "building",
        type_name: b"shop".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: None,
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    };
    Kind::of(&ty, 4.0)
}

#[test]
fn a_program_is_added_once_to_the_list_of_its_type() {
    let mut list = Upgrades::new();
    add(&mut list, 7, up(1, 0));
    add(&mut list, 7, up(2, 200));
    add(&mut list, 7, up(1, 999));
    add(&mut list, 8, up(1, 5));
    assert_eq!(list[&7], [up(1, 0), up(2, 200)]);
    assert_eq!(list[&8], [up(1, 5)]);
}

#[test]
fn the_current_upgrade_is_the_entry_at_the_index_and_none_out_of_range() {
    let mut u = units();
    add(&mut u.upgrades, 7, up(1, 0));
    add(&mut u.upgrades, 7, up(2, 200));
    // A building with no live entry uses the first upgrade.
    assert_eq!(current(&u, 7, 1).map(|x| x.upkeep), Some(0));
    let live = |index| Live {
        upgrade: index,
        ..Live::default()
    };
    u.buildings.insert(1, live(1));
    assert_eq!(current(&u, 7, 1).map(|x| x.upkeep), Some(200));
    for index in [2, 3, u32::MAX] {
        u.buildings.insert(1, live(index));
        assert_eq!(current(&u, 7, 1), None, "index {index}");
    }
    // Another type, or a type with no list.
    assert_eq!(current(&u, 8, 1), None);
}

#[test]
fn a_button_sets_the_index_only_within_the_list() {
    let mut u = units();
    add(&mut u.upgrades, 7, up(1, 0));
    add(&mut u.upgrades, 7, up(2, 200));
    let k = kind(7);
    assert!(choose(&mut u, &k, 4, 1));
    assert_eq!(u.buildings[&4].upgrade, 1);
    assert!(choose(&mut u, &k, 4, 0));
    assert_eq!(u.buildings[&4].upgrade, 0);
    // Beyond the list, the buttons' last position, a huge position and a type with no list.
    for n in [2, BUTTONS - 1, usize::MAX] {
        assert!(!choose(&mut u, &k, 4, n), "position {n}");
    }
    assert!(!choose(&mut u, &kind(9), 5, 0));
    assert_eq!(u.buildings[&4].upgrade, 0);
    assert!(!u.buildings.contains_key(&5));
}

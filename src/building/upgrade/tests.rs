use super::*;
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
    u.upgrade_index.insert(1, 1);
    assert_eq!(current(&u, 7, 1).map(|x| x.upkeep), Some(200));
    for index in [2, 3, u32::MAX] {
        u.upgrade_index.insert(1, index);
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
    assert!(choose(&mut u, 7, 4, 1));
    assert_eq!(u.upgrade_index[&4], 1);
    assert!(choose(&mut u, 7, 4, 0));
    assert!(u.upgrade_index.is_empty(), "the first is the default");
    assert!(choose(&mut u, 7, 4, 1));
    // Beyond the list, the buttons' last position, a huge position and a type with no list.
    for n in [2, BUTTONS - 1, usize::MAX] {
        assert!(!choose(&mut u, 7, 4, n), "position {n}");
    }
    assert!(!choose(&mut u, 9, 5, 0));
    assert_eq!(u.upgrade_index.len(), 1);
    assert_eq!(u.upgrade_index[&4], 1);
}

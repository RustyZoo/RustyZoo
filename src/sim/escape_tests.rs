//! The `escape ID` script command, and the helicopter's recovery on the full game's scenario map
//! that holds a DRT base.

use super::drive::{Command, Run};
use crate::animal::Class;
use crate::entity::Catalogue;
use crate::money::Money;
use crate::unit::{Part, testing};

fn loose(run: &Run, id: u32) -> Option<bool> {
    match &run.world.units.get(id)?.part {
        Part::Animal(a) => Some(a.escaped),
        _ => None,
    }
}

#[test]
fn escape_takes_one_unit_id() {
    assert_eq!("escape 7".parse(), Ok(Command::Escape(7)));
    assert_eq!("escape 0".parse(), Ok(Command::Escape(0)));
    assert_eq!("escape 4294967295".parse(), Ok(Command::Escape(u32::MAX)));
    for bad in [
        "escape",
        "escape -1",
        "escape 4294967296",
        "escape x",
        "escape 1 2",
    ] {
        assert!(bad.parse::<Command>().is_err(), "{bad:?}");
    }
}

#[test]
fn escape_lets_an_animal_loose_once_and_changes_nothing_for_another_unit() {
    let world = testing::world((4, 4), |_, e| {
        e.push(testing::entity("animals", "zebra", 5, (96, 96), 0));
        e.push(testing::entity("keeper", "keeper", 6, (160, 160), 0));
    });
    let mut run = Run::new(
        (b"", b""),
        (Catalogue::default(), Class::default()),
        world,
        Money::default(),
        1,
    );
    assert_eq!(loose(&run, 5), Some(false));
    for id in [0, 6, 99] {
        assert!(!run.apply(&Command::Escape(id)), "{id}");
    }
    assert!(run.apply(&Command::Escape(5)) && run.apply(&Command::Escape(5)));
    assert_eq!(loose(&run, 5), Some(true));
    assert_eq!(run.world.units.escapes, [5], "listed once");
    let lines = run.step(67);
    assert_eq!(lines.iter().filter(|l| l.ends_with("escape 5")).count(), 1);
}

/// The scenario map that holds the DRT base: its helicopter takes off for an animal of a species it
/// recovers that is loose on the grounds, and darts it.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_helicopter_of_the_full_game_scenario_recovers_a_loose_animal() {
    use crate::{vfs::Vfs, zoo::Zoo};
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
    let map = "maps/scn24.zoo";
    if !crate::fixtures::has_member("the_helicopter_of_the_full_game", &vfs, map) {
        return;
    }
    let cat = Catalogue::load(&vfs).unwrap();
    let loaded = Zoo::parse_money_in(&vfs.read(map).unwrap(), &cat).unwrap();
    let mut run = Run::from_vfs(&vfs, &[], cat, loaded, 1, Some(false));
    let heli = run
        .world
        .zoo
        .entities
        .iter()
        .find_map(|e| e.common.as_ref().filter(|_| e.class == "helicopter"))
        .map(|c| c.id)
        .expect("the map holds a helicopter");
    let Some(Part::Staff(s)) = run.world.units.get(heli).map(|u| &u.part) else {
        panic!("the helicopter is no staff");
    };
    let base = run.world.find(s.heli.base).expect("the DRT base");
    let c = run.world.zoo.entities[base].common.as_ref().unwrap();
    let (bx, by) = (c.x.div_euclid(64), c.y.div_euclid(64));
    let darts = |run: &Run| {
        run.world.zoo.entities[base]
            .common
            .as_ref()
            .unwrap()
            .building
            .as_ref()
            .unwrap()
            .fill
    };
    assert_eq!(darts(&run), 0);
    // An old file stores no movement mode: the helicopter is free all the same.
    let mode = |run: &Run| {
        run.world.zoo.entities[run.world.find(heli).unwrap()]
            .common
            .as_ref()
            .map(|c| c.movement_mode)
    };
    assert_eq!(mode(&run), Some(1));
    // A stegosaurus (a species the helicopter recovers) on the nearest free tile of the grounds.
    let tiles = (-12..=12).flat_map(|dx| (-12..=12).map(move |dy| (bx + dx, by + dy)));
    let mut near: Vec<_> = tiles.collect();
    near.sort_by_key(|&(x, y)| (x - bx).abs().max((y - by).abs()));
    let free = near.into_iter().find(|&(x, y)| {
        let t = run.world.tile_index(x, y);
        t.is_some_and(|t| run.world.exhibits().region(t) == Some(crate::exhibit::GROUNDS))
            && run.apply(&Command::Animal("stego".into(), x, y, Some(false)))
    });
    assert!(free.is_some(), "a free tile of the grounds by the base");
    let id = run
        .world
        .zoo
        .entities
        .iter()
        .rev()
        .find(|e| e.type_name == "stego")
        .and_then(|e| e.common.as_ref())
        .map(|c| c.id)
        .unwrap();
    assert!(run.apply(&Command::Escape(id)));
    let mut steps = 0;
    while darts(&run) == 0 && steps < 6000 {
        run.step_with(67, Some(&vfs));
        steps += 1;
    }
    assert_eq!(
        darts(&run),
        1,
        "the helicopter darted the animal within {steps} steps"
    );
    assert!(matches!(&run.world.units.get(id).unwrap().part, Part::Animal(a) if a.tranq));
}

//! A save writes the behaviour set a started unit runs now, not the one its block was read with
//! (behaviours 4.7, zoo format 6.14).

use crate::entity::{Catalogue, EntityType};
use crate::unit::resume::load;
use crate::unit::testing::{centre, entity, world_with};
use crate::world::World;
use crate::zoo::{Level, UnitBlock};

/// A world with one keeper (id 1) whose block holds the set `bOld`.
fn keeper() -> World {
    let mut w = world_with((12, 4), |zoo| {
        zoo.entities
            .push(entity("keeper", "x", 1, centre((1, 1)), 0));
    });
    let mut block = UnitBlock::default();
    block.behaviour.levels[0] = Level {
        name: "bOld".into(),
        list_b: vec!["fPlay(old)".into()],
        tail: [1, 0, 1, 0],
        ..Default::default()
    };
    w.zoo.entities[0].common.as_mut().unwrap().unit = Some(Box::new(block));
    w
}

/// The block a save of `w` writes for its keeper.
fn saved_block(w: &World) -> UnitBlock {
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "keeper",
        type_name: b"x".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: None,
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    let mut zoo = w.saved(|_| true);
    w.finish_save(&mut zoo, &cat);
    *zoo.entities[0].common.take().unwrap().unit.unwrap()
}

/// A saved set: a top set with a nested one, the current call done in the nested one.
fn live() -> crate::zoo::Behaviour {
    let level = |name: &str, calls: &[&str], tail| Level {
        name: name.into(),
        list_b: calls.iter().map(|c| c.to_string()).collect(),
        tail,
        child: 1,
        ..Default::default()
    };
    crate::zoo::Behaviour {
        levels: vec![
            level("bTop", &["fPlaySet(bIn)", "fPlay(c)"], [1, 0, 1, 0]),
            level("bIn", &["fPlay(a)", "fPlay(d)"], [1, 1, 1, 0]),
        ],
    }
}

#[test]
fn a_save_writes_the_live_set_and_it_loads_back_the_same() {
    let mut w = keeper();
    w.units.get_mut(1).unwrap().started = true;
    // The unit has not changed its set: the block it was read with is written as it was.
    assert_eq!(saved_block(&w).behaviour.levels[0].name, "bOld");
    // The unit runs another set now.
    assert!(w.units.get_mut(1).unwrap().runner.restore(&live()));
    let block = saved_block(&w);
    let (top, nested) = (&block.behaviour.levels[0], &block.behaviour.levels[1]);
    assert_eq!(top.list_a, ["bTop", "bIn"]);
    assert_eq!((top.name.as_str(), top.child), ("bTop", 1));
    assert_eq!(top.list_b, ["fPlaySet(bIn)", "fPlay(c)"]);
    assert_eq!((nested.name.as_str(), nested.child), ("bIn", 0));
    assert_eq!(nested.list_b, ["fPlay(a)", "fPlay(d)"]);
    assert_eq!(
        (nested.tail, nested.text.as_str()),
        ([1, 1, 1, 0], "fPlay(a)")
    );
    // A load of that block runs the same set.
    let mut loaded = keeper();
    loaded.zoo.entities[0].common.as_mut().unwrap().unit = Some(Box::new(block));
    load(&mut loaded, 0);
    assert_eq!(
        loaded.units.get(1).unwrap().runner,
        w.units.get(1).unwrap().runner
    );
    // A unit that has not started keeps its block.
    let mut w = keeper();
    w.units.get_mut(1).unwrap().runner.restore(&live());
    assert_eq!(saved_block(&w).behaviour.levels[0].name, "bOld");
}

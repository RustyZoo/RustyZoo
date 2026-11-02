//! The cost rules that read the live units: other animals, avoided edges, shows, random walks,
//! food, the building flag, a guest's water states and predators (pathfinding 4 to 6).

use super::*;

/// A unit entity on a tile, with its id.
fn who(class: &str, kind: &str, at: (i32, i32), id: u32) -> Entity {
    let mut e = entity(class, kind, "", at, 0);
    e.common.as_mut().unwrap().id = id;
    e
}

fn animal(at: (i32, i32), id: u32) -> Mover {
    Mover {
        id: Some(id),
        ..unit(Kind::Animal, at)
    }
}

#[test]
fn a_crowded_tile_is_closed_to_an_animal() {
    let w = world(|_, e| {
        e.push(who("animals", "", (3, 2), 7));
        e.push(who("animals", "soft", (2, 3), 8));
        e.push(who("animals", "", (5, 5), 9));
        e.push(who("animals", "", (6, 5), 10));
    });
    let (here, east, south) = ((2, 2), t((3, 2)), t((2, 3)));
    let m = animal(here, 1);
    assert_eq!(terrain(&w, &m, east, None), MAX);
    assert_eq!(terrain(&w, &m, south, None), 1, "a walkable animal");
    let chasing = Mover {
        chasing: Some(7),
        ..m.clone()
    };
    assert_eq!(terrain(&w, &chasing, east, None), 1);
    let other = Mover {
        chasing: Some(99),
        ..m.clone()
    };
    assert_eq!(terrain(&w, &other, east, None), MAX);
    // The unit on the animal's own tile does not count, and nor does the animal itself.
    let crowd = animal((5, 5), 1);
    assert_eq!(terrain(&w, &crowd, t((6, 5)), None), 1);
    let me = animal((5, 5), 9);
    assert_eq!(terrain(&w, &me, t((6, 5)), None), MAX);
    // A search goes round the animal.
    assert!(!path(&w, &m, (4, 2)).contains(&(3, 2)));
}

#[test]
fn an_animal_chasing_a_unit_may_enter_its_crowded_tile() {
    let w = world(|_, e| {
        e.push(who("animals", "", (3, 2), 7));
        e.push(who("guests", "", (3, 2), 11));
        e.push(who("animals", "soft", (2, 3), 8));
        e.push(who("guests", "", (2, 3), 12));
    });
    let chasing = |id| Mover {
        chasing: Some(id),
        ..animal((2, 2), 1)
    };
    let (east, south) = (t((3, 2)), t((2, 3)));
    // Whoever else is there, the chased unit's tile is open to the chaser.
    assert_eq!(terrain(&w, &chasing(7), east, None), 1);
    assert_eq!(terrain(&w, &chasing(11), east, None), 1);
    assert_eq!(terrain(&w, &chasing(99), east, None), MAX);
    // The chased unit may be one that lets others through.
    assert_eq!(terrain(&w, &chasing(8), south, None), 1);
    assert_eq!(terrain(&w, &chasing(7), south, None), MAX);
}

#[test]
fn an_animal_keeps_off_the_fences_it_avoids() {
    let w = world(|_, e| {
        for y in 4..=6 {
            e.push(entity("fences", "", "f", (4, y), 6));
        }
    });
    let m = Mover {
        avoid_edges: 2,
        ..animal((1, 5), 1)
    };
    let near = t((3, 5));
    assert_eq!(terrain(&w, &m, near, None), MAX);
    assert_eq!(terrain(&w, &m, t((2, 5)), None), 1);
    let off = |f: fn(&mut Mover)| {
        let mut m = m.clone();
        f(&mut m);
        terrain(&w, &m, near, None)
    };
    assert_eq!(off(|m| m.avoid_edges = 1), 1, "r 1 looks at the tile only");
    assert_eq!(off(|m| m.avoid_edges = 0), 1);
    assert_eq!(
        off(|m| m.avoid_edges = u32::MAX),
        1,
        "a huge radius reaches the own tile"
    );
    assert_eq!(off(|m| m.test = true), 1, "the reachability test");
    assert_eq!(off(|m| m.show = true), 1, "a show tank trip");
    assert_eq!(off(|m| m.at = Some(t((3, 6)))), 1, "already near an edge");
    assert_eq!(off(|m| m.dest = Some(t((8, 8)))), MAX, "both goals on land");
}

#[test]
fn a_show_trip_crosses_the_gate_of_a_show_tank() {
    let tank = |kind: &str| {
        world(|_, e| {
            let walls = [(4, 4, 0), (4, 4, 6), (4, 4, 4), (4, 4, 2)];
            for (x, y, side) in walls {
                let gate = if side == 6 { "g" } else { "f" };
                e.push(entity("tankwall", kind, gate, (x, y), side));
            }
        })
    };
    let m = Mover {
        underwater: true,
        ..animal((3, 4), 1)
    };
    let going = Mover {
        show: true,
        ..m.clone()
    };
    let (show, plain) = (tank("show"), tank(""));
    assert!(show.tank_at(t((4, 4))).is_some());
    assert_eq!(cost::edge(&show, &m, t((3, 4)), 2), MAX);
    assert_eq!(cost::edge(&show, &going, t((3, 4)), 2), 1);
    assert_eq!(cost::edge(&plain, &going, t((3, 4)), 2), MAX);
    // The other sides stay closed.
    assert_eq!(cost::edge(&show, &going, t((4, 3)), 4), MAX);
}

#[test]
fn a_show_trip_pays_no_terrain_extras() {
    let w = world(|_, e| paths(e, &[(3, 3)]));
    let m = animal((2, 3), 1);
    assert_eq!(terrain(&w, &m, t((3, 3)), None), 16);
    let show = Mover { show: true, ..m };
    assert_eq!(terrain(&w, &show, t((3, 3)), None), 1);
}

#[test]
fn a_random_walk_pays_for_turning_on_its_own_tile() {
    let w = world(|_, _| {});
    let walk = |facing| Mover {
        turn: Some(facing),
        ..unit(Kind::Keeper, (5, 5))
    };
    let from = t((5, 5));
    let edges = |m: &Mover| [0, 2, 4, 6].map(|d| cost::edge(&w, m, from, d));
    assert_eq!(edges(&walk(0)), [1, 2, 4, 2]);
    assert_eq!(edges(&walk(2)), [2, 1, 2, 4]);
    assert_eq!(edges(&walk(7)), [1, 4, 4, 1], "an odd facing");
    assert_eq!(edges(&unit(Kind::Keeper, (5, 5))), [1; 4]);
    // Only from the unit's own tile.
    assert_eq!(cost::edge(&w, &walk(0), t((5, 4)), 4), 1);
    // The marks come after: a low fence on the south side adds 15 to the turn.
    let low = world(|_, e| e.push(entity("fences", "low", "f", (5, 6), 0)));
    assert_eq!(cost::edge(&low, &walk(0), from, 4), 4 + 15);
    assert_eq!(cost::edge(&low, &walk(4), from, 4), 1 + 15);
    // A diagonal step is the worst of its four edges.
    assert_eq!(cost::edge(&low, &walk(0), from, 3), 4 + 15);
    assert_eq!(cost::edge(&w, &walk(0), from, 1), 2);
}

#[test]
fn a_keeper_pays_16_for_a_tile_with_food() {
    let w = world(|_, e| {
        e.extend([(2, 2), (2, 3)].map(|p| {
            let mut e = entity_at("food", "", "", (p.0 * 64 + 16, p.1 * 64 + 16), 0);
            e.common.as_mut().unwrap().food = 1000;
            e
        }));
        for q in [(48, 16), (16, 48), (48, 48)] {
            e.push(entity_at("bush", "", "", (2 * 64 + q.0, 2 * 64 + q.1), 0));
        }
    });
    let m = unit(Kind::Keeper, (1, 1));
    assert_eq!(terrain(&w, &m, t((2, 3)), None), 16, "food on open ground");
    assert_eq!(terrain(&w, &m, t((2, 2)), None), 16, "food in a full tile");
    let relaxed = Mover { relaxed: true, ..m };
    assert_eq!(terrain(&w, &relaxed, t((2, 3)), None), 1);
}

#[test]
fn a_keepers_food_tile_stops_counting_when_the_units_reach_0() {
    let tile = (2, 3);
    let units = |food: i32| {
        let mut e = entity_at("food", "", "", (tile.0 * 64 + 16, tile.1 * 64 + 16), 0);
        e.common.as_mut().unwrap().food = food;
        world(|_, es| es.push(e))
    };
    let m = unit(Kind::Keeper, (1, 1));
    let cost = |food| terrain(&units(food), &m, t(tile), None);
    assert_eq!(
        (cost(1000), cost(1), cost(0)),
        (16, 16, 2),
        "eaten down to 0"
    );
}

#[test]
fn a_unit_at_a_building_looks_at_the_edge_cost_only() {
    // Water costs a keeper 20: no step into it, but one that goes in or out of a building.
    let mut w = world(|tiles, _| tiles[t((4, 5))].terrain = 1);
    w.set_water(vec![false, true]);
    let (s, step) = (Settings::default(), (t((3, 5)), t((4, 5)), 2));
    let m = unit(Kind::Keeper, (3, 5));
    assert!(!allowed(&w, &m, &s, step, false));
    assert!(allowed(&w, &m, &s, step, true));
    let inside = Mover {
        building: true,
        ..m
    };
    assert!(allowed(&w, &inside, &s, step, false));
    assert!(
        path(&w, &inside, (4, 5)).len() == 2,
        "the search takes the step too"
    );
    // A fence still closes it.
    let fenced = world(|tiles, e| {
        tiles[t((4, 5))].terrain = 1;
        e.push(entity("fences", "", "f", (4, 5), 6));
    });
    assert!(!allowed(&fenced, &inside, &s, step, true));
}

#[test]
fn a_guest_in_and_out_of_water() {
    let mut w = world(|tiles, _| (3..=5).for_each(|x| tiles[t((x, 3))].terrain = 1));
    w.set_water(vec![false, true]);
    let guest = |at| Mover {
        ..unit(Kind::Guest, at)
    };
    let dry = guest((2, 3));
    // From dry ground into the water: closed, unless recovering or avoiding a predator.
    assert_eq!(terrain(&w, &dry, t((3, 3)), None), MAX);
    for m in [
        Mover {
            recovering: true,
            ..dry.clone()
        },
        Mover {
            avoiding: true,
            ..dry.clone()
        },
    ] {
        assert_eq!(terrain(&w, &m, t((3, 3)), None), 1);
    }
    // From the water: water costs 1; a guest travelling in water may not step on dry ground,
    // unless it enters a building or recovers.
    let wet = guest((3, 3));
    assert_eq!(terrain(&w, &wet, t((4, 3)), None), 1);
    assert_eq!(terrain(&w, &wet, t((3, 4)), None), 7);
    let swimming = Mover {
        in_water: true,
        ..wet
    };
    assert_eq!(terrain(&w, &swimming, t((3, 4)), None), MAX);
    for m in [
        Mover {
            building: true,
            ..swimming.clone()
        },
        Mover {
            recovering: true,
            ..swimming.clone()
        },
    ] {
        assert_eq!(terrain(&w, &m, t((3, 4)), None), 7);
    }
}

#[test]
fn a_guest_avoiding_a_predator_pays_the_emergency_cost_and_leaves_the_paths() {
    let w = world(|_, e| paths(e, &[(1, 1), (2, 1)]));
    let guest = unit(Kind::Guest, (1, 1));
    let goal = Some(t((2, 1)));
    assert_eq!(terrain(&w, &guest, t((1, 2)), goal), MAX);
    let fleeing = Mover {
        avoiding: true,
        ..guest.clone()
    };
    assert_eq!(terrain(&w, &fleeing, t((1, 2)), goal), 1);
    let slow = Mover {
        avoiding: false,
        ..fleeing
    };
    assert_eq!(terrain(&w, &slow, t((1, 2)), None), 7);
    // An afraid guest on its way out keeps the emergency cost.
    let out = Mover {
        leaving: true,
        emergency: true,
        ..slow
    };
    assert_eq!(terrain(&w, &out, t((1, 2)), None), 1);
}

#[test]
fn a_tile_with_a_predator_costs_a_guest_at_least_16() {
    let w = world(|_, e| {
        e.push(who("animals", "", (3, 2), 40));
        e.push(who("animals", "", (4, 2), 41));
    });
    let guest = unit(Kind::Guest, (2, 2));
    assert_eq!(terrain(&w, &guest, t((3, 2)), None), 7);
    let afraid = Mover {
        predators: vec![40],
        ..guest
    };
    assert_eq!(terrain(&w, &afraid, t((3, 2)), None), 16);
    assert_eq!(
        terrain(&w, &afraid, t((4, 2)), None),
        7,
        "not a predator for this guest"
    );
}

#[test]
fn a_request_stopped_before_the_search_keeps_the_switch() {
    let w = world(|_, _| {});
    let mut finder = Finder::default();
    finder.request(&w, &unit(Kind::Keeper, (1, 1)), t((4, 4)), false);
    assert!(finder.last().diagonal);
    let guest = unit(Kind::Guest, (1, 1));
    finder.request(&w, &guest, t((1, 1)), false);
    finder.request(&w, &guest, 10_000, false);
    assert!(finder.last().diagonal, "same tile, off the map");
    finder.request(&w, &guest, t((4, 4)), false);
    assert!(!finder.last().diagonal, "a search sets it");
}

#[test]
fn a_guest_ignores_its_type_flags_for_water_and_tanks() {
    let mut w = world(|tiles, _| tiles[t((3, 3))].terrain = 1);
    w.set_water(vec![false, true]);
    // `swims` and `underwater` are false, as the unit code builds a guest.
    let dry = unit(Kind::Guest, (2, 3));
    assert!(!dry.swims && !dry.underwater);
    assert_eq!(terrain(&w, &dry, t((3, 3)), None), MAX, "dry to water");
    let recovering = Mover {
        recovering: true,
        ..dry.clone()
    };
    assert_eq!(terrain(&w, &recovering, t((3, 3)), None), 1);
    let wet = unit(Kind::Guest, (3, 3));
    assert_eq!(terrain(&w, &wet, t((4, 3)), None), 7);
    // Only a tile outside the zoo is closed to a guest by 6.1.
    let outside = world(|_, _| {});
    assert_eq!(terrain(&outside, &dry, t((9, 9)), None), 7);
}

#[test]
fn the_last_request_sets_the_diagonal_switch_of_the_gate_choice() {
    let w = world(|_, _| {});
    let mut finder = Finder::default();
    assert!(
        finder.last().diagonal,
        "before the first request: AllowDiagonalMoves"
    );
    finder.request(&w, &unit(Kind::Guest, (1, 1)), t((4, 4)), false);
    assert!(!finder.last().diagonal);
    finder.request(&w, &unit(Kind::Keeper, (1, 1)), t((4, 4)), false);
    assert!(finder.last().diagonal);
    finder.request(
        &w,
        &Mover {
            test: true,
            ..unit(Kind::Animal, (1, 1))
        },
        t((4, 4)),
        false,
    );
    assert!(
        !finder.last().diagonal,
        "an animal in the reachability test"
    );
    let mut off = Finder::new(Settings {
        diagonal: false,
        ..Settings::default()
    });
    off.request(&w, &unit(Kind::Keeper, (1, 1)), t((4, 4)), false);
    assert!(!off.last().diagonal);
}

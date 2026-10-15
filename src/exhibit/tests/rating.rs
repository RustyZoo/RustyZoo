//! The rating on synthetic exhibits: the worked examples of habitat rating 16 (a flat grass
//! exhibit with two zebras, then one feature at a time), and the timers.

use crate::exhibit::rating::{Input, Item, Likes, Record, Site, State, Water, bit, rate};

fn zebra() -> Likes {
    let mut terrain = [0; 18];
    for (k, w) in [(1, 95), (9, 5), (8, -50), (10, -20), (16, -5)] {
        terrain[k] = w;
    }
    Likes {
        key: "zebra".into(),
        subtype: "m".into(),
        terrain,
        objects: vec![
            (9400, 3),
            (9410, -15),
            (7058, 3),
            (8104, 20),
            (9205, 2),
            (9414, -100),
        ],
        trees: 2,
        rocks: 2,
        shelter: true,
        attractiveness: 5,
        ..Likes::default()
    }
}

/// An item of a kind: `f` foliage, `r` rock, `s` shelter for two, `t` toy for two.
fn item(habitat: i32, name: i32, slots: i32, kind: char) -> Item {
    Item {
        habitat,
        name,
        slots,
        foliage: kind == 'f',
        rock: kind == 'r',
        rooms: match kind {
            's' => Some((2, 0)),
            't' => Some((0, 2)),
            _ => None,
        },
        ..Item::default()
    }
}

/// 25 tiles of grass, two zebras.
fn flat() -> Input {
    let mut site = Site {
        tiles: 25,
        ..Site::default()
    };
    site.terrain[0] = 25;
    site.slots = vec![Vec::new(); 25];
    Input {
        site,
        species: vec![zebra()],
        animals: vec![0, 0],
    }
}

fn only(input: &Input) -> Record {
    let mut records = rate(input);
    assert_eq!(records.len(), 1);
    records.remove(0)
}

fn put(input: &mut Input, it: Item, tiles: &[usize]) {
    input.site.items.push(it);
    let at = input.site.items.len() - 1;
    for &t in tiles {
        input.site.slots[t].push(at);
    }
}

#[test]
fn flat_grass_with_two_zebras() {
    let r = only(&flat());
    assert_eq!((r.animals, r.terrain, r.objects), (2, 0.0, 0.0));
    assert_eq!(
        (r.foliage, r.rocks, r.elevation, r.shelters, r.toys),
        (84.0, 88.0, 100.0, 100.0, 100.0)
    );
    assert!((r.rating() - 47.2).abs() < 1e-4);
    assert_eq!(r.shown(), 48);
    for b in [bit::FEW_TREES, bit::FEW_ROCKS, bit::NEEDS_SHELTER] {
        assert!(r.has(b), "bit {b}");
    }
    for b in [bit::FEW_TREES, bit::FEW_ROCKS, bit::NEEDS_SHELTER] {
        assert_eq!(r.important >> b & 1, 0, "bit {b} is not important");
    }
    // The savannah grass painted on all 25 tiles adds 0.3 x 95.
    let mut grass = flat();
    grass.site.terrain = [0, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(only(&grass).shown(), 76);
}

#[test]
fn a_disliked_terrain_is_too_much_but_never_important() {
    // Terrain 8 is valued below 0: present on 20 of 25 tiles, it shows "too much" only.
    let mut input = flat();
    input.site.terrain = [5, 0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let r = only(&input);
    let b = bit::terrain(8) + 1;
    assert!(r.has(b));
    assert_eq!(r.important >> b & 1, 0);
    // The liked savannah grass missing from the same exhibit is important.
    let b = bit::terrain(1);
    assert!(r.has(b));
    assert_eq!(r.important >> b & 1, 1);
}

#[test]
fn one_feature_at_a_time() {
    let mut input = flat();
    let shown = |input: &Input| {
        let r = only(input);
        (r.rating(), r.shown())
    };
    // The acacia (habitat 9400 + name 7058: value 6), a small rock (value 2).
    put(&mut input, item(9400, 7058, 1, 'f'), &[3]);
    let (rating, s) = shown(&input);
    assert!((rating - 48.4).abs() < 1e-4 && s == 49);
    assert!(
        !only(&input).has(bit::FEW_TREES),
        "4 percent of foliage is close enough"
    );
    put(&mut input, item(9411, 9205, 1, 'r'), &[4]);
    let (rating, s) = shown(&input);
    assert!((rating - 49.4).abs() < 1e-4 && s == 50);
    // One corner point raised: four tiles slope by one step.
    input.site.hills = 4;
    let (rating, s) = shown(&input);
    assert!((rating - 48.6).abs() < 1e-4 && s == 49);
    // A lean-to of 8 slots, value 5: 496 in the tile pass, 4 x 62 in the claims (original bug).
    put(
        &mut input,
        item(9410, 8104, 8, 's'),
        &[10, 10, 10, 10, 11, 11, 11, 11],
    );
    input.site.listed = vec![2];
    let r = only(&input);
    assert!((r.objects - 15.44).abs() < 1e-4);
    assert!((r.rating() - 50.088).abs() < 1e-3 && r.shown() == 51);
    assert!(!r.has(bit::NEEDS_SHELTER));
    // A swinging log (value -15, 4 slots): the sum turns negative and is multiplied by 50.
    put(&mut input, item(9410, 6100, 4, 't'), &[12, 12, 12, 12]);
    input.site.listed = vec![2, 3];
    let r = only(&input);
    assert!(r.objects < -100.0 && r.rating() < 0.0);
    assert_eq!(r.shown(), 0);
}

#[test]
fn a_path_tile_costs_ten_points() {
    let mut input = flat();
    input.site.paths = vec![(9414, 0)];
    let r = only(&input);
    assert_eq!(r.objects, -50.0);
    assert!((r.rating() - 37.2).abs() < 1e-4);
}

#[test]
fn shelters_toys_and_stink_follow_the_counts() {
    // Four zebras and no shelter: 15 points each beyond three.
    let mut input = flat();
    input.animals = vec![0; 4];
    let r = only(&input);
    assert_eq!(r.shelters, 40.0);
    // Three unused lean-tos are worth 250 points.
    let mut input = flat();
    input.animals = Vec::new();
    for k in 0..3 {
        put(&mut input, item(9410, 8104, 1, 's'), &[k]);
    }
    input.site.listed = vec![0, 1, 2];
    let r = only(&input);
    assert_eq!(r.animals, 0);
    assert!(r.has(bit::UNUSED_SHELTER) && r.shelters == -150.0);
    assert_eq!(
        r.important & 1 << bit::UNUSED_SHELTER,
        0,
        "three are not far off"
    );
    // Over 5 percent of the tiles is some stink, over 10 percent much.
    for (tiles, shown, much) in [(1, false, false), (2, true, false), (3, true, true)] {
        let mut input = flat();
        input.site.stinky = tiles;
        let r = only(&input);
        assert_eq!(
            (r.has(bit::STINK), r.important >> bit::STINK & 1 == 1),
            (shown, much)
        );
    }
}

#[test]
fn a_tank_is_rated_by_its_depth_and_water() {
    let mut input = flat();
    let mut likes = zebra();
    likes.depth = (3, 5);
    input.species = vec![likes];
    input.site.water = Some(Water {
        level: 1,
        purity: 40,
        show: false,
    });
    let r = only(&input);
    assert_eq!((r.terrain, r.depth, r.water), (100.0, 60.0, 40.0));
    assert!(r.has(bit::TOO_SHALLOW) && r.has(bit::DIRTY_WATER));
    assert_eq!(
        r.important >> bit::TOO_SHALLOW & 1,
        0,
        "40 points are not far off"
    );
    input.site.water = Some(Water {
        level: 9,
        purity: 100,
        show: false,
    });
    let r = only(&input);
    assert!(r.has(bit::TOO_DEEP) && r.depth == 20.0);
    assert_eq!(r.important >> bit::TOO_DEEP & 1, 1);
}

/// A generator that hands out the given draws and fails on one more.
fn draws(list: &'static [u32]) -> impl FnMut() -> u32 {
    let mut list = list.iter().copied();
    move || list.next().expect("more draws than the rules make")
}

fn ambient_zebras() -> Input {
    Input {
        species: vec![Likes {
            ambient: true,
            ..zebra()
        }],
        ..flat()
    }
}

#[test]
fn a_new_exhibit_is_rated_at_the_first_update_and_the_timer_restarts_at_random() {
    let mut state = State::new();
    state.update(67, &mut draws(&[4321]), flat);
    assert!(!state.dirty && state.records.len() == 1);
    assert_eq!((state.rating_ms, state.species_ms), (4321 % 200, 67));
    assert_eq!(state.attractiveness, 10);
    // Nothing is due until the timer passes 6999: no draw, and the description is not built.
    state.update(6878, &mut draws(&[]), || unreachable!());
    assert_eq!(state.rating_ms, 6999);
    state.update(1, &mut draws(&[399]), flat);
    assert_eq!(state.rating_ms, 399 % 200);
}

#[test]
fn the_species_list_is_revised_after_eight_seconds_and_draws_a_tile_for_a_sound() {
    let mut state = State {
        species_ms: 7990,
        ..State::default()
    };
    state.update(10, &mut draws(&[1234, 77]), ambient_zebras);
    assert_eq!(state.species_ms, 1234 % 200);
    assert_eq!(
        (state.types.clone(), state.sounds.clone()),
        (
            vec!["zebra/m".to_string()],
            vec![("zebra/m".to_string(), 77 % 25)]
        )
    );
    assert!(
        state.records.is_empty(),
        "the records wait for their own timer"
    );
    // The type is known now: no second sound. Both are due: the list first, the records last.
    let mut state = State {
        species_ms: 7990,
        rating_ms: 6995,
        ..state
    };
    state.update(10, &mut draws(&[9, 8]), ambient_zebras);
    assert_eq!((state.species_ms, state.rating_ms), (9, 8));
    assert_eq!(state.sounds.len(), 1);
    // The type left: its sound goes.
    state.species_ms = 8000;
    state.update(0, &mut draws(&[0]), Input::default);
    assert!(state.types.is_empty() && state.sounds.is_empty());
}

#[test]
fn extreme_values_neither_panic_nor_draw() {
    let mut input = flat();
    input.species[0].objects = vec![(9400, i32::MAX), (9410, i32::MIN)];
    input.species[0].depth = (i32::MIN, i32::MAX - 1);
    input.species[0].trees = i32::MAX;
    put(&mut input, item(9400, 9410, i32::MAX, 'f'), &[0, 0]);
    put(&mut input, item(9410, 0, 0, 's'), &[1]);
    input.site.listed = vec![1];
    input.site.water = Some(Water {
        level: i32::MAX,
        purity: -5,
        show: true,
    });
    input.animals.push(7); // a type that is not in the list
    assert_eq!(only(&input).animals, 2);
    // No tiles, no animals, no species; the clock at its limit.
    assert!(rate(&Input::default()).is_empty());
    let mut state = State::new();
    state.update(u32::MAX, &mut draws(&[1, 2, 3]), ambient_zebras);
    state.update(u32::MAX, &mut draws(&[4, 5]), Input::default);
    assert_eq!(state.rating_ms, 5);
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_likes_of_the_plains_zebra_in_the_full_game() {
    use crate::entity::Catalogue;
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let likes = |sub: &[u8]| Likes::of(catalogue.find("animals", b"zebra", sub).unwrap());
    let (adult, young) = (likes(b"m"), likes(b"y"));
    assert_eq!(
        (adult.terrain[1], adult.terrain[9], adult.terrain[8]),
        (95, 5, -50)
    );
    assert_eq!((adult.trees, adult.rocks, adult.elevation), (2, 2, 0));
    assert!(adult.shelter && !adult.toys);
    assert_eq!(
        adult.objects.iter().find(|p| p.0 == 9410),
        Some(&(9410, -15))
    );
    assert_eq!((adult.attractiveness, young.attractiveness), (5, 15));
}

#[test]
fn the_other_animals_sum_adds_name_genus_and_family_of_each_type_of_another_name() {
    let me = Likes {
        ids: [5004, 5104, 5204],
        compatible: vec![(1, -100), (2, -10), (3, 5), (9, 7), (2, -20)],
        ..zebra()
    };
    let lion = |subtype: &str| Likes {
        key: "lion".into(),
        subtype: subtype.into(),
        ids: [1, 2, 3],
        ..Likes::default()
    };
    // The other subtype of the zebra is skipped; the lion's two subtypes count twice, and the
    // last pair for an id wins.
    let kin = Likes {
        subtype: "f".into(),
        ..me.clone()
    };
    let input = Input {
        species: vec![me, kin, lion("m"), lion("f")],
        animals: vec![0, 2],
        ..flat()
    };
    let records = rate(&input);
    assert_eq!(
        (records[0].key.as_str(), records[0].other),
        ("zebra", -230.0)
    );
    assert_eq!((records[1].key.as_str(), records[1].other), ("lion", 0.0));
}

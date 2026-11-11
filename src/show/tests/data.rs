//! The show follow-ups on the expansion's `scn28`: a toy trick's need through a save (shows
//! 32.1) and the portal's frames (shows 32.6), with the orca, the orcaball and the portal walls
//! of the game or the expansion trial.

use crate::behaviour::{Data, Library, Timings};
use crate::entity::Catalogue;
use crate::goal::{GO_TO_SHOW, NONE, USE};
use crate::render::MapRenderer;
use crate::sim::drive::Run;
use crate::unit::Part;
use crate::unit::testing::{centre, entity};
use crate::vfs::Vfs;
use crate::world::World;
use crate::zoo::Zoo;

/// The show tank of `scn28` and the plain tank linked to it.
const SHOW: u32 = 2;
const PLAIN: u32 = 1;

fn open_install() -> Vfs {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    Vfs::open(std::path::Path::new(&dir)).unwrap()
}

/// A run of `scn28` whose world `edit` changed first.
fn run_of(vfs: &Vfs, cat: Catalogue, edit: impl FnOnce(&mut World, &MapRenderer)) -> Run {
    let (zoo, money) = Zoo::parse_money_in(&vfs.read("maps/scn28.zoo").unwrap(), &cat).unwrap();
    let renderer = crate::render::MapRenderer::new(vfs, &cat);
    let language = crate::strings::Language::of(vfs.root());
    let mut world = World::with_language(zoo, |e| renderer.shape(e), language);
    world.set_water(crate::place::water(&crate::terrain::types(vfs)));
    world.set_tank_config(crate::exhibit::TankConfig::load(vfs));
    world.units.show = crate::show::Params::load(vfs);
    edit(&mut world, &renderer);
    let types = (cat, crate::animal::Class::load(vfs));
    Run::start((vfs.ini(), &[]), types, world, money, 1, false)
}

/// The first tile of exhibit `x` that is free of walls.
fn tile_in(world: &World, x: u32, skip: usize) -> usize {
    (0..world.zoo.tiles.len())
        .filter(|&t| world.exhibits().region(t) == Some(x))
        .nth(skip)
        .unwrap()
}

/// Adds the entity `class`/`kind`/`subtype` on the middle of tile `t`; its id.
fn add(
    world: &mut World,
    renderer: &MapRenderer,
    (class, kind, subtype): (&str, &str, &str),
    t: usize,
) -> u32 {
    let width = world.zoo.width as usize;
    let mut e = entity(
        class,
        kind,
        0,
        centre(((t % width) as i32, (t / width) as i32)),
        0,
    );
    e.subtype = subtype.into();
    let shape = renderer.shape(&e);
    world.add(e, shape).unwrap()
}

/// An orca in the show tank doing its trick at an orcaball: the toy trick of shows 23.4.
fn orca_at_its_ball(vfs: &Vfs) -> (Run, u32, u32) {
    let cat = Catalogue::load(vfs).unwrap();
    let ids = std::cell::Cell::new((0, 0));
    let mut run = run_of(vfs, cat, |w, r| {
        let ball = add(
            w,
            r,
            ("building", "building", "orcaball"),
            tile_in(w, SHOW, 10),
        );
        let orca = add(w, r, ("animals", "orca", "m"), tile_in(w, SHOW, 30));
        ids.set((orca, ball));
    });
    let (orca, ball) = ids.get();
    run.step(100);
    let u = run.world.units.get_mut(orca).unwrap();
    (u.started, u.typed) = (true, true);
    u.goal.record.task = USE;
    let Part::Animal(a) = &mut u.part else {
        panic!("no animal")
    };
    (a.building, a.need) = (ball, crate::guest::TOY_NEED);
    (run, orca, ball)
}

/// The run `run` saved and opened again as Continue does.
fn reload(vfs: &Vfs, run: &mut Run) -> Run {
    let cat = Catalogue::load(vfs).unwrap();
    let (zoo, money) = Zoo::parse_money_in(&run.save().unwrap(), &cat).unwrap();
    let renderer = MapRenderer::new(vfs, &cat);
    let language = crate::strings::Language::of(vfs.root());
    let mut world = World::with_language(zoo, |e| renderer.shape(e), language);
    world.set_water(crate::place::water(&crate::terrain::types(vfs)));
    world.set_tank_config(crate::exhibit::TankConfig::load(vfs));
    let types = (
        Catalogue::load(vfs).unwrap(),
        crate::animal::Class::load(vfs),
    );
    Run::resume((vfs.ini(), &[]), types, world, money, 1)
}

/// The data of the install for the runner.
fn lib_of<'a>(cat: &'a Catalogue, vfs: &'a Vfs, timings: &'a Timings) -> Library<'a> {
    Library {
        catalogue: cat,
        vfs,
        timings,
    }
}

/// The need byte of the unit block of entity `id` in a saved file.
fn saved_need(bytes: &[u8], id: u32) -> u8 {
    let zoo = Zoo::parse(bytes).unwrap();
    let e = zoo
        .entities
        .iter()
        .find(|e| e.common.as_ref().is_some_and(|c| c.id == id));
    e.and_then(|e| e.common.as_ref()?.unit.as_ref())
        .unwrap()
        .need
}

#[test]
#[ignore = "needs the game or the expansion trial: set ZT_DATA_DIR to the install folder"]
fn an_orca_in_an_orcaball_keeps_its_trick_need_through_a_save_a_reset_and_a_load() {
    // Shows 32.1 points 2, 3 and 5.
    let vfs = open_install();
    if !crate::fixtures::has_member("an_orca_in_an_orcaball", &vfs, "maps/scn28.zoo") {
        return;
    }
    let (mut run, orca, _) = orca_at_its_ball(&vfs);
    let toy = crate::guest::TOY_NEED;
    assert_eq!(saved_need(&run.save().unwrap(), orca), toy);
    // The animal's goal reset leaves the byte.
    crate::goal::stop_use(run.world.units.get_mut(orca).unwrap());
    assert_eq!(saved_need(&run.save().unwrap(), orca), toy);
    // A load gives it back whatever the task and the building are.
    let mut back = reload(&vfs, &mut run);
    let Part::Animal(a) = &back.world.units.get(orca).unwrap().part else {
        panic!("no animal")
    };
    assert_eq!(a.need, toy);
    assert_eq!(saved_need(&back.save().unwrap(), orca), toy);
}

/// The portal walls of the show tank of `scn28`: position and id of both pieces.
fn walls(world: &World) -> [(usize, u32); 2] {
    let p = world.exhibits().get(SHOW).unwrap().portals[0];
    [p.mine, p.theirs].map(|id| (world.find(id).unwrap(), id))
}

fn open_byte(world: &World, at: usize) -> bool {
    world.zoo.entities[at]
        .common
        .as_ref()
        .unwrap()
        .fence
        .unwrap()
        .open
}

/// The frame the wall `id` shows among the `n` of its animation.
fn frame(world: &World, id: u32, n: u32) -> u32 {
    let f = world.portal_frames().find(|f| f.0 == id).map(|f| f.1);
    f.map_or(0, |f| f.min(n - 1))
}

#[test]
#[ignore = "needs the game or the expansion trial: set ZT_DATA_DIR to the install folder"]
fn the_portal_of_scn28_opens_for_an_orca_on_its_way_to_the_show_and_closes_after() {
    // Shows 32.6 points 3 to 5, 8 and 9.
    let vfs = open_install();
    if !crate::fixtures::has_member("the_portal_of_scn28", &vfs, "maps/scn28.zoo") {
        return;
    }
    let cat = Catalogue::load(&vfs).unwrap();
    let ids = std::cell::Cell::new(0);
    let mut run = run_of(&vfs, cat, |w, r| {
        let orca = add(w, r, ("animals", "orca", "m"), tile_in(w, PLAIN, 12));
        ids.set(orca);
    });
    let orca = ids.get();
    let timings = Timings::default();
    let [first, second] = walls(&run.world);
    let timing = lib_of(&run.cat, &vfs, &timings)
        .timing(&run.world.zoo.entities[first.0], "portal")
        .unwrap();
    let (n, d) = (timing.count, timing.delay);
    assert!(n > 1 && d > 0, "{timing:?}");
    let target = tile_in(&run.world, SHOW, 12);
    let go = |run: &mut Run, task: u8| {
        let u = run.world.units.get_mut(orca).unwrap();
        u.goal.record.task = task;
        // The walk's start sets the record's target; the own target tile is the portal's (32.2).
        u.goal.record.target.tile = Some(target);
        u.own_tile = Some(target);
        crate::show::open_portals(&mut run.world, &run.cat, SHOW);
    };
    // The orca is called: both walls open, one open sound.
    go(&mut run, GO_TO_SHOW);
    assert!(open_byte(&run.world, first.0) && open_byte(&run.world, second.0));
    assert_eq!(run.world.units.take_sounds().len(), 1);
    let both = |run: &mut Run, steps: u32| {
        for _ in 0..steps {
            run.step_with(d, Some(&vfs));
        }
    };
    both(&mut run, n - 1);
    assert_eq!(frame(&run.world, first.1, n), n - 1);
    both(&mut run, 1);
    assert_eq!(frame(&run.world, second.1, n), n - 1);
    // Saved while open and loaded: byte 1, the last frame, nothing animating.
    let loaded = reload(&vfs, &mut run);
    let [a, _] = walls(&loaded.world);
    assert!(open_byte(&loaded.world, a.0));
    assert_eq!(frame(&loaded.world, a.1, n), n - 1);
    // Its task ends: the portal closes down to frame 0.
    go(&mut run, NONE);
    assert!(!open_byte(&run.world, first.0) && !open_byte(&run.world, second.0));
    both(&mut run, n);
    assert_eq!(
        (
            frame(&run.world, first.1, n),
            frame(&run.world, second.1, n)
        ),
        (0, 0)
    );
}

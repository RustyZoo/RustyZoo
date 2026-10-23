//! The draws of the fence heal at a start follow the seeds of a replay as they follow the seeds
//! of a window.

use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::menu::Start;
use crate::render::tests::{cache_install, flat_map, frame_file, tile_with};
use crate::sim::Rng;
use crate::vfs::Vfs;
use crate::zoo::{Common, Fence};

const SIZE: (u32, u32) = (64, 48);
const SEEDS: [u32; 2] = [5, 6];

fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    for sub in [b"f", b"g"] {
        let ini = "[Characteristics/Integers]\ncStrength=300\ncLife=3\n";
        cat.push(EntityType {
            class: "fences",
            type_name: b"iron".to_vec(),
            subtype: sub.to_vec(),
            file: None,
            config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

/// A map of version `version` with four fence pieces; `stored` gives them a condition.
fn map(version: u32, stored: bool) -> Zoo {
    let mut zoo = tile_with(&[]);
    (zoo.width, zoo.height, zoo.tiles, zoo.version) = (40, 40, flat_map(40, 40), version);
    for (n, (x, y)) in [(32, 0), (63, 32), (32, 63), (0, 32)]
        .into_iter()
        .enumerate()
    {
        zoo.entities.push(Entity {
            class: "fences".into(),
            type_name: "iron".into(),
            subtype: "f".into(),
            body_size: None,
            common: Some(Common {
                x: 20 * 64 + x,
                y: 20 * 64 + y,
                rotation: 2 * n as u32,
                id: n as u32 + 1,
                fence: stored.then_some(Fence {
                    strength: 9,
                    life: 1,
                    timers: [7; 2],
                    portal: false,
                }),
                ..Default::default()
            }),
        });
    }
    zoo.id_counter = 100;
    zoo
}

/// The wear intervals of the pieces, in file order.
fn timers(app: &App) -> Vec<[u32; 2]> {
    let zoo = &app.game.as_ref().unwrap().world.zoo;
    let pieces = zoo.entities.iter().filter_map(|e| e.common.as_ref());
    pieces.filter_map(|c| Some(c.fence?.timers)).collect()
}

/// The intervals the pieces draw one after the other from the generator seeded with the last
/// of the `seeds`: the first seeding is the start of the program, the second the map.
fn drawn(app: &App) -> Vec<[u32; 2]> {
    let game = app.game.as_ref().unwrap();
    let mut world = World::new(game.world.zoo.clone(), |_| None);
    world.set_decay(crate::world::Decay::load(app.renderer.vfs()));
    let mut rng = Rng::default();
    rng.seed(SEEDS[1]);
    (1..=4)
        .map(|id| [world.heal(app.renderer.catalogue(), id, &mut || rng.rand())])
        .map(|i| [i[0].unwrap(); 2])
        .collect()
}

fn start(reset_fences: bool) -> Start {
    Start {
        path: "scenario.zoo".into(),
        label: "scenario".into(),
        forget_last: true,
        freeform: false,
        cash: Default::default(),
        reset: false,
        reset_fences,
    }
}

#[test]
fn a_scenario_start_that_resets_the_fences_draws_them_from_the_seeds_of_the_replay() {
    let install = cache_install("app-replay-start", frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let cat = catalogue();
    let mut app = App::empty(
        MapRenderer::new(&vfs, &cat),
        Timers::new(66, 16, 200),
        SIZE,
        Settings::default(),
    )
    .unwrap();
    app.set_seeds(SEEDS.to_vec());
    app.open_game(map(106, true), Money::default()).unwrap();
    assert_eq!(timers(&app), vec![[7; 2]; 4], "stored until the start");
    app.begin(&start(true));
    let want = drawn(&app);
    assert!(want.iter().all(|t| t[0] != 7));
    assert_eq!(timers(&app), want);
}

#[test]
fn a_file_older_than_version_55_draws_its_fences_from_the_seeds_of_the_replay() {
    let install = cache_install("app-replay-old", frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let cat = catalogue();
    let mut app = App::new(
        MapRenderer::new(&vfs, &cat),
        map(54, false),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.set_seeds(SEEDS.to_vec());
    let want = drawn(&app);
    assert_eq!(timers(&app), want);
    // The log tells the seedings of the replay and then the intervals drawn from them, once.
    let mut told: Vec<String> = vec!["sim seed 5".into(), "sim seed 6".into()];
    told.extend((1..=4).map(|id| format!("sim fence {id} healthy {}", want[id - 1][0])));
    app.pass(0, &[]);
    let lines = app.log_lines(0, 0);
    let sim: Vec<_> = lines
        .iter()
        .filter_map(|l| l.strip_prefix("pass 0 t=0 "))
        .collect();
    let sim: Vec<_> = sim
        .into_iter()
        .filter(|l| l.starts_with("sim fence") || l.starts_with("sim seed"))
        .collect();
    assert_eq!(sim, told);
}

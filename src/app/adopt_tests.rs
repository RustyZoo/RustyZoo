//! Adopting animals through the log, on a made-up map with one pen.

use super::*;
use crate::camera::zoom_view;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::money::ANIMAL_PURCHASE;
use crate::render::order::project;
use crate::render::tests::{cache_install, flat_map, frame_file, tile_with};
use crate::sprite::tests::Install;
use crate::ui::draw::Strings as UiStrings;
use crate::ui::layout::Tree;
use crate::vfs::Vfs;
use crate::zoo::{Common, Fence};

const SIZE: (u32, u32) = (128, 96);

struct Texts;

impl UiStrings for Texts {
    fn string(&self, id: i32) -> String {
        format!("text{id}")
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

fn animal(subtype: &str, cost: u32) -> EntityType {
    let ini =
        format!("[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncPurchaseCost={cost}\n");
    EntityType {
        class: "animals",
        type_name: b"zebra".to_vec(),
        subtype: subtype.into(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 5004,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// A map of 40 by 40 tiles with a pen of one tile at (20, 20).
fn map() -> Zoo {
    let mut zoo = tile_with(&[]);
    (zoo.width, zoo.height, zoo.tiles) = (40, 40, flat_map(40, 40));
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
                z: 0,
                rotation: 2 * n as u32,
                id: n as u32 + 1,
                name: String::new(),
                colors: Vec::new(),
                fence: Some(Fence {
                    strength: 300,
                    life: 10,
                    timers: [0; 2],
                    portal: false,
                }),
                ..Default::default()
            }),
        });
    }
    zoo.id_counter = 100;
    let (px, py) = project(0, 1280, 1280, 0);
    zoo.view.scroll = (px - SIZE.0 as i32 / 2, py - SIZE.1 as i32 / 2);
    zoo
}

/// The window point of the world point (`wx`, `wy`).
fn point(camera: &Camera, wx: i32, wy: i32) -> Command {
    let (px, py) = zoom_view(camera.zoom, project(camera.rotation, wx, wy, 0));
    let (x, y) = (
        px - i64::from(camera.scroll.0),
        py - i64::from(camera.scroll.1),
    );
    Command::Mouse(Mouse::Move(x as i32, y as i32))
}

/// Runs `test` on a game with the pen map, the fences, a zebra (2) and a zookeeper (3) in the
/// catalogue, 2000 in cash, and the clock stopped.
fn with_app(name: &str, test: impl FnOnce(&mut App)) {
    let install: Install = cache_install(name, frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let mut catalogue = Catalogue::default();
    for sub in [b"f", b"g"] {
        let ini = "[HabitatFence]\ncHeight=1\n[Characteristics/Integers]\ncPurchaseCost=125\n";
        catalogue.push(EntityType {
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
    catalogue.push(animal("m", 800));
    let ini = "[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncPurchaseCost=800\n";
    for sub in ["m", "f"] {
        catalogue.push(EntityType {
            class: "keeper",
            type_name: b"keeper".to_vec(),
            subtype: sub.into(),
            file: None,
            config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
            name_id: 5500,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    let tree = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n[a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n";
    let files = Install::new("app-adopt-ui", &[("ui/root.lyt", tree.as_bytes().to_vec())]);
    let mut tree = Tree::load(&Vfs::open(&files.0).unwrap(), "ui/root.lyt", 0);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    app.set_interface(Interface::new(tree, Box::new(Texts)));
    app.set_money(Money {
        cash: 2000.0,
        ..Money::default()
    });
    app.pass(0, &[]);
    test(&mut app);
}

/// The log lines of the pass `pass` that are about the world, the tool or the money.
fn said(app: &App, pass: u64) -> Vec<String> {
    let lead = format!("pass {pass} t={pass} ");
    app.log_lines(pass, pass)
        .iter()
        .filter_map(|l| l.strip_prefix(&lead))
        .filter(|l| {
            ["world ", "tool message", "tool refused", "money book"]
                .iter()
                .any(|w| l.starts_with(w))
        })
        .map(String::from)
        .collect()
}

fn click(app: &mut App, pass: u64, to: Command) -> Vec<String> {
    app.pass(pass, &[to]);
    app.pass(pass + 1, &[Command::Mouse(Mouse::Down)]);
    app.pass(pass + 2, &[Command::Mouse(Mouse::Up)]);
    said(app, pass + 2)
}

#[test]
fn an_animal_is_adopted_into_the_pen_named_booked_and_refused_on_the_grounds() {
    with_app("app-adopt", |app| {
        app.game.as_mut().unwrap().build.hold(2);
        let camera = app.camera().unwrap().clone();
        let at = |lx: i32, ly: i32| point(&camera, 20 * 64 + lx, 20 * 64 + ly);
        // Two animals in the pen, one in the first quarter of the tile and one in the last.
        let first = click(app, 10, at(10, 10));
        assert_eq!(
            first,
            [
                "world add animals/zebra/m 20,20 id 101 name=text5004 1",
                "money book 2 800"
            ]
        );
        let second = click(app, 20, at(50, 50));
        assert_eq!(
            second,
            [
                "world add animals/zebra/m 20,20 id 102 name=text5004 2",
                "money book 2 800"
            ]
        );
        let game = app.game.as_ref().unwrap();
        assert_eq!(
            (
                game.money.cash,
                game.money.total[ANIMAL_PURCHASE],
                game.world.units.len()
            ),
            (400.0, 1600.0, 2)
        );
        // A third does not fit the cash (10002), and the grounds refuse it (10108).
        assert_eq!(click(app, 30, at(50, 10)), ["tool message 10002"]);
        let grounds = point(&camera, 19 * 64 + 32, 20 * 64 + 32);
        assert_eq!(click(app, 40, grounds), ["tool message 10108"]);
        assert_eq!(app.money().unwrap().cash, 400.0);
    });
}

#[test]
fn a_staff_member_is_hired_anywhere_booked_in_the_wages_and_the_next_sex_is_drawn() {
    with_app("app-hire", |app| {
        // The clock stands still, so the only draw is the hire's.
        app.game.as_mut().unwrap().paused = true;
        app.game.as_mut().unwrap().build.hold(3);
        let camera = app.camera().unwrap().clone();
        let grounds = point(&camera, 19 * 64 + 32, 20 * 64 + 32);
        let mut generator = app.sim.rng.clone();
        let told = click(app, 10, grounds);
        assert_eq!(
            told,
            [
                "world add keeper/keeper/m 19,20 id 101 name=text5500 1",
                "money book 11 800"
            ]
        );
        let game = app.game.as_ref().unwrap();
        assert_eq!(
            (
                game.money.cash,
                game.money.monthly[11][0],
                game.world.can_undo()
            ),
            (1200.0, 800.0, false)
        );
        // One draw of the simulation's generator gave the sex of the next hire.
        generator.rand();
        assert_eq!(app.sim.rng, generator);
        assert_eq!(app.sim.hire_male, (generator.state() >> 16) & 1 == 1);
        // A second hire, then the cash is short: a refusal draws nothing.
        let elsewhere = point(&camera, 20 * 64 + 32, 20 * 64 + 32);
        assert_eq!(click(app, 20, elsewhere).len(), 2);
        let state = app.sim.rng.clone();
        assert_eq!(
            click(app, 30, point(&camera, 18 * 64 + 32, 20 * 64 + 32)),
            ["tool message 10002"]
        );
        assert_eq!(app.sim.rng, state);
        let names: Vec<_> = (app.game.as_ref().unwrap().world.zoo.entities.iter())
            .filter_map(|e| e.common.as_ref().map(|c| c.name.clone()))
            .collect();
        assert_eq!(&names[4..], ["text5500 1", "text5500 2"]);
    });
}

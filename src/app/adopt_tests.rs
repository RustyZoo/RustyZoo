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
                movement_mode: 0,
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

#[test]
fn an_animal_is_adopted_into_the_pen_named_booked_and_refused_on_the_grounds() {
    let install: Install = cache_install("app-adopt", frame_file());
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
    app.game.as_mut().unwrap().build.hold(2);
    let camera = app.camera().unwrap().clone();
    let at = |lx: i32, ly: i32| point(&camera, 20 * 64 + lx, 20 * 64 + ly);
    let said = |app: &App, pass: u64| -> Vec<String> {
        let lead = format!("pass {pass} t={pass} ");
        app.log_lines(pass, pass)
            .iter()
            .filter_map(|l| l.strip_prefix(&lead))
            .filter(|l| {
                l.starts_with("world ")
                    || l.starts_with("tool message")
                    || l.starts_with("tool refused")
            })
            .map(String::from)
            .collect()
    };
    let click = |app: &mut App, pass: u64, to: Command| {
        app.pass(pass, &[to]);
        app.pass(pass + 1, &[Command::Mouse(Mouse::Down)]);
        app.pass(pass + 2, &[Command::Mouse(Mouse::Up)]);
        said(app, pass + 2)
    };
    // Two animals in the pen, one in the first quarter of the tile and one in the last.
    let first = click(&mut app, 10, at(10, 10));
    assert_eq!(
        first,
        ["world add animals/zebra/m 20,20 id 101 name=text5004 1"]
    );
    let second = click(&mut app, 20, at(50, 50));
    assert_eq!(
        second,
        ["world add animals/zebra/m 20,20 id 102 name=text5004 2"]
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
    assert_eq!(click(&mut app, 30, at(50, 10)), ["tool message 10002"]);
    let grounds = point(&camera, 19 * 64 + 32, 20 * 64 + 32);
    let told = click(&mut app, 40, grounds);
    assert_eq!(told, ["tool message 10108"]);
    assert_eq!(app.money().unwrap().cash, 400.0);
}

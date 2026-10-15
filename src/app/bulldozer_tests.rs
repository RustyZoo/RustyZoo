//! The bulldozer through the pass: the button and the Delete key, the hover, a click, an area
//! drag, the questions, on a made-up map with a made-up left bar.

use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::render::order::project;
use crate::render::tests::{cache_install_with, flat_map, frame_file, tile_with};
use crate::ui::draw::Strings as UiStrings;
use crate::ui::layout::Tree;
use crate::vfs::Vfs;
use crate::zoo::{Common, Fence};

const SIZE: (u32, u32) = (320, 240);

struct Texts;

impl UiStrings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            154 => "sell %s?\nRefund: ".into(),
            156 => "merge %s with %s?".into(),
            157 => "animals escape".into(),
            6208 => "Exhibit".into(),
            22100 => "$".into(),
            9001 => "Shop".into(),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

const ROOT: &str = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
    [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
    [c]\ntype=UIButton\nid=1028\nx=300\ny=220\ndx=8\ndy=8\nstate=2048\ntransparent=0\n\
    [d]\ntype=UILayout\nlayout=ui/q.lyt\nstate=1\n";
const QUESTION: &str = "[LayoutInfo]\nid=45000\nlayer=7\nx=0\ny=0\ndx=20\ndy=10\n\
    [a]\ntype=UIButton\nid=45010\ndx=8\ndy=8\ntarget=6\naction=5\ntransparent=0\n\
    [b]\ntype=UIButton\nid=45011\nx=10\ndx=8\ndy=8\ntarget=7\naction=5\ntransparent=0\n";

fn map() -> Zoo {
    let (px, py) = project(0, 1280, 1280, 0);
    let mut zoo = tile_with(&[]);
    zoo.width = 40;
    zoo.height = 40;
    zoo.tiles = flat_map(40, 40);
    zoo.view.scroll = (px - SIZE.0 as i32 / 2, py - SIZE.1 as i32 / 2);
    zoo
}

fn ty(class: &'static str, name: &[u8], subtype: &[u8], ini: &str, fp: [i32; 3]) -> EntityType {
    EntityType {
        class,
        type_name: name.to_vec(),
        subtype: subtype.to_vec(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 9001,
        footprint: fp,
        height: 0,
    }
}

/// 0 dirt path (10), 1 iron fence (125, an exhibit fence), 2 shop (350, asks), 3 wall (125, not
/// deletable), 4 the gate of the iron fence.
fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    let cost = |n: u32| format!("[Characteristics/Integers]\ncPurchaseCost={n}\n");
    cat.push(ty("paths", b"dirt", b"", &cost(10), [2, 2, 1]));
    cat.push(ty(
        "fences",
        b"iron",
        b"f",
        &format!("{}[HabitatFence]\n", cost(125)),
        [1, 1, 1],
    ));
    cat.push(ty(
        "building",
        b"shop",
        b"",
        &format!("{}cNeedsConfirm=1\n", cost(350)),
        [2, 2, 1],
    ));
    cat.push(ty(
        "fences",
        b"wall",
        b"f",
        &format!("{}cDeletable=0\n", cost(125)),
        [1, 1, 1],
    ));
    cat.push(ty(
        "fences",
        b"iron",
        b"g",
        &format!("{}[HabitatFence]\n", cost(125)),
        [1, 1, 1],
    ));
    cat
}

fn install(name: &str, ini: &str) -> crate::sprite::tests::Install {
    let ui = [
        ("ui/root.lyt", ROOT.as_bytes().to_vec()),
        ("ui/q.lyt", QUESTION.as_bytes().to_vec()),
        ("zoo.ini", ini.as_bytes().to_vec()),
    ];
    cache_install_with(&format!("app-dozer-{name}"), frame_file(), &ui)
}

struct Session<'a> {
    app: App<'a>,
    now: u64,
}

fn with_game(name: &str, ini: &str, body: impl FnOnce(&mut Session)) {
    let install = install(name, ini);
    let mut vfs = Vfs::open(&install.0).unwrap();
    vfs.overlay_ini(&Config::parse(ini.as_bytes()).unwrap())
        .unwrap();
    let catalogue = catalogue();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    app.set_interface(Interface::new(tree, Box::new(Texts)));
    app.interface().unwrap().set_hidden(1028, false);
    let mut money = Money::default();
    money.set_cash(50_000.0);
    app.set_money(money);
    let mut session = Session { app, now: 0 };
    session.pass(&[]);
    body(&mut session);
}

impl Session<'_> {
    /// One pass; the lines this file looks at, without their lead.
    fn pass(&mut self, commands: &[Command]) -> Vec<String> {
        self.now += 20;
        self.app.pass(self.now, commands);
        self.app
            .log_lines(self.now / 20, self.now)
            .into_iter()
            .filter_map(|l| {
                let rest = l.split_once(" t=")?.1.split_once(' ')?.1.to_string();
                [
                    "tool bulldoze",
                    "tool drop",
                    "world remove",
                    "exhibit gate",
                    "money cash",
                ]
                .iter()
                .any(|p| rest.starts_with(p))
                .then_some(rest)
            })
            .collect()
    }

    /// A window point inside tile (`tx`, `ty`), at the local point (`lx`, `ly`).
    fn at(&self, tx: i32, ty: i32, lx: i32, ly: i32) -> Command {
        let c = self.app.camera().unwrap();
        let p = project(c.rotation, 64 * tx + lx, 64 * ty + ly, 0);
        Command::Mouse(Mouse::Move(p.0 - c.scroll.0, p.1 - c.scroll.1))
    }

    fn click(&mut self, at: Command) -> Vec<String> {
        self.pass(&[at, Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)])
    }

    /// Adds an entity of the catalogue at a world position.
    fn add(&mut self, ty: usize, x: i32, y: i32, rotation: u32) -> u32 {
        let t = &self.app.renderer.catalogue().types[ty];
        let text = |b: &[u8]| b.iter().map(|&c| char::from(c)).collect::<String>();
        let fence = matches!(t.class, "fences").then_some(Fence {
            strength: 300,
            life: 10,
            timers: [0; 2],
            portal: false,
        });
        let entity = Entity {
            class: t.class.into(),
            type_name: text(&t.type_name),
            subtype: text(&t.subtype),
            body_size: None,
            common: Some(Common {
                x,
                y,
                z: 0,
                rotation,
                id: 0,
                name: String::new(),
                colors: Vec::new(),
                fence,
                movement_mode: 0,
            }),
        };
        let shape = self.app.renderer.shape(&entity);
        self.app
            .game
            .as_mut()
            .unwrap()
            .world
            .add(entity, shape)
            .unwrap()
    }

    fn fence(&mut self, ty: usize, tx: i32, ty_: i32, side: u32) -> u32 {
        let (x, y) = [(32, 0), (63, 32), (32, 63), (0, 32)][side as usize / 2];
        self.add(ty, tx * 64 + x, ty_ * 64 + y, side)
    }

    fn cash(&self) -> f32 {
        self.app.money().unwrap().cash
    }

    fn count(&self) -> usize {
        self.app.game.as_ref().unwrap().world.zoo.entities.len()
    }

    fn cost(&self) -> Option<i64> {
        self.app.game.as_ref().unwrap().cost()
    }

    fn asking(&mut self) -> bool {
        self.app.interface().unwrap().modal_open()
    }
}

const DELETE: Command = Command::Key(Key::Delete, false, false);

#[test]
fn the_delete_key_and_the_button_make_the_bulldozer_the_tool_and_escape_drops_it() {
    with_game("tool", "", |s| {
        assert_eq!(s.pass(&[DELETE]), ["tool bulldozer on"]);
        assert!(s.app.game.as_ref().unwrap().build.bulldozing());
        // Delete again presses the button again: it ends the tool.
        assert_eq!(s.pass(&[DELETE]), ["tool bulldozer off"]);
        // The button does the same.
        let button = [
            Command::Mouse(Mouse::Move(302, 222)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ];
        assert_eq!(s.pass(&button), ["tool bulldozer on"]);
        // Escape leaves the tool first and does not ask the question that leads to the menu.
        let escape = Command::Key(Key::Escape, false, false);
        assert_eq!(s.pass(&[escape]), ["tool bulldozer off", "tool drop"]);
        assert!(!s.asking());
        assert!(!s.app.game.as_ref().unwrap().build.bulldozing());
    });
}

#[test]
fn a_click_removes_the_hovered_object_and_refunds_it_and_the_zoo_wall_stays() {
    with_game("click", "", |s| {
        let fence = s.fence(1, 20, 20, 0);
        let wall = s.fence(3, 20, 20, 6);
        let path = s.add(0, 21 * 64 + 32, 20 * 64 + 32, 0);
        s.pass(&[DELETE]);
        // Hover the fence piece: tinted, its refund as the cost text, and its line in the log.
        let over = s.at(20, 20, 32, 4);
        assert_eq!(
            s.pass(&[over]),
            [format!(
                "tool bulldoze fences/iron/f id {fence} refund 100 name=Shop"
            )]
        );
        assert_eq!(s.cost(), Some(100));
        let game = s.app.game.as_ref().unwrap();
        assert_eq!(game.dozer.at.len(), 1);
        // Nothing over the zoo wall or bare ground.
        assert_eq!(s.pass(&[s.at(20, 20, 4, 32)]), ["tool bulldoze none"]);
        assert_eq!(s.cost(), None);
        assert_eq!(s.click(s.at(20, 20, 4, 32)), Vec::<String>::new());
        assert_eq!(s.click(s.at(20, 22, 32, 32)), Vec::<String>::new());
        assert_eq!((s.count(), s.cash()), (3, 50_000.0));
        // A click removes the piece and pays 80 % of its cost, booked as a refund.
        let told = s.click(s.at(20, 20, 32, 4));
        assert_eq!(
            told,
            [
                // The pointer is looked at again after the click: the hover line comes first and
                // the removal leaves nothing under it.
                format!("tool bulldoze fences/iron/f id {fence} refund 100 name=Shop"),
                "tool bulldoze none".into(),
                format!("world remove fences/iron/f 20,20 side 0 id {fence} refund 100"),
                "money cash 50100".into(),
            ]
        );
        assert_eq!((s.count(), s.cash()), (2, 50_100.0));
        let refunds = &s.app.game.as_ref().unwrap().money;
        assert_eq!(refunds.total[crate::money::CONSTRUCTION_REFUND], 100.0);
        // The path: 8; the wall and the path are still there until clicked.
        let told = s.click(s.at(21, 20, 32, 32));
        assert!(told.contains(&format!(
            "world remove paths/dirt/ 21,20 id {path} refund 8"
        )));
        assert!(told.contains(&"money cash 50108".to_string()));
        assert_eq!(s.count(), 1);
        let _ = wall;
    });
}

#[test]
fn an_area_drag_removes_what_the_rectangle_covers_and_sums_the_refunds() {
    with_game("area", "", |s| {
        let ids: Vec<u32> = (18..25)
            .map(|x| s.add(0, x * 64 + 32, 20 * 64 + 32, 0))
            .collect();
        s.pass(&[DELETE]);
        // Press on the first tile, move to the last one: the rectangle is outlined, the seven
        // pieces are tinted and the cost text is their sum.
        let first = s.at(18, 20, 32, 32);
        s.pass(&[first, Command::Mouse(Mouse::Down), s.at(19, 20, 32, 32)]);
        let told = s.pass(&[s.at(24, 20, 32, 32)]);
        assert_eq!(told, ["tool bulldoze area 18,20 24,20 objects 7 refund 56"]);
        assert_eq!(s.cost(), Some(56));
        assert_eq!(s.app.game.as_ref().unwrap().dozer.at.len(), 7);
        // Back on the press tile within 16 pixels it is a click again.
        let back = s.pass(&[s.at(18, 20, 33, 33)]);
        assert_eq!(back.len(), 1);
        assert!(back[0].starts_with("tool bulldoze paths/dirt/ id"));
        s.pass(&[s.at(24, 20, 32, 32)]);
        // The release removes them all.
        let told = s.pass(&[Command::Mouse(Mouse::Up)]);
        let removes: Vec<&String> = told
            .iter()
            .filter(|l| l.starts_with("world remove"))
            .collect();
        assert_eq!(removes.len(), 7);
        assert_eq!(
            removes[0],
            &format!("world remove paths/dirt/ 18,20 id {} refund 8", ids[0])
        );
        assert!(told.contains(&"tool bulldoze removed 7 refund 56".to_string()));
        assert!(told.contains(&"money cash 50056".to_string()));
        assert_eq!((s.count(), s.cash()), (0, 50_056.0));
        // A drag started over nothing removable shows no cost text until its rectangle has
        // collected something; from then on the sum, 0 included. A release over nothing keeps
        // everything.
        s.add(0, 22 * 64 + 32, 22 * 64 + 32, 0);
        s.pass(&[
            s.at(20, 22, 32, 32),
            Command::Mouse(Mouse::Down),
            s.at(21, 22, 32, 32),
        ]);
        assert_eq!(s.cost(), None);
        s.pass(&[s.at(22, 22, 32, 32)]);
        assert_eq!(s.cost(), Some(8));
        s.pass(&[s.at(21, 22, 32, 32)]);
        assert_eq!(s.cost(), Some(0));
        let told = s.pass(&[Command::Mouse(Mouse::Up)]);
        assert!(told.iter().all(|l| l.starts_with("tool bulldoze")));
        assert_eq!((s.cash(), s.count()), (50_056.0, 1));
    });
}

#[test]
fn a_building_asks_before_it_is_sold_and_the_answer_decides() {
    with_game("sell", "", |s| {
        s.add(2, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.pass(&[DELETE]);
        let over = s.at(20, 20, 32, 32);
        s.pass(&[over]);
        assert_eq!(s.cost(), Some(280));
        // The click asks; the game stands still; No keeps the building.
        let told = s.click(over);
        assert_eq!(told, ["tool bulldoze ask Sell"]);
        assert!(s.asking());
        let told = s.pass(&[Command::Key(Key::N, false, false)]);
        assert_eq!(told, ["tool bulldoze answer no"]);
        assert!(!s.asking());
        assert_eq!((s.count(), s.cash()), (1, 50_000.0));
        // Again, and Yes removes it with its refund.
        s.click(over);
        assert!(s.asking());
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        assert_eq!(told[0], "tool bulldoze answer yes");
        let removed = told.iter().find(|l| l.starts_with("world remove")).unwrap();
        assert!(removed.starts_with("world remove building/shop/ 20,20 id"));
        assert!(removed.ends_with("refund 280"));
        assert!(told.contains(&"money cash 50280".to_string()));
        assert_eq!(s.count(), 0);
    });
}

#[test]
fn with_confirm_sell_off_a_building_goes_without_a_question() {
    with_game("noask", "[UI]\nconfirmSell=0\n", |s| {
        s.add(2, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.pass(&[DELETE]);
        let told = s.click(s.at(20, 20, 32, 32));
        assert!(
            told.iter()
                .any(|l| l.starts_with("world remove building/shop/"))
        );
        assert!(!s.asking());
        assert_eq!(s.cash(), 50_280.0);
    });
}

#[test]
fn the_exhibit_question_comes_once_per_pair_of_regions_and_a_no_keeps_the_pieces() {
    with_game("exhibit", "[UI]\nconfirmSell=0\n", |s| {
        // Tile (20, 20) closed on its four sides is an exhibit next to the zoo grounds.
        for side in [0, 2, 4, 6] {
            s.fence(1, 20, 20, side);
        }
        assert_eq!(
            s.app.game.as_ref().unwrap().world.exhibits().list().len(),
            1
        );
        s.pass(&[DELETE]);
        // An area drag over the tile: the first piece asks; No keeps all four (the same pair).
        let press = [s.at(20, 20, 32, 32), Command::Mouse(Mouse::Down)];
        s.pass(&press);
        s.pass(&[s.at(20, 21, 32, 32)]);
        let told = s.pass(&[Command::Mouse(Mouse::Up)]);
        assert_eq!(told, ["tool bulldoze ask Escape(1)"]);
        assert!(s.asking());
        let told = s.pass(&[Command::Key(Key::N, false, false)]);
        assert_eq!(told[0], "tool bulldoze answer no");
        assert!(!told.iter().any(|l| l.starts_with("world remove")));
        assert!(!s.asking());
        assert_eq!((s.count(), s.cash()), (4, 50_000.0));
        // Once more: Yes removes the first piece, the exhibit is gone, the others follow without
        // a question.
        s.pass(&press);
        s.pass(&[s.at(20, 21, 32, 32)]);
        s.pass(&[Command::Mouse(Mouse::Up)]);
        assert!(s.asking());
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        let removes = told
            .iter()
            .filter(|l| l.starts_with("world remove"))
            .count();
        assert_eq!(removes, 4);
        assert!(!s.asking());
        assert!(told.contains(&"money cash 50400".to_string()));
        assert!(
            s.app
                .game
                .as_ref()
                .unwrap()
                .world
                .exhibits()
                .list()
                .is_empty()
        );
    });
}

#[test]
fn bulldozing_a_gate_keeps_the_exhibit_and_its_name_and_gives_the_line_a_new_gate() {
    with_game("gate", "[UI]\nconfirmSell=0\n", |s| {
        // Tile (20, 20) closed, with a second piece on the line of its west side.
        let pieces: Vec<u32> = [0, 2, 4, 6].map(|side| s.fence(1, 20, 20, side)).to_vec();
        let other = s.fence(1, 19, 20, 2);
        s.app.exhibits_told();
        assert_eq!(
            s.app.game.as_ref().unwrap().world.exhibits().list().len(),
            1
        );
        let world = &mut s.app.game.as_mut().unwrap().world;
        world.rename(1, "Lions".into());
        world.set_gate(1, pieces[3]);
        // The bulldozer takes the gate: the other piece of the line is the gate now.
        s.pass(&[DELETE]);
        s.pass(&[s.at(20, 20, 12, 32)]);
        let told = s.click(s.at(20, 20, 12, 32));
        assert!(
            told.iter().any(|l| l.starts_with("world remove")),
            "{told:?}"
        );
        assert!(
            told.contains(&"exhibit gate 1 19,20 side 2".to_string()),
            "{told:?}"
        );
        let world = &s.app.game.as_ref().unwrap().world;
        assert_eq!(world.exhibits().list().len(), 1);
        let exhibit = world.exhibits().get(1).unwrap();
        assert_eq!(exhibit.name, "Lions");
        assert_eq!(exhibit.gate, Some(other));
    });
}

#[test]
fn escape_or_a_closed_window_stops_a_removal_that_is_asking() {
    with_game("stop", "", |s| {
        s.add(2, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.add(2, 21 * 64 + 32, 20 * 64 + 32, 0);
        s.pass(&[DELETE]);
        let press = [s.at(20, 20, 32, 32), Command::Mouse(Mouse::Down)];
        s.pass(&press);
        s.pass(&[s.at(21, 20, 32, 32)]);
        s.pass(&[Command::Mouse(Mouse::Up)]);
        assert!(s.asking());
        // Escape answers No for this one and the second one asks next.
        s.pass(&[Command::Key(Key::Escape, false, false)]);
        assert!(s.asking());
        // The window being closed answers Cancel and drops the rest.
        s.pass(&[Command::Close]);
        assert!(s.app.game.as_ref().unwrap().sweep.is_none());
        assert_eq!((s.count(), s.cash()), (2, 50_000.0));
    });
}

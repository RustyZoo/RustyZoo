//! The bulldozer through the pass: the button and the Delete key, the hover, a click, an area
//! drag, the questions, on a made-up map with a made-up left bar.

use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::place::bulldozer::Question;
use crate::render::order::project;
use crate::render::tests::{cache_install_with, flat_map, frame_file, palette_file, tile_with};
use crate::sprite::tests::{sized, sprite_file};
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
            155 => "fire %s?\nsalary: ".into(),
            156 => "merge %s with %s?".into(),
            157 => "animals escape".into(),
            160 => "tank drained".into(),
            10552 => "tank objects".into(),
            10571 => "no merge".into(),
            159 => "blocked: %s".into(),
            6208 => "Exhibit".into(),
            4999 => "Broken".into(),
            4914 => "Minimum".into(),
            4913 => "Maximum".into(),
            4916 => "%s month(s) ago".into(),
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
    [e]\ntype=UIButton\nid=1075\nx=280\ny=220\ndx=8\ndy=8\nstate=2\ntransparent=0\n\
    [d]\ntype=UILayout\nlayout=ui/q.lyt\nstate=1\n\
    [f]\ntype=UILayout\nlayout=ui/animal.lyt\nstate=1\n\
    [g]\ntype=UILayout\nlayout=ui/staff.lyt\nstate=1\n\
    [i]\ntype=UILayout\nlayout=ui/guest.lyt\nstate=1\n\
    [h]\ntype=UILayout\nlayout=ui/exhibit.lyt\nstate=1\n\
    [j]\ntype=UILayout\nlayout=ui/filter.lyt\nstate=1\n";
const ANIMAL: &str = "[LayoutInfo]\nid=6\nlayer=7\nx=0\ny=0\ndx=20\ndy=10\n\
    [a]\ntype=UIButton\nid=3100\ndx=8\ndy=8\ntarget=6\naction=2\ntransparent=0\n";
const EXHIBIT: &str = "[LayoutInfo]\nid=30\nlayer=7\nx=0\ny=0\ndx=40\ndy=40\n\
    [a]\ntype=UIListBox\nid=4326\nx=0\ny=0\ndx=40\ndy=36\n\
    [b]\ntype=UIText\nid=4868\ndx=8\ndy=8\n[c]\ntype=UIText\nid=4881\ndx=8\ndy=8\n\
    [d]\ntype=UIText\nid=4883\ndx=8\ndy=8\n[e]\ntype=UIText\nid=4891\ndx=8\ndy=8\n\
    [f]\ntype=UIText\nid=4893\ndx=8\ndy=8\n[g]\ntype=UIText\nid=4871\ndx=8\ndy=8\n\
    [h]\ntype=UIText\nid=4872\ndx=8\ndy=8\n[i]\ntype=UIButton\nid=4869\ndx=8\ndy=8\n\
    [j]\ntype=UIButton\nid=4870\ndx=8\ndy=8\n[k]\ntype=UIButton\nid=4885\ndx=8\ndy=8\n\
    [l]\ntype=UIText\nid=4882\ndx=8\ndy=8\n[m]\ntype=UIButton\nid=4879\ndx=8\ndy=8\n\
    [n]\ntype=UIButton\nid=4880\ndx=8\ndy=8\n";
const GUEST: &str = "[LayoutInfo]\nid=9\nlayer=7\nx=0\ny=0\ndx=20\ndy=10\n\
    [a]\ntype=UIButton\nid=3413\ndx=8\ndy=8\ntarget=9\naction=2\ntransparent=0\n";
const FILTER: &str = "[LayoutInfo]\nid=46\nlayer=7\nx=0\ny=0\ndx=40\ndy=40\n\
    [a]\ntype=UIText\nid=4913\ndx=8\ndy=8\n[b]\ntype=UIText\nid=4916\ndx=8\ndy=8\n\
    [c]\ntype=UIText\nid=4918\ndx=8\ndy=8\n[d]\ntype=UIText\nid=4921\ndx=8\ndy=8\n\
    [e]\ntype=UIText\nid=4923\ndx=8\ndy=8\n[f]\ntype=UIText\nid=4925\ndx=8\ndy=8\n\
    [g]\ntype=UIEditableText\nid=4904\ndx=8\ndy=8\n\
    [h]\ntype=UIButton\nid=4906\nx=20\ndx=8\ndy=8\ntransparent=0\n";
const STAFF: &str = "[LayoutInfo]\nid=17\nlayer=7\nx=0\ny=0\ndx=20\ndy=10\n";
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
    // A type without pictures of its own is the square of `obj.ani`.
    let ini = match ini.contains("[Animations]") {
        true => ini.to_string(),
        false => format!("{ini}[Animations]\nidle=dozer/obj.ani\n"),
    };
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
/// deletable), 4 the gate of the iron fence, 5 tank wall, 6 show tank wall, 7 tree (40), 8 bench (50,
/// a scenery object that does not live under water), 9 stone path (20), 10 steel fence (150) and
/// 11 its gate, 12 a zookeeper (800, its monthly cost 650.5), 13 a zebra (800), 14 a guest and 15 a moveable cart. Every picture is a square around the anchor: 24 pixels, the tree's 16.
pub(super) fn catalogue() -> Catalogue {
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
    cat.push(ty(
        "tankwall",
        b"glass",
        b"f",
        &format!("{}[TankFence]\n", cost(100)),
        [1, 1, 1],
    ));
    cat.push(ty(
        "tankwall",
        b"show",
        b"f",
        &format!("{}cIsShowFence=1\n[TankFence]\n", cost(100)),
        [1, 1, 1],
    ));
    // 7: a tree (40), a square of 16 pixels.
    cat.push(ty(
        "objects",
        b"tree",
        b"",
        &format!("{}[Animations]\nidle=dozer/small.ani\n", cost(40)),
        [1, 1, 1],
    ));
    cat.push(ty("objects", b"bench", b"", &cost(50), [1, 1, 1]));
    cat.push(ty("paths", b"stone", b"", &cost(20), [2, 2, 1]));
    for subtype in [b"f", b"g"] {
        let ini = format!("{}[HabitatFence]\n", cost(150));
        cat.push(ty("fences", b"steel", subtype, &ini, [1, 1, 1]));
    }
    let keeper = "[m/Characteristics/Integers]\ncPurchaseCost=800\ncMonthlyCost=650.5\n";
    cat.push(ty("keeper", b"keeper", b"m", keeper, [1, 1, 1]));
    let zebra = "[m/Characteristics/Integers]\ncPurchaseCost=800\n\
        [Sounds]\nplacesound=zplace\nplacesound=1500\npickupsound=zpick\n";
    cat.push(ty("animals", b"zebra", b"m", zebra, [1, 1, 1]));
    cat.push(ty("guests", b"man", b"", "", [1, 1, 1]));
    // 15: a cart (60, moveable), a building the player can pick up.
    let cart = format!("{}cMoveable=1\n[Satisfies]\nbuilding\n", cost(60));
    cat.push(ty("building", b"cart", b"", &cart, [2, 2, 1]));
    // 16: a dock (2000) the bulldozer cannot remove.
    let dock = format!("{}cNeedsConfirm=1\ncDeletable=0\n", cost(2000));
    cat.push(ty("building", b"dock", b"", &dock, [2, 2, 1]));
    // 17: a seal, an animal that lives under water.
    let seal = "[m/Characteristics/Integers]\ncUnderwater=1\n";
    cat.push(ty("animals", b"seal", b"m", seal, [1, 1, 1]));
    // 18: a tank filter (200).
    let filter = format!("{}cNeedsConfirm=1\n", cost(200));
    cat.push(ty("filter", b"filter1", b"", &filter, [2, 2, 1]));
    cat
}

fn install(name: &str, ini: &str) -> crate::sprite::tests::Install {
    // Every object is a square of 24 pixels around its anchor.
    let mut frame = Vec::new();
    for field in [24u16, 24, 12, 12, 0] {
        frame.extend(field.to_le_bytes());
    }
    for _ in 0..24 {
        frame.extend([1, 0, 24]);
        frame.extend([1; 24]);
    }
    let mut small = Vec::new();
    for field in [16u16, 16, 8, 8, 0] {
        small.extend(field.to_le_bytes());
    }
    for _ in 0..16 {
        small.extend([1, 0, 16]);
        small.extend([1; 16]);
    }
    let ui = [
        (
            "dozer/small.ani",
            b"[animation]\ndir0=dozer\ndir1=small\nanimation=N\n".to_vec(),
        ),
        ("dozer/small/N", sprite_file(None, 1, &[sized(&small)])),
        (
            "dozer/obj.ani",
            b"[animation]\ndir0=dozer\ndir1=spr\nanimation=N\n".to_vec(),
        ),
        ("dozer/spr/N", sprite_file(None, 1, &[sized(&frame)])),
        (
            "animals/Lion/lion.pal",
            palette_file(&[[255, 0, 255], [1, 2, 3]]),
        ),
        ("ui/root.lyt", ROOT.as_bytes().to_vec()),
        ("ui/q.lyt", QUESTION.as_bytes().to_vec()),
        ("ui/animal.lyt", ANIMAL.as_bytes().to_vec()),
        ("ui/staff.lyt", STAFF.as_bytes().to_vec()),
        ("ui/guest.lyt", GUEST.as_bytes().to_vec()),
        ("ui/exhibit.lyt", EXHIBIT.as_bytes().to_vec()),
        ("ui/filter.lyt", FILTER.as_bytes().to_vec()),
        ("zoo.ini", ini.as_bytes().to_vec()),
    ];
    cache_install_with(&format!("app-dozer-{name}"), frame_file(), &ui)
}

pub(super) struct Session<'a> {
    pub(super) app: App<'a>,
    now: u64,
}

pub(super) fn with_game(name: &str, ini: &str, body: impl FnOnce(&mut Session)) {
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
    app.interface().unwrap().set_hidden(1075, false);
    let mut money = Money::default();
    money.set_cash(50_000.0);
    app.set_money(money);
    let mut session = Session { app, now: 0 };
    session.pass(&[]);
    body(&mut session);
}

impl Session<'_> {
    /// One pass; every line, without its lead.
    pub(super) fn log(&mut self, commands: &[Command]) -> Vec<String> {
        self.now += 20;
        self.app.pass(self.now, commands);
        self.app
            .log_lines(self.now / 20, self.now)
            .into_iter()
            .filter_map(|l| Some(l.split_once(" t=")?.1.split_once(' ')?.1.to_string()))
            .collect()
    }

    /// One pass; the lines this file looks at, without their lead.
    pub(super) fn pass(&mut self, commands: &[Command]) -> Vec<String> {
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
                    "tool message",
                    "tool tank",
                    "exhibit new",
                    "exhibit tank",
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
    pub(super) fn at(&self, tx: i32, ty: i32, lx: i32, ly: i32) -> Command {
        let c = self.app.camera().unwrap();
        let p = project(c.rotation, 64 * tx + lx, 64 * ty + ly, 0);
        Command::Mouse(Mouse::Move(p.0 - c.scroll.0, p.1 - c.scroll.1))
    }

    /// A window point `(dx, dy)` view pixels from the anchor of the world position (`x`, `y`) on
    /// the ground.
    pub(super) fn near(&self, x: i32, y: i32, (dx, dy): (i32, i32)) -> Command {
        let c = self.app.camera().unwrap();
        let p = project(c.rotation, x, y, 0);
        Command::Mouse(Mouse::Move(p.0 + dx - c.scroll.0, p.1 + dy - c.scroll.1))
    }

    pub(super) fn click(&mut self, at: Command) -> Vec<String> {
        self.pass(&[at, Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)])
    }

    /// The same, with every line of the pass.
    pub(super) fn click_log(&mut self, at: Command) -> Vec<String> {
        self.log(&[at, Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)])
    }

    /// An entity of the catalogue at a world position, and how it takes its place.
    pub(super) fn entity(
        &self,
        ty: usize,
        x: i32,
        y: i32,
        rotation: u32,
    ) -> (Entity, Option<crate::world::Shape>) {
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
                ..Default::default()
            }),
        };
        let shape = self.app.renderer.shape(&entity);
        (entity, shape)
    }

    /// Adds an entity of the catalogue at a world position.
    pub(super) fn add(&mut self, ty: usize, x: i32, y: i32, rotation: u32) -> u32 {
        let (entity, shape) = self.entity(ty, x, y, rotation);
        self.app
            .game
            .as_mut()
            .unwrap()
            .world
            .add(entity, shape)
            .unwrap()
    }

    pub(super) fn fence(&mut self, ty: usize, tx: i32, ty_: i32, side: u32) -> u32 {
        let (x, y) = [(32, 0), (63, 32), (32, 63), (0, 32)][side as usize / 2];
        self.add(ty, tx * 64 + x, ty_ * 64 + y, side)
    }

    pub(super) fn cash(&self) -> f32 {
        self.app.money().unwrap().cash
    }

    pub(super) fn count(&self) -> usize {
        self.app.game.as_ref().unwrap().world.zoo.entities.len()
    }

    pub(super) fn cost(&self) -> Option<i64> {
        self.app.game.as_ref().unwrap().cost()
    }

    pub(super) fn asking(&mut self) -> bool {
        self.app.interface().unwrap().modal_open()
    }
}

pub(super) const DELETE: Command = Command::Key(Key::Delete, false, false);

#[test]
fn the_delete_key_and_the_button_make_the_bulldozer_the_tool_and_escape_drops_it() {
    with_game("tool", "", |s| {
        assert_eq!(s.pass(&[DELETE]), ["tool bulldozer on"]);
        assert!(s.app.game.as_ref().unwrap().build.bulldozing());
        s.pass(&[s.at(20, 20, 32, 32)]);
        assert_eq!(s.app.cursor(), 207, "the bulldozer");
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
fn the_sell_button_of_the_building_panel_sells_as_the_bulldozer_does() {
    with_game("sell-button", "", |s| {
        let id = s.add(2, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.pass(&[]);
        // With no building selected the button does nothing.
        s.app.building_sell();
        assert!(!s.asking());
        s.app.game.as_mut().unwrap().building.select(id, 21);
        s.app.game.as_mut().unwrap().selected = Some(id);
        s.app.building_sell();
        assert!(s.asking());
        // The selection went before the question, so a No leaves the building unselected.
        assert_eq!(s.app.game.as_ref().unwrap().selected, None);
        let told = s.pass(&[Command::Key(Key::N, false, false)]);
        assert_eq!(told, ["tool bulldoze answer no"]);
        assert_eq!((s.count(), s.cash()), (1, 50_000.0));
        // (The test has no panel to keep shown, so the pass deselected the building.)
        s.app.game.as_mut().unwrap().building.select(id, 21);
        s.app.building_sell();
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        let removed = told.iter().find(|l| l.starts_with("world remove")).unwrap();
        assert!(removed.ends_with("refund 280"), "{removed}");
        assert_eq!((s.count(), s.cash()), (0, 50_280.0));
    });
}

#[test]
fn the_sell_button_does_not_test_cdeletable_but_the_bulldozer_does() {
    with_game("sell-undeletable", "", |s| {
        let id = s.add(16, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.pass(&[DELETE]);
        let over = s.at(20, 20, 32, 32);
        s.pass(&[over]);
        assert_eq!(
            s.click(over).iter().filter(|l| l.contains("ask")).count(),
            0
        );
        assert_eq!((s.count(), s.cash(), s.asking()), (1, 50_000.0, false));
        s.app.game.as_mut().unwrap().building.select(id, 21);
        s.app.building_sell();
        assert!(s.asking());
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        let removed = told.iter().find(|l| l.starts_with("world remove")).unwrap();
        assert!(removed.ends_with("refund 1600"), "{removed}");
        assert_eq!((s.count(), s.cash()), (0, 51_600.0));
    });
}

#[test]
fn a_staff_member_is_fired_after_a_question_that_names_the_salary_and_costs_nothing() {
    with_game("fire", "", |s| {
        s.add(12, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.pass(&[DELETE]);
        let over = s.at(20, 20, 32, 32);
        s.pass(&[over]);
        assert_eq!(s.cost(), Some(0));
        assert_eq!(s.click(over), ["tool bulldoze ask Fire"]);
        let game = s.app.game.as_ref().unwrap();
        let (ui, cat) = (s.app.ui.as_ref().unwrap(), s.app.renderer.catalogue());
        let text = bulldozer::question_text(ui, cat, &game.world, 0, Question::Fire);
        assert_eq!(text, "fire Shop?\nsalary: $650", "cut to whole units");
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        let removed = told.iter().find(|l| l.starts_with("world remove")).unwrap();
        assert!(removed.starts_with("world remove keeper/keeper/m 20,20 id"));
        assert!(removed.ends_with("refund 0"), "{removed}");
        assert_eq!((s.count(), s.cash()), (0, 50_000.0));
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
        world.set_gate(1, other);
        // The two pieces of the line are drawn on top of each other and the later one is picked.
        // The bulldozer takes the gate: the other piece of the line is the gate now.
        s.pass(&[DELETE]);
        s.pass(&[s.at(20, 20, 12, 32)]);
        let told = s.click(s.at(20, 20, 12, 32));
        assert!(
            told.iter().any(|l| l.starts_with("world remove")),
            "{told:?}"
        );
        assert!(
            told.contains(&"exhibit gate 1 20,20 side 6".to_string()),
            "{told:?}"
        );
        let world = &s.app.game.as_ref().unwrap().world;
        assert_eq!(world.exhibits().list().len(), 1);
        let exhibit = world.exhibits().get(1).unwrap();
        assert_eq!(exhibit.name, "Lions");
        assert_eq!(exhibit.gate, Some(pieces[3]));
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

#[test]
fn a_tank_asks_whether_to_drain_it_and_a_wall_between_unlike_tanks_says_why_it_stays() {
    with_game("tank", "[UI]\nconfirmSell=0\n", |s| {
        // Tile (20, 20) closed by tank walls is a tank: string 160 instead of 157.
        for side in [0, 2, 4, 6] {
            s.fence(5, 20, 20, side);
        }
        s.pass(&[DELETE]);
        let told = s.click(s.at(20, 20, 32, 4));
        assert_eq!(told.last().unwrap(), "tool bulldoze ask Escape(1)");
        let game = s.app.game.as_ref().unwrap();
        let ui = s.app.ui.as_ref().unwrap();
        let cat = s.app.renderer.catalogue();
        let at = game.world.edge(20 * 40 + 20, 0).unwrap();
        let question = Question::Escape(1);
        assert_eq!(
            bulldozer::question_text(ui, cat, &game.world, at, question),
            "tank drained"
        );
        s.pass(&[Command::Key(Key::N, false, false)]);
        // A show tank beside it: the line between them stays, the click says message 10571.
        for side in [6, 0, 2, 4] {
            s.fence(6, 21, 20, side);
        }
        let told = s.click(s.at(20, 20, 60, 32));
        assert!(told.contains(&"tool message 10571".to_string()), "{told:?}");
        assert!(!told.iter().any(|l| l.contains("ask")), "{told:?}");
        assert!(!s.asking());
        assert_eq!(s.count(), 8);
    });
}

#[test]
fn the_tree_in_front_of_a_building_is_picked_where_their_pictures_overlap() {
    with_game("front", "", |s| {
        let (x, y) = (20 * 64 + 32, 20 * 64 + 32);
        let shop = s.add(2, x, y, 0);
        // The tree stands on the next tile, 16 pixels right and 12 lower in the view, and is
        // drawn after the shop: its 16 pixels overlap the lower right of the shop's 24.
        let tree = s.add(7, x - 8, y + 40, 0);
        s.pass(&[DELETE]);
        let name = |id: u32, refund: u32| {
            let class = if id == shop {
                "building/shop/"
            } else {
                "objects/tree/"
            };
            format!("tool bulldoze {class} id {id} refund {refund} name=Shop")
        };
        // Where both have a pixel the tree, drawn last, wins.
        assert_eq!(s.pass(&[s.near(x, y, (12, 8))]), [name(tree, 32)]);
        // Beside the overlap each has pixels of its own.
        assert_eq!(s.pass(&[s.near(x, y, (0, 8))]), [name(shop, 280)]);
        assert_eq!(s.pass(&[s.near(x, y, (16, 16))]), [name(tree, 32)]);
        // Outside both pictures there is nothing.
        assert_eq!(s.pass(&[s.near(x, y, (20, -8))]), ["tool bulldoze none"]);
        // The click removes the one picked, without a question for a tree.
        let told = s.click(s.near(x, y, (12, 8)));
        assert!(
            told.iter()
                .any(|l| l.starts_with("world remove objects/tree/"))
        );
        assert_eq!(s.count(), 1);
    });
}

/// The tank (20, 20) closed with walls but its last, the bench of tile (20, 20) in it; the
/// game is told what happened to the exhibits, as after a placed piece.
pub(super) fn tank_with_a_bench(s: &mut Session) {
    s.add(8, 20 * 64 + 16, 20 * 64 + 16, 0);
    s.app.game.as_mut().unwrap().world.begin();
    for side in [0, 2, 4, 6] {
        s.fence(5, 20, 20, side);
    }
    s.app.exhibits_told();
}

#[test]
fn objects_in_a_new_tank_are_asked_about_and_yes_sells_them_before_the_name() {
    with_game("tank-yes", "[UI]\nconfirmSell=0\n", |s| {
        tank_with_a_bench(s);
        assert!(s.asking());
        assert_eq!(
            s.app.game.as_ref().unwrap().world.exhibits().list().len(),
            1
        );
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        assert_eq!(told[0], "tool tank answer yes");
        assert!(
            told.iter()
                .any(|l| l.starts_with("world remove objects/bench"))
        );
        assert!(told.iter().any(|l| l.contains("exhibit new 1 \"Tank 1\"")));
        assert!(!s.asking());
        // The bench was sold for 80 percent of its price.
        assert_eq!(s.cash(), 50_040.0);
        let world = &s.app.game.as_ref().unwrap().world;
        assert!(world.refused(1).is_empty());
        assert_eq!(world.exhibits().list().len(), 1);
    });
}

#[test]
fn no_undoes_what_made_the_tank_and_gives_its_money_back() {
    with_game("tank-no", "[UI]\nconfirmSell=0\n", |s| {
        tank_with_a_bench(s);
        assert!(s.asking());
        let told = s.pass(&[Command::Key(Key::N, false, false)]);
        assert_eq!(told[0], "tool tank answer no");
        assert!(!s.asking());
        assert_eq!(s.count(), 1, "the bench stays, the four walls are gone");
        // The four walls were placed free by this test, so nothing comes back.
        assert_eq!(s.cash(), 50_000.0);
        let world = &s.app.game.as_ref().unwrap().world;
        assert!(world.exhibits().list().is_empty());
        assert_eq!(world.exhibits().counter, 1);
    });
}

#[test]
fn one_click_on_the_line_between_two_tanks_takes_both_walls_and_merges_them() {
    with_game("tank-tied", "[UI]\nconfirmSell=0\n", |s| {
        // The second tank leans on the wall of the first: the wall step builds its own.
        for (tile, side) in [
            (20, 0),
            (20, 2),
            (20, 4),
            (20, 6),
            (21, 0),
            (21, 2),
            (21, 4),
        ] {
            s.fence(5, tile, 20, side);
        }
        assert_eq!(
            s.app.game.as_ref().unwrap().world.exhibits().list().len(),
            2
        );
        assert_eq!(s.count(), 8, "a copy of the wall of the first tank");
        s.pass(&[DELETE]);
        // The hover shows both refunds.
        s.pass(&[s.at(20, 20, 60, 32)]);
        assert_eq!(s.cost(), Some(160));
        // The first removal keeps the line closed and asks to merge, the second goes without.
        let cash = s.cash();
        let told = s.click(s.at(20, 20, 60, 32));
        assert_eq!(told.last().unwrap(), "tool bulldoze ask Merge(1, 2)");
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        let removed = told
            .iter()
            .filter(|l| l.starts_with("world remove"))
            .count();
        assert_eq!((removed, s.cash()), (2, cash + 160.0), "{told:?}");
        assert!(!s.asking());
        let world = &s.app.game.as_ref().unwrap().world;
        assert_eq!(
            world.exhibits().list().len(),
            1,
            "{:?}",
            world.exhibits().list()
        );
        assert_eq!(world.exhibits().list()[0].tiles, 2);
    });
}

#[test]
fn a_no_on_the_line_between_two_tanks_keeps_both_walls_and_asks_once() {
    with_game("tank-tied-no", "[UI]\nconfirmSell=0\n", |s| {
        for (tile, side) in [
            (20, 0),
            (20, 2),
            (20, 4),
            (20, 6),
            (21, 0),
            (21, 2),
            (21, 4),
        ] {
            s.fence(5, tile, 20, side);
        }
        s.pass(&[DELETE]);
        let cash = s.cash();
        let told = s.click(s.at(20, 20, 60, 32));
        assert_eq!(told.last().unwrap(), "tool bulldoze ask Merge(1, 2)");
        // The second removal of the click finds the pair and takes it as a No without a dialog.
        let told = s.pass(&[Command::Key(Key::N, false, false)]);
        assert_eq!(told[0], "tool bulldoze answer no");
        assert!(!told.iter().any(|l| l.contains("ask")), "{told:?}");
        assert!(!told.iter().any(|l| l.starts_with("world remove")));
        assert!(!s.asking());
        assert_eq!((s.count(), s.cash()), (8, cash));
        let world = &s.app.game.as_ref().unwrap().world;
        assert_eq!(world.exhibits().list().len(), 2);
    });
}

#[test]
fn a_no_on_the_second_removal_of_a_tied_wall_click_leaves_the_first_one_gone() {
    with_game("tank-tied-no-q", "[UI]\nconfirmSell=0\n", |s| {
        // The tank of tile (20, 20), and a land exhibit on (21, 20) whose west wall Q is a tank
        // wall: Q has no wall height, so it goes first, without a question.
        for side in [0, 2, 4, 6] {
            s.fence(5, 20, 20, side);
        }
        for side in [0, 2, 4] {
            s.fence(1, 21, 20, side);
        }
        let q = s.fence(5, 21, 20, 6);
        s.pass(&[DELETE]);
        let cash = s.cash();
        let told = s.click(s.at(20, 20, 60, 32));
        assert!(
            told.iter().any(|l| l.starts_with("world remove")),
            "{told:?}"
        );
        assert!(
            told.contains(&"tool bulldoze ask Merge(1, 2)".to_string()),
            "{told:?}"
        );
        assert!(
            s.app.game.as_ref().unwrap().world.find(q).is_none(),
            "Q is gone"
        );
        let told = s.pass(&[Command::Key(Key::N, false, false)]);
        assert_eq!(told[0], "tool bulldoze answer no");
        assert!(!told.iter().any(|l| l.starts_with("world remove")));
        assert!(!s.asking());
        assert_eq!((s.count(), s.cash()), (7, cash + 80.0));
    });
}

/// The levels the lines `exhibit tank 1 ...` tell over `passes` passes.
fn levels(s: &mut Session, passes: usize) -> Vec<u32> {
    let mut found = Vec::new();
    for _ in 0..passes {
        for line in s.pass(&[]) {
            let words: Vec<&str> = line.split(' ').collect();
            if let Some(at) = words.iter().position(|w| *w == "level") {
                found.push(words[at + 1].parse().unwrap());
            }
        }
    }
    found
}

/// Closes the tank (20, 20) with walls after giving the world the numbers of a shipped game.
fn new_tank(s: &mut Session) {
    let world = &mut s.app.game.as_mut().unwrap().world;
    world.set_tank_config(crate::exhibit::TankConfig {
        height: [5, 9],
        offset: -1,
        ..Default::default()
    });
    world.begin();
    for side in [0, 2, 4, 6] {
        s.fence(5, 20, 20, side);
    }
    s.app.exhibits_told();
}

#[test]
fn the_water_of_a_new_tank_rises_one_step_per_simulation_step() {
    with_game("tank-water", "", |s| {
        new_tank(s);
        let told = levels(s, 20);
        // The tank is told at 0 and the steps that follow tell 1 to 4, once each.
        assert_eq!(told.last(), Some(&4), "{told:?}");
        assert!(told.windows(2).all(|w| w[1] == w[0] || w[1] == w[0] + 1));
        assert_eq!(told.iter().filter(|&&l| l > 0).count(), 4);
    });
}

#[test]
fn the_water_waits_while_the_game_is_paused_and_a_tank_made_then_is_full_at_once() {
    with_game("tank-water-paused", "", |s| {
        s.pass(&[Command::Pause]);
        assert!(s.app.paused());
        new_tank(s);
        let told = levels(s, 20);
        assert!(told.is_empty(), "{told:?}");
        let tank = s.app.game.as_ref().unwrap().world.tank(1).unwrap();
        assert_eq!((tank.level, tank.filled), (4, true), "full at once");
    });
}

#[test]
fn a_piece_the_tool_places_to_close_a_pen_marks_and_rates_the_pen() {
    with_game("close", "", |s| {
        for side in [0, 2, 4] {
            s.fence(1, 20, 20, side);
        }
        s.app.game.as_mut().unwrap().build.hold(1);
        s.pass(&[s.at(20, 20, 12, 32)]);
        s.pass(&[Command::Mouse(Mouse::Down)]);
        s.pass(&[Command::Mouse(Mouse::Up)]);
        let world = &s.app.game.as_ref().unwrap().world;
        let pen = world.exhibits().region(20 * 40 + 20).unwrap();
        assert_ne!(pen, crate::exhibit::GROUNDS);
        let pen = world.exhibits().get(pen).unwrap();
        assert!(!pen.rating.dirty, "marked and rated by the pair");
    });
}

#[test]
fn a_piece_the_bulldozer_takes_to_join_two_pens_marks_and_rates_the_one_left() {
    with_game("join", "[UI]\nconfirmSell=0\n", |s| {
        // Two pens side by side, the line between them held by one piece of the west pen.
        for side in [0, 2, 4, 6] {
            s.fence(1, 20, 20, side);
        }
        for side in [0, 2, 4] {
            s.fence(1, 21, 20, side);
        }
        assert_eq!(
            s.app.game.as_ref().unwrap().world.exhibits().list().len(),
            2
        );
        // The pair alone rates the pen: the game is paused, so no step does.
        s.pass(&[Command::Pause]);
        s.pass(&[DELETE]);
        s.pass(&[s.at(20, 20, 62, 32)]);
        assert_eq!(
            s.click(s.at(20, 20, 62, 32)),
            ["tool bulldoze ask Merge(1, 2)"]
        );
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        assert!(
            told.iter().any(|l| l.starts_with("world remove")),
            "{told:?}"
        );
        let world = &s.app.game.as_ref().unwrap().world;
        assert_eq!(world.exhibits().list().len(), 1);
        let pen = world.exhibits().region(20 * 40 + 20).unwrap();
        assert!(
            !world.exhibits().get(pen).unwrap().rating.dirty,
            "rated by the pair"
        );
    });
}

/// A filter of the catalogue (18) on tile (20, 20) with this health, as placed at game time 0.
fn filter_at(s: &mut Session, health: u32) -> u32 {
    let (mut entity, shape) = s.entity(18, 20 * 64 + 32, 20 * 64 + 32, 0);
    let ty = &s.app.renderer.catalogue().types[18];
    let f = crate::world::placed_filter(ty, &Default::default(), 0);
    entity.common.as_mut().unwrap().filter = Some(crate::zoo::Filter { health, ..f });
    s.app
        .game
        .as_mut()
        .unwrap()
        .world
        .add(entity, shape)
        .unwrap()
}

#[test]
fn a_selected_filter_shows_its_panel_and_the_sell_button_refunds_its_own_price() {
    with_game("filter-panel", "", |s| {
        let id = filter_at(s, 10);
        assert_eq!(s.app.building_layout(id), Some(46));
        s.app.game.as_mut().unwrap().selected = Some(id);
        let told = s.log(&[]);
        assert!(
            told.iter().any(|l| l.contains(&format!("filter {id} "))),
            "{told:?}"
        );
        assert!(s.app.interface().unwrap().shown(46));
        // The sell button asks (the type needs confirmation) and a Yes books the filter's price:
        // 0.8 of 200 healthy, half of that decayed.
        assert!(s.app.interface().unwrap().press_button(4906));
        s.pass(&[]);
        assert!(s.asking());
        assert_eq!(s.app.game.as_ref().unwrap().selected, None);
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        let removed = told.iter().find(|l| l.starts_with("world remove")).unwrap();
        assert!(removed.ends_with("refund 160"), "{removed}");
        assert_eq!((s.count(), s.cash()), (0, 50_160.0));
        // A decayed one gives 80; the panel shows when selected again, then a closed panel clears
        // the selection.
        let id = filter_at(s, 5);
        s.app.game.as_mut().unwrap().selected = Some(id);
        s.pass(&[]);
        assert!(s.app.interface().unwrap().shown(46));
        s.app.interface().unwrap().set_hidden(46, true);
        let told = s.log(&[]);
        assert!(told.iter().any(|l| l == "panel filter none"), "{told:?}");
        assert_eq!(s.app.game.as_ref().unwrap().selected, None);
        s.app.game.as_mut().unwrap().selected = Some(id);
        s.pass(&[]);
        s.app.building_sell();
        let told = s.pass(&[Command::Key(Key::Y, false, false)]);
        let removed = told.iter().find(|l| l.starts_with("world remove")).unwrap();
        assert!(removed.ends_with("refund 80"), "{removed}");
    });
}

#[test]
fn the_filter_panel_refreshes_every_second_of_the_clock() {
    with_game("filter-panel-clock", "", |s| {
        let id = filter_at(s, 10);
        s.app.game.as_mut().unwrap().selected = Some(id);
        // A count that changes at every pass tells each refresh by its line.
        let mut seen = Vec::new();
        for pass in 0..260u32 {
            let game = s.app.game.as_mut().unwrap();
            let at = game.world.find(id).unwrap();
            let f = game.world.zoo.entities[at]
                .common
                .as_mut()
                .unwrap()
                .filter
                .as_mut();
            f.unwrap().months = pass as i32 + 100;
            let told = s.log(&[]);
            if told.iter().any(|l| l.starts_with("panel filter ")) {
                seen.push(pass);
            }
        }
        // Shown at once, then once more as the forced refresh sets its due time (information
        // panels 5.1), then about every 50 passes of 20 ms (the interface timer ticks between).
        assert!(seen.len() >= 5, "{seen:?}");
        let gaps: Vec<u32> = seen.windows(2).skip(1).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().all(|g| (50..=53).contains(g)), "{seen:?}");
    });
}

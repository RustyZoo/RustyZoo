//! The terrain tool through the pass: the tabs, the panel, the brush on the map, the session, the
//! cash and the grid, on a made-up map and a made-up terrain panel.

use super::*;
use crate::bitmap::tests::{WHITE, bmp24};
use crate::entity::Catalogue;
use crate::render::order::project;
use crate::render::tests::{flat_map, frame_file, tile_with};
use crate::sprite::tests::Install;
use crate::terrain::Mode;
use crate::ui::draw::Strings as UiStrings;
use crate::ui::layout::Tree;
use crate::vfs::Vfs;

const SIZE: (u32, u32) = (128, 96);

struct Texts;

impl UiStrings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            3363 => "short".into(),
            3365 => "Grass".into(),
            3367 => "Sand".into(),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// A flat map of 40 by 40 tiles of grass whose camera looks at the middle of it and whose view
/// state stores the brush size `brush`.
fn map_with_brush(brush: u32) -> Zoo {
    let (px, py) = project(0, 1280, 1280, 0);
    let mut zoo = tile_with(&[]);
    zoo.width = 40;
    zoo.height = 40;
    zoo.tiles = flat_map(40, 40);
    zoo.view.scroll = (px - SIZE.0 as i32 / 2, py - SIZE.1 as i32 / 2);
    zoo.view.brush = brush;
    zoo
}

const ROOT: &str = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
    [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
    [b]\ntype=UILayout\nlayout=ui/c.lyt\n\
    [c]\ntype=UIButton\nid=1002\nx=110\ny=80\ndx=8\ndy=8\nstate=2048\naction=3\ntarget=8\n\
    transparent=0\n\
    [d]\ntype=UILayout\nlayout=ui/q.lyt\nstate=1\n\
    [e]\ntype=UIText\nid=7003\nlayer=10\nx=0\ny=0\ndx=1\ndy=1\ntimed=1\ntimerDuration=100\n\
    transparent=0\nstate=1025\n";
const QUESTION: &str = "[LayoutInfo]\nid=45000\nlayer=7\nx=0\ny=0\ndx=20\ndy=10\n\
    [a]\ntype=UIButton\nid=45010\ndx=8\ndy=8\ntarget=6\naction=5\ntransparent=0\n";
/// The construction panel with one list tab and the two terrain tabs, and the terrain panel in it.
const CONSTRUCTION: &str = "[LayoutInfo]\nid=8\nx=0\ny=0\ndx=40\ndy=90\nstate=1\nlayer=1\n\
    [t]\ntype=UIText\nid=3261\ndx=30\ndy=6\n\
    [s]\ntype=UIRadioSet\nid=3260\nbutton=3251\nbutton=3362\nbutton=3361\n\
    [a]\ntype=UIButton\nid=3251\nx=32\ny=2\ndx=6\ndy=6\nstate=4096\nstate=8\nstringData=fence\n\
    layer=3\ntransparent=0\n\
    [b]\ntype=UIButton\nid=3362\nx=32\ny=10\ndx=6\ndy=6\nstate=4096\nlayer=3\ntransparent=0\n\
    [c]\ntype=UIButton\nid=3361\nx=32\ny=18\ndx=6\ndy=6\nstate=4096\nlayer=3\ntransparent=0\n\
    [p]\ntype=UILayout\nlayout=ui/t.lyt\nstate=1\n";
const TERRAIN: &str = "[LayoutInfo]\nid=7\nx=0\ny=0\ndx=30\ndy=90\nlayer=2\nstate=1\n\
    [a]\ntype=UIText\nid=3305\nx=0\ny=40\ndx=28\ndy=6\n\
    [b]\ntype=UIText\nid=3304\nx=0\ny=47\ndx=28\ndy=6\n\
    [c]\ntype=UIText\nid=3306\nx=0\ny=54\ndx=28\ndy=6\n\
    [d]\ntype=UIText\nid=3319\nx=0\ny=61\ndx=28\ndy=6\n\
    [e]\ntype=UIText\nid=3321\nx=0\ny=68\ndx=28\ndy=6\n\
    [f]\ntype=UIButton\nid=3302\nx=0\ny=78\ndx=6\ndy=6\ntransparent=0\n\
    [g]\ntype=UIButton\nid=3303\nx=8\ny=78\ndx=6\ndy=6\ntransparent=0\n\
    [h]\ntype=UIButton\nid=3310\nx=0\ny=8\ndx=6\ndy=6\ntransparent=0\n\
    [i]\ntype=UIButton\nid=3314\nx=8\ny=8\ndx=6\ndy=6\ntransparent=0\n\
    [j]\ntype=UIRadioSet\nid=3370\nbutton=3315\nbutton=3316\n\
    [k]\ntype=UIButton\nid=3315\nx=0\ny=16\ndx=6\ndy=6\nstate=4096\ntransparent=0\n\
    [l]\ntype=UIButton\nid=3316\nx=8\ny=16\ndx=6\ndy=6\nstate=4096\ntransparent=0\n\
    [r]\ntype=UIScrollingRegion\nid=3350\nx=0\ny=20\ndx=28\ndy=16\nradioset=3353\n\
    template=3354\nscrollbar=3355\nspacing=1\nspacing=1\n\
    [s]\ntype=UIRadioSet\nid=3353\n\
    [u]\ntype=UIButton\nid=3354\ndx=8\ndy=8\ntransparent=0\nstate=256\nstate=4096\n\
    [v]\ntype=UIScrollBar\nid=3355\ndx=3\ndy=10\n";
/// Grass, sand and fresh water.
const TYPES: &str = "[ttGrass]\ntype=0\ncost=40\nhelpid=3365\n\
    [ttSand]\ntype=2\ncost=30\nhelpid=3367\n\
    [ttFresh]\ntype=9\ncost=30\nwater=1\nhelpid=3374\n";

fn install(name: &str) -> Install {
    let mut palette = 2u32.to_le_bytes().to_vec();
    palette.extend([0, 0, 0, 255, 200, 100, 50, 255]);
    let mut members = vec![
        ("terrain/tiletex.cfg".to_string(), TYPES.as_bytes().to_vec()),
        ("ztats/00.pal".to_string(), palette),
        (
            "tile.cfg".to_string(),
            b"[0]\nnorthgrid=g.bmp\neastgrid=g.bmp\nhighlight=h.bmp\n".to_vec(),
        ),
        (
            "g.bmp".to_string(),
            bmp24(&[vec![WHITE, [255, 0, 0]], vec![WHITE, WHITE]], false),
        ),
        (
            "h.bmp".to_string(),
            bmp24(&[vec![WHITE, [0, 0, 255]], vec![WHITE, WHITE]], false),
        ),
        ("ui/root.lyt".to_string(), ROOT.as_bytes().to_vec()),
        ("ui/q.lyt".to_string(), QUESTION.as_bytes().to_vec()),
        ("ui/c.lyt".to_string(), CONSTRUCTION.as_bytes().to_vec()),
        ("ui/t.lyt".to_string(), TERRAIN.as_bytes().to_vec()),
    ];
    for variation in 0..9 {
        members.push((format!("ztats/0000{variation:02x}.lle"), frame_file()));
    }
    let members: Vec<(&str, Vec<u8>)> = members
        .iter()
        .map(|(n, c)| (n.as_str(), c.clone()))
        .collect();
    Install::new(&format!("app-terrain-{name}"), &members)
}

/// Runs `body` on a game with the terrain panel and 50000 in cash.
fn with_game(name: &str, body: impl FnOnce(&mut Session)) {
    with_view(name, 0, body)
}

/// The same, on a map whose view state stores this brush size.
fn with_view(name: &str, brush: u32, body: impl FnOnce(&mut Session)) {
    let install = install(name);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map_with_brush(brush),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    app.set_interface(Interface::new(tree, Box::new(Texts)));
    app.interface().unwrap().set_hidden(1002, false);
    let mut money = Money::default();
    money.set_cash(50_000.0);
    app.set_money(money);
    let mut session = Session { app, now: 0 };
    session.pass(&[]);
    body(&mut session);
}

struct Session<'a> {
    app: App<'a>,
    now: u64,
}

impl Session<'_> {
    /// One pass; the lines of the families this file looks at, without their lead.
    fn pass(&mut self, commands: &[Command]) -> Vec<String> {
        self.app.pass(self.now, commands);
        let lines = self.app.log_lines(self.now / 20, self.now);
        self.now += 20;
        lines
            .into_iter()
            .filter_map(|l| {
                let rest = l.split_once(" t=")?.1.split_once(' ')?.1.to_string();
                ["terrain ", "money cash", "menu message", "world "]
                    .iter()
                    .any(|p| rest.starts_with(p))
                    .then_some(rest)
            })
            .collect()
    }

    fn click(&mut self, (x, y): (i32, i32)) -> Vec<String> {
        self.pass(&[
            Command::Mouse(Mouse::Move(x, y)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ])
    }

    fn hover(&mut self, at: (i32, i32)) -> Vec<String> {
        self.pass(&[Command::Mouse(Mouse::Move(at.0, at.1))])
    }

    /// The window point of the middle of tile (`tx`, `ty`).
    fn tile(&self, tx: i32, ty: i32) -> (i32, i32) {
        let camera = self.app.camera().unwrap();
        let p = project(camera.rotation, 64 * tx + 32, 64 * ty + 32, 0);
        (p.0 - camera.scroll.0, p.1 - camera.scroll.1)
    }

    fn game(&self) -> &Game {
        self.app.game.as_ref().unwrap()
    }

    fn text(&mut self, id: i32) -> String {
        let text = self.app.interface().unwrap().text(id).unwrap();
        String::from_utf8_lossy(&text).into_owned()
    }

    fn frame(&mut self) -> u64 {
        // Draw once more with what the last pass changed.
        self.app.pass(self.now, &[]);
        self.now += 20;
        frame_hash(&self.app.frame)
    }

    /// Opens the construction panel, selects the paint tab and the sand.
    fn open_paint(&mut self) {
        self.click((112, 82));
        self.click((35, 13));
        assert_eq!(self.game().terrain.mode(), Some(Mode::Paint));
        // The three types of the list are at 1, 10 and 19 across, 24 down.
        self.click((14, 28));
        assert_eq!(self.game().terrain.kind(), 2);
    }
}

#[test]
fn the_paint_tab_opens_the_panel_and_a_stroke_makes_a_priced_session() {
    with_game("paint", |s| {
        s.click((112, 82));
        assert!(!crate::build::terrain::shown(s.app.interface().unwrap()));
        let lines = s.click((35, 13));
        assert_eq!(lines, ["terrain enter paint"]);
        assert!(crate::build::terrain::shown(s.app.interface().unwrap()));
        assert_eq!(s.text(3306), "", "the subtitle is the string of the tab");
        assert_eq!(s.text(3305), "$0");
        assert!(s.game().grid && !s.game().choice, "the grid is forced on");
        // Sand: the name and the cost of the brush.
        assert_eq!(s.click((14, 28)), ["terrain type 2"]);
        assert_eq!(s.text(3319), "Sand");
        // The brush follows the pointer over the map, and its tiles are marked.
        let at = s.tile(20, 20);
        assert!(at.0 > 45 && at.0 < 120 && at.1 < 80, "{at:?}");
        assert_eq!(
            s.hover(at),
            ["terrain brush 2 20,20", "terrain amount -120"]
        );
        assert_eq!(s.game().marks.len(), 4);
        assert_eq!(s.text(3321), "$120");
        // A press paints the four tiles at once: the session costs 4 times 30.
        let lines = s.pass(&[Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)]);
        assert_eq!(
            lines,
            [
                "world stage",
                "terrain paint 4",
                "terrain cost 120",
                "terrain amount 0"
            ]
        );
        assert_eq!(s.text(3305), "$120");
        assert_eq!(s.text(3321), "$0", "nothing left to paint under the brush");
        let tiles = &s.game().world.zoo.tiles;
        assert_eq!(tiles[20 * 40 + 20].terrain, 2);
        assert_eq!(tiles[21 * 40 + 21].terrain, 2);
        assert_eq!(tiles[22 * 40 + 20].terrain, 0);
        assert_eq!(s.game().money.cash, 50_000.0, "nothing is paid yet");
    });
}

#[test]
fn the_paint_tab_writes_the_brush_cost_next_to_the_pointer_and_the_help_of_the_type() {
    with_game("pointer-text", |s| {
        s.open_paint();
        let at = s.tile(20, 20);
        let amount = |s: &Session| s.game().cost();
        assert_eq!(amount(s), None, "the pointer was not over the map yet");
        // The cost of painting the brush where it is, as an amount spent.
        s.hover(at);
        assert_eq!(amount(s), Some(-120));
        // The tooltip of the grass under the pointer shows after its delay, once per tile.
        for _ in 0..4 {
            s.pass(&[]);
        }
        assert_eq!(s.text(7003), "Grass");
        // Right after a paint it is 0, and the sand under the pointer has its own help.
        s.pass(&[Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)]);
        assert_eq!(amount(s), Some(0));
        let wait = |s: &mut Session| (0..8).for_each(|_| drop(s.pass(&[])));
        // The tile beside it is grass; the tooltip shows after its delay and expires.
        s.hover(s.tile(19, 20));
        (0..4).for_each(|_| drop(s.pass(&[])));
        assert_eq!(s.text(7003), "Grass");
        assert!(s.app.interface().unwrap().shown(7003));
        wait(s);
        assert!(
            !s.app.interface().unwrap().shown(7003),
            "the tooltip expired"
        );
        // The painted tile: its type's help, once; a move inside the tile asks for nothing more.
        s.hover(s.tile(20, 20));
        (0..4).for_each(|_| drop(s.pass(&[])));
        assert_eq!(s.text(7003), "Sand");
        wait(s);
        s.hover((s.tile(20, 20).0 + 2, s.tile(20, 20).1));
        wait(s);
        assert!(!s.app.interface().unwrap().shown(7003));
        // A held button shows no help, and off the map nothing is written.
        s.pass(&[Command::Mouse(Mouse::Down)]);
        let other = s.tile(20, 19);
        s.pass(&[Command::Mouse(Mouse::Move(other.0, other.1))]);
        wait(s);
        assert!(!s.app.interface().unwrap().shown(7003));
        // Released on another tile than the one asked about (painted on the way): its help shows
        // without a move.
        s.pass(&[Command::Mouse(Mouse::Up)]);
        (0..4).for_each(|_| drop(s.pass(&[])));
        assert_eq!(s.text(7003), "Sand");
        assert!(s.app.interface().unwrap().shown(7003));
        wait(s);
        // Off the map, over the panel and back on the same tile: nothing more to show.
        s.hover((-5, -5));
        assert_eq!(amount(s), None);
        s.hover(other);
        wait(s);
        assert!(!s.app.interface().unwrap().shown(7003));
    });
}

#[test]
fn the_brush_starts_at_the_size_the_map_stores_and_at_2_when_that_is_not_a_size() {
    // Only the low byte counts; 0, 6 and more are no size.
    for (stored, size) in [(3, 3), (1, 1), (5, 5), (0x103, 3), (0, 2), (6, 2), (255, 2)] {
        with_view(&format!("brush-{stored}"), stored, |s| {
            assert_eq!(s.game().terrain.size(), size, "stored {stored}");
        });
    }
}

#[test]
fn the_modify_tab_shows_no_amount_near_the_pointer() {
    with_game("pointer-modify", |s| {
        s.click((112, 82));
        s.click((35, 21));
        assert_eq!(s.game().terrain.mode(), Some(Mode::Modify));
        s.hover(s.tile(20, 20));
        assert_eq!(s.game().cost(), None);
    });
}

#[test]
fn cancel_gives_the_tiles_and_the_picture_back_and_accept_pays() {
    with_game("accept", |s| {
        s.open_paint();
        let at = s.tile(20, 20);
        s.hover(at);
        // The tooltip of the tile comes and goes before the picture is taken.
        (0..10).for_each(|_| drop(s.pass(&[])));
        let before = s.frame();
        s.pass(&[Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)]);
        let cancel = (11, 81);
        let lines = s.click(cancel);
        assert_eq!(
            lines,
            ["world stage", "terrain amount none", "terrain cancel 120"]
        );
        assert_eq!(s.text(3305), "$0");
        assert_eq!(s.game().world.zoo.tiles[20 * 40 + 20].terrain, 0);
        s.hover(at);
        assert_eq!(s.frame(), before, "the same picture as before the stroke");
        // Paint again and accept.
        s.pass(&[Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)]);
        let lines = s.click((3, 81));
        assert_eq!(lines[1], "terrain accept 120");
        assert!(lines.contains(&"money cash 49880".to_string()), "{lines:?}");
        assert_eq!(s.game().money.cash, 49_880.0);
        assert_eq!(s.game().money.total[crate::money::CONSTRUCTION], 120.0);
        assert_eq!(s.game().world.zoo.tiles[20 * 40 + 20].terrain, 2);
        // The accepted session is over: cancelling now changes nothing.
        assert!(s.click(cancel).is_empty());
        assert_eq!(s.game().world.zoo.tiles[20 * 40 + 20].terrain, 2);
    });
}

#[test]
fn leaving_accepts_what_the_cash_covers_and_cancels_with_a_message_otherwise() {
    for (cash, kept) in [(120.0, true), (119.0, false)] {
        with_game(&format!("leave-{kept}"), |s| {
            let mut money = Money::default();
            money.set_cash(cash);
            s.app.set_money(money);
            s.open_paint();
            s.hover(s.tile(20, 20));
            s.pass(&[Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)]);
            // Escape drops the tool and leaves it.
            let lines = s.pass(&[Command::Key(Key::Escape, false, false)]);
            let sand = s.game().world.zoo.tiles[20 * 40 + 20].terrain;
            assert_eq!(s.game().terrain.mode(), None);
            assert!(!s.game().grid, "the grid goes back to the player's choice");
            if kept {
                assert_eq!(sand, 2);
                assert_eq!(s.game().money.cash, 0.0);
                assert!(
                    lines.contains(&"terrain accept 120".to_string()),
                    "{lines:?}"
                );
            } else {
                assert_eq!(sand, 0);
                assert_eq!(s.game().money.cash, 119.0);
                assert!(
                    lines.contains(&"menu message short".to_string()),
                    "{lines:?}"
                );
                assert!(s.app.interface().unwrap().modal_open());
            }
        });
    }
}

#[test]
fn the_grid_is_forced_on_by_the_tool_and_goes_back_to_the_players_choice() {
    with_game("grid", |s| {
        // The player switched it on: it stays on after the tool.
        s.pass(&[Command::Grid]);
        assert!(s.game().grid && s.game().choice);
        s.open_paint();
        assert!(s.game().grid);
        s.pass(&[Command::Key(Key::Escape, false, false)]);
        assert!(s.game().grid, "the choice was on");
        // Off, then forced on while a terrain tab is the tool, and off again after it.
        s.pass(&[Command::Grid]);
        assert!(!s.game().grid && !s.game().choice);
        s.click((35, 13));
        assert!(s.game().grid && !s.game().choice);
        // Ctrl+G inside the tool is the player's new choice.
        s.pass(&[Command::Grid]);
        assert!(!s.game().grid && !s.game().choice);
        s.pass(&[Command::Grid]);
        assert!(s.game().grid && s.game().choice);
        // Another tab of the panel leaves the tool; the closing of the panel too.
        s.pass(&[Command::Grid]);
        s.click((35, 5));
        assert_eq!(s.game().terrain.mode(), None);
        assert!(!s.game().grid);
        s.click((35, 13));
        assert!(s.game().grid);
        s.click((112, 82));
        assert_eq!(s.game().terrain.mode(), None, "the panel is closed");
        assert!(!s.game().grid);
    });
}

#[test]
fn the_brush_size_buttons_and_keys_stay_between_1_and_5_and_are_kept() {
    with_game("size", |s| {
        s.click((112, 82));
        s.click((35, 13));
        let plus = |s: &mut Session| s.pass(&[Command::Char(b'+')]);
        let minus = |s: &mut Session| s.pass(&[Command::Char(b'-')]);
        assert_eq!(plus(s), ["terrain size 3"]);
        assert_eq!(plus(s), ["terrain size 4"]);
        assert_eq!(plus(s), ["terrain size 5"]);
        assert!(plus(s).is_empty(), "no more than 5");
        assert!(!s.app.interface().unwrap().press_button(3310), "disabled");
        for n in [4, 3, 2, 1] {
            assert_eq!(minus(s), [format!("terrain size {n}")]);
        }
        assert!(minus(s).is_empty(), "no less than 1");
        // The buttons do the same, and the size is kept when the panel is shown again.
        s.click((11, 11));
        s.click((3, 11));
        assert_eq!(s.game().terrain.size(), 2);
        plus(s);
        s.pass(&[Command::Key(Key::Escape, false, false)]);
        s.click((35, 13));
        assert_eq!(s.game().terrain.size(), 3);
        // The pointer over the map puts a brush of that size: 9 tiles.
        s.hover(s.tile(20, 20));
        assert_eq!(s.game().marks.len(), 9);
    });
}

#[test]
fn a_click_in_the_modify_tab_paints_nothing_and_shapes_nothing() {
    with_game("modify", |s| {
        s.click((112, 82));
        assert_eq!(s.click((35, 21)), ["terrain enter modify"]);
        s.hover(s.tile(20, 20));
        s.pass(&[Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)]);
        assert_eq!(s.game().marks.len(), 4);
        assert_eq!(s.game().terrain.cost(&s.game().world), 0.0);
        assert!(s.game().world.zoo.tiles.iter().all(|t| t.terrain == 0));
        // From the modify tab to the paint tab: the first is left, the second entered.
        let lines = s.click((35, 13));
        assert_eq!(lines, ["terrain leave", "terrain enter paint"]);
    });
}

impl Session<'_> {
    /// A stroke on the middle of tile (`tx`, `ty`) as it was before the ground changed, that
    /// moves the pointer up by each of `ups` pixels from where it pressed, in turn.
    fn stroke(&mut self, (tx, ty): (i32, i32), ups: &[i32]) -> Vec<String> {
        let at = self.tile(tx, ty);
        self.hover(at);
        let mut commands = vec![Command::Mouse(Mouse::Down)];
        commands.extend(
            ups.iter()
                .map(|up| Command::Mouse(Mouse::Move(at.0, at.1 - up))),
        );
        commands.push(Command::Mouse(Mouse::Up));
        self.pass(&commands)
    }
}

#[test]
fn dragging_up_and_down_shapes_the_ground_with_the_cost_of_the_session() {
    with_game("shape", |s| {
        s.click((112, 82));
        s.click((35, 21));
        assert_eq!(s.game().terrain.mode(), Some(Mode::Modify));
        s.hover(s.tile(20, 20));
        let flat = s.frame();
        // Contiguous (the first mode): one raise of the 2 by 2 brush.
        let lines = s.stroke((20, 20), &[16]);
        assert_eq!(
            lines,
            ["world stage", "terrain shape 16", "terrain cost 128"]
        );
        assert_eq!(s.text(3305), "$128");
        let raised = s.frame();
        assert_ne!(raised, flat);
        // Cancel gives the ground and the picture back.
        let lines = s.click((11, 81));
        assert_eq!(lines, ["world stage", "terrain cancel 128"]);
        assert_eq!(s.text(3305), "$0");
        assert_eq!(s.frame(), flat);
        // The same stroke again is the same picture; two raises in a stroke cost 384.
        s.stroke((20, 20), &[16]);
        assert_eq!(s.frame(), raised);
        s.click((11, 81));
        let lines = s.stroke((20, 20), &[32]);
        assert_eq!(lines[1..], ["terrain shape 32", "terrain cost 384"]);
        s.click((11, 81));
        // A raise then a lower, in one stroke, costs nothing.
        let lines = s.stroke((20, 20), &[16, 0]);
        assert_eq!(
            lines[1..],
            [
                "terrain shape 16",
                "terrain cost 128",
                "terrain shape 16",
                "terrain cost 0"
            ]
        );
        assert_eq!(s.frame(), flat);
        // Not contiguous: the second mode button, and a raise of the brush alone.
        assert_eq!(s.click((11, 19)), ["terrain mode 3316"]);
        assert_eq!(s.game().terrain.shaping(), (false, false));
        let lines = s.stroke((20, 20), &[16]);
        assert_eq!(lines[1..], ["terrain shape 4", "terrain cost 32"]);
        assert_ne!(s.frame(), raised);
        assert_eq!(s.game().world.zoo.tiles[20 * 40 + 20].elevation, 1);
        // Accepting pays 32.
        let lines = s.click((3, 81));
        assert_eq!(lines[0], "terrain accept 32");
        assert_eq!(s.game().money.cash, 50_000.0 - 32.0);
    });
}

//! The tabs of the zoo status window through the pass: the zoo name and the price, the graphs and
//! the commerce list, on the made-up map of the bulldozer tests and a made-up window.

use super::bulldozer_tests::{Session, catalogue, with_game_of};
use super::*;
use crate::building::Live;
use crate::entity::{Catalogue, EntityType};
use crate::fixtures;
use crate::ui::layout::DISABLED;

const WINDOW_LAYOUT: &str = "[LayoutInfo]\nid=14\nlayer=7\nx=0\ny=0\ndx=300\ndy=200\nstate=1\n\
    [a]\ntype=UIEditableText\nid=4199\nx=0\ny=0\ndx=80\ndy=12\n\
    [b]\ntype=UIEditableText\nid=4190\nx=0\ny=14\ndx=60\ndy=12\nnumeric=1\ncharlimit=7\n\
    [c]\ntype=UIButton\nid=4195\nx=100\ny=0\ndx=8\ndy=8\ntransparent=0\n\
    [d]\ntype=UIButton\nid=4191\nx=100\ny=10\ndx=8\ndy=8\ntransparent=0\n\
    [e]\ntype=UIText\nid=4193\nx=120\ny=0\ndx=40\ndy=12\n\
    [f]\ntype=UIText\nid=4194\nx=120\ny=14\ndx=40\ndy=12\n\
    [g1]\ntype=UIText\nid=4115\nx=0\ny=30\ndx=40\ndy=12\n\
    [g2]\ntype=UIText\nid=4117\nx=0\ny=42\ndx=40\ndy=12\n\
    [g3]\ntype=UIText\nid=4119\nx=0\ny=54\ndx=40\ndy=12\n\
    [g4]\ntype=UIText\nid=4121\nx=0\ny=66\ndx=40\ndy=12\n\
    [g5]\ntype=UIText\nid=4122\nx=0\ny=78\ndx=40\ndy=12\n\
    [g6]\ntype=UIText\nid=4171\nx=0\ny=90\ndx=40\ndy=12\n\
    [h1]\ntype=UIGraph\nid=4150\nx=0\ny=110\ndx=100\ndy=60\nForcedMinY=0\nForcedMaxY=100\n\
    [h2]\ntype=UIGraph\nid=13978\nx=130\ny=0\ndx=50\ndy=60\nyIsMoney=1\n\
    [h3]\ntype=UIGraph\nid=13967\nx=190\ny=0\ndx=50\ndy=60\nyIsMoney=1\n\
    [h4]\ntype=UIGraph\nid=13965\nx=240\ny=0\ndx=50\ndy=60\nForcedMinY=0\n\
    [s1]\ntype=UIButton\nid=4151\nx=110\ny=0\ndx=8\ndy=8\ntransparent=0\n\
    [s2]\ntype=UIButton\nid=4152\nx=110\ny=10\ndx=8\ndy=8\ntransparent=0\n\
    [u1]\ntype=UIButton\nid=4104\nx=0\ny=180\ndx=8\ndy=8\nstate=2048\n\
    [u2]\ntype=UIButton\nid=4154\nx=10\ny=180\ndx=8\ndy=8\nstate=2048\n\
    [u3]\ntype=UIButton\nid=4106\nx=20\ny=180\ndx=8\ndy=8\nstate=2048\n\
    [u4]\ntype=UIButton\nid=13983\nx=30\ny=180\ndx=8\ndy=8\nstate=2048\n\
    [t1]\ntype=UILayout\nlayout=ui/zc.lyt\nstate=1\n\
    [t2]\ntype=UILayout\nlayout=ui/zr.lyt\nstate=1\n";
const COMMERCE_LAYOUT: &str = "[LayoutInfo]\nid=810\nlayer=1\nx=0\ny=0\ndx=300\ndy=200\nstate=1\n\
    [l]\ntype=UIListBox\nid=4172\nx=150\ny=100\ndx=140\ndy=90\ncolumn=0\ncolumn=0\ncolumn=50\n\
    column=70\ncolumn=100\ncolumn=120\nhandledoubleclicks=1\nstate=131072\nlayer=1\n\
    [a]\ntype=UIButton\nid=4175\nx=150\ny=80\ndx=8\ndy=8\ntransparent=0\n\
    [b]\ntype=UIButton\nid=4176\nx=160\ny=80\ndx=8\ndy=8\ntransparent=0\n\
    [c]\ntype=UIButton\nid=4177\nx=170\ny=80\ndx=8\ndy=8\ntransparent=0\n\
    [d]\ntype=UIButton\nid=4178\nx=180\ny=80\ndx=8\ndy=8\ntransparent=0\n\
    [e]\ntype=UIButton\nid=4179\nx=190\ny=80\ndx=8\ndy=8\ntransparent=0\n\
    [h1]\ntype=UIText\nid=4184\nx=150\ny=70\ndx=8\ndy=8\nforecolor=255\nforecolor=228\nforecolor=173\n\
    [h2]\ntype=UIText\nid=4185\nx=160\ny=70\ndx=8\ndy=8\nforecolor=255\nforecolor=228\nforecolor=173\n\
    [h3]\ntype=UIText\nid=4186\nx=170\ny=70\ndx=8\ndy=8\nforecolor=255\nforecolor=228\nforecolor=173\n\
    [h4]\ntype=UIText\nid=4187\nx=180\ny=70\ndx=8\ndy=8\nforecolor=255\nforecolor=228\nforecolor=173\n\
    [h5]\ntype=UIText\nid=4188\nx=190\ny=70\ndx=8\ndy=8\nforecolor=255\nforecolor=228\nforecolor=173\n";
const RESEARCH_LAYOUT: &str = "[LayoutInfo]\nid=804\nlayer=1\nx=0\ny=0\ndx=300\ndy=200\nstate=1\n\
    [l]\ntype=UIListBox\nid=4164\nx=150\ny=100\ndx=140\ndy=90\nminiconwidth=0\nlayer=1\n";

/// The sorted names of the commerce types: 20 and 21 sell, 22 satisfies a need without selling, 23
/// is an attraction, 24 is the entrance hall.
const SELLS: &str = "[Characteristics/Integers]\ncCommerce=1\ncPurchaseCost=200\n[Satisfies]\nbuilding\n";

fn ty(name: &[u8], ini: &str) -> EntityType {
    let ini = format!("{ini}[Animations]\nidle=dozer/obj.ani\n");
    EntityType {
        name_id: 9001,
        footprint: [2, 2, 1],
        ..fixtures::ty("building", std::str::from_utf8(name).unwrap(), "", &ini)
    }
}

fn types() -> Catalogue {
    let mut cat = catalogue();
    cat.push(ty(b"stall", SELLS));
    cat.push(ty(b"burger", SELLS));
    cat.push(ty(b"bench", "[Satisfies]\nbuilding\n"));
    cat.push(ty(b"ride", "[Satisfies]\nfun\n"));
    cat.push(ty(b"hall", ""));
    cat
}

const GUEST: usize = 14;
const STALL: usize = 20;
const BURGER: usize = 21;
const BENCH: usize = 22;
const RIDE: usize = 23;
const HALL: usize = 24;

fn root() -> String {
    let more = "[w]\ntype=UILayout\nlayout=ui/zs.lyt\nstate=1\n";
    format!("{}{more}", super::bulldozer_tests::ROOT)
}

fn run(name: &str, body: impl FnOnce(&mut Session)) {
    let more = [
        ("ui/zs.lyt", WINDOW_LAYOUT.as_bytes().to_vec()),
        ("ui/zc.lyt", COMMERCE_LAYOUT.as_bytes().to_vec()),
        ("ui/zr.lyt", RESEARCH_LAYOUT.as_bytes().to_vec()),
    ];
    let root = root();
    let ini = "[UI]\ndefaultEditCharLimit=22\n";
    with_game_of(name, ini, (types(), &root, &more), body);
}

/// Where the entities of a test stand: the tiles of a row.
fn at(tile: i32) -> (i32, i32) {
    (tile * 64 + 32, 5 * 64 + 32)
}

fn text(s: &mut Session, id: i32) -> String {
    let bytes = s.app.interface().unwrap().text(id).unwrap_or_default();
    String::from_utf8(bytes).unwrap()
}

/// Shows the window with its tabs hidden but the first, as the radio set of the real layout does.
fn open(s: &mut Session) {
    let ui = s.app.interface().unwrap();
    ui.show(14);
    ui.set_hidden(810, true);
    ui.set_hidden(804, true);
    s.pass(&[]);
}

fn passes(s: &mut Session, n: usize) {
    for _ in 0..n {
        s.pass(&[]);
    }
}

/// Adds the entrance building named `name`, and returns its entity id.
fn entrance(s: &mut Session, name: &str) -> u32 {
    let (x, y) = at(20);
    let id = s.add(HALL, x, y - 5 * 64 + 20 * 64, 0);
    let game = s.app.game.as_mut().unwrap();
    game.world.zoo.entrance = (20, 20);
    let i = game.world.find(id).unwrap();
    game.world.zoo.entities[i].common.as_mut().unwrap().name = name.into();
    id
}

fn name_of(s: &Session, id: u32) -> String {
    let world = &s.app.game.as_ref().unwrap().world;
    let i = world.find(id).unwrap();
    world.zoo.entities[i].common.as_ref().unwrap().name.clone()
}

/// A commerce building named `name` on tile `tile` with its figures: guests, income and upkeep
/// since construction.
fn shop(s: &mut Session, ty: usize, tile: i32, name: &str, figures: (u32, f32, f32)) -> u32 {
    let (x, y) = at(tile);
    let id = s.add(ty, x, y, 0);
    let game = s.app.game.as_mut().unwrap();
    let i = game.world.find(id).unwrap();
    game.world.zoo.entities[i].common.as_mut().unwrap().name = name.into();
    let live = Live {
        guests: [0, 0, figures.0],
        income: [0.0, 0.0, figures.1],
        upkeep: figures.2,
        ..Live::default()
    };
    game.world.units.buildings.insert(id, live);
    id
}

#[test]
fn opening_the_window_writes_the_zoo_name_the_counts_and_the_prices() {
    run("zw-open", |s| {
        entrance(s, "Fox Park");
        s.add(RIDE, at(3).0, at(3).1, 0);
        open(s);
        assert_eq!(text(s, 4199), "Fox Park");
        // The entrance hall is a building, the ride an attraction.
        assert_eq!(text(s, 4115), "0");
        assert_eq!(text(s, 4119), "1");
        assert_eq!(text(s, 4171), "0");
        let price = s.app.sim.status.price;
        assert_eq!(text(s, 4193), format!("${price:.2}"));
        assert_eq!(text(s, 4194), format!("${:.2}", price * 0.5));
        assert_eq!(text(s, 4190), format!("${price:.2}"));
        // The name field has the length limit of the name fields.
        let ui = s.app.interface().unwrap();
        let i = ui.tree.find(4199).unwrap();
        let crate::ui::layout::Kind::EditableText(f) = &ui.tree.elements[i].kind else {
            panic!("not a field");
        };
        assert_eq!(f.char_limit, 22);
    });
}

#[test]
fn a_zoo_without_an_entrance_building_leaves_the_name_field_alone() {
    run("zw-no-entrance", |s| {
        s.app.interface().unwrap().set_text(4199, b"typed");
        open(s);
        assert_eq!(text(s, 4199), "typed");
    });
}

#[test]
fn the_window_refreshes_each_ten_seconds_and_when_a_building_comes_or_goes() {
    run("zw-refresh", |s| {
        open(s);
        assert_eq!(text(s, 4119), "0");
        // A ride that comes shows at the next look, a building having come.
        s.add(RIDE, at(3).0, at(3).1, 0);
        s.pass(&[]);
        assert_eq!(text(s, 4119), "1");
        // A guest does not force a refresh: the count waits for the ten seconds (500 passes of
        // 20 ms), counted from the last refresh.
        s.add(GUEST, at(5).0, at(5).1, 0);
        s.pass(&[]);
        assert_eq!(text(s, 4121), "0");
        passes(s, 520);
        assert_eq!(text(s, 4121), "1");
    });
}

#[test]
fn a_window_that_stays_hidden_writes_nothing() {
    run("zw-hidden", |s| {
        s.add(RIDE, at(3).0, at(3).1, 0);
        passes(s, 600);
        assert_eq!(text(s, 4119), "");
    });
}

#[test]
fn editing_the_zoo_name_renames_the_entrance_building_and_a_blank_name_puts_the_old_one_back() {
    run("zw-name", |s| {
        let id = entrance(s, "Fox Park");
        open(s);
        let edit = |s: &mut Session, typed: &str| {
            s.app.interface().unwrap().set_text(4199, typed.as_bytes());
            s.app.zoo_rename();
        };
        edit(s, "Bear Land");
        assert_eq!(name_of(s, id), "Bear Land");
        // Blank: the field goes back to the last name, the building keeps it.
        for blank in ["", "   "] {
            edit(s, blank);
            assert_eq!((name_of(s, id), text(s, 4199)), ("Bear Land".into(), "Bear Land".into()));
        }
        // A tab is a character, which the name setter does not accept as a visible one: the
        // building keeps its name and the field keeps the tab, which a blank text then restores.
        edit(s, "\t");
        assert_eq!((name_of(s, id), text(s, 4199)), ("Bear Land".into(), "\t".into()));
        edit(s, "");
        assert_eq!(text(s, 4199), "\t");
    });
}

#[test]
fn a_name_typed_with_no_entrance_building_is_lost_and_the_field_keeps_it() {
    run("zw-name-lost", |s| {
        open(s);
        s.app.interface().unwrap().set_text(4199, b"Nobody");
        s.app.zoo_rename();
        assert_eq!(text(s, 4199), "Nobody");
        // A blank text puts back the copy, which is empty.
        s.app.interface().unwrap().set_text(4199, b"  ");
        s.app.zoo_rename();
        assert_eq!(text(s, 4199), "");
    });
}

#[test]
fn the_field_action_of_the_name_runs_at_the_end_of_the_edit() {
    run("zw-name-action", |s| {
        let id = entrance(s, "Fox Park");
        open(s);
        // Click the field, type, press Enter.
        let ui = s.app.interface().unwrap();
        assert!(ui.focus(4199));
        s.pass(&[Command::Key(Key::End, false, false)]);
        s.pass(&[Command::Char(b'!'), Command::Key(Key::Enter, false, false)]);
        assert_eq!(name_of(s, id), "Fox Park!");
    });
}

fn price(s: &Session) -> f32 {
    s.app.sim.status.price
}

#[test]
fn the_price_buttons_move_the_price_by_a_quarter_inside_the_range() {
    run("zw-price", |s| {
        s.app.sim.status.price = 22.0;
        open(s);
        s.pass(&[Command::Press(4195)]);
        assert_eq!((price(s), text(s, 4193)), (22.25, "$22.25".into()));
        assert_eq!(text(s, 4194), "$11.13");
        s.pass(&[Command::Press(4191), Command::Press(4191)]);
        assert_eq!(price(s), 21.75);
        assert_eq!(text(s, 4190), "$21.75");
        // At the maximum the up button is disabled, at the minimum the down button.
        let (min, max) = (s.app.sim.status.settings.min_price, s.app.sim.status.settings.max_price);
        let off = |s: &mut Session, id| {
            let ui = s.app.interface().unwrap();
            ui.tree.elements[ui.tree.find(id).unwrap()].state & DISABLED != 0
        };
        s.app.sim.status.price = max;
        s.pass(&[Command::Press(4195)]);
        assert_eq!((price(s), off(s, 4195), off(s, 4191)), (max, true, false));
        s.app.sim.status.price = min;
        s.pass(&[Command::Press(4191)]);
        assert_eq!((price(s), off(s, 4191), off(s, 4195)), (min, true, false));
    });
}

#[test]
fn a_typed_price_is_limited_but_the_field_shows_the_typed_number_until_the_next_refresh() {
    run("zw-price-typed", |s| {
        s.app.sim.status.price = 22.0;
        open(s);
        let max = s.app.sim.status.settings.max_price;
        let type_in = |s: &mut Session, typed: &str| {
            s.app.interface().unwrap().set_text(4190, typed.as_bytes());
            s.app.zoo_price_typed();
        };
        type_in(s, "500");
        // The original's bug: the price and the labels use the limit, the field the typed number.
        assert_eq!(price(s), max);
        assert_eq!((text(s, 4193), text(s, 4190)), (format!("${max:.2}"), "$500.00".into()));
        // The refresh writes the field again.
        passes(s, 520);
        assert_eq!(text(s, 4190), format!("${max:.2}"));
        // A currency symbol and the group separator are cut; no number is 0, a negative one too.
        type_in(s, "$12.50");
        assert_eq!(price(s), 12.5);
        let min = s.app.sim.status.settings.min_price;
        for typed in ["", "abc", "-5", "0"] {
            type_in(s, typed);
            assert_eq!(price(s), min, "{typed:?}");
        }
        type_in(s, "99999999999999999999999999");
        assert_eq!(price(s), max);
    });
}

#[test]
fn a_price_field_with_the_focus_is_not_rewritten_by_a_refresh() {
    run("zw-price-focus", |s| {
        s.app.sim.status.price = 22.0;
        open(s);
        assert!(s.app.interface().unwrap().focus(4190));
        s.app.interface().unwrap().set_text(4190, b"12");
        passes(s, 520);
        assert_eq!(text(s, 4190), "12");
        assert_eq!(text(s, 4193), "$22.00");
    });
}

#[test]
fn the_switch_buttons_give_their_graph_line_or_bar_and_the_graphs_hold_the_accounts() {
    use crate::ui::graph::Style;
    run("zw-graphs", |s| {
        s.app.game.as_mut().unwrap().money.monthly[crate::money::RATING][0] = 61.9;
        open(s);
        let style = |s: &mut Session, id| s.app.interface().unwrap().graph_style(id);
        assert_eq!(style(s, 4150), Some(Style::Line));
        s.pass(&[Command::Press(4152)]);
        assert_eq!(style(s, 4150), Some(Style::Bar));
        // Only its own graph.
        assert_eq!(style(s, 13978), Some(Style::Line));
        s.pass(&[Command::Press(4151)]);
        assert_eq!(style(s, 4150), Some(Style::Line));
        // The first month of the first year: one point, the rating cut toward zero.
        let ui = s.app.interface().unwrap();
        let i = ui.tree.find(4150).unwrap();
        let crate::ui::layout::Kind::Graph(g) = &ui.tree.elements[i].kind else {
            panic!("not a graph");
        };
        assert_eq!(g.points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(), [(11, 61)]);
        assert_eq!(g.symbol, "$");
    });
}

fn selected_text(s: &mut Session, id: i32) -> Option<usize> {
    s.app.interface().unwrap().selected_row(id)
}

fn rows(s: &mut Session) -> Vec<Vec<u8>> {
    let ui = s.app.interface().unwrap();
    let i = ui.tree.find(4172).unwrap();
    let crate::ui::layout::Kind::ListBox(l) = &ui.tree.elements[i].kind else {
        panic!("not a list");
    };
    l.rows.rows.iter().map(|r| r.text.clone()).collect()
}

fn names(s: &mut Session) -> Vec<String> {
    rows(s).into_iter().map(|r| String::from_utf8(r).unwrap()).collect()
}

fn show_commerce(s: &mut Session) {
    open(s);
    s.app.interface().unwrap().set_hidden(810, false);
    s.pass(&[]);
}

#[test]
fn the_commerce_list_holds_the_buildings_that_sell_sorted_by_type_then_name() {
    run("zw-commerce", |s| {
        shop(s, STALL, 3, "Zed", (5, 100.0, 40.0));
        shop(s, BURGER, 5, "Amy", (9, 10.0, 40.0));
        shop(s, STALL, 7, "Bob", (1, 0.0, 0.0));
        // Not listed: it sells nothing, and neither does an attraction.
        shop(s, BENCH, 9, "Bench", (0, 0.0, 0.0));
        s.add(RIDE, at(11).0, at(11).1, 0);
        show_commerce(s);
        // The subtype of a type is the one of the catalogue's types (empty here): the names.
        assert_eq!(names(s), ["Amy", "Bob", "Zed"]);
        // The header of the current key is coloured, the others have the layout's.
        let colour = |s: &mut Session, id| {
            let ui = s.app.interface().unwrap();
            ui.tree.elements[ui.tree.find(id).unwrap()].look.fore_color
        };
        assert_eq!(colour(s, 4184), [255, 217, 90]);
        assert_eq!(colour(s, 4185), [255, 228, 173]);
    });
}

#[test]
fn a_sort_button_re_sorts_flips_the_current_key_and_highlights_its_header() {
    run("zw-commerce-sort", |s| {
        shop(s, STALL, 3, "Zed", (5, 100.0, 40.0));
        shop(s, BURGER, 5, "Amy", (9, 10.0, 40.0));
        shop(s, STALL, 7, "Bob", (1, 0.0, 0.0));
        show_commerce(s);
        let colour = |s: &mut Session, id| {
            let ui = s.app.interface().unwrap();
            ui.tree.elements[ui.tree.find(id).unwrap()].look.fore_color
        };
        // Guests: descending first.
        s.pass(&[Command::Press(4177)]);
        assert_eq!(names(s), ["Amy", "Zed", "Bob"]);
        assert_eq!((colour(s, 4186), colour(s, 4184)), ([255, 217, 90], [255, 228, 173]));
        // The same key again flips it.
        s.pass(&[Command::Press(4177)]);
        assert_eq!(names(s), ["Bob", "Zed", "Amy"]);
        // Profit descending: Zed 60, Bob 0, Amy -30.
        s.pass(&[Command::Press(4178)]);
        assert_eq!(names(s), ["Zed", "Bob", "Amy"]);
        // The key that was current keeps the direction the flip gave it when it comes back.
        s.pass(&[Command::Press(4177)]);
        assert_eq!(names(s), ["Bob", "Zed", "Amy"]);
        // The first key: ascending as at the start; a second press flips it.
        s.pass(&[Command::Press(4175)]);
        assert_eq!(names(s), ["Amy", "Bob", "Zed"]);
        s.pass(&[Command::Press(4175)]);
        assert_eq!(names(s), ["Zed", "Bob", "Amy"]);
    });
}

#[test]
fn a_click_on_a_row_selects_the_building_and_a_second_click_on_it_does_nothing() {
    run("zw-commerce-click", |s| {
        let zed = shop(s, STALL, 3, "Zed", (5, 100.0, 40.0));
        let amy = shop(s, BURGER, 5, "Amy", (9, 10.0, 40.0));
        show_commerce(s);
        let click = |y| {
            [
                Command::Mouse(Mouse::Move(160, y)),
                Command::Mouse(Mouse::Down),
                Command::Mouse(Mouse::Up),
            ]
        };
        let selected = |s: &Session| s.app.game.as_ref().unwrap().selected;
        assert_eq!(selected(s), None);
        // The first row is Amy (burger sorts before stall by name only: the same empty subtype).
        s.pass(&click(106));
        assert_eq!(selected(s), Some(amy));
        let scroll = s.app.game.as_ref().unwrap().camera.scroll;
        s.pass(&click(118));
        assert_eq!(selected(s), Some(zed));
        // Clicking the selected building's row again leaves the view where it is.
        let scroll_after = s.app.game.as_ref().unwrap().camera.scroll;
        assert_ne!(scroll, scroll_after, "the view centred on the building");
        s.pass(&click(118));
        assert_eq!(s.app.game.as_ref().unwrap().camera.scroll, scroll_after);
        assert_eq!(selected_text(s, 4172), Some(1));
    });
}

#[test]
fn a_double_click_hides_the_window_with_a_building_selected_and_a_single_click_does_not() {
    run("zw-commerce-double", |s| {
        let amy = shop(s, BURGER, 5, "Amy", (9, 10.0, 40.0));
        show_commerce(s);
        let move_to = Command::Mouse(Mouse::Move(160, 106));
        s.pass(&[move_to, Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)]);
        assert!(s.app.interface().unwrap().shown(14));
        s.pass(&[Command::Mouse(Mouse::Double), Command::Mouse(Mouse::Up)]);
        assert!(!s.app.interface().unwrap().shown(14));
        // The building stays selected.
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(amy));
    });
}

#[test]
fn delete_in_the_list_sells_the_selected_building_and_gives_the_list_the_focus_back() {
    run("zw-commerce-delete", |s| {
        let amy = shop(s, BURGER, 5, "Amy", (9, 10.0, 40.0));
        let zed = shop(s, STALL, 3, "Zed", (5, 100.0, 40.0));
        show_commerce(s);
        let click = [
            Command::Mouse(Mouse::Move(160, 106)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ];
        s.pass(&click);
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(amy));
        let cash = s.cash();
        s.pass(&[Command::Key(Key::Delete, false, false)]);
        let world = &s.app.game.as_ref().unwrap().world;
        assert!(world.find(amy).is_none() && world.find(zed).is_some());
        assert!(s.cash() > cash, "refunded");
        // The list is made again and has the focus.
        s.pass(&[]);
        assert_eq!(names(s), ["Zed"]);
        assert!(s.app.interface().unwrap().has_focus(4172));
    });
}

#[test]
fn delete_with_nothing_selected_does_not_press_the_bulldozer_from_the_list() {
    run("zw-commerce-delete-none", |s| {
        shop(s, BURGER, 5, "Amy", (9, 10.0, 40.0));
        show_commerce(s);
        assert!(s.app.interface().unwrap().focus(4172));
        let lines = s.pass(&[Command::Key(Key::Delete, false, false)]);
        assert!(lines.is_empty(), "{lines:?}");
        assert!(!s.app.game.as_ref().unwrap().build.bulldozing());
        assert_eq!(names(s), ["Amy"]);
    });
}

#[test]
fn the_selected_building_is_selected_again_after_the_list_is_made_again() {
    run("zw-commerce-keep", |s| {
        shop(s, STALL, 3, "Zed", (5, 100.0, 40.0));
        shop(s, BURGER, 5, "Amy", (9, 10.0, 40.0));
        show_commerce(s);
        let click = [
            Command::Mouse(Mouse::Move(160, 118)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ];
        s.pass(&click);
        assert_eq!(selected_text(s, 4172), Some(1));
        // The sort reverses the rows: the row of Zed is the first now and stays selected.
        s.pass(&[Command::Press(4175)]);
        assert_eq!(names(s), ["Zed", "Amy"]);
        assert_eq!(selected_text(s, 4172), Some(0));
        // A building that is added changes the list; the selection follows its entity.
        shop(s, STALL, 9, "Aaa", (0, 0.0, 0.0));
        s.pass(&[]);
        assert_eq!(names(s), ["Zed", "Amy", "Aaa"]);
        assert_eq!(selected_text(s, 4172), Some(0));
    });
}

fn completed<'a>(s: &'a mut Session<'_>) -> &'a mut crate::status::completed::Completed {
    &mut s.app.game.as_mut().unwrap().window.completed
}

fn research_rows(s: &mut Session) -> usize {
    let ui = s.app.interface().unwrap();
    let i = ui.tree.find(4164).unwrap();
    let crate::ui::layout::Kind::ListBox(l) = &ui.tree.elements[i].kind else {
        panic!("not a list");
    };
    l.rows.rows.len()
}

#[test]
fn a_completion_adds_a_row_to_the_research_list_once_whether_the_window_is_shown_or_not() {
    use crate::research::{Branch, Category, Program, Tree};
    run("zw-research", |s| {
        let program = |name| Program {
            name,
            cost: 10.0,
            progress: 10.0,
            ..Program::default()
        };
        let tree = Tree {
            branches: vec![Branch {
                categories: vec![Category {
                    programs: vec![program(1), program(2)],
                    ..Category::default()
                }],
                ..Branch::default()
            }],
            ..Tree::default()
        };
        assert_eq!(research_rows(s), 0);
        completed(s).add(&tree, 1);
        // Written at the next look, the window shown or not.
        s.pass(&[]);
        assert_eq!(research_rows(s), 1);
        completed(s).add(&tree, 2);
        completed(s).add(&tree, 2);
        s.pass(&[]);
        assert_eq!(research_rows(s), 2);
        assert_eq!(completed(s).rows.iter().map(|r| r.0).collect::<Vec<_>>(), [2, 1]);
    });
}

#[test]
fn a_new_game_puts_the_tabs_and_the_graph_tabs_back_to_their_first() {
    use crate::render::tests::{flat_map, tile_with};
    run("zw-tabs", |s| {
        let ui = s.app.interface().unwrap();
        ui.set_button(4104, false, true);
        ui.set_button(4154, true, true);
        ui.set_button(4106, false, true);
        ui.set_button(13983, true, true);
        let mut zoo = tile_with(&[]);
        zoo.width = 4;
        zoo.height = 4;
        zoo.tiles = flat_map(4, 4);
        s.app.open_game(zoo, Money::default()).unwrap();
        let ui = s.app.interface().unwrap();
        let down = [4104, 4154, 4106, 13983].map(|id| ui.is_down(id));
        assert_eq!(down, [true, false, true, false]);
    });
}

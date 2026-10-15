use std::collections::HashMap;

use super::*;
use crate::app::Mouse;
use crate::sprite::tests::Install;
use crate::text::Font;
use crate::ui::draw::Strings;
use crate::ui::layout::Tree;
use crate::ui::widgets::Row;
use crate::vfs::Vfs;

struct Texts(HashMap<i32, &'static str>);

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        self.0.get(&id).map_or(String::new(), |s| s.to_string())
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// A window of 100 x 80. Buttons 20 (help 5), 21 (help 6, no long text), 22 (no help) and 23
/// (help 0), a list 30 with rows of help 7 and 0, and the tooltip 7003: a text with a border of
/// 1 pixel, timed for 100 ms.
fn interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [b]\ntype=UILayout\nlayout=ui/p.lyt\n[c]\ntype=UILayout\nlayout=ui/q.lyt\n";
    let question = "[LayoutInfo]\nid=100\nlayer=7\nx=60\ny=50\ndx=30\ndy=20\n\
        [a]\ntype=UIButton\nid=112\nx=10\ny=0\ndx=10\ndy=10\ncolorasicon=1\naction=5\n\
        target=1\nhelpid=5\n";
    let button = |id: i32, x: i32, help: &str| {
        format!("[b{id}]\ntype=UIButton\nid={id}\nx={x}\ny=0\ndx=10\ndy=10\ncolorasicon=1\n{help}")
    };
    let panel = format!(
        "[LayoutInfo]\nid=10\nlayer=1\nx=0\ny=0\ndx=whole\ndy=whole\n{}{}{}{}\
         [c]\ntype=UIListBox\nid=30\nx=0\ny=20\ndx=40\ndy=36\n\
         [d]\ntype=UIText\nid=7003\nlayer=10\nx=0\ny=0\ndx=1\ndy=1\nborder=1\ntimed=1\n\
         timerDuration=100\nshowborder=1\ntransparent=0\nstate=1025\n",
        button(20, 0, "helpid=5\n"),
        button(21, 12, "helpid=6\n"),
        button(22, 24, ""),
        button(23, 36, "helpid=0\n"),
    );
    let install = Install::new(
        &format!("tooltip-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/p.lyt", panel.into_bytes()),
            ("ui/q.lyt", question.as_bytes().to_vec()),
        ],
    );
    let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(100, 80, |_| 12);
    let texts = Texts(
        [
            (5, "short five"),
            (30005, "long help five"),
            (6, "short six"),
            (7, "row seven"),
        ]
        .into(),
    );
    let mut ui = Interface::new(tree, Box::new(texts));
    ui.show(10);
    // The tooltip layer shows it hidden: state 1025 is hidden and timed.
    ui.take_events();
    ui
}

fn tip(ui: &Interface) -> (u32, (i32, i32, i32, i32)) {
    let i = ui.tree.find(7003).unwrap();
    (ui.tree.elements[i].state, ui.tree.rect(i))
}

fn tip_shown(ui: &Interface) -> bool {
    tip(ui).0 & HIDDEN == 0
}

fn text_width(text: &str) -> i32 {
    Font::request("Arial", "8").width(text.as_bytes())
}

#[test]
fn a_tooltip_is_placed_at_the_pointer_30_pixels_lower_and_kept_in_the_window() {
    let window = (100, 80);
    assert_eq!(place((10, 10), (30, 14), window), (10, 40));
    // Past the right edge: against it. Past the bottom: above the pointer.
    assert_eq!(place((90, 10), (30, 14), window), (70, 40));
    assert_eq!(place((10, 60), (30, 14), window), (10, 46));
    assert_eq!(place((90, 70), (30, 14), window), (70, 56));
    // Exactly fitting stays.
    assert_eq!(place((70, 36), (30, 14), window), (70, 66));
    assert_eq!(place((71, 37), (30, 14), window), (70, 23));
    // Bigger than the window: no clamp, as the rule has none.
    assert_eq!(place((10, 10), (300, 200), window), (-200, -190));
    assert_eq!(place((0, 0), (0, 0), (0, 0)), (0, 0));
    // Nothing overflows.
    place(
        (i32::MAX, i32::MIN),
        (i32::MAX, i32::MAX),
        (i32::MIN, i32::MAX),
    );
    place(
        (i32::MIN, i32::MAX),
        (i32::MIN, i32::MIN),
        (i32::MAX, i32::MIN),
    );
}

#[test]
fn the_tooltip_of_the_element_under_the_pointer_comes_after_the_delay_and_expires() {
    let mut ui = interface("show");
    ui.mouse(Mouse::Move(5, 5));
    ui.take_events();
    assert!(!tip_shown(&ui));
    // The delay (1 ms by default) runs on the update: the first one has none elapsed.
    ui.update(0);
    assert!(!tip_shown(&ui));
    ui.update(66);
    let w = text_width("short five") + 2;
    let h = Font::request("Arial", "8").line_height() + 2;
    assert!(tip_shown(&ui));
    assert_eq!(tip(&ui).1, (5, 35, w, h));
    assert_eq!(ui.take_events(), [format!("tooltip 5 35 {w} {h}")]);
    assert_eq!(ui.text(7003), Some(b"short five".to_vec()));
    // Timed: the update that showed it started the count, which ends 99 ms later.
    ui.update(99);
    ui.update(132);
    assert!(tip_shown(&ui), "66 ms");
    ui.update(165);
    assert!(!tip_shown(&ui));
    assert_eq!(ui.take_events(), ["hide 7003"]);
    // It does not come back while the pointer stays.
    ui.update(1000);
    assert!(!tip_shown(&ui));
}

#[test]
fn a_help_string_asked_for_replaces_the_tooltip_shown_or_due() {
    let mut ui = interface("help");
    // Over the map view (no help of its own): the asked help comes after the delay.
    ui.mouse(Mouse::Move(80, 70));
    ui.help_tip(7);
    ui.update(0);
    ui.update(66);
    assert!(tip_shown(&ui));
    assert_eq!(ui.text(7003), Some(b"row seven".to_vec()));
    // Another one hides the shown one at once; none, or one without a text, leaves nothing due.
    ui.help_tip(5);
    assert!(!tip_shown(&ui));
    ui.help_tip(0);
    ui.update(132);
    ui.help_tip(99);
    ui.help_tip(-3);
    ui.update(200);
    assert!(!tip_shown(&ui));
    // The short text, even when the settings ask for the long help.
    ui.set_settings(Settings {
        long_help: true,
        ..Settings::default()
    });
    ui.help_tip(5);
    ui.update(300);
    ui.update(400);
    assert_eq!(ui.text(7003), Some(b"short five".to_vec()));
}

#[test]
fn another_element_hides_the_tooltip_and_restarts_it_and_none_clears_it() {
    let mut ui = interface("change");
    ui.mouse(Mouse::Move(5, 5));
    ui.update(0);
    ui.update(66);
    assert!(tip_shown(&ui));
    // Moving within the button changes nothing.
    ui.mouse(Mouse::Move(6, 6));
    assert!(tip_shown(&ui));
    // Onto another one: hidden at once, the new text after the delay, at the pointer.
    ui.take_events();
    ui.mouse(Mouse::Move(15, 3));
    assert!(!tip_shown(&ui));
    assert!(ui.take_events().contains(&"hide 7003".to_string()));
    ui.update(132);
    assert!(tip_shown(&ui));
    assert_eq!(ui.text(7003), Some(b"short six".to_vec()));
    assert_eq!(tip(&ui).1.0, 15);
    // Off the buttons (the map view has no help): cleared, and nothing follows.
    ui.mouse(Mouse::Move(80, 70));
    assert!(!tip_shown(&ui));
    ui.update(200);
    ui.update(300);
    assert!(!tip_shown(&ui));
    // No help string, or number 0: no tooltip.
    for x in [27, 39] {
        ui.mouse(Mouse::Move(x, 5));
        ui.update(400 + x as u64 * 10);
        ui.update(800 + x as u64 * 10);
        assert!(!tip_shown(&ui), "x {x}");
    }
}

#[test]
fn a_tooltip_that_is_over_by_the_time_the_pointer_moves_on_never_shows() {
    let mut ui = interface("quick");
    ui.settings.tooltip_delay = 100;
    ui.update(0);
    ui.mouse(Mouse::Move(5, 5));
    ui.update(66);
    assert!(!tip_shown(&ui), "66 of 100 ms");
    ui.mouse(Mouse::Move(15, 5));
    ui.update(132);
    assert!(!tip_shown(&ui), "the delay starts again");
    ui.update(231);
    assert!(tip_shown(&ui));
}

#[test]
fn the_long_help_is_the_number_plus_30000_and_the_short_text_if_that_string_is_empty() {
    let mut ui = interface("long");
    ui.set_settings(Settings {
        long_help: true,
        ..Settings::default()
    });
    ui.mouse(Mouse::Move(5, 5));
    ui.update(0);
    ui.update(66);
    assert_eq!(ui.text(7003), Some(b"long help five".to_vec()));
    // The width limit is the long one: 400, so this fits in a line.
    ui.mouse(Mouse::Move(15, 5));
    ui.update(132);
    assert!(tip_shown(&ui), "no long text: the short one");
    assert_eq!(ui.text(7003), Some(b"short six".to_vec()));
    // Neither text: no tooltip.
    let i = ui.tree.find(21).unwrap();
    ui.tree.elements[i].help_id = vec![99];
    ui.mouse(Mouse::Move(50, 50));
    ui.mouse(Mouse::Move(15, 5));
    ui.update(500);
    assert!(!tip_shown(&ui), "neither text");
}

#[test]
fn a_long_text_wraps_at_the_widest_tooltip_of_the_settings() {
    let mut ui = interface("wrap");
    ui.set_settings(Settings {
        max_short_width: 40,
        ..Settings::default()
    });
    ui.mouse(Mouse::Move(5, 5));
    ui.update(0);
    ui.update(66);
    let (_, (_, _, w, h)) = tip(&ui);
    // More than 40 wide on one line: the width is the smaller of 40 and three fifths of the
    // text, and the lines are as many as it takes.
    assert!(w <= 40 + 2, "{w}");
    assert!(h > Font::request("Arial", "8").line_height() + 2, "{h}");
}

#[test]
fn a_list_passes_the_help_of_its_hovered_row() {
    let mut ui = interface("rows");
    let row = |help| Row {
        text: b"row".to_vec(),
        help,
        ..Row::default()
    };
    ui.set_rows(30, vec![row(7), row(0), row(7)]);
    ui.mouse(Mouse::Move(5, 22));
    ui.update(0);
    ui.update(66);
    assert_eq!(ui.text(7003), Some(b"row seven".to_vec()));
    // Another row is another target: hidden, and no help for this one.
    ui.mouse(Mouse::Move(5, 34));
    assert!(!tip_shown(&ui));
    ui.update(132);
    ui.update(500);
    assert!(!tip_shown(&ui));
    // The third row has it again, a text longer than the row.
    ui.mouse(Mouse::Move(5, 46));
    ui.update(600);
    assert!(tip_shown(&ui));
}

#[test]
fn the_tooltip_is_never_hit_and_a_tree_without_one_has_no_tooltips() {
    let mut ui = interface("nothit");
    ui.mouse(Mouse::Move(5, 5));
    ui.update(0);
    ui.update(66);
    // The tooltip lies over the pointer's next place: it is not the target.
    let (_, (x, y, w, h)) = tip(&ui);
    ui.take_events();
    ui.mouse(Mouse::Move(x + w / 2, y + h / 2));
    assert_eq!(ui.take_events(), ["hover 30", "hide 7003"]);
    let i = ui.tree.find(7003).unwrap();
    ui.tree.elements[i].kind = Kind::Image;
    ui.mouse(Mouse::Move(5, 5));
    ui.update(500);
    ui.update(600);
}

#[test]
fn settings_come_from_the_ui_section_ignoring_case_and_keep_the_defaults_for_the_rest() {
    let read = |ini: &str| Settings::from_config(&Config::parse(ini.as_bytes()).unwrap());
    let d = Settings::default();
    assert_eq!(read(""), d);
    assert_eq!(
        read(
            "[ui]\ntooltipdelay=250\nHELPTYPE=1\nMaxShortTooltipWidth=120\nmaxlongtooltipwidth=300\n"
        ),
        Settings {
            tooltip_delay: 250,
            long_help: true,
            max_short_width: 120,
            max_long_width: 300,
        }
    );
    // 0 delay is no delay; below 0 is 0; a number of the wrong kind is not a setting.
    assert_eq!(read("[UI]\nTooltipDelay=0\n").tooltip_delay, 0);
    assert_eq!(read("[UI]\nTooltipDelay=-5\n").tooltip_delay, 0);
    assert_eq!(
        read("[UI]\nTooltipDelay=abc\n").tooltip_delay,
        d.tooltip_delay
    );
    // Too many digits wrap around, as the original's reader does.
    assert_eq!(
        read("[UI]\nTooltipDelay=99999999999\n").tooltip_delay,
        1_215_752_191
    );
    // The widths keep to 1 to 4096; helpType 0 is the short help.
    assert_eq!(read("[UI]\nmaxShortTooltipWidth=0\n").max_short_width, 1);
    assert_eq!(read("[UI]\nmaxShortTooltipWidth=-9\n").max_short_width, 1);
    assert_eq!(
        read("[UI]\nmaxLongTooltipWidth=1000000\n").max_long_width,
        4096
    );
    assert!(!read("[UI]\nhelpType=0\n").long_help);
    // Another section does not count.
    assert_eq!(read("[user]\ntooltipDelay=9\n"), d);
}

#[test]
fn a_question_leaves_the_tooltip_until_the_next_mouse_message() {
    let mut ui = interface("question");
    ui.mouse(Mouse::Move(5, 5));
    ui.update(0);
    ui.update(66);
    assert!(tip_shown(&ui));
    ui.take_events();
    // Opening a question unhighlights the hover element; the tooltip stays.
    assert!(ui.ask(100, b"", crate::ui::interface::Buttons::OK));
    assert!(tip_shown(&ui));
    assert_eq!(ui.take_events(), ["hover none", "ask 100"]);
    // The next mouse message outside the question hides it; onto the question's button, it
    // restarts for that one.
    ui.mouse(Mouse::Move(6, 6));
    assert!(!tip_shown(&ui));
    // The question is put in the middle of the window: its button is at (45, 30).
    ui.mouse(Mouse::Move(46, 31));
    ui.update(132);
    assert!(tip_shown(&ui));
    assert_eq!(ui.text(7003), Some(b"short five".to_vec()));
}

#[test]
fn a_list_row_without_help_or_below_the_last_row_clears_the_tooltip() {
    let mut ui = interface("norow");
    ui.set_rows(
        30,
        vec![
            Row {
                text: b"a".to_vec(),
                help: 7,
                ..Row::default()
            },
            Row::default(),
        ],
    );
    ui.mouse(Mouse::Move(5, 22));
    ui.update(0);
    ui.update(66);
    assert!(tip_shown(&ui));
    ui.mouse(Mouse::Move(5, 34));
    assert!(!tip_shown(&ui), "a row without help");
    ui.update(132);
    assert!(!tip_shown(&ui));
    ui.mouse(Mouse::Move(5, 22));
    ui.update(200);
    assert!(tip_shown(&ui));
    ui.mouse(Mouse::Move(5, 50));
    assert!(!tip_shown(&ui), "below the last row");
    // The list's own help string, if it had one, is not what shows for a row.
    ui.update(300);
    assert!(!tip_shown(&ui));
}

use super::*;
use crate::app::Mouse;
use crate::fixtures::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{Picture, Tree};
use crate::vfs::Vfs;

struct NoTexts;

impl Strings for NoTexts {
    fn string(&self, _: i32) -> String {
        String::new()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// A 2 x 2 image.
fn tga() -> Vec<u8> {
    let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 24, 0x20];
    out.extend([200; 12]);
    out
}

/// The map view, the message line 7001 (timer 100 ms, `extra` keys), the button 1006 and the
/// history layout 29 with its list 7201, its bar and its close button.
fn history(name: &str, extra: &str) -> (Interface, Picture) {
    let root = format!(
        "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
         [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
         [b]\ntype=ZTMessageQueue\nid=7001\nx=0\ny=0\ndx=60\ndy=20\ntransparent=0\nlistbox=7201\n\
         generalcolor=1\ngeneralcolor=2\ngeneralcolor=3\ngoodcolor=0\ngoodcolor=255\ngoodcolor=0\n\
         urgentcolor=255\nurgentcolor=0\nurgentcolor=0\n{extra}timed=1\ntimerDuration=100\n\
         state=1025\n\
         [c]\ntype=UIButton\nid=1006\nx=0\ny=30\ndx=10\ndy=10\nstate=2048\ncolorasicon=1\n\
         [d]\ntype=UILayout\nlayout=ui/message.lyt\n"
    );
    let message = "[LayoutInfo]\nid=29\nlayer=2\nx=0\ny=24\ndx=64\ndy=24\nstate=1\n\
        [a]\ntype=UIListBox\nid=7201\nx=0\ny=0\ndx=50\ndy=24\nminiconwidth=10\nminiconheight=6\n\
        scrollbar=7202\n\
        [b]\ntype=UIScrollBar\nid=7202\nx=50\ny=0\ndx=6\ndy=24\n\
        [c]\ntype=UIButton\nid=7203\nx=56\ny=0\ndx=8\ndy=8\naction=2\ntarget=-1\ncolorasicon=1\n";
    let install = Install::new(
        &format!("history-{name}"),
        &[
            ("ui/root.lyt", root.into_bytes()),
            ("ui/message.lyt", message.as_bytes().to_vec()),
            ("ui/icon.tga", tga()),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    let icon = tree.picture(&vfs, "ui/icon.tga").unwrap();
    tree.resolve(64, 48, |_| 12);
    let mut ui = Interface::new(tree, Box::new(NoTexts));
    ui.set_hidden(1006, false);
    (ui, icon)
}

/// The rows of the history list: their texts, whether each has an icon, and their colours.
fn rows(ui: &Interface) -> Vec<(Vec<u8>, bool, Option<[u8; 3]>)> {
    let Kind::ListBox(l) = &ui.tree.elements[ui.tree.find(7201).unwrap()].kind else {
        panic!("no list");
    };
    let colors = |n: usize| l.rows.colors.get(n).copied().flatten();
    (l.rows.rows.iter().enumerate())
        .map(|(n, r)| (r.text.clone(), r.icon.is_some(), colors(n)))
        .collect()
}

/// Two updates 33 ms apart from `at`: the first starts the count of the line.
fn count_from(ui: &mut Interface, at: u64) {
    ui.update(at);
    ui.update(at + 33);
}

/// The text on the line and whether the line is shown.
fn line(ui: &Interface) -> (Vec<u8>, bool) {
    (ui.text(7001).unwrap(), ui.shown(7001))
}

#[test]
fn every_post_is_a_row_newest_first_in_the_colour_of_its_kind_with_an_icon_for_an_entity() {
    let (mut ui, icon) = history("rows", "");
    assert!(ui.message(b"A", 0));
    assert!(ui.post(Message::new(b"B", 2).about(5, Some(icon))));
    assert!(ui.post(Message::new(b"C", 1).about(6, None)));
    // A repeat within the interval is dropped: no row.
    assert!(!ui.message(b"A", 0));
    assert_eq!(
        rows(&ui),
        [
            (b"C".to_vec(), false, Some([0, 255, 0])),
            (b"B".to_vec(), true, Some([255, 0, 0])),
            (b"A".to_vec(), false, Some([1, 2, 3])),
        ]
    );
    // The line shows the icon of the message on it.
    assert_eq!(ui.history(), [b"C".to_vec(), b"B".to_vec(), b"A".to_vec()]);
    assert_eq!(ui.tree.elements[2].look.icon, None);
}

#[test]
fn the_rows_are_the_queue_entries_and_the_oldest_goes_with_it() {
    for (size, kept) in [(2, 2), (1, 1), (0, 1), (-3, 5), (i32::MAX, 5)] {
        let (mut ui, _) = history(&format!("size{size}"), &format!("queueSize={size}\n"));
        for n in 0..5 {
            ui.message(format!("m{n}").as_bytes(), 3);
        }
        assert_eq!(rows(&ui).len(), kept, "queueSize {size}");
        assert_eq!(rows(&ui)[0].0, b"m4", "the newest is row 0");
    }
    // A missing key is 25.
    let (mut ui, _) = history("size-missing", "");
    for n in 0..30 {
        ui.message(format!("m{n}").as_bytes(), 3);
    }
    assert_eq!(rows(&ui).len(), 25);
}

#[test]
fn a_list_only_message_never_goes_on_the_line() {
    let (mut ui, _) = history("hidden-line", "");
    assert!(ui.post(Message::new(b"dialog", 3).listed_only()));
    assert_eq!((line(&ui), ui.waiting), ((vec![], false), 0));
    assert_eq!(ui.tree.elements[2].countdown, None);
    assert_eq!(ui.history(), [b"dialog".to_vec()]);
    // On a shown line it queues, ends the one shown, is skipped and the line hides.
    ui.message(b"A", 3);
    ui.update(1000);
    ui.post(Message::new(b"dialog 2", 3).listed_only());
    assert_eq!((ui.waiting, ui.tree.elements[2].countdown), (1, Some(0)));
    assert_eq!(line(&ui), (b"A".to_vec(), true), "until the update");
    ui.update(1033);
    assert_eq!((line(&ui).1, ui.waiting), (false, 0));
    assert_eq!(ui.history().len(), 3, "the list keeps it");
    // A message with the flag that waits behind it is the next one shown.
    ui.message(b"B", 3);
    ui.update(1100);
    ui.post(Message::new(b"dialog 3", 3).listed_only());
    ui.message(b"C", 3);
    ui.update(1133);
    assert_eq!(line(&ui), (b"C".to_vec(), true));
    // Paused, a waiting list-only message is still skipped and the line hides.
    ui.hold_message(true);
    ui.post(Message::new(b"dialog 4", 3).listed_only());
    ui.update(1166);
    assert!(!line(&ui).1);
}

#[test]
fn the_pause_string_leaves_the_list_with_the_line() {
    let (mut ui, _) = history("pause", "");
    ui.hold_message(true);
    ui.post(Message::new(b"P", 3).replacing());
    assert_eq!(
        (line(&ui), ui.history()),
        ((b"P".to_vec(), true), vec![b"P".to_vec()])
    );
    ui.update(1000);
    ui.update(1200);
    assert_eq!(ui.history().len(), 1, "stays for the whole pause");
    ui.hold_message(false);
    ui.end_message();
    ui.update(1233);
    assert!(!line(&ui).1 && ui.history().is_empty() && rows(&ui).is_empty());
}

#[test]
fn a_post_removes_the_pause_string_that_waits() {
    // Shown line, the string waiting behind it: the post removes it and the message waits.
    let (mut ui, _) = history("removal", "");
    ui.message(b"X", 3);
    ui.update(1000);
    ui.post(Message::new(b"P", 3).replacing());
    ui.message(b"M", 3);
    assert_eq!(ui.history(), [b"M".to_vec(), b"X".to_vec()]);
    ui.update(1033);
    assert_eq!(line(&ui).0, b"M");
    // A message that does not replace stays in the list when the line hides.
    let (mut ui, _) = history("no-removal", "");
    ui.message(b"A", 3);
    ui.update(1000);
    ui.update(1100);
    assert!(!line(&ui).1);
    assert_eq!(ui.history(), [b"A".to_vec()]);
}

#[test]
fn the_waiting_cut_does_not_count_the_removed_pause_string() {
    // [400, X] + M gives [M, X] with 1 waiting; [400, X, Y] + M gives 2.
    for (older, waiting) in [(1, 1), (2, 2)] {
        let (mut ui, _) = history(&format!("cut{older}"), "");
        for n in 0..older {
            ui.message(format!("X{n}").as_bytes(), 3);
        }
        ui.update(1000);
        ui.post(Message::new(b"P", 3).replacing());
        ui.message(b"M", 3);
        assert_eq!(ui.history().len(), older + 1);
        assert_eq!(ui.waiting, waiting, "{older} older");
    }
    // The string alone: the post is listed, waits for nothing and never reaches the line; the
    // string stays for the whole pause and the line hides with the message still listed.
    let (mut ui, _) = history("cut-alone", "");
    ui.hold_message(true);
    ui.post(Message::new(b"P", 3).replacing());
    ui.update(1000);
    ui.message(b"M", 3);
    assert_eq!((ui.history(), ui.waiting), (vec![b"M".to_vec()], 0));
    ui.update(1033);
    ui.update(1300);
    assert_eq!(line(&ui), (b"P".to_vec(), true));
    ui.hold_message(false);
    ui.end_message();
    ui.update(1400);
    assert!(!line(&ui).1);
    assert_eq!(ui.history(), [b"M".to_vec()]);
}

#[test]
fn a_full_list_drops_its_oldest_before_the_pause_string_goes() {
    let (mut ui, _) = history("full", "queueSize=3\n");
    ui.message(b"A", 3);
    ui.message(b"B", 3);
    ui.update(1000);
    ui.post(Message::new(b"P", 3).replacing());
    assert_eq!(ui.history(), [b"P".to_vec(), b"B".to_vec(), b"A".to_vec()]);
    ui.message(b"M", 3);
    assert_eq!(ui.history(), [b"M".to_vec(), b"B".to_vec()]);
    assert_eq!(ui.waiting, 1);
}

#[test]
fn the_line_icon_is_tested_again_when_a_waiting_message_is_taken() {
    let (mut ui, icon) = history("icon-again", "");
    let line_icon = |ui: &Interface| ui.tree.elements[2].look.icon;
    ui.message(b"A", 3);
    ui.post(Message::new(b"B", 3).about(5, Some(icon)));
    ui.post(Message::new(b"C", 3).about(6, Some(icon)));
    ui.entities_found(|_| true);
    ui.update(1000);
    ui.update(1100);
    assert_eq!((line(&ui).0, line_icon(&ui)), (b"B".to_vec(), Some(icon)));
    // 6 leaves the world before its message is taken: no icon on the line, the row keeps it.
    ui.entities_found(|id| id != 6);
    ui.update(1200);
    ui.update(1300);
    assert_eq!((line(&ui).0, line_icon(&ui)), (b"C".to_vec(), None));
    assert!(rows(&ui)[0].1, "the row keeps its icon");
}

#[test]
fn a_click_goes_to_the_entity_of_the_message_and_ends_the_one_on_the_line() {
    let (mut ui, _) = history("click", "");
    ui.post(Message::new(b"A", 3).about(7, None));
    ui.message(b"B", 3);
    count_from(&mut ui, 1000);
    // No such message: nothing happens.
    for row in [-1, 2, 3, i32::MAX, i32::MIN] {
        assert_eq!(ui.message_click(row), None, "row {row}");
    }
    assert!(ui.tree.elements[2].countdown.is_some_and(|n| n > 0));
    assert_eq!(ui.message_click(1), Some(Some(7)));
    assert_eq!(ui.tree.elements[2].countdown, Some(0));
    ui.update(1100);
    assert!(!line(&ui).1, "gave way");
    // A message with no entity: the line's message still ends.
    ui.message(b"C", 3);
    count_from(&mut ui, 1200);
    assert_eq!(ui.message_click(0), Some(None));
    assert_eq!(ui.tree.elements[2].countdown, Some(0));
    // No message line: nothing.
    assert_eq!(history("click-none", "").0.message_click(0), None);
}

#[test]
fn a_release_on_the_line_clicks_the_message_on_it() {
    let (mut ui, _) = history("line-click", "");
    ui.message(b"A", 3);
    ui.update(1000);
    ui.message(b"B", 3);
    ui.message(b"C", 3);
    ui.take_actions();
    ui.mouse(Mouse::Move(5, 5));
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Up);
    // A and B were posted, the line shows A: two wait.
    assert_eq!(ui.take_actions(), [Action::Message(2)]);
    // Pressed on the line and released off it: no click.
    ui.mouse(Mouse::Move(5, 5));
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Move(60, 45));
    ui.mouse(Mouse::Up);
    assert_eq!(ui.take_actions(), []);
}

#[test]
fn message_display_0_drops_every_post_and_the_unpause_too() {
    let (mut ui, _) = history("off", "");
    ui.set_settings(crate::ui::interface::Settings {
        message_display: false,
        ..crate::ui::interface::Settings::default()
    });
    assert!(!ui.message(b"A", 3) && !ui.post(Message::new(b"B", 3).listed_only()));
    assert_eq!(
        (line(&ui), ui.history().len(), rows(&ui).len()),
        ((vec![], false), 0, 0)
    );
    ui.end_message();
    assert_eq!(ui.tree.elements[2].countdown, None);
    // On: the same posts go in.
    ui.set_settings(crate::ui::interface::Settings::default());
    assert!(ui.message(b"A", 3));
}

#[test]
fn the_button_toggles_the_history_and_any_hide_releases_it() {
    let (mut ui, _) = history("button", "");
    let down = |ui: &Interface| ui.is_down(1006);
    assert!(!ui.shown(29));
    // Five messages on a shown line: four wait, so the list opens on row 4.
    ui.message(b"0", 3);
    count_from(&mut ui, 1000);
    for n in 1..5 {
        ui.message(format!("{n}").as_bytes(), 3);
    }
    assert_eq!(ui.waiting, 4);
    assert!(ui.press_button(1006));
    assert!(ui.shown(29) && down(&ui));
    let Kind::ListBox(l) = &ui.tree.elements[ui.tree.find(7201).unwrap()].kind else {
        panic!("no list");
    };
    assert!(l.rows.top > 0, "scrolled to the message on the line");
    // The button again closes it; the close button closes it and the button comes up.
    assert!(ui.press_button(1006));
    assert!(!ui.shown(29) && !down(&ui));
    ui.press_button(1006);
    assert!(ui.shown(29));
    ui.take_selected();
    assert!(ui.press_button(7203));
    assert!(!ui.shown(29) && !down(&ui));
    assert_eq!(ui.take_selected(), [7203], "the button 1006 ran no action");
}

#[test]
fn clearing_empties_the_queue_the_list_and_the_line() {
    let (mut ui, icon) = history("clear", "");
    ui.post(Message::new(b"A", 3).about(1, Some(icon)));
    count_from(&mut ui, 1000);
    ui.message(b"B", 3);
    ui.press_button(1006);
    assert!(ui.waiting > 0 && ui.shown(29) && ui.is_down(1006));
    ui.clear_messages();
    assert_eq!((ui.waiting, ui.history().len(), rows(&ui).len()), (0, 0, 0));
    assert_eq!(
        (line(&ui), ui.tree.elements[2].look.icon),
        ((vec![], false), None)
    );
    assert!(!ui.shown(29) && !ui.is_down(1006));
    // Again, and on a tree with no line.
    ui.clear_messages();
    interface_without_line().clear_messages();
}

fn interface_without_line() -> Interface {
    let install = Install::new(
        "history-none",
        &[("ui/root.lyt", b"[LayoutInfo]\nid=1\n".to_vec())],
    );
    let tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    Interface::new(tree, Box::new(NoTexts))
}

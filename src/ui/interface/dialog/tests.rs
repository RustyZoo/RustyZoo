use super::*;
use crate::app::Mouse;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{FOCUSED, HIGHLIGHTED, SELECTED, Tree};
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

const BUTTON: &str = "type=UIButton\ncolorasicon=1\ndx=12\ndy=8\ny=30\naction=5\n";

/// A window of 100 x 80. The question 100 (60 x 40, hidden, draggable by its background) has
/// the message text 106 and the buttons 110 to 113 (Yes, No, OK, Cancel) in the places of three
/// buttons, with other places for two; the question 200 is the same with a text field 208. A
/// button 20 under the map view's corner is for clicks outside a question.
fn interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [b]\ntype=UILayout\nlayout=ui/q100.lyt\n[c]\ntype=UILayout\nlayout=ui/q200.lyt\n\
        [d]\ntype=UIButton\nid=20\nx=0\ny=0\ndx=10\ndy=10\ncolorasicon=1\naction=1\ntarget=100\n";
    let question = |id: i32| {
        format!(
            "[LayoutInfo]\nid={id}\nlayer=7\nx=0\ny=0\ndx=60\ndy=40\n\
         [a]\ntype=UIImage\nid={}\ndx=60\ndy=40\nalias=-1\ntransparent=0\n\
         [b]\ntype=UIText\nid={}\nx=2\ny=2\ndx=40\ndy=24\nscrollbar={}\n\
         [h]\ntype=UIScrollBar\nid={}\ndx=8\n\
         [c]\nid={}\nx=2\naltx=10\ntarget=6\n{BUTTON}\
         [d]\nid={}\nx=22\naltx=38\ntarget=7\n{BUTTON}\
         [e]\nid={}\nx=22\naltx=0\ntarget=1\n{BUTTON}\
         [f]\nid={}\nx=42\naltx=0\ntarget=2\n{BUTTON}",
            id + 1,
            id + 6,
            id + 7,
            id + 7,
            id + 10,
            id + 11,
            id + 12,
            id + 13
        )
    };
    let field = "[g]\ntype=UIEditableText\nid=208\nx=2\ny=26\ndx=30\ndy=4\ntransparent=0\n";
    let install = Install::new(
        &format!("dialog-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/q100.lyt", question(100).into_bytes()),
            ("ui/q200.lyt", (question(200) + field).into_bytes()),
        ],
    );
    let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    assert_eq!(tree.warnings, Vec::<String>::new());
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(NoTexts));
    ui.show(1);
    // The questions are closed.
    for id in [100, 200] {
        let layout = ui.tree.find(id).unwrap();
        ui.tree.hide(layout);
    }
    ui.take_events();
    ui
}

fn state(ui: &Interface, id: i32) -> u32 {
    ui.tree.elements[ui.tree.find(id).unwrap()].state
}

fn shown(ui: &Interface, id: i32) -> bool {
    state(ui, id) & HIDDEN == 0
}

fn rect(ui: &Interface, id: i32) -> (i32, i32, i32, i32) {
    ui.tree.rect(ui.tree.find(id).unwrap())
}

fn click(ui: &mut Interface, x: i32, y: i32) {
    ui.mouse(Mouse::Move(x, y));
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Up);
}

#[test]
fn a_question_shows_its_message_and_the_buttons_it_needs_in_the_middle_of_the_window() {
    let mut ui = interface("ask");
    assert!(!ui.modal_open());
    assert!(!ui.ask(999, b"?", Buttons::OK), "no such layout");
    assert!(!ui.ask(20, b"?", Buttons::OK), "not a layout");
    assert!(ui.ask(100, b"Sure?", Buttons::YES_NO_CANCEL));
    assert!(ui.modal_open());
    assert_eq!(ui.take_events().last().unwrap(), "ask 100");
    assert_eq!(ui.text(106), Some(b"Sure?".to_vec()));
    // (100 - 60) / 2 and (80 - 40) / 2.
    assert_eq!(rect(&ui, 100), (20, 20, 60, 40));
    let buttons: Vec<bool> = [110, 111, 112, 113]
        .iter()
        .map(|&b| shown(&ui, b))
        .collect();
    assert_eq!(buttons, [true, true, false, true]);
    // Three buttons: their own places (x 22 + 2, 22 + 22, 20 + 42).
    assert_eq!(rect(&ui, 110).0, 22);
    assert_eq!(rect(&ui, 111).0, 42);
    assert_eq!(rect(&ui, 113).0, 62);
    // Two buttons take their other places.
    assert!(ui.ask(100, b"", Buttons::YES_NO));
    assert_eq!((rect(&ui, 110).0, rect(&ui, 111).0), (30, 58));
    assert!(ui.ask(100, b"", Buttons::OK_CANCEL));
    assert_eq!((rect(&ui, 112).0, rect(&ui, 113).0), (20, 20));
    assert!(shown(&ui, 112) && shown(&ui, 113) && !shown(&ui, 110) && !shown(&ui, 111));
    // One button and then three again: the places are the plain ones.
    assert!(ui.ask(100, b"", Buttons::OK));
    assert_eq!(rect(&ui, 112).0, 42);
    assert!(ui.ask(100, b"", Buttons::YES_NO_CANCEL));
    assert_eq!(rect(&ui, 110).0, 22);
}

#[test]
fn a_click_on_a_button_answers_and_closes_the_question() {
    let mut ui = interface("click");
    ui.ask(100, b"Sure?", Buttons::YES_NO_CANCEL);
    ui.take_events();
    // Yes is at (22, 50), 12 by 8.
    click(&mut ui, 25, 52);
    let events = ui.take_events();
    assert!(events.contains(&"select 110".to_string()), "{events:?}");
    assert!(events.contains(&"answer 6".to_string()));
    assert_eq!(ui.take_actions().last(), Some(&Action::Answer(6)));
    assert!(!ui.modal_open());
    assert!(!shown(&ui, 100) && !shown(&ui, 110));
    // No and Cancel answer 7 and 2.
    for (button_x, answer) in [(45, 7), (65, 2)] {
        ui.ask(100, b"", Buttons::YES_NO_CANCEL);
        ui.take_actions();
        click(&mut ui, button_x, 52);
        assert_eq!(ui.take_actions().last(), Some(&Action::Answer(answer)));
    }
    // OK answers 1; a target above 9 counts as 1.
    ui.ask(100, b"", Buttons::OK);
    ui.take_actions();
    let ok = ui.tree.find(112).unwrap();
    ui.tree.elements[ok].target = 77;
    click(&mut ui, 45, 52);
    assert_eq!(ui.take_actions().last(), Some(&Action::Answer(1)));
}

#[test]
fn only_the_question_receives_the_mouse() {
    let mut ui = interface("modal-mouse");
    // Without a question the button 20 opens it by its built-in action.
    click(&mut ui, 5, 5);
    assert!(shown(&ui, 100));
    let layout = ui.tree.find(100).unwrap();
    ui.tree.hide(layout);
    ui.mouse(Mouse::Move(6, 6));
    assert_eq!(state(&ui, 20) & HIGHLIGHTED, HIGHLIGHTED);
    ui.ask(100, b"", Buttons::YES_NO);
    // Opening it took the highlight from the hover element.
    assert_eq!(state(&ui, 20) & HIGHLIGHTED, 0);
    assert_eq!(ui.take_events().last().unwrap(), "ask 100");
    // Outside the question: nothing is hovered or pressed, and the map view is not hit either.
    assert!(!ui.mouse(Mouse::Move(5, 5)));
    assert!(!ui.mouse(Mouse::Down));
    ui.mouse(Mouse::Up);
    assert!(ui.take_events().is_empty());
    assert_eq!(state(&ui, 20) & (HIGHLIGHTED | SELECTED), 0);
    // Inside, the background hit is the layout: dragging works as for any dialog.
    ui.mouse(Mouse::Move(70, 25));
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Move(75, 30));
    ui.mouse(Mouse::Up);
    assert_eq!(rect(&ui, 100).0, 25);
    assert!(ui.modal_open());
}

#[test]
fn enter_escape_y_and_n_answer_by_the_buttons_that_are_shown() {
    let answer = |buttons: Buttons, key: Key| {
        let mut ui = interface("keys");
        ui.ask(100, b"", buttons);
        ui.take_actions();
        assert!(ui.key(key, false), "the key stops at the question");
        match ui.take_actions().as_slice() {
            [Action::Answer(a)] => Some(*a),
            [] => None,
            other => panic!("{other:?}"),
        }
        .inspect(|_| assert!(!ui.modal_open()))
    };
    use Key::{Enter, Escape, N, Y};
    let all = Buttons::YES_NO_CANCEL;
    assert_eq!(answer(all, Enter), Some(6));
    assert_eq!(answer(Buttons::OK, Enter), Some(1));
    assert_eq!(answer(Buttons::OK_CANCEL, Enter), Some(1));
    assert_eq!(answer(Buttons::YES_NO, Enter), Some(6));
    assert_eq!(answer(all, Escape), Some(2));
    assert_eq!(answer(Buttons::YES_NO, Escape), Some(7));
    assert_eq!(answer(Buttons::OK_CANCEL, Escape), Some(2));
    assert_eq!(answer(Buttons::OK, Escape), Some(2));
    assert_eq!(answer(all, Y), Some(6));
    assert_eq!(answer(all, N), Some(7));
    assert_eq!(answer(Buttons::OK_CANCEL, Y), None, "no Yes button");
    assert_eq!(answer(Buttons::OK, N), None);
    assert_eq!(answer(all, Key::Tab), None);
    assert_eq!(answer(all, Key::Left), None);
    // With no question the keys are not for it.
    let mut ui = interface("nokeys");
    assert!(!ui.key(Enter, false));
}

#[test]
fn a_field_of_the_question_reads_the_keys_first_and_enter_ends_the_edit_and_answers() {
    let mut ui = interface("field");
    ui.ask(200, b"Name?", Buttons::OK_CANCEL);
    assert!(ui.focus(208));
    assert_eq!(state(&ui, 208) & FOCUSED, FOCUSED);
    // Letters go to the field, whatever they are for the question.
    for byte in *b"ay" {
        ui.char(byte);
    }
    assert!(ui.key(Key::Y, false));
    assert!(ui.key(Key::N, false));
    assert!(ui.modal_open());
    assert_eq!(ui.text(208), Some(b"ay".to_vec()));
    // Enter ends the edit, keeps the text and answers OK.
    ui.take_events();
    assert!(ui.key(Key::Enter, false));
    assert_eq!(ui.text(208), Some(b"ay".to_vec()));
    assert_eq!(
        ui.take_events(),
        ["unfocus 208", "key 200 enter", "hide 200", "answer 1"]
    );
    assert_eq!(
        ui.take_actions(),
        [Action::Key(200, Key::Enter), Action::Answer(1)]
    );
    assert!(!ui.modal_open());
    // Escape puts the old text back and answers Cancel.
    ui.ask(200, b"", Buttons::OK_CANCEL);
    ui.set_text(208, b"old");
    ui.focus(208);
    ui.char(b'!');
    ui.take_actions();
    ui.key(Key::Escape, false);
    assert_eq!(ui.text(208), Some(b"old".to_vec()));
    assert_eq!(ui.take_actions().last(), Some(&Action::Answer(2)));
}

#[test]
fn a_question_asked_while_one_is_open_answers_it_with_cancel_first() {
    let mut ui = interface("twice");
    ui.ask(100, b"one", Buttons::YES_NO);
    ui.take_actions();
    assert!(ui.ask(200, b"two", Buttons::OK));
    assert_eq!(ui.take_actions(), [Action::Answer(2)]);
    assert!(!shown(&ui, 100));
    assert!(shown(&ui, 200));
    assert!(ui.modal_open());
    ui.answer(1);
    ui.answer(1);
    assert_eq!(ui.take_actions(), [Action::Answer(1)], "none open: nothing");
    assert!(!ui.modal_open());
}

#[test]
fn the_question_takes_the_focus_from_the_map_view_and_gives_the_keys_back() {
    let mut ui = interface("focus");
    assert!(ui.map_view_has_the_keys());
    ui.ask(100, b"", Buttons::OK);
    assert!(!ui.map_view_has_the_keys());
    ui.answer(1);
    assert!(ui.map_view_has_the_keys());
}

#[test]
fn the_question_shows_its_scroll_bar_only_while_the_message_does_not_fit() {
    let mut ui = interface("bar");
    // The text is 40 x 24 pixels at (22, 22): two lines. The bar is to the right of it.
    ui.ask(100, b"Short", Buttons::OK);
    ui.take_events();
    ui.mouse(Mouse::Move(66, 30));
    assert_eq!(
        ui.take_events(),
        ["hover 100"],
        "the background of the question is there"
    );
    ui.ask(100, b"one\ntwo\nthree\nfour", Buttons::OK);
    ui.take_events();
    ui.mouse(Mouse::Move(66, 28));
    assert_eq!(ui.take_events(), ["hover 107"]);
    // A new message that fits takes it away again.
    ui.ask(100, b"Short", Buttons::OK);
    ui.take_events();
    ui.mouse(Mouse::Move(66, 29));
    assert_eq!(ui.take_events(), ["hover 100"]);
}

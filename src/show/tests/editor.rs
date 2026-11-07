//! The show panel's trick editor (information panels 19.9): the two lists, the buttons, the
//! benefit, and the script the list is written to.

use super::cycle::{SEAL, info_mut, info_of_show, staged};
use super::panel::{on, text, window};
use super::*;
use crate::entity::Catalogue;
use crate::panel::show::Panel;
use crate::show::trick::tricks;
use crate::ui::interface::Interface;
use crate::ui::layout::Kind;
use crate::ui::widgets::Row;
use crate::vfs::Vfs;
use crate::world::World;
use crate::zoo::Script;

const HOOP: u16 = 11201;
const TAIL: u16 = 11202;
const BALL: u16 = 11203;
const FOOD: u16 = 11299;
const AVAILABLE: i32 = 11111;
const SHOWN: i32 = 11115;
const ADD: i32 = 11113;
const REMOVE: i32 = 11114;
const EARLIER: i32 = 11117;
const LATER: i32 = 11118;

/// A hoop (4, delta 3, complexity 5), a tail (3, 3, 2), a ball too deep for the tank (water at 3)
/// and the food trick.
const TRICKS: &str = "[tricks]\ntrick=hoop\ntrick=tail\ntrick=ball\ntrick=food\n\
    [tricks/hoop]\nid=11201\ntype=0\ncomplexity=5\nsatisfaction=4\nsatisfactiondelta=3\n\
    defaultavailable=1\nnormalHelpID=5\ngrayedHelpID=6\n\
    [tricks/tail]\nid=11202\ntype=0\ncomplexity=2\nsatisfaction=3\nsatisfactiondelta=3\n\
    defaultavailable=1\n\
    [tricks/ball]\nid=11203\ntype=0\ncomplexity=1\nsatisfaction=2\nsatisfactiondelta=1\n\
    minimumdepth=4\ndefaultavailable=1\nnormalHelpID=7\ngrayedHelpID=8\n\
    [tricks/food]\nid=11299\ntype=3\ncomplexity=0\nsatisfaction=0\nsatisfactiondelta=0\n\
    visible=0\ndefaultavailable=1\n";

/// The staged show tank with the panel open on it.
struct Rig {
    w: World,
    cat: Catalogue,
    ui: Interface,
    p: Panel,
    vfs: Vfs,
    x: u32,
}

/// A rig whose seals have the tricks above, whose script holds `script`, and whose record has
/// used its default script up.
fn rig(name: &str, script: &[u16]) -> Rig {
    let (mut w, mut cat) = staged(vec![]);
    let seal = cat.types.iter_mut().find(|t| t.name_id == SEAL as i32);
    let seal = seal.unwrap();
    seal.config = Some(Arc::new(Config::parse(TRICKS.as_bytes()).unwrap()));
    let list = tricks(seal);
    let items = script
        .iter()
        .map(|id| list.iter().find(|t| t.id == *id).cloned());
    w.zoo.shows.scripts[0].items = items.map(Option::unwrap).collect();
    w.zoo.shows.next_script = 2001;
    info_mut(&mut w).lookup(SEAL, 0).default_flag = false;
    let x = super::cycle::tank_mut(&mut w).id;
    let (ui, vfs) = window(name);
    let mut r = Rig {
        w,
        cat,
        ui,
        p: Panel::default(),
        vfs,
        x,
    };
    r.open();
    r
}

impl Rig {
    fn open(&mut self) {
        let x = Some(self.x);
        self.p
            .set_exhibit(&mut self.ui, &mut self.w, &self.cat, &self.vfs, x);
    }

    /// The row `row` of `list` clicked, with the select action it runs (-1 for a disabled row).
    fn pick(&mut self, list: i32, row: usize) {
        self.ui.select_row(list, Some(row));
        let off = rows(&self.ui, list)[row].disabled;
        let value = usize::try_from(if off { -1 } else { row as i32 }).ok();
        match list {
            AVAILABLE => self.p.avail_select(&mut self.ui, value),
            _ => self.p.show_select(&mut self.ui, value),
        }
    }

    /// The available trick named `name` picked and added.
    fn add(&mut self, row: usize) -> Option<String> {
        self.pick(AVAILABLE, row);
        self.p.add(&mut self.ui, &mut self.w, &self.cat, &self.vfs)
    }

    fn remove(&mut self) {
        self.p
            .remove(&mut self.ui, &mut self.w, &self.cat, &self.vfs);
    }

    fn shift(&mut self, earlier: bool) {
        let (ui, w, cat, vfs) = (&mut self.ui, &mut self.w, &self.cat, &self.vfs);
        self.p.shift(ui, w, cat, vfs, earlier);
    }

    fn shown(&self) -> Vec<String> {
        names(&self.ui, SHOWN)
    }

    /// The buttons that are on, among add, remove, earlier and later.
    fn buttons(&self) -> [bool; 4] {
        [ADD, REMOVE, EARLIER, LATER].map(|b| on(&self.ui, b))
    }

    /// The ids of the script of the seals, and the script's id.
    fn script(&self) -> (u16, Vec<u16>) {
        let id = info_of_show(&self.w).record(SEAL).unwrap().script;
        let script = self.w.zoo.shows.scripts.iter().find(|s| s.id == id);
        (
            id,
            script.map_or_else(Vec::new, |s| s.items.iter().map(|i| i.id).collect()),
        )
    }

    /// The window is closed: a changed list is written.
    fn close(&mut self) {
        self.p.hidden(&mut self.ui, &mut self.w, &self.cat);
    }
}

fn rows(ui: &Interface, id: i32) -> Vec<Row> {
    match &ui.tree.elements[ui.tree.find(id).unwrap()].kind {
        Kind::ListBox(l) => l.rows.rows.clone(),
        _ => panic!(),
    }
}

fn names(ui: &Interface, id: i32) -> Vec<String> {
    let name = |r: Row| String::from_utf8(r.text).unwrap();
    rows(ui, id).into_iter().map(name).collect()
}

#[test]
fn the_available_list_has_the_default_subtypes_tricks_but_food_and_grays_the_refused_one() {
    let r = rig("ed-avail", &[]);
    assert_eq!(names(&r.ui, AVAILABLE), ["Hoop", "Tail", "Ball"]);
    // The ball needs more water: grayed, with its grayed help; the others show their normal help.
    let state = |r: Row| (r.disabled, r.help);
    let found: Vec<_> = rows(&r.ui, AVAILABLE).into_iter().map(state).collect();
    assert_eq!(found, [(false, 5), (false, 0), (true, 8)]);
    // No row is selected and nothing can be pressed.
    assert_eq!(r.ui.selected_row(AVAILABLE), None);
    assert_eq!(r.buttons(), [false; 4]);
}

#[test]
fn the_show_list_is_the_script_in_order_without_the_food_trick() {
    let r = rig("ed-script", &[TAIL, HOOP, FOOD]);
    assert_eq!(r.shown(), ["Tail", "Hoop"]);
    // Nothing was dropped, so the script is as it was.
    assert_eq!(r.script().0, 2000);
}

#[test]
fn a_trick_that_is_not_valid_is_dropped_and_the_shorter_list_is_written_at_once() {
    let mut r = rig("ed-drop", &[TAIL, BALL, HOOP]);
    assert_eq!(r.shown(), ["Tail", "Hoop"]);
    // The new script has the food trick after the run that adds up to the threshold (2 + 5).
    let (id, items) = r.script();
    assert_ne!(id, 2000);
    assert_eq!(items, [TAIL, HOOP, FOOD]);
    assert!(r.w.zoo.shows.scripts.iter().all(|s| s.id != 2000));
    // The change is still not written for the editor: closing writes the list again.
    r.close();
    assert_ne!(r.script().0, id);
}

#[test]
fn the_pending_script_is_listed_before_the_scripts_of_the_species() {
    let mut r = rig("ed-pending", &[TAIL]);
    let hoop = tricks(
        r.cat
            .types
            .iter()
            .find(|t| t.name_id == SEAL as i32)
            .unwrap(),
    );
    let item = hoop.into_iter().find(|t| t.id == HOOP).unwrap();
    let pending = Script {
        id: 2001,
        marker: u32::MAX,
        kind: SEAL,
        items: vec![item],
    };
    r.w.zoo.shows.scripts.push(pending);
    info_mut(&mut r.w).lookup(SEAL, 0).pending = 2001;
    r.open();
    assert_eq!(r.shown(), ["Hoop"]);
}

#[test]
fn an_empty_script_gets_the_default_script_once_for_a_record_the_editor_has_not_touched() {
    let mut r = rig("ed-default", &[]);
    assert!(r.shown().is_empty() && r.script().1.is_empty());
    // A record the editor never filled (after a load, or a new one) gets the available tricks.
    info_mut(&mut r.w).lookup(SEAL, 0).default_flag = true;
    r.open();
    assert_eq!(r.shown(), ["Hoop", "Tail"]);
    assert_eq!(r.script().1, [HOOP, FOOD, TAIL]);
    assert!(!info_of_show(&r.w).record(SEAL).unwrap().default_flag);
    // Emptied by the player, it stays empty.
    r.pick(SHOWN, 0);
    r.remove();
    r.pick(SHOWN, 0);
    r.remove();
    r.close();
    r.open();
    assert!(r.shown().is_empty() && r.script().1.is_empty());
    // The flag of a record with a script that has items is not used.
    assert!(!on(&r.ui, 11108));
}

#[test]
fn the_buttons_follow_the_selected_rows() {
    let mut r = rig("ed-buttons", &[TAIL, HOOP, TAIL]);
    // Add is on for an enabled row only, and the grayed ball selects none.
    r.pick(AVAILABLE, 0);
    assert_eq!(r.buttons(), [true, false, false, false]);
    r.pick(AVAILABLE, 2);
    assert_eq!(r.buttons(), [false; 4]);
    // The first row cannot go earlier, the last cannot go later.
    r.pick(SHOWN, 0);
    assert_eq!(r.buttons(), [false, true, false, true]);
    r.pick(SHOWN, 1);
    assert_eq!(r.buttons(), [false, true, true, true]);
    r.pick(SHOWN, 2);
    assert_eq!(r.buttons(), [false, true, true, false]);
}

#[test]
fn tricks_are_added_at_the_end_removed_and_moved_with_the_benefit_following() {
    let mut r = rig("ed-edit", &[]);
    assert_eq!(text(&r.ui, 11120), "0");
    // Add the hoop: it is selected, and the food trick (0) follows it at the threshold.
    assert_eq!(r.add(0), None);
    assert_eq!(
        (r.shown(), r.ui.selected_row(SHOWN)),
        (vec!["Hoop".into()], Some(0))
    );
    assert_eq!(
        (text(&r.ui, 11120), r.buttons()),
        ("+4".into(), [true, true, false, false])
    );
    // The same trick again, right after itself, loses its delta: 4 + 1.
    r.add(0);
    assert_eq!(
        (text(&r.ui, 11120), r.ui.selected_row(SHOWN)),
        ("+5".into(), Some(1))
    );
    r.add(1);
    assert_eq!(
        (text(&r.ui, 11120), r.buttons()),
        ("+8".into(), [true, true, true, false])
    );
    // Later on the last row moves nothing; earlier swaps with the row above and follows it.
    r.shift(false);
    assert_eq!(r.shown(), ["Hoop", "Hoop", "Tail"]);
    r.shift(true);
    assert_eq!(
        (r.shown(), r.ui.selected_row(SHOWN)),
        (vec!["Hoop".into(), "Tail".into(), "Hoop".into()], Some(1))
    );
    // The hoop is no longer right after itself: it keeps the value the last pass left.
    assert_eq!(text(&r.ui, 11120), "+11");
    // Remove selects the row above; row 0 leaves row 0 (the next one) selected.
    r.remove();
    assert_eq!(
        (r.shown(), r.ui.selected_row(SHOWN)),
        (vec!["Hoop".into(), "Hoop".into()], Some(0))
    );
    assert_eq!(r.buttons(), [true, true, false, true]);
    r.remove();
    r.remove();
    assert_eq!(
        (r.shown().len(), r.ui.selected_row(SHOWN), r.buttons()),
        (0, None, [true, false, false, false])
    );
    // An empty show has no benefit and no frequency to choose.
    assert_eq!((text(&r.ui, 11120), on(&r.ui, 11108)), ("0".into(), false));
}

#[test]
fn a_full_show_refuses_the_add_by_equality_and_a_longer_one_still_takes_it() {
    let mut r = rig("ed-full", &[TAIL, HOOP]);
    r.w.units.show.length = 2;
    assert_eq!(r.add(0), Some("show message 11493".into()));
    assert_eq!(r.shown(), ["Tail", "Hoop"]);
    // A list longer than the limit is not full: the test is an equality.
    r.w.units.show.length = 1;
    assert_eq!(r.add(0), None);
    assert_eq!(r.shown().len(), 3);
    // No limit at 0, below 0 and far above.
    for length in [0, -1, i32::MAX] {
        r.w.units.show.length = length;
        assert_eq!(r.add(0), None);
    }
    assert_eq!(r.shown().len(), 6);
}

#[test]
fn closing_writes_the_list_as_a_new_script_with_the_food_trick_and_deletes_the_old_one() {
    let mut r = rig("ed-write", &[TAIL]);
    r.add(0);
    r.add(0);
    r.add(1);
    // Nothing is written before the window closes.
    assert_eq!(r.script(), (2000, vec![TAIL]));
    r.close();
    // Tail 2, hoop 7 (food), hoop 5 (food), tail 2: the food trick follows each run.
    let (id, items) = r.script();
    assert_ne!(id, 2000);
    assert_eq!(items, [TAIL, HOOP, FOOD, HOOP, FOOD, TAIL]);
    assert!(r.w.zoo.shows.scripts.iter().all(|s| s.id != 2000));
    assert_eq!(info_of_show(&r.w).record(SEAL).unwrap().pending, u16::MAX);
    // Written once: closing again changes nothing.
    r.close();
    assert_eq!(r.script().0, id);
}

#[test]
fn while_the_show_runs_the_list_waits_as_the_pending_script() {
    let mut r = rig("ed-running", &[TAIL]);
    let info = info_mut(&mut r.w);
    (info.show.running, info.show.script) = (true, 2000);
    r.add(0);
    r.close();
    let record = info_of_show(&r.w).record(SEAL).unwrap();
    let (script, pending) = (record.script, record.pending);
    assert_eq!(script, 2000);
    assert_ne!(pending, u16::MAX);
    // The species' script is untouched, the pending one holds the list.
    assert_eq!(r.script(), (2000, vec![TAIL]));
    let waiting =
        r.w.zoo
            .shows
            .scripts
            .iter()
            .find(|s| s.id == pending)
            .unwrap();
    assert_eq!(
        waiting.items.iter().map(|i| i.id).collect::<Vec<_>>(),
        [TAIL, HOOP, FOOD]
    );
    // The pending script is the list the editor shows, and a second write reuses it.
    r.open();
    assert_eq!(r.shown(), ["Tail", "Hoop"]);
    r.add(1);
    r.close();
    assert_eq!(info_of_show(&r.w).record(SEAL).unwrap().pending, pending);
    assert_eq!(
        r.w.zoo
            .shows
            .scripts
            .iter()
            .filter(|s| s.id == pending)
            .count(),
        1
    );
}

#[test]
fn the_script_keeps_the_satisfactions_the_pass_gave_and_they_can_differ_from_the_benefit_shown() {
    let mut r = rig("ed-example", &[]);
    for row in [0, 0, 1, 0] {
        r.add(row);
    }
    // Hoop 4 (food), hoop again 4 - 3 (food), tail 3, hoop the 3rd time 1 - 3 = -2 (food).
    assert_eq!(text(&r.ui, 11120), "+6");
    r.close();
    let id = r.script().0;
    let script = r.w.zoo.shows.scripts.iter().find(|s| s.id == id).unwrap();
    let now: Vec<(u16, i32)> = script.items.iter().map(|i| (i.id, i.current)).collect();
    // The write copies the hoop at -2 everywhere and its own pass takes -3 off the third one
    // after a food item: the show will add up 4 - 2 + 3 - 5 = 0, not +6 (kept as the original).
    let hoop = [4, -2, -5];
    let want = [HOOP, FOOD, HOOP, FOOD, TAIL, HOOP, FOOD];
    assert_eq!(now.iter().map(|x| x.0).collect::<Vec<_>>(), want);
    let hoops: Vec<i32> = now.iter().filter(|x| x.0 == HOOP).map(|x| x.1).collect();
    assert_eq!(hoops, hoop);
    assert_eq!(now.iter().find(|x| x.0 == TAIL).unwrap().1, 3);
}

#[test]
fn a_set_exhibit_loses_the_edits_not_written_yet() {
    let mut r = rig("ed-lost", &[TAIL]);
    r.add(0);
    assert_eq!(r.shown(), ["Tail", "Hoop"]);
    // The exhibit is set again while the window is open: the edit is dropped (kept as the original).
    r.open();
    assert_eq!(r.shown(), ["Tail"]);
    r.close();
    assert_eq!(r.script(), (2000, vec![TAIL]));
}

#[test]
fn a_species_change_leaves_the_buttons_on_and_an_earlier_on_no_row_only_marks_the_list_changed() {
    let mut r = rig("ed-stale", &[TAIL, HOOP]);
    r.pick(SHOWN, 0);
    assert_eq!(r.buttons(), [false, true, false, true]);
    let (ui, w, cat, vfs) = (&mut r.ui, &mut r.w, &r.cat, &r.vfs);
    r.p.set_species(ui, w, cat, vfs, None);
    r.p.set_species(ui, w, cat, vfs, Some(SEAL));
    // Both lists are filled again with no selection, and the buttons are as they were.
    assert_eq!(r.ui.selected_row(SHOWN), None);
    assert_eq!(r.buttons(), [false, true, false, true]);
    // Earlier moves nothing but turns the other buttons off and counts as a change.
    r.shift(true);
    assert_eq!(r.shown(), ["Tail", "Hoop"]);
    assert_eq!(r.buttons(), [false; 4]);
    r.close();
    assert_ne!(r.script().0, 2000);
}

#[test]
fn a_research_or_a_toy_asks_the_available_tricks_again_and_a_removed_toy_drops_the_tricks() {
    let mut r = rig("ed-outside", &[TAIL]);
    r.pick(AVAILABLE, 0);
    let ball = |r: &Rig| rows(&r.ui, AVAILABLE)[2].clone();
    assert!(ball(&r).disabled);
    // More water: the ball is valid, but nothing asks until a research or a toy does.
    super::cycle::tank_mut(&mut r.w)
        .tank
        .as_mut()
        .unwrap()
        .level = 5;
    assert!(ball(&r).disabled);
    // A research on another species changes nothing.
    r.p.update_available(&mut r.ui, &r.w, &r.cat, &r.vfs, Some(SEAL + 1));
    assert!(ball(&r).disabled);
    r.p.update_available(&mut r.ui, &r.w, &r.cat, &r.vfs, Some(SEAL));
    assert_eq!((ball(&r).disabled, ball(&r).help), (false, 7));
    // The selection stays and the add button follows it.
    r.pick(AVAILABLE, 2);
    assert!(on(&r.ui, ADD));
    r.add(2);
    assert_eq!(r.shown(), ["Tail", "Ball"]);
    let before = text(&r.ui, 11120);
    // The water drops and a toy is removed: the ball goes from the list, and the benefit stays.
    super::cycle::tank_mut(&mut r.w)
        .tank
        .as_mut()
        .unwrap()
        .level = 3;
    r.p.toy_removed(&mut r.ui, &r.w, &r.cat, &r.vfs);
    assert_eq!(r.shown(), ["Tail"]);
    assert_eq!(text(&r.ui, 11120), before);
    assert!(rows(&r.ui, AVAILABLE)[2].disabled && !on(&r.ui, ADD));
    // The removal marks the list changed: closing writes it.
    r.close();
    assert_eq!(r.script().1, [TAIL]);
    assert_ne!(r.script().0, 2000);
}

#[test]
fn showing_the_tab_turns_the_buttons_off_and_forgets_the_change() {
    let mut r = rig("ed-tab", &[TAIL]);
    r.add(0);
    assert!(on(&r.ui, REMOVE));
    r.ui.show(42);
    r.p.look_script(&mut r.ui);
    assert_eq!(r.buttons(), [false; 4]);
    // The change is forgotten: closing writes nothing.
    r.close();
    assert_eq!(r.script(), (2000, vec![TAIL]));
    // The tab is looked at once when it is shown, not at every look.
    r.pick(SHOWN, 0);
    r.p.look_script(&mut r.ui);
    assert!(on(&r.ui, REMOVE));
}

#[test]
fn maxlength_of_shows_cfg_is_read_and_a_missing_key_means_no_limit() {
    let config = Config::parse(b"[show]\nmaxLength=20\n").unwrap();
    assert_eq!(Params::read(&config).length, 20);
    assert_eq!(Params::read(&Config::parse(b"[show]\n").unwrap()).length, 0);
    assert_eq!(Params::default().length, 0);
}

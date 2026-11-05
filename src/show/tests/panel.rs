//! The show panel (information panels 19; shows 33.12): what it fills, what its controls write.

use super::cycle::{SEAL, info_mut, info_of_show, staged, tank_mut};
use super::*;
use crate::fixtures::Install;
use crate::panel::show::{self, Panel};
use crate::ui::draw::Strings;
use crate::ui::interface::Interface;
use crate::ui::layout::{DISABLED, Tree};
use crate::vfs::Vfs;

struct Words;

impl Strings for Words {
    fn string(&self, id: i32) -> String {
        let text = match id {
            22100 => "$",
            5400 => "Seal",
            11192 => "Never",
            11193 => "Infrequent",
            11194 => "Frequent",
            11195 => "Continuous",
            _ => "",
        };
        text.to_string()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        Vec::new()
    }
}

/// The window with the elements the panel writes, all of one size.
fn window(name: &str) -> Interface {
    let one =
        |kind: &str, id: i32| format!("[e{id}]\ntype={kind}\nid={id}\nx=0\ny=0\ndx=60\ndy=20\n");
    let texts = [11102, 11120, 11173].into_iter().chain(11140..11156);
    let buttons = [11105, 11106, 11123, 11108, 11113, 11114, 11117, 11118];
    let lists = [11111, 11115, 11196, 11191];
    let body: String = texts.map(|i| one("UIText", i)).collect::<String>()
        + &buttons.map(|i| one("UIButton", i)).concat()
        + &lists.map(|i| one("UIListBox", i)).concat()
        + &one("UIEditableText", 11104);
    let layout = format!("[LayoutInfo]\nid=41\nx=0\ny=0\ndx=60\ndy=20\n{body}");
    let root =
        "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n[b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let files = [
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/p.lyt", layout.into_bytes()),
    ];
    let install = Install::new(name, &files);
    let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Words));
    ui.show(show::LAYOUT);
    ui
}

fn text(ui: &Interface, id: i32) -> String {
    String::from_utf8(ui.text(id).unwrap()).unwrap()
}

fn on(ui: &Interface, id: i32) -> bool {
    ui.tree.elements[ui.tree.find(id).unwrap()].state & DISABLED == 0
}

/// The staged world with the panel open on its show tank.
fn open(name: &str) -> (World, Catalogue, Interface, Panel, u32) {
    let (mut w, cat) = staged(vec![]);
    let x = tank_mut(&mut w).id;
    let mut ui = window(name);
    let mut p = Panel::default();
    p.set_exhibit(&mut ui, &mut w, &cat, Some(x));
    (w, cat, ui, p, x)
}

#[test]
fn the_typed_text_loses_the_first_symbol_and_every_separator() {
    let typed = |t: &str| show::typed(t.as_bytes(), b"$", b",");
    assert_eq!(typed("$1,234,567.50"), 1_234_567.5);
    // A second symbol stops the number, as atof does.
    assert_eq!(typed("$$5"), 0.0);
    assert_eq!((typed("-3"), typed("1e30"), typed("")), (-3.0, 1e30, 0.0));
    assert_eq!(show::typed(b"5.5", b"", b""), 5.5);
}

#[test]
fn the_species_rows_skip_type_0_the_specialist_and_records_without_units() {
    let (mut w, ..) = staged(vec![]);
    let x = tank_mut(&mut w).id;
    let ui = window("panel-species");
    let info = info_mut(&mut w);
    info.lookup(0, 0).units = vec![1];
    info.lookup(5401, 0).units = vec![2];
    // A record with units whose name string is missing gets no row either.
    info.lookup(5300, 0).units = vec![3];
    assert_eq!(show::species_of(&ui, &w, x), [SEAL]);
}

#[test]
fn opening_on_a_show_tank_fills_the_species_the_admission_and_the_frequency() {
    let (mut w, cat, mut ui, mut p, _) = open("panel-open");
    assert_eq!((p.species, p.rows.clone()), (Some(SEAL), vec![SEAL]));
    assert_eq!(text(&ui, 11123), "Seal");
    // The frequency of the info, 90, is the third row.
    assert_eq!(
        (text(&ui, 11108), ui.selected_row(11191)),
        ("Frequent".into(), Some(2))
    );
    assert_eq!(text(&ui, 11104), "$0.00");
    assert!(on(&ui, 11105) && !on(&ui, 11106) && !on(&ui, 11113));
    // The refresh of the same state tells the exhibit and species once.
    assert_eq!(p.refresh(&mut ui, &mut w, &cat).len(), 2);
    assert!(p.refresh(&mut ui, &mut w, &cat).is_empty());
}

#[test]
fn the_buttons_and_the_typed_text_set_the_admission_and_the_grandstand_price() {
    let (mut w, cat, mut ui, p, _) = open("panel-admission");
    p.press(&mut ui, &mut w, &cat, true);
    assert_eq!(
        (text(&ui, 11104), w.units.buildings[&7].price),
        ("$0.25".into(), 0.25)
    );
    assert!(on(&ui, 11106));
    // The text ends an edit: the symbol and the separators go, a number above the maximum is it.
    ui.set_text(11104, b"$1,000.00");
    p.type_admission(&mut ui, &mut w, &cat);
    assert_eq!(
        (text(&ui, 11104), w.units.buildings[&7].price),
        ("$10.00".into(), 10.0)
    );
    assert!(!on(&ui, 11105));
    // A text that is no number is 0: the minimum.
    ui.set_text(11104, b"free");
    p.type_admission(&mut ui, &mut w, &cat);
    assert_eq!((text(&ui, 11104), on(&ui, 11106)), ("$0.00".into(), false));
}

#[test]
fn never_turns_the_controls_off_but_the_frequency_and_another_row_turns_them_on() {
    let (mut w, cat, mut ui, mut p, _) = open("panel-never");
    w.units.ai_ticks = 40;
    p.choose_frequency(&mut ui, &mut w, &cat, 0);
    assert_eq!(
        (info_of_show(&w).frequency, info_of_show(&w).next),
        (-1, -1)
    );
    assert_eq!(text(&ui, 11108), "Never");
    assert!(![11123, 11105, 11106, 11104].iter().any(|&i| on(&ui, i)) && on(&ui, 11108));
    // Infrequent is 120 ticks from now (the built-in number).
    p.choose_frequency(&mut ui, &mut w, &cat, 1);
    assert_eq!(
        (info_of_show(&w).frequency, info_of_show(&w).next),
        (120, 160)
    );
    assert!([11123, 11104, 11105].iter().all(|&i| on(&ui, i)));
    // The same row again, and a row that is not one, change nothing.
    p.choose_frequency(&mut ui, &mut w, &cat, 1);
    p.choose_frequency(&mut ui, &mut w, &cat, -1);
    assert_eq!(info_of_show(&w).frequency, 120);
}

#[test]
fn a_frequency_of_never_aborts_the_announced_show_but_not_the_running_one() {
    let (mut w, cat) = staged(vec![]);
    info_mut(&mut w).show.ready = true;
    crate::show::set_frequency(&mut w, &cat, 1001, -1);
    assert!(!info_of_show(&w).show.ready);
    assert_eq!(w.units.posts.iter().filter(|e| e.kind == 0x2717).count(), 1);
    // A started show runs on: only the frequency and the next time change.
    let (mut w, cat, _) = super::cycle::started(vec![super::cycle::item(0, 3, 5)]);
    crate::show::set_frequency(&mut w, &cat, 1001, -1);
    assert!(info_of_show(&w).show.running && info_of_show(&w).next == -1);
}

#[test]
fn shows_cfg_gives_the_three_intervals_and_a_missing_key_keeps_its_own() {
    let config =
        Config::parse(b"[timing]\ninfrequentShowInterval=300\nfrequentShowInterval=0\n").unwrap();
    assert_eq!(Params::read(&config).intervals, [300, 0, 65]);
    assert_eq!(Params::default().intervals, [120, 90, 65]);
}

#[test]
fn the_species_drop_down_selects_a_species_and_a_missing_row_selects_none() {
    let (mut w, cat, mut ui, mut p, _) = open("panel-choice");
    p.choose_species(&mut ui, &mut w, &cat, -1);
    assert_eq!(
        (p.species, text(&ui, 11123), on(&ui, 11123)),
        (None, "".into(), false)
    );
    assert_eq!(text(&ui, 11104), "$0.00");
    p.choose_species(&mut ui, &mut w, &cat, 0);
    assert_eq!((p.species, text(&ui, 11123)), (Some(SEAL), "Seal".into()));
    // A species that leaves the show leaves the list: the refresh empties the panel.
    info_mut(&mut w).lookup(SEAL, 0).units.clear();
    p.refresh(&mut ui, &mut w, &cat);
    assert_eq!(
        (p.species, text(&ui, 11123), on(&ui, 11108)),
        (None, "".into(), false)
    );
}

#[test]
fn the_finances_show_the_whole_show_with_averages_over_the_months_in_operation() {
    let (mut w, cat, mut ui, mut p, _) = open("panel-finances");
    // The record of the show's species (type 0 between shows) is six months old.
    let now = crate::timing::file_time(w.units.now);
    info_mut(&mut w).lookup(0, 0).date = now - 170 * 864_000_000_000;
    let f = |v: f32| v.to_bits();
    let info = info_mut(&mut w);
    info.attendance = [4, 3, 180];
    info.receipts = [f(10.0), f(2.5), f(1012.0)];
    info.upkeep = [f(30.0), f(0.0), f(120.0)];
    p.refresh(&mut ui, &mut w, &cat);
    let row = |first: i32| (first..first + 4).map(|i| text(&ui, i)).collect::<Vec<_>>();
    assert_eq!(row(11140), ["4", "3", "30", "180"]);
    assert_eq!(row(11144), ["$10.00", "$2.50", "$168.67", "$1,012.00"]);
    assert_eq!(row(11148), ["($30.00)", "$0.00", "($20.00)", "($120.00)"]);
    assert_eq!(row(11152), ["($20.00)", "$2.50", "$148.67", "$892.00"]);
    let colour = |id| ui.tree.elements[ui.tree.find(id).unwrap()].look.fore_color;
    assert_eq!(
        [11152, 11153, 11154].map(colour)[..2],
        [[250, 7, 7], colour(11144)]
    );
    // The record was made by the lookup, and nothing is shown without a species.
    assert!(info_of_show(&w).record(0).is_some());
    p.choose_species(&mut ui, &mut w, &cat, -1);
    assert!(
        (11140..11156)
            .chain([11173])
            .all(|i| text(&ui, i).is_empty())
    );
}

#[test]
fn no_exhibit_hides_the_window() {
    let (mut w, cat, mut ui, mut p, _) = open("panel-none");
    p.set_exhibit(&mut ui, &mut w, &cat, None);
    assert!(!ui.shown(show::LAYOUT));
    assert_eq!((p.species, text(&ui, 11104)), (None, "$0.00".into()));
    assert!(!on(&ui, 11105) && !on(&ui, 11106));
}

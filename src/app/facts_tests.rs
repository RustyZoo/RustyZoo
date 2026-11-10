//! The facts plaques through the pass (unit placing 17): the Adopt and Hire buttons, the
//! recommendations plaque's button and the plaques' own buttons, on the made-up map of the
//! bulldozer tests with a small layout for each plaque.

use super::adopt_tests::hold;
use super::bulldozer_tests::{Session, catalogue, ty, with_game_of};
use crate::entity::{Catalogue, EntityType};
use crate::ui::layout::DISABLED;

const FOX: usize = 20;
const JELLY: usize = 21;
const RANGER: usize = 22;
const BASE: usize = 23;
/// The first of the foxes that differ in their `cKeeper` (see `KEEPERS`), then of those that
/// differ in their `cEra`.
const KEEPER_FOXES: usize = 25;
const ERA_FOXES: usize = 32;
const ZEBRA: usize = 13;

const KEEPERS: [(&str, i32); 7] = [
    ("", 0),
    ("cKeeper=9500\n", 0),
    ("cKeeper=9551\n", 1),
    ("cKeeper=9552\n", 2),
    ("cKeeper=0\n", 0),
    ("cKeeper=-1\n", 0),
    ("cKeeper=4294967296\n", 0),
];
const ERAS: [(&str, bool); 5] = [
    ("", false),
    ("cEra=0\n", false),
    ("cEra=5400\n", true),
    ("cEra=-5\n", true),
    ("cEra=4294967296\n", true),
];

fn animal(name: &str, name_id: i32, strings: &str, ints: &str, tricks: bool) -> EntityType {
    let tricks = if tricks {
        "[m/tricks]\ntrick=11204\n"
    } else {
        ""
    };
    let ini = format!(
        "[Global]\nDefaultSubtype=m\n[m/Characteristics/Strings]\n{strings}\n\
         [m/Characteristics/Integers]\ncPurchaseCost=100\n{ints}{tricks}"
    );
    EntityType {
        name_id,
        ..ty("animals", name.as_bytes(), b"m", &ini, [1, 1, 1])
    }
}

fn staff(class: &'static str, name: &str, name_id: i32, text: &str, picture: &str) -> EntityType {
    let ini = format!(
        "[Global]\nDefaultSubtype=m\n[m/Characteristics/Strings]\n\
         cGeneralInfoTextName={text}\n{picture}[m/Characteristics/Integers]\ncPurchaseCost=0\n"
    );
    EntityType {
        name_id,
        ..ty(class, name.as_bytes(), b"m", &ini, [1, 1, 1])
    }
}

/// The catalogue of the bulldozer tests, then 20 a fox that performs, 21 a jellyfish that
/// performs (name 5403), 22 a ranger, 23 the DRT base, 24 the helicopter (name 9550), the foxes of
/// `KEEPERS` and those of `ERAS`.
fn types() -> Catalogue {
    let mut cat = catalogue();
    let fox = "cPlaqueImageName=dozer/obj\ncPrefIcon=dozer/small\ncGeneralInfoTextName=FOX";
    let ints = "cHabitat=9400\ncLocation=9600\ncEra=5400\ncPrefIconID=7060\ncKeeper=9551\n";
    cat.push(animal("fox", 9001, fox, ints, true));
    cat.push(animal("jelly", 5403, fox, ints, true));
    let picture = "cPlaqueImageName=dozer/obj\n";
    cat.push(staff("keeper", "ranger", 5400, "KEEPER", picture));
    cat.push(EntityType {
        name_id: 8031,
        ..ty("building", b"helibase", b"", "", [1, 1, 1])
    });
    cat.push(staff("helicopter", "helicopter", 9550, "FOX", ""));
    for (keeper, _) in KEEPERS {
        cat.push(animal("k", 9001, fox, keeper, false));
    }
    for (era, _) in ERAS {
        cat.push(animal("e", 9001, "", era, false));
    }
    cat
}

fn files() -> Vec<(String, Vec<u8>)> {
    let layout = |id: i32, body: &str| {
        let head = format!("[LayoutInfo]\nid={id}\nlayer=5\nx=0\ny=0\ndx=60\ndy=60\nstate=1\n");
        (format!("ui/w{id}.lyt"), (head + body).into_bytes())
    };
    let button = |id: i32, at: i32, rest: &str| {
        format!("[b{id}]\ntype=UIButton\nid={id}\nx={at}\ndx=8\ndy=8\ntransparent=0\n{rest}")
    };
    let item = |kind: &str, id: i32| format!("[e{id}]\ntype={kind}\nid={id}\ndx=8\ndy=8\n");
    let species: String = [(25001, "UIText"), (25002, "UIImage"), (25003, "UIText")]
        .into_iter()
        .chain([(25050, "UIImage"), (25051, "UIImage"), (25052, "UIImage")])
        .chain([(25053, "UIImage"), (25054, "UIImage"), (25055, "UIText")])
        .chain([(25098, "UIImage"), (25099, "UIText")])
        .map(|(id, kind)| item(kind, id))
        .collect();
    let keeper = "state=2\naction=4\ntarget=28\nanimation=dozer/obj\nanimation=dozer/obj\n\
                  animation=dozer/obj\nhelpid=3123\nhelpid=3124\nhelpid=3125\n";
    let staff_items: String = [(40001, "UIText"), (40002, "UIImage"), (40003, "UIText")]
        .into_iter()
        .map(|(id, kind)| item(kind, id))
        .collect();
    vec![
        layout(
            27,
            &(species + &button(25011, 20, keeper) + &button(25010, 30, "action=2\ntarget=-1\n")),
        ),
        layout(28, &button(26021, 0, "action=4\ntarget=27\n")),
        layout(
            33,
            &(staff_items + &button(40010, 30, "action=2\ntarget=33\n")),
        ),
        (
            "ui/animal2.lyt".into(),
            ("[LayoutInfo]\nid=6\nlayer=7\nx=0\ny=0\ndx=40\ndy=10\n".to_string()
                + &button(3100, 0, "target=6\naction=2\n")
                + &button(3122, 10, "state=2048\n"))
                .into_bytes(),
        ),
        (
            "ui/infoimg.cfg".into(),
            b"[Images]\n9400=dozer/obj\n9600=dozer/small\n".to_vec(),
        ),
    ]
}

fn root() -> String {
    let mut root = super::bulldozer_tests::ROOT.replace("ui/animal.lyt", "ui/animal2.lyt");
    for id in [27, 28, 33] {
        root += &format!("[w{id}]\ntype=UILayout\nlayout=ui/w{id}.lyt\nstate=1\n");
    }
    root += "[p]\ntype=UIButton\nid=2050\nx=100\ny=200\ndx=8\ndy=8\nstate=2048\ntransparent=0\n";
    root += "[q]\ntype=UIButton\nid=3699\nx=120\ny=200\ndx=8\ndy=8\nstate=2048\n\
             action=3\ntarget=33\ntransparent=0\n";
    root
}

fn with_facts(name: &str, body: impl FnOnce(&mut Session)) {
    let files = files();
    let more: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    with_game_of(name, "", (types(), &root(), &more), |s| {
        for id in [2050, 3699] {
            s.app.interface().unwrap().set_hidden(id, false);
        }
        s.pass(&[]);
        body(s);
    });
}

fn shown(s: &mut Session, id: i32) -> bool {
    s.app.interface().unwrap().shown(id)
}

fn down(s: &mut Session, id: i32) -> bool {
    s.app.interface().unwrap().is_down(id)
}

fn press(s: &mut Session, id: i32) {
    assert!(s.app.interface().unwrap().press_button(id), "button {id}");
    s.pass(&[]);
}

fn text(s: &mut Session, id: i32) -> String {
    let bytes = s.app.interface().unwrap().text(id).unwrap();
    String::from_utf8_lossy(&bytes).into()
}

/// The tooltip, the picture number and whether the element is enabled.
fn button(s: &mut Session, id: i32) -> (i32, usize, bool) {
    let ui = s.app.interface().unwrap();
    let e = &ui.tree.elements[ui.tree.find(id).unwrap()];
    (e.help_id[0], e.look.shown, e.state & DISABLED == 0)
}

fn tip(s: &mut Session, id: i32) -> i32 {
    let ui = s.app.interface().unwrap();
    ui.tree.elements[ui.tree.find(id).unwrap()].help_id[0]
}

/// The Adopt panel holds `ty` and its facts button is pressed.
fn open_species(s: &mut Session, ty: usize) {
    hold(&mut s.app, ty);
    press(s, 2050);
}

#[test]
fn the_adopt_button_opens_the_species_plaque_written_for_the_held_animal() {
    with_facts("facts-adopt", |s| {
        s.app.interface().unwrap().show(28);
        hold(&mut s.app, FOX);
        s.pass(&[]);
        assert!(!shown(s, 27), "nothing opens it before the button");
        press(s, 2050);
        assert!(shown(s, 27) && !shown(s, 28) && down(s, 2050));
        assert_eq!(text(s, 25001), "Shop");
        assert_eq!(text(s, 25003), "foxes");
        assert_eq!(text(s, 25055), "Seal");
        for id in [
            25002, 25050, 25051, 25052, 25053, 25054, 25055, 25098, 25099,
        ] {
            assert!(shown(s, id), "element {id}");
        }
        // The location and the habitat with their values, the preferred object with its id.
        assert_eq!(
            [25050, 25051, 25053].map(|id| tip(s, id)),
            [9600, 9400, 7060]
        );
        // The keeper button: the scientist's picture and tooltip, disabled off an animal panel.
        assert_eq!(button(s, 25011), (3124, 1, false));
        press(s, 2050);
        assert!(!shown(s, 27) && !down(s, 2050));
    });
}

#[test]
fn the_adopt_button_does_nothing_without_a_held_animal() {
    with_facts("facts-none", |s| {
        press(s, 2050);
        assert!(!shown(s, 27));
        // A staff member and a fence held, with an animal chosen before: no animal is held.
        press(s, 2050);
        hold(&mut s.app, FOX);
        hold(&mut s.app, 0);
        press(s, 2050);
        assert!(!shown(s, 27));
        press(s, 2050);
        hold(&mut s.app, RANGER);
        press(s, 2050);
        assert!(!shown(s, 27));
        press(s, 2050);
        // The same button opens it once an animal is held.
        open_species(s, FOX);
        assert!(shown(s, 27));
    });
}

#[test]
fn a_new_adopt_choice_writes_the_plaque_again_only_when_the_species_changed() {
    with_facts("facts-choice", |s| {
        open_species(s, FOX);
        s.app.interface().unwrap().set_text(25001, b"mark");
        hold(&mut s.app, FOX);
        s.pass(&[]);
        assert_eq!(text(s, 25001), "mark", "the same species");
        hold(&mut s.app, ZEBRA);
        s.pass(&[]);
        assert_eq!((text(s, 25001), shown(s, 25055)), ("Shop".into(), false));
        assert!(shown(s, 27) && down(s, 2050));
    });
}

#[test]
fn the_closing_button_forgets_the_animal_and_releases_the_adopt_button() {
    with_facts("facts-close", |s| {
        open_species(s, FOX);
        press(s, 25010);
        assert!(!shown(s, 27) && !down(s, 2050));
        // The plaque does not come back by itself when the choice changes.
        hold(&mut s.app, ZEBRA);
        s.pass(&[]);
        assert!(!shown(s, 27));
    });
}

#[test]
fn the_recommendations_button_opens_the_species_plaque_and_the_keeper_button_goes_back() {
    with_facts("facts-keeper", |s| {
        let id = s.add(FOX, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.app.open_animal(id);
        s.pass(&[]);
        press(s, 3122);
        assert!(shown(s, 28) && !shown(s, 27) && down(s, 3122));
        press(s, 26021);
        assert!(
            shown(s, 27) && !shown(s, 28),
            "the species plaque replaces it"
        );
        assert!(down(s, 2050), "its Adopt button is selected");
        assert_eq!(text(s, 25003), "foxes");
        // Opened from an animal's panel: the keeper button is enabled.
        assert_eq!(button(s, 25011), (3124, 1, true));
        press(s, 25011);
        assert!(shown(s, 28) && !shown(s, 27) && !down(s, 2050));
        assert!(down(s, 3122), "the animal panel's button is down again");
        assert_eq!(s.app.game.as_ref().unwrap().panel.plaque, Some(id));
    });
}

#[test]
fn another_animal_panel_changes_the_species_and_no_animal_closes_the_plaque() {
    with_facts("facts-selected", |s| {
        let fox = s.add(FOX, 20 * 64 + 32, 20 * 64 + 32, 0);
        let zebra = s.add(ZEBRA, 22 * 64 + 32, 20 * 64 + 32, 0);
        s.app.open_animal(fox);
        s.pass(&[]);
        press(s, 3122);
        press(s, 26021);
        s.app.open_animal(zebra);
        s.pass(&[]);
        assert!(shown(s, 27));
        assert_eq!((text(s, 25003), shown(s, 25055)), ("".into(), false));
        s.app.close_animal();
        // The look that notices the deselected animal hides the plaque, the next one its button.
        for _ in 0..3 {
            s.pass(&[]);
        }
        assert!(!shown(s, 27) && !down(s, 2050));
    });
}

#[test]
fn the_keeper_button_shows_the_picture_and_tooltip_of_the_keeper_kind() {
    with_facts("facts-kinds", |s| {
        open_species(s, FOX);
        for (n, (_, kind)) in KEEPERS.into_iter().enumerate() {
            hold(&mut s.app, KEEPER_FOXES + n);
            s.pass(&[]);
            assert_eq!(
                button(s, 25011),
                (3123 + kind, kind as usize, false),
                "case {n}"
            );
        }
    });
}

#[test]
fn the_era_shows_when_it_is_not_zero_and_is_hidden_when_zero_or_missing() {
    with_facts("facts-era", |s| {
        open_species(s, FOX);
        for (n, (_, era)) in ERAS.into_iter().enumerate() {
            hold(&mut s.app, ERA_FOXES + n);
            s.pass(&[]);
            assert_eq!(shown(s, 25055), era, "case {n}");
        }
    });
}

#[test]
fn the_two_show_elements_need_a_performing_species_that_is_not_the_jellyfish() {
    with_facts("facts-shows", |s| {
        open_species(s, FOX);
        assert_eq!([25098, 25099].map(|id| shown(s, id)), [true, true]);
        hold(&mut s.app, JELLY);
        s.pass(&[]);
        assert_eq!([25098, 25099].map(|id| shown(s, id)), [false, false]);
        hold(&mut s.app, ZEBRA);
        s.pass(&[]);
        assert_eq!([25098, 25099].map(|id| shown(s, id)), [false, false]);
        hold(&mut s.app, FOX);
        s.pass(&[]);
        assert_eq!([25098, 25099].map(|id| shown(s, id)), [true, true]);
    });
}

#[test]
fn the_hire_button_writes_the_staff_plaque_and_the_close_button_releases_it() {
    with_facts("facts-hire", |s| {
        hold(&mut s.app, RANGER);
        press(s, 3699);
        assert!(shown(s, 33) && down(s, 3699));
        assert_eq!(
            (text(s, 40001), text(s, 40003)),
            ("Seal".into(), "keeps".into())
        );
        assert!(shown(s, 40002));
        press(s, 40010);
        assert!(!shown(s, 33) && !down(s, 3699));
        // The button again: the plaque is shown by the button and written for the item.
        press(s, 3699);
        assert!(shown(s, 33));
        press(s, 3699);
        assert!(!shown(s, 33) && !down(s, 3699));
    });
}

#[test]
fn the_drt_base_shows_the_helicopter_and_an_item_that_is_no_staff_leaves_the_plaque_empty() {
    with_facts("facts-heli", |s| {
        hold(&mut s.app, BASE);
        press(s, 3699);
        assert!(shown(s, 33));
        assert_eq!(text(s, 40003), "foxes", "the helicopter's text");
        assert!(!shown(s, 40002), "and it has no picture");
        press(s, 3699);
        // The fence and the animals are no staff type: shown by the button, never written.
        for ty in [0, FOX] {
            hold(&mut s.app, ty);
            press(s, 3699);
            assert!(shown(s, 33), "item {ty}");
            assert_eq!(text(s, 40001), "", "item {ty}");
            press(s, 3699);
        }
    });
}

#[test]
fn a_new_hire_choice_rewrites_the_open_staff_plaque() {
    with_facts("facts-hire-follow", |s| {
        hold(&mut s.app, RANGER);
        press(s, 3699);
        assert_eq!(text(s, 40003), "keeps");
        hold(&mut s.app, BASE);
        s.pass(&[]);
        assert!(shown(s, 33) && down(s, 3699));
        assert_eq!(
            text(s, 40003),
            "foxes",
            "the helicopter's text after the new choice"
        );
        // An item that is no staff changes nothing; the plaque is kept.
        hold(&mut s.app, 0);
        s.pass(&[]);
        assert_eq!(text(s, 40003), "foxes");
    });
}

/// The yeti, a user-made animal (name id in the user range) with `info` as its language section.
fn with_yeti(
    name: &str,
    info: &str,
    member: Option<&[u8]>,
    body: impl FnOnce(&mut Session, usize),
) {
    let mut cat = types();
    let yeti = cat.types.len();
    let ini = format!(
        "[Global]\nDefaultSubtype=m\n[1033]\ncName=Yeti\n{info}\
         [m/Characteristics/Strings]\ncGeneralInfoTextName=FOX\n\
         [m/Characteristics/Integers]\ncPurchaseCost=100\n"
    );
    cat.push(EntityType {
        name_id: 19001,
        ..ty("animals", b"yeti", b"m", &ini, [1, 1, 1])
    });
    let mut files = files();
    files.extend(member.map(|b| ("yeti.txt".into(), b.to_vec())));
    let more: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    with_game_of(name, "", (cat, &root(), &more), |s| {
        for id in [2050, 3699] {
            s.app.interface().unwrap().set_hidden(id, false);
        }
        s.pass(&[]);
        body(s, yeti);
    });
}

#[test]
fn a_user_animal_plaque_shows_its_info_file() {
    let key = "cGeneralInfoFileName=yeti.txt\n";
    with_yeti("facts-yeti", key, Some(b"Abominable"), |s, yeti| {
        open_species(s, yeti);
        assert!(shown(s, 27));
        assert_eq!(text(s, 25003), "Abominable");
    });
    // Read again each time the plaque is written, not kept.
    with_yeti("facts-yeti-twice", key, Some(b"Abominable"), |s, yeti| {
        open_species(s, yeti);
        press(s, 2050);
        s.app.interface().unwrap().set_text(25003, b"stale");
        press(s, 2050);
        assert_eq!(text(s, 25003), "Abominable");
    });
}

#[test]
fn a_user_animal_plaque_is_empty_when_its_info_file_is_missing_or_unnamed() {
    let key = "cGeneralInfoFileName=yeti.txt\n";
    for (n, info, member) in [
        ("facts-yeti-nomember", key, None),
        ("facts-yeti-empty", key, Some(&b""[..])),
        ("facts-yeti-nokey", "", Some(&b"Abominable"[..])),
        (
            "facts-yeti-blank",
            "cGeneralInfoFileName=\n",
            Some(&b"x"[..]),
        ),
    ] {
        with_yeti(n, info, member, |s, yeti| {
            open_species(s, yeti);
            assert_eq!(text(s, 25003), "", "{n}");
        });
    }
}

#[test]
fn the_info_file_is_read_for_the_species_plaque_of_user_animals_only() {
    // A built-in animal and the staff plaque keep their text entries.
    with_yeti(
        "facts-yeti-others",
        "cGeneralInfoFileName=yeti.txt\n",
        Some(b"Abominable"),
        |s, _| {
            open_species(s, FOX);
            assert_eq!(text(s, 25003), "foxes");
            hold(&mut s.app, RANGER);
            press(s, 3699);
            assert_eq!(text(s, 40003), "keeps");
        },
    );
}

/// The panel `p` of the build panels was open and is closed by the next pass.
fn close_panel(s: &mut Session, p: usize) {
    s.app.game.as_mut().unwrap().build.open_hidden(p);
    s.pass(&[]);
}

#[test]
fn closing_the_hire_panel_with_the_facts_button_down_hides_the_staff_plaque() {
    with_facts("facts-hire-close", |s| {
        hold(&mut s.app, RANGER);
        press(s, 3699);
        assert!(shown(s, 33));
        close_panel(s, 3);
        s.pass(&[]);
        assert!(!shown(s, 33) && !down(s, 3699));
        // The button up: nothing to hide, and the plaque opened again is not closed by a stale close.
        hold(&mut s.app, RANGER);
        press(s, 3699);
        s.pass(&[]);
        assert!(shown(s, 33) && down(s, 3699));
        // Another panel closing leaves it.
        close_panel(s, 1);
        s.pass(&[]);
        assert!(shown(s, 33));
    });
}

#[test]
fn closing_the_adopt_panel_hides_the_plaque_of_the_pending_animal_only() {
    with_facts("facts-adopt-close", |s| {
        open_species(s, FOX);
        close_panel(s, 2);
        s.pass(&[]);
        assert!(!shown(s, 27) && !down(s, 2050));
        // A plaque opened from a placed animal is not the pending one: it stays.
        let id = s.add(FOX, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.app.open_animal(id);
        s.pass(&[]);
        press(s, 3122);
        press(s, 26021);
        assert!(shown(s, 27));
        close_panel(s, 2);
        s.pass(&[]);
        assert!(shown(s, 27) && down(s, 2050));
    });
}

#[test]
fn a_user_animal_plaque_gives_its_preferred_object_as_a_number_or_a_user_type_name() {
    let mut cat = types();
    let fox = "cPrefIcon=dozer/small";
    let wants = [("8102", 8102), ("myrock", 19002), ("unknown", 0), ("99", 0)];
    let first = cat.types.len();
    cat.push(animal("myrock", 19002, fox, "", false));
    for (value, _) in wants {
        let ints = format!("cPrefIconID={value}\n");
        cat.push(animal("yeti", 19001, fox, &ints, false));
    }
    let files = files();
    let more: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    with_game_of("facts-pref", "", (cat, &root(), &more), |s| {
        for id in [2050, 3699] {
            s.app.interface().unwrap().set_hidden(id, false);
        }
        s.pass(&[]);
        open_species(s, first + 1);
        for (n, (value, want)) in wants.into_iter().enumerate() {
            hold(&mut s.app, first + 1 + n);
            s.pass(&[]);
            assert_eq!(tip(s, 25053), want, "{value}");
        }
    });
}

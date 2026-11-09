//! The recovery team panel through the pass (staff 18): the helicopter clicked, the name field and
//! the fire button, on the map of the bulldozer tests with a made-up layout 1400.

use super::bulldozer_tests::{Session, catalogue, with_game_of};
use super::*;
use crate::panel::recovery::{FIRE, LAYOUT, NAME};
use crate::unit::Kind;

const DRT: &str = "[LayoutInfo]\nid=1400\nlayer=7\nx=0\ny=0\ndx=40\ndy=40\n\
    [a]\ntype=UIEditableText\nid=1403\ndx=8\ndy=8\n\
    [b]\ntype=UIButton\nid=1405\nx=10\ndx=8\ndy=8\ntransparent=0\n\
    [c]\ntype=UIButton\nid=1419\nx=20\ndx=8\ndy=8\ntransparent=0\n\
    [d]\ntype=UIButton\nid=1418\nx=30\ndx=8\ndy=8\ntarget=1400\naction=2\ntransparent=0\n";

fn with_team(name: &str, ini: &str, body: impl FnOnce(&mut Session)) {
    let root = format!(
        "{}[k]\ntype=UILayout\nlayout=ui/drt.lyt\nstate=1\n",
        super::bulldozer_tests::ROOT
    );
    let files = [("ui/drt.lyt", DRT.as_bytes().to_vec())];
    with_game_of(name, ini, (catalogue(), &root, &files), body);
}

fn shown(s: &Session, layout: i32) -> bool {
    s.app.ui.as_ref().unwrap().shown(layout)
}

/// A helicopter (a keeper of the catalogue made one) standing on tile (19, 20), with a shop as its
/// base: one with no base removes itself at its first goal update.
fn heli(s: &mut Session) -> u32 {
    let base = s.add(2, 30 * 64 + 32, 30 * 64 + 32, 0);
    let id = s.add(12, 19 * 64 + 32, 20 * 64 + 32, 0);
    let unit = s
        .app
        .game
        .as_mut()
        .unwrap()
        .world
        .units
        .get_mut(id)
        .unwrap();
    unit.kind = Kind::Heli;
    if let crate::unit::Part::Staff(team) = &mut unit.part {
        team.heli.base = base;
    }
    s.pass(&[]);
    id
}

#[test]
fn a_helicopter_clicked_shows_the_team_panel_and_not_the_staff_panel() {
    with_team("team-panel", "", |s| {
        // Adding an entity makes the units again: the keeper first, then the helicopter.
        let keeper = s.add(12, 22 * 64 + 32, 20 * 64 + 32, 0);
        let team = heli(s);
        s.click(s.at(19, 20, 32, 32));
        s.pass(&[]);
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(team));
        assert!(shown(s, LAYOUT) && !shown(s, staff::LAYOUT));
        // A keeper clicked has the staff panel and closes the team panel.
        s.click(s.at(22, 20, 32, 32));
        s.pass(&[]);
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(keeper));
        assert!(shown(s, staff::LAYOUT) && !shown(s, LAYOUT));
        // The helicopter again; the panel's close button clears the selection.
        s.click(s.at(19, 20, 32, 32));
        s.pass(&[]);
        assert!(shown(s, LAYOUT) && !shown(s, staff::LAYOUT));
        assert!(s.app.ui.as_mut().unwrap().press_button(1418));
        for _ in 0..4 {
            s.pass(&[]);
        }
        assert_eq!(s.app.game.as_ref().unwrap().selected, None);
        assert!(!shown(s, LAYOUT));
    });
}

#[test]
fn the_name_field_renames_the_team_and_the_follow_button_follows_it() {
    with_team("team-name", "", |s| {
        let team = heli(s);
        s.click(s.at(19, 20, 32, 32));
        s.pass(&[]);
        s.app.ui.as_mut().unwrap().set_text(NAME, b"Eagle");
        s.app.recovery_rename();
        let world = &s.app.game.as_ref().unwrap().world;
        let name = |w: &World| {
            w.zoo.entities[w.find(team).unwrap()]
                .common
                .as_ref()
                .unwrap()
                .name
                .clone()
        };
        assert_eq!(name(world), "Eagle");
        assert!(!s.app.following());
        s.app
            .ui
            .as_mut()
            .unwrap()
            .set_down(crate::panel::recovery::FOLLOW, true);
        assert!(s.app.following());
    });
}

#[test]
fn the_fire_button_removes_the_team() {
    with_team("team-fire", "[UI]\nconfirmSell=0\n", |s| {
        let team = heli(s);
        s.click(s.at(19, 20, 32, 32));
        s.pass(&[]);
        assert!(s.app.ui.as_mut().unwrap().press_button(FIRE));
        s.pass(&[]);
        let game = s.app.game.as_ref().unwrap();
        assert!(game.world.find(team).is_none());
        assert_eq!(game.selected, None);
    });
}

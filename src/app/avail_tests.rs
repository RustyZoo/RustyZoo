//! The start of a scenario: the triggers set the availability, then the ini rewards come last.

use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::menu::Start;
use crate::render::tests::{cache_install_with, frame_file, tile_with};
use crate::vfs::Vfs;

const SIZE: (u32, u32) = (64, 48);

pub(super) fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    for (name, id) in [("lion", 5), ("zebra", 6)] {
        cat.push(EntityType {
            class: "animals",
            type_name: name.as_bytes().to_vec(),
            subtype: b"m".to_vec(),
            file: None,
            config: None,
            name_id: id,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

/// Starts the scenario `text` with the reward of the name id 5 and tells the two flags.
fn flags(name: &str, text: &str) -> [bool; 2] {
    let files = [("s/a.scn", text.as_bytes().to_vec())];
    let install = cache_install_with(name, frame_file(), &files);
    let mut vfs = Vfs::open(&install.0).unwrap();
    let ini = Config::parse(b"[user]\nunlockCount=1\nunlockEntity0=5\n").unwrap();
    vfs.overlay_ini(&ini).unwrap();
    let cat = catalogue();
    let mut app = App::empty(
        MapRenderer::new(&vfs, &cat),
        Timers::new(66, 16, 200),
        SIZE,
        Settings::default(),
    )
    .unwrap();
    app.open_game(tile_with(&[]), Money::default()).unwrap();
    app.begin(&Start {
        path: "scenario.zoo".into(),
        label: "scenario".into(),
        forget_last: true,
        freeform: false,
        cash: Default::default(),
        reset: false,
        reset_fences: false,
        scenario: Some("s/a.scn".into()),
    });
    let build = &app.game.as_ref().unwrap().build;
    [0, 1].map(|i| build.is_available(&cat, i))
}

#[test]
fn a_hide_all_start_trigger_does_not_undo_an_ini_reward() {
    let hide = |truleb: i32| format!("[start]\ntriggers=t\n[t]\ntrulea=4\ntruleb={truleb}\n");
    let one = |id: i32| format!("{}targa={id}\n", hide(1));
    // Both types start available; the reward is the lion's.
    assert_eq!(
        flags("avail-none", "[duration]\nnummonths=-1\n"),
        [true, true]
    );
    assert_eq!(flags("avail-all", &hide(9)), [true, false]);
    assert_eq!(flags("avail-animals", &hide(7)), [true, false]);
    assert_eq!(
        flags("avail-one", &hide(1)),
        [true, true],
        "name id 0 matches neither"
    );
    assert_eq!(flags("avail-6", &one(6)), [true, false]);
    assert_eq!(flags("avail-5", &one(5)), [true, true], "the reward wins");
}

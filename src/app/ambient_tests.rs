//! The ambient one-shots of the soundscape (sound 8.1) reach the player placed in the view.

use super::developer_tests::with_world;
use super::*;
use crate::entity::Catalogue;

const WORLD: &[u8] = b"[ambientlevels]\ngroup = g\nvalue = -100\nvalue = 100\n[g]\nchance = 1\nsound = sounds/bird.wav\nprob = 100\n";

#[test]
fn a_one_shot_is_placed_at_its_offset_from_the_centre_of_the_view() {
    with_world(
        "ambient",
        (false, false),
        (Catalogue::default(), &[]),
        |app, _| {
            let read = |name: &str| (name == "worldsnd.cfg").then(|| WORLD.to_vec());
            app.sim
                .set_soundscape(Soundscape::from_files(&read, &|_| true, None));
            let mut now = 0;
            let mut sounds = Vec::new();
            while sounds.is_empty() && now < 20_000 {
                now += 66;
                app.pass(now, &[]);
                sounds = app.unit_sounds().to_vec();
            }
            let s = sounds.first().expect("a bird within 20 s");
            let centre = app.game.as_ref().unwrap().listener();
            let (dx, dy) = (s.at.0 - centre.0, s.at.1 - centre.1);
            assert!(dx.abs() < 400 && dy.abs() < 300, "{dx},{dy}");
            assert_eq!((s.id, s.file.as_str(), s.base), (0, "sounds/bird.wav", 0));
            assert_eq!(s.voice, "ambient 0 sounds/bird.wav");
            assert_eq!((s.distance, s.pan), positional(s.at, centre));
            // Paused, the simulation draws nothing, so nothing plays.
            app.pass(now + 66, &[Command::Pause]);
            assert!(app.unit_sounds().is_empty());
        },
    );
}

#[test]
fn each_entry_of_a_group_is_its_own_voice() {
    let twice: &[u8] = b"[ambientlevels]\ngroup = g\nvalue = -100\nvalue = 100\n[g]\nchance = 1\n\
        sound = a.wav\nprob = 50\nsound = a.wav\nprob = 50\n";
    with_world(
        "ambient_voices",
        (false, false),
        (Catalogue::default(), &[]),
        |app, _| {
            let read = |name: &str| (name == "worldsnd.cfg").then(|| twice.to_vec());
            app.sim
                .set_soundscape(Soundscape::from_files(&read, &|_| true, None));
            let mut voices = std::collections::BTreeSet::new();
            let mut now = 0;
            while voices.len() < 2 && now < 600_000 {
                now += 66;
                app.pass(now, &[]);
                for s in app.unit_sounds() {
                    assert_eq!(s.file, "a.wav");
                    voices.insert(s.voice.clone());
                }
            }
            // One file listed twice: two voices, the same two whatever the number of shots.
            assert_eq!(voices.len(), 2, "{voices:?}");
        },
    );
}

#[test]
fn the_world_loop_and_the_crowd_loop_are_asked_for_from_the_first_pass() {
    let files = |name: &str| -> Option<Vec<u8>> {
        Some(match name {
            "worldlp.cfg" => b"[worldsound]\nname=sounds/forest.wav\natten=2000\n".to_vec(),
            "crowd.cfg" => b"[crowdsound]\nquiet=sounds/crowds.wav\n".to_vec(),
            _ => return None,
        })
    };
    with_world(
        "scene_loops",
        (false, false),
        (Catalogue::default(), &[]),
        |app, _| {
            assert!(app.scene_loops().is_empty());
            app.sim
                .set_soundscape(Soundscape::from_files(&files, &|_| true, None));
            let mut now = 0;
            // The world loop is there from the start of the game, the crowd loop from the first step.
            assert_eq!(app.scene_loops().len(), 1);
            while app.scene_loops().len() < 2 && now < 2000 {
                now += 66;
                app.pass(now, &[]);
            }
            let loops: Vec<_> = (app.scene_loops().into_iter())
                .map(|l| (l.file, l.terms))
                .collect();
            // No guests: the quiet loop, at its base term and the whole fade term.
            let want = [("sounds/forest.wav", 2000), ("sounds/crowds.wav", 6000)];
            assert_eq!(loops, want.map(|(f, t)| (f.to_string(), t)));
            // Paused, the fade does not advance.
            app.pass(now + 10_000, &[Command::Pause]);
            assert_eq!(app.scene_loops()[1].terms, 6000);
            // A game without a soundscape has no loops.
            app.sim.start_game(16, Default::default());
            assert!(app.scene_loops().is_empty());
        },
    );
}

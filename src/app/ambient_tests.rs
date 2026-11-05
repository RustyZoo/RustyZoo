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
            app.sim.set_soundscape(Soundscape::from_files(&read, None));
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
            assert_eq!(s.voice, "ambient sounds/bird.wav");
            assert_eq!((s.distance, s.pan), positional(s.at, centre));
            // Paused, the simulation draws nothing, so nothing plays.
            app.pass(now + 66, &[Command::Pause]);
            assert!(app.unit_sounds().is_empty());
        },
    );
}

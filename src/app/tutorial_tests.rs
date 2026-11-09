//! The steps of a tutorial, on the made-up interface of the start tests: modal popups hold the pass
//! (scenarios 8.5), the hint (8.3), the end dialog after the last popup (10.6).

use super::scenario_tests::play;
use super::start_tests::*;
use super::*;
use crate::ui::layout::DISABLED;

/// Three goals that hold at the first pass (a fourth never does): the first two show a modal popup (the second with a
/// hint of -1), the third enables the button the start disabled and has the line the hint takes.
const STEPS: &[u8] = b"[desc]\nname=9001\nstory=SCN_STORY\n[start]\ntriggers=off\n\
    [off]\ntrulea=2\ntruleb=3\ntarga=2001\n[goals]\ngoal=a\ngoal=b\ngoal=c\ngoal=d\n\
    [a]\nrulea=99\narga=1\nvalue=1\ntrulea=2\ntruleb=0\ntarga=8\n\
    [b]\nrulea=99\narga=1\nvalue=1\ntrulea=2\ntruleb=0\ntarga=9\n\
    [c]\nrulea=99\narga=5\nvalue=3\ntext=9102\ntrulea=2\ntruleb=5\ntarga=2001\n\
    [d]\nrulea=99\narga=0\nvalue=1\n[8]\nlayout=45000\nshortText1=151\nmodal=1\n\
    [9]\nlayout=45000\nshortText1=153\nmodal=1\nimage1=s/pic.tga\nimage2=s/none.tga\n\
    persistentTextLayout=45400\npersistentShortText=-1\n";

/// The passes of a test: each runs at the next clock reading and returns the lines of the log.
struct Passes(u64);

impl Passes {
    fn run(&mut self, app: &mut App, commands: &[Command]) -> Vec<String> {
        self.0 += 1;
        app.pass(self.0 * 66, commands);
        let lead = format!("pass {} t={} ", self.0, self.0 * 66);
        let lines = app.log_lines(self.0, self.0 * 66);
        lines
            .iter()
            .filter_map(|l| l.strip_prefix(&lead))
            .map(String::from)
            .collect()
    }

    /// Runs until a question is open (at most 60 passes); the lines of that pass.
    fn until_asked(&mut self, app: &mut App) -> Vec<String> {
        for _ in 0..60 {
            let lines = self.run(app, &[]);
            if app.interface().unwrap().modal_open() {
                return lines;
            }
        }
        panic!("no question")
    }

    /// Answers the open question with OK and runs the pass that sees the answer.
    fn answer(&mut self, app: &mut App) -> Vec<String> {
        app.interface().unwrap().answer(1);
        self.run(app, &[])
    }
}

fn count(lines: &[String], what: &str) -> usize {
    lines.iter().filter(|l| l.contains(what)).count()
}

fn disabled(app: &mut App, id: i32) -> bool {
    let ui = app.interface().unwrap();
    ui.tree.elements[ui.tree.find(id).unwrap()].state & DISABLED != 0
}

#[test]
fn modal_popups_of_one_pass_come_one_after_the_other_and_the_end_dialog_after_the_last() {
    let files = [("s/steps.scn", STEPS.to_vec())];
    with_game("steps", "", &files, None, |app, _| {
        let mut passes = Passes(play(app, "s/steps.scn") / 66);
        // The first popup opens in the pass of the triggers, which stand still after it.
        let first = passes.until_asked(app);
        assert_eq!(count(&first, "sim scenario trigger a 1 2 0 8 0"), 1);
        assert_eq!(count(&first, "sim scenario popup 45000 modal"), 1);
        // The triggers of the pass are logged in the order of the file.
        let triggers: Vec<&str> = (first.iter())
            .filter_map(|l| l.strip_prefix("sim scenario trigger "))
            .map(|l| &l[..1])
            .collect();
        assert_eq!(triggers, ["a", "b", "c"]);
        for _ in 0..20 {
            let lines = passes.run(app, &[]);
            assert_eq!(count(&lines, "sim step"), 0, "{lines:?}");
            assert_eq!(count(&lines, "scenario"), 0, "{lines:?}");
        }
        assert!(app.interface().unwrap().text(45006) == Some(b"text-151".to_vec()));
        // The answer lets the same pass go on to the second goal: its popup opens, nothing else.
        let second = passes.answer(app);
        assert_eq!(count(&second, "sim scenario popup 45000 modal"), 1);
        assert_eq!(count(&second, "sim step"), 0, "no step between the popups");
        assert_eq!(count(&second, "ui answer 2"), 0, "no popup is cancelled");
        assert!(app.interface().unwrap().modal_open());
        assert!(app.interface().unwrap().text(45006) == Some(b"text-153".to_vec()));
        // The third goal and the hint wait for the answer.
        assert!(disabled(app, 2001) && !shown(app, 45400));
        // The last answer: the third goal acts, the hint comes with the line the goal in progress
        // had at the second trigger.
        let last = passes.answer(app);
        assert_eq!(
            count(&last, "sim scenario element 2001 enable"),
            1,
            "{last:?}"
        );
        assert_eq!(count(&last, "sim scenario hint 45400"), 1);
        assert!(!disabled(app, 2001) && shown(app, 45400));
        assert_eq!(
            app.interface().unwrap().text(45406),
            Some(b"hint 3 lion".to_vec())
        );
        assert!(!app.interface().unwrap().modal_open() && !shown(app, 45200));
        assert_eq!(count(&last, "ui answer 2"), 0);
    });
}

#[test]
fn a_win_whose_last_step_is_a_modal_popup_asks_yes_or_no_once_it_is_answered() {
    let scn = b"[start]\ntriggers=t\n[t]\ntrulea=2\n[goals]\ngoal=a\n\
        [a]\nrulea=99\narga=1\nvalue=1\ntrulea=2\ntruleb=0\ntarga=8\n\
        [8]\nlayout=45000\nshortText1=151\nmodal=1\n";
    let files = [("s/last.scn", scn.to_vec())];
    with_game("last-popup", "", &files, None, |app, _| {
        let mut passes = Passes(play(app, "s/last.scn") / 66);
        passes.until_asked(app);
        assert!(shown(app, 45000) && !shown(app, 45200));
        let lines = passes.answer(app);
        assert!(shown(app, 45200) && app.interface().unwrap().modal_open());
        assert_eq!(count(&lines, "menu scenario dialog"), 1, "{lines:?}");
        assert_eq!(count(&passes.run(app, &[]), "ui answer 2"), 0);
        // No is possible and stops the game.
        app.interface().unwrap().answer(7);
        passes.run(app, &[]);
        assert!(app.camera().is_none());
    });
}

#[test]
fn the_picture_of_the_first_image_goes_on_both_image_elements_and_the_second_is_ignored() {
    // The picture s/pic.tga of the install is 2 pixels wide.
    let files = [("s/steps.scn", STEPS.to_vec())];
    with_game("images", "", &files, None, |app, _| {
        let mut passes = Passes(play(app, "s/steps.scn") / 66);
        passes.until_asked(app);
        passes.answer(app);
        let ui = app.interface().unwrap();
        for id in [45004, 45005] {
            assert!(ui.shown(id), "{id}");
            assert_eq!(ui.tree.rect(ui.tree.find(id).unwrap()).2, 2, "{id}");
        }
    });
}

#[test]
fn the_line_of_the_hint_shows_the_popup_again_with_the_goals_as_they_are_then() {
    // Goal b is in progress at the popup, then holds when the value rises; the line follows it.
    let scn = b"[start]\ntriggers=t\n[t]\ntrulea=2\ntruleb=0\ntarga=9\n[goals]\ngoal=a\ngoal=b\n\
        [a]\nrulea=99\narga=5\nvalue=1\ntext=9102\n\
        [b]\nrulea=99\narga=6\nvalue=1\ntype=1\ntext=9102\n\
        [9]\nlayout=45000\nshortText1=153\nmodal=1\npersistentTextLayout=45400\n\
        persistentShortText=-1\n";
    let files = [("s/replay.scn", scn.to_vec())];
    with_game("replay", "", &files, None, |app, _| {
        let mut passes = Passes(play(app, "s/replay.scn") / 66);
        passes.run(app, &[]);
        assert!(app.interface().unwrap().modal_open());
        passes.answer(app);
        let hint = |app: &mut App| app.interface().unwrap().text(45406);
        assert_eq!(hint(app), Some(b"hint 1 lion".to_vec()));
        // Goal a holds at its first pass, so the line is b's now: the name of 6 is empty.
        for _ in 0..30 {
            passes.run(app, &[]);
        }
        let again = |app: &mut App, passes: &mut Passes| {
            passes.run(app, &click(3, 3));
            assert!(app.interface().unwrap().modal_open() && !shown(app, 45400));
            passes.answer(app);
            assert!(shown(app, 45400));
        };
        again(app, &mut passes);
        assert_eq!(hint(app), Some(b"hint 1 ".to_vec()));
    });
}

#[test]
fn the_simulation_steps_in_the_frame_after_a_start_whose_triggers_left_effects() {
    let files = [("s/steps.scn", STEPS.to_vec())];
    with_game("start-step", "", &files, None, |app, _| {
        // The frame of the start (pass 1) leaves the effects of the start triggers; its step runs.
        let at = play(app, "s/steps.scn");
        let start = app.log_lines(1, at);
        assert_eq!(count(&start, "sim step"), 1, "{start:?}");
        assert!(disabled(app, 2001));
    });
}

#[test]
fn a_press_of_a_zoom_button_reaches_the_goal_that_asks_for_it() {
    // A goal on the zoom-in button, asked about for a pass before the press (8.2): the press is
    // seen at the next scenario pass and its popup opens.
    let scn = b"[start]\ntriggers=t\n[t]\ntrulea=2\n[goals]\ngoal=z\n\
        [z]\nrulea=10\nruleb=1\narga=1007\nvalue=1\ntrulea=2\ntruleb=0\ntarga=8\n\
        [8]\nlayout=45000\nshortText1=151\nmodal=1\n";
    let files = [("s/zoom.scn", scn.to_vec())];
    with_game("zoom-goal", "", &files, None, |app, _| {
        let mut passes = Passes(play(app, "s/zoom.scn") / 66);
        for _ in 0..30 {
            passes.run(app, &[]);
        }
        assert!(!app.interface().unwrap().modal_open());
        let zoom = app.game.as_ref().unwrap().camera.zoom;
        passes.run(app, &[Command::Press(1007)]);
        assert_ne!(app.game.as_ref().unwrap().camera.zoom, zoom);
        passes.until_asked(app);
        assert!(shown(app, 45000));
    });
}

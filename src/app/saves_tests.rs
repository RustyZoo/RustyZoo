//! Save Game, the Yes of the questions and the list of saved games, on the made-up interface of
//! the start tests.

use super::exhibits::NAME_FIELD;
use super::start_tests::{CONTINUE, SIZE, click, interface, key, shown, started, with_menu};
use super::*;
use crate::entity::Catalogue;
use crate::render::MapRenderer;
use crate::render::tests::{cache_install, frame_file};
use crate::sprite::tests::Install;
use crate::vfs::Vfs;
use crate::zoo::Zoo;

const INI: &str = "[user]\nlastfile=saved.zoo\n";
/// The Save Game and Load Game buttons of the options panel, the load button of the main menu, the
/// first two rows of the list, and its Load and Back buttons.
const SAVE: (i32, i32) = (42, 32);
const LOAD_GAME: (i32, i32) = (52, 22);
const LOAD_SAVED: (i32, i32) = (45, 5);
const ROW_1: (i32, i32) = (5, 14);
const ROW_2: (i32, i32) = (5, 26);
const PLAY: (i32, i32) = (46, 14);
const BACK: (i32, i32) = (46, 26);

fn chars(text: &str) -> Vec<Command> {
    text.bytes().map(Command::Char).collect()
}

fn enter() -> Command {
    Command::Key(Key::Enter, false, false)
}

/// The `menu` lines of the last pass.
fn said(app: &App) -> Vec<String> {
    let lead = "pass 0 t=0 menu ";
    let lines = app.log_lines(0, 0);
    lines
        .iter()
        .filter_map(|l| l.strip_prefix(lead))
        .map(String::from)
        .collect()
}

/// One more pass at the clock after `now`, with `commands`; returns the new clock.
fn pass(app: &mut App, now: u64, commands: &[Command]) -> u64 {
    app.pass(now + 66, commands);
    now + 66
}

/// Opens the options panel and presses Save Game.
fn press_save(app: &mut App, now: u64) -> u64 {
    app.interface().unwrap().show(5);
    pass(app, now, &click(SAVE.0, SAVE.1))
}

/// The save dialog is open: types `name` over the one in the field and presses Enter.
fn type_name(app: &mut App, now: u64, name: &str) -> u64 {
    assert!(app.interface().unwrap().modal_open(), "the dialog is open");
    let mut typed = chars(name);
    typed.push(enter());
    pass(app, now, &typed)
}

fn last(files: &Install) -> String {
    std::fs::read_to_string(files.0.join("user/zoo.ini")).unwrap_or_default()
}

fn zoo_files(files: &Install) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(&files.0)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().ends_with(".zoo") || n.ends_with(".tmp"))
        .collect();
    names.sort();
    names
}

#[test]
fn the_save_button_asks_a_name_writes_the_file_and_makes_it_the_last_file() {
    with_menu("save", INI, |app, files| {
        let now = started(app);
        let now = press_save(app, now);
        assert!(
            app.paused(),
            "the game stands still while the dialog is open"
        );
        let ui = app.interface().unwrap();
        assert_eq!(ui.text(45308), Some(b"saved.zoo".to_vec()), "the last file");
        assert_eq!(said(app), ["game save dialog"]);
        pass(app, now, &chars("zeta"));
        let now = pass(app, now, &[enter()]);
        let path = files.0.join("zeta.zoo");
        let bytes = std::fs::read(&path).expect("zeta.zoo is written");
        assert!(crate::zoo::Zoo::parse_money(&bytes).is_ok());
        assert_eq!(zoo_files(files), ["bad.zoo", "saved.zoo", "zeta.zoo"]);
        assert!(!app.paused() && !shown(app, 5), "the panel is hidden");
        assert_eq!(said(app), [format!("game saved {}", path.display())]);
        let want = format!("[user]\nlastfile={}\n", path.display());
        assert_eq!(last(files), want, "an absolute path");
        // The next dialog starts from the new last file.
        let now = press_save(app, now);
        let _ = now;
        assert_eq!(
            app.interface().unwrap().text(45308),
            Some(b"zeta.zoo".to_vec())
        );
    });
}

#[test]
fn cancel_and_a_blank_name_write_nothing_and_a_name_gets_zoo_unless_it_ends_exactly_in_it() {
    with_menu("names", INI, |app, files| {
        let mut now = started(app);
        now = press_save(app, now);
        now = pass(app, now, &key(Key::Escape));
        assert!(shown(app, 5) && !app.interface().unwrap().modal_open());
        assert!(last(files).is_empty());
        now = press_save(app, now);
        now = type_name(app, now, "  ");
        assert!(last(files).is_empty() && zoo_files(files) == ["bad.zoo", "saved.zoo"]);
        // Windows' dialog would take `W.ZOO` as it is; the original writes `W.ZOO.zoo`.
        now = press_save(app, now);
        now = type_name(app, now, "W.ZOO");
        now = press_save(app, now);
        now = type_name(app, now, "x.zoo");
        let _ = now;
        assert_eq!(
            zoo_files(files),
            ["W.ZOO.zoo", "bad.zoo", "saved.zoo", "x.zoo"]
        );
    });
}

#[test]
fn a_name_of_more_than_256_bytes_is_refused_with_its_message() {
    with_menu("long", INI, |app, files| {
        let mut now = started(app);
        for (len, message) in [(257, true), (256, false), (200, false)] {
            now = press_save(app, now);
            now = pass(app, now, &[]);
            let name = "n".repeat(len);
            app.interface().unwrap().set_text(45308, name.as_bytes());
            now = pass(app, now, &[enter()]);
            let told = said(app).contains(&"message text-11646".to_string());
            assert_eq!(told, message, "{len}");
            // The message box (or, on a system that cannot take 256 letters, the failure) is
            // answered; 257 wrote nothing.
            if app.interface().unwrap().modal_open() {
                now = pass(app, now, &[enter()]);
            }
            let there = files.0.join(format!("{name}.zoo")).is_file();
            assert!(!(message && there), "{len}");
            assert_eq!(there, len == 200, "{len}");
            if len == 200 {
                assert!(last(files).contains(&name));
            }
        }
    });
}

#[test]
fn a_failed_write_shows_a_message_and_keeps_the_old_file_and_the_last_file() {
    with_menu("fail", INI, |app, files| {
        let mut now = started(app);
        // A folder where the file should go: the rename fails, and the aside file is removed.
        std::fs::create_dir(files.0.join("d.zoo")).unwrap();
        now = press_save(app, now);
        now = type_name(app, now, "d");
        assert_eq!(said(app), ["message text-11645"]);
        assert!(files.0.join("d.zoo").is_dir() && !files.0.join("d.zoo.tmp").exists());
        assert!(last(files).is_empty(), "lastfile is not touched");
        assert!(shown(app, 5), "the panel stays");
        // So does a folder that is not there.
        now = pass(app, now, &[enter()]);
        now = press_save(app, now);
        now = type_name(app, now, "no/such");
        assert_eq!(said(app), ["message text-11645"]);
        assert!(!files.0.join("no").exists() && last(files).is_empty());
        let _ = now;
        assert!(app.camera().is_some(), "the run goes on");
    });
}

#[test]
fn yes_saves_then_goes_on_and_a_cancelled_save_ends_the_command() {
    with_menu("yes", INI, |app, files| {
        let mut now = started(app);
        now = pass(app, now, &key(Key::Escape));
        now = pass(app, now, &key(Key::Y));
        // Cancel in the dialog: nothing else happens.
        now = pass(app, now, &key(Key::Escape));
        assert!(app.camera().is_some() && !app.interface().unwrap().modal_open());
        // Yes and OK: the file is written over, then the game stops for the menu.
        now = pass(app, now, &key(Key::Escape));
        now = pass(app, now, &key(Key::Y));
        let before = std::fs::metadata(files.0.join("saved.zoo")).unwrap().len();
        now = pass(app, now, &[enter()]);
        // The file exists: its replacement is confirmed.
        now = pass(app, now, &key(Key::Y));
        assert!(app.camera().is_none() && shown(app, 26));
        let after = std::fs::metadata(files.0.join("saved.zoo")).unwrap().len();
        assert_ne!(before, after, "saved.zoo was written");
        assert!(last(files).contains("saved.zoo"));
        // Close asks the same way and ends the run after the save.
        now = pass(app, now, &CONTINUE);
        now = pass(app, now, &[Command::Close]);
        now = pass(app, now, &key(Key::Y));
        assert!(!app.quit(), "the dialog is open");
        now = pass(app, now, &[enter()]);
        // The replacement is confirmed, then the run ends.
        pass(app, now, &key(Key::Y));
        assert!(app.quit());
    });
}

#[test]
fn a_disabled_save_button_stops_ctrl_s_and_the_questions() {
    with_menu("off", INI, |app, files| {
        let mut now = started(app);
        let ui = app.interface().unwrap();
        let button = ui.tree.find(1502).unwrap();
        ui.tree.elements[button].state |= crate::ui::layout::DISABLED;
        now = pass(app, now, &[Command::Key(Key::S, true, false)]);
        assert!(
            !app.interface().unwrap().modal_open(),
            "Ctrl+S does nothing"
        );
        // Escape goes to the menu without a question.
        pass(app, now, &key(Key::Escape));
        assert!(app.camera().is_none() && shown(app, 26));
        assert!(!files.0.join("user").exists());
    });
}

#[test]
fn ctrl_s_saves_and_ctrl_l_asks_then_lists_the_saved_games_by_name() {
    with_menu("ctrl", INI, |app, files| {
        let mut now = started(app);
        now = pass(app, now, &[Command::Key(Key::S, true, false)]);
        assert!(app.interface().unwrap().modal_open());
        now = pass(app, now, &key(Key::Escape));
        // Ctrl+L asks, No lists the `.zoo` files of the last file's folder by name.
        std::fs::write(files.0.join("A.ZOO"), super::start_tests::map()).unwrap();
        std::fs::write(files.0.join("note.txt"), b"x").unwrap();
        for name in ["m.zoo", "c.zoo", "x.zoo", "B.zoo"] {
            std::fs::write(files.0.join(name), super::start_tests::map()).unwrap();
        }
        now = pass(app, now, &[Command::Key(Key::L, true, false)]);
        assert_eq!(said(app), ["game ask load"]);
        now = pass(app, now, &key(Key::N));
        assert_eq!(said(app), ["game load list 7"]);
        assert!(shown(app, 39) && app.paused());
        let ui = app.interface().unwrap();
        let list = ui.tree.find(11504).unwrap();
        let rows = match &ui.tree.elements[list].kind {
            crate::ui::layout::Kind::ListBox(l) => l.rows.rows.clone(),
            _ => panic!("no list"),
        };
        let names: Vec<_> = rows.iter().map(|r| r.text.clone()).collect();
        let want = [
            "A.ZOO",
            "B.zoo",
            "bad.zoo",
            "c.zoo",
            "m.zoo",
            "saved.zoo",
            "x.zoo",
        ];
        assert_eq!(names, want.map(|n| n.as_bytes().to_vec()));
        let _ = now;
    });
}

#[test]
fn load_game_asks_lists_the_files_and_loading_one_opens_it_and_closes_the_screens() {
    with_menu("load", INI, |app, files| {
        let mut now = started(app);
        app.interface().unwrap().show(5);
        now = pass(app, now, &click(LOAD_GAME.0, LOAD_GAME.1));
        assert_eq!(said(app), ["game ask load"]);
        now = pass(app, now, &key(Key::N));
        assert_eq!(said(app), ["game load list 2"]);
        assert!(shown(app, 39) && app.paused());
        let ui = app.interface().unwrap();
        assert_eq!(ui.text(11503), Some(b"".to_vec()));
        // Load is off until a row is selected.
        now = pass(app, now, &click(PLAY.0, PLAY.1));
        assert!(shown(app, 39) && said(app).is_empty());
        // A file the loader refuses: its message, and the list stays.
        now = pass(app, now, &click(ROW_1.0, ROW_1.1));
        now = pass(app, now, &click(PLAY.0, PLAY.1));
        assert_eq!(said(app).len(), 1, "{:?}", said(app));
        assert!(said(app)[0].starts_with("message ") && said(app)[0].contains("bad.zoo"));
        assert!(shown(app, 39) && last(files).is_empty());
        now = pass(app, now, &[enter()]);
        // Escape closes the list and the game goes on.
        now = pass(app, now, &key(Key::Escape));
        assert!(!shown(app, 39) && !app.paused() && shown(app, 5));
        // Ctrl+L, No, the second row, Load: the zoo opens.
        now = pass(app, now, &[Command::Key(Key::L, true, false)]);
        now = pass(app, now, &key(Key::N));
        now = pass(app, now, &click(ROW_2.0, ROW_2.1));
        now = pass(app, now, &click(PLAY.0, PLAY.1));
        let path = files.0.join("saved.zoo");
        assert_eq!(said(app), [format!("game start load {}", path.display())]);
        assert!(!shown(app, 39) && !shown(app, 5) && !app.paused());
        assert_eq!(
            last(files),
            format!("[user]\nlastfile={}\n", path.display())
        );
        // Back closes the list too.
        now = pass(app, now, &[Command::Key(Key::L, true, false)]);
        now = pass(app, now, &key(Key::N));
        pass(app, now, &click(BACK.0, BACK.1));
        assert!(!shown(app, 39) && !app.paused());
    });
}

#[test]
fn load_saved_game_in_the_menu_lists_the_files_and_starts_the_one_chosen() {
    with_menu("menu-load", INI, |app, files| {
        app.pass(0, &[]);
        let mut now = pass(app, 0, &click(LOAD_SAVED.0, LOAD_SAVED.1));
        assert_eq!(said(app), ["game load list 2"]);
        assert!(shown(app, 39) && app.camera().is_none());
        now = pass(app, now, &click(ROW_2.0, ROW_2.1));
        pass(app, now, &click(PLAY.0, PLAY.1));
        let path = files.0.join("saved.zoo");
        assert_eq!(said(app), [format!("game start load {}", path.display())]);
        assert!(app.camera().is_some() && !shown(app, 39) && !shown(app, 26));
        assert_eq!(
            last(files),
            format!("[user]\nlastfile={}\n", path.display())
        );
    });
}

/// Runs `body` on a game started without the menu, as `rz run` starts one, with the user folder
/// `files/user` and the interface of the start tests.
fn with_game(name: &str, body: impl FnOnce(&mut App, &Install)) {
    let files = Install::new(&format!("saves-game-{name}"), &[]);
    let game = cache_install(&format!("saves-game-install-{name}"), frame_file());
    let vfs = Vfs::open(&game.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = Zoo::parse(&super::start_tests::map()).unwrap();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        zoo,
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.set_user_folder(Some(files.0.join("user")));
    app.set_interface(interface(&files));
    body(&mut app, &files);
}

#[test]
fn without_the_menu_a_save_makes_the_last_file_and_the_next_dialog_starts_from_it() {
    with_game("last", |app, files| {
        let mut now = pass(app, 0, &[]);
        // The game was opened from a file, so the dialog starts in its folder.
        app.last_file = Some(files.0.join("start.zoo"));
        now = pass(app, now, &[Command::Key(Key::S, true, false)]);
        now = type_name(app, now, "made");
        assert!(files.0.join("made.zoo").is_file());
        assert!(last(files).contains(&format!("lastfile={}", files.0.join("made.zoo").display())));
        now = pass(app, now, &[Command::Key(Key::S, true, false)]);
        let field = app.interface().unwrap().text(NAME_FIELD);
        assert_eq!(field.as_deref(), Some(&b"made.zoo"[..]));
        let _ = now;
    });
}

#[test]
fn ctrl_s_and_ctrl_l_close_the_open_panels_and_clear_the_selection_before_the_dialog_and_the_question()
 {
    with_game("panels", |app, _| {
        let mut now = pass(app, 0, &[]);
        app.game.as_mut().unwrap().selected = Some(1);
        app.interface().unwrap().show(5);
        now = pass(app, now, &[Command::Key(Key::S, true, false)]);
        assert!(app.interface().unwrap().modal_open());
        assert!(!shown(app, 5), "the options panel closes");
        assert_eq!(app.game.as_ref().unwrap().selected, None);
        now = pass(app, now, &key(Key::Escape));
        app.interface().unwrap().show(5);
        now = pass(app, now, &[Command::Key(Key::L, true, false)]);
        assert_eq!(said(app), ["game ask load"]);
        assert!(
            !shown(app, 5),
            "the options panel closes before the question"
        );
        let _ = now;
    });
}

#[test]
fn the_name_takes_up_to_256_bytes_and_is_not_cut_at_the_width_of_the_field() {
    with_menu("wide", INI, |app, files| {
        let now = started(app);
        let now = press_save(app, now);
        let name = "w".repeat(30);
        let _ = type_name(app, now, &name);
        assert!(files.0.join(format!("{name}.zoo")).is_file());
    });
}

#[test]
fn loading_a_saved_game_from_the_list_is_a_busy_pass() {
    with_menu("busy-load", INI, |app, _| {
        let mut now = started(app);
        let told = std::rc::Rc::new(std::cell::Cell::new(0));
        let hook = told.clone();
        app.on_busy(move || hook.set(hook.get() + 1));
        // Opening the list and choosing a row load nothing.
        for commands in [
            [Command::Key(Key::L, true, false)].as_slice(),
            &key(Key::N),
            &click(ROW_2.0, ROW_2.1),
        ] {
            now = pass(app, now, commands);
            assert!(!app.busy());
        }
        assert_eq!(told.get(), 0);
        // The Load button in play loads: the hook is told once, and the pass is busy.
        now = pass(app, now, &click(PLAY.0, PLAY.1));
        assert!(app.busy() && told.get() == 1);
        assert!(!shown(app, 39), "the file was loaded");
        // The next pass is not.
        now = pass(app, now, &[]);
        assert!(!app.busy());
        // From the main menu, through the list, too.
        pass(app, now, &[Command::Key(Key::Escape, false, false)]);
    });
    with_menu("busy-load-menu", INI, |app, _| {
        app.pass(0, &[]);
        let mut now = pass(app, 0, &click(LOAD_SAVED.0, LOAD_SAVED.1));
        now = pass(app, now, &click(ROW_2.0, ROW_2.1));
        assert!(!app.busy());
        pass(app, now, &click(PLAY.0, PLAY.1));
        assert!(app.busy() && app.camera().is_some());
    });
}

#[test]
fn a_file_that_exists_is_confirmed_before_it_is_replaced() {
    with_menu("replace", INI, |app, files| {
        let mut now = started(app);
        let original = std::fs::read(files.0.join("saved.zoo")).unwrap();
        now = press_save(app, now);
        now = type_name(app, now, "saved");
        assert!(
            app.interface().unwrap().modal_open(),
            "the question is open"
        );
        assert_eq!(std::fs::read(files.0.join("saved.zoo")).unwrap(), original);
        // No writes nothing and returns to the dialog, the name kept.
        now = pass(app, now, &key(Key::N));
        assert_eq!(std::fs::read(files.0.join("saved.zoo")).unwrap(), original);
        assert!(last(files).is_empty() && app.interface().unwrap().modal_open());
        // Enter asks again; Yes writes the file over.
        now = pass(app, now, &[enter()]);
        assert!(
            app.interface().unwrap().modal_open(),
            "the question is open"
        );
        now = pass(app, now, &key(Key::Y));
        assert_ne!(std::fs::read(files.0.join("saved.zoo")).unwrap(), original);
        assert!(last(files).contains("saved.zoo"));
        let _ = now;
    });
}

#[test]
fn the_question_checks_the_name_as_typed_and_not_the_name_with_zoo_added() {
    with_menu("asked", INI, |app, files| {
        let now = started(app);
        // z.txt is saved as z.txt.zoo; saving z.txt again asks nothing, as z.txt does not exist.
        let now = press_save(app, now);
        let now = type_name(app, now, "z.txt");
        assert!(files.0.join("z.txt.zoo").is_file());
        let now = press_save(app, now);
        let _ = type_name(app, now, "z.txt");
        assert!(!app.interface().unwrap().modal_open(), "no question");
    });
}

#[test]
fn rotating_or_zooming_with_no_game_is_not_busy() {
    with_menu("busy-turn-menu", INI, |app, _| {
        app.pass(0, &[]);
        assert!(app.camera().is_none());
        let now = pass(app, 0, &[Command::Rotate(true), Command::Zoom(false)]);
        assert!(!app.busy());
        let _ = now;
    });
    with_game("busy-turn", |app, _| {
        let mut now = pass(app, 0, &[]);
        for command in [Command::Rotate(false), Command::Zoom(true)] {
            now = pass(app, now, &[command]);
            assert!(app.busy(), "{command}");
            now = pass(app, now, &[]);
            assert!(!app.busy());
        }
    });
}

#[test]
fn a_name_with_an_extension_is_asked_as_typed() {
    with_menu("typed", INI, |app, files| {
        let now = started(app);
        std::fs::write(files.0.join("ex.txt"), b"x").unwrap();
        let now = press_save(app, now);
        let now = type_name(app, now, "ex.txt");
        assert!(app.interface().unwrap().modal_open(), "asked about ex.txt");
        let _ = pass(app, now, &key(Key::Y));
        assert!(files.0.join("ex.txt.zoo").is_file());
    });
}

#[test]
fn a_save_is_not_busy() {
    with_game("busy-save", |app, files| {
        let mut now = pass(app, 0, &[]);
        app.last_file = Some(files.0.join("start.zoo"));
        now = pass(app, now, &[Command::Key(Key::S, true, false)]);
        assert!(!app.busy());
        now = type_name(app, now, "made");
        assert!(files.0.join("made.zoo").is_file());
        assert!(!app.busy());
        let file = files.0.join("scripted.zoo");
        let file: &'static str = Box::leak(file.to_string_lossy().into_owned().into_boxed_str());
        pass(app, now, &[Command::Save(file)]);
        assert!(std::path::Path::new(file).is_file() && !app.busy());
    });
}

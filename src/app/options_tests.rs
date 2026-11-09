//! The game options window and the help window through the pass (ui layout 16): start-up values,
//! what each control does and writes, the video check, the restart notice and the help topics, on
//! the made-up map of the bulldozer tests with small layouts.

use super::bulldozer_tests::{ROOT, catalogue, install, map};
use super::{App, Command, Key, Mouse};
use crate::camera::Settings;
use crate::config::Config;
use crate::fixtures::TempDir;
use crate::render::MapRenderer;
use crate::timing::{Timers, ini_whole};
use crate::ui::draw::Strings;
use crate::ui::interface::Interface;
use crate::ui::layout::Tree;
use crate::vfs::Vfs;
use std::path::Path;

const SIZE: (u32, u32) = (800, 600);
/// The row of the sliders: the volume, the music, the scroll speed, the tooltip delay and duration.
const BARS: [(i32, i32); 5] = [(1511, 10), (1513, 40), (1515, 70), (1523, 100), (1525, 130)];

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            11700 => "restart".into(),
            22007 => "too small".into(),
            22008 => "too shallow".into(),
            28101 => "About the game".into(),
            28102 => "Technical Support".into(),
            28103 => "Quick start".into(),
            28120 => "Zebra".into(),
            28121 | 28122 => "Apple".into(),
            28123 => "Mid".into(),
            28124 => "App".into(),
            _ => String::new(),
        }
    }

    fn text_entry(&self, name: &[u8]) -> Vec<u8> {
        match name {
            b"T_ABOUT" => b"Version %s, id %s, %s".to_vec(),
            b"T_ZEBRA" => b"stripes".to_vec(),
            _ => Vec::new(),
        }
    }
}

fn button(id: i32, state: i32, extra: &str) -> String {
    format!("[b{id}]\ntype=UIButton\nid={id}\nx=0\ny=0\ndx=8\ndy=8\nstate={state}\n{extra}")
}

fn set(id: i32, members: &[i32]) -> String {
    let list: String = members.iter().map(|m| format!("button={m}\n")).collect();
    format!("[s{id}]\ntype=UIRadioSet\nid={id}\n{list}")
}

fn bar(id: i32, y: i32) -> String {
    format!("[r{id}]\ntype=UIScrollBar\nid={id}\nx=20\ny={y}\ndx=110\ndy=26\n")
}

fn page(id: i32, body: &str) -> (String, Vec<u8>) {
    let head = format!("[LayoutInfo]\nid={id}\nlayer=7\nx=0\ny=0\ndx=170\ndy=300\n");
    (format!("ui/p{id}.lyt"), (head + body).into_bytes())
}

fn files() -> Vec<(String, Vec<u8>)> {
    let tab =
        |id: i32, state: i32, page: i32| button(id, state, &format!("action=3\ntarget={page}\n"));
    let mut opt = String::from("[LayoutInfo]\nid=5\nlayer=7\nx=0\ny=0\ndx=170\ndy=300\n");
    opt += &set(1505, &[1506, 1507, 1508, 1530]);
    opt += &[
        tab(1506, 4105, 900),
        tab(1507, 4097, 901),
        tab(1508, 4097, 902),
        tab(1530, 4097, 903),
    ]
    .concat();
    opt += &button(1509, 1, "");
    for p in 900..904 {
        opt += &format!("[g{p}]\ntype=UILayout\nlayout=ui/p{p}.lyt\nstate=1\n");
    }
    let sound = [
        set(1580, &[1517, 1518, 1519]),
        set(1589, &[1590, 1591, 1592, 1593]),
        bar(1511, 10),
        bar(1513, 40),
        [1517, 1518, 1519, 1587, 1588, 1590, 1591, 1592, 1593]
            .map(|id| button(id, 4097, ""))
            .concat(),
    ]
    .concat();
    let tips = [
        set(1599, &[1520, 1521]),
        bar(1515, 70),
        bar(1523, 100),
        bar(1525, 130),
    ]
    .concat()
        + &[1520, 1521].map(|id| button(id, 4097, "")).concat();
    let advanced = [
        set(1538, &[1539, 1546]),
        set(1549, &[1551, 1552]),
        set(1554, &[1555, 1556]),
    ]
    .concat()
        + &[1539, 1546, 1551, 1552, 1555, 1556]
            .map(|id| button(id, 4097, ""))
            .concat();
    let help = "[LayoutInfo]\nid=38\nlayer=7\nx=0\ny=0\ndx=200\ndy=100\n\
        [c]\ntype=UIButton\nid=28007\nx=150\ndx=8\ndy=8\naction=2\ntarget=-1\n\
        [l]\ntype=UIListBox\nid=28003\nx=0\ny=0\ndx=60\ndy=40\n\
        [t]\ntype=UIText\nid=28002\nx=0\ny=50\ndx=60\ndy=10\n\
        [x]\ntype=UIText\nid=28005\nx=0\ny=60\ndx=60\ndy=30\n";
    let cfg = |topics: &[&str], sections: &str| {
        let list: String = topics.iter().map(|t| format!("topic={t}\n")).collect();
        format!("[Version]\nversion=1\n[Topics]\n{list}{sections}").into_bytes()
    };
    vec![
        ("ui/opt.lyt".into(), opt.into_bytes()),
        page(900, ""),
        page(901, &sound),
        page(902, &tips),
        page(903, &advanced),
        ("ui/help.lyt".into(), help.as_bytes().to_vec()),
        (
            "help.cfg".into(),
            b"[Topics]\ntopic=Old\n[Old]\ntitle=28120\n".to_vec(),
        ),
        (
            "help00.cfg".into(),
            cfg(
                &["Zebra", "Quick start", "About", "Support", "Apple", "Gone"],
                "[Zebra]\ntitle=28120\ntext=T_ZEBRA\n[Quick start]\ntitle=28103\n[About]\ntitle=28101\n\
                 text=T_ABOUT\n[Support]\ntitle=28102\n[Apple]\ntitle=28121\n[Gone]\ntitle=28122\n",
            ),
        ),
        (
            "help01.cfg".into(),
            cfg(
                &["Mid", "Prefix"],
                "[Mid]\ntitle=28123\n[Prefix]\ntitle=28124\n",
            ),
        ),
    ]
}

struct T<'a> {
    app: App<'a>,
    now: u64,
    /// The log lines of every pass.
    lines: Vec<String>,
}

impl T<'_> {
    fn pass(&mut self, commands: &[Command]) {
        self.now += 20;
        self.app.pass(self.now, commands);
        self.lines
            .extend(self.app.log_lines(self.now / 20, self.now));
    }

    /// Whether a pass told that it showed the notice with this text.
    fn noticed(&self, text: &str) -> bool {
        self.lines
            .iter()
            .any(|l| l.ends_with(&format!("options notice {text}")))
    }

    fn press(&mut self, id: i32) {
        self.pass(&[Command::Press(id)]);
    }

    fn ui(&mut self) -> &mut Interface {
        self.app.interface().unwrap()
    }

    /// A click on the right part of the track of slider `id`, or on its left part.
    fn nudge(&mut self, id: i32, right: bool) {
        let y = BARS.iter().find(|b| b.0 == id).unwrap().1;
        let x = if right { 125 } else { 21 };
        let at = [
            Command::Mouse(Mouse::Move(x, y + 10)),
            Command::Mouse(Mouse::Down),
        ];
        self.pass(&at);
        self.pass(&[Command::Mouse(Mouse::Up)]);
    }

    fn slider(&mut self, id: i32) -> i32 {
        self.ui().slider(id).unwrap()
    }

    fn down(&mut self, id: i32) -> bool {
        self.ui().is_down(id)
    }

    fn open(&mut self, tab: i32) {
        self.press(1004);
        if tab != 1506 {
            self.press(tab);
        }
    }
}

/// What the user folder holds of `section` `key` after the game wrote it.
fn written(dir: &Path, section: &str, key: &str) -> Option<i32> {
    let file = std::fs::read(dir.join("zoo.ini")).unwrap_or_default();
    ini_whole(&Config::parse(&file).unwrap(), section, key)
}

/// A game with the options window (800 x 600, in a window) and the user folder of the test.
fn with_options(name: &str, ini: &str, body: impl FnOnce(&mut T, &Path)) {
    let root = ROOT.to_string()
        + "[o]\ntype=UIButton\nid=1004\nx=0\ny=500\ndx=8\ndy=8\naction=3\ntarget=5\nstate=2048\n\
           [w]\ntype=UILayout\nlayout=ui/opt.lyt\nstate=1\n[hh]\ntype=UILayout\nlayout=ui/help.lyt\nstate=1\n\
           [p]\ntype=UIButton\nid=1071\nx=0\ny=520\ndx=8\ndy=8\n[y]\ntype=UIButton\nid=1072\nx=10\ny=520\ndx=8\ndy=8\nstate=16\n\
           [k]\ntype=UIText\nid=7003\nx=0\ny=540\ndx=10\ndy=10\ntimed=1\nstate=1025\n";
    let files = files();
    let more: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let cat = catalogue();
    let install = install(name, ini, &root, &more);
    let mut vfs = Vfs::open(&install.0).unwrap();
    vfs.overlay_ini(&Config::parse(ini.as_bytes()).unwrap())
        .unwrap();
    let user = TempDir::new(&format!("options-{name}"));
    let mut app = App::new(
        MapRenderer::new(&vfs, &cat),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.set_user_folder(Some(user.0.clone()));
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    app.set_interface(Interface::new(tree, Box::new(Texts)));
    app.interface().unwrap().set_hidden(1004, false);
    let mut t = T {
        app,
        now: 0,
        lines: Vec::new(),
    };
    t.pass(&[]);
    body(&mut t, &user.0);
}

#[test]
fn the_window_always_opens_on_its_first_tab_and_hides_the_other_panels() {
    with_options("tab", "", |t, _| {
        t.ui().set_hidden(30, false);
        t.open(1507);
        assert!(t.down(1507) && !t.down(1506));
        assert!(
            t.ui().shown(901) && !t.ui().shown(900),
            "the sound page shows"
        );
        assert!(!t.ui().shown(30), "an info panel is hidden by the showing");
        // Closed on the Sound & Video tab, it opens on File Options again.
        t.press(1004);
        assert!(!t.ui().shown(5) && !t.down(1004));
        t.press(1004);
        assert!(t.ui().shown(5) && t.down(1506) && !t.down(1507));
        assert!(t.ui().shown(900) && !t.ui().shown(901) && !t.ui().shown(902));
    });
}

#[test]
fn the_resolution_of_the_window_is_selected_when_it_shows() {
    with_options("res", "", |t, _| {
        t.open(1507);
        assert!(
            t.down(1517) && !t.down(1518) && !t.down(1519),
            "800 is the first"
        );
    });
}

#[test]
fn the_controls_take_the_values_of_the_ini_once() {
    let ini = "[UI]\nhelpType=1\ntooltipDelay=101\ntooltipDuration=3050\nmouseScrollX=32\n\
               menuMusicMaxAttenuation=-2000\nmenuMusicAttenuation=-1000\nuserAttenuation=600\n\
               useAlternateCursors=1\n[advanced]\nlevel=3\nloadHalfAnims=1\nuse8BitSound=1\n";
    with_options("start", ini, |t, _| {
        let at: Vec<i32> = BARS.iter().map(|b| b.0).map(|id| t.slider(id)).collect();
        // volume 100 - 600 / 15, music 100 - 1000 / 50, scroll trunc(32 / 120 * 100), delay
        // 100 / 10, duration 2550 / 100.
        assert_eq!(at, [60, 80, 26, 10, 25]);
        for id in [1521, 1593, 1546, 1552, 1556] {
            assert!(t.down(id), "{id}");
        }
        assert!(!t.down(1520) && !t.down(1590) && !t.down(1539) && !t.down(1551) && !t.down(1555));
        // Applied: the delay 10 s + 1, the duration 100 s + 500, the main volume.
        assert_eq!(t.ui().settings().tooltip_delay, 101);
        let tip = t.ui().tree.find(7003).unwrap();
        assert_eq!(t.ui().tree.elements[tip].timer_duration, 3000);
        assert_eq!(t.app.user_term(), 600);
        assert!(t.app.alternate_cursors());
    });
}

#[test]
fn a_missing_key_takes_the_built_in_value_and_odd_values_are_quantised() {
    with_options("defaults", "", |t, _| {
        let at: Vec<i32> = BARS.iter().map(|b| b.0).map(|id| t.slider(id)).collect();
        assert_eq!(at, [100, 100, 26, 0, 45], "1000 gives 100, 5000 gives 45");
        for id in [1520, 1592, 1539, 1551, 1555] {
            assert!(t.down(id), "{id}");
        }
        assert_eq!(t.app.user_term(), 0);
    });
    for (volume, slider, term) in [
        (37, 98, 30),
        (1500, 0, 5000),
        (-5, 100, 0),
        (99_999, 0, 5000),
    ] {
        let ini = format!("[UI]\nuserAttenuation={volume}\ntooltipDelay=-50\n");
        with_options(&format!("volume{volume}"), &ini, |t, _| {
            assert_eq!(t.slider(1511), slider, "{volume}");
            assert_eq!(t.app.user_term(), term, "{volume}");
            // A delay below 0 is the slider's 0 or less, the thumb held at 0, the delay at 0 ms.
            assert_eq!(t.slider(1523), 0);
            assert_eq!(t.ui().settings().tooltip_delay, 0);
        });
    }
}

#[test]
fn a_terrain_level_without_a_button_selects_and_changes_nothing() {
    for level in ["4", "-1", "99999"] {
        let ini = format!("[advanced]\nlevel={level}\nnormal=7\n");
        with_options(&format!("level{level}"), &ini, |t, dir| {
            t.pass(&[]);
            assert!([1590, 1591, 1592, 1593].iter().all(|&id| !t.down(id)));
            assert_eq!(written(dir, "advanced", "normal"), None);
            assert!(t.app.renderer.blending());
        });
    }
}

#[test]
fn the_start_up_replaces_the_terrain_keys_by_the_preset_and_writes_the_scroll_step() {
    let ini = "[UI]\nmouseScrollX=40\n[advanced]\nlevel=1\n";
    with_options("preset", ini, |t, dir| {
        // Level 1: normal 3, click 4, drag 4; the shipped level 2 would keep 3, 1, 2.
        let keys = ["normal", "click", "drag", "level"].map(|k| written(dir, "advanced", k));
        assert_eq!(
            keys,
            [Some(3), Some(4), Some(4), None],
            "the level is already in the ini"
        );
        assert_eq!(written(dir, "UI", "mouseScrollY"), Some(40));
        assert_eq!(written(dir, "UI", "mouseScrollX"), None, "already 40");
        assert!(t.app.renderer.blending());
    });
    with_options("preset0", "[advanced]\nlevel=0\n", |t, dir| {
        assert!(!t.app.renderer.blending(), "level 0 does not blend");
        assert_eq!(
            written(dir, "advanced", "level"),
            None,
            "already in the ini"
        );
        assert_eq!(written(dir, "advanced", "drag"), Some(0));
        assert_eq!(written(dir, "advanced", "click"), Some(0));
        assert_eq!(written(dir, "advanced", "normal"), Some(0));
    });
}

#[test]
fn the_main_volume_is_applied_at_once_and_saved_when_the_game_ends() {
    with_options("volume", "[UI]\nuserAttenuation=600\n", |t, dir| {
        t.open(1507);
        t.nudge(1511, true);
        assert_eq!(t.slider(1511), 70);
        assert_eq!(t.app.user_term(), 450, "(100 - 70) * 15");
        assert_eq!(
            written(dir, "UI", "userAttenuation"),
            None,
            "not before the end"
        );
        t.pass(&[Command::Quit]);
        assert_eq!(written(dir, "UI", "userAttenuation"), Some(450));
    });
    with_options("silent", "[UI]\nuserAttenuation=1425\n", |t, _| {
        t.open(1507);
        assert_eq!(t.slider(1511), 5);
        t.nudge(1511, false);
        assert_eq!(
            (t.slider(1511), t.app.user_term()),
            (0, 5000),
            "0 is nearly silent"
        );
    });
}

#[test]
fn the_menu_music_slider_writes_the_attenuation_and_nothing_else() {
    with_options(
        "music",
        "[UI]\nmenuMusicMaxAttenuation=-2000\n",
        |t, dir| {
            t.open(1507);
            assert_eq!(t.slider(1513), 80, "100 - (-1000 + 2000) / 50");
            t.nudge(1513, true);
            assert_eq!(t.slider(1513), 90);
            assert_eq!(
                written(dir, "UI", "menuMusicAttenuation"),
                Some(-2000 + (100 - 90) * 50)
            );
            t.nudge(1513, true);
            assert_eq!(t.slider(1513), 100, "held to the end of the bar");
            assert_eq!(written(dir, "UI", "menuMusicAttenuation"), Some(-2000));
            assert_eq!(t.app.user_term(), 0);
        },
    );
}

#[test]
fn a_resolution_is_kept_for_the_next_start_and_the_video_check_follows_the_display() {
    with_options("video", "", |t, dir| {
        t.app.set_display(Some((1280, 1024, 32)));
        t.open(1507);
        let on = |t: &mut T, id| {
            let i = t.ui().tree.find(id).unwrap();
            t.ui().tree.elements[i].state & crate::ui::layout::DISABLED == 0
        };
        // In a window a size is enabled when the desktop is larger than it.
        assert_eq!([1517, 1518, 1519].map(|id| on(t, id)), [true, true, false]);
        assert!(
            t.ui().shown(1588) && !t.ui().shown(1587),
            "only Full Screen shows in a window"
        );
        t.press(1518);
        assert_eq!(written(dir, "user", "screenwidth"), Some(1024));
        assert_eq!(written(dir, "user", "screenheight"), Some(768));
        // In full screen a size is enabled when the display is as large, and Window Mode shows.
        t.pass(&[Command::Fullscreen(true)]);
        assert_eq!([1517, 1518, 1519].map(|id| on(t, id)), [true, true, true]);
        assert!(t.ui().shown(1587) && !t.ui().shown(1588));
        assert!(on(t, 1587), "800 x 600 is below the desktop");
    });
}

#[test]
fn the_mode_buttons_ask_for_the_other_mode_and_a_window_is_refused_on_a_small_desktop() {
    with_options("mode", "", |t, _| {
        t.open(1507);
        t.press(1588);
        assert_eq!(t.app.take_mode_request(), Some(true));
        assert_eq!(t.app.take_mode_request(), None, "once");
        t.pass(&[Command::Fullscreen(true)]);
        t.press(1587);
        assert_eq!(t.app.take_mode_request(), Some(false));
        // The desktop is no larger than the game: refused with the notice, nothing asked.
        t.app.set_display(Some((800, 600, 32)));
        t.pass(&[]);
        assert!(!t.ui().is_enabled(1587), "Window Mode is disabled then");
        t.app.mode_pressed(false);
        t.pass(&[]);
        assert_eq!(t.app.take_mode_request(), None);
        assert!(t.noticed("too small"));
        t.pass(&[Command::Key(Key::Enter, false, false)]);
        // Eight bits per pixel wins over the size.
        t.app.set_display(Some((800, 600, 8)));
        t.app.mode_pressed(false);
        t.pass(&[]);
        assert!(t.noticed("too shallow"));
    });
}

#[test]
fn the_tooltip_buttons_and_sliders_act_at_once_and_only_the_help_type_is_written() {
    with_options("tips", "", |t, dir| {
        t.open(1508);
        t.press(1521);
        assert!(t.ui().settings().long_help);
        assert_eq!(written(dir, "UI", "helpType"), Some(1));
        t.press(1520);
        assert!(!t.ui().settings().long_help);
        assert_eq!(written(dir, "UI", "helpType"), Some(0));
        // The delay slider moves from 0 to 10: 10 s + 1 ms; the duration from 45 to 55.
        t.nudge(1523, true);
        assert_eq!(t.ui().settings().tooltip_delay, 101);
        t.nudge(1525, true);
        let tip = t.ui().tree.find(7003).unwrap();
        assert_eq!(t.ui().tree.elements[tip].timer_duration, 100 * 55 + 500);
        assert_eq!(written(dir, "UI", "tooltipDelay"), None);
        assert_eq!(written(dir, "UI", "tooltipDuration"), None);
    });
}

#[test]
fn the_scroll_slider_sets_both_steps_and_writes_them() {
    with_options("scroll", "", |t, dir| {
        t.open(1508);
        assert_eq!(t.slider(1515), 26);
        t.nudge(1515, true);
        // 8 - trunc(36 * 0.01 * -120) = 51, not the 32 + 10 the thumb's start suggests.
        assert_eq!(t.slider(1515), 36);
        assert_eq!(written(dir, "UI", "mouseScrollX"), Some(51));
        assert_eq!(written(dir, "UI", "mouseScrollY"), Some(51));
        assert_eq!(t.app.game.as_ref().unwrap().camera.settings.mouse, 51);
        assert_eq!(t.app.settings.mouse, 51, "a game that opens later");
    });
}

#[test]
fn a_terrain_button_writes_its_preset_in_order_and_switches_the_blending() {
    with_options("terrain", "", |t, dir| {
        t.open(1507);
        t.press(1590);
        assert!(!t.app.renderer.blending());
        let keys = |t: &T| {
            ["level", "drag", "click", "normal"].map(|k| written(t.dir(dir), "advanced", k))
        };
        assert_eq!(keys(t), [Some(0), Some(0), Some(0), Some(0)]);
        t.press(1591);
        assert!(t.app.renderer.blending());
        assert_eq!(keys(t), [Some(1), Some(4), Some(4), Some(3)]);
        t.press(1592);
        assert_eq!(keys(t), [Some(2), Some(3), Some(1), Some(2)]);
        t.press(1593);
        assert_eq!(keys(t), [Some(3), Some(0), Some(0), Some(0)]);
    });
}

impl T<'_> {
    fn dir<'a>(&self, dir: &'a Path) -> &'a Path {
        dir
    }
}

#[test]
fn the_advanced_buttons_write_only_a_change_and_the_cursors_flip_at_once() {
    with_options("advanced", "", |t, dir| {
        t.open(1530);
        t.press(1539);
        t.press(1551);
        assert_eq!(
            written(dir, "advanced", "loadHalfAnims"),
            None,
            "the current value"
        );
        assert_eq!(written(dir, "advanced", "use8BitSound"), None);
        t.press(1546);
        t.press(1552);
        assert_eq!(written(dir, "advanced", "loadHalfAnims"), Some(1));
        assert_eq!(written(dir, "advanced", "use8BitSound"), Some(1));
        t.press(1556);
        assert!(t.app.alternate_cursors());
        assert_eq!(written(dir, "UI", "useAlternateCursors"), Some(1));
        // Ctrl+C: the switch and the matching button.
        t.app.set_alternate_cursors(false);
        assert!(t.down(1555) && !t.down(1556));
        assert_eq!(written(dir, "UI", "useAlternateCursors"), Some(0));
    });
}

#[test]
fn the_restart_notice_comes_when_a_value_differs_at_the_hiding() {
    with_options("notice", "", |t, _| {
        t.open(1530);
        t.press(1546);
        t.press(1004);
        assert!(t.ui().modal_open());
        assert!(t.noticed("restart"));
        t.pass(&[Command::Key(Key::Enter, false, false)]);
        assert!(!t.ui().modal_open());
        // The window shown again remembers the new value: no change, no notice.
        t.open(1530);
        t.press(1004);
        assert!(!t.ui().modal_open());
    });
    with_options("back", "", |t, _| {
        t.open(1530);
        t.press(1546);
        t.press(1539);
        t.press(1004);
        assert!(!t.ui().modal_open(), "changed and changed back");
    });
    with_options("nothing", "", |t, _| {
        t.open(1530);
        t.press(1556);
        t.press(1521);
        t.press(1004);
        assert!(
            !t.ui().modal_open(),
            "the cursors and the tooltips need no restart"
        );
    });
}

#[test]
fn the_help_topics_are_listed_once_from_the_versioned_files_in_order() {
    with_options("topics", "", |t, _| {
        let rows: Vec<String> = (0..)
            .map_while(|r| {
                t.ui()
                    .row_text(28003, r)
                    .map(|b| String::from_utf8_lossy(b).into())
            })
            .collect();
        // About, Support, Quick start, then by title, a prefix before a longer text; the unversioned
        // file is skipped and the second "Apple" is the same topic as the first.
        let want = [
            "About the game",
            "Technical Support",
            "Quick start",
            "App",
            "Apple",
            "Mid",
            "Zebra",
        ];
        assert_eq!(rows, want);
    });
}

#[test]
fn the_help_button_opens_the_about_topic_and_f1_the_one_shown_last() {
    with_options("help", "", |t, _| {
        t.press(1004);
        t.press(1509);
        assert!(t.ui().shown(38));
        let text = |t: &mut T, id| String::from_utf8(t.ui().text(id).unwrap()).unwrap();
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(text(t, 28002), "About the game");
        assert_eq!(text(t, 28005), format!("Version {version}, id ----, %s"));
        // A row click shows its topic; the text is the named one.
        t.ui().select_row(28003, Some(6));
        t.pass(&[]);
        t.app.help_show(6);
        assert_eq!(
            (text(t, 28002), text(t, 28005)),
            ("Zebra".into(), "stripes".into())
        );
        t.press(28007);
        assert!(!t.ui().shown(38));
        // F1 reopens the topic shown last; the button, when shown, shows the About topic again.
        t.pass(&[Command::Key(Key::F1, false, false)]);
        assert_eq!(text(t, 28002), "Zebra");
        t.press(1509);
        assert_eq!(text(t, 28002), "About the game");
        t.pass(&[Command::Key(Key::F1, false, false)]);
        assert_eq!(text(t, 28002), "About the game", "the topic shown last");
        // Ctrl+F1 is something else.
        t.pass(&[Command::Key(Key::Escape, false, false)]);
        t.pass(&[Command::Key(Key::F1, true, false)]);
        assert!(!t.ui().shown(38));
    });
}

#[test]
fn the_help_window_pauses_a_running_game_and_only_that() {
    with_options("pause", "", |t, _| {
        let paused = |t: &T| t.app.game.as_ref().unwrap().paused;
        assert!(!paused(t));
        t.pass(&[Command::Key(Key::F1, false, false)]);
        assert!(paused(t) && t.ui().shown(38));
        t.pass(&[Command::Key(Key::Escape, false, false)]);
        assert!(
            !t.ui().shown(38) && !paused(t),
            "Escape closes it and unpauses"
        );
        // A game the player paused stays paused when the window closes.
        t.pass(&[Command::Pause]);
        assert!(paused(t));
        t.pass(&[Command::Key(Key::F1, false, false)]);
        t.press(28007);
        assert!(!t.ui().shown(38) && paused(t));
    });
}

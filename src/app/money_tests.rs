//! The cash on the status bar, the `$` key, the month change and the cash a start sets, seen
//! through the log, on a made-up map and a made-up status bar.

use super::*;
use crate::entity::Catalogue;
use crate::menu::Start;
use crate::money::{CONSTRUCTION, NET, StartCash};
use crate::render::tests::{cache_install, flat_map, frame_file, tile_with};
use crate::sprite::tests::Install;
use crate::ui::draw::Strings as UiStrings;
use crate::ui::layout::{Kind, Tree};
use crate::vfs::Vfs;

const SIZE: (u32, u32) = (64, 48);

struct Texts;

impl UiStrings for Texts {
    fn string(&self, _: i32) -> String {
        String::new()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

fn map() -> Zoo {
    let mut zoo = tile_with(&[]);
    zoo.width = 4;
    zoo.height = 4;
    zoo.tiles = flat_map(4, 4);
    zoo
}

/// The map view and a status bar with the cash (element 1016, a text of the layout's own colour).
fn interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [b]\ntype=UILayout\nlayout=ui/bar.lyt\n";
    let bar = "[LayoutInfo]\nid=2\nlayer=1\nx=0\ny=0\ndx=40\ndy=10\n\
        [m]\ntype=UIText\nid=1016\nx=0\ny=0\ndx=40\ndy=10\nforecolor=255\nforecolor=212\nforecolor=60\n\
        [u]\ntype=UIStatusImage\nid=1011\nx=0\ny=0\ndx=10\ndy=2\n\
        [v]\ntype=UIStatusImage\nid=1013\nx=0\ny=0\ndx=10\ndy=2\n\
        [w]\ntype=UIStatusImage\nid=1015\nx=0\ny=0\ndx=10\ndy=2\n";
    let files = Install::new(
        &format!("app-money-ui-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/bar.lyt", bar.as_bytes().to_vec()),
        ],
    );
    let mut tree = Tree::load(&Vfs::open(&files.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    Interface::new(tree, Box::new(Texts))
}

/// The map view, the status bar and the zoo status window with a Finances tab of 60 cells: the
/// button that opens it is element 1014 and the tab of the table is 4154, at (55, 42).
fn status_interface(name: &str, first_tab: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [b]\ntype=UILayout\nlayout=ui/bar.lyt\n\
        [c]\ntype=UILayout\nlayout=ui/status.lyt\n";
    let bar = "[LayoutInfo]\nid=2\nlayer=1\nx=0\ny=0\ndx=40\ndy=10\n\
        [m]\ntype=UIText\nid=1016\nx=0\ny=0\ndx=40\ndy=10\n\
        [z]\ntype=UIButton\nid=1014\nx=0\ny=0\ndx=5\ndy=5\nstate=2048\naction=3\ntarget=14\n";
    let status = format!(
        "[LayoutInfo]\nid=14\nlayer=5\nx=0\ny=0\ndx=64\ndy=48\nstate=1\n\
        [r]\ntype=UIRadioSet\nid=4103\nbutton=4104\nbutton=4154\n\
        [i]\ntype=UIButton\nid=4104\nx=50\ny=30\ndx=10\ndy=5\nlayer=1\nstate=4096\n{first_tab}\naction=3\ntarget=800\n\
        [p]\ntype=UILayout\nlayout=ui/info.lyt\nstate=1\n\
        [f]\ntype=UIButton\nid=4154\nx=50\ny=40\ndx=10\ndy=5\nlayer=1\ntransparent=0\nstate=4096\naction=3\ntarget=110\n\
        [t]\ntype=UILayout\nlayout=ui/finances.lyt\nstate=1\n"
    );
    let mut finances = "[LayoutInfo]\nid=110\nx=0\ny=0\ndx=64\ndy=48\nstate=1\n".to_string();
    for id in (3920..=3924)
        .chain(3926..=3929)
        .chain(3930..3990)
        .chain([3991])
    {
        // The admissions income and construction cells give the colours of the totals.
        let colour = match (id - 3930) % 15 {
            1 => "forecolor=83\nforecolor=219\nforecolor=83\n",
            2 => "forecolor=186\nforecolor=69\nforecolor=33\n",
            _ => "forecolor=237\nforecolor=228\nforecolor=173\n",
        };
        finances += &format!("[e{id}]\ntype=UIText\nid={id}\nx=0\ny=0\ndx=9\ndy=9\n{colour}");
    }
    let files = Install::new(
        &format!("app-status-ui-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/bar.lyt", bar.as_bytes().to_vec()),
            ("ui/status.lyt", status.into_bytes()),
            (
                "ui/info.lyt",
                b"[LayoutInfo]\nid=800\nx=0\ny=0\ndx=9\ndy=9\nstate=1\n".to_vec(),
            ),
            ("ui/finances.lyt", finances.into_bytes()),
        ],
    );
    let mut tree = Tree::load(&Vfs::open(&files.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Texts));
    // The game shows its main panel, with the button of the window.
    assert!(ui.show(2));
    ui
}

/// Runs `body` on a game with a status bar.
fn with_game(name: &str, body: impl FnOnce(&mut App)) {
    let install = cache_install(&format!("app-money-{name}"), frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.set_interface(interface(name));
    body(&mut app);
}

/// The log lines of a pass that tell of money, without the lead.
fn said(app: &App, pass: u64, now: u64) -> Vec<String> {
    let lead = format!("pass {pass} t={now} money ");
    app.log_lines(pass, now)
        .iter()
        .filter_map(|l| l.strip_prefix(&lead))
        .map(String::from)
        .collect()
}

fn bar(app: &mut App) -> (String, [u8; 3]) {
    let ui = app.interface().unwrap();
    let i = ui.tree.find(1016).unwrap();
    let color = ui.tree.elements[i].look.fore_color;
    (String::from_utf8(ui.text(1016).unwrap()).unwrap(), color)
}

fn with_cash(cash: f32) -> Money {
    Money {
        cash,
        ..Money::default()
    }
}

#[test]
fn the_status_bar_shows_the_cash_in_whole_units_and_the_log_tells_each_change() {
    with_game("bar", |app| {
        app.set_money(with_cash(50_000.0));
        app.pass(0, &[]);
        assert_eq!(said(app, 0, 0), ["cash 50000", "bar $50,000 green"]);
        assert_eq!(bar(app), ("$50,000".into(), [83, 219, 83]));
        // Nothing changed: nothing is told.
        app.pass(20, &[]);
        assert!(said(app, 1, 20).is_empty());
        // Sub-unit changes that round to the same units change nothing on the bar.
        app.set_money(with_cash(50_000.25));
        app.pass(40, &[]);
        assert!(said(app, 2, 40).is_empty());
        // The three colours.
        for (cash, text, name, color) in [
            (0.0, "$0", "yellow", [255, 212, 60]),
            (-1234.0, "($1,234)", "red", [186, 69, 33]),
            (12.5, "$12", "green", [83, 219, 83]),
        ] {
            app.set_money(with_cash(cash));
            app.pass(60, &[]);
            let units = text.trim_matches(|c| c == '$' || c == '(' || c == ')');
            let units = if cash < 0.0 {
                format!("-{units}")
            } else {
                units.into()
            };
            let units = units.replace(',', "");
            assert_eq!(
                said(app, 3, 60),
                [format!("cash {units}"), format!("bar {text} {name}")]
            );
            assert_eq!(bar(app), (text.into(), color));
        }
    });
}

#[test]
fn the_dollar_key_donates_ten_thousand_unless_the_game_is_paused() {
    with_game("dollar", |app| {
        app.set_money(with_cash(100.0));
        app.pass(0, &[]);
        app.pass(20, &[Command::Char(b'$')]);
        assert_eq!(said(app, 1, 20), ["cash 10100", "bar $10,100 green"]);
        let m = app.money().unwrap();
        // Booked as a donation: the category, the month, the year and the net line.
        assert_eq!(
            (m.total[8], m.monthly[8][0], m.yearly[8][0], m.total[NET]),
            (10_000.0, 10_000.0, 10_000.0, 10_000.0)
        );
        app.pass(40, &[Command::Pause]);
        app.pass(60, &[Command::Char(b'$')]);
        assert!(said(app, 3, 60).is_empty());
        assert_eq!(app.money().unwrap().cash, 10_100.0);
        // Other characters do nothing to the cash.
        app.pass(80, &[Command::Pause, Command::Char(b'4')]);
        assert_eq!(app.money().unwrap().cash, 10_100.0);
    });
}

#[test]
fn the_month_change_clears_the_new_column_one_step_after_the_calendar_enters_the_month() {
    with_game("month", |app| {
        let mut money = with_cash(1000.0);
        money.charge(CONSTRUCTION, 10.0);
        money.monthly[CONSTRUCTION][1] = 99.0; // February of last year
        app.set_money(money);
        let (mut now, mut months, mut hashes) = (0, Vec::new(), Vec::new());
        // Steps of the cap (200 ms) are 24 minutes of game time: 31 days take 1860 of them.
        for pass in 0..1870 {
            app.pass(now, &[]);
            months.extend(
                said(app, pass, now)
                    .into_iter()
                    .filter(|l| l.starts_with("month")),
            );
            // The state hash follows the month change, as `sim --hash` computes it.
            let lead = format!("pass {pass} t={now} sim hash ");
            for line in app.log_lines(pass, now) {
                if let Some(hash) = line.strip_prefix(&lead) {
                    let game = app.game.as_ref().unwrap();
                    let state = crate::sim::hash::state(&app.sim, &game.world, &game.money);
                    hashes.push((hash.to_string(), format!("{state:016x}")));
                }
            }
            now += 200;
        }
        assert_eq!(months, ["month 2 year 0"]);
        assert_eq!(hashes.len(), 1);
        assert_eq!(hashes[0].0, hashes[0].1);
        let m = app.money().unwrap();
        assert_eq!(m.monthly[CONSTRUCTION][..2], [10.0, 0.0]);
        assert_eq!(m.total[CONSTRUCTION], 10.0);
        assert_eq!(m.cash, 990.0, "the cash is not touched");
    });
}

#[test]
fn the_three_status_bars_follow_the_zoo_status_once_a_second_and_the_log_tells_each_change() {
    with_game("bars", |app| {
        app.set_money(with_cash(5000.0));
        let (mut now, mut told) = (0, Vec::new());
        for pass in 0..60 {
            app.pass(now, &[]);
            told.extend(
                said(app, pass, now)
                    .into_iter()
                    .filter(|l| l.starts_with("bars")),
            );
            now += 200;
        }
        // No animal and no guest: 0 and 0; the load's rating check gives 12 + 12 for the
        // happiness parts and 1 for 5000 of zoo value at once, and the check 4 s in changes nothing.
        assert_eq!(told, ["bars 0 0 25"]);
        let ui = app.interface().unwrap();
        let i = ui.tree.find(1015).unwrap();
        let Kind::StatusImage(levels) = &ui.tree.elements[i].kind else {
            panic!("not a bar");
        };
        assert_eq!(levels.value, 25);
    });
}

fn start(cash: StartCash, reset: bool) -> Start {
    Start {
        path: "x.zoo".into(),
        label: "test".into(),
        forget_last: false,
        freeform: false,
        cash,
        reset,
    }
}

#[test]
fn a_freeform_start_sets_the_cash_and_a_scenario_start_sets_then_adds_without_booking() {
    with_game("start", |app| {
        let mut stored = with_cash(7.0);
        stored.total[CONSTRUCTION] = 40.0;
        stored.total[NET] = -40.0;
        // A freeform start: the cash of its screen; the file's accounts are reset.
        app.open_game(map(), stored.clone()).unwrap();
        let freeform = StartCash {
            set: Some(50_000.0),
            add: None,
        };
        app.begin(&start(freeform, true));
        app.pass(0, &[]);
        assert_eq!(
            said(app, 0, 0),
            ["set 50000", "cash 50000", "bar $50,000 green"]
        );
        assert_eq!(app.money().unwrap().total, [0.0; 31]);
        // A scenario: its set, then its add.
        app.open_game(map(), stored.clone()).unwrap();
        let scenario = StartCash {
            set: Some(100_000.0),
            add: Some(200.0),
        };
        app.begin(&start(scenario, false));
        app.pass(20, &[]);
        assert_eq!(
            said(app, 1, 20),
            ["set 100000", "add 200", "cash 100200", "bar $100,200 green"]
        );
        // Nothing was booked; the accounts of the file stay when the start does not reset.
        let m = app.money().unwrap();
        assert_eq!((m.total[CONSTRUCTION], m.total[NET]), (40.0, -40.0));
        // A scenario with an add and no set adds to what the file stores; one with neither
        // keeps it.
        app.open_game(map(), stored.clone()).unwrap();
        app.begin(&start(
            StartCash {
                set: None,
                add: Some(-10.0),
            },
            false,
        ));
        app.pass(40, &[]);
        assert_eq!(said(app, 2, 40)[..2], ["add -10", "cash -3"]);
        app.open_game(map(), stored).unwrap();
        app.begin(&start(StartCash::default(), false));
        app.pass(60, &[]);
        assert_eq!(said(app, 3, 60)[0], "cash 7");
    });
}

/// The log lines about the table in a pass: `cell ID RRGGBB TEXT`.
fn cells(app: &App, pass: u64, now: u64) -> Vec<String> {
    said(app, pass, now)
        .into_iter()
        .filter_map(|l| l.strip_prefix("cell ").map(String::from))
        .collect()
}

/// Passes at steps of 20 ms from `now` until the table is written; the lines of that pass, with
/// the clock moved to it. The clock does not go past `limit` ms.
fn until_written(app: &mut App, now: &mut u64, limit: u64) -> Vec<String> {
    while *now + 20 <= limit {
        *now += 20;
        app.pass(*now, &[]);
        let lines = cells(app, *now, *now);
        if !lines.is_empty() {
            return lines;
        }
    }
    vec![]
}

#[test]
fn the_z_key_opens_the_zoo_status_window_and_its_finances_table_follows_the_accounts() {
    with_game("table", |app| {
        app.set_interface(status_interface("table", "state=8"));
        let mut money = with_cash(50_000.0);
        money.charge(CONSTRUCTION, 40.0);
        app.set_money(money);
        // Hidden: the table is not written.
        app.pass(0, &[]);
        assert!(cells(app, 0, 0).is_empty());
        assert!(!app.interface().unwrap().shown(110));
        // Z presses the button: the window opens on its first tab. The first run of the update
        // is due at once and writes the table, whether or not its page is shown.
        app.pass(20, &[Command::Char(b'z')]);
        assert!(app.interface().unwrap().shown(14));
        assert!(!app.interface().unwrap().shown(110));
        let mut now = 20;
        let mut lines = cells(app, 20, 20);
        if lines.is_empty() {
            lines = until_written(app, &mut now, 200);
        }
        let first = now;
        assert_eq!(lines.len(), 4 * 14 + 3, "{lines:?}");
        assert_eq!(lines[0], "3920 ede4ad Jan");
        // The total is a loss: the colour is the construction cell's.
        for want in [
            "3941 ba4521 ($40)",
            "3926 53db53 $49,960",
            "3991 ba4521 ($40)",
            "3932 ba4521 ($40)",
        ] {
            assert!(lines.contains(&want.to_string()), "{want} {lines:?}");
        }
        assert_eq!(app.interface().unwrap().text(3921).unwrap(), b"");
        // The Finances tab shows the page: nothing changed, nothing is written.
        let tab = [
            Command::Mouse(Mouse::Move(55, 42)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ];
        now += 20;
        app.pass(now, &tab);
        assert!(app.interface().unwrap().shown(110));
        assert!(cells(app, now, now).is_empty());
        // A donation shows only at the next run, 10 s of clock after the first.
        now += 20;
        app.pass(now, &[Command::Char(b'$')]);
        assert!(cells(app, now, now).is_empty());
        let early = until_written(app, &mut now, first + 9_980);
        assert!(early.is_empty(), "{first} {now} {early:?}");
        let lines = until_written(app, &mut now, first + 10_100);
        assert!(now >= first + 10_000, "{now}");
        assert!(
            lines.contains(&"3938 ede4ad $10,000".to_string()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"3941 53db53 $9,960".to_string()),
            "{lines:?}"
        );
        assert!(lines.iter().all(|l| !l.starts_with("3921")), "{lines:?}");
        // Closing and showing the page again writes at once what changed meanwhile.
        app.pass(now + 20, &[Command::Char(b'Z')]);
        assert!(!app.interface().unwrap().shown(14));
        app.pass(now + 40, &[Command::Char(b'$')]);
        app.pass(now + 60, &[Command::Char(b'z')]);
        assert!(app.interface().unwrap().shown(110));
        let lines = cells(app, now + 60, now + 60);
        assert!(
            lines.contains(&"3938 ede4ad $20,000".to_string()),
            "{lines:?}"
        );
        let now = now + 60;
        // Another character does nothing, and Z again closes the window.
        app.pass(now + 80, &[Command::Char(b'x')]);
        assert!(app.interface().unwrap().shown(110));
        app.pass(now + 100, &[Command::Char(b'Z')]);
        assert!(!app.interface().unwrap().shown(14));
        // No button, a hidden panel, or a text that is no button: nothing to press.
        let ui = app.interface().unwrap();
        assert!(!ui.press_button(99_999) && !ui.press_button(1016));
        ui.tree.hide(ui.tree.find(2).unwrap());
        assert!(!ui.press_button(1014));
    });
}

#[test]
fn a_month_change_rewrites_the_four_columns_while_the_window_is_open() {
    with_game("table-month", |app| {
        app.set_interface(status_interface("table-month", "state=8"));
        let mut money = with_cash(1000.0);
        money.charge(CONSTRUCTION, 10.0);
        app.set_money(money);
        let tab = [
            Command::Char(b'z'),
            Command::Mouse(Mouse::Move(55, 42)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ];
        app.pass(0, &tab);
        assert_eq!(app.interface().unwrap().text(3920).unwrap(), b"Jan");
        let mut now = 0;
        // The month changes after 1860 steps; the update that follows is at most 10 s later.
        for _ in 0..1920 {
            now += 200;
            app.pass(now, &[]);
        }
        let ui = app.interface().unwrap();
        assert_eq!(ui.text(3920).unwrap(), b"Feb");
        // January moved to the column of the previous month, February is the current one.
        assert_eq!(ui.text(3923).unwrap(), b"Jan");
        assert_eq!(ui.text(3921).unwrap(), b"");
        assert_eq!(ui.text(3932).unwrap(), b"$0");
        assert_eq!(ui.text(3932 + 15 * 3).unwrap(), b"($10)");
    });
}

#[test]
fn a_disabled_remembered_tab_gives_way_to_the_next_enabled_one() {
    with_game("table-disabled", |app| {
        // The first tab is selected and disabled: the window opens on the Finances tab.
        app.set_interface(status_interface("table-disabled", "state=8\nstate=2"));
        app.pass(20, &[Command::Char(b'z')]);
        let ui = app.interface().unwrap();
        assert!(ui.shown(14) && ui.shown(110) && !ui.shown(800));
    });
}

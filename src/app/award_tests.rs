//! Award goals in play: a goals file shaped like the shipped extra file (an award goal, then a
//! popup goal and a cash goal of the same rule; sticky, hidden, optional) with invented strings.

use super::scenario_tests::play;
use super::start_tests::*;
use super::*;

/// One award in the shape of the extra file: the goal, its popup goal and its cash goal.
fn award(name: &str, id: i32, rule: &str, cash: Option<i32>) -> String {
    let body = format!("{rule}\nsticky=1\nhidden=1\noptional=1\ntext=9101\n");
    let cash = cash.map_or(String::new(), |c| {
        format!("[{name}_cash]\n{body}trulea=3\ntruleb=1\ntarga={c}\n")
    });
    format!(
        "[{name}]\n{body}trulea=6    \ntruleb=0    \ntarga={id}\ntargb=  \n\
         [{name}_popup]\n{body}trulea=2    \ntruleb=0    \ntarga={id}\n{cash}"
    )
}

/// A goals file: these goals, in this order, then their sections and the popup sections.
fn file(goals: &[&str], sections: &[String]) -> Vec<u8> {
    let list: String = goals.iter().map(|g| format!("goal={g}\n")).collect();
    format!(
        "[duration]\nnummonths=-1\n[goals]\n{list}{}\
         [default]\nlayout=45200\nimage1=\nshortText1=\nlongText1= \nlongText2=0 \nok=1\npause=0\n\
         [7]\nlongText1=AWD_SEVEN \n[9]\nlongText1=AWD_NINE \n",
        sections.concat()
    )
    .into_bytes()
}

/// Plays `scn` for `passes` of 66 ms (a scenario pass is every 23rd), `hook` running before each
/// with its number. Returns what the scenario logged, in order, triggers apart.
fn run(name: &str, scn: Vec<u8>, passes: u64, mut hook: impl FnMut(&mut App, u64)) -> Vec<String> {
    let files = [("s/aw.scn", scn)];
    let lines = std::cell::RefCell::new(Vec::new());
    with_game(name, "", &files, None, |app, _| {
        play(app, "s/aw.scn");
        for p in 1..=passes {
            hook(app, p);
            app.pass(66 * p, &[]);
            let seen = app.log_lines(p, 66 * p).into_iter();
            lines.borrow_mut().extend(seen.filter_map(|l| {
                l.split_once(" sim scenario ")
                    .filter(|(_, r)| !r.starts_with("trigger"))
                    .map(|(_, r)| r.to_string())
            }));
        }
        let (zoo, money) = (
            &app.game.as_ref().unwrap().world.zoo,
            &app.game.as_ref().unwrap().money,
        );
        lines
            .borrow_mut()
            .push(format!("awards {:?} cash {}", zoo.awards, money.cash));
    });
    lines.into_inner()
}

fn held(app: &App) -> Vec<i32> {
    app.game.as_ref().unwrap().world.zoo.awards.clone()
}

fn text(app: &mut App) -> String {
    let text = app.interface().unwrap().text(45206).unwrap();
    String::from_utf8_lossy(&text).into_owned()
}

#[test]
fn awards_come_in_file_order_once_and_a_month_gate_and_a_goal_that_cannot_be_won_hold() {
    let rich = "rulea=1\nruleb=7\narga=6\ntype=0\nvalue=1";
    // The sick animals counted below a value the file leaves out: never true.
    let sick = "rulea=1\nruleb=5\narga=11\ntype=4";
    let sections = [
        award("a", 7, "rulea=99\narga=1\ntype=0\nvalue=1", Some(100)),
        award("b", 9, rich, Some(50)),
        award("c", 17, sick, None),
    ];
    let goals = [
        "a", "a_popup", "a_cash", "b", "b_popup", "b_cash", "c", "c_popup",
    ];
    let months = |app: &mut App, month, year| {
        let money = &mut app.game.as_mut().unwrap().money;
        (money.month, money.year) = (month, year);
    };
    let lines = run(
        "award-order",
        file(&goals, &sections),
        1200,
        |app, p| match p {
            // The first pass of the manager: the first award, its popup, its cash.
            40 => assert_eq!(text(app), "seven won"),
            // Month 5 is before the gate of the second award.
            100 => months(app, 5, 0),
            290 => assert_eq!(held(app), [7, 0]),
            300 => months(app, 6, 0),
            340 => assert_eq!(text(app), "nine won"),
            // Past month 11 of the next year the unwinnable goal still never fires.
            600 => months(app, 12, 3),
            _ => {}
        },
    );
    assert_eq!(
        lines,
        [
            "award 7",
            "popup 45200 open",
            "cash 1 100",
            "award 9",
            "popup 45200 open",
            "cash 1 50",
            "awards [9, 7, 0] cash 150",
        ]
    );
}

#[test]
fn two_awards_ready_in_one_pass_come_one_after_the_other_in_file_order() {
    let now = "rulea=99\narga=1\ntype=0\nvalue=1";
    // The goals are listed with the second award first; a goal that is not listed never runs.
    let sections = [
        award("a", 7, now, Some(100)),
        award("b", 9, now, None),
        award("z", 5, now, Some(1)),
    ];
    let goals = ["b", "b_popup", "a", "a_popup", "a_cash"];
    let lines = run("award-same-pass", file(&goals, &sections), 100, |_, _| {});
    assert_eq!(
        lines,
        [
            "award 9",
            "popup 45200 open",
            "award 7",
            "popup 45200 open",
            "cash 1 100",
            "awards [9, 7, 0] cash 100",
        ]
    );
}

/// The shipped extra file has the shape the tests above are made after: for each award 1 to 18 a
/// goal that gives it, directly followed by a goal that shows its popup, whose text is the named
/// text of the award; and award 17's goal compares with a `value` it does not have.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_award_goals_are_followed_by_their_popup() {
    use crate::config::{Config, to_int};
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let tables = crate::strings::StringTables::open(vfs.root()).unwrap();
    let cfg = Config::parse(&vfs.read("freeform/awards.scn").unwrap()).unwrap();
    let int = |name: &[u8], key: &str| {
        cfg.section(name)
            .and_then(|s| s.get(key))
            .and_then(to_int)
            .unwrap_or(0)
    };
    let goals = cfg.section("goals").unwrap().get_all("goal").to_vec();
    let mut found = Vec::new();
    for (at, goal) in goals.iter().enumerate() {
        if int(goal, "trulea") != 6 {
            continue;
        }
        let id = int(goal, "targa");
        found.push(id);
        let popup = &goals[at + 1];
        assert_eq!((int(popup, "trulea"), int(popup, "targa")), (2, id), "{id}");
        for key in [
            "rulea", "ruleb", "arga", "type", "value", "sticky", "hidden", "optional",
        ] {
            assert_eq!(int(goal, key), int(popup, key), "{id} {key}");
        }
        let section = cfg.section(id.to_string()).unwrap();
        let name = section.get("longText1").unwrap();
        assert_eq!(name, format!("AWARD_AWARD{id}").as_bytes().trim_ascii_end());
        assert!(
            !tables.text(&String::from_utf8_lossy(name)).is_empty(),
            "{id}"
        );
    }
    assert_eq!(found, (1..=18).collect::<Vec<_>>());
    // Award 17: "below" a value the file leaves out.
    let seventeen = goals
        .iter()
        .find(|g| int(g, "targa") == 17 && int(g, "trulea") == 6);
    let seventeen = seventeen.unwrap();
    assert_eq!((int(seventeen, "type"), int(seventeen, "value")), (4, 0));
}

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
    // The base game has the awards 1 to 12, the expansions the others.
    let last = if crate::fixtures::has_expansions(&dir) {
        18
    } else {
        12
    };
    assert_eq!(found, (1..=last).collect::<Vec<_>>());
    if last == 12 {
        eprintln!("skipped full_game_award_goals (awards 13 to 18): the install has no expansions");
        return;
    }
    // Award 17: "below" a value the file leaves out.
    let seventeen = goals
        .iter()
        .find(|g| int(g, "targa") == 17 && int(g, "trulea") == 6);
    let seventeen = seventeen.unwrap();
    assert_eq!((int(seventeen, "type"), int(seventeen, "value")), (4, 0));
}

/// A chain of the shape of the Marine Mania file: the condition, the award goal, then the cash,
/// the popup and a permanent unlock.
fn chain(name: &str, rule: &str, id: i32, cash: i32, unlock: i32) -> String {
    let trigger =
        |kind: &str, t: &str| format!("[{name}_{kind}]\nrulea=99\nhidden=1\noptional=0\n{t}\n");
    format!(
        "[{name}]\nrtype=2\noptional=1\nchild={name}_rule\nchild={name}_award\n\
         child={name}_cash\nchild={name}_popup\nchild={name}_unlock\n\
         [{name}_rule]\nrtype=1\nchild={name}_held\n[{name}_held]\n{rule}\n\
         sticky=0\nhidden=1\noptional=0\n{}{}{}{}",
        trigger("award", &format!("trulea=6\ntruleb=0\ntarga={id}")),
        trigger("cash", &format!("trulea=3\ntruleb=0\ntarga={cash}")),
        trigger("popup", &format!("trulea=2\ntruleb=0\ntarga={id}")),
        trigger("unlock", &format!("trulea=4\ntruleb=10\ntarga={unlock}")),
    )
}

#[test]
fn a_chain_goal_gives_award_cash_popup_and_unlock_and_the_next_one_counts_the_award() {
    // The first chain has no condition to wait for; the second wants three awards held, the
    // map's own and the ones of the first chain and of this pass.
    let always = "rulea=99\narga=1\nvalue=1";
    let two = "rulea=1\nruleb=11\nvalue=2";
    let text = format!(
        "[duration]\nnummonths=-1\n[goals]\ngoal=open\ngoal=a\ngoal=b\n\
         [open]\nrulea=99\ntype=0\nvalue=1\n{}{}\
         [default]\nlayout=45200\n[7]\nlongText1=AWD_SEVEN \n[9]\nlongText1=AWD_NINE \n",
        chain("a", always, 7, 100, 8001),
        chain("b", two, 9, 50, 8002),
    );
    let files = [("s/aw.scn", text.into_bytes())];
    with_game("award-chain", "", &files, None, |app, install| {
        play(app, "s/aw.scn");
        let (mut lines, mut passes) = (Vec::new(), std::collections::BTreeSet::new());
        for p in 1..=60u64 {
            app.pass(66 * p, &[]);
            let seen = app.log_lines(p, 66 * p).into_iter();
            let before = lines.len();
            lines.extend(seen.filter_map(|l| {
                let (_, r) = l.split_once(" sim scenario ")?;
                (!r.starts_with("trigger")).then(|| r.to_string())
            }));
            if lines.len() > before {
                passes.insert(p);
            }
        }
        assert_eq!(passes.len(), 1, "both chains in the same pass: {passes:?}");
        // Awards in order, each followed by its cash and popup; the map holds award 0.
        assert_eq!(
            lines,
            [
                "award 7",
                "cash 0 100",
                "popup 45200 open",
                "award 9",
                "cash 0 50",
                "popup 45200 open",
            ]
        );
        assert_eq!(held(app), [9, 7, 0]);
        assert_eq!(
            app.game.as_ref().unwrap().money.cash - Money::default().cash,
            150.0
        );
        let ini = std::fs::read_to_string(install.0.join("user/zoo.ini")).unwrap();
        assert!(ini.contains("unlockEntity0=8001") && ini.contains("unlockEntity1=8002"));
    });
}

/// The leaf rules of a goal, whatever its nesting: `rulea`, `ruleb`.
fn leaves(cfg: &crate::config::Config, name: &[u8]) -> Vec<(i32, i32)> {
    let int = |key: &str| {
        let value = cfg.section(name).and_then(|s| s.get(key));
        value.and_then(crate::config::to_int).unwrap_or(0)
    };
    if int("rtype") == 0 {
        return vec![(int("rulea"), int("ruleb"))];
    }
    let kids = cfg.section(name).map_or(&[][..], |s| s.get_all("child"));
    kids.iter().flat_map(|k| leaves(cfg, k)).collect()
}

/// A trigger as (`trulea`, `truleb`, `targa`).
type Trigger = (i32, i32, i32);

/// The shipped Marine Mania file has one chain goal per award 19 to 31, in this order: the
/// condition (a group of rules, or one rule), the award goal, then the cash, the popup and the
/// extras. The cash is trigger 3, the popup 2, a boxed animal 5, a permanent unlock 4 of
/// `truleb` 10 (`awards.md` 6.3).
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_award_chains_have_the_condition_the_award_and_the_companions_of_the_spec() {
    use crate::config::{Config, to_int};
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    if !crate::fixtures::with_expansions("full_game_award_chains", &dir) {
        return;
    }
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let cfg = Config::parse(&vfs.read("freeform/unlock02.scn").unwrap()).unwrap();
    let int = |name: &[u8], key: &str| {
        let value = cfg.section(name).and_then(|s| s.get(key));
        value.and_then(to_int).unwrap_or(0)
    };
    let chains: Vec<Vec<u8>> = (cfg.section("goals").unwrap().get_all("goal").iter())
        .filter(|g| int(g, "rtype") == 2)
        .cloned()
        .collect();
    assert_eq!(chains.len(), 13);
    // Per award: the count of rules of the condition, the first rule, and the triggers after
    // the award goal as (trulea, truleb, targa).
    let cash = |n| (3, 0, n);
    let popup = |n| (2, 0, n);
    let unlock = |n| (4, 10, n);
    #[rustfmt::skip]
    let spec: [(usize, (i32, i32), Vec<Trigger>); 13] = [
        (4, (6, 3), vec![cash(10_000), popup(19)]),
        (5, (6, 3), vec![popup(20), cash(20_000)]),
        (2, (6, 3), vec![popup(21), cash(5_000)]),
        (3, (6, 3), vec![(5, 0, 5039), (5, 1, 5039), popup(22)]),
        (5, (6, 3), vec![cash(25_000), popup(23)]),
        (7, (6, 3), vec![cash(50_000), popup(24), unlock(9240)]),
        (1, (1, 11), vec![cash(100_000), popup(25), (5, 2, 5001)]),
        (4, (1, 0), vec![popup(26), cash(150_000), unlock(6800)]),
        (1, (15, 3), vec![popup(27), unlock(6498)]),
        (1, (15, 3), vec![popup(28), cash(10_000)]),
        (1, (15, 3), vec![popup(29), cash(5_000), unlock(6496)]),
        (1, (15, 3), vec![popup(30), cash(10_000), unlock(6497)]),
        (19, (6, 3), vec![popup(31), cash(50_000)]),
    ];
    for (at, (chain, (count, first, tail))) in chains.iter().zip(spec).enumerate() {
        let id = 19 + at as i32;
        let kids = cfg.section(chain).unwrap().get_all("child");
        let rules = leaves(&cfg, &kids[0]);
        assert_eq!((rules.len(), rules[0]), (count, first), "{id}");
        let award = &kids[1];
        assert_eq!((int(award, "trulea"), int(award, "targa")), (6, id), "{id}");
        assert_eq!(int(award, "truleb"), 0, "{id}");
        let after: Vec<Trigger> = (kids[2..].iter())
            .map(|k| (int(k, "trulea"), int(k, "truleb"), int(k, "targa")))
            .collect();
        assert_eq!(after, tail, "{id}");
    }
}

#[test]
fn a_load_gives_the_file_list_even_when_a_saved_goal_state_says_an_award_was_given() {
    // The goal gave award 3 and is saved in state 1; the list of the file lacks it.
    let scn = b"[duration]\nnummonths=-1\n[goals]\ngoal=g\n\
        [g]\nrulea=99\narga=1\nvalue=1\ntrulea=6\ntarga=3\n"
        .to_vec();
    with_game(
        "award-resume",
        "",
        &[("s/aw.scn", scn)],
        None,
        |app, install| {
            let mut now = play(app, "s/aw.scn");
            for _ in 0..60 {
                now += 66;
                app.pass(now, &[]);
            }
            assert!(held(app).contains(&3));
            app.game
                .as_mut()
                .unwrap()
                .world
                .zoo
                .awards
                .retain(|&a| a != 3);
            let file = install.0.join("awards.zoo");
            app.save_game(file.to_str().unwrap());
            assert!(app.load_saved(file.to_str().unwrap()));
            let before = held(app);
            app.pass(now + 66, &[]);
            assert_eq!((held(app), before.contains(&3)), (before, false));
            assert!(!app.interface().unwrap().modal_open());
        },
    );
}

#[test]
fn a_modal_donation_popup_is_answered_before_the_award_popups_of_its_pass_show() {
    let now = "rulea=99\narga=1\ntype=0\nvalue=1";
    // A donation goal with its modal popup, then an award in the shape of the extra file.
    let donation = format!(
        "[d]\n{now}\ntrulea=3\ntruleb=0\ntarga=10\n[d_popup]\n{now}\ntrulea=2\ntruleb=0\ntarga=5\n\
         [5]\npause=1\nlongText1=AWD_SEVEN\n"
    );
    let sections = [donation, award("a", 7, now, Some(100))];
    let goals = ["d", "d_popup", "a", "a_popup", "a_cash"];
    let mut at_modal = None;
    let lines = run("award-modal", file(&goals, &sections), 120, |app, _| {
        if app.interface().unwrap().modal_open() {
            // The award waits behind the modal popup.
            at_modal.get_or_insert_with(|| held(app));
            app.interface().unwrap().answer(1);
        }
    });
    assert_eq!(at_modal, Some(vec![0]));
    assert_eq!(
        lines,
        [
            "cash 0 10",
            "popup 45200 modal",
            "award 7",
            "popup 45200 open",
            "cash 1 100",
            "awards [7, 0] cash 110",
        ]
    );
}

#[test]
fn the_month_command_makes_24_months_and_a_month_gated_award_comes_at_its_date_only() {
    // The shape of awards 8 and 17: cash at least 1 from total month 6; sick animals below a
    // value the file leaves out, from total month 11 (never true).
    let rich = "rulea=1\nruleb=7\narga=6\ntype=0\nvalue=1";
    let sick = "rulea=1\nruleb=5\narga=11\ntype=4";
    // A first award gives the cash the gated one reads.
    let sections = [
        award("a", 7, "rulea=99\narga=1\ntype=0\nvalue=1", Some(100)),
        award("b", 9, rich, Some(50)),
        award("c", 17, sick, None),
    ];
    let goals = [
        "a", "a_popup", "a_cash", "b", "b_popup", "b_cash", "c", "c_popup",
    ];
    let files = [("s/aw.scn", file(&goals, &sections))];
    with_game("award-months", "", &files, None, |app, _| {
        play(app, "s/aw.scn");
        let date = |app: &App| {
            let money = &app.game.as_ref().unwrap().money;
            (money.month as i32, money.year as i32)
        };
        let mut now = 66;
        let mut first = None;
        let mut cash = Vec::new();
        for skipped in 0..24 {
            let (month, year) = date(app);
            assert_eq!(month + 12 * year, skipped + 1, "one month for each skip");
            now += 66;
            app.pass(now, &[Command::Month]);
            // The month changes in the step that follows, its work one step later; then a
            // pass of the manager (every 23rd).
            for _ in 0..30 {
                now += 66;
                app.pass(now, &[]);
                let lines = app.log_lines(now / 66, now).into_iter();
                cash.extend(lines.filter(|l| l.contains(" sim scenario cash ")));
            }
            let (month, year) = date(app);
            let total = month + 12 * year;
            assert_eq!(total, skipped + 2);
            let awards = held(app);
            assert!(
                !awards.contains(&17),
                "the award goal that cannot be won: {awards:?}"
            );
            // The gate: held from total month 6 on, not before.
            assert_eq!(
                awards.contains(&9),
                total >= 6,
                "total month {total}: {awards:?}"
            );
            if awards.contains(&9) && first.is_none() {
                first = Some(total);
            }
        }
        assert_eq!(first, Some(6));
        assert_eq!(cash.len(), 2, "{cash:?}");
        assert_eq!(date(app), (1, 2));
    });
}

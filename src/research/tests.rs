use super::*;
use crate::config::Config;
use crate::entity::EntityType;
use crate::money::Money;
use crate::sprite::tests::Install;
use std::sync::Arc;

fn program(name: i32, cost: &str, target: i32, effect: i32) -> (String, Vec<u8>) {
    let body = format!("[research]\nname={name}\ncost={cost}\ntarget={target}\neffect={effect}\n");
    (format!("p/{name}.cfg"), body.into_bytes())
}

fn category(name: i32, expansion: i32, programs: &[i32]) -> (String, Vec<u8>) {
    let mut body = format!("[category]\nname={name}\nexpansion={expansion}\n");
    for p in programs {
        body += &format!("program=p/{p}.cfg\n");
    }
    (format!("c/{name}.cfg"), body.into_bytes())
}

fn branch(file: &str, name: i32, cost: u32, categories: &[i32]) -> (String, Vec<u8>) {
    let mut body = format!("[branch]\nname={name}\nfunding=none\nfunding=max\n");
    for c in categories {
        body += &format!("category=c/{c}.cfg\n");
    }
    body += &format!("[none]\nname=1\n[max]\nname=2\ncost={cost}\nwork=45.5\n");
    (file.to_string(), body.into_bytes())
}

fn list(file: &str, branches: &[&str]) -> (String, Vec<u8>) {
    let mut body = "[branches]\n".to_string();
    for b in branches {
        body += &format!("branch={b}\n");
    }
    (file.to_string(), body.into_bytes())
}

/// An install holding `files` and a scenario `s.scn` with the given `[start]` lines.
fn vfs(name: &str, start: &str, files: Vec<(String, Vec<u8>)>) -> (Install, Vfs) {
    let mut members: Vec<(&str, Vec<u8>)> =
        files.iter().map(|(n, c)| (n.as_str(), c.clone())).collect();
    let scn = format!("[start]\n{start}").into_bytes();
    members.push(("s.scn", scn));
    let install = Install::new(&format!("research-{name}"), &members);
    let vfs = Vfs::open(&install.0).unwrap();
    (install, vfs)
}

fn tree(name: &str, start: &str, files: Vec<(String, Vec<u8>)>) -> Tree {
    let (_i, vfs) = vfs(name, start, files);
    Tree::of_scenario(&vfs, "s.scn", &mut || 0).unwrap()
}

#[test]
fn lists_merge_by_prefix_and_suffix_and_a_branch_of_the_same_name_is_reused() {
    let files = vec![
        list("research.cfg", &["b1.cfg"]),
        list("researd.cfg", &["b2.cfg", "b3.cfg"]),
        // Not matched: another prefix, another suffix.
        list("resnoa.cfg", &["b3.cfg"]),
        list("research.txt", &["b3.cfg"]),
        branch("b1.cfg", 7, 100, &[10]),
        branch("b2.cfg", 7, 200, &[11]),
        branch("b3.cfg", 8, 0, &[12]),
        category(10, 0, &[1, 2]),
        category(11, 1, &[3]),
        category(12, 2, &[]),
        program(1, "10", 5, 0),
        program(2, "0", 6, 0),
        program(3, "20", 7, 1),
    ];
    let t = tree("merge", "research=research.cfg\n", files);
    let names = |t: &Tree| t.branches.iter().map(|b| b.name).collect::<Vec<_>>();
    assert_eq!(names(&t), [7, 8]);
    let b = &t.branches[0];
    // The funding levels are the later file's; the categories of both are kept in order.
    assert_eq!(
        b.funding[1],
        Level {
            name: 2,
            cost: 200.0,
            work: 45.5
        }
    );
    assert_eq!(b.funding[0].cost, 0.0);
    assert_eq!(b.level, 0);
    let cats: Vec<_> = b
        .categories
        .iter()
        .map(|c| (c.name, c.expansion, c.tick))
        .collect();
    assert_eq!(cats, [(10, 0, true), (11, 1, true)]);
    let p = &b.categories[0].programs;
    assert_eq!(
        (
            p[0].name,
            p[0].cost,
            p[0].progress,
            p[0].target,
            p[0].effect
        ),
        (1, 10.0, 0.0, 5, 0)
    );
    assert_eq!(p[1].name, 2);
    // Done: the free program only.
    assert_eq!(t.share(), 33);
}

#[test]
fn a_value_lists_by_the_first_six_characters_and_a_bad_value_lists_nothing() {
    let files = || {
        vec![
            list("resear.cfg", &["b.cfg"]),
            list("researx.cfg", &["b.cfg"]),
            // Shares only the first three characters.
            list("resnoa.cfg", &["b.cfg"]),
            branch("b.cfg", 1, 0, &[10]),
            category(10, 0, &[1]),
            program(1, "5", 0, 0),
        ]
    };
    let count = |start: &str| tree("prefix", start, files()).programs().count();
    assert_eq!(count("research=research.cfg\n"), 2);
    assert_eq!(count("research=other.cfg\n"), 0);
    for none in [
        "",
        "research=research\n",
        "research=research.\n",
        "research=\n",
    ] {
        assert_eq!(count(none), 0, "{none:?}");
    }
    // A repeated value reads its files again: the same program gives two records.
    assert_eq!(count("research=resear.cfg\nresearch=resear.cfg\n"), 4);
}

#[test]
fn an_unreadable_file_or_a_branch_without_a_name_is_dropped() {
    let files = vec![
        list("r.cfg", &["gone.cfg", "noname.cfg", "b.cfg"]),
        (
            "noname.cfg".into(),
            b"[branch]\ncategory=c/10.cfg\n".to_vec(),
        ),
        branch("b.cfg", 1, 0, &[10, 99]),
        category(10, 0, &[1, 99]),
        program(1, "5", 0, 0),
    ];
    let t = tree("dropped", "research=r.cfg\n", files);
    assert_eq!(t.branches.len(), 1);
    assert_eq!(t.branches[0].categories.len(), 1);
    assert_eq!(t.branches[0].categories[0].programs.len(), 1);
    // A scenario that cannot be read has no tree.
    let (_i, vfs) = vfs("gone", "", vec![]);
    assert!(Tree::of_scenario(&vfs, "none.scn", &mut || 0).is_none());
}

#[test]
fn the_share_counts_the_programs_done_and_is_100_for_none() {
    let files = vec![
        list("r.cfg", &["b.cfg"]),
        branch("b.cfg", 1, 0, &[10]),
        category(10, 0, &[1, 2, 3, 4, 5]),
        program(1, "0", 0, 0),
        // Missing, negative and huge costs: the first two are done, the huge one is not.
        ("p/2.cfg".into(), b"[research]\nname=2\n".to_vec()),
        program(3, "-4", 0, 0),
        program(4, "1e30", 0, 0),
        program(5, "99999999999999999999", 0, 0),
    ];
    let t = tree("share", "research=r.cfg\n", files);
    assert_eq!(t.share(), 60);
    let p = &t.branches[0].categories[0].programs;
    assert_eq!((p[1].target, p[1].effect), (-1, -1));
    assert_eq!(tree("empty", "", vec![]).share(), 100);
    assert_eq!(Tree::default().share(), 100);
}

fn ty(name_id: i32) -> EntityType {
    EntityType {
        class: "animals",
        type_name: "t".into(),
        subtype: b"f".to_vec(),
        file: None,
        config: Some(Arc::new(
            Config::parse(b"[Global]\nAvailableAtStartup=1\n").unwrap(),
        )),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

#[test]
fn building_the_tree_makes_unfinished_targets_unavailable_and_free_ones_available() {
    let files = vec![
        list("r.cfg", &["b.cfg"]),
        branch("b.cfg", 1, 0, &[10]),
        category(10, 0, &[1, 2, 3, 4, 5]),
        program(1, "10", 5001, 0),
        program(2, "0", 5002, 0),
        // The same target twice: the free one comes last and wins.
        program(3, "10", 5003, 0),
        program(4, "0", 5003, 0),
        // Another effect leaves the flag alone.
        program(5, "10", 5004, 2),
    ];
    let t = tree("apply", "research=r.cfg\n", files);
    let mut cat = Catalogue::default();
    cat.types = vec![5001, 5002, 5003, 5004, 5005]
        .into_iter()
        .map(ty)
        .collect();
    let mut avail = Availability::default();
    t.apply(&cat, &mut avail);
    let flags: Vec<bool> = (0..5).map(|i| avail.is(&cat, i)).collect();
    assert_eq!(flags, [false, true, true, true, true]);
}

#[test]
fn a_tree_takes_no_more_than_the_most_programs() {
    let lines = "program=q\n".repeat(104_000);
    let files = vec![
        list("r.cfg", &["b.cfg"]),
        branch("b.cfg", 1, 0, &[10]),
        (
            "c/10.cfg".into(),
            format!("[category]\n{lines}").into_bytes(),
        ),
        ("q".into(), b"[research]\ncost=1\n".to_vec()),
    ];
    let t = tree("most", "research=r.cfg\n", files);
    assert_eq!(t.programs().count(), MAX_PROGRAMS);
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_full_install_gives_two_branches_22_categories_and_about_120_programs_at_9_percent() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
    // Every freeform start says `research=research.cfg`.
    let map = crate::lists::freeform_list(&vfs).remove(0);
    let t = Tree::of_scenario(&vfs, &map.file, &mut || 0).unwrap();
    let categories = t.branches.iter().map(|b| b.categories.len()).sum::<usize>();
    // 118 programs, or 122 with the content update that adds four to the category 23204.
    let programs = t.programs().count();
    assert_eq!((t.branches.len(), categories), (2, 22));
    assert!([118, 122].contains(&programs), "{programs}");
    assert_eq!(t.share(), 9);
}

/// A tree of one branch at its second level (`cost`, `work` a month) with one category holding
/// programs of `(cost, order)`.
fn plain(cost: f32, work: f32, programs: &[(f32, i32)]) -> Tree {
    let program = |&(cost, order)| Program {
        name: 1,
        cost,
        progress: 0.0,
        target: 5,
        effect: 0,
        order,
    };
    let mut tree = Tree {
        branches: vec![Branch {
            name: 1,
            funding: vec![
                Level {
                    name: 1,
                    cost: 0.0,
                    work: 0.0,
                },
                Level {
                    name: 2,
                    cost,
                    work,
                },
            ],
            level: 1,
            categories: vec![Category {
                name: 1,
                expansion: 0,
                tick: true,
                programs: programs.iter().map(program).collect(),
            }],
            current: None,
        }],
        ..Tree::default()
    };
    tree.branches[0].choose(&tree.off, &mut || 0);
    tree
}

fn cash(amount: f32) -> Money {
    let mut money = Money::default();
    money.set_cash(amount);
    money
}

fn progress(t: &Tree) -> f32 {
    t.branches[0].categories[0].programs[0].progress
}

#[test]
fn the_branches_work_when_3_seconds_have_gone_and_the_accumulator_restarts_at_zero() {
    let mut t = plain(2000.0, 45.0, &[(20.0, 1)]);
    let mut money = cash(1000.0);
    assert!(t.step(2999, &mut money, &mut || 0).is_empty());
    assert_eq!((progress(&t), money.cash), (0.0, 1000.0));
    // 3000 ms are 360 minutes: pays 360 * 2000 / 43200 and adds 360 * 45 / 43200.
    t.step(1, &mut money, &mut || 0);
    let pay = 360.0_f32 * 2000.0 * 2.314_814_8e-5;
    assert_eq!(progress(&t), 360.0_f32 * 45.0 * 2.314_814_8e-5);
    assert_eq!(progress(&t), 0.375);
    assert_eq!(money.cash, 1000.0 - pay);
    assert_eq!(money.total[crate::money::RESEARCH], pay);
    // The overshoot is lost: 2999 ms later nothing happens, one more ms and the next payment.
    t.step(2999, &mut money, &mut || 0);
    assert_eq!(progress(&t), 0.375);
    t.step(1, &mut money, &mut || 0);
    assert_eq!(progress(&t), 0.75);
}

#[test]
fn a_program_of_cost_20_at_max_completes_at_the_54th_update() {
    let mut t = plain(2000.0, 45.0, &[(20.0, 1), (30.0, 2)]);
    let mut money = cash(1_000_000.0);
    for n in 1..=54 {
        let done = t.step(3000, &mut money, &mut || 0);
        assert_eq!(done.len(), usize::from(n == 54), "{n}");
    }
    assert_eq!(progress(&t), 20.25);
    // The next program is chosen at once.
    assert_eq!(t.branches[0].current, Some((0, 1)));
}

#[test]
fn every_operation_of_the_progress_is_rounded_to_a_single() {
    let mut t = plain(0.0, 45.5, &[(1e6, 1)]);
    let mut money = cash(1.0);
    for _ in 0..1000 {
        t.step(3000, &mut money, &mut || 0);
    }
    // 1000 sums of 0.37916666 held in a double give 379.1666564941406.
    assert_eq!(progress(&t), 379.170_72);
}

#[test]
fn the_product_is_rounded_to_a_single_before_the_constant_multiplies_it() {
    let mut t = plain(0.0, 0.37, &[(1.0, 1)]);
    t.step(3000, &mut cash(1.0), &mut || 0);
    // 360 * 0.37 rounds to a float first: a double chain gives 0.0030833334.
    assert_eq!(progress(&t), 0.003_083_333_1_f32);
}

#[test]
fn a_zoo_that_cannot_pay_stalls_and_a_free_level_pays_nothing() {
    let mut t = plain(2000.0, 45.0, &[(20.0, 1)]);
    let mut money = cash(16.0);
    t.step(3000, &mut money, &mut || 0);
    assert_eq!((progress(&t), money.cash), (0.0, 16.0));
    money.set_cash(-5.0);
    t.branches[0].level = 0;
    t.step(3000, &mut money, &mut || 0);
    assert_eq!(
        (
            progress(&t),
            money.cash,
            money.total[crate::money::RESEARCH]
        ),
        (0.0, -5.0, 0.0)
    );
    // A level out of range does nothing either.
    t.branches[0].level = 9;
    t.step(3000, &mut money, &mut || 0);
    assert_eq!(progress(&t), 0.0);
}

#[test]
fn a_zero_step_or_a_huge_one_never_fails() {
    let mut t = plain(2000.0, 45.0, &[(1e9, 1)]);
    let mut money = cash(1e9);
    assert!(t.step(0, &mut money, &mut || 0).is_empty());
    assert_eq!(progress(&t), 0.0);
    t.step(u32::MAX, &mut money, &mut || 0);
    t.step(u32::MAX, &mut money, &mut || 0);
    t.step(u32::MAX, &mut money, &mut || 0);
}

#[test]
fn the_current_program_is_a_started_one_else_a_draw_among_the_lowest_rank() {
    let mut t = plain(
        0.0,
        0.0,
        &[(10.0, 2), (10.0, 1), (0.0, 1), (10.0, 1), (10.0, 3)],
    );
    let off = [false; 4];
    let b = &mut t.branches[0];
    let mut draws = Vec::new();
    // Rank 1 unfinished: programs 1 and 3 (the free one is done). The draw is taken modulo 2.
    b.choose(&off, &mut || {
        draws.push(());
        3
    });
    assert_eq!((b.current, draws.len()), (Some((0, 3)), 1));
    // A started program is resumed first, with no draw.
    b.categories[0].programs[4].progress = 1.0;
    b.choose(&off, &mut || panic!("no draw"));
    assert_eq!(b.current, Some((0, 4)));
    // Nothing left: no program and no draw.
    for p in &mut b.categories[0].programs {
        p.progress = 10.0;
    }
    b.choose(&off, &mut || panic!("no draw"));
    assert_eq!(b.current, None);
}

#[test]
fn the_lowest_rank_is_the_one_of_the_branch_and_a_cleared_tick_leaves_the_competition() {
    let mut t = plain(0.0, 0.0, &[(10.0, 1)]);
    let mut second = t.branches[0].categories[0].clone();
    second.programs[0].order = 2;
    t.branches[0].categories.push(second);
    let off = [false; 4];
    let b = &mut t.branches[0];
    b.categories[1].programs[0].order = 0;
    b.choose(&off, &mut || 0);
    assert_eq!(b.current, Some((1, 0)));
    b.categories[1].tick = false;
    b.choose(&off, &mut || 0);
    assert_eq!(b.current, Some((0, 0)));
    // The update that finds its tick cleared chooses again instead of working.
    b.categories[1].tick = true;
    b.categories[0].tick = false;
    let mut money = cash(1e6);
    t.step(3000, &mut money, &mut || 0);
    assert_eq!(t.branches[0].current, Some((1, 0)));
    assert_eq!(money.cash, 1e6);
}

#[test]
fn a_category_of_a_disabled_expansion_is_skipped() {
    let mut t = plain(2000.0, 45.0, &[(10.0, 1)]);
    t.branches[0].categories[0].expansion = 1;
    t.off = [false, false, true, false];
    t.branches[0].choose(&t.off, &mut || 0);
    assert_eq!(t.branches[0].current, None);
    t.off = [false; 4];
    t.branches[0].choose(&t.off, &mut || 0);
    assert_eq!(t.branches[0].current, Some((0, 0)));
    // Huge and negative expansions are skipped by no entry.
    for e in [i32::MAX, -1] {
        t.branches[0].categories[0].expansion = e;
        t.off = [true; 4];
        assert!(!t.branches[0].categories[0].skipped(&t.off));
    }
}

#[test]
fn the_scenario_disables_the_expansions_it_does_not_have() {
    for (desc, want) in [
        ("name=16000", [false, false, true, true]),
        ("name=16015", [false, false, true, true]),
        ("name=16022", [false, false, true, true]),
        ("name=16016", [false; 4]),
        (
            "name=16027\nexpansionLocked=1\nexpansion=3",
            [false, false, true, false],
        ),
        ("name=16050\nexpansionLocked=0\nexpansion=3", [false; 4]),
        ("", [false; 4]),
    ] {
        let t = tree("off", &format!("\n[desc]\n{desc}\n"), vec![]);
        assert_eq!(t.off, want, "{desc}");
    }
}

#[test]
fn each_branch_file_read_draws_once_for_its_current_program() {
    let files = vec![
        list("research.cfg", &["b1.cfg", "b3.cfg"]),
        branch("b1.cfg", 7, 100, &[10]),
        branch("b3.cfg", 8, 100, &[11]),
        category(10, 0, &[1, 2]),
        category(11, 0, &[3]),
        program(1, "10", 5, 0),
        program(2, "10", 6, 0),
        program(3, "0", 7, 0),
    ];
    let (_i, vfs) = vfs("draws", "research=research.cfg\n", files);
    let mut draws = Vec::new();
    let mut rand = || {
        draws.push(());
        1
    };
    let t = Tree::of_scenario(&vfs, "s.scn", &mut rand).unwrap();
    // The first branch has two candidates (draw 1 picks the second); the second has none.
    assert_eq!(draws.len(), 1);
    assert_eq!(t.branches[0].current, Some((0, 1)));
    assert_eq!(t.branches[1].current, None);
}

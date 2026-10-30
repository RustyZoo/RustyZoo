use super::*;
use crate::avail::Availability;
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
    let mut t = tree("apply", "research=r.cfg\n", files);
    let mut cat = Catalogue::default();
    cat.types = vec![5001, 5002, 5003, 5004, 5005]
        .into_iter()
        .map(ty)
        .collect();
    let mut avail = Availability::default();
    for (target, on) in t.apply(&cat, &mut Overlay::default()).0 {
        avail.set(&cat, target, on);
    }
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
        vals: [0; 3],
        order,
        icon: String::new(),
        ..Default::default()
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
                ..Default::default()
            }],
            current: None,
            ..Default::default()
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
fn a_start_sets_the_selector_from_the_scenario() {
    for (desc, want) in [
        ("name=16000", Some(1)),
        ("name=16022", Some(1)),
        ("name=16027\nexpansionLocked=1\nexpansion=3", Some(3)),
        (
            "name=16027\nexpansionLocked=1\nexpansion=3\ndefaultExpansion=2",
            Some(2),
        ),
        ("expansionLocked=1", Some(0)),
        ("expansionLocked=1\ndefaultExpansion=-5", Some(-5)),
        ("name=16050\nexpansionLocked=0\nexpansion=3", None),
        ("", None),
    ] {
        let t = tree("sel", &format!("\n[desc]\n{desc}\n"), vec![]);
        assert_eq!(t.selection, want, "{desc}");
    }
}

#[test]
fn the_selector_filters_the_category_list() {
    let category = |expansion| Category {
        expansion,
        ..Category::default()
    };
    let mut t = Tree {
        branches: vec![Branch {
            categories: [0, 1, 2, i32::MAX, -1].map(category).to_vec(),
            ..Branch::default()
        }],
        ..Tree::default()
    };
    assert_eq!(t.listed(0, 0), [0, 1, 2, 3, 4], "All");
    assert_eq!(t.listed(0, 1), [0]);
    assert_eq!(t.listed(0, 3), [2]);
    assert!(t.listed(0, 4).is_empty(), "no category of that entry");
    assert!(t.listed(0, -1).is_empty());
    assert!(t.listed(0, i32::MIN).is_empty());
    // A row counts in the filtered list; an off expansion stays out of it.
    t.toggle(0, 0, 3);
    assert!(t.branches[0].categories[2].tick && !t.branches[0].categories[0].tick);
    t.off[3] = true;
    assert!(t.listed(0, 3).is_empty());
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

/// Two branches: 7 (two levels; categories 10 with programs 1, 2 and 11 with program 3) and 8
/// (category 12 with program 4). Programs 1 to 4 cost 10, 0, 20 and 30.
fn saved() -> Tree {
    let files = vec![
        list("r.cfg", &["b1.cfg", "b2.cfg"]),
        branch("b1.cfg", 7, 100, &[10, 11]),
        branch("b2.cfg", 8, 100, &[12]),
        category(10, 0, &[1, 2]),
        category(11, 0, &[3]),
        category(12, 0, &[4]),
        program(1, "10", 5001, 0),
        program(2, "0", 5002, 0),
        program(3, "20", 5003, 0),
        program(4, "30", 5004, 0),
    ];
    tree("saved", "research=r.cfg\n", files)
}

fn word(n: i32) -> [u8; 4] {
    n.to_le_bytes()
}

/// The records of a section: the leading word, the records, the end record.
fn section(records: &[Vec<u8>]) -> Vec<u8> {
    let mut o = word(0).to_vec();
    o.extend(records.concat());
    o.extend(word(-1));
    o
}

fn rec(kind: i32, name: i32, value: &[u8]) -> Vec<u8> {
    [&word(kind)[..], &word(name), value].concat()
}

fn loaded(t: &mut Tree, records: &[Vec<u8>]) {
    t.load(&section(records), &mut || 0);
}

fn levels_ticks_progress(t: &Tree) -> (Vec<usize>, Vec<bool>, Vec<f32>) {
    let cats = || t.branches.iter().flat_map(|b| &b.categories);
    (
        t.branches.iter().map(|b| b.level).collect(),
        cats().map(|c| c.tick).collect(),
        t.programs().map(|p| p.progress).collect(),
    )
}

#[test]
fn a_tree_is_written_branch_then_its_categories_then_their_programs_and_ends_with_minus_one() {
    let mut t = saved();
    t.branches[0].level = 1;
    t.branches[0].categories[1].tick = false;
    t.branches[1].categories[0].programs[0].progress = 12.5;
    let want = [
        rec(0, 7, &word(1)),
        rec(1, 10, &[1]),
        rec(2, 1, &0f32.to_le_bytes()),
        rec(2, 2, &0f32.to_le_bytes()),
        rec(1, 11, &[0]),
        rec(2, 3, &0f32.to_le_bytes()),
        rec(0, 8, &word(0)),
        rec(1, 12, &[1]),
        rec(2, 4, &12.5f32.to_le_bytes()),
    ];
    assert_eq!(t.save(), section(&want));
    // The leading word and the end record alone for an empty tree.
    assert_eq!(Tree::default().save(), section(&[]));
}

#[test]
fn what_is_written_reads_back_and_writes_again_the_same_bytes() {
    let mut t = saved();
    t.branches[0].level = 1;
    t.branches[1].categories[0].tick = false;
    t.branches[0].categories[0].programs[0].progress = 4.25;
    t.branches[0].categories[1].programs[0].progress = -3.0;
    let bytes = t.save();
    let mut back = saved();
    back.load(&bytes, &mut || 0);
    assert_eq!(levels_ticks_progress(&back), levels_ticks_progress(&t));
    assert_eq!(back.save(), bytes);
    // A started program is the current one of its branch.
    assert_eq!(back.branches[0].current, Some((0, 0)));
}

#[test]
fn a_load_resets_everything_first_and_a_file_without_a_section_leaves_the_tree_as_built() {
    let mut t = saved();
    t.branches[0].level = 1;
    t.branches[0].categories[0].tick = false;
    t.branches[0].categories[0].programs[0].progress = 9.0;
    t.load(&[], &mut || 0);
    let fresh = saved();
    assert_eq!(levels_ticks_progress(&t), levels_ticks_progress(&fresh));
    // Only the end record, or a section that is cut short, read nothing.
    t.branches[1].level = 1;
    t.load(&section(&[]), &mut || 0);
    assert_eq!(t.branches[1].level, 0);
    for cut in 0..12 {
        t.load(&section(&[rec(0, 7, &word(1))])[..cut], &mut || 0);
        assert_eq!(levels_ticks_progress(&t), levels_ticks_progress(&fresh));
    }
}

#[test]
fn an_unknown_id_is_ignored_and_the_first_match_of_a_name_is_the_one_set() {
    let files = vec![
        list("r.cfg", &["b.cfg"]),
        branch("b.cfg", 7, 0, &[10, 10]),
        category(10, 0, &[1, 1]),
        program(1, "10", 5001, 0),
    ];
    let mut t = tree("twice", "research=r.cfg\n", files);
    loaded(
        &mut t,
        &[
            rec(0, 99, &word(1)),
            rec(1, 99, &[0]),
            rec(2, 99, &1f32.to_le_bytes()),
            rec(1, 10, &[0]),
            rec(2, 1, &3f32.to_le_bytes()),
        ],
    );
    let (_, ticks, progress) = levels_ticks_progress(&t);
    assert_eq!(ticks, [false, true]);
    assert_eq!(progress, [3.0, 0.0, 0.0, 0.0]);
}

#[test]
fn a_funding_level_out_of_range_is_0() {
    // The branch has two levels.
    for (stored, want) in [
        (0, 0),
        (1, 1),
        (2, 0),
        (7, 0),
        (u32::MAX, 0),
        (i32::MIN as u32, 0),
    ] {
        let mut t = saved();
        loaded(&mut t, &[rec(0, 7, &stored.to_le_bytes())]);
        assert_eq!(t.branches[0].level, want, "{stored}");
    }
    // A branch with no level at all takes 0.
    let mut t = saved();
    t.branches[0].funding.clear();
    loaded(&mut t, &[rec(0, 7, &word(0))]);
    assert_eq!(t.branches[0].level, 0);
}

#[test]
fn a_tick_is_set_by_any_non_zero_byte_and_a_progress_is_taken_as_stored() {
    let mut t = saved();
    t.branches[0].categories[0].tick = false;
    loaded(
        &mut t,
        &[
            rec(1, 10, &[0x80]),
            rec(1, 11, &[0]),
            rec(2, 1, &(-5f32).to_le_bytes()),
            rec(2, 2, &1e30f32.to_le_bytes()),
            rec(2, 3, &f32::NAN.to_le_bytes()),
        ],
    );
    let (_, ticks, progress) = levels_ticks_progress(&t);
    assert_eq!(ticks, [true, false, true]);
    assert_eq!(progress[..2], [-5.0, 1e30]);
    assert!(progress[2].is_nan());
    // A program is done when its progress reached its cost: not a negative one, not a NaN.
    let done: Vec<bool> = t.programs().map(Program::done).collect();
    assert_eq!(done, [false, true, false, false]);
    // And what is written is what was read, a NaN included.
    assert_eq!(t.save().len(), saved().save().len());
}

#[test]
fn a_record_of_another_type_or_a_negative_one_ends_the_read() {
    for kind in [3, 100, -1, -2, i32::MIN, i32::MAX] {
        let mut t = saved();
        loaded(
            &mut t,
            &[
                rec(0, 7, &word(1)),
                rec(kind, 7, &word(1)),
                rec(0, 8, &word(1)),
            ],
        );
        let levels: Vec<usize> = t.branches.iter().map(|b| b.level).collect();
        assert_eq!(levels, [1, 0], "{kind}");
    }
}

#[test]
fn the_load_chooses_each_branchs_program_with_one_draw_when_none_was_started() {
    let mut t = saved();
    let mut draws = 0;
    t.load(&section(&[]), &mut || {
        draws += 1;
        0
    });
    // Both branches have candidates.
    assert_eq!(draws, 2);
    assert_eq!(t.branches[0].current, Some((0, 0)));
    // Branch 7 has a started program (no draw) and branch 8 is finished (no candidate, no draw).
    let mut draws = 0;
    t.load(
        &section(&[
            rec(2, 3, &1f32.to_le_bytes()),
            rec(2, 4, &30f32.to_le_bytes()),
        ]),
        &mut || {
            draws += 1;
            0
        },
    );
    assert_eq!(draws, 0);
    assert_eq!(t.branches[0].current, Some((1, 0)));
    assert_eq!(t.branches[1].current, None);
}

#[test]
fn a_loaded_tree_makes_the_targets_of_its_complete_programs_available() {
    let mut t = saved();
    loaded(&mut t, &[rec(2, 3, &20f32.to_le_bytes())]);
    let mut cat = Catalogue::default();
    cat.types = [5001, 5002, 5003, 5004].into_iter().map(ty).collect();
    let mut avail = Availability::default();
    for (target, on) in t.apply(&cat, &mut Overlay::default()).0 {
        avail.set(&cat, target, on);
    }
    let flags: Vec<bool> = (0..4).map(|i| avail.is(&cat, i)).collect();
    // Program 2 is free; program 3 reached its cost; 1 and 4 did not.
    assert_eq!(flags, [false, true, true, false]);
}

/// A saved game resumes its scenario through the start's set-up (3.5, 7.2): the same entries are
/// disabled, so the build and the load skip the category of a disabled expansion. Both kinds of
/// scenario: a base game one (both expansions off) and a second-expansion one (first expansion off).
#[test]
fn a_saved_game_resumes_with_the_expansions_disabled_as_a_start() {
    let files = vec![
        list("r.cfg", &["b.cfg"]),
        // Category 10 belongs to the first expansion, so a start and a resume both skip it.
        branch("b.cfg", 7, 0, &[10, 11]),
        category(10, 1, &[1]),
        category(11, 0, &[2]),
        program(1, "10", 5001, 0),
        program(2, "10", 5002, 0),
    ];
    for (name, desc) in [
        ("base", "name=16000\n"),
        ("locked", "expansionLocked=1\nexpansion=3\n"),
    ] {
        let start = format!("research=r.cfg\n[desc]\n{desc}");
        let (_i, vfs) = vfs(&format!("resume-{name}"), &start, files.clone());
        let begin = Tree::of_scenario(&vfs, "s.scn", &mut || 0).unwrap();
        let mut resume = Tree::of_scenario(&vfs, "s.scn", &mut || 0).unwrap();
        assert_eq!(resume.off, begin.off, "{name}: entries");
        assert_eq!(resume.branches[0].current, Some((1, 0)), "{name}: build");
        resume.load(&section(&[]), &mut || 0);
        assert_eq!(resume.branches[0].current, Some((1, 0)), "{name}: load");
    }
}

/// The research section of every shipped file that names a scenario reads and writes back the same
/// bytes a second time; the saved game of the install reads back the section it holds.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_sections_round_trip() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
    let (mut files, mut same) = (0, 0);
    for member in vfs.list(None).iter().filter(|m| m.ends_with(".zoo")) {
        let Ok(zoo) = crate::zoo::Zoo::parse(&vfs.read(member).unwrap()) else {
            continue;
        };
        let Some(scenario) = zoo.scenario_file() else {
            continue;
        };
        let Some(mut tree) = Tree::of_scenario(&vfs, &scenario, &mut || 0) else {
            continue;
        };
        tree.load(zoo.research(), &mut || 0);
        let out = tree.save();
        let mut again = Tree::of_scenario(&vfs, &scenario, &mut || 0).unwrap();
        again.load(&out, &mut || 0);
        assert_eq!(again.save(), out, "{member}: second write");
        files += 1;
        same += usize::from(out == zoo.research());
        // The saved game of the install, when it has one, is the reference of research 3.4.
        if member.to_lowercase().ends_with("saved games/test.zoo") {
            assert!(out == zoo.research(), "{member}: {} bytes", out.len());
        }
    }
    eprintln!("{files} files, {same} with the same section");
    // The saved game is in the English install only (the French one lacks it).
    assert!(files > 0);
}

#[test]
fn a_complete_effect_1_program_gives_its_building_an_upgrade_in_tree_order() {
    let up = |name: i32, cost: &str, target: i32| {
        let body = format!(
            "[research]\nname={name}\ncost={cost}\ntarget={target}\neffect=1\n\
             effectval1=200\neffectval2=-1\neffectval3=15\nentityIcon=ui/up{name}\n"
        );
        (format!("p/{name}.cfg"), body.into_bytes())
    };
    let files = vec![
        list("research.cfg", &["b.cfg"]),
        branch("b.cfg", 1, 100, &[10]),
        category(10, 0, &[1, 2, 3, 4]),
        // Free, paid, free for another type, and an effect 0 program.
        up(1, "0", 7),
        up(2, "50", 7),
        up(3, "0", 8),
        program(4, "0", 7, 0),
    ];
    let mut t = tree("upgrades", "research=research.cfg\n", files);
    let free = |t: &Tree| {
        t.upgrades()
            .into_iter()
            .map(|(b, u)| (b, u.name))
            .collect::<Vec<_>>()
    };
    assert_eq!(free(&t), [(7, 1), (8, 3)]);
    let (target, u) = t.upgrade_of(1).unwrap();
    assert_eq!(
        (target, u.icon.as_str(), u.upkeep, u.adult, u.child),
        (7, "ui/up1", 200, -1, 15)
    );
    // Neither a program of another effect nor an unknown one gives an upgrade.
    assert!(t.upgrade_of(4).is_none() && t.upgrade_of(99).is_none());
    // The paid one counts once its progress reaches its cost, in tree order.
    t.branches[0].categories[0].programs[1].progress = 50.0;
    assert_eq!(free(&t), [(7, 1), (7, 2), (8, 3)]);
}

#[test]
fn the_pictures_and_the_help_strings_are_read_from_the_files() {
    let files = vec![
        list("research.cfg", &["b.cfg"]),
        (
            "b.cfg".into(),
            b"[branch]\nname=7\nnoprogicon=ui/n/n\ncategory=c/1.cfg\n".to_vec(),
        ),
        (
            "c/1.cfg".into(),
            b"[category]\nname=1\nhelpid=4500\nprogram=p/1.cfg\n".to_vec(),
        ),
        (
            "p/1.cfg".into(),
            b"[research]\nname=1\ncost=5\nicon=ui/a/a\nentityIcon=ui/e/e\nhelpid=4501\n".to_vec(),
        ),
    ];
    let t = tree("pictures", "research=research.cfg\n", files);
    let b = &t.branches[0];
    assert_eq!((b.no_icon.as_str(), b.categories[0].help), ("ui/n/n", 4500));
    let p = &b.categories[0].programs[0];
    assert_eq!(
        (p.picture.as_str(), p.icon.as_str(), p.help),
        ("ui/a/a", "ui/e/e", 4501)
    );
}

#[test]
fn the_panel_reads_the_days_the_percent_the_funding_and_the_next_levels() {
    // A program of cost 90 at max (cost 2000, work 45 a month): 60 days, all of it to do.
    let mut t = plain(2000.0, 45.0, &[(90.0, 1)]);
    let b = &t.branches[0];
    assert_eq!(
        (b.days_remaining(), b.percent_remaining()),
        (Some(60.0), Some(100))
    );
    assert_eq!(b.funding_text(), Some((2, 67)));
    assert_eq!((b.increase(), b.decrease()), (1, 0));
    t.branches[0].categories[0].programs[0].progress = 45.0;
    let b = &t.branches[0];
    assert_eq!(
        (b.days_remaining(), b.percent_remaining()),
        (Some(30.0), Some(50))
    );
    // Nothing is funded at level 0: no days, no percent, and the level can go up only.
    t.fund(0, false);
    let b = &t.branches[0];
    assert_eq!(
        (b.days_remaining(), b.percent_remaining(), b.level),
        (None, None, 0)
    );
    assert_eq!((b.increase(), b.decrease()), (1, 0));
    // A level out of range (a mod's file) shows no text and does not panic.
    t.branches[0].level = 9;
    let b = &t.branches[0];
    assert_eq!((b.days_remaining(), b.funding_text()), (None, None));
    assert_eq!((b.increase(), b.decrease()), (1, 8));
    // No program, no levels.
    let b = Branch::default();
    assert_eq!(
        (
            b.days_remaining(),
            b.percent_remaining(),
            b.increase(),
            b.decrease()
        ),
        (None, None, 0, 0)
    );
}

#[test]
fn a_funding_button_steps_the_level_and_a_row_flips_the_tick_of_a_listed_category() {
    let mut t = plain(2000.0, 45.0, &[(90.0, 1)]);
    t.branches[0].categories.push(Category {
        name: 2,
        expansion: 1,
        tick: true,
        ..Default::default()
    });
    t.fund(0, false);
    assert_eq!(t.branches[0].level, 0);
    t.fund(0, true);
    t.fund(0, true);
    assert_eq!(t.branches[0].level, 1);
    // A branch that is not there changes nothing.
    t.fund(7, true);
    assert_eq!(t.listed(0, 0), [0, 1]);
    t.toggle(0, 1, 0);
    assert_eq!(
        t.branches[0]
            .categories
            .iter()
            .map(|c| c.tick)
            .collect::<Vec<_>>(),
        [true, false]
    );
    t.toggle(0, 1, 0);
    t.toggle(0, 0, 0);
    assert!(!t.branches[0].categories[0].tick);
    // A row out of the list, a branch that is not there: nothing.
    for (b, row) in [(0, 2), (0, usize::MAX), (3, 0)] {
        t.toggle(b, row, 0);
    }
    assert_eq!(
        t.branches[0]
            .categories
            .iter()
            .map(|c| c.tick)
            .collect::<Vec<_>>(),
        [false, true]
    );
    // The category of a disabled expansion is not listed, so the rows count the others only.
    t.off[2] = true;
    assert_eq!(t.listed(0, 0), [0]);
    t.toggle(0, 1, 0);
    assert!(t.branches[0].categories[1].tick);
    assert!(t.listed(5, 0).is_empty());
}

#[test]
fn a_ticked_off_category_stops_the_work_at_the_next_update() {
    let mut t = plain(2000.0, 45.0, &[(20.0, 1)]);
    let mut money = cash(1000.0);
    t.toggle(0, 0, 0);
    t.step(3000, &mut money, &mut || 0);
    assert_eq!((progress(&t), money.cash), (0.0, 1000.0));
    assert_eq!(
        t.branches[0].current, None,
        "no category is eligible: no program"
    );
    assert!(t.branch_of(1).is_some() && t.branch_of(99).is_none());
}

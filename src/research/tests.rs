use super::*;
use crate::config::Config;
use crate::entity::EntityType;
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
    Tree::of_scenario(&vfs, "s.scn").unwrap()
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
    assert!(Tree::of_scenario(&vfs, "none.scn").is_none());
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
    let t = Tree::of_scenario(&vfs, &map.file).unwrap();
    let categories = t.branches.iter().map(|b| b.categories.len()).sum::<usize>();
    // 118 programs, or 122 with the content update that adds four to Animal Houses.
    let programs = t.programs().count();
    assert_eq!((t.branches.len(), categories), (2, 22));
    assert!([118, 122].contains(&programs), "{programs}");
    assert_eq!(t.share(), 9);
}

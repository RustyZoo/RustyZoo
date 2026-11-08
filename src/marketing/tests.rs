use super::*;
use crate::fixtures::Install;
use crate::money::NET;

const LEVELS: &str = "[marketing]\nname=4050\nfunding=none\nfunding=min\nfunding=normal\nfunding=max\n\
    [none]\nname=23100\ncost=0\nbenefit=0\n[min]\nname=23101\ncost=200\nbenefit=5\n\
    [normal]\nname=23102\ncost=500\nbenefit=15\n[max]\nname=23103\ncost=1000\nbenefit=30\n";

/// The marketing the set-up makes from a scenario with these `[start]` lines and these files.
fn setup(name: &str, start: &str, files: &[(&str, &str)]) -> Marketing {
    let scn = format!("[start]\n{start}");
    let mut members = vec![("s.scn", scn.into_bytes())];
    members.extend(files.iter().map(|(n, c)| (*n, c.as_bytes().to_vec())));
    let install = Install::new(&format!("marketing-{name}"), &members);
    let vfs = Vfs::open(&install.0).unwrap();
    Marketing::of_scenario(&vfs, "s.scn")
}

fn shipped() -> Marketing {
    let files = [
        ("mktg.cfg", "[marketing]\nmarketing=mktgnorm.cfg\n"),
        ("mktgnorm.cfg", LEVELS),
    ];
    setup("shipped", "marketing=mktg.cfg\n", &files)
}

fn at(level: usize) -> Marketing {
    let mut m = shipped();
    (0..level).for_each(|_| m.fund(true));
    m
}

fn money(cash: f32) -> Money {
    Money {
        cash,
        ..Money::default()
    }
}

#[test]
fn the_set_up_reads_the_levels_in_file_order_at_level_0() {
    let m = shipped();
    let c = m.config().unwrap();
    assert_eq!(c.level, 0);
    let got: Vec<_> = c.levels.iter().map(|l| (l.name, l.cost, l.benefit)).collect();
    assert_eq!(
        got,
        [(23100, 0.0, 0), (23101, 200.0, 5), (23102, 500.0, 15), (23103, 1000.0, 30)]
    );
}

#[test]
fn a_missing_key_or_section_leaves_0_and_a_duplicate_stays() {
    let level = "[marketing]\nfunding=a\nfunding=ghost\nfunding=a\n[a]\ncost=12.5\nbenefit=zz\n";
    let files = [("m.cfg", "[marketing]\nmarketing=l.cfg\n"), ("l.cfg", level)];
    let m = setup("holes", "marketing=m.cfg\n", &files);
    let levels = &m.config().unwrap().levels;
    let a = Level {
        name: 0,
        cost: 12.5,
        benefit: 0,
    };
    let ghost = Level {
        name: 0,
        cost: 0.0,
        benefit: 0,
    };
    assert_eq!(levels, &[a.clone(), ghost, a]);
}

#[test]
fn no_key_or_an_unreadable_outer_file_makes_no_configuration() {
    assert_eq!(setup("nokey", "research=x.cfg\n", &[]), Marketing::default());
    assert_eq!(setup("noouter", "marketing=ghost.cfg\n", &[]), Marketing::default());
    assert_eq!(setup("empty", "marketing=\n", &[]), Marketing::default());
    // Without a configuration nothing is charged and nothing is added to the rating.
    let (mut m, mut cash) = (Marketing::default(), money(100.0));
    m.step(100_000, &mut cash);
    m.fund(true);
    assert_eq!((cash.cash, m.benefit(), m.saved()), (100.0, 0, 0));
}

#[test]
fn an_unreadable_level_file_leaves_an_empty_list_that_does_nothing() {
    let files = [("m.cfg", "[marketing]\nmarketing=ghost.cfg\n")];
    let mut m = setup("nolevels", "marketing=m.cfg\n", &files);
    assert_eq!(m.config(), Some(&Funding::default()));
    let mut cash = money(100.0);
    m.step(100_000, &mut cash);
    for up in [true, false] {
        m.fund(up);
        assert_eq!((m.config().unwrap().level, m.benefit()), (0, 0));
    }
    assert_eq!(cash.cash, 100.0);
    // A file with no `marketing` key at all is the same.
    let m = setup("nokeyinside", "marketing=m.cfg\n", &[("m.cfg", "[x]\n")]);
    assert_eq!(m.config(), Some(&Funding::default()));
}

#[test]
fn every_3_seconds_the_level_is_paid_in_the_marketing_category() {
    let mut m = at(3);
    let mut cash = money(10_000.0);
    // 91 steps of 33 ms reach 3003 ms: 360 minutes, 1000 / 120 at max.
    for _ in 0..90 {
        m.step(33, &mut cash);
    }
    assert_eq!(cash.cash, 10_000.0);
    m.step(33, &mut cash);
    let pay = 360.0f32 * 1000.0 * PER_MINUTE;
    assert!((pay - 8.333).abs() < 1e-3);
    assert_eq!(cash.cash, 10_000.0 - pay);
    let month = cash.month as usize - 1;
    assert_eq!(cash.monthly[MARKETING][month], pay);
    assert_eq!((cash.total[MARKETING], cash.total[NET]), (pay, -pay));
    // The clock restarted: 90 more steps pay nothing, the 91st pays.
    for _ in 0..90 {
        m.step(33, &mut cash);
    }
    assert_eq!(cash.total[MARKETING], pay);
    m.step(33, &mut cash);
    assert_eq!(cash.total[MARKETING], pay + pay);
}

#[test]
fn the_overshoot_of_the_step_is_lost_and_pays_more() {
    let mut m = at(3);
    let mut cash = money(10_000.0);
    m.step(2999, &mut cash);
    assert_eq!(cash.total[MARKETING], 0.0);
    // 200 ms more: 3199 ms, 383 minutes, and the 199 ms over are not kept.
    m.step(200, &mut cash);
    let pay = 383.0f32 * 1000.0 * PER_MINUTE;
    assert!((pay - 8.866).abs() < 1e-3);
    assert_eq!(cash.total[MARKETING], pay);
    m.step(2999, &mut cash);
    assert_eq!(cash.total[MARKETING], pay);
}

#[test]
fn a_share_the_cash_cannot_cover_is_skipped_and_never_owed() {
    let mut m = at(1);
    let mut cash = money(4.0);
    let pay = 360.0f32 * 200.0 * PER_MINUTE;
    for _ in 0..5 {
        m.step(3000, &mut cash);
    }
    // 4.00 pays two shares of 1.6667; the 0.6667 left covers no third one.
    assert_eq!(cash.cash, 4.0 - pay - pay);
    assert_eq!(cash.total[MARKETING], pay + pay);
    // Income comes in: the next share is one share, not the ones that were skipped.
    cash.cash += 100.0;
    let before = cash.cash;
    m.step(3000, &mut cash);
    assert_eq!(cash.cash, before - pay);
    // A cash of exactly the share pays it; one cent less does not.
    let mut cash = money(pay);
    m.step(3000, &mut cash);
    assert_eq!(cash.cash, 0.0);
    let mut cash = money(pay - 0.01);
    m.step(3000, &mut cash);
    assert_eq!(cash.cash, pay - 0.01);
}

#[test]
fn the_benefit_counts_whether_or_not_the_shares_are_paid() {
    let mut m = at(2);
    let mut cash = money(0.0);
    m.step(3000, &mut cash);
    assert_eq!((cash.total[MARKETING], m.benefit()), (0.0, 15));
    assert_eq!((at(0).benefit(), at(1).benefit(), at(3).benefit()), (0, 5, 30));
}

#[test]
fn level_0_and_odd_costs_book_as_the_values_say() {
    let mut cash = money(10.0);
    let mut m = at(0);
    m.step(3000, &mut cash);
    assert_eq!((cash.cash, cash.total[MARKETING]), (10.0, 0.0));
    // A negative cash pays nothing, whatever the level.
    let mut cash = money(-1.0);
    m.step(3000, &mut cash);
    assert_eq!((cash.cash, cash.total[MARKETING]), (-1.0, 0.0));
    // A negative cost gives money; an infinite or a not-a-number cost books nothing.
    let make = |cost: &str| {
        let level = format!("[marketing]\nfunding=a\n[a]\ncost={cost}\nbenefit=1\n");
        let files = [("m.cfg", "[marketing]\nmarketing=l.cfg\n"), ("l.cfg", &level)];
        setup("cost", "marketing=m.cfg\n", &files)
    };
    let mut cash = money(10.0);
    make("-1000").step(3000, &mut cash);
    assert!(cash.cash > 10.0);
    for cost in ["1e99", "inf", "nan", "1e400"] {
        let mut cash = money(10.0);
        make(cost).step(3000, &mut cash);
        assert_eq!((cash.cash, cash.total[MARKETING]), (10.0, 0.0), "{cost}");
    }
}

#[test]
fn the_clock_wraps_like_32_bits_and_a_step_of_0_changes_nothing() {
    let mut m = at(3);
    let mut cash = money(1e9);
    m.step(0, &mut cash);
    assert_eq!(cash.cash, 1e9);
    // A huge step: acc * 7200 wraps, no panic whatever it gives.
    for ms in [u32::MAX, 1 << 31, 600_000, 1] {
        m.step(ms, &mut cash);
    }
    assert!(cash.cash.is_finite());
}

#[test]
fn the_buttons_move_one_level_and_stop_at_the_ends() {
    let mut m = shipped();
    let levels: Vec<_> = [false, true, true, true, true, true, false, false, false, false, false]
        .into_iter()
        .map(|up| {
            m.fund(up);
            m.saved()
        })
        .collect();
    assert_eq!(levels, [0, 1, 2, 3, 3, 3, 2, 1, 0, 0, 0]);
}

#[test]
fn the_file_word_is_the_level_and_a_level_out_of_range_reads_as_0() {
    assert_eq!(at(2).saved(), 2);
    for (word, want) in [(0, 0), (3, 3), (4, 0), (u32::MAX, 0), (1 << 31, 0)] {
        let mut m = shipped();
        m.load(word);
        assert_eq!(m.saved(), want, "{word}");
    }
    // With no configuration the word is dropped and 0 is saved; an empty list is 0 too.
    let mut m = Marketing::default();
    m.load(2);
    assert_eq!((m.config(), m.saved()), (None, 0));
    let files = [("m.cfg", "[marketing]\nmarketing=ghost.cfg\n")];
    let mut m = setup("emptyload", "marketing=m.cfg\n", &files);
    m.load(1);
    assert_eq!(m.saved(), 0);
}

#[test]
fn a_set_up_starts_the_clock_and_the_level_over() {
    let mut cash = money(10_000.0);
    let mut old = at(3);
    old.step(2000, &mut cash);
    assert_eq!((old.saved(), old.acc), (3, 2000));
    let fresh = shipped();
    assert_eq!((fresh.saved(), fresh.acc), (0, 0));
}

//! Trigger 10 and the -1 hint inside a group, in the order of the pass (`scenarios.md` 8.6).

use super::*;

fn build(text: &str) -> Scenario {
    Scenario::build("s.scn", &Config::parse(text.as_bytes()).unwrap(), &[])
}

/// A simple goal on rule 99 that completes at the first pass.
fn done(name: &str, extra: &str) -> String {
    format!("[{name}]\nrulea=99\narga=1\nvalue=1\n{extra}\n")
}

/// A simple goal that stays in progress.
fn open(name: &str, text: i32) -> String {
    format!("[{name}]\nrulea=99\narga=0\nvalue=1\ntext={text}\n")
}

const POPUP: &str = "[9]\nlayout=45200\n\
    [default]\nlayout=45100\npersistentTextLayout=45400\npersistentShortText=-1\n";
const SHOW: &str = "trulea=2\ntruleb=0\ntarga=9";

/// One pass; the text of the line the popup of the pass took.
fn hint(goals: &str, sections: &str) -> Option<i32> {
    let mut s = build(&format!(
        "[duration]\nnummonths=-1\n[goals]\n{goals}{sections}{POPUP}"
    ));
    s.update(PERIOD, &Env::new(1, 0, None), &mut Vec::new());
    let [Effect::Popup(p)] = &s.effects()[..] else {
        panic!("one popup")
    };
    p.hint.goal.map(|l| l.text)
}

#[test]
fn trigger_10_is_one_item_of_the_queue_between_the_triggers_before_and_after_it() {
    let avail = |b: i32| format!("trulea=4\ntruleb={b}\ntarga=5");
    let text = format!(
        "[goals]\ngoal=a\ngoal=b\ngoal=c\n{}{}{}",
        done("a", &avail(1)),
        done("b", "trulea=10\ntarga=1"),
        done("c", &avail(0)),
    );
    let mut s = build(&text);
    s.update(PERIOD, &Env::new(1, 0, None), &mut Vec::new());
    let mut seen = Vec::new();
    while let Some(act) = s.next_act() {
        seen.push(match act {
            Act::Avail(a) => format!("avail {}", a[0].on),
            Act::Forced(p) => format!("forced {p}"),
            other => format!("{other:?}"),
        });
    }
    assert_eq!(seen, ["avail false", "forced true", "avail true"]);
    // A load takes it out and leaves the rest.
    let mut s = build(&text);
    s.resume(&[0, 1, 1, 1], true, &mut Vec::new());
    assert_eq!(s.gui().take_forced(), [true]);
    assert!(matches!(s.next_act(), Some(Act::Avail(_))));
    assert!(matches!(s.next_act(), Some(Act::Avail(_))));
    assert!(s.next_act().is_none());
}

#[test]
fn the_hint_of_a_child_reads_the_children_as_they_are_at_its_trigger() {
    // Chain k0, k1, k2: k0 and k1 complete in the same update, k2 stays. The popup of k0 reads k1
    // (still in progress then), not k2 where the chain stops.
    let chain = || {
        format!(
            "{}{}{}[g]\nrtype=2\nchild=k0\nchild=k1\nchild=k2\n",
            done("k0", &format!("{SHOW}\ntext=10")),
            done("k1", "text=11"),
            open("k2", 12),
        )
    };
    assert_eq!(hint("goal=g\n", &chain()), Some(11));
    // The popup of k1 reads k2.
    let at_k1 = chain().replace(&format!("{SHOW}\ntext=10"), "text=10");
    let at_k1 = at_k1.replace("[k1]\n", &format!("[k1]\n{SHOW}\n"));
    assert_eq!(hint("goal=g\n", &at_k1), Some(12));
    // A complex group: the sibling after the child is still in progress, whatever follows.
    let group = chain().replace("rtype=2", "rtype=1");
    assert_eq!(hint("goal=g\n", &group), Some(11));
    // The child whose trigger runs has its new state: a group of that one child has no text left,
    // and the walk goes on to the next top-level goal.
    let single = format!(
        "{}[g]\nrtype=1\nchild=k0\n{}",
        done("k0", &format!("{SHOW}\ntext=10")),
        open("z", 20)
    );
    assert_eq!(hint("goal=g\ngoal=z\n", &single), Some(20));
    // A goal after the group that completes later in the pass still has its old state then.
    let later = format!("{single}{}", done("a", "text=30"));
    assert_eq!(hint("goal=g\ngoal=a\ngoal=z\n", &later), Some(30));
}

#[test]
fn the_hint_sees_the_goals_before_the_group_with_their_new_states_and_the_ones_after_with_the_old() {
    // `a` before the group completes in this pass, so the walk passes it; `z` after the group is
    // read with its previous state, in progress.
    let sections = format!(
        "{}[g]\nrtype=1\nchild=k0\nchild=k1\n{}{}{}",
        done("k0", &format!("{SHOW}\ntext=10")),
        open("k1", 11),
        done("a", "text=30"),
        open("z", 20),
    );
    assert_eq!(hint("goal=a\ngoal=g\ngoal=z\n", &sections), Some(11));
    // With the group first, the group's own old state 0 is read: k1 is its first child in progress.
    assert_eq!(hint("goal=g\ngoal=a\ngoal=z\n", &sections), Some(11));
}

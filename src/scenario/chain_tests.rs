//! The chain goals of the Marine Mania award file (`awards.md` 6.3) on invented ids: the
//! condition, the award, the cash, the popup and the extras of one chain run in one pass, in
//! the order of the file, once.

use super::*;
use crate::entity::Catalogue;
use crate::money::Money;
use crate::status::zoo::Status;
use crate::unit::testing::world_with;

/// One chain in the shape of the file: a complex condition of `rules`, the award goal, then
/// `after` (rule 99 goals with a trigger, as `trulea truleb targa`), all hidden and optional.
fn chain(name: &str, rules: &[&str], award: i32, after: &[&str]) -> String {
    let kids = |n: usize| {
        (0..n)
            .map(|i| format!("child={name}{i}\n"))
            .collect::<String>()
    };
    let tail: String = after
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let [a, b, c] = t.split(' ').collect::<Vec<_>>()[..] else {
                panic!("{t}")
            };
            format!(
                "[{name}_t{i}]\nrulea=99\nhidden=1\noptional=0\n\
                 trulea={a}\ntruleb={b}\ntarga={c}\n"
            )
        })
        .collect();
    let after_kids: String = (0..after.len())
        .map(|i| format!("child={name}_t{i}\n"))
        .collect();
    let conds: String = rules
        .iter()
        .enumerate()
        .map(|(i, r)| format!("[{name}{i}]\n{r}\nsticky=0\nhidden=1\noptional=0\n"))
        .collect();
    format!(
        "[{name}]\nrtype=2\noptional=1\nchild={name}_rule\nchild={name}_award\n{after_kids}\
         [{name}_rule]\nrtype=1\n{}{conds}\
         [{name}_award]\nrulea=99\nhidden=1\noptional=0\ntrulea=6\ntruleb=0\ntarga={award}\n{tail}",
        kids(rules.len())
    )
}

/// A scenario file with an unwinnable main goal (so the optional chains never end it), the
/// chains, and a popup section per award.
fn scenario(chains: &[String], names: &[&str]) -> Scenario {
    let goals: String = names.iter().map(|n| format!("goal={n}\n")).collect();
    let text = format!(
        "[duration]\nnummonths=-1\n[goals]\ngoal=open\n{goals}\
         [open]\nrulea=99\narga=0\ntype=0\nvalue=1\n{}\
         [default]\nlayout=45200\n[7]\nlongText1=X\n[8]\nlongText1=Y\n",
        chains.concat()
    );
    Scenario::build("s.scn", &Config::parse(text.as_bytes()).unwrap(), &[])
}

/// One pass at this month with these awards held, and what it left, in order, as words.
fn once(s: &mut Scenario, month: i32, held: &[i32], rewards: &mut Vec<i32>) -> Vec<String> {
    let mut world = world_with((4, 4), |z| z.awards = held.to_vec());
    let (cat, money, status) = (Catalogue::default(), Money::default(), Status::default());
    let game = Game {
        world: &world,
        cat: &cat,
        money: &money,
        status: &status,
    };
    s.update(PERIOD, &Env::new(month, 0, Some(game)), &mut Vec::new());
    world.zoo.awards.clear();
    let mut seen = Vec::new();
    while let Some(act) = s.next_act() {
        match act {
            Act::Deed(deeds) => seen.extend(deeds.iter().map(|d| match d {
                Deed::Award { id, .. } => format!("award {id}"),
                Deed::Cash { how, amount, .. } => format!("cash {how} {amount}"),
                Deed::Boxed { sub, name, .. } => format!("box {} {name}", *sub as char),
                other => format!("{other:?}"),
            })),
            Act::Effects(effects) => seen.extend(effects.iter().map(|e| match e {
                Effect::Popup(p) => format!("popup {}", p.layout),
                other => format!("{other:?}"),
            })),
            Act::Avail(made) => {
                for a in made {
                    if let Which::Unlock(id) = a.which {
                        rewards.push(id);
                        seen.push(format!("unlock {id}"));
                    }
                }
            }
        }
    }
    seen
}

#[test]
fn a_chain_gives_its_award_cash_popup_and_extras_in_one_pass_in_file_order_and_once() {
    let held = "rulea=1\nruleb=11\nvalue=2";
    let c = chain(
        "g",
        &[held, "rulea=7\nruleb=0\nvalue=1"],
        7,
        &["3 0 500", "2 0 7", "5 0 900", "5 1 900", "4 10 9100"],
    );
    let mut s = scenario(&[c], &["g"]);
    let mut rewards = Vec::new();
    // One award held: the condition is short of it, the chain stops at it.
    assert!(once(&mut s, 1, &[3], &mut rewards).is_empty());
    let words = [
        "award 7",
        "cash 0 500",
        "popup 45200",
        "box m 900",
        "box f 900",
        "unlock 9100",
    ];
    assert_eq!(once(&mut s, 1, &[3, 4], &mut rewards), words);
    assert_eq!(rewards, [9100]);
    // The condition falls back (month 0) and holds again: the goals after it keep their state.
    assert!(once(&mut s, 0, &[3, 4], &mut rewards).is_empty());
    assert!(once(&mut s, 1, &[3, 4], &mut rewards).is_empty());
}

#[test]
fn a_chain_counts_the_awards_a_chain_before_it_gave_in_the_same_pass() {
    let always = "rulea=99\narga=1\nvalue=1";
    let first = chain("a", &[always], 7, &["3 0 100"]);
    // Two held, one given by `a`: the count reaches 3 in this pass.
    let second = chain("b", &["rulea=1\nruleb=11\nvalue=3"], 8, &["3 0 200"]);
    // An award given again that is held already is counted once: 1, 2, 7 and 8 are four.
    let again = chain("d", &[always], 2, &["3 0 400"]);
    let third = chain("c", &["rulea=1\nruleb=11\nvalue=5"], 9, &["3 0 300"]);
    let mut s = scenario(&[first, second, again, third], &["a", "b", "d", "c"]);
    let words = [
        "award 7",
        "cash 0 100",
        "award 8",
        "cash 0 200",
        "award 2",
        "cash 0 400",
    ];
    assert_eq!(once(&mut s, 1, &[1, 2], &mut Vec::new()), words);
    // The first chain's award is held already; the next pass has nothing to give.
    assert!(once(&mut s, 1, &[1, 2, 7], &mut Vec::new()).is_empty());
}

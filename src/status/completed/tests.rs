use super::*;
use crate::research::{Branch, Category, Program};

fn program(name: i32, cost: f32, progress: f32) -> Program {
    Program {
        name,
        cost,
        progress,
        picture: format!("ui/p{name}"),
        ..Program::default()
    }
}

fn tree(programs: Vec<Program>) -> Tree {
    Tree {
        branches: vec![Branch {
            categories: vec![Category {
                programs,
                ..Category::default()
            }],
            ..Branch::default()
        }],
        ..Tree::default()
    }
}

fn names(list: &Completed) -> Vec<i32> {
    list.rows.iter().map(|r| r.0).collect()
}

#[test]
fn a_completion_is_added_on_top_with_the_icon_of_its_program() {
    let tree = tree(vec![program(1, 10.0, 10.0), program(2, 10.0, 10.0)]);
    let mut list = Completed::default();
    list.add(&tree, 1);
    list.add(&tree, 2);
    assert_eq!(names(&list), [2, 1]);
    assert_eq!(list.rows[0].1, "ui/p2");
    assert!(list.changed);
}

#[test]
fn a_program_of_cost_0_or_one_already_listed_or_unknown_adds_nothing() {
    let tree = tree(vec![program(1, 0.0, 0.0), program(2, 10.0, 10.0)]);
    let mut list = Completed::default();
    list.add(&tree, 1);
    list.add(&tree, 99);
    assert!(list.rows.is_empty() && !list.changed);
    list.add(&tree, 2);
    list.changed = false;
    list.add(&tree, 2);
    assert_eq!((names(&list), list.changed), (vec![2], false));
}

#[test]
fn a_game_that_starts_lists_the_programs_done_with_the_last_read_on_top() {
    let tree = tree(vec![
        program(1, 10.0, 10.0),
        program(2, 10.0, 3.0),
        program(3, 10.0, 12.0),
        // Done from the start, with no cost: never listed.
        program(4, 0.0, 0.0),
    ]);
    let list = Completed::replay(&tree);
    assert_eq!(names(&list), [3, 1]);
    assert!(list.changed, "the list is written even when empty");
    // The programs of an earlier game are gone.
    assert!(Completed::replay(&Tree::default()).rows.is_empty());
}

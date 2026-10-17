use super::*;

const NAMED: i32 = 9500;

fn unit(kind: Kind, task: u8, job: u8, tile: Option<usize>, job_target: u32) -> Unit {
    let mut u = Unit::new(1, kind);
    let r = &mut u.goal.record;
    (r.task, r.job, r.target.tile, r.job_target.id) = (task, job, tile, job_target);
    u
}

/// The text of a unit: tile 1 is in the exhibit "Pen", tile 2 on the grounds, any other has none;
/// entity 7 is an egg, 8 an animal, anything else no animal.
fn text(u: &Unit, type_id: i32) -> Option<(i32, Option<String>)> {
    let at = |t: Option<usize>| match t {
        Some(1) => Place::Exhibit("Pen".into()),
        Some(2) => Place::Grounds,
        _ => Place::None,
    };
    let animal = |id: u32| match id {
        7 => Some(true),
        8 => Some(false),
        _ => None,
    };
    duty(u, type_id, at, animal)
}

fn keeper(task: u8, job: u8, tile: Option<usize>, target: u32) -> Option<(i32, Option<String>)> {
    text(&unit(Kind::Keeper, task, job, tile, target), NAMED)
}

#[test]
fn a_keepers_text_follows_the_job_first_and_then_the_task() {
    let plain = |id| Some((id, None));
    assert_eq!(keeper(0x6e, 1, None, 0), plain(10300));
    assert_eq!(keeper(0x6e, 2, None, 0), plain(10300));
    assert_eq!(keeper(0x6e, 3, None, 8), plain(10301), "an animal to heal");
    assert_eq!(keeper(0x6e, 4, None, 7), plain(10321), "an egg");
    assert_eq!(keeper(0x6e, 3, None, 9), None, "no such animal");
    assert_eq!(keeper(0x6e, 5, None, 0), plain(10302));
    assert_eq!(keeper(0x6e, 6, None, 0), plain(10302));
    assert_eq!(keeper(0x6e, 24, None, 0), plain(10327));
    assert_eq!(keeper(0x6e, 25, None, 0), plain(10327));
    // The scientist rakes "poo" under another text.
    let scientist = unit(Kind::Keeper, 0x6e, 5, None, 0);
    assert_eq!(text(&scientist, 9551), plain(10514));
    // A job wins over the task of his round.
    assert_eq!(keeper(0x65, 1, Some(1), 0), plain(10300));
}

#[test]
fn a_keepers_walk_names_the_exhibit_and_a_wander_monitors() {
    let named = |id| Some((id, Some("Pen".to_string())));
    assert_eq!(keeper(0x65, 0, Some(1), 0), named(10303));
    assert_eq!(keeper(0x7e, 0, Some(1), 0), named(10303));
    assert_eq!(
        keeper(0x65, 0, Some(2), 0),
        Some((10306, None)),
        "the grounds"
    );
    assert_eq!(keeper(0x65, 0, None, 0), None, "no exhibit there");
    assert_eq!(keeper(0x65, 0, Some(99), 0), None, "a tile that is none");
    assert_eq!(keeper(0x7f, 0, None, 0), Some((10306, None)));
    assert_eq!(keeper(1, 0, None, 0), Some((10304, None)));
    // Between two jobs, in his duties, and with no task, he shows nothing.
    assert_eq!(keeper(0x6e, 0, None, 0), None);
    assert_eq!(keeper(0, 0, None, 0), None);
}

#[test]
fn a_workers_text_is_the_job_or_monitoring_with_none() {
    let worker = |task, job| text(&unit(Kind::Worker, task, job, None, 0), 9501);
    for (job, id) in [
        (7, 10500),
        (8, 10501),
        (9, 10502),
        (10, 10503),
        (11, 10504),
        (12, 10505),
    ] {
        assert_eq!(worker(0x78, job), Some((id, None)), "job {job}");
    }
    for (job, id) in [(16, 10509), (17, 10510), (26, 10515), (27, 10516)] {
        assert_eq!(worker(0x78, job), Some((id, None)), "job {job}");
    }
    assert_eq!(worker(1, 0), Some((10506, None)));
    assert_eq!(worker(0x78, 0), None);
    assert_eq!(worker(0, 0), None);
    assert_eq!(worker(1, 3), None, "a job of a keeper");
    assert_eq!(worker(1, u8::MAX), None);
}

#[test]
fn a_guides_text_is_going_speaking_or_monitoring() {
    let guide = |task, tile| text(&unit(Kind::Guide, task, 0, tile, 0), 9502);
    assert_eq!(guide(0x76, Some(1)), Some((10400, Some("Pen".into()))));
    assert_eq!(guide(0x76, Some(2)), None, "the grounds");
    assert_eq!(guide(0x76, None), None);
    assert_eq!(guide(0x77, None), Some((10401, None)));
    assert_eq!(guide(1, None), Some((10402, None)));
    assert_eq!(guide(0, None), Some((10402, None)));
}

#[test]
fn units_that_are_not_staff_have_no_text() {
    assert_eq!(text(&unit(Kind::Animal, 1, 0, None, 0), 0), None);
    assert_eq!(text(&unit(Kind::Guest, 1, 0, None, 0), 0), None);
}

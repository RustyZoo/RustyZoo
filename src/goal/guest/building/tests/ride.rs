//! A guest out of a return building (guest buildings 12.2, goals 11.13) on a lake: a boat shack, a
//! dolphin shack and a changing room 9 at (5, 7), whose exits are on the water at (5, 6), a lake
//! of 5 by 4 tiles (x 3 to 7, y 3 to 6), and a stand 7 at (1, 1) on the land. Guest 1 stands in
//! the water at (5, 3). As in the shipped files, the exit on the water lies at the building's
//! side opposite to its rotation, so the tile next to the exit in the rotation is the building.

use super::*;
use crate::behaviour::{Call, Heard, Row, Timing};
use crate::building::{Live, ret};
use crate::goal::{NONE, RECOVER, RETURN_TO_BUILDING};
use crate::guest::NO_SLOT;
use crate::unit::Part;

/// A return building: the slot exits on the water, the return slot is entered from the water
/// and left on the land; `keys` are more integers, `extra` more keys of the slot.
fn deck(keys: &str) -> String {
    format!(
        "[Characteristics/Integers]\ncCapacity=25\ncHideUser=1\ncUserUsesExit=1\ncDirectEntrance=1\n\
         cReturnBuilding=1\ncAdultChange=8\n{keys}[Satisfies]\nbuilding\nfun\n[slots]\nname=slot\n\
         name=back\n[slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=-48\nexitpos=0\nexitpos=-48\n\
         capacity=25\nexitonland=1\nexitonwater=1\n[back]\nslotpos=0\nslotpos=0\nentrpos=0\n\
         entrpos=-48\nexitpos=0\nexitpos=48\ncapacity=25\nisreturnslot=1\nexitonland=1\n"
    )
}

fn cat() -> Catalogue {
    let mut cat = types();
    for (key, name_id, keys) in [
        ("boatshak", 8059, "cExhibitViewer=1\ncUsedThought=10215\n"),
        (
            "dolshack",
            8060,
            "cUserTracker=1\ncStrictBuildingCapacity=2\ncCommerce=1\n",
        ),
        ("chroom", 8061, "cIdler=1\n"),
    ] {
        cat.push(EntityType {
            subtype: key.as_bytes().to_vec(),
            name_id,
            ..ty("building", "building", &deck(keys))
        });
    }
    cat
}

/// The lake with a building 9 of the registry key `key`; `wet` is whether the lake has water;
/// `fenced`, whether the 4 by 4 tiles x 3 to 6, y 3 to 6 stand in a ring of fences, but for the
/// edge to the building; `extra` more fences, as tile and edge.
fn lake_in(key: &str, wet: bool, fenced: bool, extra: &[((i32, i32), u32)]) -> (Catalogue, World) {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entrance = (8, 5);
        for y in 3..=6 {
            for x in 3..=7 {
                zoo.tiles[y * 10 + x].terrain = u8::from(wet);
            }
        }
        let mut b = entity("building", "building", 9, centre((5, 7)), 4);
        b.subtype = key.into();
        zoo.entities.push(b);
        zoo.entities
            .push(entity("building", "stand", 7, centre((1, 1)), 4));
        zoo.entities
            .push(entity("guests", "man", 1, centre((5, 3)), 6));
        for i in (3..=6).filter(|_| fenced) {
            for (at, d) in [((i, 3), 0), ((i, 6), 4), ((3, i), 6), ((6, i), 2)] {
                if (at, d) == ((5, 6), 4) {
                    continue;
                }
                let id = 100 + zoo.entities.len() as u32;
                zoo.entities.push(entity("fences", "", id, centre(at), d));
            }
        }
        for &(at, d) in extra {
            let id = 100 + zoo.entities.len() as u32;
            zoo.entities.push(entity("fences", "", id, centre(at), d));
        }
    });
    w.set_water(vec![false, true]);
    (cat(), w)
}

pub(super) fn lake(key: &str) -> (Catalogue, World) {
    lake_in(key, true, false, &[])
}

fn lake_with(key: &str, wet: bool) -> (Catalogue, World) {
    lake_in(key, wet, false, &[])
}

pub(super) fn set(w: &mut World, f: impl FnOnce(&mut Guest)) {
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        f(g);
    }
}

/// Guest 1 is a user of slot `slot` of building 9, its target.
pub(super) fn inside(cat: &Catalogue, w: &mut World, slot: usize) {
    let kind = with_cx(cat, w, |cx| cx.building(9).unwrap().1);
    let live = (w.units.buildings.entry(9)).or_insert_with(|| Live::new(&kind));
    live.users.push((1, slot, 0));
    set(w, |g| (g.building, g.slot) = (9, slot));
}

pub(super) fn leave(cat: &Catalogue, w: &mut World) {
    with_cx(cat, w, |cx| cx.remove_user());
}

fn out(w: &World) -> (Option<i32>, Option<String>) {
    (guest(w).returning, guest(w).return_set.clone())
}

fn names(w: &World) -> (Option<i32>, Option<&str>) {
    (guest(w).returning, guest(w).return_set.as_deref())
}

#[test]
fn the_first_leave_by_the_slot_sets_the_name_id_the_string_and_the_members() {
    // T1: the string is the key and the upgrade index; only a tracker lists the guest.
    for (key, id, string, members) in [
        ("boatshak", 8059, "bReturnboatshak0", vec![]),
        ("dolshack", 8060, "bReturndolshack0", vec![1]),
    ] {
        let (cat, mut w) = lake(key);
        inside(&cat, &mut w, 0);
        leave(&cat, &mut w);
        assert_eq!(names(&w), (Some(id), Some(string)), "{key}");
        assert_eq!(w.units.buildings[&9].members, members, "{key}");
    }
    let (cat, mut w) = lake("boatshak");
    w.units.upgrade_index.insert(9, 2);
    inside(&cat, &mut w, 0);
    leave(&cat, &mut w);
    assert_eq!(names(&w).1, Some("bReturnboatshak2"));
    // A building that is no return building sets nothing.
    let (cat, mut w) = lake("boatshak");
    set(&mut w, |g| g.building = 7);
    let live = Live::default();
    w.units.buildings.insert(
        7,
        Live {
            users: vec![(1, 0, 0)],
            ..live
        },
    );
    leave(&cat, &mut w);
    assert_eq!(names(&w), (None, None));
}

#[test]
fn the_first_leave_computes_the_water_capacity_again() {
    // 20 tiles of water: (20 + 7) / 10 = 2; the dolphin shack's strict capacity is 2 too.
    let (cat, mut w) = lake("boatshak");
    inside(&cat, &mut w, 0);
    leave(&cat, &mut w);
    assert_eq!(w.units.buildings[&9].water, 2);
}

#[test]
fn a_changing_room_gives_the_string_the_drown_goal_reads() {
    // T2: a man given task 0x84 after leaving a changing room drowns in the suit, any other
    // string (or none) in the surf.
    let (cat, mut w) = lake("chroom");
    inside(&cat, &mut w, 0);
    leave(&cat, &mut w);
    assert_eq!(names(&w).1, Some("bReturnchroom0"));
    with_cx(&cat, &mut w, |cx| cx.drown_init());
    assert_eq!(names(&w), (None, Some("bSuitDrown")));
    let (cat, mut w) = lake("boatshak");
    inside(&cat, &mut w, 0);
    leave(&cat, &mut w);
    with_cx(&cat, &mut w, |cx| cx.drown_init());
    assert_eq!(names(&w), (None, Some("bSurfDrown")));
}

#[test]
fn the_return_leave_and_a_forced_leave_clear_both_fields() {
    // T3: by the return slot.
    let (cat, mut w) = lake("dolshack");
    inside(&cat, &mut w, 0);
    leave(&cat, &mut w);
    inside(&cat, &mut w, 1);
    leave(&cat, &mut w);
    assert_eq!(out(&w), (None, None));
    // A forced leave of a guest that is not returning sets nothing.
    let (cat, mut w) = lake("dolshack");
    inside(&cat, &mut w, 0);
    crate::building::leave::push_out(&mut w, &cat, 9);
    assert_eq!(out(&w), (None, None));
    assert!(w.units.buildings[&9].members.is_empty());
    // And a returning guest forced out forgets the building.
    let (cat, mut w) = lake("boatshak");
    set(&mut w, |g| {
        (g.returning, g.return_set) = (Some(8059), Some("x".into()))
    });
    inside(&cat, &mut w, 1);
    crate::building::leave::push_out(&mut w, &cat, 9);
    assert_eq!(out(&w), (None, None));
}

#[test]
fn a_pick_up_leaves_by_the_normal_leave_and_so_sets_the_return_building() {
    // 12.2 point 21, 11.2: the guest picked up inside a dolphin shack on its first visit.
    let (cat, mut w) = lake("dolshack");
    inside(&cat, &mut w, 0);
    set(&mut w, |g| g.building = 9);
    w.units.get_mut(1).unwrap().goal.record.task = crate::goal::USE;
    w.units.get_mut(1).unwrap().goal.record.target.id = 9;
    crate::building::leave::pick_up(&mut w, &cat, 1);
    assert_eq!(names(&w), (Some(8060), Some("bReturndolshack0")));
}

#[test]
fn a_drop_clears_the_string_and_the_member_lists_and_a_pick_up_the_name_id_only() {
    let (cat, mut w) = lake("dolshack");
    inside(&cat, &mut w, 0);
    leave(&cat, &mut w);
    assert_eq!(w.units.buildings[&9].members, [1]);
    assert!(crate::place::unit::pick_up(&mut w, &cat, 1, 0).is_some());
    assert_eq!(names(&w), (None, Some("bReturndolshack0")));
    let (x, y) = centre((1, 3));
    assert!(crate::place::unit::put_down(
        &mut w,
        1,
        (x, y, 0),
        0,
        &crate::behaviour::NoData
    ));
    assert_eq!(names(&w), (None, None));
    assert!(w.units.buildings[&9].members.is_empty());
}

#[test]
fn the_change_record_goes_to_the_returning_guest_and_the_income_to_both_leaves() {
    // T4: 8 at the return leave, none at the first; the dolphin shack books at both.
    let (cat, mut w) = lake("dolshack");
    inside(&cat, &mut w, 0);
    leave(&cat, &mut w);
    let income = w.units.buildings[&9].income[0];
    assert!(income > 0.0);
    assert_eq!(guest(&w).pending[HAPPINESS], 0);
    inside(&cat, &mut w, 1);
    leave(&cat, &mut w);
    assert_eq!(w.units.buildings[&9].income[0], 2.0 * income);
    assert_eq!(guest(&w).pending[HAPPINESS], 8);
}

fn may(cat: &Catalogue, w: &mut World, building: u32, slot: usize) -> bool {
    with_cx(cat, w, |cx| cx.may_add(building, slot))
}

#[test]
fn a_returning_guest_uses_only_a_return_slot_and_the_others_do_not() {
    // T5 and point 11: the stand's slot is a normal slot.
    let (cat, mut w) = lake("boatshak");
    assert!(may(&cat, &mut w, 7, 0));
    assert!(may(&cat, &mut w, 9, 0) && !may(&cat, &mut w, 9, 1));
    set(&mut w, |g| g.returning = Some(8059));
    assert!(!may(&cat, &mut w, 7, 0));
    assert!(!may(&cat, &mut w, 9, 0) && may(&cat, &mut w, 9, 1));
    // A return slot refuses a guest that left a building of another type.
    let (cat, mut w) = lake("boatshak");
    set(&mut w, |g| g.returning = Some(8060));
    assert!(!may(&cat, &mut w, 9, 1));
    // A boat takes any guest of its type back, a dolphin shack's return slot refuses another type.
    let (cat, mut w) = lake("dolshack");
    set(&mut w, |g| g.returning = Some(8059));
    assert!(!may(&cat, &mut w, 9, 1));
    // With its own name id it takes only a member.
    set(&mut w, |g| g.returning = Some(8060));
    assert!(!may(&cat, &mut w, 9, 1));
    w.units.buildings.insert(
        9,
        Live {
            members: vec![1],
            ..Live::default()
        },
    );
    assert!(may(&cat, &mut w, 9, 1));
}

#[test]
fn the_members_and_the_strict_capacity_limit_the_guests_let_out() {
    // T6: two members fill the dolphin shack; so do two users now.
    let (cat, mut w) = lake("dolshack");
    assert!(may(&cat, &mut w, 9, 0));
    w.units.buildings.insert(
        9,
        Live {
            members: vec![10],
            ..Live::default()
        },
    );
    assert!(may(&cat, &mut w, 9, 0));
    w.units.buildings.insert(
        9,
        Live {
            members: vec![10, 11],
            ..Live::default()
        },
    );
    assert!(!may(&cat, &mut w, 9, 0));
    w.units.buildings.insert(
        9,
        Live {
            users: vec![(10, 0, 0), (11, 0, 0)],
            ..Live::default()
        },
    );
    assert!(!may(&cat, &mut w, 9, 0));
    // A boat with no water behind it has a capacity of 0 and lets no guest out.
    let (cat, mut w) = lake_with("boatshak", false);
    assert!(!may(&cat, &mut w, 9, 0));
    let (cat, mut w) = lake("boatshak");
    assert!(may(&cat, &mut w, 9, 0));
    // The cached capacity is used when it is above 0.
    w.units.buildings.insert(
        9,
        Live {
            water: 1,
            members: vec![10],
            ..Live::default()
        },
    );
    assert!(!may(&cat, &mut w, 9, 0));
}

#[test]
fn the_water_area_is_the_connected_water_without_objects_or_fences() {
    let (cat, w) = lake("boatshak");
    let at = w.find(9).unwrap();
    let kind = crate::building::kind_of(&cat, &w.zoo.entities[at], 29.0).unwrap();
    assert_eq!(ret::area(&w, at, &kind), 20);
    assert_eq!(
        (ret::legs(&w, at, &kind), ret::capacity(&w, at, &kind)),
        (2, 2)
    );
    // No water at the exit tile: none.
    let (cat, w) = lake_with("boatshak", false);
    let at = w.find(9).unwrap();
    let kind = crate::building::kind_of(&cat, &w.zoo.entities[at], 29.0).unwrap();
    assert_eq!(ret::area(&w, at, &kind), 0);
}

#[test]
fn a_fenced_pond_of_4_by_4_tiles_behind_the_building_has_an_area_of_16_and_a_capacity_of_2() {
    // Open point 11: the lake is 5 wide; fence the 4 by 4 tiles x 3 to 6, y 3 to 6.
    let (cat, w) = lake_in("boatshak", true, true, &[]);
    let at = w.find(9).unwrap();
    let kind = crate::building::kind_of(&cat, &w.zoo.entities[at], 29.0).unwrap();
    assert_eq!(ret::area(&w, at, &kind), 16);
    assert_eq!(ret::capacity(&w, at, &kind), 2);
}

#[test]
fn a_returning_guest_entering_is_not_counted_thinks_nothing_and_leaves_the_members() {
    // Point 14, against the first entry of a guest.
    let (cat, mut w) = lake("boatshak");
    assert!(with_cx(&cat, &mut w, |cx| cx.add_user(9, 0)));
    assert_eq!(w.units.buildings[&9].guests, [1, 0, 1]);
    assert_eq!(thoughts(&w), [10215]);
    let (cat, mut w) = lake("dolshack");
    set(&mut w, |g| g.returning = Some(8060));
    w.units.buildings.insert(
        9,
        Live {
            members: vec![1, 5],
            ..Live::default()
        },
    );
    assert!(with_cx(&cat, &mut w, |cx| cx.add_user(9, 1)));
    assert_eq!(w.units.buildings[&9].guests, [0; 3]);
    assert_eq!(w.units.buildings[&9].users.len(), 1);
    assert_eq!(w.units.buildings[&9].members, [5]);
    assert!(thoughts(&w).is_empty());
    let (cat, mut w) = lake("boatshak");
    set(&mut w, |g| g.returning = Some(8059));
    assert!(with_cx(&cat, &mut w, |cx| cx.add_user(9, 1)));
    assert!(thoughts(&w).is_empty());
}

#[test]
fn the_exit_complete_of_a_first_leave_sends_the_guest_riding_with_its_wandered_flag_down() {
    // T8: the first leave, then the return leave.
    let (cat, mut w) = lake("boatshak");
    inside(&cat, &mut w, 0);
    with_cx(&cat, &mut w, |cx| cx.exit_done());
    let r = &w.units.get(1).unwrap().goal.record;
    assert_eq!((r.task, r.target.id), (RETURN_TO_BUILDING, 9));
    assert!(!w.units.get(1).unwrap().goal.wandered);
    inside(&cat, &mut w, 1);
    with_cx(&cat, &mut w, |cx| cx.exit_done());
    assert_eq!(w.units.get(1).unwrap().goal.record.task, NONE);
    assert!(w.units.get(1).unwrap().goal.wandered);
}

#[test]
fn the_ai_reset_writes_0_and_keeps_the_string_and_the_membership() {
    // T7: in the use of a dolphin shack on the first visit.
    let (cat, mut w) = lake("dolshack");
    inside(&cat, &mut w, 0);
    w.units.get_mut(1).unwrap().goal.record.task = crate::goal::USE;
    w.units.get_mut(1).unwrap().goal.record.target.id = 9;
    with_cx(&cat, &mut w, |cx| cx.guest_reset_ai());
    assert_eq!(names(&w), (None, Some("bReturndolshack0")));
    assert_eq!(w.units.buildings[&9].members, [1]);
    assert!(w.units.buildings[&9].users.is_empty());
    assert!((1000..5000).contains(&w.units.get(1).unwrap().wait));
}

/// The guest type plays any return string as a walk.
struct Rides(crate::goal::guest::tests::Walks);

impl Data for Rides {
    fn set(&self, e: &crate::zoo::Entity, name: &str) -> Option<&[Call]> {
        let ride = name.starts_with("bReturn") || name == "bSuitIdle";
        self.0.set(e, if ride { "bWalk" } else { name })
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[Row] {
        &[]
    }
    fn timing(&self, e: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        self.0.timing(e, name)
    }
    fn number(&self, e: &crate::zoo::Entity, number: &crate::behaviour::Number) -> i32 {
        self.0.number(e, number)
    }
    fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<Heard> {
        None
    }
}

pub(super) fn riding(key: &str, id: i32) -> (Catalogue, World) {
    let (cat, mut w) = lake(key);
    set(&mut w, |g| {
        (g.returning, g.return_set) = (Some(id), Some(format!("bReturn{key}0")))
    });
    let r = &mut w.units.get_mut(1).unwrap().goal.record;
    (r.task, r.target.id) = (RETURN_TO_BUILDING, 9);
    (cat, w)
}

pub(super) fn ride<R>(cat: &Catalogue, w: &mut World, f: impl FnOnce(&mut Cx) -> R) -> R {
    with_data(cat, w, &Rides(walks()), f)
}

#[test]
fn the_ride_has_legs_from_the_water_area_and_walks_one_more_than_that() {
    let (cat, mut w) = riding("boatshak", 8059);
    assert!(ride(&cat, &mut w, |cx| cx.ride_init()));
    let mut legs = 0;
    while !w.units.get(1).unwrap().goal.record.done && legs < 10 {
        ride(&cat, &mut w, |cx| {
            cx.ride_decide();
            let target = cx.record().target.tile;
            assert!(target.is_some(), "a leg is walked");
            // The leg is walked to the water, with the return string.
            assert!(cx.world.is_water(target.unwrap()));
            assert_eq!(cx.unit.runner.top(), Some("bReturnboatshak0"));
            // It arrives.
            let own = cx.tile();
            cx.unit.goal.record.target.set_tile(cx.world, own);
            cx.ride_check_done();
        });
        legs += 1;
    }
    // Two legs at the start: three walks, the last with the legs at -1.
    assert_eq!(legs, 3);
}

#[test]
fn a_ride_whose_target_is_no_building_fails_at_the_init() {
    let (cat, mut w) = riding("boatshak", 8059);
    w.units.get_mut(1).unwrap().goal.record.target.id = 1;
    assert!(!ride(&cat, &mut w, |cx| cx.ride_init()));
    w.units.get_mut(1).unwrap().goal.record.target.id = 77;
    assert!(!ride(&cat, &mut w, |cx| cx.ride_init()));
}

#[test]
fn an_idler_pauses_between_the_legs_and_a_viewer_does_not() {
    let (cat, mut w) = riding("chroom", 8061);
    ride(&cat, &mut w, |cx| {
        assert!(cx.ride_init());
        assert!(cx.unit.goal.ride.idler);
        // Legs 2 at the start: the first decide pauses, treading water until its time.
        cx.ride_decide();
        let until = cx.unit.goal.ride.until;
        assert!(cx.unit.goal.ride.idle && (cx.now() + 6..cx.now() + 13).contains(&until));
        assert_eq!(cx.unit.runner.top(), Some("bSuitIdle"));
        cx.ride_check_done();
        assert!(cx.record().target.tile.is_some(), "still pausing");
        cx.world.units.ai_ticks = until;
        cx.ride_check_done();
        assert!(cx.record().target.tile.is_none() && !cx.record().done);
        // The next decide is a leg of an idler's reach, 6.
        cx.ride_decide();
        assert!(!cx.unit.goal.ride.idle && cx.record().target.tile.is_some());
    });
    let (cat, mut w) = riding("boatshak", 8059);
    ride(&cat, &mut w, |cx| {
        assert!(cx.ride_init());
        assert!(!cx.unit.goal.ride.idler && cx.unit.goal.ride.viewer);
        cx.ride_decide();
        assert!(!cx.unit.goal.ride.idle);
    });
}

#[test]
fn a_viewer_sees_overjoyed_animals_three_tiles_ahead() {
    // The guest faces west at (5, 5); the pen of 3 by 3 round (2, 5) holds zebras.
    for (happy, seen) in [(90, true), (80, false), (100, true), (81, true), (0, false)] {
        let mut w = world_with((10, 10), |zoo| {
            zoo.entrance = (9, 5);
            zoo.entities
                .push(entity("guests", "man", 1, centre((5, 5)), 6));
            for i in 0..3 {
                for (at, d) in [
                    ((1 + i, 4), 0),
                    ((1 + i, 6), 4),
                    ((1, 4 + i), 6),
                    ((3, 4 + i), 2),
                ] {
                    let id = 100 + zoo.entities.len() as u32;
                    zoo.entities.push(entity("fences", "", id, centre(at), d));
                }
            }
            zoo.entities
                .push(entity("animals", "zebra", 10, centre((2, 5)), 0));
        });
        let mut cat = cat();
        cat.push(EntityType {
            footprint: [1, 1, 1],
            ..ty("animals", "zebra", "")
        });
        let part = Part::Animal(crate::animal::Animal {
            happiness: happy,
            ..Default::default()
        });
        w.units.get_mut(10).unwrap().part = part;
        let pen = w.exhibits().list()[0].id;
        crate::exhibit::rating::refresh(&mut w, &cat, pen, &mut || 0);
        ride(&cat, &mut w, |cx| cx.ride_view());
        let g = guest(&w);
        assert_eq!(g.pending[HAPPINESS], if seen { 10 } else { 0 }, "{happy}");
        assert_eq!(
            thoughts(&w),
            if seen { vec![10098] } else { vec![] },
            "{happy}"
        );
    }
}

#[test]
fn the_ride_ends_by_looking_for_a_building_of_the_type_and_recovers_when_there_is_none() {
    let (cat, mut w) = riding("boatshak", 8059);
    ride(&cat, &mut w, |cx| cx.ride_complete());
    let r = &w.units.get(1).unwrap().goal.record;
    assert_eq!(
        (r.task, guest(&w).building, guest(&w).need),
        (NONE, 9, Some("fun"))
    );
    // A guest of another return building finds none: task 0x83, mode 0.
    let (cat, mut w) = riding("boatshak", 7000);
    ride(&cat, &mut w, |cx| cx.ride_complete());
    let r = &w.units.get(1).unwrap().goal.record;
    assert_eq!((r.task, r.mode, guest(&w).building), (RECOVER, 0, 0));
}

#[test]
fn the_ladder_walks_a_returning_guest_to_the_return_slot_with_its_string() {
    let (cat, mut w) = riding("boatshak", 8059);
    set(&mut w, |g| (g.building, g.need) = (9, Some("fun")));
    w.units.get_mut(1).unwrap().goal.record.task = NONE;
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = centre((6, 5));
    // The guest ladder runs the update.
    ride(&cat, &mut w, |cx| {
        cx.guest_ladder(NONE, false);
        assert_eq!(cx.unit.runner.top(), Some("bReturnboatshak0"));
    });
    // The boat has a direct entrance: task 0x67, to the water tile of its return slot.
    let r = &w.units.get(1).unwrap().goal.record;
    assert_eq!(
        (r.task, r.target.tile),
        (crate::goal::ENTRANCE, Some(6 * 10 + 5))
    );
    assert_eq!(guest(&w).slot, 1);
}

#[test]
fn a_failed_building_tiles_update_leaves_the_members_and_recovers_with_mode_1() {
    // The guest left a building of another type: the shack takes it back by no slot.
    let (cat, mut w) = riding("dolshack", 8061);
    set(&mut w, |g| (g.building, g.need) = (9, Some("fun")));
    w.units.get_mut(1).unwrap().goal.record.task = NONE;
    w.units.buildings.insert(
        9,
        Live {
            members: vec![1],
            ..Live::default()
        },
    );
    assert!(ride(&cat, &mut w, |cx| cx.return_ladder()));
    assert!(w.units.buildings[&9].members.is_empty());
    let r = &w.units.get(1).unwrap().goal.record;
    assert_eq!((r.task, r.mode), (RECOVER, 1));
    // A guest that is not returning is left alone.
    let (cat, mut w) = lake("dolshack");
    set(&mut w, |g| g.building = 9);
    assert!(!ride(&cat, &mut w, |cx| cx.return_ladder()));
}

#[test]
fn a_failed_walk_start_keeps_the_slot_it_found() {
    // A held guest cannot walk: the start fails after the slot search found the return slot.
    let (cat, mut w) = riding("boatshak", 8059);
    set(&mut w, |g| {
        (g.building, g.need, g.slot) = (9, Some("fun"), NO_SLOT)
    });
    w.units.get_mut(1).unwrap().goal.record.task = NONE;
    w.units.get_mut(1).unwrap().held = true;
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = centre((6, 5));
    ride(&cat, &mut w, |cx| cx.guest_ladder(NONE, false));
    // The goal recovers from the water, and the slot stays (goals 11.3.1 point 2).
    assert_eq!(w.units.get(1).unwrap().goal.record.task, RECOVER);
    assert_eq!(guest(&w).slot, 1);
}

#[test]
fn the_walks_of_a_returning_guest_and_of_the_recover_goal_use_the_return_string() {
    let (cat, mut w) = riding("boatshak", 8059);
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = centre((6, 5));
    ride(&cat, &mut w, |cx| {
        let to = cx.world.tile_index(5, 5).unwrap();
        assert!(cx.path_to(to, false));
        assert!(cx.return_walk(true, false));
        assert_eq!(cx.unit.runner.top(), Some("bReturnboatshak0"));
    });
    // A guest that is not returning keeps the default walk, a recovering one takes the string.
    set(&mut w, |g| g.returning = None);
    ride(&cat, &mut w, |cx| {
        assert!(cx.return_walk(true, false));
        assert_eq!(cx.unit.runner.top(), Some("bWalk"));
    });
    w.units.get_mut(1).unwrap().runner = Default::default();
    w.units.get_mut(1).unwrap().goal.record.target.clear();
    w.units.get_mut(1).unwrap().goal.record.task = RECOVER;
    ride(&cat, &mut w, |cx| {
        cx.recover_decide();
        assert_eq!(cx.unit.runner.top(), Some("bReturnboatshak0"));
    });
}

#[test]
fn a_returning_guest_refused_for_the_price_keeps_its_goal() {
    // Open point 13: the original's behaviour.
    for (returning, kept) in [(Some(8060), true), (None, false)] {
        let (cat, mut w) = lake("dolshack");
        let live = Live {
            price: 1e6,
            members: vec![1],
            ..Default::default()
        };
        w.units.buildings.insert(9, live);
        set(&mut w, |g| (g.returning, g.building) = (returning, 9));
        w.units.get_mut(1).unwrap().goal.record.task = crate::goal::ENTRANCE;
        if returning.is_some() {
            set(&mut w, |g| g.slot = 1);
        }
        with_cx(&cat, &mut w, |cx| cx.entrance_arrived());
        assert!(thoughts(&w).contains(&10073), "{returning:?}");
        let task = w.units.get(1).unwrap().goal.record.task;
        assert_eq!(task == crate::goal::ENTRANCE, kept, "{returning:?}");
    }
}

#[test]
fn a_guest_turned_away_for_the_price_clears_its_goal_and_wanders_but_a_returning_one_keeps_it() {
    // Shows 30.6 point 2: the ending is the guest's clear goal, with the wandered flag; a
    // returning guest keeps its goal (guest buildings 12.2 point 20).
    for (returning, cleared) in [(Some(8060), false), (None, true)] {
        let (cat, mut w) = lake("dolshack");
        let live = Live {
            price: 1e6,
            members: vec![1],
            ..Default::default()
        };
        w.units.buildings.insert(9, live);
        set(&mut w, |g| (g.returning, g.building) = (returning, 9));
        w.units.get_mut(1).unwrap().goal.record.task = crate::goal::ENTRANCE;
        if returning.is_some() {
            set(&mut w, |g| g.slot = 1);
        }
        with_cx(&cat, &mut w, |cx| {
            cx.unit.goal.wandered = false;
            cx.entrance_arrived();
            let task = cx.record().task;
            assert_eq!(
                (task == crate::goal::ENTRANCE, cx.unit.goal.wandered),
                (!cleared, cleared)
            );
        });
        assert!(thoughts(&w).contains(&10073), "{returning:?}");
    }
}

#[test]
fn a_guest_holding_a_return_building_travels_in_water() {
    let (_, mut w) = lake("boatshak");
    let at = w.find(1).unwrap();
    let here = w.tile_index(5, 3).unwrap();
    let mover = |w: &World| crate::unit::movement::mover(w.units.get(1).unwrap(), w, at, here);
    assert!(!mover(&w).in_water);
    set(&mut w, |g| g.returning = Some(8059));
    assert!(mover(&w).in_water);
}

/// A map of `n` by `n` tiles with guest 1 returning from a boat shack at its centre; `land` tells
/// which tiles are dry, `edit` adds entities.
fn sea(
    n: i32,
    land: impl Fn(i32, i32) -> bool,
    edit: impl FnOnce(&mut crate::zoo::Zoo),
) -> (Catalogue, World) {
    let mut w = world_with((n, n), |zoo| {
        zoo.entrance = (n as u32 - 1, 0);
        for t in 0..(n * n) as usize {
            zoo.tiles[t].terrain = u8::from(!land(t as i32 % n, t as i32 / n));
        }
        let at = centre((n / 2, n / 2));
        zoo.entities.push(entity("guests", "man", 1, at, 6));
        edit(zoo);
    });
    w.set_water(vec![false, true]);
    set(&mut w, |g| {
        (g.returning, g.return_set) = (Some(8059), Some("bReturnboatshak0".into()))
    });
    let r = &mut w.units.get_mut(1).unwrap().goal.record;
    (r.task, r.target.id) = (RETURN_TO_BUILDING, 9);
    (cat(), w)
}

/// The generator at state `seed`.
fn seeded(seed: u32) -> Rng {
    let mut rng = Rng::default();
    rng.seed(seed);
    rng
}

/// Runs a decide of the ride of guest 1 with `legs` left, the flags `idler` (an idler decides
/// out of its pause) and `viewer`, and the generator `rng`. The tile of the walk it started.
fn leg(cat: &Catalogue, w: &mut World, rng: &mut Rng, idler: bool, viewer: bool) -> Option<usize> {
    with_rng(cat, w, &Rides(walks()), rng, |cx| {
        let ride = &mut cx.unit.goal.ride;
        (ride.legs, ride.idler, ride.idle, ride.viewer) = (5, idler, idler, viewer);
        cx.ride_decide();
        cx.record().target.tile
    })
}

#[test]
fn a_leg_draws_x_then_y_in_a_window_of_6_for_an_idler_and_10_for_the_others() {
    // In an open sea the first try is the tile; the window and the order show in the tile.
    let n = 24;
    for (idler, reach) in [(false, 10), (true, 6)] {
        let tile = |a: u32, b: u32, r: u32| {
            let at = |v: u32| n / 2 + (v % r) as i32 - r as i32 / 2;
            at(a) + n * at(b)
        };
        let other = if idler { 10 } else { 6 };
        let seed = (1..)
            .find(|&s| {
                let mut r = seeded(s);
                let (a, b) = (r.rand(), r.rand());
                let t = tile(a, b, reach);
                t != tile(b, a, reach) && t != tile(a, b, other) && t != n / 2 * (n + 1)
            })
            .unwrap();
        let mut by_hand = seeded(seed);
        let (a, b) = (by_hand.rand(), by_hand.rand());
        let (cat, mut w) = sea(n, |_, _| false, |_| ());
        let mut rng = seeded(seed);
        let target = leg(&cat, &mut w, &mut rng, idler, false);
        assert_eq!(target, Some(tile(a, b, reach) as usize), "idler {idler}");
        assert_eq!(rng, by_hand, "two draws");
    }
}

/// The offsets of the tries of a leg search from the generator at state `seed`: x then y, in a
/// window of `reach`.
fn tries(seed: u32, reach: u32) -> impl Iterator<Item = (i32, i32)> {
    let mut rng = seeded(seed);
    let mut draw = move || rng.rand() % reach;
    std::iter::repeat_with(move || {
        let dx = draw() as i32 - reach as i32 / 2;
        (dx, draw() as i32 - reach as i32 / 2)
    })
}

#[test]
fn the_leg_search_makes_20_rounds_of_30_tries_and_requests_the_30th_tile() {
    let n = 24;
    // A path under the guest: no other tile is enterable for it, so every try but its own fails
    // and the 30th tile of the first round, a water tile it can reach, is requested.
    let path = |zoo: &mut crate::zoo::Zoo| {
        let at = centre((n / 2, n / 2));
        zoo.entities.push(entity("paths", "stone", 600, at, 0));
    };
    let seed = (1..)
        .find(|&s| tries(s, 10).take(30).all(|d| d != (0, 0)))
        .unwrap();
    let (dx, dy) = tries(seed, 10).nth(29).unwrap();
    let (cat, mut w) = sea(n, |_, _| false, path);
    let mut rng = seeded(seed);
    let target = leg(&cat, &mut w, &mut rng, false, false);
    assert_eq!(target, Some(((n / 2 + dx) + n * (n / 2 + dy)) as usize));
    let mut by_hand = seeded(seed);
    (0..60).for_each(|_| _ = by_hand.rand());
    assert_eq!(rng, by_hand, "60 draws");
    // On dry land every request fails: 20 rounds make 1200 draws, and the ride is over.
    let (cat, mut w) = sea(n, |_, _| true, |_| ());
    let (mut rng, mut by_hand) = (Rng::default(), Rng::default());
    assert_eq!(leg(&cat, &mut w, &mut rng, false, false), None);
    assert!(w.units.get(1).unwrap().goal.record.done);
    (0..1200).for_each(|_| _ = by_hand.rand());
    assert_eq!(rng, by_hand, "1200 draws");
}

#[test]
fn the_leg_search_tries_the_diagonals_in_order_after_20_failed_rounds() {
    // Water on the guest's tile, its four sides and the diagonals (-1, 1) and (1, -1); the other
    // diagonals are dry. A crowd of 4 guests on a tile leaves it no room: only the two diagonals
    // are tiles the guest may enter, and a request to a side still reaches them. The seed is one
    // for which no try of the 20 rounds hits a diagonal and no round ends on a side or on the
    // guest's own tile (found by a search over `tries`), so every round fails.
    const SEED: u32 = 442_478;
    let n = 24;
    let (cx, cy) = (n / 2, n / 2);
    let sides = [(0, 0), (0, -1), (-1, 0), (1, 0), (0, 1)];
    let wet = |x: i32, y: i32| {
        let d = (x - cx, y - cy);
        sides.contains(&d) || [(-1, 1), (1, -1)].contains(&d)
    };
    let crowd = |zoo: &mut crate::zoo::Zoo| {
        for (i, d) in sides.iter().enumerate() {
            let at = centre((cx + d.0, cy + d.1));
            for k in 0..(4 - usize::from(i == 0)) {
                let id = 10 * (i as u32 + 1) + k as u32;
                zoo.entities.push(entity("guests", "man", id, at, 6));
            }
        }
    };
    let (cat, mut w) = sea(n, |x, y| !wet(x, y), crowd);
    let mut rng = seeded(SEED);
    let target = leg(&cat, &mut w, &mut rng, false, false);
    // The first of the diagonals that is water, not the last.
    assert_eq!(target, Some(((cx - 1) + n * (cy + 1)) as usize));
    let mut by_hand = seeded(SEED);
    (0..1200).for_each(|_| _ = by_hand.rand());
    assert_eq!(rng, by_hand, "1200 draws");
}

fn area_of((cat, w): &(Catalogue, World)) -> usize {
    let at = w.find(9).unwrap();
    let kind = crate::building::kind_of(cat, &w.zoo.entities[at], 29.0).unwrap();
    ret::area(w, at, &kind)
}

#[test]
fn a_fence_between_the_exit_tile_and_the_building_gives_no_water() {
    // The exit tile (5, 6) lies north of the building (5, 7), which faces 4: the tile next to the
    // exit in the rotation is the building. A fence on that edge, from either tile, gives no
    // water; a fence on another edge of the exit tile or of the lake does not.
    for (fences, area) in [
        (vec![], 20),
        (vec![((5, 6), 4)], 0),
        (vec![((5, 7), 0)], 0),
        (vec![((5, 6), 2)], 20),
        (vec![((5, 6), 6)], 20),
        (vec![((4, 6), 4)], 20),
    ] {
        let lake = lake_in("boatshak", true, false, &fences);
        assert_eq!(area_of(&lake), area, "{fences:?}");
    }
}

#[test]
fn the_legs_and_the_water_capacity_stop_at_10() {
    // A lake of 12 by 12 tiles: (144 + 7) / 10 = 15.
    let mut w = world_with((16, 16), |zoo| {
        zoo.entrance = (15, 8);
        for y in 1..=12 {
            for x in 2..=13 {
                zoo.tiles[y * 16 + x].terrain = 1;
            }
        }
        let mut b = entity("building", "building", 9, centre((5, 13)), 4);
        b.subtype = "boatshak".into();
        zoo.entities.push(b);
    });
    w.set_water(vec![false, true]);
    let cat = cat();
    let at = w.find(9).unwrap();
    let kind = crate::building::kind_of(&cat, &w.zoo.entities[at], 29.0).unwrap();
    assert_eq!(ret::area(&w, at, &kind), 144);
    assert_eq!(
        (ret::legs(&w, at, &kind), ret::capacity(&w, at, &kind)),
        (10, 10)
    );
}

#[test]
fn an_idlers_pause_lasts_6_to_12_ai_ticks() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..400 {
        let (cat, mut w) = riding("chroom", 8061);
        let mut rng = seeded(seed);
        with_rng(&cat, &mut w, &Rides(walks()), &mut rng, |cx| {
            assert!(cx.ride_init());
            cx.ride_decide();
            assert!(cx.unit.goal.ride.idle);
            seen.insert(cx.unit.goal.ride.until - cx.now());
        });
    }
    assert_eq!(seen, (6..=12).collect());
}

#[test]
fn a_viewer_sees_nothing_2_or_4_tiles_ahead() {
    // The guest at (6, 5) faces west. A pen of one tile with an overjoyed zebra stands at each
    // of the given tiles; only the one 3 tiles ahead, (3, 5), is seen.
    for (pens, seen) in [
        (vec![4, 2], false),
        (vec![4], false),
        (vec![2], false),
        (vec![3], true),
        (vec![4, 3, 2], true),
    ] {
        let mut w = world_with((10, 10), |zoo| {
            zoo.entrance = (9, 5);
            zoo.entities
                .push(entity("guests", "man", 1, centre((6, 5)), 6));
            for &x in &pens {
                for d in [0, 2, 4, 6] {
                    let id = 100 + zoo.entities.len() as u32;
                    zoo.entities
                        .push(entity("fences", "", id, centre((x, 5)), d));
                }
                let id = 10 + x as u32;
                zoo.entities
                    .push(entity("animals", "zebra", id, centre((x, 5)), 0));
            }
        });
        let mut cat = cat();
        cat.push(EntityType {
            footprint: [1, 1, 1],
            ..ty("animals", "zebra", "")
        });
        for &x in &pens {
            let part = Part::Animal(crate::animal::Animal {
                happiness: 90,
                ..Default::default()
            });
            w.units.get_mut(10 + x as u32).unwrap().part = part;
        }
        for pen in w.exhibits().list().iter().map(|x| x.id).collect::<Vec<_>>() {
            crate::exhibit::rating::refresh(&mut w, &cat, pen, &mut || 0);
        }
        ride(&cat, &mut w, |cx| cx.ride_view());
        assert_eq!(
            guest(&w).pending[HAPPINESS],
            if seen { 10 } else { 0 },
            "{pens:?}"
        );
    }
}

#[test]
fn the_recover_complete_clears_the_return_word() {
    // Guest buildings 12.2 point 23: the string stays, the name id is 0.
    let (cat, mut w) = riding("boatshak", 8059);
    with_cx(&cat, &mut w, |cx| crate::goal::Goal::Recover.complete(cx));
    assert_eq!(names(&w), (None, Some("bReturnboatshak0")));
}

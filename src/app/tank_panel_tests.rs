//! The tank tab's buttons through the app: what a wall step, a fill and a drain pay, change and
//! tell, on the made-up map of the bulldozer tests (a tank of one tile at 20,20).

use super::bulldozer_tests::{Session, with_game};
use crate::exhibit::TankConfig;

/// Shipped numbers, but the walls cost 100 a piece (4 pieces over the divisor 5: 80 a step).
fn config() -> TankConfig {
    TankConfig {
        height: [5, 9],
        filled: true,
        salt: true,
        offset: -1,
        limit: 8,
        water: [1.5, 1.0],
        divisor: 5.0,
        ..TankConfig::default()
    }
}

/// The tank at 20,20 with its panel on it, in a game that has the config.
fn tank(s: &mut Session) {
    let game = s.app.game.as_mut().unwrap();
    game.world.set_tank_config(config());
    game.world.begin();
    for side in [0, 2, 4, 6] {
        s.fence(5, 20, 20, side);
    }
    s.app.exhibits_told();
    let game = s.app.game.as_mut().unwrap();
    assert_eq!(game.world.tank(1).map(|t| t.height), Some(5));
    game.panel.current = Some(1);
}

fn set_cash(s: &mut Session, cash: f32) {
    s.app.game.as_mut().unwrap().money.set_cash(cash);
}

fn water(s: &Session) -> (u32, u32, bool, bool) {
    let t = s.app.game.as_ref().unwrap().world.tank(1).unwrap();
    (t.height, t.level, t.filled, t.salt)
}

/// The messages the buttons told since the last call.
fn told(s: &mut Session) -> Vec<String> {
    std::mem::take(&mut s.app.tool_lines)
        .into_iter()
        .filter(|l| l.starts_with("tool message"))
        .collect()
}

#[test]
fn a_wall_step_pays_its_price_and_a_lowering_gives_it_back() {
    with_game("tank-wall", "", |s| {
        tank(s);
        // Filled: 4 pieces of 100 over 5, and 1.5 for the one tile's water.
        s.app.tank_wall(true);
        assert_eq!((water(s).0, s.cash()), (6, 50_000.0 - 81.5));
        s.app.tank_wall(false);
        assert_eq!((water(s).0, s.cash()), (5, 50_000.0));
        // At or below the initial height the steps are free, down to the lowest of 2.
        for _ in 0..5 {
            s.app.tank_wall(false);
        }
        assert_eq!((water(s).0, s.cash()), (2, 50_000.0));
        // And up to the highest of tanks.cfg, 8, where a press changes and costs nothing.
        for _ in 0..9 {
            s.app.tank_wall(true);
        }
        assert_eq!((water(s).0, s.cash()), (8, 50_000.0 - 3.0 * 81.5));
    });
}

#[test]
fn a_raise_the_cash_does_not_cover_is_refused_with_10002_and_books_nothing() {
    with_game("tank-wall-poor", "", |s| {
        tank(s);
        set_cash(s, 81.0);
        s.app.tank_wall(true);
        assert_eq!((water(s).0, s.cash()), (5, 81.0));
        assert_eq!(told(s), ["tool message 10002"]);
        // A lowering never asks for cash.
        set_cash(s, 0.0);
        s.app.tank_wall(false);
        assert_eq!(water(s).0, 4);
    });
}

#[test]
fn a_fill_pays_by_height_and_tiles_and_only_an_empty_tank_takes_one() {
    with_game("tank-fill", "", |s| {
        tank(s);
        s.app.game.as_mut().unwrap().world.set_paused(true);
        // Water above 0: the buttons do nothing.
        s.app.game.as_mut().unwrap().world.drain(1);
        s.app.game.as_mut().unwrap().world.fill(1);
        assert_eq!(water(s), (5, 4, true, true));
        s.app.tank_fill(false);
        assert_eq!((water(s), s.cash()), ((5, 4, true, true), 50_000.0));
        // Empty: fresh water costs 5 x 1 tile x 1.0 and turns the water fresh, full at once while paused.
        s.app.game.as_mut().unwrap().world.drain(1);
        s.app.tank_fill(false);
        assert_eq!((water(s), s.cash()), ((5, 4, true, false), 49_995.0));
        s.app.game.as_mut().unwrap().world.drain(1);
        s.app.tank_fill(true);
        assert_eq!((water(s), s.cash()), ((5, 4, true, true), 49_995.0 - 7.5));
    });
}

#[test]
fn a_fill_the_cash_does_not_cover_is_refused_with_10002() {
    with_game("tank-fill-poor", "", |s| {
        tank(s);
        s.app.game.as_mut().unwrap().world.set_paused(true);
        s.app.game.as_mut().unwrap().world.drain(1);
        set_cash(s, 7.0);
        s.app.tank_fill(true);
        assert_eq!((water(s), s.cash()), ((5, 0, false, true), 7.0));
        assert_eq!(told(s), ["tool message 10002"]);
    });
}

#[test]
fn an_object_that_cannot_be_in_water_refuses_the_fill_and_keeps_the_price() {
    with_game("tank-fill-bench", "", |s| {
        tank(s);
        s.app.game.as_mut().unwrap().world.drain(1);
        s.add(8, 20 * 64 + 16, 20 * 64 + 16, 0);
        s.app.tank_fill(true);
        assert_eq!((water(s), s.cash()), ((5, 0, false, true), 50_000.0 - 7.5));
        assert!(told(s).contains(&"tool message 10550".to_string()));
    });
}

#[test]
fn an_animal_in_the_tank_refuses_the_drain_with_10551() {
    with_game("tank-drain", "", |s| {
        tank(s);
        s.app.tank_drain();
        assert!(!water(s).2, "free, with no animal in it");
        s.app.game.as_mut().unwrap().world.fill(1);
        s.add(13, 20 * 64 + 40, 20 * 64 + 40, 0);
        s.app.tank_drain();
        assert!(water(s).2);
        assert_eq!(told(s), ["tool message 10551"]);
        assert_eq!(s.cash(), 50_000.0);
    });
}

#[test]
fn the_tab_tells_the_walls_the_limits_and_the_prices() {
    with_game("tank-tab", "", |s| {
        tank(s);
        let game = s.app.game.as_mut().unwrap();
        let ui = s.app.ui.as_mut().unwrap();
        let (vfs, cat) = (s.app.renderer.vfs(), s.app.renderer.catalogue());
        let lines = game.panel.refresh(ui, &game.world, cat, vfs);
        let tab = lines.iter().find(|l| l.starts_with("tank 1")).unwrap();
        assert_eq!(
            tab,
            "tank 1 walls 5 level 0 min 2 max 8 next $82 water salt fill $8 $5"
        );
    });
}

#[test]
fn a_new_salinity_with_an_animal_in_the_dry_tank_is_refused_with_10551_and_costs_nothing() {
    with_game("tank-fill-animal", "", |s| {
        tank(s);
        s.app.game.as_mut().unwrap().world.drain(1);
        s.add(13, 20 * 64 + 40, 20 * 64 + 40, 0);
        s.app.tank_fill(false);
        assert_eq!((water(s), s.cash()), ((5, 0, false, true), 50_000.0));
        assert_eq!(told(s), ["tool message 10551"]);
    });
}

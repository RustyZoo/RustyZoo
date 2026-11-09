//! The player of a tutorial: for the first goal that is not done, what a person does in the
//! interface to complete it, with the presses and the mouse of a script.

use super::*;
use crate::place::Verdict;

/// The panels of the building tools: the button of the left bar, the layout, the list and the
/// tabs that list items.
const PANELS: [(i32, i32, i32, &[i32]); 4] = [
    (1002, 8, 3208, &[3251, 3056, 3252, 3256]),
    (1001, 4, 3019, &[3050, 3053]),
    (1000, 3, 2019, &[2075, 3255, 3254, 2076]),
    (1005, 10, 3602, &[]),
];

/// The offsets of the tiles around a tile, the nearest first.
fn around(radius: i32) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = (-radius..=radius)
        .flat_map(|x| (-radius..=radius).map(move |y| (x, y)))
        .collect();
    out.sort_by_key(|&(x, y)| x * x + y * y);
    out
}

impl Play<'_, '_> {
    /// Whether each goal, in file order, left state 0.
    fn done(&self) -> Vec<bool> {
        let words = self.app.sim.scenario().map(|s| s.states());
        let words = words.unwrap_or_default();
        words.iter().skip(1).map(|&w| w != 0).collect()
    }

    /// The first simple goal that is not done.
    pub(super) fn next_goal(&self) -> Option<Goal> {
        let done = self.done();
        let at = (self.goals.iter().enumerate())
            .find(|(i, g)| g.leaf && !done.get(*i).copied().unwrap_or(true))?;
        Some(at.1.clone())
    }

    /// What a player does for the goal: presses what it names, or builds what it counts.
    pub(super) fn act(&mut self, g: &Goal) {
        let [rule, b, a, _, value] = g.rule;
        match (rule, b) {
            // The play button shows once the pause button was pressed.
            (10, 1..) if a == 1072 => {
                self.press(1071);
                self.press(a);
            }
            // The undo button waits for something to undo: a bench.
            (10, 1..) if a == 1075 && self.disabled(a) => self.supply(6050, 1),
            (10, 1..) => self.press(a),
            // The bulldozer takes out a piece of fence.
            (1, _) => self.delete_fence(),
            (2, 0) => self.supply(a, value),
            (3, 0) => {
                self.exhibit("fences", 3);
            }
            _ => {}
        }
    }

    /// The bulldozer sweeps over a piece of fence and the tile beside it.
    fn delete_fence(&mut self) {
        let world = &self.app.game.as_ref().unwrap().world;
        let zoo = &world.zoo;
        let width = zoo.width as usize;
        let inside = |x: i32, y: i32| {
            let tile = world.tile_index(x / 64, y / 64);
            let list = world.exhibits().list();
            list.iter().any(|e| tile.is_some_and(|t| e.order.contains(&t)))
        };
        let _ = width;
        let fence = zoo.entities.iter().find(|e| {
            let c = e.common.as_ref();
            e.class == "fences" && c.is_some_and(|c| inside(c.x, c.y))
        });
        let Some(c) = fence.and_then(|e| e.common.as_ref()) else {
            return;
        };
        let (x, y) = (c.x, c.y);
        if !self.app.game.as_ref().unwrap().build.bulldozing() {
            self.press(1028);
        }
        self.look((x / 64, y / 64));
        self.drag((x, y), (x + 64, y));
    }

    /// One more of the type with the name id `name_id`, as its panel puts it: an animal in the
    /// last exhibit, a path as a strip, the rest near the middle.
    fn supply(&mut self, name_id: i32, _value: i32) {
        let want = self.types(name_id);
        if !self.choose(&want) {
            return;
        }
        let game = self.app.game.as_ref().unwrap();
        let tool = game.build.tool().unwrap();
        let class = self.app.renderer.catalogue().types[tool].class;
        let middle = self.middle();
        match class {
            "paths" => self.strip(tool, middle, 8),
            "animals" => {
                let width = game.world.zoo.width as usize;
                let last = game.world.exhibits().list().last();
                let tile = last.map_or(0, |x| x.order[0]);
                self.put(((tile % width) as i32, (tile / width) as i32));
            }
            _ => {
                self.put(middle);
            }
        }
    }

    /// A path of `n` tiles in a row, on the first free row near `near`.
    fn strip(&mut self, tool: usize, near: (i32, i32), n: i32) {
        let free = |x: i32, y: i32| self.valid(tool, (x * 64 + 32, y * 64 + 32));
        let at = (around(15).into_iter())
            .map(|(x, y)| (near.0 + x, near.1 + y))
            .find(|&(x, y)| (0..n).all(|k| free(x + k, y)));
        if let Some((x, y)) = at {
            self.look((x + n / 2, y));
            self.drag((x * 64 + 32, y * 64 + 32), ((x + n - 1) * 64 + 32, y * 64 + 32));
        }
    }

    fn press(&mut self, id: i32) {
        self.tick(100, &[Command::Press(id)]);
        self.tick(100, &[]);
    }

    /// The window point of the world point.
    fn win(&self, wx: i32, wy: i32) -> (i32, i32) {
        match super::super::adopt_tests::point(self.app.camera().unwrap(), wx, wy) {
            Command::Mouse(Mouse::Move(x, y)) => (x, y),
            _ => unreachable!(),
        }
    }

    /// The view is centred on a tile.
    fn look(&mut self, tile: (i32, i32)) {
        let game = self.app.game.as_mut().unwrap();
        game.camera
            .centre_on(&game.world.zoo, tile.0 as u32, tile.1 as u32);
    }

    /// The middle of the map.
    fn middle(&self) -> (i32, i32) {
        let zoo = &self.app.game.as_ref().unwrap().world.zoo;
        (zoo.width as i32 / 2, zoo.height as i32 / 2)
    }

    /// Chooses an item of one of `want` (positions in the catalogue) in the panels: the bar
    /// button, the tab, the item. Gives whether one was listed.
    fn choose(&mut self, want: &[usize]) -> bool {
        for (bar, layout, list, tabs) in PANELS {
            for tab in tabs.iter().map(|&t| Some(t)).chain(tabs.is_empty().then_some(None)) {
                if !self.app.ui.as_ref().unwrap().shown(layout) {
                    self.press(bar);
                }
                if let Some(tab) = tab {
                    self.press(tab);
                }
                let game = self.app.game.as_ref().unwrap();
                let Some(n) = game.build.listed().iter().position(|t| want.contains(t)) else {
                    continue;
                };
                let id = 1_000_000 + list * 1000 + n as i32;
                let ui = self.app.ui.as_mut().unwrap();
                for top in 0..=n {
                    if ui.shown(id) {
                        break;
                    }
                    ui.set_top(list, top);
                }
                self.press(id);
                return true;
            }
        }
        false
    }

    /// The catalogue positions of the types with the name id `name_id`.
    fn types(&self, name_id: i32) -> Vec<usize> {
        let cat = self.app.renderer.catalogue();
        (0..cat.types.len())
            .filter(|&i| cat.types[i].name_id == name_id)
            .collect()
    }

    /// A click on the first tile around `near` where the pending object is valid.
    fn put(&mut self, near: (i32, i32)) -> bool {
        self.look(near);
        for (dx, dy) in around(9) {
            let (x, y) = self.win((near.0 + dx) * 64 + 32, (near.1 + dy) * 64 + 32);
            if !(300..950).contains(&x) || !(120..650).contains(&y) {
                continue;
            }
            self.tick(50, &[Command::Mouse(Mouse::Move(x, y))]);
            let pending = self.app.game.as_ref().unwrap().placing.pending.as_ref();
            if pending.is_some_and(|p| matches!(p.verdict, Verdict::Valid | Verdict::Overwrite)) {
                self.tick(50, &[Command::Mouse(Mouse::Down), Command::Mouse(Mouse::Up)]);
                return true;
            }
        }
        false
    }

    /// A drag between two world points, one pass.
    fn drag(&mut self, from: (i32, i32), to: (i32, i32)) {
        let (a, b) = (self.win(from.0, from.1), self.win(to.0, to.1));
        self.tick(
            50,
            &[
                Command::Mouse(Mouse::Move(a.0, a.1)),
                Command::Mouse(Mouse::Down),
                Command::Mouse(Mouse::Move(b.0, b.1)),
                Command::Mouse(Mouse::Up),
            ],
        );
    }

    /// Whether the type `tool` is valid, as a new object, at the world point.
    fn valid(&self, tool: usize, point: (i32, i32)) -> bool {
        let game = self.app.game.as_ref().unwrap();
        let cat = self.app.renderer.catalogue();
        let point = (i64::from(point.0), i64::from(point.1));
        let pending = crate::place::moved(&game.world, cat, &game.water, &|_| None, false, tool, 0, point);
        pending.is_some_and(|p| p.verdict == Verdict::Valid)
    }

    /// Whether a square of `n` tiles with its top corner at `at` can be fenced: its tiles and
    /// the ring round them are empty and the held fence is valid on every side.
    fn fits(&self, tool: usize, at: (i32, i32), n: i32) -> bool {
        let game = self.app.game.as_ref().unwrap();
        let world = &game.world;
        for x in at.0 - 1..=at.0 + n {
            for y in at.1 - 1..=at.1 + n {
                match world.tile_index(x, y) {
                    Some(t) if world.in_zoo(t) && world.is_empty(t) => {}
                    _ => return false,
                }
            }
        }
        sides(at, n).iter().all(|&(a, _)| self.valid(tool, a))
    }

    /// A new exhibit: a square of `n` tiles fenced with four drags, then its name.
    fn exhibit(&mut self, walls: &str, n: i32) -> bool {
        let cat = self.app.renderer.catalogue();
        let want: Vec<usize> = (0..cat.types.len())
            .filter(|&i| cat.types[i].class == walls)
            .collect();
        if !self.choose(&want) {
            return false;
        }
        let Some(tool) = self.app.game.as_ref().unwrap().build.tool() else {
            return false;
        };
        let middle = self.middle();
        let spot = around(20)
            .into_iter()
            .map(|(x, y)| (middle.0 + x, middle.1 + y))
            .find(|&at| self.fits(tool, at, n));
        let Some(at) = spot else { return false };
        self.look((at.0 + n / 2, at.1 + n / 2));
        for (from, to) in sides(at, n) {
            self.drag(from, to);
        }
        true
    }
}

/// The four drags that fence a square of `n` tiles with its top corner at `at`: the first piece
/// and the last of each side, in world points.
fn sides(at: (i32, i32), n: i32) -> [((i32, i32), (i32, i32)); 4] {
    let (x0, y0, x1, y1) = (at.0 * 64, at.1 * 64, (at.0 + n - 1) * 64, (at.1 + n - 1) * 64);
    [
        ((x0 + 32, y0 + 6), (x1 + 32, y0 + 6)),
        ((x1 + 58, y0 + 32), (x1 + 58, y1 + 32)),
        ((x1 + 32, y1 + 58), (x0 + 32, y1 + 58)),
        ((x0 + 6, y1 + 32), (x0 + 6, y0 + 32)),
    ]
}

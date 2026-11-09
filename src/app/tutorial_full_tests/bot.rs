//! The player of a tutorial: for the first goal that is not done, what a person does in the
//! interface to complete it, with the presses and the mouse of a script.

use super::*;
use crate::place::Verdict;
use crate::ui::layout::Kind;

/// The panels of the building tools: the button of the left bar, the layout, the list and the
/// tabs that list items.
const PANELS: [(i32, i32, i32, &[i32]); 4] = [
    (1002, 8, 3208, &[3251, 3056, 3252, 3256]),
    (1001, 4, 3019, &[3050, 3053]),
    (1000, 3, 2019, &[2075, 3255, 3254, 2076]),
    (1005, 10, 3602, &[]),
];

/// The buttons that open the list of expansions of each panel, and the list.
const SELECTORS: [i32; 4] = [3204, 3005, 2005, 3601];

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
            // The window of a show opens on the exhibit of a show tank.
            (10, 1..) if a == 11113 => {
                self.show_window();
                self.click_row(11111, 0);
                self.press(a);
            }
            (10, 1..) => self.press(a),
            // The bulldozer takes out a piece of fence.
            (1, _) => self.delete_fence(),
            // The DRT workers come with the base.
            (2, 0) if a == 9550 => self.supply(8031, value),
            (2, 0) => self.supply(a, value),
            (11, _) => self.pick_row(a, b, g.rule[3], value),
            (3, 0) => {
                self.exhibit(6);
            }
            (3, 1) => self.adopt(),
            (5, 1..=3) => self.improve(b, a, value),
            (6, 1..=3) => self.improve(b, g.rule[3], value),
            (16, _) => self.wall(b, a),
            _ => {}
        }
    }

    /// A row of a list opened by its button: row `value`, or with `b` not 0 the row that reads
    /// the string `text`.
    fn pick_row(&mut self, list: i32, b: i32, text: i32, value: i32) {
        if !self.app.ui.as_ref().unwrap().shown(list) {
            if list == 11191 {
                self.show_window();
                self.press(11108);
            } else {
                self.open_selector();
            }
        }
        let ui = self.app.ui.as_ref().unwrap();
        let Some(Kind::ListBox(l)) = ui.tree.find(list).map(|i| &ui.tree.elements[i].kind) else {
            return;
        };
        let want = crate::text::to_cp1252(&ui.string(text));
        let row = match b {
            0 => Some(value as usize),
            _ => l.rows.rows.iter().position(|r| r.text == want),
        };
        if let Some(row) = row {
            self.click_row(list, row);
        }
    }

    /// The list of expansions of an enabled panel.
    fn open_selector(&mut self) {
        let shown = |n: usize, ui: &Interface| ui.shown(PANELS[n].1);
        let open = (0..4).find(|&n| shown(n, self.app.ui.as_ref().unwrap()));
        let open = open.or_else(|| (0..4).find(|&n| !self.disabled(PANELS[n].0)));
        let open = open.unwrap_or(0);
        if !shown(open, self.app.ui.as_ref().unwrap()) {
            self.press(PANELS[open].0);
        }
        self.press(SELECTORS[open]);
    }

    /// A click on row `row` of a list box: its place is found by hovering down the list.
    fn click_row(&mut self, list: i32, row: usize) {
        let ui = self.app.ui.as_mut().unwrap();
        let i = ui.tree.find(list).unwrap();
        let (x, y, w, h) = ui.tree.rect(i);
        println!("ROW {list} {row} rect {x} {y} {w} {h} shown {} layout41 {} 11115 {:?} sel {:?}", ui.shown(list), ui.shown(41), ui.selected_row(11115), ui.selected_row(11196));
        for y in (y..y + h).step_by(2) {
            ui.mouse(Mouse::Move(x + w / 2, y));
            let Kind::ListBox(l) = &ui.tree.elements[i].kind else {
                println!("ROW not a list");
                return;
            };
            if (y - 361) % 10 == 0 { println!("ROW y {y} hover {:?} rows {} state {:x}", l.rows.hover, l.rows.rows.len(), ui.tree.elements[i].state); }
            if l.rows.hover == Some(row) {
                println!("ROW found {y}");
                let click = [Mouse::Move(x + w / 2, y), Mouse::Down, Mouse::Up];
                return self.tick(50, &click.map(Command::Mouse));
            }
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
            "animals" => self.house(),
            _ => {
                self.put(middle);
            }
        }
    }

    /// One wall button on the tank that is nearest to the wanted height: `b` 1 wants it at least
    /// `height`, 2 exactly (rule 16).
    fn wall(&mut self, b: i32, height: i32) {
        let world = &self.app.game.as_ref().unwrap().world;
        let tanks = world.exhibits().list().iter();
        let tanks: Vec<(u32, i32)> = tanks
            .filter_map(|x| Some((x.id, world.tank(x.id)?.height as i32)))
            .filter(|&(_, h)| if b == 1 { h < height } else { h != height })
            .collect();
        let Some(&(id, h)) = tanks.iter().min_by_key(|t| (t.1 - height).abs()) else {
            return;
        };
        self.app.open_exhibit(id);
        self.press(crate::panel::exhibit::TANK_TAB);
        self.press(if h < height { crate::panel::exhibit::WALL_UP } else { crate::panel::exhibit::WALL_DOWN });
    }

    /// The exhibit panel on a show tank, which opens the window of its show.
    fn show_window(&mut self) {
        if self.app.ui.as_ref().unwrap().shown(crate::panel::show::LAYOUT) {
            return;
        }
        let world = &self.app.game.as_ref().unwrap().world;
        let list = world.exhibits().list();
        let tank = list.iter().find(|x| crate::show::info_of(world, x.id).is_some());
        if let Some(id) = tank.map(|x| x.id) {
            self.app.open_exhibit(id);
            self.tick(STEP, &[]);
        }
    }

    /// The animal types whose family (`b` 1), genus (2) or species (3) is `id`.
    fn kinds(&self, b: i32, id: i32) -> Vec<usize> {
        let cat = self.app.renderer.catalogue();
        let at = [0, 2, 1, 0][b as usize];
        let kind = |&i: &usize| {
            let t = &cat.types[i];
            t.class == "animals" && crate::exhibit::rating::Likes::of(t).ids[at] == id
        };
        (0..cat.types.len()).filter(kind).collect()
    }

    /// The exhibits where the animals of these types live, with the key of the type.
    fn homes(&self, kinds: &[usize]) -> Vec<(u32, String)> {
        let world = &self.app.game.as_ref().unwrap().world;
        let cat = self.app.renderer.catalogue();
        let mut homes = Vec::new();
        for (at, e) in world.zoo.entities.iter().enumerate() {
            let is = kinds.iter().any(|&t| cat.types[t].type_name == e.type_name.as_bytes());
            let home = crate::panel::exhibit::animals_exhibit(world, at);
            if let (true, Some(x)) = (is, home)
                && !homes.contains(&(x, e.type_name.clone()))
            {
                homes.push((x, e.type_name.clone()));
            }
        }
        homes
    }

    /// One step towards a suitability of `target` for the animals of the kind `b` and `id`: the
    /// ground painted with what they like best, then a plant or a rock at a time, taken back when
    /// it lowered the rating.
    fn improve(&mut self, b: i32, id: i32, target: i32) {
        let kinds = self.kinds(b, id);
        let homes = self.homes(&kinds);
        if homes.is_empty() {
            if self.choose(&kinds) {
                self.house();
            }
            return;
        }
        let game = self.app.game.as_ref().unwrap();
        let cat = self.app.renderer.catalogue();
        let rate = |&(x, ref key): &(u32, String)| {
            crate::panel::animal::suitability_in(&game.world, cat, x, key)
        };
        let Some(low) = homes.iter().find(|h| rate(h) < target) else {
            return;
        };
        let (now, (x, key)) = (rate(low), low.clone());
        if let Some((before, t)) = self.last.take()
            && now < before
        {
            self.press(1075);
            self.bad.push(t);
        }
        if self.painted.contains(&x) {
            self.plant(x, &key, now);
        } else {
            self.painted.push(x);
            self.paint(x, &key);
        }
    }

    /// The ground of exhibit `x` painted with the terrain the species `key` likes best.
    fn paint(&mut self, x: u32, key: &str) {
        let cat = self.app.renderer.catalogue();
        let ty = cat.types.iter().find(|t| t.class == "animals" && t.type_name == key.as_bytes());
        let likes = crate::exhibit::rating::Likes::of(ty.unwrap());
        let kind = (0..likes.terrain.len()).max_by_key(|&k| likes.terrain[k]).unwrap_or(0);
        if !self.app.ui.as_ref().unwrap().shown(8) {
            self.press(1002);
        }
        self.press(3362);
        let at = (self.app.game.as_ref().unwrap().terrain.listed().iter())
            .position(|t| usize::from(t.kind) == kind);
        if let Some(n) = at {
            self.press(1_000_000 + 3350 * 1000 + n as i32);
        }
        let game = self.app.game.as_ref().unwrap();
        let width = game.world.zoo.width as usize;
        let order = game.world.exhibits().get(x).map(|e| e.order.clone()).unwrap_or_default();
        let tiles: Vec<(i32, i32)> = (order.iter())
            .map(|&t| ((t % width) as i32, (t / width) as i32))
            .collect();
        self.look(tiles.first().copied().unwrap_or((0, 0)));
        let mut commands = Vec::new();
        for (tx, ty) in tiles {
            let (px, py) = self.win(tx * 64 + 32, ty * 64 + 32);
            commands.extend([Mouse::Move(px, py), Mouse::Down, Mouse::Up].map(Command::Mouse));
        }
        self.tick(50, &commands);
        self.press(3302);
    }

    /// A plant, a rock or a shelter the species `key` likes, in exhibit `x`: the best of those on
    /// offer, then the next ones in turn.
    fn plant(&mut self, x: u32, key: &str, now: i32) {
        let cat = self.app.renderer.catalogue();
        let animal = cat.types.iter().find(|t| t.class == "animals" && t.type_name == key.as_bytes());
        let likes = crate::exhibit::rating::Likes::of(animal.unwrap());
        let value = |t: &crate::entity::EntityType| {
            let (habitat, name) = crate::exhibit::rating::object(t);
            let of = |id: i32| (likes.objects.iter().rev()).find(|p| p.0 == id).map_or(0, |p| p.1);
            of(habitat) + of(name)
        };
        let game = self.app.game.as_ref().unwrap();
        let mut want: Vec<(i32, usize)> = (0..cat.types.len())
            .filter(|&i| matches!(cat.types[i].class, "objects" | "building"))
            .filter(|&i| game.build.is_available(cat, i))
            .map(|i| (value(&cat.types[i]), i))
            .filter(|&(v, i)| {
                let plain = |key| crate::build::number(&cat.types[i], key) == Some(1.0);
                v > 0 || (v == 0 && (plain("cFoliage") || plain("cRock")))
            })
            .collect();
        want.sort();
        want.reverse();
        want.retain(|w| !self.bad.contains(&w.1));
        let Some(&(_, t)) = want.get(self.painted.len() % want.len().max(1)) else {
            return;
        };
        // An item no panel lists is dropped.
        if !self.choose(&[t]) {
            self.bad.push(t);
            return;
        }
        let world = &self.app.game.as_ref().unwrap().world;
        let width = world.zoo.width as usize;
        let tile = world.exhibits().get(x).map_or(0, |e| e.order[e.order.len() / 2]);
        self.last = Some((now, t));
        self.painted.push(0);
        self.put(((tile % width) as i32, (tile / width) as i32));
    }

    /// The held animal goes in an exhibit with no animal, or the last one.
    fn house(&mut self) {
        let game = self.app.game.as_ref().unwrap();
        let width = game.world.zoo.width as usize;
        let list = game.world.exhibits().list();
        let empty = list.iter().find(|x| x.rating.counts.1 == 0);
        let tile = empty.or(list.last()).map_or(0, |x| x.order[0]);
        self.put(((tile % width) as i32, (tile / width) as i32));
    }

    /// An animal of the first animals tab, in an exhibit.
    fn adopt(&mut self) {
        let want: Vec<usize> = {
            let cat = self.app.renderer.catalogue();
            (0..cat.types.len()).filter(|&i| cat.types[i].class == "animals").collect()
        };
        if self.choose(&want) {
            self.house();
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
        let game = self.app.game.as_ref().unwrap();
        let cat = self.app.renderer.catalogue();
        let want: Vec<usize> = (want.iter().copied())
            .filter(|&t| game.build.is_available(cat, t))
            .collect();
        if want.is_empty() {
            return false;
        }
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
    fn exhibit(&mut self, n: i32) -> bool {
        let cat = self.app.renderer.catalogue();
        let want: Vec<usize> = (0..cat.types.len())
            .filter(|&i| matches!(cat.types[i].class, "fences" | "tankwall"))
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

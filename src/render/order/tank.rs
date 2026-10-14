//! The paint order of a tank exhibit (map drawing 9.7): its tiles are first collected, then
//! painted in layers: floor and back walls, the contents under the water, the water, the
//! contents above it, and the front walls.

use std::collections::HashMap;

use super::*;

/// A sortable entry of a tile: paints last, forced z, view y, view x, object.
type Entry = (bool, i32, i32, i32, usize);

impl Painter<'_> {
    fn tank_at(&self, i: usize) -> Option<usize> {
        self.scene.tank[self.order[i]]
    }

    /// The tank wall on an edge of tile `i`, if it is an active one.
    fn active_edge(&self, i: usize, edge: usize) -> Option<usize> {
        self.edge_object(i, edge)
            .filter(|&o| self.scene.wall_tank(o).is_some())
    }

    /// Whether the way across the edge stays inside the tank: the ground is joined and no active
    /// wall stands there.
    fn joined_t(&self, a: usize, b: usize, e: usize) -> bool {
        let d = map_direction(self.scene.rotation, e * 2);
        self.scene.edge_state(self.order[a], d) < 2
            && self.active_edge(a, e).is_none()
            && self.active_edge(b, (e + 2) % 4).is_none()
    }

    /// Whether the ground of tile `a` is joined to its neighbour `b` across the screen edge `e`
    /// with no edge object of any kind between them.
    fn join_s(&self, a: usize, b: usize, e: usize) -> bool {
        let d = map_direction(self.scene.rotation, e * 2);
        self.scene.edge_state(self.order[a], d) < 2
            && self.edge_object(a, e).is_none()
            && self.edge_object(b, (e + 2) % 4).is_none()
    }

    /// Collects the tank that tile `start` belongs to, walking like [`Painter::paint_group`]
    /// but painting nothing, then paints it.
    pub(super) fn paint_tank(&mut self, start: usize, mut limit: i32) {
        self.calls += 1;
        if self.calls > 40 * self.order.len() + 1000 {
            return;
        }
        for edge in [UPPER_LEFT, UPPER_RIGHT] {
            if let Some(n) = self.nb(start, edge)
                && self.state[n] != State::Done
            {
                self.paint_from_back(n);
                if self.state[n] != State::Done {
                    return;
                }
            }
        }
        if self.state[start] != State::New {
            return;
        }
        let mut list = Vec::new();
        let mut i = start;
        'rows: loop {
            let first = i;
            if self.state[i] != State::New {
                break;
            }
            self.state[i] = State::Collected;
            list.push(i);
            let mut prev = i;
            let mut j = self.nb(i, LOWER_RIGHT);
            while let Some(jj) = j {
                if self.pos(jj) > limit
                    || self.state[jj] != State::New
                    || !self.joined_t(jj, prev, UPPER_LEFT)
                {
                    break;
                }
                if let Some(u) = self.nb(jj, UPPER_RIGHT)
                    && !matches!(self.state[u], State::Done | State::Collected)
                {
                    break;
                }
                self.state[jj] = State::Collected;
                list.push(jj);
                prev = jj;
                j = self.nb(jj, LOWER_RIGHT);
            }
            limit = limit.min(self.pos(prev));

            let Some(mut n) = self.nb(first, LOWER_LEFT) else {
                break;
            };
            if self.active_edge(first, LOWER_LEFT).is_some()
                || self.active_edge(n, UPPER_RIGHT).is_some()
                || self.tank_at(n).is_none()
            {
                break;
            }
            loop {
                if self.state[n] != State::New {
                    break 'rows;
                }
                if let Some(u) = self.nb(n, UPPER_RIGHT)
                    && self.state[u] == State::New
                {
                    break 'rows;
                }
                let Some(p) = self.nb(n, UPPER_LEFT) else {
                    break;
                };
                if self.state[p] == State::Done {
                    break;
                }
                if self.joined_t(n, p, UPPER_LEFT) {
                    n = p;
                    continue;
                }
                self.paint_from_back(p);
                if self.state[p] == State::New {
                    break 'rows;
                }
                break;
            }
            i = n;
        }
        self.paint_tank_list(&list);
    }

    fn paint_wall(&mut self, w: usize) {
        if !self.painted[w] {
            self.painted[w] = true;
            self.out.push(Item::Edge(w));
        }
    }

    /// An edge object that is not an active tank wall, once.
    fn paint_plain_edge(&mut self, i: usize, edge: usize) {
        if let Some(o) = self.edge_object(i, edge)
            && self.scene.wall_tank(o).is_none()
        {
            self.paint_wall(o);
        }
    }

    fn panes(&mut self, wall: usize, tank: &TankSpec, glass: Glass) {
        for k in tank.level..tank.height {
            self.out.push(Item::Pane { wall, k, glass });
        }
    }

    /// The things of tile `i` that the sorting of 9.5 puts in a row: the units of its list and
    /// the objects of its quadrants that `keep` accepts, the latter once the last of their
    /// quadrants is reached.
    pub(super) fn tile_entries(&mut self, i: usize, keep: &dyn Fn(usize) -> bool) -> Vec<Entry> {
        let scene = self.scene;
        let t = self.order[i];
        let mut list: Vec<Entry> = Vec::new();
        let push = |list: &mut Vec<Entry>, forced: i32, o: usize| {
            let obj = &scene.objects[o];
            let (px, py) = project(scene.rotation, obj.x, obj.y, obj.z);
            list.push((obj.late, forced, py, px, o));
        };
        for &o in &scene.list[t] {
            if keep(o) {
                push(&mut list, 0, o);
            }
        }
        let quads = self.quads.map(|q| scene.quad[t][q]);
        for (k, quad) in quads.iter().enumerate() {
            let Some(o) = *quad else { continue };
            if !keep(o) {
                continue;
            }
            self.counter[o] += 1;
            if self.counter[o] != scene.covers[o] {
                continue;
            }
            let forced = match k {
                0 => -1,
                1 | 2 if quads[0] == Some(o) => -1,
                3 if quads[1] == Some(o) || quads[2] == Some(o) => 1,
                _ => 0,
            };
            push(&mut list, forced, o);
        }
        list.sort_by_key(|&(late, forced, py, px, _)| (late, forced, py, px));
        list
    }

    /// The units and the objects of the quadrants of tile `i` that `keep` accepts, each once and
    /// without the counter of the quadrants, sorted.
    fn loose_entries(&self, i: usize, keep: &dyn Fn(usize) -> bool) -> Vec<usize> {
        let scene = self.scene;
        let t = self.order[i];
        let mut found: Vec<usize> = scene.list[t].clone();
        for q in self.quads {
            if let Some(o) = scene.quad[t][q]
                && !found.contains(&o)
            {
                found.push(o);
            }
        }
        let mut list: Vec<Entry> = found
            .into_iter()
            .filter(|&o| keep(o))
            .map(|o| {
                let obj = &scene.objects[o];
                let (px, py) = project(scene.rotation, obj.x, obj.y, obj.z);
                (obj.late, 0, py, px, o)
            })
            .collect();
        list.sort_by_key(|&(late, forced, py, px, _)| (late, forced, py, px));
        list.into_iter().map(|e| e.4).collect()
    }

    /// The temporary entities attached to tile `i`, sorted.
    pub(super) fn overlays(&mut self, i: usize) {
        let scene = self.scene;
        let mut list: Vec<Entry> = scene.overlay[self.order[i]]
            .iter()
            .map(|&o| {
                let obj = &scene.objects[o];
                let (px, py) = project(scene.rotation, obj.x, obj.y, obj.z);
                (obj.late, 0, py, px, o)
            })
            .collect();
        list.sort_by_key(|&(late, forced, py, px, _)| (late, forced, py, px));
        self.out.extend(list.into_iter().map(|e| Item::Object(e.4)));
    }

    fn surface_at(&self, i: usize) -> i32 {
        self.scene.surface(self.order[i]).unwrap_or(i32::MIN)
    }

    fn submerged(&self, o: usize, i: usize) -> bool {
        self.scene.objects[o].z < self.surface_at(i)
    }

    /// An object of a quadrant of tile `i` that is submerged and as tall as the water is deep.
    fn reaches(&self, o: usize, i: usize, level: i32) -> bool {
        level > 0 && self.submerged(o, i) && self.scene.objects[o].height >= level
    }

    fn holds_surface(&self, i: usize, level: i32) -> bool {
        let t = self.order[i];
        self.scene.quad[t]
            .iter()
            .flatten()
            .any(|&o| self.reaches(o, i, level))
    }

    /// The order of the walk of 9.4 restricted to `members` (array positions, in list order):
    /// the groups in the order they are painted, each as its runs. `join(a, b, e)` tells whether
    /// the ground of `a` is joined to its neighbour `b` across the screen edge `e`; a run and a
    /// row also stop at a change of `big`. A tile for which `stop` is true starts a group of its
    /// own: the run breaks before it and the walk ends before a new row that starts with it. If
    /// it is the first tile of a group, the group goes on only over the tiles whose `big` is the
    /// same object, or is one tile when there is none. The recursion of the original into
    /// groups that are not finished is left out.
    fn walk(
        &self,
        members: &[usize],
        join: &dyn Fn(usize, usize, usize) -> bool,
        big: &dyn Fn(usize) -> Option<usize>,
        stop: &dyn Fn(usize) -> bool,
    ) -> Vec<Vec<usize>> {
        // 0 free, 1 in the group being built, 2 painted
        let mut state: HashMap<usize, u8> = members.iter().map(|&m| (m, 0)).collect();
        let mut groups = Vec::new();
        for &s in members {
            if state[&s] != 0 {
                continue;
            }
            let free = |state: &HashMap<usize, u8>, i: usize| state.get(&i) == Some(&0);
            let b = big(s);
            let mut i = s;
            while !stop(i) {
                let up = [UPPER_RIGHT, UPPER_LEFT].into_iter().find_map(|e| {
                    let u = self.nb(i, e)?;
                    (free(&state, u) && !stop(u) && join(i, u, e) && big(u) == b).then_some(u)
                });
                match up {
                    Some(u) => i = u,
                    None => break,
                }
            }
            let (mut runs, mut limit, b) = (Vec::new(), i32::MAX, big(i));
            // A tile that holds such an object joins only a group that has that big object.
            let blocks = |j: usize| stop(j) && (b.is_none() || big(j) != b);
            let alone = b.is_none() && stop(i);
            'rows: loop {
                let first = i;
                state.insert(i, 1);
                runs.push(i);
                let mut prev = i;
                let mut j = self.nb(i, LOWER_RIGHT);
                while let Some(jj) = j {
                    if self.pos(jj) > limit
                        || !free(&state, jj)
                        || !join(jj, prev, UPPER_LEFT)
                        || big(jj) != b
                        || blocks(jj)
                        || alone
                    {
                        break;
                    }
                    if self.nb(jj, UPPER_RIGHT).is_some_and(|u| free(&state, u)) {
                        break;
                    }
                    state.insert(jj, 1);
                    runs.push(jj);
                    prev = jj;
                    j = self.nb(jj, LOWER_RIGHT);
                }
                limit = limit.min(self.pos(prev));
                let Some(mut n) = self.nb(first, LOWER_LEFT) else {
                    break;
                };
                if alone
                    || !state.contains_key(&n)
                    || !join(first, n, LOWER_LEFT)
                    || big(n) != b
                    || blocks(n)
                {
                    break;
                }
                loop {
                    if !free(&state, n) || self.nb(n, UPPER_RIGHT).is_some_and(|u| free(&state, u))
                    {
                        break 'rows;
                    }
                    match self.nb(n, UPPER_LEFT) {
                        Some(p)
                            if state.get(&p) == Some(&0)
                                && join(n, p, UPPER_LEFT)
                                && big(p) == b
                                && !blocks(p) =>
                        {
                            n = p
                        }
                        Some(p) if state.get(&p) == Some(&1) && join(n, p, UPPER_LEFT) => n = p,
                        _ => break,
                    }
                }
                i = n;
            }
            for &r in &runs {
                state.insert(r, 2);
            }
            groups.push(runs);
        }
        groups
    }

    /// Paints the collected tiles of one tank.
    fn paint_tank_list(&mut self, list: &[usize]) {
        let Some(k) = list.first().and_then(|&i| self.tank_at(i)) else {
            return;
        };
        let tank = self.scene.tanks[k];
        let level = tank.level;

        // Floor and back walls (9.7.3).
        let mut back = Vec::new();
        let mut prev_row = None;
        for &i in list {
            self.paint_terrain(i);
            if prev_row != Some(self.row(i))
                && let Some(w) = self.edge_object(i, UPPER_LEFT)
            {
                back.push(w);
                if self.scene.wall_tank(w).is_some() {
                    self.panes(w, &tank, Glass::UpperLeft);
                }
            }
            prev_row = Some(self.row(i));
        }
        for w in back {
            self.paint_wall(w);
        }
        let mut back = Vec::new();
        for &i in list {
            match self.nb(i, UPPER_RIGHT) {
                None => {
                    if let Some(w) = self.edge_object(i, UPPER_RIGHT) {
                        self.paint_wall(w);
                    }
                }
                Some(u) if self.tank_at(u) != Some(k) => {
                    if let Some(w) = self.edge_object(i, UPPER_RIGHT) {
                        back.push(w);
                        if self.scene.wall_tank(w).is_some() {
                            self.panes(w, &tank, Glass::UpperRight);
                        }
                    }
                }
                _ => {}
            }
        }
        for w in back {
            self.paint_wall(w);
        }

        // Contents and water (9.7.4).
        if level > 0 {
            for &i in list {
                let scene = self.scene;
                let t = self.order[i];
                self.out
                    .extend(scene.list[t].iter().map(|&o| Item::Shadow(o)));
                let mut seen: Vec<usize> = Vec::new();
                for q in self.quads {
                    if let Some(o) = scene.quad[t][q]
                        && !seen.contains(&o)
                    {
                        seen.push(o);
                        self.out.push(Item::Shadow(o));
                    }
                }
            }
        }
        // The big object that reaches the surface on a tile, if any.
        let big_reaching = |i: usize| {
            let o = self.scene.big[self.order[i]]?;
            self.reaches(o, i, level).then_some(o)
        };
        let groups = self.walk(list, &|a, b, e| self.join_s(a, b, e), &big_reaching, &|i| {
            self.holds_surface(i, level)
        });
        let at: HashMap<usize, usize> = list.iter().enumerate().map(|(n, &i)| (i, n)).collect();
        for group in groups {
            let mut in_list_order = group.clone();
            in_list_order.sort_by_key(|i| at[i]);
            if self.holds_surface(group[0], level) {
                self.tile_by_tile(&in_list_order, &tank);
            } else {
                self.layers(&group, &in_list_order, &tank);
            }
            for &i in &in_list_order {
                self.overlays(i);
                self.state[i] = State::Done;
            }
        }

        // Front walls (9.7.5).
        for (idx, &i) in list.iter().enumerate() {
            let last = idx + 1 == list.len() || self.row(list[idx + 1]) != self.row(i);
            if let Some(w) = self.active_edge(i, LOWER_RIGHT)
                && last
            {
                let n = self.nb(i, LOWER_RIGHT);
                if n.is_none_or(|n| self.tank_at(n) != Some(k)) {
                    self.faces(i, w, LOWER_RIGHT, n, &tank);
                }
                self.paint_wall(w);
            }
            if let Some(w) = self.edge_object(i, LOWER_LEFT) {
                let n = self.nb(i, LOWER_LEFT);
                if self.scene.wall_tank(w).is_some() && n.is_none_or(|n| self.tank_at(n) != Some(k))
                {
                    self.faces(i, w, LOWER_LEFT, n, &tank);
                }
                self.paint_wall(w);
            }
        }
    }

    /// The layers of one sub-group: submerged contents, underwater pictures, water, afloat
    /// contents.
    fn layers(&mut self, group: &[usize], in_list_order: &[usize], tank: &TankSpec) {
        let level = tank.level;
        let big_if = |i: usize, wet: bool| {
            let o = self.scene.big[self.order[i]]?;
            (self.submerged(o, i) == wet).then_some(o)
        };
        let order_of = |wet: bool| -> Vec<usize> {
            self.walk(
                group,
                &|a, b, e| self.join_s(a, b, e),
                &|i| big_if(i, wet),
                &|_| false,
            )
            .concat()
        };
        let wet = order_of(true);
        let dry = order_of(false);

        for &i in &wet {
            self.paint_plain_edge(i, UPPER_LEFT);
            self.paint_plain_edge(i, UPPER_RIGHT);
            let (scene, surface) = (self.scene, self.surface_at(i));
            let entries = self.tile_entries(i, &|o| scene.objects[o].z < surface);
            self.out
                .extend(entries.into_iter().map(|e| Item::Object(e.4)));
            self.paint_plain_edge(i, LOWER_RIGHT);
            self.paint_plain_edge(i, LOWER_LEFT);
        }
        for &i in &wet {
            let found = self.loose_entries(i, &|o| !self.submerged(o, i));
            self.out.extend(found.into_iter().map(Item::Under));
        }
        if level > 0 {
            for &i in in_list_order {
                let t = self.order[i];
                let z = self.scene.surface(t).unwrap_or(0);
                let noise = self.scene.scum[i];
                self.out.push(Item::Surface { tile: t, z, noise });
            }
        }
        for &i in in_list_order {
            let t = self.order[i];
            self.out
                .extend(self.scene.transient[t].iter().map(|&o| Item::Object(o)));
        }
        for &i in &dry {
            let (scene, surface) = (self.scene, self.surface_at(i));
            let entries = self.tile_entries(i, &|o| scene.objects[o].z >= surface);
            self.out
                .extend(entries.into_iter().map(|e| Item::Object(e.4)));
            self.gate(i, tank);
        }
    }

    /// A sub-group with an object that reaches the surface: painted tile by tile, the surface
    /// in four quarters that go in as the objects of their quadrants are reached (9.7.6).
    fn tile_by_tile(&mut self, group: &[usize], tank: &TankSpec) {
        let level = tank.level;
        for &i in group {
            let t = self.order[i];
            let z = self.scene.surface(t).unwrap_or(0);
            self.paint_plain_edge(i, UPPER_LEFT);
            self.paint_plain_edge(i, UPPER_RIGHT);
            let entries = self.tile_entries(i, &|_| true);
            let quads = self.quads.map(|q| self.scene.quad[t][q]);
            let reaching = quads.map(|q| q.filter(|&o| self.reaches(o, i, level)));
            let mut drawn = [false; 4];
            for (.., o) in entries {
                if let Some(last) = (0..4).rev().find(|&k| reaching[k] == Some(o)) {
                    for (quad, drawn) in drawn.iter_mut().enumerate().take(last + 1) {
                        if !std::mem::replace(drawn, true) {
                            self.out.push(Item::Quarter {
                                tile: t,
                                z,
                                quad: quad as u8,
                            });
                        }
                    }
                }
                self.out.push(Item::Object(o));
            }
            for (quad, drawn) in drawn.iter().enumerate() {
                if !drawn {
                    self.out.push(Item::Quarter {
                        tile: t,
                        z,
                        quad: quad as u8,
                    });
                }
            }
            self.overlays(i);
            self.paint_plain_edge(i, LOWER_RIGHT);
            self.paint_plain_edge(i, LOWER_LEFT);
            self.gate(i, tank);
        }
    }

    /// The platform of the gate, on the gate tile of the tank.
    fn gate(&mut self, i: usize, tank: &TankSpec) {
        if self.scene.index(tank.gate.0, tank.gate.1) == Some(self.order[i]) {
            self.out.push(Item::Platform(self.order[i]));
        }
    }

    /// The glass above the water and the water faces of the wall `w` on an outer edge of tile
    /// `i`; `n` is the tile across the edge.
    fn faces(&mut self, i: usize, w: usize, edge: usize, n: Option<usize>, tank: &TankSpec) {
        let right = edge == LOWER_RIGHT;
        let glass = if right {
            Glass::LowerRight
        } else {
            Glass::LowerLeft
        };
        self.panes(w, tank, glass);
        if tank.level <= 0 {
            return;
        }
        let t = self.order[i];
        let d = map_direction(self.scene.rotation, edge * 2);
        let mut from = 0;
        if let (true, Some(n)) = (self.scene.edge_state(t, d) >= 2, n) {
            let across = &self.scene.tiles[self.order[n]];
            let lowest = across.elevation - i32::from(across.shape & 3);
            from = (lowest - self.scene.tiles[t].elevation).max(0);
        }
        let wrap = |c: i32| if c > 254 { 0 } else { c };
        let mut c = wrap(i32::from(self.scene.scum[i]) + from);
        for k in from..tank.level {
            let noise = self.scene.face_scum[c as usize];
            self.out.push(Item::Face {
                wall: w,
                k,
                right,
                noise,
            });
            c = wrap(c + 1);
        }
    }
}

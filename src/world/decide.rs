//! The show decision of an exhibit (shows 31.1): once the show test has run, a show tank holds a
//! show info and its plain walls are show walls, and an exhibit that fails the test holds no info.

use super::{Shape, World};
use crate::exhibit::Exhibits;
use crate::show::Move;
use crate::zoo::{HabitatInfo, ShowInfo};

/// The frequency of a new show info in AI ticks; built in, `shows.cfg` does not say it.
const FREQUENCY: i32 = 90;

impl World {
    /// The show decision (31.1 point 1) on the exhibits `ids` after their show test: a show tank
    /// gets a show info when it has none and, with `morph`, its plain walls become show walls
    /// (point 4.2: the decisions after the wall step); an exhibit that fails the test loses its
    /// info (point 5).
    pub(super) fn decide(&mut self, ids: &[u32], morph: bool) {
        let mut done = Vec::new();
        for &id in ids {
            if done.contains(&id) || self.exhibits.get(id).is_none() {
                continue;
            }
            done.push(id);
            match self.tank(id).is_some_and(|t| t.show) {
                true => {
                    self.make_show(id, self.units.ai_ticks);
                    if morph {
                        self.morph_walls(id);
                    }
                }
                false => self.unmake_show(id),
            }
        }
    }

    /// The position in the shows section of the show info of exhibit `id` of `regions`, if any.
    pub(crate) fn info_slot_in(&self, regions: &Exhibits, id: u32) -> Option<usize> {
        let region = |&(x, y): &(i32, i32)| self.tile_index(x, y).and_then(|t| regions.region(t));
        (self.zoo.habitat_tiles.iter().enumerate())
            .find(|&(n, p)| region(p) == Some(id) && self.zoo.shows.info(n).is_some())
            .map(|(n, _)| n)
    }

    /// Make show, first part (31.1 point 2): an exhibit with no show info gets a new one, due 90
    /// after the AI clock `clock`. Its id is
    /// the show id counter plus 1 (modulo 65535), and the info is kept in the habitat record of the
    /// exhibit, or in a new one for an exhibit made in play. The counter is spent even when the id
    /// is taken (only after it wrapped), which leaves the exhibit without an info.
    fn make_show(&mut self, id: u32, clock: u32) {
        if self.info_slot_in(&self.exhibits, id).is_some() {
            return;
        }
        let next = (clock as i32).wrapping_add(FREQUENCY);
        let shows = &mut self.zoo.shows;
        shows.next_show = shows.next_show.wrapping_add(1);
        let show = shows.next_show % 65535;
        if shows.infos.iter().flatten().any(|i| i.id == show) {
            return;
        }
        let info = ShowInfo::new(show, FREQUENCY, next);
        let region =
            |&(x, y): &(i32, i32)| self.tile_index(x, y).and_then(|t| self.exhibits.region(t));
        let record = self
            .zoo
            .habitat_tiles
            .iter()
            .position(|p| region(p) == Some(id));
        let first = self.exhibits.get(id).map_or(0, |e| e.first);
        let width = self.zoo.width.max(1) as usize;
        let zoo = &mut self.zoo;
        zoo.shows.infos.resize(zoo.habitat_tiles.len(), None);
        match record {
            Some(n) => zoo.shows.infos[n] = Some(info),
            None => {
                zoo.habitat_tiles
                    .push(((first % width) as i32, (first / width) as i32));
                zoo.habitat_gates.push(((-1, -1), -1));
                zoo.habitat_info.push(HabitatInfo {
                    name: String::new(),
                    stats: [0; 9],
                    created: 0,
                    viewing: 0,
                    serviced: 0,
                });
                zoo.shows.infos.push(Some(info));
            }
        }
    }

    /// Unmake show (31.1 point 5): the show info of exhibit `id` is deleted, with no abort and no
    /// event.
    fn unmake_show(&mut self, id: u32) {
        if let Some(n) = self.info_slot_in(&self.exhibits, id) {
            self.zoo.shows.infos[n] = None;
        }
    }

    /// The show info of an exhibit of `old`, which is gone, is deleted (31.5 point 1); the show
    /// lists are told, for the grandstands that faced it (shows 30.4 point 1).
    pub(super) fn unmake_gone(&mut self, old: &Exhibits, id: u32) {
        let Some(n) = self.info_slot_in(old, id) else {
            return;
        };
        if let Some(info) = self.zoo.shows.infos[n].take() {
            let tiles = (0..self.zoo.tiles.len())
                .filter(|&t| old.region(t) == Some(id))
                .collect();
            self.show_moves.push(Move::Gone {
                show: info.id,
                tiles,
            });
        }
    }

    /// The walls of tank `id` that are recorded: the tank walls on the slots of its own tiles
    /// round its boundary, each once, in boundary order (3.9 step 2).
    fn recorded_walls(&self, id: u32) -> Vec<usize> {
        let mut found = Vec::new();
        for (t, n) in self.exhibits.boundary(self, id) {
            let wall = (n.and_then(|n| self.side_to(t, n))).and_then(|d| self.edge(t, d));
            if let Some(w) = wall.filter(|&w| self.shapes[w].is_some_and(|s| s.tank.wall))
                && !found.contains(&w)
            {
                found.push(w);
            }
        }
        found
    }

    /// The wall morph (31.1 point 4.2): the type of the first recorded show wall of tank `id`
    /// replaces the type of every recorded wall that is not a show wall (a gate keeps its name),
    /// and the piece is made healthy; no charge, no undo record.
    fn morph_walls(&mut self, id: u32) {
        let walls = self.recorded_walls(id);
        let Some(first) =
            (walls.iter().copied()).find(|&w| self.shapes[w].is_some_and(|s| s.tank.show))
        else {
            return;
        };
        let (class, name) = {
            let e = &self.zoo.entities[first];
            (e.class.clone(), e.type_name.clone())
        };
        let Some(show) = self.shapes[first].map(|s| s.tank) else {
            return;
        };
        let mut any = false;
        for w in walls {
            let Some(old) = self.shapes[w].filter(|s| s.tank.wall && !s.tank.show) else {
                continue;
            };
            self.shapes[w] = Some(Shape { tank: show, ..old });
            let e = &mut self.zoo.entities[w];
            (e.class, e.type_name) = (class.clone(), name.clone());
            if let Some(f) = e.common.as_mut().and_then(|c| c.fence.as_mut()) {
                (f.life, f.strength) = (show.life, show.strength);
            }
            any = true;
        }
        if any {
            self.changed(None);
        }
    }

    /// The creation of a tank first builds a temporary exhibit, which draws an id for a show tank
    /// and is deleted (31.2 step 0): the counter moves up by one for each of the tanks `ids` that
    /// passes the show test, before the real tank draws the id it keeps.
    pub(super) fn spend_temporary_ids(&mut self, ids: impl Iterator<Item = u32>) {
        let count = ids
            .filter(|&id| self.tank(id).is_some_and(|t| t.show))
            .count();
        let shows = &mut self.zoo.shows;
        shows.next_show = shows.next_show.wrapping_add(count as u16);
    }

    /// The load (31.6 points 1, 2 and 5): each show tank is made twice (31.2 step 0), the second
    /// time with an info of its own, which the stored one replaces: the counter moves up by two
    /// for each, and a show tank whose record holds no info keeps the fresh one, due 90 after the
    /// loaded AI clock (O25).
    pub(super) fn load_infos(&mut self) {
        let clock = self.zoo.clocks.ai_ticks;
        let tanks = self.exhibits.list().iter();
        let tanks: Vec<u32> = tanks
            .filter(|e| e.tank.is_some_and(|t| t.show))
            .map(|e| e.id)
            .collect();
        for id in tanks {
            self.spend_temporary_ids([id].into_iter());
            if self.info_slot_in(&self.exhibits, id).is_some() {
                let shows = &mut self.zoo.shows;
                shows.next_show = shows.next_show.wrapping_add(1);
            } else {
                self.make_show(id, clock);
            }
        }
    }

    /// The grandstands facing a show tank take its show id when the decision ends (31.2 steps 9
    /// and 11): the show lists tell it with the next step's moves.
    pub(crate) fn note_decision(&mut self) {
        self.show_moves.push(Move::Associate);
    }
}

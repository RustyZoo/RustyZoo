//! The maintenance task pool (staff 8.2): the tasks the maintenance workers share, made from the
//! events posted on the channel 0x4d. A normal task goes to the end of the pool, an urgent one to
//! the front; duplicates are not merged.
//!
//! The pool turns the events of the trash cans (10030), of the litter on the ground (10031), of
//! the dung on the grounds and the leftovers of an emptied exhibit (10032), of the fences (10033
//! decayed, 10034 broken, urgent) and of the tank filters (10035 decayed, 10036 broken, urgent).

use crate::zoo::Event;

/// The jobs a task names (goals 5): litter, a full trash can, dung, a worn fence and a worn filter.
pub const TRASH_PILE: u8 = 7;
pub const TRASH_CAN: u8 = 9;
pub const ZOO_DOO: u8 = 11;
pub const GO_TO_FENCE: u8 = 16;
pub const GO_TO_FILTER: u8 = 26;

/// A task: what to do and where, the worker that holds it (0 none) and whether it comes first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: u32,
    pub job: u8,
    pub tile: usize,
    pub worker: u32,
    pub urgent: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pool {
    pub tasks: Vec<Task>,
    last: u32,
}

/// The job and the urgency of an event of the pool.
fn job_of(kind: u16) -> Option<(u8, bool)> {
    match kind {
        10030 => Some((TRASH_CAN, false)),
        10031 => Some((TRASH_PILE, false)),
        10032 => Some((ZOO_DOO, false)),
        10033 => Some((GO_TO_FENCE, false)),
        10034 => Some((GO_TO_FENCE, true)),
        10035 => Some((GO_TO_FILTER, false)),
        10036 => Some((GO_TO_FILTER, true)),
        _ => None,
    }
}

impl Pool {
    /// The pool reads the events of its channel (simulation 4.2): one task for each, at the tile of
    /// the event's data. An event with a tile outside the map of `tiles` tiles makes none.
    pub fn listen<'a>(&mut self, events: impl Iterator<Item = &'a Event>, tiles: usize) {
        for e in events {
            let Some((job, urgent)) = job_of(e.kind) else {
                continue;
            };
            let tile = e.data as usize;
            if tile >= tiles {
                continue;
            }
            self.last = self.last.wrapping_add(1).max(1);
            let task = Task {
                id: self.last,
                job,
                tile,
                worker: 0,
                urgent,
            };
            match urgent {
                true => self.tasks.insert(0, task),
                false => self.tasks.push(task),
            }
        }
    }

    pub fn get(&self, id: u32) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// The task is done or void: it leaves the pool.
    pub fn finish(&mut self, id: u32) {
        self.tasks.retain(|t| t.id != id);
    }

    /// Worker `worker` gives up its tasks; they stay in the pool for anyone.
    pub fn unlink(&mut self, worker: u32) {
        for t in self.tasks.iter_mut().filter(|t| t.worker == worker) {
            t.worker = 0;
        }
    }
}

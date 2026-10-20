//! The event queue of the AI manager (simulation 5): events are posted into the next buffer, read
//! from the current one, and the two are swapped when the AI manager ends.

pub use crate::zoo::Event;
use crate::zoo::Queue;

/// The channel an entity posts on and units read.
pub const UNIT_CHANNEL: u8 = 0x55;

/// The channel the maintenance task pool reads (staff 8.2).
pub const TASK_CHANNEL: u8 = 0x4d;

/// The channel keepers read (the cues of shows).
pub const KEEPER_CHANNEL: u8 = 0x4b;

#[derive(Debug, Default)]
pub struct Events {
    current: Vec<Event>,
    next: Vec<Event>,
}

impl Events {
    /// Posts an event: it goes to the end of the next buffer.
    pub fn post(&mut self, event: Event) {
        self.next.push(event);
    }

    /// The events a listener reads in this step, in buffer order: those for it or for everyone
    /// on its channel whose delay is below 2. Reading does not consume them.
    pub fn listen(&self, id: u32, channel: u8) -> impl Iterator<Item = &Event> {
        self.current.iter().filter(move |e| {
            (e.target == id || e.target == 0) && e.channel == channel && e.delay < 2
        })
    }

    /// The end of the AI manager's update: the events still waiting go on after the ones posted
    /// during the step, the rest are dropped, and the next buffer becomes the current one.
    pub fn age(&mut self) {
        for mut event in std::mem::take(&mut self.current) {
            if event.delay > 1 {
                event.delay -= 1;
                self.next.push(event);
            }
        }
        std::mem::swap(&mut self.current, &mut self.next);
    }

    /// The queue a map stores, for a new game (simulation 8.3): the buffer the file calls "next"
    /// is the next one, the other is read at the first step.
    pub fn restore(&mut self, queue: &Queue) {
        let (next, current) = match queue.next_is_first {
            true => (&queue.first, &queue.second),
            false => (&queue.second, &queue.first),
        };
        (self.next, self.current) = (next.clone(), current.clone());
    }

    /// The two buffers, the one read in this step and the next one, for the state hash.
    pub fn buffers(&self) -> [&[Event]; 2] {
        [&self.current, &self.next]
    }

    /// Forgets every event, for a new game.
    pub fn clear(&mut self) {
        self.current.clear();
        self.next.clear();
    }
}

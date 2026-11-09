//! The event queue of the AI manager (simulation 5): events are posted into the next buffer, read
//! from the current one, and the two are swapped when the AI manager ends.

pub(crate) use crate::zoo::Event;
use crate::zoo::Queue;

/// The channel an entity posts on and units read.
pub(crate) const UNIT_CHANNEL: u8 = 0x55;

/// The channel the maintenance task pool reads (staff 8.2).
pub(crate) const TASK_CHANNEL: u8 = 0x4d;

/// The channel keepers read (the cues of shows).
pub(crate) const KEEPER_CHANNEL: u8 = 0x4b;

/// The channel a show info reads (shows 5.7), the one the world manager reads for the shows (6.2),
/// the one a grandstand reads (6.3) and the one a show tank is told on (read by nobody).
pub(crate) const SHOW_CHANNEL: u8 = 0x53;
pub(crate) const WORLD_CHANNEL: u8 = 0x57;
pub(crate) const BUILDING_CHANNEL: u8 = 0x42;
pub(crate) const TANK_CHANNEL: u8 = 0x48;

#[derive(Debug, Default)]
pub(crate) struct Events {
    current: Vec<Event>,
    next: Vec<Event>,
    /// Whether the next buffer is the first of the two a file stores (save games 15.4 point 3):
    /// as the file had it until a swap, which turns it over.
    next_is_first: bool,
}

impl Events {
    /// Posts an event: it goes to the end of the next buffer.
    pub(crate) fn post(&mut self, event: Event) {
        self.next.push(event);
    }

    /// The events posted since the last swap, taken out (for code that posts through the world).
    pub(crate) fn take_posted(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.next)
    }

    /// The events a listener reads in this step, in buffer order: those for it or for everyone
    /// on its channel whose delay is below 2. Reading does not consume them.
    pub(crate) fn listen(&self, id: u32, channel: u8) -> impl Iterator<Item = &Event> {
        self.current.iter().filter(move |e| {
            (e.target == id || e.target == 0) && e.channel == channel && e.delay < 2
        })
    }

    /// The events of a channel the world manager reads in this step, whatever their target: those
    /// whose delay is below 2, in buffer order.
    pub(crate) fn listen_all(&self, channel: u8) -> impl Iterator<Item = &Event> {
        (self.current.iter()).filter(move |e| e.channel == channel && e.delay < 2)
    }

    /// The end of the AI manager's update: the events still waiting go on after the ones posted
    /// during the step, the rest are dropped, and the next buffer becomes the current one.
    pub(crate) fn age(&mut self) {
        for mut event in std::mem::take(&mut self.current) {
            if event.delay > 1 {
                event.delay -= 1;
                self.next.push(event);
            }
        }
        std::mem::swap(&mut self.current, &mut self.next);
        self.next_is_first = !self.next_is_first;
    }

    /// The queue a map stores, for a new game (simulation 8.3): the buffer the file calls "next"
    /// is the next one, the other is read at the first step.
    pub(crate) fn restore(&mut self, queue: &Queue) {
        let (next, current) = match queue.next_is_first {
            true => (&queue.first, &queue.second),
            false => (&queue.second, &queue.first),
        };
        (self.next, self.current) = (next.clone(), current.clone());
        self.next_is_first = queue.next_is_first;
    }

    /// The two buffers, the one read in this step and the next one, for the state hash.
    pub(crate) fn buffers(&self) -> [&[Event]; 2] {
        [&self.current, &self.next]
    }

    /// The two buffers as a map stores them (the inverse of [`Events::restore`]).
    pub(crate) fn queue(&self) -> Queue {
        let (current, next) = (self.current.clone(), self.next.clone());
        let (first, second) = match self.next_is_first {
            true => (next, current),
            false => (current, next),
        };
        Queue {
            first,
            second,
            next_is_first: self.next_is_first,
        }
    }

    /// Forgets every event, for a new game.
    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.current.clear();
        self.next.clear();
        self.next_is_first = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: u16) -> Event {
        Event {
            sender: 1,
            target: 0,
            data: 0,
            extra: 0,
            kind,
            delay: 1,
            sender_channel: 0,
            channel: 0,
        }
    }

    #[test]
    fn the_next_flag_is_written_as_read_and_turns_over_with_each_swap() {
        for flag in [false, true] {
            let stored = Queue {
                first: vec![event(1)],
                second: vec![event(2)],
                next_is_first: flag,
            };
            let mut events = Events::default();
            events.restore(&stored);
            assert_eq!(events.queue(), stored);
            // A post goes to the buffer the flag names.
            events.post(event(3));
            let saved = events.queue();
            let next = if flag { &saved.first } else { &saved.second };
            assert_eq!(
                next.iter().map(|e| e.kind).collect::<Vec<_>>(),
                [if flag { 1 } else { 2 }, 3]
            );
            // A swap makes the other buffer the next one.
            events.age();
            assert_eq!(events.queue().next_is_first, !flag);
        }
    }
}

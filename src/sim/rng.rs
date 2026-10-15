//! The game's random generator and the seconds it is seeded from (main loop 9).

use std::time::{SystemTime, UNIX_EPOCH};

/// The `rand` of the C runtime the original links: one 32-bit state. The state is 1 when the
/// program starts, which is also what seeding with 1 gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng(u32);

impl Default for Rng {
    fn default() -> Self {
        Rng(1)
    }
}

impl Rng {
    pub fn seed(&mut self, seed: u32) {
        self.0 = seed;
    }

    /// The state, for the state hash of a run.
    pub fn state(&self) -> u32 {
        self.0
    }

    /// The next number, 0 to 32767.
    pub fn rand(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.0 >> 16) & 0x7fff
    }

    /// `rand() % 100 < percent`: one draw. A percent of 0 or less never succeeds, 100 or more
    /// always does, and the number is drawn all the same.
    pub fn chance(&mut self, percent: i32) -> bool {
        (self.rand() as i32) % 100 < percent
    }

    /// The percent test of the zoo status (`chance(p)`, zoo status 4): nothing is drawn and the
    /// answer is no when `percent` is 0; otherwise as [`Rng::chance`] (a negative percent draws
    /// and answers no).
    pub fn status_chance(&mut self, percent: i32) -> bool {
        percent != 0 && self.chance(percent)
    }

    /// `rand() % n`: one draw, a number from 0 to `n - 1`. The original divides by zero for an
    /// `n` of 0; here it gives 0 without a draw.
    pub fn pick(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { self.rand() % n }
    }
}

/// Where the whole seconds that seed the generator come from (main loop 9.2, O5): the wall clock in
/// play, `seed + n` in a test run, where n counts the seedings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seconds {
    Clock,
    Test { seed: u32, n: u32 },
}

impl Seconds {
    pub fn now(&mut self) -> u32 {
        match self {
            // Seconds since 1970, cut to 32 bits like the original's `time`.
            Seconds::Clock => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as u32),
            Seconds::Test { seed, n } => {
                let seconds = seed.wrapping_add(*n);
                *n = n.wrapping_add(1);
                seconds
            }
        }
    }
}

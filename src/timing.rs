//! The timers of the main loop and the calendar of the original game.
//!
//! One [`Timers::pass`] reads the clock once and runs three independent timers in a fixed order:
//! draw, interface, simulation. Each fires at most once per pass and none catches up after a
//! delay: there is no accumulator. The simulation gets a variable step in whole milliseconds,
//! the real time since it last fired multiplied by the speed, capped. Nothing here knows about
//! SDL, windows or the game state: the caller supplies the clock and acts on the [`Pass`].
//!
//! Differences from the original, on purpose: the clock is 64 bits and the tests are written
//! `now - last >= period`, so a wrap of a 32-bit counter cannot stop a timer; a rate that the
//! original would turn into a period of 0 (below 1 or above 1000) uses the fallback instead;
//! `MaxSimTick=0` uses the default.

use crate::config::{self, Config};

/// Period of the update timer when `UpdateRate` is absent or out of range: 10 per second.
const FALLBACK_UPDATE_PERIOD: u32 = 100;
/// Period of the draw timer when `DrawRate` is absent or out of range: 30 per second.
const FALLBACK_DRAW_PERIOD: u32 = 33;
/// The default and the replacement of 0 for `MaxSimTick`.
pub const DEFAULT_MAX_SIM_TICK: u32 = 200;

/// What the caller must do after one pass, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pass {
    /// Draw a frame.
    pub draw: bool,
    /// Update the interface, handing it this nominal period (a constant, not the measured time).
    pub interface: Option<u32>,
    /// Advance the simulation by this many milliseconds.
    pub step: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Timers {
    update_period: u32,
    draw_period: u32,
    cap: u32,
    /// Times of the last firings; `None` before the first pass, when all three fire.
    last_draw: Option<u64>,
    last_interface: Option<u64>,
    last_simulation: Option<u64>,
}

impl Timers {
    /// Periods and cap in milliseconds. The cap is unsigned: `u32::MAX` means no cap.
    pub fn new(update_period: u32, draw_period: u32, cap: u32) -> Self {
        Self {
            update_period,
            draw_period,
            cap,
            last_draw: None,
            last_interface: None,
            last_simulation: None,
        }
    }

    /// Reads `UpdateRate`, `DrawRate` and `MaxSimTick` from the `[user]` section of the game's
    /// ini file. Section and key names ignore case, as in the original's ini reading.
    pub fn from_config(ini: &Config) -> Self {
        let value = |key: &str| {
            ini.sections()
                .filter(|(name, _)| name.eq_ignore_ascii_case(b"user"))
                .find_map(|(_, section)| {
                    let k = section
                        .keys()
                        .find(|k| k.eq_ignore_ascii_case(key.as_bytes()))?;
                    section.get(k)
                })
        };
        let period = |key, fallback| value(key).map_or(fallback, |v| period_of(v, fallback));
        let cap = value("MaxSimTick")
            .and_then(whole)
            .filter(|&n| n != 0)
            .map_or(DEFAULT_MAX_SIM_TICK, |n| n as u32);
        Self::new(
            period("UpdateRate", FALLBACK_UPDATE_PERIOD),
            period("DrawRate", FALLBACK_DRAW_PERIOD),
            cap,
        )
    }

    pub fn update_period(&self) -> u32 {
        self.update_period
    }

    pub fn draw_period(&self) -> u32 {
        self.draw_period
    }

    pub fn cap(&self) -> u32 {
        self.cap
    }

    /// One pass at the clock reading `now` (milliseconds, from any origin), at this speed
    /// factor. The first pass fires all three timers; its step is the cap, as the original's
    /// is when the clock is far from its zero.
    pub fn pass(&mut self, now: u64, speed: f64) -> Pass {
        let due = |last: Option<u64>, period: u32| {
            last.is_none_or(|last| now.saturating_sub(last) >= u64::from(period))
        };
        let draw = due(self.last_draw, self.draw_period);
        if draw {
            self.last_draw = Some(now);
        }
        let interface = due(self.last_interface, self.update_period);
        if interface {
            self.last_interface = Some(now);
        }
        // Before the first firing the last time is the clock's zero: the longest elapsed time
        // of a 32-bit counter, which any speed above 0 brings to the cap.
        let elapsed = self
            .last_simulation
            .map_or(u64::from(u32::MAX), |last| now.saturating_sub(last));
        let step = (elapsed > u64::from(self.update_period)).then(|| {
            self.last_simulation = Some(now);
            // Product in double precision, truncated toward zero (a saturating cast), capped.
            (((elapsed as f64) * speed) as u64).min(u64::from(self.cap)) as u32
        });
        Pass {
            draw,
            interface: interface.then_some(self.update_period),
            step,
        }
    }

    /// Milliseconds from `now` to the earliest time at which a pass can fire a timer: 0 when
    /// one is due. A caller that sleeps this long runs the same firings as one that spins.
    pub fn next_due(&self, now: u64) -> u64 {
        let draw = self.last_draw.map(|l| l + u64::from(self.draw_period));
        let interface = self
            .last_interface
            .map(|l| l + u64::from(self.update_period));
        // Strictly more than the period.
        let simulation = self
            .last_simulation
            .map(|l| l + u64::from(self.update_period) + 1);
        [draw, interface, simulation]
            .into_iter()
            .map(|due| due.map_or(0, |due| due.saturating_sub(now)))
            .min()
            .unwrap_or(0)
    }
}

/// A whole number as `%d` of `sscanf` reads it, which is how the executable's ini reader reads
/// them: optional white space, an optional sign, decimal digits, the rest ignored. No `0x`
/// rule: `0x1F` reads 0. `None` when the value does not start with a number. Too many digits
/// wrap around.
fn whole(value: &[u8]) -> Option<i32> {
    let value = value.trim_ascii_start();
    let (negative, digits) = match value.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, value),
    };
    let count = digits.iter().take_while(|b| b.is_ascii_digit()).count();
    let number = digits[..count].iter().fold(0i32, |n, &d| {
        n.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
    });
    (count > 0).then(|| {
        if negative {
            number.wrapping_neg()
        } else {
            number
        }
    })
}

/// `truncate(1000 / rate)` from a rate held in single precision; the fallback when the value
/// is outside 1 to 1000 (the original gives 0 below 1 and above 1000; a value that cannot be
/// read converts to 0).
fn period_of(value: &[u8], fallback: u32) -> u32 {
    let rate = config::to_float(value);
    if (1.0..=1000.0).contains(&rate) {
        (1000.0 / f64::from(rate)) as u32
    } else {
        fallback
    }
}

/// Game milliseconds per millisecond of step: 7.2 seconds.
const GAME_MS_PER_STEP_MS: u64 = 7200;
const DAY_MS: u64 = 86_400_000;
/// Days from 1970-01-01 to 2001-01-01.
const DAYS_TO_2001: i64 = 11_323;

/// A date and time of day of the proleptic Gregorian calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: u32,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    /// 0 for Monday to 6 for Sunday.
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millisecond: u32,
}

/// The game's calendar: starts on Monday 1 January 2001, 00:00, and advances by 7.2 seconds
/// per millisecond of step, in whole numbers.
#[derive(Debug, Clone, Default)]
pub struct Calendar {
    /// Game milliseconds since the start.
    elapsed: u64,
    /// Raised by the step that changed the month; acted on by the next one.
    month_changed: bool,
}

impl Calendar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances by one simulation step. Returns whether the previous step changed the month:
    /// the month-end work (the finance check) belongs to this step, one late. A step that
    /// changes the month raises the flag for the next one.
    pub fn advance(&mut self, step: u32) -> bool {
        let month_end = std::mem::take(&mut self.month_changed);
        let before = self.date().month;
        self.elapsed += u64::from(step) * GAME_MS_PER_STEP_MS;
        self.month_changed = self.date().month != before;
        month_end
    }

    pub fn date(&self) -> Date {
        let days = (self.elapsed / DAY_MS) as i64;
        let ms = (self.elapsed % DAY_MS) as u32;
        // Civil date from a day count (days since 1970-01-01).
        let z = days + DAYS_TO_2001 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = (yoe + era * 400) as u32 + u32::from(month <= 2);
        Date {
            year,
            month,
            day,
            weekday: (days % 7) as u32,
            hour: ms / 3_600_000,
            minute: ms / 60_000 % 60,
            second: ms / 1000 % 60,
            millisecond: ms % 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn periods(ini: &str) -> (u32, u32, u32) {
        let t = Timers::from_config(&Config::parse(ini.as_bytes()).unwrap());
        (t.update_period(), t.draw_period(), t.cap())
    }

    #[test]
    fn period_table() {
        let period = |rate: &str| period_of(rate.as_bytes(), 7);
        // Shipped rates, then the rest of the table; out of range gives the fallback (7 here).
        for (rate, expected) in [
            ("15", 66),
            ("60", 16),
            ("10", 100),
            ("30", 33),
            ("1", 1000),
            ("20", 50),
            ("100", 10),
            ("29.97", 33),
            ("1000", 1),
            ("144", 6),
            ("0.99", 7),
            ("0", 7),
            ("-5", 7),
            ("1001", 7),
            ("abc", 7),
            ("", 7),
        ] {
            assert_eq!(period(rate), expected, "rate {rate:?}");
        }
    }

    #[test]
    fn settings_from_the_ini() {
        assert_eq!(
            periods("[user]\nUpdateRate=15\nDrawRate=60\n"),
            (66, 16, 200)
        );
        assert_eq!(periods("[user]\nfullscreen=1\n"), (100, 33, 200), "absent");
        assert_eq!(
            periods("[resource]\npath=.\n"),
            (100, 33, 200),
            "no section"
        );
        assert_eq!(
            periods("[USER]\nupdaterate=20\ndrawrate=100\n"),
            (50, 10, 200),
            "case"
        );
        assert_eq!(
            periods("[user]\nUpdateRate=0\nDrawRate=5000\n"),
            (100, 33, 200)
        );
        assert_eq!(periods("[user]\nUpdateRate=x\nDrawRate=\n"), (100, 33, 200));
        assert_eq!(periods("[user]\nMaxSimTick=500\n").2, 500);
        assert_eq!(periods("[user]\nMaxSimTick=0\n").2, 200, "0 is refused");
        assert_eq!(
            periods("[user]\nMaxSimTick=-1\n").2,
            u32::MAX,
            "unsigned: no cap"
        );
        assert_eq!(periods("[user]\nMaxSimTick=0x\n").2, 200, "0 is refused");
        assert_eq!(
            periods("[user]\nMaxSimTick=0x1F\n").2,
            200,
            "no 0x rule: reads 0"
        );
        assert_eq!(periods("[user]\nMaxSimTick=abc\n").2, 200, "unreadable");
        assert_eq!(periods("[user]\nMaxSimTick= +31x\n").2, 31);
    }

    /// Passes at every clock reading in `times`, collecting the firings.
    fn run(timers: &mut Timers, times: impl IntoIterator<Item = u64>) -> Vec<(u64, Pass)> {
        times
            .into_iter()
            .map(|now| (now, timers.pass(now, 1.0)))
            .collect()
    }

    #[test]
    fn first_pass_fires_everything() {
        let mut t = Timers::new(66, 16, 200);
        let p = t.pass(0, 1.0);
        assert_eq!(
            p,
            Pass {
                draw: true,
                interface: Some(66),
                step: Some(200)
            }
        );
    }

    #[test]
    fn fast_machine_ten_seconds() {
        // Spec 11.2: a pass per millisecond, from a moment when all three fire.
        let mut t = Timers::new(66, 16, 200);
        t.pass(0, 1.0);
        let passes = run(&mut t, 1..=10_000);
        let draws = passes.iter().filter(|(_, p)| p.draw).count();
        let interfaces: Vec<_> = passes.iter().filter_map(|(_, p)| p.interface).collect();
        let steps: Vec<_> = passes.iter().filter_map(|(_, p)| p.step).collect();
        assert_eq!((draws, interfaces.len(), steps.len()), (625, 151, 149));
        assert!(interfaces.iter().all(|&n| n == 66));
        assert!(steps.iter().all(|&n| n == 67));
        let times = |f: fn(&Pass) -> bool| -> Vec<u64> {
            passes
                .iter()
                .filter(|(_, p)| f(p))
                .map(|(n, _)| *n)
                .take(3)
                .collect()
        };
        assert_eq!(times(|p| p.draw), [16, 32, 48]);
        assert_eq!(times(|p| p.interface.is_some()), [66, 132, 198]);
        assert_eq!(times(|p| p.step.is_some()), [67, 134, 201]);
    }

    #[test]
    fn sixty_passes_a_second_alternate_83_and_67() {
        // Spec 11.3: the clock reads 16, 33, 50, 66, 83, 100, 116, 133, 150, ...
        let mut t = Timers::new(66, 16, 200);
        t.pass(0, 1.0);
        let passes = run(&mut t, (1..=120).map(|k| k * 1000 / 60));
        let steps: Vec<_> = passes.iter().filter_map(|(_, p)| p.step).collect();
        assert_eq!(&steps[..6], [83, 67, 83, 67, 83, 67]);
        assert!(steps.iter().all(|&n| n == 83 || n == 67));
        // The same total of simulated time: 2 s of real time, minus the part not yet stepped.
        let last = passes
            .iter()
            .rev()
            .find(|(_, p)| p.step.is_some())
            .unwrap()
            .0;
        assert_eq!(steps.iter().sum::<u32>() as u64, last);
    }

    #[test]
    fn coarse_clock_steps_78() {
        // Spec 11.4: jumps of 15.625 ms; the step fires at the fifth jump.
        let mut t = Timers::new(66, 16, 200);
        t.pass(0, 1.0);
        let passes = run(&mut t, (1..=64).map(|k| (k as f64 * 15.625) as u64));
        let steps: Vec<_> = passes.iter().filter_map(|(_, p)| p.step).collect();
        assert_eq!(steps[0], 78);
        assert!(steps.iter().all(|&n| n == 78 || n == 79), "{steps:?}");
    }

    #[test]
    fn step_for_elapsed_and_speed() {
        // Spec 11.5, cap 200, update period 66.
        let table: [(u64, [Option<u32>; 5]); 7] = [
            (16, [None; 5]),
            (66, [None; 5]),
            (67, [Some(33), Some(67), Some(100), Some(134), Some(200)]),
            (80, [Some(40), Some(80), Some(120), Some(160), Some(200)]),
            (100, [Some(50), Some(100), Some(150), Some(200), Some(200)]),
            (200, [Some(100), Some(200), Some(200), Some(200), Some(200)]),
            (500, [Some(200); 5]),
        ];
        for (elapsed, expected) in table {
            for (speed, expected) in [0.5, 1.0, 1.5, 2.0, 3.0].into_iter().zip(expected) {
                let mut t = Timers::new(66, 16, 200);
                t.pass(1000, 1.0);
                assert_eq!(
                    t.pass(1000 + elapsed, speed).step,
                    expected,
                    "{elapsed} at {speed}"
                );
            }
        }
        // The lowest ordinary developer speed.
        let mut t = Timers::new(66, 16, 200);
        t.pass(1000, 1.0);
        assert_eq!(t.pass(1067, 0.10000000000000014).step, Some(6));
        // Zero speed gives steps of 0; a negative or NaN product gives 0 too.
        let mut t = Timers::new(66, 16, 200);
        t.pass(1000, 1.0);
        assert_eq!(t.pass(1100, f64::NAN).step, Some(0));
    }

    #[test]
    fn regular_passes() {
        // Spec 11.5: passes 16 ms apart, a step of 80 in the fifth and then every fifth.
        let mut t = Timers::new(66, 16, 200);
        t.pass(0, 1.0);
        let steps: Vec<_> = run(&mut t, (1..=15).map(|k| k * 16))
            .into_iter()
            .map(|(_, p)| p.step)
            .collect();
        let at = |i: usize| steps[i - 1];
        assert!((1..=4).all(|i| at(i).is_none()));
        assert_eq!((at(5), at(10), at(15)), (Some(80), Some(80), Some(80)));
        // Passes 67 ms apart: 67 every time. 500 ms apart: 200 every time.
        for (gap, expected) in [(67, 67), (500, 200)] {
            let mut t = Timers::new(66, 16, 200);
            t.pass(0, 1.0);
            assert!(
                run(&mut t, (1..=10).map(|k| k * gap))
                    .iter()
                    .all(|(_, p)| p.step == Some(expected)),
                "gap {gap}"
            );
        }
    }

    #[test]
    fn one_capped_step_after_a_stall() {
        // Spec 11.6: blocked for 5 s: one frame, one interface update of 66, one step of 200.
        let mut t = Timers::new(66, 16, 200);
        t.pass(0, 1.0);
        t.pass(70, 1.0);
        let p = t.pass(5070, 1.0);
        assert_eq!(
            p,
            Pass {
                draw: true,
                interface: Some(66),
                step: Some(200)
            }
        );
        // The pass after that is an ordinary one: nothing is due, and nothing catches up.
        assert_eq!(
            t.pass(5071, 1.0),
            Pass {
                draw: false,
                interface: None,
                step: None
            }
        );
        assert_eq!(t.pass(5087, 1.0).step, None);
        assert_eq!(t.pass(5137, 1.0).step, Some(67));
    }

    #[test]
    fn clock_beyond_32_bits() {
        // 64-bit clock and subtraction form: a gap across 2^32 is an ordinary gap.
        let near = u64::from(u32::MAX) - 10;
        let mut t = Timers::new(66, 16, 200);
        t.pass(near, 1.0);
        let p = t.pass(near + 100, 1.0);
        assert_eq!(
            p,
            Pass {
                draw: true,
                interface: Some(66),
                step: Some(100)
            }
        );
    }

    #[test]
    fn cap_is_unsigned() {
        let mut t = Timers::new(66, 16, u32::MAX);
        t.pass(0, 1.0);
        assert_eq!(t.pass(100_000, 1.0).step, Some(100_000));
    }

    #[test]
    fn next_due_is_the_earliest_firing() {
        let mut t = Timers::new(66, 16, 200);
        assert_eq!(t.next_due(0), 0, "before the first pass");
        t.pass(1000, 1.0);
        assert_eq!(t.next_due(1000), 16);
        assert_eq!(t.next_due(1010), 6);
        assert_eq!(t.next_due(1016), 0);
        assert_eq!(t.next_due(5000), 0);
        // Sleeping until the due time reproduces the spinning loop's firings.
        let mut spinning = Timers::new(66, 16, 200);
        let mut sleeping = Timers::new(66, 16, 200);
        spinning.pass(0, 1.0);
        sleeping.pass(0, 1.0);
        let mut want = Vec::new();
        for now in 1..=1000 {
            let p = spinning.pass(now, 1.0);
            if p != (Pass {
                draw: false,
                interface: None,
                step: None,
            }) {
                want.push((now, p));
            }
        }
        let mut got = Vec::new();
        let mut now = 0;
        loop {
            now += sleeping.next_due(now).max(1);
            if now > 1000 {
                break;
            }
            got.push((now, sleeping.pass(now, 1.0)));
        }
        assert_eq!(got, want);
    }

    fn at(c: &Calendar) -> (u32, u32, u32, u32, u32, u32, u32) {
        let d = c.date();
        (
            d.year,
            d.month,
            d.day,
            d.hour,
            d.minute,
            d.second,
            d.millisecond,
        )
    }

    #[test]
    fn calendar_starts_on_a_monday() {
        let c = Calendar::new();
        assert_eq!(at(&c), (2001, 1, 1, 0, 0, 0, 0));
        assert_eq!(c.date().weekday, 0);
    }

    fn after(ms: u32, step: u32) -> Calendar {
        let mut c = Calendar::new();
        for _ in 0..ms / step {
            c.advance(step);
        }
        c
    }

    #[test]
    fn calendar_worked_example() {
        // Spec 11.7, steps of 67 ms.
        let mut c = Calendar::new();
        c.advance(67);
        assert_eq!(at(&c), (2001, 1, 1, 0, 8, 2, 400), "67 * 7.2 s");
        assert_eq!(at(&after(1005, 67)).3, 2, "1 s of simulated time: 02:00");
        assert_eq!(
            at(&after(12_000, 1)),
            (2001, 1, 2, 0, 0, 0, 0),
            "12 s: 2 January"
        );
        assert_eq!(
            at(&after(60_000, 1)),
            (2001, 1, 6, 0, 0, 0, 0),
            "60 s: 6 January"
        );
        assert_eq!(
            at(&after(372_000, 1)),
            (2001, 2, 1, 0, 0, 0, 0),
            "6 min 12 s: 1 February"
        );
        assert_eq!(
            at(&after(3_600_000, 1)),
            (2001, 10, 28, 0, 0, 0, 0),
            "one hour"
        );
        assert_eq!(after(12_000, 1).date().weekday, 1, "Tuesday");
    }

    #[test]
    fn month_end_comes_one_step_late() {
        let mut c = Calendar::new();
        // 371.999 s: still January.
        for _ in 0..371_999 {
            assert!(!c.advance(1));
        }
        assert_eq!(c.date().month, 1);
        assert!(
            !c.advance(1),
            "the step that crosses midnight only raises the flag"
        );
        assert_eq!(c.date().month, 2);
        assert!(c.advance(1), "the next step runs the month end");
        assert!(!c.advance(1), "once");
    }

    #[test]
    fn calendar_real_months_and_leap_years() {
        let day = 12_000; // ms of step per game day
        // February 2004 has 29 days.
        let c = after(day * (365 * 3 + 31 + 28), 1000);
        assert_eq!(at(&c), (2004, 2, 29, 0, 0, 0, 0));
        let c = after(day * (365 * 3 + 31 + 29), 1000);
        assert_eq!(at(&c), (2004, 3, 1, 0, 0, 0, 0));
        let c = after(day * (365 * 4 + 1), 1000);
        assert_eq!(at(&c), (2005, 1, 1, 0, 0, 0, 0), "a leap year has 366 days");
        // A year takes 73 minutes of simulated time.
        assert_eq!(at(&after(73 * 60_000, 1000)), (2002, 1, 1, 0, 0, 0, 0));
    }
}

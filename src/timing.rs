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
use crate::single;

/// Period of the update timer when `UpdateRate` is absent or out of range: 10 per second.
const FALLBACK_UPDATE_PERIOD: u32 = 100;
/// Period of the draw timer when `DrawRate` is absent or out of range: 30 per second.
const FALLBACK_DRAW_PERIOD: u32 = 33;
/// The default and the replacement of 0 for `MaxSimTick`.
pub(crate) const DEFAULT_MAX_SIM_TICK: u32 = 200;

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

/// The speed factor of the simulation and the developer mode that unlocks its keys (main loop 5.2).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Speed {
    /// `[advanced] devModeEnabled` is not 0: Ctrl+D may switch the developer mode on.
    enabled: bool,
    developer: bool,
    factor: f64,
}

impl Speed {
    /// Speed 1.0, developer mode off.
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            developer: false,
            factor: 1.0,
        }
    }

    pub(crate) fn factor(&self) -> f64 {
        self.factor
    }

    pub(crate) fn developer(&self) -> bool {
        self.developer
    }

    /// Ctrl+D: switches the developer mode, if the ini allows it. Whether it changed.
    pub(crate) fn toggle(&mut self) -> bool {
        self.developer ^= self.enabled;
        self.enabled
    }

    /// A character key: `.` adds 0.1, `,` subtracts 0.1 and `/` sets 1.0, in developer mode.
    /// Each sum is rounded to `f32` (main loop 5.2, 5.4): 1.2000000476837158 after two `.`, and
    /// `,` stops at 0.09999992698431015, which is not above 0.1. Whether the key acted.
    pub(crate) fn key(&mut self, byte: u8) -> bool {
        if !self.developer {
            return false;
        }
        match byte {
            b'.' => self.factor = single(self.factor + 0.1),
            b',' if self.factor > 0.1 => self.factor = single(self.factor - 0.1),
            b',' => {}
            b'/' => self.factor = 1.0,
            _ => return false,
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct Timers {
    /// The speed factor `pass` is called with.
    pub(crate) speed: Speed,
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
    pub(crate) fn new(update_period: u32, draw_period: u32, cap: u32) -> Self {
        Self {
            speed: Speed::new(false),
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
        let value = |key: &str| ini_value(ini, "user", key);
        let period = |key, fallback| value(key).map_or(fallback, |v| period_of(v, fallback));
        let cap = value("MaxSimTick")
            .and_then(whole)
            .filter(|&n| n != 0)
            .map_or(DEFAULT_MAX_SIM_TICK, |n| n as u32);
        let mut timers = Self::new(
            period("UpdateRate", FALLBACK_UPDATE_PERIOD),
            period("DrawRate", FALLBACK_DRAW_PERIOD),
            cap,
        );
        timers.speed =
            Speed::new(ini_whole(ini, "advanced", "devModeEnabled").is_some_and(|n| n != 0));
        timers
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
            // Product rounded to 24 bits, truncated toward zero (a saturating cast), capped.
            (single(elapsed as f64 * speed) as u64).min(u64::from(self.cap)) as u32
        });
        Pass {
            draw,
            interface: interface.then_some(self.update_period),
            step,
        }
    }

    /// Forgets when the simulation last ran: its next step is the cap, as after a load that took
    /// longer than the cap (main loop 3.5).
    pub(crate) fn forget_simulation(&mut self) {
        self.last_simulation = None;
    }

    /// Milliseconds from `now` to the earliest time at which a pass can fire a timer: 0 when
    /// one is due. A caller that sleeps this long runs the same firings as one that spins.
    pub(crate) fn next_due(&self, now: u64) -> u64 {
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

/// The value of a key of a section of the ini file; section and key names ignore case.
pub(crate) fn ini_value<'a>(ini: &'a Config, section: &str, key: &str) -> Option<&'a [u8]> {
    ini_values(ini, section, key).first().map(Vec::as_slice)
}

/// All values of a key of a section of the ini file, in file order; section and key names ignore
/// case. Empty if the key is missing.
pub(crate) fn ini_values<'a>(ini: &'a Config, section: &str, key: &str) -> &'a [Vec<u8>] {
    ini.sections()
        .filter(|(name, _)| name.eq_ignore_ascii_case(section.as_bytes()))
        .find_map(|(_, section)| {
            let k = section
                .keys()
                .find(|k| k.eq_ignore_ascii_case(key.as_bytes()))?;
            Some(section.get_all(k)).filter(|v| !v.is_empty())
        })
        .unwrap_or(&[])
}

/// A whole-number setting of `[user]` (`screenwidth`, ...), read like the loop settings.
pub fn user_whole(ini: &Config, key: &str) -> Option<i32> {
    ini_whole(ini, "user", key)
}

/// A whole-number setting of any section of the ini file, read like the loop settings.
pub fn ini_whole(ini: &Config, section: &str, key: &str) -> Option<i32> {
    ini_value(ini, section, key).and_then(whole)
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
pub(crate) const DAY_MS: u64 = 86_400_000;
/// Days from 1970-01-01 to 2001-01-01.
const DAYS_TO_2001: i64 = 11_323;
/// Days from 1601-01-01 to 1970-01-01.
const DAYS_1601_TO_1970: i64 = 134_774;

/// A date and time of day of the proleptic Gregorian calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Date {
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
/// per millisecond of step, in whole numbers. A saved date before the start is a negative time.
#[derive(Debug, Clone, Default)]
pub(crate) struct Calendar {
    /// Game milliseconds since the start (negative before it).
    elapsed: i64,
    /// Raised by the step that changed the month; acted on by the next one.
    month_changed: bool,
}

impl Calendar {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A calendar that stands at `elapsed` game milliseconds after the start (a saved zoo's date).
    pub(crate) fn at(elapsed: i64) -> Self {
        Self {
            elapsed,
            month_changed: false,
        }
    }

    /// Advances by one simulation step. Returns whether the previous step changed the month:
    /// the month-end work (the finance check) belongs to this step, one late. A step that
    /// changes the month raises the flag for the next one.
    pub(crate) fn advance(&mut self, step: u32) -> bool {
        let month_end = std::mem::take(&mut self.month_changed);
        let before = self.date().month;
        self.elapsed += i64::from(step) * GAME_MS_PER_STEP_MS as i64;
        self.month_changed = self.date().month != before;
        month_end
    }

    /// Moves to the last millisecond of the current month; the next step changes the month.
    pub(crate) fn skip_to_month_end(&mut self) {
        let d = self.date();
        let (year, month) = if d.month == 12 {
            (d.year + 1, 1)
        } else {
            (d.year, d.month + 1)
        };
        let next = Date {
            year,
            month,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            millisecond: 0,
            ..d
        };
        self.elapsed = self.elapsed.max(next.game_ms() - 1);
    }

    /// The game milliseconds since the start and the month flag, for the state hash.
    pub(crate) fn state(&self) -> (i64, bool) {
        (self.elapsed, self.month_changed)
    }

    /// The game milliseconds the timers count from the start, negative before it: a stamp taken
    /// before 2001 ages from its own date, and the difference of two timers is the time between.
    pub(crate) fn elapsed_ms(&self) -> i64 {
        self.elapsed
    }

    pub(crate) fn date(&self) -> Date {
        let day_ms = DAY_MS as i64;
        let days = self.elapsed.div_euclid(day_ms) + DAYS_TO_2001;
        Date::from_days(days, self.elapsed.rem_euclid(day_ms) as u32)
    }
}

/// The computer's calendar day in UTC, as (month, day) (cheats 6.1 point 2). A test build answers
/// (0, 0): no test depends on today's date.
pub(crate) fn utc_day() -> (u32, u32) {
    if cfg!(test) {
        return (0, 0);
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    utc_day_of(now.map_or(0, |d| d.as_secs()))
}

/// The (month, day) in UTC of `secs` seconds after 1 January 1970.
fn utc_day_of(secs: u64) -> (u32, u32) {
    let date = Date::from_days((secs / 86_400) as i64, 0);
    (date.month, date.day)
}

/// The days of a month of a year of the proleptic Gregorian calendar (`month` 1 to 12).
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl Date {
    /// The date `days` after 1 January 1970 and `ms` into the day.
    fn from_days(days: i64, ms: u32) -> Date {
        // Civil date from a day count (days since 1970-01-01).
        let z = days + 719_468;
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
            weekday: (days + 3).rem_euclid(7) as u32,
            hour: ms / 3_600_000,
            minute: ms / 60_000 % 60,
            second: ms / 1000 % 60,
            millisecond: ms % 1000,
        }
    }

    /// Milliseconds from 1 January 1970 to this date and time of day.
    pub(crate) fn millis(&self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = i64::from((self.month + 9) % 12);
        let doy = (153 * mp + 2) / 5 + i64::from(self.day) - 1;
        let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
        let day_ms = i64::from(self.hour) * 3_600_000
            + i64::from(self.minute) * 60_000
            + i64::from(self.second) * 1000
            + i64::from(self.millisecond);
        days * DAY_MS as i64 + day_ms
    }

    /// Game milliseconds from the calendar's start to this date, negative before it. A date the
    /// system's conversion refuses (a field out of range, a day the month does not have, a year
    /// before 1601 or after 30827) is the start, 0.
    pub(crate) fn game_ms(&self) -> i64 {
        let ok = (1..=12).contains(&self.month)
            && (1..=days_in_month(self.year, self.month)).contains(&self.day)
            && self.hour < 24
            && self.minute < 60
            && self.second < 60
            && self.millisecond < 1000
            && (1601..=30827).contains(&self.year);
        let start = DAYS_TO_2001 * DAY_MS as i64;
        if ok { self.millis() - start } else { 0 }
    }

    /// The date of a file time: 100 ns steps since 1 January 1601 (zoo format 1).
    pub(crate) fn from_file_time(t: u64) -> Date {
        let ms = t / 10_000;
        Date::from_days(
            (ms / DAY_MS) as i64 - DAYS_1601_TO_1970,
            (ms % DAY_MS) as u32,
        )
    }
}

/// The file time (100 ns steps since 1 January 1601, zoo format 1) of a game date in game
/// milliseconds from the calendar's start; 0 before 1601.
pub(crate) fn file_time(game_ms: i64) -> u64 {
    let ms = game_ms.saturating_add((DAYS_TO_2001 + DAYS_1601_TO_1970) * DAY_MS as i64);
    u64::try_from(ms).map_or(0, |ms| ms.saturating_mul(10_000))
}

#[cfg(test)]
mod tests;

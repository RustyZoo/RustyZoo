//! The month and weekday names and the number and money forms of the user's regional settings: the time locale of the
//! environment (`LC_ALL`, `LC_TIME`, `LANG`) on Linux and macOS, the user locale on Windows. On
//! macOS, where a game started from the Finder has none of them, the region of the System
//! Settings stands in.

use super::Names;
use super::profile::{Marks, Profile};

/// Runs `read` with the thread's locale set to the user's for the categories of `mask` (or to
/// the locale `name`, tests only); none when the system has no such locale.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn with_locale<T>(mask: libc::c_int, name: &str, read: impl FnOnce() -> Option<T>) -> Option<T> {
    #[cfg(target_os = "macos")]
    let name = if name.is_empty() && !environment_gives_locale() {
        region_locale().unwrap_or_default()
    } else {
        name.to_string()
    };
    let name = std::ffi::CString::new(name).ok()?;
    // SAFETY: the locale object is made, used on this thread only and freed here; `read` copies
    // what it needs before the thread's locale is put back and the object freed.
    unsafe {
        let locale = libc::newlocale(mask, name.as_ptr(), std::ptr::null_mut());
        if locale.is_null() {
            return None;
        }
        let old = libc::uselocale(locale);
        let result = read();
        libc::uselocale(old);
        libc::freelocale(locale);
        result
    }
}

/// The names of the user's time locale, or none when the system gives none (or not as UTF-8).
/// `name` is a locale name to use instead of the user's (tests only).
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn names(name: &str) -> Option<Names> {
    use std::ffi::CStr;
    with_locale(libc::LC_TIME_MASK, name, || {
        let text = |item| {
            // SAFETY: nl_langinfo gives a valid string, copied here.
            unsafe { CStr::from_ptr(libc::nl_langinfo(item)) }
                .to_str()
                .ok()
                .map(String::from)
        };
        // The items of each table are consecutive; a week starts on Sunday, ours on Monday.
        let month = |first, m: usize| text(first + m as libc::nl_item);
        let weekday = |first, w: usize| text(first + ((w + 1) % 7) as libc::nl_item);
        Some(Names {
            months: try_array(|m| month(libc::MON_1, m))?,
            short_months: try_array(|m| month(libc::ABMON_1, m))?,
            weekdays: try_array(|w| weekday(libc::DAY_1, w))?,
            short_weekdays: try_array(|w| weekday(libc::ABDAY_1, w))?,
        })
    })
}

/// The number and money forms of the user's locale, or none when the system gives none (or not
/// as UTF-8). `name` is a locale name to use instead of the user's (tests only). What the locale
/// leaves unspecified (the C locale) keeps the US form.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn profile(name: &str) -> Option<Profile> {
    use std::ffi::CStr;
    with_locale(libc::LC_NUMERIC_MASK | libc::LC_MONETARY_MASK, name, || {
        // SAFETY: localeconv gives the thread's locale, valid until the next call; every string
        // is copied here.
        let c = unsafe { &*libc::localeconv() };
        let text = |s: *mut libc::c_char| {
            // SAFETY: the strings of the structure are valid and closed by a zero.
            unsafe { CStr::from_ptr(s) }.to_str().ok().map(String::from)
        };
        // The first group size; a first byte of 0 or CHAR_MAX means no grouping.
        let size = |s: *mut libc::c_char| match unsafe { *s } {
            n @ 1..=126 => n as usize,
            _ => 0,
        };
        let marks = |decimal, group, grouping| {
            let us = Marks::new(".", ",", 3);
            let (decimal, group) = (text(decimal)?, text(group)?);
            Some(
                match (group.is_empty() && size(grouping) == 0, decimal.is_empty()) {
                    (true, _) => us, // unspecified
                    (_, true) => Marks::new(".", &group, size(grouping)),
                    _ => Marks::new(&decimal, &group, size(grouping)),
                },
            )
        };
        let (n, m) = (
            marks(c.decimal_point, c.thousands_sep, c.grouping)?,
            marks(c.mon_decimal_point, c.mon_thousands_sep, c.mon_grouping)?,
        );
        let int = |v: libc::c_char| i32::from(v);
        Some(Profile::from_c(
            n,
            m,
            (int(c.p_cs_precedes), int(c.p_sep_by_space)),
            (
                int(c.n_cs_precedes),
                int(c.n_sep_by_space),
                int(c.n_sign_posn),
            ),
        ))
    })
}

/// Whether `LC_ALL`, `LC_TIME` or `LANG` is set to something.
#[cfg(target_os = "macos")]
fn environment_gives_locale() -> bool {
    ["LC_ALL", "LC_TIME", "LANG"]
        .iter()
        .any(|v| std::env::var_os(v).is_some_and(|v| !v.is_empty()))
}

/// The libc locale name of the region set in the System Settings (`fr_CH.UTF-8`), if any.
#[cfg(target_os = "macos")]
fn region_locale() -> Option<String> {
    use std::ffi::{c_char, c_void};
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFLocaleCopyCurrent() -> *const c_void;
        fn CFLocaleGetIdentifier(locale: *const c_void) -> *const c_void;
        fn CFStringGetCString(s: *const c_void, buf: *mut c_char, size: isize, enc: u32) -> u8;
        fn CFRelease(object: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100; // kCFStringEncodingUTF8
    let mut buffer = [0 as c_char; 128];
    // SAFETY: the locale is owned (Copy) and released once; the identifier belongs to it and is
    // copied into the buffer, whose size is passed, before the release.
    let id = unsafe {
        let locale = CFLocaleCopyCurrent();
        if locale.is_null() {
            return None;
        }
        let id = CFLocaleGetIdentifier(locale);
        let ok = !id.is_null()
            && CFStringGetCString(id, buffer.as_mut_ptr(), buffer.len() as isize, UTF8) != 0;
        CFRelease(locale);
        ok.then(|| std::ffi::CStr::from_ptr(buffer.as_ptr()))?
    };
    region_locale_name(id.to_str().ok()?)
}

/// The libc locale name (`fr_CH.UTF-8`) of a CFLocale identifier (`fr_CH`, `zh-Hans_CN`,
/// `en_US@calendar=gregorian`): its language and its region; none when it has no region.
#[cfg(any(target_os = "macos", test))]
pub(super) fn region_locale_name(identifier: &str) -> Option<String> {
    let identifier = identifier.split('@').next()?;
    let mut parts = identifier.split(['_', '-']);
    let language = parts
        .next()
        .filter(|l| l.chars().all(|c| c.is_ascii_lowercase()))?;
    let region = parts
        .next_back()
        .filter(|r| r.len() == 2 && r.chars().all(|c| c.is_ascii_uppercase()))?;
    (!language.is_empty()).then(|| format!("{language}_{region}.UTF-8"))
}

/// The names of the user locale (LOCALE_USER_DEFAULT); `name` is not used.
#[cfg(windows)]
pub(super) fn names(_name: &str) -> Option<Names> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocaleInfoW(locale: u32, item: u32, data: *mut u16, len: i32) -> i32;
    }
    const USER_DEFAULT: u32 = 0x400;
    let text = |item: u32, i: usize| {
        let mut buffer = [0u16; 80];
        // SAFETY: the buffer is as long as the length passed.
        let len = unsafe { GetLocaleInfoW(USER_DEFAULT, item + i as u32, buffer.as_mut_ptr(), 80) };
        // The length counts the closing zero; 0 is a failure.
        String::from_utf16(buffer.get(..usize::try_from(len).ok()?.checked_sub(1)?)?).ok()
    };
    Some(Names {
        months: try_array(|m| text(0x38, m))?, // LOCALE_SMONTHNAME1
        short_months: try_array(|m| text(0x44, m))?, // LOCALE_SABBREVMONTHNAME1
        weekdays: try_array(|w| text(0x2a, w))?, // LOCALE_SDAYNAME1, Monday
        short_weekdays: try_array(|w| text(0x31, w))?, // LOCALE_SABBREVDAYNAME1
    })
}

/// The number and money forms of the user locale (LOCALE_USER_DEFAULT); `name` is not used.
#[cfg(windows)]
pub(super) fn profile(_name: &str) -> Option<Profile> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocaleInfoW(locale: u32, item: u32, data: *mut u16, len: i32) -> i32;
    }
    const USER_DEFAULT: u32 = 0x400;
    let text = |item: u32| {
        let mut buffer = [0u16; 80];
        // SAFETY: the buffer is as long as the length passed.
        let len = unsafe { GetLocaleInfoW(USER_DEFAULT, item, buffer.as_mut_ptr(), 80) };
        String::from_utf16(buffer.get(..usize::try_from(len).ok()?.checked_sub(1)?)?).ok()
    };
    // "3;2;0" groups by 3 then 2: only the first size counts.
    let marks = |decimal, group, grouping| {
        let size = text(grouping)?.split(';').next()?.parse().ok()?;
        Some(Marks::new(&text(decimal)?, &text(group)?, size))
    };
    Some(Profile::from_windows(
        marks(0x0e, 0x0f, 0x10)?,  // LOCALE_SDECIMAL, STHOUSAND, SGROUPING
        marks(0x16, 0x17, 0x18)?,  // LOCALE_SMONDECIMALSEP, SMONTHOUSANDSEP, SMONGROUPING
        text(0x1b)?.parse().ok()?, // LOCALE_ICURRENCY
        text(0x1c)?.parse().ok()?, // LOCALE_INEGCURR
    ))
}

/// No regional settings are read on other systems.
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(super) fn names(_name: &str) -> Option<Names> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(super) fn profile(_name: &str) -> Option<Profile> {
    None
}

/// An array of `N` names, or none if one fails.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn try_array<const N: usize>(name: impl FnMut(usize) -> Option<String>) -> Option<[String; N]> {
    let names: Vec<String> = (0..N).map(name).collect::<Option<_>>()?;
    names.try_into().ok()
}

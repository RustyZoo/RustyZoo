//! The month and weekday names of the user's regional settings: the time locale of the
//! environment (`LC_ALL`, `LC_TIME`, `LANG`) on Linux and macOS, the user locale on Windows. On
//! macOS, where a game started from the Finder has none of them, the region of the System
//! Settings stands in.

use super::Names;

/// The names of the user's time locale, or none when the system gives none (or not as UTF-8).
/// `name` is a locale name to use instead of the user's (tests only).
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn names(name: &str) -> Option<Names> {
    use std::ffi::{CStr, CString};
    #[cfg(target_os = "macos")]
    let name = if name.is_empty() && !environment_gives_locale() {
        region_locale().unwrap_or_default()
    } else {
        name.to_string()
    };
    let name = CString::new(name).ok()?;
    // SAFETY: the locale object is made, used on this thread only and freed here; each name is
    // copied before the thread's locale is put back and the object freed.
    unsafe {
        let locale = libc::newlocale(libc::LC_TIME_MASK, name.as_ptr(), std::ptr::null_mut());
        if locale.is_null() {
            return None;
        }
        let old = libc::uselocale(locale);
        let text = |item| {
            CStr::from_ptr(libc::nl_langinfo(item))
                .to_str()
                .ok()
                .map(String::from)
        };
        // The items of each table are consecutive; a week starts on Sunday, ours on Monday.
        let month = |first, m: usize| text(first + m as libc::nl_item);
        let weekday = |first, w: usize| text(first + ((w + 1) % 7) as libc::nl_item);
        let names = (|| {
            Some(Names {
                months: try_array(|m| month(libc::MON_1, m))?,
                short_months: try_array(|m| month(libc::ABMON_1, m))?,
                weekdays: try_array(|w| weekday(libc::DAY_1, w))?,
                short_weekdays: try_array(|w| weekday(libc::ABDAY_1, w))?,
            })
        })();
        libc::uselocale(old);
        libc::freelocale(locale);
        names
    }
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

/// No regional settings are read on other systems.
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(super) fn names(_name: &str) -> Option<Names> {
    None
}

/// An array of `N` names, or none if one fails.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn try_array<const N: usize>(name: impl FnMut(usize) -> Option<String>) -> Option<[String; N]> {
    let names: Vec<String> = (0..N).map(name).collect::<Option<_>>()?;
    names.try_into().ok()
}

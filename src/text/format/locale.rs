//! The month and weekday names of the user's regional settings: the time locale of the
//! environment (`LC_ALL`, `LC_TIME`, `LANG`) on Linux and macOS, the user locale on Windows.

use super::Names;

/// The names of the user's time locale, or none when the system gives none (or not as UTF-8).
/// `name` is a locale name to use instead of the user's (tests only).
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn names(name: &str) -> Option<Names> {
    use std::ffi::{CStr, CString};
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

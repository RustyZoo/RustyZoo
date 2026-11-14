//! The console of a Windows program built for the windows subsystem, which gets none of its own.

use std::ffi::c_void;

const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
/// `STD_INPUT_HANDLE`, `STD_OUTPUT_HANDLE` and `STD_ERROR_HANDLE` (-10, -11, -12 as unsigned).
const STD_HANDLES: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32];

#[link(name = "kernel32")]
unsafe extern "system" {
    fn AttachConsole(process: u32) -> i32;
    fn GetStdHandle(which: u32) -> *mut c_void;
    fn SetStdHandle(which: u32, handle: *mut c_void) -> i32;
}

/// Writes to the console of the terminal the program was started from, if any (none from the file
/// manager); a redirected standard handle (a pipe, a file) is kept.
pub fn attach() {
    // SAFETY: plain Win32 calls on the process's own standard handles.
    unsafe {
        let kept = STD_HANDLES.map(|which| (which, GetStdHandle(which)));
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        for (which, handle) in kept {
            // Null when not inherited, -1 (INVALID_HANDLE_VALUE) on an error.
            if !handle.is_null() && handle.addr() != usize::MAX {
                SetStdHandle(which, handle);
            }
        }
    }
}

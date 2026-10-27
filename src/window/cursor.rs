//! The mouse cursor of the window: the original's, read from the install at the first time it is
//! shown, else the system's arrow. A cursor that cannot be made only warns.

use std::collections::HashMap;
use std::path::Path;

use rustyzoo::cursor;
use sdl3::mouse::{Cursor, SystemCursor};
use sdl3::pixels::PixelFormat;
use sdl3::surface::Surface;

pub struct Cursors<'a> {
    install: &'a Path,
    made: HashMap<u32, Option<Cursor>>,
    shown: Option<u32>,
}

impl<'a> Cursors<'a> {
    pub fn new(install: &'a Path) -> Self {
        Cursors {
            install,
            made: HashMap::new(),
            shown: None,
        }
    }

    /// Shows cursor `number` (with the 50 of the alternate set added), if it is not shown yet.
    pub fn show(&mut self, number: u32) {
        if self.shown.replace(number) == Some(number) {
            return;
        }
        let install = self.install;
        if let Some(cursor) = self
            .made
            .entry(number)
            .or_insert_with(|| make(install, number))
        {
            cursor.set();
        }
    }
}

fn make(install: &Path, number: u32) -> Option<Cursor> {
    let made = cursor::load(install, number).map(|mut c| {
        let surface = Surface::from_data(
            &mut c.rgba,
            c.width,
            c.height,
            c.width * 4,
            PixelFormat::RGBA32,
        )?;
        Cursor::from_surface(surface, c.hot.0 as i32, c.hot.1 as i32)
    });
    match made {
        Some(Ok(cursor)) => return Some(cursor),
        Some(Err(e)) => eprintln!("rustyzoo: warning: cursor {number}: {e}"),
        None => {
            eprintln!("rustyzoo: warning: no cursor {number} in the install, the arrow is used")
        }
    }
    match Cursor::from_system(SystemCursor::Arrow) {
        Ok(cursor) => Some(cursor),
        Err(e) => {
            eprintln!("rustyzoo: warning: {e}");
            None
        }
    }
}

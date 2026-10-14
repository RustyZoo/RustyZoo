//! The game state of the map view and its pass: the map, the staged picture, the camera, the
//! timers, the calendar and the pause flag, driven by [`Command`]s. No window and no input
//! events: a shell turns what the player does into commands and presents the frames that
//! [`App::pass`] paints, so a windowed run and a scripted one share every line of this.
//!
//! One pass applies the commands first (in the original they arrive between two passes, main
//! loop 3.1), then runs the timers in their order: the draw, the interface (the camera's edge
//! scroll), the simulation (only the calendar, so far; the pause stops it, main loop 3.3).

use std::fmt;
use std::str::FromStr;

use crate::Framebuffer;
use crate::camera::{Camera, Settings};
use crate::render::{MapRenderer, RenderError, Staged, check_size};
use crate::timing::{Calendar, Pass, Timers};
use crate::zoo::Zoo;

/// What the player does in the map view (input 4). The text form is the script format of a
/// headless run: `scroll DX DY`, `pointer X Y`, `pointer none`, `rotate cw`, `rotate ccw`,
/// `zoom in`, `zoom out`, `pause`, `quit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// One key-down message of a scroll key: the direction on each axis, -1, 0 or 1.
    Scroll(i32, i32),
    /// The pointer is at this position in the window, or has left it (or the window is not
    /// active): the edge scroll of the interface pass reads it.
    Pointer(Option<(i32, i32)>),
    /// A quarter turn, clockwise or not.
    Rotate(bool),
    /// Zoom in or out.
    Zoom(bool),
    /// Toggle the pause.
    Pause,
    /// Close the game.
    Quit,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Command::Scroll(x, y) => write!(f, "scroll {x} {y}"),
            Command::Pointer(Some((x, y))) => write!(f, "pointer {x} {y}"),
            Command::Pointer(None) => f.write_str("pointer none"),
            Command::Rotate(cw) => write!(f, "rotate {}", if cw { "cw" } else { "ccw" }),
            Command::Zoom(zoom_in) => write!(f, "zoom {}", if zoom_in { "in" } else { "out" }),
            Command::Pause => f.write_str("pause"),
            Command::Quit => f.write_str("quit"),
        }
    }
}

impl FromStr for Command {
    type Err = String;

    fn from_str(text: &str) -> Result<Command, String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let number = |w: &str| w.parse::<i32>().map_err(|_| format!("not a number: {w:?}"));
        Ok(match words.as_slice() {
            ["scroll", x, y] => {
                let (x, y) = (number(x)?, number(y)?);
                if !(-1..=1).contains(&x) || !(-1..=1).contains(&y) {
                    return Err("scroll takes directions from -1 to 1".into());
                }
                Command::Scroll(x, y)
            }
            ["pointer", "none"] => Command::Pointer(None),
            ["pointer", x, y] => Command::Pointer(Some((number(x)?, number(y)?))),
            ["rotate", "cw"] => Command::Rotate(true),
            ["rotate", "ccw"] => Command::Rotate(false),
            ["zoom", "in"] => Command::Zoom(true),
            ["zoom", "out"] => Command::Zoom(false),
            ["pause"] => Command::Pause,
            ["quit"] => Command::Quit,
            _ => return Err(format!("not a command: {text:?}")),
        })
    }
}

/// A pass in which no timer fired.
const IDLE: Pass = Pass {
    draw: false,
    interface: None,
    step: None,
};

/// What the camera looked like when a frame was painted: the frame changes with nothing else.
type Look = ((i32, i32), i32, u8);

pub struct App<'a> {
    renderer: MapRenderer<'a>,
    zoo: Zoo,
    staged: Staged,
    camera: Camera,
    timers: Timers,
    calendar: Calendar,
    paused: bool,
    quit: bool,
    pointer: Option<(i32, i32)>,
    frame: Framebuffer,
    /// The look of the frame in `frame`; `None` before the first one.
    painted: Option<Look>,
    /// What the last pass did: the commands it applied and the timers it fired.
    applied: Vec<Command>,
    fired: Pass,
    /// Whether the last pass painted a frame.
    drew: bool,
    /// The camera when the last draw fired, before the interface moved it.
    drawn_at: Look,
}

impl<'a> App<'a> {
    /// A game on `zoo` seen through a window of `size` pixels, with the camera the map stores.
    pub fn new(
        mut renderer: MapRenderer<'a>,
        zoo: Zoo,
        settings: Settings,
        timers: Timers,
        size: (u32, u32),
    ) -> Result<App<'a>, RenderError> {
        check_size(&[0, 0, i64::from(size.0), i64::from(size.1)])?;
        let camera = Camera::open(&zoo.view, (size.0 as i32, size.1 as i32), settings);
        let staged = renderer.stage(&zoo, camera.rotation, false)?;
        let drawn_at = (camera.scroll, camera.zoom, camera.rotation);
        Ok(App {
            renderer,
            zoo,
            staged,
            camera,
            timers,
            calendar: Calendar::new(),
            paused: false,
            quit: false,
            pointer: None,
            frame: Framebuffer::new(size.0, size.1),
            painted: None,
            applied: Vec::new(),
            fired: IDLE,
            drew: false,
            drawn_at,
        })
    }

    /// One pass at the clock reading `now` (ms) with the commands that came before it. Returns
    /// the frame when this pass painted one: a due draw paints only when the camera has moved
    /// since the last frame. A `Quit` ends the game before the rest of the pass: the commands
    /// after it and the timers do not run (main loop 3.1), and neither does any later pass.
    pub fn pass(&mut self, now: u64, commands: &[Command]) -> Option<&Framebuffer> {
        (self.applied, self.fired, self.drew) = (Vec::new(), IDLE, false);
        for &command in commands {
            if self.quit {
                return None;
            }
            self.applied.push(command);
            self.apply(command);
        }
        if self.quit {
            return None;
        }
        self.fired = self.timers.pass(now, 1.0);
        if self.fired.draw {
            self.drawn_at = self.look();
            self.drew = self.paint();
        }
        if self.fired.interface.is_some() {
            self.camera.edge_scroll(&self.zoo, now, self.pointer);
        }
        if let Some(step) = self.fired.step
            && !self.paused
        {
            self.calendar.advance(step);
        }
        self.drew.then_some(&self.frame)
    }

    /// Milliseconds from `now` to the next time a pass fires a timer: 0 when one is due.
    pub fn next_due(&self, now: u64) -> u64 {
        self.timers.next_due(now)
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Scroll(x, y) => self.camera.key_scroll(&self.zoo, (x, y)),
            Command::Pointer(at) => self.pointer = at,
            Command::Rotate(clockwise) => {
                let before = self.camera.clone();
                self.camera.rotate(&self.zoo, None, clockwise);
                // Restage from the new side. If that fails the view stays as it was.
                match self.renderer.stage(&self.zoo, self.camera.rotation, false) {
                    Ok(staged) => self.staged = staged,
                    Err(_) => self.camera = before,
                }
            }
            Command::Zoom(zoom_in) => self.camera.zoom(&self.zoo, None, zoom_in),
            Command::Pause => self.paused = !self.paused,
            Command::Quit => self.quit = true,
        }
    }

    /// Paints the window of the view if it differs from the last frame.
    fn paint(&mut self) -> bool {
        let look = self.look();
        if self.painted == Some(look) {
            return false;
        }
        // The one place that draws; the zoom of the camera applies here.
        self.staged.draw(
            &mut self.frame,
            self.camera.scroll.0,
            self.camera.scroll.1,
            self.camera.zoom,
        );
        self.painted = Some(look);
        true
    }

    fn look(&self) -> Look {
        (self.camera.scroll, self.camera.zoom, self.camera.rotation)
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Whether the player asked to close the game.
    pub fn quit(&self) -> bool {
        self.quit
    }

    /// The timers that fired in the last pass.
    pub fn fired(&self) -> Pass {
        self.fired
    }

    /// The first line of a log: the size of the window, which the frames depend on.
    pub fn log_header(&self) -> String {
        format!("size {}x{}", self.frame.width, self.frame.height)
    }

    /// The lines of the last pass, each led by the pass index and the clock: one per command it
    /// applied, in the text form of a script, then one per timer that fired, in the order draw,
    /// interface, step. A draw tells the hash of the frame it painted (`-` if it painted none)
    /// and the camera it drew with, a step the new date. A pass that did nothing has no line.
    /// A log, with the size of the window, is all a replay needs: the commands and the clock of
    /// each pass that did something (`run --replay`). A windowed run prints the same lines.
    pub fn log_lines(&self, pass: u64, now: u64) -> Vec<String> {
        let mut lines = Vec::new();
        let lead = format!("pass {pass} t={now}");
        lines.extend(self.applied.iter().map(|c| format!("{lead} {c}")));
        if self.fired.draw {
            let hash = match self.drew {
                true => format!("{:016x}", frame_hash(&self.frame)),
                false => "-".into(),
            };
            let ((x, y), zoom, rotation) = self.drawn_at;
            lines.push(format!(
                "{lead} draw {hash} view={x},{y} rotation={rotation} zoom={zoom}"
            ));
        }
        if let Some(period) = self.fired.interface {
            lines.push(format!("{lead} interface {period}"));
        }
        if let Some(step) = self.fired.step {
            let d = self.calendar.date();
            let paused = if self.paused { " paused" } else { "" };
            lines.push(format!(
                "{lead} step {step} {:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}{paused}",
                d.year, d.month, d.day, d.hour, d.minute, d.second, d.millisecond
            ));
        }
        lines
    }
}

/// FNV-1a over the pixels of a frame.
pub fn frame_hash(frame: &Framebuffer) -> u64 {
    frame.pixels.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests;

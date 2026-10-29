//! The tooltip of the element under the pointer (ui layout 7.7).
//!
//! When the pointer comes onto another element (or another row of a list) the tooltip is hidden
//! and, if that element has a help string, a countdown of `[UI] TooltipDelay` milliseconds starts
//! on the interface updates. When it ends the tooltip, the `UIText` 7003 of the layouts, is made
//! as large as its text asks, put at the pointer 30 pixels lower (kept inside the window) and
//! shown; being a timed element it hides itself after its own `timerDuration`.

use super::Interface;
use crate::config::Config;
use crate::timing::ini_whole;
use crate::ui::draw::Strings;
use crate::ui::layout::{HIDDEN, Kind, NOT_HIT};

/// The id of the tooltip element.
const TOOLTIP_ID: i32 = 7003;
/// How far under the pointer the tooltip is put.
const OFFSET: i32 = 30;
/// Added to a help string number when `[UI] helpType` is not 0 (the long help).
const LONG_HELP: i32 = 30000;
/// The widest tooltip a setting can ask for.
const MAX_WIDTH: i32 = 4096;
/// The longest a setting can keep a popup open: one hour.
const MAX_POPUP_DELAY: i32 = 3_600_000;
/// The string numbers that carry the tooltips of [`Interface::set_tip`]: this plus the element's id.
const OWN_TIPS: i32 = 1_000_000_000;

/// The settings of the section `[UI]` that the interface reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Milliseconds from the pointer coming onto an element to its tooltip.
    pub tooltip_delay: u32,
    /// Whether tooltips are the long help (the help string number plus 30000).
    pub long_help: bool,
    /// The widest a short and a long tooltip gets before it wraps.
    pub max_short_width: i32,
    pub max_long_width: i32,
    /// Milliseconds a popup that does not stop the game stays open (`NonmodalDialogDelay`).
    pub popup_delay: i32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            tooltip_delay: 1,
            long_help: false,
            max_short_width: 200,
            max_long_width: 400,
            popup_delay: 10_000,
        }
    }
}

impl Settings {
    /// Reads `[UI]` of the ini file: `TooltipDelay` (below 0 is 0), `helpType` (not 0: the long
    /// help), `maxShortTooltipWidth` and `maxLongTooltipWidth` (from 1 to 4096) and
    /// `NonmodalDialogDelay` (up to an hour; 0 or less never closes a popup). A missing key
    /// keeps the built-in value.
    pub fn from_config(ini: &Config) -> Settings {
        let d = Settings::default();
        let get = |key: &str| ini_whole(ini, "UI", key);
        Settings {
            tooltip_delay: get("TooltipDelay").map_or(d.tooltip_delay, |n| n.max(0) as u32),
            long_help: get("helpType").map_or(d.long_help, |n| n != 0),
            max_short_width: get("maxShortTooltipWidth")
                .map_or(d.max_short_width, |n| n.clamp(1, MAX_WIDTH)),
            max_long_width: get("maxLongTooltipWidth")
                .map_or(d.max_long_width, |n| n.clamp(1, MAX_WIDTH)),
            // Kept as read: 0 or less never closes the popup.
            popup_delay: get("NonmodalDialogDelay")
                .map_or(d.popup_delay, |n| n.min(MAX_POPUP_DELAY)),
        }
    }
}

/// What the tooltip is waiting for.
#[derive(Default)]
pub(super) struct Tip {
    /// The element under the pointer, and its hovered row if it is a list.
    key: Option<(usize, Option<usize>)>,
    /// Milliseconds left of the delay; `None` when no tooltip is due.
    left: Option<i64>,
    text: Vec<u8>,
}

impl Interface {
    /// Replaces the settings the interface reads from the ini file.
    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    /// Gives the element `id` a tooltip of its own, `text`; an empty text takes it away. `false`
    /// if there is no such element.
    pub fn set_tip(&mut self, id: i32, text: String) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        let key = OWN_TIPS.saturating_add(id);
        self.strings.replaced.insert(key, text.clone());
        self.tree.elements[i].help_id = if text.is_empty() { vec![] } else { vec![key] };
        true
    }

    /// The tooltip element is never hit.
    pub(super) fn start_tooltip(&mut self) {
        if let Some(i) = self.tooltip_element() {
            self.tree.elements[i].state |= NOT_HIT;
        }
    }

    fn tooltip_element(&self) -> Option<usize> {
        self.tree
            .find(TOOLTIP_ID)
            .filter(|&i| matches!(self.tree.elements[i].kind, Kind::Text(_)))
    }

    /// The layout under the pointer went away: no element is hovered and no tooltip is shown or
    /// due, until the pointer moves again.
    pub fn forget_pointer(&mut self) {
        self.hover_to(None);
        self.tooltip_for(None);
    }

    /// The pointer is over `target` (or nothing): when that is another element or row than
    /// before, the tooltip is hidden and, if the new one has help, due after the delay.
    pub(super) fn tooltip_for(&mut self, target: Option<usize>) {
        let key = target.map(|t| {
            let row = match &self.tree.elements[t].kind {
                Kind::ListBox(l) => l.rows.hover,
                _ => None,
            };
            (t, row)
        });
        if key == self.tip.key {
            return;
        }
        self.tip.key = key;
        self.hide_tip();
        let Some((t, row)) = key else { return };
        let e = &self.tree.elements[t];
        // A list gives the help of its hovered row, and none below the last row.
        let help = match (&e.kind, row) {
            (Kind::ListBox(l), r) => r.and_then(|r| l.rows.rows.get(r)).map_or(0, |r| r.help),
            _ => e.help_id.first().copied().unwrap_or(0),
        };
        let text = self.help_text(help);
        self.arm(text);
    }

    /// The pointer reached something that is not an element (a tile of the map): the tooltip shown
    /// or due goes away and, if string `help` has a text, it is due after the delay. The string
    /// is shown as it is: the game asks for the short text whatever `helpType` says.
    pub fn help_tip(&mut self, help: i32) {
        self.hide_tip();
        let text = match help {
            1.. => crate::text::to_cp1252(&self.strings.string(help)),
            _ => Vec::new(),
        };
        self.arm(text);
    }

    /// Hides the tooltip shown, and forgets the one due.
    fn hide_tip(&mut self) {
        self.tip.left = None;
        if let Some(i) = self.tooltip_element()
            && self.tree.elements[i].state & HIDDEN == 0
        {
            self.hide_element(i);
        }
    }

    /// Makes the tooltip with `text` due after the delay, when there is a text.
    fn arm(&mut self, text: Vec<u8>) {
        if !text.is_empty() {
            self.tip.left = Some(i64::from(self.settings.tooltip_delay));
            self.tip.text = text;
        }
    }

    /// The text of a help string number: the long help (the number plus 30000) when the settings
    /// ask for it, and when that string is empty, the short one (ui layout 7.7). Empty for 0 or
    /// less.
    fn help_text(&self, help: i32) -> Vec<u8> {
        if help <= 0 {
            return Vec::new();
        }
        let long = match self.settings.long_help {
            true => self.strings.string(help.saturating_add(LONG_HELP)),
            false => String::new(),
        };
        let text = if long.is_empty() {
            self.strings.string(help)
        } else {
            long
        };
        crate::text::to_cp1252(&text)
    }

    /// Runs the delay on an interface update `elapsed` milliseconds after the last one; at its
    /// end the tooltip is shown.
    pub(super) fn tooltip_update(&mut self, elapsed: u64) {
        let Some(left) = self.tip.left else { return };
        let left = left.saturating_sub(i64::try_from(elapsed).unwrap_or(i64::MAX));
        if left > 0 {
            self.tip.left = Some(left);
            return;
        }
        self.tip.left = None;
        self.show_tooltip();
    }

    /// Sizes the tooltip to its text, puts it at the pointer and shows it.
    fn show_tooltip(&mut self) {
        let Some(i) = self.tooltip_element() else {
            return;
        };
        let max = match self.settings.long_help {
            true => self.settings.max_long_width,
            false => self.settings.max_short_width,
        };
        let text = std::mem::take(&mut self.tip.text);
        self.put_text(i, &text);
        let (w, h) = {
            let Interface {
                tree,
                painter,
                strings,
                ..
            } = self;
            painter.tooltip_size(&tree.elements[i], &*strings, &text, max)
        };
        let Some(placed) = self.tree.elements[i].placed else {
            return;
        };
        self.tree.elements[i].placed = Some(crate::ui::resolve::Placed {
            dx: w,
            dy: h,
            ..placed
        });
        let window = self.tree.elements[0]
            .placed
            .map_or((0, 0), |p| (p.dx, p.dy));
        let at = place(self.input.pointer, (w, h), window);
        self.tree.set_screen_position(i, at);
        self.tree.bring_to_front(i);
        self.tree.elements[i].state &= !HIDDEN;
        self.tree.elements[i].countdown = None;
        self.event(format!("tooltip {} {} {w} {h}", at.0, at.1));
    }
}

/// Where a tooltip `size` goes for a pointer at `pointer` in a window: its left edge at the
/// pointer and its top 30 pixels lower; moved left to end at the window's right edge if it
/// would cross it, and above the pointer (its bottom at the pointer) if it would cross the
/// bottom.
pub fn place(
    (px, py): (i32, i32),
    (w, h): (i32, i32),
    (window_w, window_h): (i32, i32),
) -> (i32, i32) {
    let (px, py) = (i64::from(px), i64::from(py));
    let (w, h) = (i64::from(w), i64::from(h));
    let x = match px + w > i64::from(window_w) {
        true => i64::from(window_w) - w,
        false => px,
    };
    let y = match py + i64::from(OFFSET) + h > i64::from(window_h) {
        true => py - h,
        false => py + i64::from(OFFSET),
    };
    let clamp = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    (clamp(x), clamp(y))
}

#[cfg(test)]
mod tests;

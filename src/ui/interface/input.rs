//! The mouse reaching the widgets: hit testing, hover, the capture of the left button, buttons
//! of every kind, radio sets, the built-in actions of buttons, draggable layouts (ui layout 7).
//!
//! A message whose hit is the map view belongs to the map view: [`Interface::mouse`] says so.

use super::Interface;
use crate::app::Mouse;
use crate::ui::layout::{
    DISABLED, FOCUSED, HIDDEN, HIGHLIGHTED, Kind, NEVER_SHOWN, PRESSING, RELEASING, SELECTED,
    STICKY, TEMPLATE, TOGGLE,
};

/// The state flag of an element that hit testing never returns (the tooltip).
const NOT_HIT: u32 = 32;
/// The state flag of an element that takes the keyboard focus when clicked.
const TAKES_FOCUS: u32 = 32768;
/// The state flags of a list box that takes no mouse input.
const READ_ONLY: u32 = 262_144;
/// Milliseconds until a held repeating button fires again, and the first wait.
const REPEAT: u64 = 30;
const REPEAT_FIRST: u64 = 500;

/// The layout being dragged and where the pointer was when it last moved it.
struct Drag {
    layout: usize,
    last: (i32, i32),
}

#[derive(Default)]
pub(super) struct Input {
    pointer: (i32, i32),
    hover: Option<usize>,
    /// The element that gets every mouse message while the left button is held.
    capture: Option<usize>,
    drag: Option<Drag>,
    /// Milliseconds of interface updates so far.
    clock: u64,
    /// A repeating button held, and when it fires next.
    repeat: Option<(usize, u64)>,
}

impl Interface {
    /// Applies one mouse message. Returns whether it belongs to the map view.
    pub fn mouse(&mut self, mouse: Mouse) -> bool {
        match mouse {
            Mouse::Move(x, y) => self.moved((x, y)),
            Mouse::Down => self.down(),
            Mouse::Up => self.up(),
            Mouse::Wheel(_) => {
                let at = self.input.pointer;
                let target = self.input.capture.or_else(|| self.hit(at));
                target.is_some() && target == self.map_view
            }
        }
    }

    fn id(&self, i: usize) -> i32 {
        self.tree.elements[i].id
    }

    fn event(&mut self, line: String) {
        self.events.push(line);
        self.dirty = true;
    }

    fn flag(&mut self, i: usize, flag: u32, on: bool) {
        let state = &mut self.tree.elements[i].state;
        let was = *state;
        match on {
            true => *state |= flag,
            false => *state &= !flag,
        }
        if *state != was {
            self.dirty = true;
        }
    }

    // ---- Hit testing (7.1)

    /// The element at a point of the screen, aliases followed: the last drawn first.
    fn hit(&mut self, at: (i32, i32)) -> Option<usize> {
        let found = self.hit_in(0, at)?;
        let e = &self.tree.elements[found];
        match e.alias {
            alias if alias > 0 && alias != e.id => self.tree.find(alias).or(Some(found)),
            _ => Some(found),
        }
    }

    fn hit_in(&mut self, layout: usize, at: (i32, i32)) -> Option<usize> {
        let e = self.tree.elements.get(layout)?;
        if e.state & (HIDDEN | NEVER_SHOWN) != 0 || !inside(self.tree.rect(layout), at) {
            return None;
        }
        for child in e.children.clone().into_iter().rev() {
            let hit = match self.tree.elements[child].kind {
                Kind::Layout => self.hit_in(child, at),
                _ => self.widget_hit(child, at).then_some(child),
            };
            if hit.is_some() {
                return hit;
            }
        }
        None
    }

    /// Whether the widget is hit at the point: not hidden, never shown, a template or not to be
    /// hit, inside its rectangle, and on an opaque pixel if it is transparent.
    fn widget_hit(&mut self, i: usize, at: (i32, i32)) -> bool {
        let e = &self.tree.elements[i];
        if e.state & (HIDDEN | NEVER_SHOWN | NOT_HIT | TEMPLATE) != 0
            || !inside(self.tree.rect(i), at)
        {
            return false;
        }
        if !e.transparent || e.look.color_as_icon {
            return true;
        }
        let text_button = matches!(e.kind, Kind::Button(_)) && !e.look.text_id.is_empty();
        match self.painter.opaque_at(&self.tree, i, at) {
            Some(opaque) => opaque,
            // A button that is only its text is hit over its rectangle.
            None => text_button,
        }
    }

    // ---- Messages (7.2, 7.3)

    fn moved(&mut self, at: (i32, i32)) -> bool {
        self.input.pointer = at;
        if let Some(drag) = &self.input.drag {
            let layout = drag.layout;
            self.drag_to(layout, at);
            return false;
        }
        if let Some(held) = self.input.capture {
            if self.is_button(held) {
                self.button_moved(held, at);
            }
            return Some(held) == self.map_view;
        }
        let target = self.hit(at);
        let shown = target.filter(|t| Some(*t) != self.map_view);
        self.hover_to(shown);
        target.is_some() && target == self.map_view
    }

    fn hover_to(&mut self, target: Option<usize>) {
        if target == self.input.hover {
            return;
        }
        if let Some(old) = self.input.hover {
            self.flag(old, HIGHLIGHTED, false);
        }
        if let Some(new) = target {
            self.flag(new, HIGHLIGHTED, true);
        }
        self.input.hover = target;
        let id = target.map_or("none".to_string(), |t| self.id(t).to_string());
        self.event(format!("hover {id}"));
    }

    fn down(&mut self) -> bool {
        let at = self.input.pointer;
        if self.input.capture.is_some() {
            return false;
        }
        // A press that hits nothing is dropped and keeps the keyboard focus.
        let Some(t) = self.hit(at) else {
            return false;
        };
        let state = self.tree.elements[t].state;
        // One that reaches a control gives the focus to a field that takes it and takes it from
        // any other.
        for i in 0..self.tree.elements.len() {
            self.flag(i, FOCUSED, false);
        }
        if state & TAKES_FOCUS != 0 {
            self.flag(t, FOCUSED, true);
        }
        if matches!(self.tree.elements[t].kind, Kind::Layout) {
            // A click on the picture that stands for a layout drags the layout.
            self.tree.bring_to_front(t);
            self.input.drag = Some(Drag {
                layout: t,
                last: at,
            });
            self.input.capture = Some(t);
            let id = self.id(t);
            self.event(format!("press {id}"));
        } else if self.is_button(t) {
            if state & (DISABLED | READ_ONLY) == 0 {
                self.press(t);
            }
        } else {
            self.input.capture = Some(t);
        }
        Some(t) == self.map_view
    }

    fn up(&mut self) -> bool {
        let at = self.input.pointer;
        let held = self.input.capture.take();
        self.input.drag = None;
        if let Some(b) = held.filter(|&b| self.is_button(b)) {
            self.release(b, at);
        }
        if let Some(h) = held {
            let id = self.id(h);
            if matches!(self.tree.elements[h].kind, Kind::Layout) {
                self.event(format!("release {id}"));
            }
        }
        // With no capture, the element under the pointer is the hover element.
        let target = self.hit(at).filter(|t| Some(*t) != self.map_view);
        self.hover_to(target);
        held.is_some() && held == self.map_view
    }

    // ---- Buttons (7.4)

    fn is_button(&self, i: usize) -> bool {
        matches!(self.tree.elements[i].kind, Kind::Button(_))
    }

    fn repeating(&self, b: usize) -> bool {
        matches!(&self.tree.elements[b].kind, Kind::Button(button) if button.repeating)
    }

    fn press(&mut self, b: usize) {
        self.sounds.push("uiclick");
        self.event("sound uiclick".to_string());
        self.input.capture = Some(b);
        let id = self.id(b);
        self.event(format!("press {id}"));
        let state = self.tree.elements[b].state;
        let selected = state & SELECTED != 0;
        if state & TOGGLE != 0 {
            self.flag(b, if selected { RELEASING } else { PRESSING }, true);
        } else if state & STICKY != 0 {
            if !selected {
                self.flag(b, PRESSING, true);
            }
        } else {
            self.flag(b, SELECTED, true);
            if self.repeating(b) {
                self.select(b);
                self.input.repeat = Some((b, self.input.clock + REPEAT_FIRST));
            }
        }
    }

    /// The pointer moves with the left button held on button `b`.
    fn button_moved(&mut self, b: usize, at: (i32, i32)) {
        // Only while it is the hover element... the hover does not change while held, so the
        // button is the one the press chose.
        let over = self.widget_hit(b, at);
        let state = self.tree.elements[b].state;
        let selected = state & SELECTED != 0;
        if state & TOGGLE != 0 {
            self.flag(b, RELEASING, over && selected);
            self.flag(b, PRESSING, over && !selected);
        } else if state & STICKY != 0 {
            if !selected {
                self.flag(b, PRESSING, over);
            }
        } else {
            self.flag(b, SELECTED, over);
        }
    }

    fn release(&mut self, b: usize, at: (i32, i32)) {
        let inside = self.widget_hit(b, at);
        let id = self.id(b);
        self.event(format!(
            "release {id} {}",
            if inside { "inside" } else { "outside" }
        ));
        self.flag(b, PRESSING | RELEASING, false);
        let state = self.tree.elements[b].state;
        if state & TOGGLE != 0 {
            if inside {
                match state & SELECTED != 0 {
                    true => {
                        self.flag(b, SELECTED, false);
                        self.radio_deselect_all(b);
                        self.deselect(b);
                    }
                    false => {
                        self.flag(b, SELECTED, true);
                        self.radio_select(b);
                        self.select(b);
                    }
                }
            }
        } else if state & STICKY != 0 {
            if inside {
                self.flag(b, SELECTED, true);
                self.radio_select(b);
                self.select(b);
            }
        } else {
            self.flag(b, SELECTED, false);
            let repeating = self.repeating(b);
            if repeating {
                self.input.repeat = None;
            }
            if inside && !repeating {
                self.radio_select(b);
                self.select(b);
            }
        }
    }

    /// The other members of the radio sets that `b` belongs to.
    fn radio_members(&self, b: usize) -> Vec<usize> {
        let id = self.id(b);
        let mut members = Vec::new();
        for e in &self.tree.elements {
            if let Kind::RadioSet(buttons) = &e.kind
                && buttons.contains(&id)
            {
                members.extend(
                    buttons
                        .iter()
                        .filter_map(|&m| self.tree.find(m))
                        .filter(|&m| self.is_button(m)),
                );
            }
        }
        members
    }

    /// A member released as selected: every other member is deselected, its deselect action run
    /// even if it was not selected.
    fn radio_select(&mut self, b: usize) {
        for m in self.radio_members(b).into_iter().filter(|&m| m != b) {
            self.flag(m, SELECTED, false);
            self.deselect(m);
        }
    }

    /// A toggle member released as deselected: every member is deselected that way.
    fn radio_deselect_all(&mut self, b: usize) {
        for m in self.radio_members(b) {
            self.flag(m, SELECTED, false);
            self.deselect(m);
        }
    }

    /// A repeating button held fires every 30 ms after 500 ms while the pointer is on it.
    pub(super) fn repeat(&mut self, period: u32) {
        self.input.clock += u64::from(period);
        let Some((b, due)) = self.input.repeat else {
            return;
        };
        let state = self.tree.elements[b].state;
        if state & (HIDDEN | DISABLED) != 0 {
            self.input.repeat = None;
            self.input.capture = None;
            self.flag(b, SELECTED, false);
            return;
        }
        if self.input.clock < due {
            return;
        }
        match self.widget_hit(b, self.input.pointer) {
            true => {
                self.select(b);
                self.input.repeat = Some((b, self.input.clock + REPEAT));
            }
            false => self.input.repeat = Some((b, self.input.clock + 1)),
        }
    }

    // ---- Actions (7.5)

    fn select(&mut self, b: usize) {
        let id = self.id(b);
        self.selected.push(id);
        self.event(format!("select {id}"));
        let e = &self.tree.elements[b];
        let (action, target, layout) = (e.action, e.target, e.parent);
        match action {
            1 | 3 => self.show_target(target),
            2 => self.hide_target(target),
            4 => {
                if let Some(layout) = layout {
                    self.hide_element(layout);
                }
                self.show_target(target);
            }
            _ => {}
        }
    }

    fn deselect(&mut self, b: usize) {
        let id = self.id(b);
        self.event(format!("deselect {id}"));
        let e = &self.tree.elements[b];
        if e.action == 3 {
            let target = e.target;
            self.hide_target(target);
        }
    }

    /// Shows the target if it is hidden; a draggable layout also comes to the front.
    fn show_target(&mut self, id: i32) {
        let Some(i) = self.tree.find(id) else { return };
        if self.tree.elements[i].state & HIDDEN == 0 {
            return;
        }
        match self.tree.elements[i].kind {
            Kind::Layout => {
                self.tree.show(id);
                if self.draggable(i) {
                    self.tree.bring_to_front(i);
                }
            }
            _ => self.tree.elements[i].state &= !HIDDEN,
        }
        self.event(format!("show {id}"));
    }

    fn hide_target(&mut self, id: i32) {
        if let Some(i) = self.tree.find(id) {
            self.hide_element(i);
        }
    }

    fn hide_element(&mut self, i: usize) {
        self.tree.hide(i);
        let id = self.id(i);
        self.event(format!("hide {id}"));
    }

    /// A layout with an element that aliases it can be dragged.
    fn draggable(&self, layout: usize) -> bool {
        let id = self.id(layout);
        self.tree.elements[layout]
            .children
            .iter()
            .any(|&c| self.tree.elements[c].alias == id)
    }

    // ---- Dragging a layout (7.6)

    /// Moves the layout by what the pointer moved since it last did, kept inside the window; when
    /// that is cut, the remembered pointer moves only as far as the layout did.
    fn drag_to(&mut self, layout: usize, at: (i32, i32)) {
        let Some(drag) = &mut self.input.drag else {
            return;
        };
        let (window, placed) = (
            self.tree.elements[0].placed,
            self.tree.elements[layout].placed,
        );
        let (Some(window), Some(placed)) = (window, placed) else {
            return;
        };
        let (sx, sy) = self.tree.screen(layout);
        let wanted = (sx + (at.0 - drag.last.0), sy + (at.1 - drag.last.1));
        let clamp = |v: i32, size: i32, window: i32| v.clamp(0, (window - size).max(0));
        let to = (
            clamp(wanted.0, placed.dx, window.dx),
            clamp(wanted.1, placed.dy, window.dy),
        );
        drag.last = (drag.last.0 + (to.0 - sx), drag.last.1 + (to.1 - sy));
        if to != (sx, sy) {
            self.tree.set_screen_position(layout, to);
            self.dirty = true;
            let id = self.id(layout);
            self.event(format!("drag {id} {} {}", to.0, to.1));
        }
    }
}

fn inside((x, y, w, h): (i32, i32, i32, i32), (px, py): (i32, i32)) -> bool {
    let (px, py) = (i64::from(px), i64::from(py));
    (i64::from(x)..i64::from(x) + i64::from(w)).contains(&px)
        && (i64::from(y)..i64::from(y) + i64::from(h)).contains(&py)
}

#[cfg(test)]
mod tests;

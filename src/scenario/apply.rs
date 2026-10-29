//! The interface side of the scenario: what the application does with an [`Effect`], and the
//! clean-up (scenarios 7.1, 8.1, 8.3, 8.4).

use super::{Effect, Line, Op, Popup};
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
use crate::ui::interface::{Buttons, Interface};
use crate::vfs::Vfs;

/// The built text of a goal line (4.2, 4.5): the template of its string filled in with `value`,
/// `arga` and `argb`, where a `%r` takes the string of the value in its place (the name of `arga`).
pub fn built_line(ui: &Interface, line: Line) -> String {
    /// The strings of the values' ids, the only ones a `%r` of the line can name.
    struct Names(Vec<(u32, String)>);
    impl format::Strings for Names {
        fn string(&self, id: u32) -> &str {
            self.0.iter().find(|(n, _)| *n == id).map_or("", |(_, s)| s)
        }
    }
    let names = Names(
        line.values
            .iter()
            .filter_map(|&v| Some((u32::try_from(v).ok()?, ui.string(v))))
            .collect(),
    );
    let args = line.values.map(|v| Arg::Int(i64::from(v)));
    format::expand(&names, &ui.template(line.text), &args)
}

/// Where the dialog layouts keep their pictures and texts, from the layout's id.
const BACKGROUND: i32 = 1;
const ICON: i32 = 2;
const IMAGE1: i32 = 4;
const IMAGE2: i32 = 5;
const TEXT2: i32 = 8;

/// What the triggers changed in the interface, so that it can be given back (8.4).
#[derive(Debug, Default)]
pub struct Marks {
    disabled: Vec<i32>,
    hidden: Vec<i32>,
    /// The last popup, which the hint's button shows again, and the layout of the hint on screen.
    last: Option<Popup>,
    hint: Option<i32>,
}

impl Marks {
    /// Does what a trigger asked of the interface. Each change is told in `log`.
    pub fn apply(&mut self, effect: Effect, ui: &mut Interface, vfs: &Vfs, log: &mut Vec<String>) {
        let (popup, show) = match effect {
            Effect::Popup(p) => (Some(p), true),
            Effect::Press(id, popup) => {
                if ui.press_button(id) {
                    log.push(format!("scenario element {id} press"));
                }
                (popup, true)
            }
            Effect::Set { ops, popup, always } => {
                let mut changed = false;
                for (id, op) in ops {
                    let did = match op {
                        Op::Disable | Op::Enable => {
                            let off = op == Op::Disable;
                            let did = ui.disable(id, off);
                            self.disabled.retain(|&d| d != id);
                            if did && off {
                                self.disabled.push(id);
                            }
                            did
                        }
                        Op::Hide | Op::Show => {
                            let hide = op == Op::Hide;
                            let did = ui.conceal(id, hide);
                            self.hidden.retain(|&d| d != id);
                            if did && hide {
                                self.hidden.push(id);
                            }
                            did
                        }
                    };
                    if did {
                        log.push(format!("scenario element {id} {op:?}").to_lowercase());
                    }
                    changed |= did;
                }
                (popup, changed || always)
            }
        };
        if let Some(popup) = popup.filter(|_| show) {
            self.show(popup, ui, vfs, log);
        }
    }

    /// The hint's button was pressed: the last popup shows again.
    pub fn again(&mut self, id: i32, ui: &mut Interface, vfs: &Vfs, log: &mut Vec<String>) {
        if let Some(last) = self.last.clone().filter(|p| p.hint.button == id) {
            self.show(last, ui, vfs, log);
        }
    }

    /// Shows a popup over the hint, which goes; a popup of a layout the interface lacks, or of none,
    /// shows nothing. The hint, a line with the text it is given, stays when there is one.
    fn show(&mut self, p: Popup, ui: &mut Interface, vfs: &Vfs, log: &mut Vec<String>) {
        if let Some(hint) = self.hint.take() {
            ui.conceal(hint, true);
        }
        let text = |ui: &Interface, long: &str, short: i32| match (long, short) {
            ("", 1..) => to_cp1252(&ui.string(short)),
            ("", _) => Vec::new(),
            _ => ui.text_entry(long.as_bytes()),
        };
        let texts = [0, 1].map(|n| text(ui, &p.long[n], p.short[n]));
        let buttons = Buttons {
            ok: p.ok,
            cancel: p.cancel,
            ..Buttons::default()
        };
        let layout = p.layout;
        let opened = match p.modal {
            true => ui.ask(layout, &texts[0], buttons),
            false => ui.notify(layout, &texts[0], buttons),
        };
        if !opened {
            return;
        }
        log.push(format!(
            "scenario popup {layout} {}",
            if p.modal { "modal" } else { "open" }
        ));
        ui.set_text(layout + TEXT2, &texts[1]);
        // The pictures come after the layout is shown, which would show them again. The
        // background of the layout stays when the popup names none.
        for (at, name) in [BACKGROUND, ICON, IMAGE1, IMAGE2]
            .into_iter()
            .zip(&p.pictures)
        {
            let picture = (!name.is_empty()).then(|| ui.tree.picture(vfs, name));
            match (at, picture.flatten()) {
                (BACKGROUND, None) => {}
                (at, picture) => {
                    ui.set_picture(layout + at, picture);
                }
            }
        }
        let line = match p.hint.short {
            -1 if p.hint.long.is_empty() => p
                .hint
                .goal
                .map_or(Vec::new(), |g| to_cp1252(&built_line(ui, g))),
            _ => text(ui, &p.hint.long, p.hint.short),
        };
        if p.hint.layout > 0 && !line.is_empty() && ui.set_text(p.hint.button, &line) {
            ui.show(p.hint.layout);
            self.hint = Some(p.hint.layout);
            log.push(format!("scenario hint {}", p.hint.layout));
        }
        self.last = Some(p);
    }

    /// Gives back what the triggers changed and hides the hint (8.4).
    pub fn clean(&mut self, ui: &mut Interface) {
        for id in self.disabled.drain(..) {
            ui.disable(id, false);
        }
        for id in self.hidden.drain(..) {
            ui.conceal(id, false);
        }
        if let Some(hint) = self.hint.take() {
            ui.conceal(hint, true);
        }
        self.last = None;
    }
}

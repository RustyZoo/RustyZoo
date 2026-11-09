//! The interface side of the scenario: what the application does with an [`Effect`], and the
//! clean-up (scenarios 7.1, 8.1, 8.3, 8.4).

use super::{Effect, Line, Op, Popup};
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
use crate::ui::interface::{Buttons, Interface};
use crate::vfs::Vfs;

/// The built text of a goal line (4.2, 4.5): the template of its string filled in with `value`,
/// `arga` and `argb`, where a `%r` takes the string of the value in its place (the name of `arga`).
pub(crate) fn built_line(ui: &Interface, line: Line) -> String {
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

/// The text of a popup element: the named text when there is one, else the plain string of the id
/// (none for 0 or less).
fn text(ui: &Interface, long: &str, short: i32) -> Vec<u8> {
    match (long, short) {
        ("", 1..) => to_cp1252(&ui.string(short)),
        ("", _) => Vec::new(),
        _ => ui.text_entry(long.as_bytes()),
    }
}

/// Where the dialog layouts keep their pictures and texts, from the layout's id.
const ICON: i32 = 2;
const IMAGE1: i32 = 4;
const IMAGE2: i32 = 5;
const TEXT2: i32 = 8;
/// Where the hint panel keeps its line and its second button, from the hint layout's id.
const LINE: i32 = 6;
const SECOND_BUTTON: i32 = 12;

/// What the triggers changed in the interface, so that it can be given back (8.4).
#[derive(Debug, Default)]
pub(crate) struct Marks {
    disabled: Vec<i32>,
    hidden: Vec<i32>,
    /// The popup the hint's line shows again, and the layout of the hint on screen.
    last: Option<Popup>,
    hint: Option<i32>,
    /// A modal popup whose hint is decided once it is answered (8.3 point 1).
    waiting: Option<Popup>,
}

impl Marks {
    /// Does what a trigger asked of the interface. Each change is told in `log`.
    pub(crate) fn apply(
        &mut self,
        effect: Effect,
        ui: &mut Interface,
        vfs: &Vfs,
        log: &mut Vec<String>,
    ) {
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
            self.last = None;
            self.show(popup, ui, vfs, log);
        }
    }

    /// The hint's line was pressed: the remembered popup shows again, with the hint line `line` of
    /// the goals as they are now (8.3 point 5); nothing is forgotten.
    pub(crate) fn again(
        &mut self,
        id: i32,
        line: Option<Line>,
        ui: &mut Interface,
        vfs: &Vfs,
        log: &mut Vec<String>,
    ) {
        if let Some(mut last) = self.last.clone().filter(|p| p.hint.layout + LINE == id) {
            if last.hint.short == -1 {
                last.hint.goal = line;
            }
            self.show(last, ui, vfs, log);
        }
    }

    /// The hint of a modal popup that was answered is decided now (8.3 point 1). Call it while no
    /// question is open.
    pub(crate) fn settle(&mut self, ui: &mut Interface, log: &mut Vec<String>) {
        if let Some(popup) = self.waiting.take() {
            self.hint_of(popup, ui, log);
        }
    }

    /// Whether a modal popup still waits for its answer to decide its hint.
    pub(crate) fn waiting(&self) -> bool {
        self.waiting.is_some()
    }

    /// Shows a popup over the hint, which goes; a popup of a layout the interface lacks, or of none,
    /// shows nothing. The hint, a line with the text it is given, comes after the dialog call
    /// returns: for a modal popup when it is answered.
    fn show(&mut self, p: Popup, ui: &mut Interface, vfs: &Vfs, log: &mut Vec<String>) {
        self.waiting = None;
        if let Some(hint) = self.hint.take() {
            ui.conceal(hint, true);
        }
        let texts = [0, 1].map(|n| text(ui, &p.long[n], p.short[n]));
        let buttons = Buttons {
            ok: p.ok,
            cancel: p.cancel,
            ..Buttons::default()
        };
        let layout = p.layout;
        // The layout takes the size of its background before it is shown (ui layout 7.12).
        let background = Some(&p.pictures[0]).filter(|n| !n.is_empty());
        let background = background.and_then(|name| ui.tree.picture(vfs, name));
        ui.fit_to_background(layout, background);
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
        // The pictures come after the layout is shown, which would show them again. The routine
        // puts the first image on both image elements and never reads the second (original bug,
        // engine issue 1458).
        let image = &p.pictures[2];
        for (at, name) in [(ICON, &p.pictures[1]), (IMAGE1, image), (IMAGE2, image)] {
            let picture = (!name.is_empty()).then(|| ui.tree.picture(vfs, name));
            ui.set_picture(layout + at, picture.flatten());
        }
        match p.modal {
            true => self.waiting = Some(p),
            false => self.hint_of(p, ui, log),
        }
    }

    /// The hint panel of a popup that was shown: its line, in the element `layout + 6`, and the
    /// popup is remembered if none is (8.3).
    fn hint_of(&mut self, p: Popup, ui: &mut Interface, log: &mut Vec<String>) {
        let line = match p.hint.short {
            -1 if p.hint.long.is_empty() => p
                .hint
                .goal
                .map_or(Vec::new(), |g| to_cp1252(&built_line(ui, g))),
            _ => text(ui, &p.hint.long, p.hint.short),
        };
        let layout = p.hint.layout;
        if layout > 0 && !line.is_empty() && ui.set_text(layout + LINE, &line) {
            ui.conceal(layout + SECOND_BUTTON, !p.hint.button);
            ui.show(layout);
            self.hint = Some(layout);
            log.push(format!("scenario hint {layout}"));
            if self.last.is_none() {
                self.last = Some(p);
            }
        }
    }

    /// Gives back what the triggers changed and hides the hint (8.4). An element a trigger hid
    /// stays hidden: only its mark goes, so the next show of its layout brings it back.
    pub(crate) fn clean(&mut self, ui: &mut Interface) {
        for id in self.disabled.drain(..) {
            ui.disable(id, false);
        }
        self.hidden.clear();
        if let Some(hint) = self.hint.take() {
            ui.conceal(hint, true);
        }
        (self.last, self.waiting) = (None, None);
    }
}

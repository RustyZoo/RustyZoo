//! The terrain panel (terrain editing 3): the layout the two terrain tabs of the construction
//! panel show over it, its two modes, the brush buttons and picture, the list of terrain types,
//! the cost and its colour, accept and cancel. It only shows what [`Tool`] holds and says which
//! element was pressed; the tool and the world are the game's.

use crate::money::{Money, SYMBOL, Symbol, Tone};
use crate::terrain::{MAX_SIZE, MIN_SIZE, Mode, Tool};
use crate::text::format::whole_units;
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::layout::{HIDDEN, SELECTED};
use crate::vfs::Vfs;

/// The panel's layout, and the two tabs of the construction panel that show it.
pub const PANEL: i32 = 7;
const TABS: [(i32, Mode); 2] = [(3361, Mode::Modify), (3362, Mode::Paint)];
const ACCEPT: i32 = 3302;
const CANCEL: i32 = 3303;
const COST_LABEL: i32 = 3304;
const COST: i32 = 3305;
const SUBTITLE: i32 = 3306;
/// The brush size buttons, which the keys `+` and `-` press.
pub const PLUS: i32 = 3310;
pub const MINUS: i32 = 3314;
/// The four mode buttons of the modify tab, the text of the mode and the brush picture.
const MODES: [i32; 4] = [3315, 3316, 3317, 3318];
const MODE_TEXT: i32 = 3319;
const BRUSH: i32 = 3320;
const BRUSH_COST: i32 = 3321;
const LIST: i32 = 3350;

/// What the player pressed in the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Plus,
    Minus,
    Accept,
    Cancel,
    /// A mode button of the modify tab.
    Mode(i32),
    /// Item `n` of the list of terrain types.
    Type(usize),
}

/// What the button `id` is, if it belongs to the panel.
pub fn pressed(ui: &Interface, id: i32) -> Option<Press> {
    Some(match id {
        PLUS => Press::Plus,
        MINUS => Press::Minus,
        ACCEPT => Press::Accept,
        CANCEL => Press::Cancel,
        id if MODES.contains(&id) => Press::Mode(id),
        id => Press::Type(ui.item_of(LIST, id)?),
    })
}

/// Shows or hides the panel for the tab of the construction panel that is selected, and tells
/// which mode that is: none when the tab is one of the item lists (`listed`).
pub fn tab(ui: &mut Interface, listed: bool) -> Option<Mode> {
    let selected = |ui: &Interface, id: i32| {
        ui.tree
            .find(id)
            .is_some_and(|i| ui.tree.elements[i].state & SELECTED != 0)
    };
    let mode = TABS
        .iter()
        .find(|&&(id, _)| !listed && selected(ui, id))
        .map(|&(_, mode)| mode);
    ui.set_hidden(PANEL, mode.is_none());
    mode
}

/// Hides the panel and releases its tabs.
pub fn hide(ui: &mut Interface) {
    ui.set_hidden(PANEL, true);
    for (id, _) in TABS {
        ui.set_button(id, false, true);
    }
}

/// The panel opens in `mode`: the types list is filled and the paint type chosen, or the modes
/// are shown with the first one selected (terrain editing 3.1).
pub fn show(ui: &mut Interface, vfs: &Vfs, tool: &Tool, mode: Mode) {
    let paint = mode == Mode::Paint;
    let (subtitle, _) = TABS
        .iter()
        .find(|t| t.1 == mode)
        .copied()
        .unwrap_or(TABS[0]);
    ui.set_text(SUBTITLE, &to_cp1252(&ui.string(subtitle)));
    for id in [COST_LABEL, BRUSH_COST] {
        ui.set_hidden(id, !paint);
    }
    for id in MODES {
        ui.set_hidden(id, paint);
    }
    if paint {
        let listed = tool.listed();
        let icons: Vec<_> = listed
            .iter()
            .map(|t| {
                (!t.icon.is_empty())
                    .then(|| ui.tree.picture(vfs, &t.icon))
                    .flatten()
            })
            .collect();
        ui.set_items(LIST, &icons);
        let at = listed.iter().position(|t| t.kind == tool.kind());
        ui.select_item(LIST, at);
    } else {
        ui.set_items(LIST, &[]);
        for (n, id) in MODES.into_iter().enumerate() {
            ui.set_button(id, n == 0, true);
        }
        ui.set_text(MODE_TEXT, &to_cp1252(&ui.string(MODES[0])));
    }
}

/// A mode button was chosen: its text shows.
pub fn mode_text(ui: &mut Interface, id: i32) {
    ui.set_text(MODE_TEXT, &to_cp1252(&ui.string(id)));
}

/// Writes what the tool and the cash say: the size buttons and picture, the cost with its colour
/// and the accept and cancel buttons, in paint mode the name of the type and the cost of the brush.
pub fn refresh(ui: &mut Interface, tool: &Tool, cost: f32, money: &Money) {
    let Some(mode) = tool.mode() else { return };
    let size = tool.size();
    ui.set_button(PLUS, false, size < MAX_SIZE);
    ui.set_button(MINUS, false, size > MIN_SIZE);
    ui.set_image(BRUSH, size - 1);
    let symbol = Symbol::new(ui.string(SYMBOL));
    let amount = |value: f32| to_cp1252(&symbol.text(whole_units(f64::from(value))));
    let (tone, pay) = match cost {
        c if c <= 0.0 => (Tone::Yellow, false),
        c if money.can_pay(c) => (Tone::Green, true),
        _ => (Tone::Red, false),
    };
    ui.set_text(COST, &amount(cost));
    ui.set_fore_color(COST, tone.rgb());
    ui.set_button(ACCEPT, false, pay);
    ui.set_button(CANCEL, false, cost > 0.0);
    if mode == Mode::Paint {
        let name = tool.find(tool.kind()).map_or(0, |t| t.help);
        ui.set_text(MODE_TEXT, &to_cp1252(&ui.string(name)));
        ui.set_text(BRUSH_COST, &amount(tool.brush_cost()));
    }
}

/// Whether the panel is shown.
pub fn shown(ui: &Interface) -> bool {
    ui.tree
        .find(PANEL)
        .is_some_and(|i| ui.tree.elements[i].state & HIDDEN == 0)
}

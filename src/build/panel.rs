//! The four panels of the building tools and the layout elements each one owns.

use super::*;

/// One panel: its layout, the left bar button that shows it, its elements, the tabs that list
/// items, and the other tabs that the terrain tools own.
pub(super) struct Panel {
    pub(super) name: &'static str,
    pub(super) layout: i32,
    pub(super) bar: i32,
    pub(super) title: i32,
    pub(super) expansion: i32,
    pub(super) list: i32,
    pub(super) rotate: [i32; 2],
    pub(super) name_text: i32,
    pub(super) cost_text: i32,
    pub(super) tabs: &'static [i32],
    pub(super) other_tabs: &'static [i32],
    /// Elements that showing the layout brings back and that this milestone leaves hidden: the
    /// terrain panel inside it.
    pub(super) hidden: &'static [i32],
    /// The location picture, the habitat picture and its border; none on a panel without them
    /// (placing 3.5).
    pub(super) pictures: [i32; 3],
    /// The era text, the fence descriptor's label and value: shown for the chosen item when it has
    /// them (placing 3.5).
    pub(super) details: [i32; 3],
    /// The gate button and the tab (a position in `tabs`) that shows it.
    pub(super) gate: Option<(i32, usize)>,
    /// The sex buttons (male, female) and the "no items to display" text; none, 0 without them.
    pub(super) sexes: &'static [i32],
    pub(super) empty: i32,
}

pub(super) const PANELS: [Panel; PANEL_COUNT] = [
    Panel {
        name: "construction",
        layout: CONSTRUCTION,
        bar: 1002,
        title: 3261,
        expansion: 3204,
        list: 3208,
        rotate: [3258, 3259],
        name_text: 3263,
        cost_text: 3207,
        tabs: &[3251, 3056, 3252, 3256],
        other_tabs: &[3362, 3361],
        hidden: &[7],
        pictures: [3283, 3281, 3280],
        details: [3284, 3296, 3299],
        gate: Some((3240, 0)),
        sexes: &[],
        empty: 0,
    },
    Panel {
        name: "objects",
        layout: OBJECTS,
        bar: 1001,
        title: 3061,
        expansion: 3005,
        list: 3019,
        rotate: [3054, 3055],
        name_text: 3058,
        cost_text: 3016,
        tabs: &[3050, 3053],
        other_tabs: &[],
        hidden: &[],
        pictures: [0; 3],
        details: [0; 3],
        gate: None,
        sexes: &[],
        empty: 0,
    },
    Panel {
        name: "adopt",
        layout: ADOPT,
        bar: 1000,
        title: 2023,
        expansion: 2005,
        list: 2019,
        rotate: [2054, 2055],
        name_text: 2043,
        cost_text: 2014,
        tabs: &[2075, 3255, 3254, 2076],
        other_tabs: &[],
        // The Young button, which only a developer key shows (unit placing 3.3 item 7), and the
        // shelter capacity, which a rebuild of the list hides (item 6).
        hidden: &[2051, 2090, 2091],
        pictures: [2045, 2047, 2046],
        details: [2082, 0, 0],
        gate: None,
        sexes: &[2000, 2001],
        empty: 2009,
    },
    Panel {
        name: "hire",
        layout: HIRE,
        bar: 1005,
        title: HIRE_TITLE,
        expansion: 3601,
        list: 3602,
        rotate: [3676, 3677],
        name_text: 3620,
        cost_text: 3611,
        tabs: &[],
        other_tabs: &[],
        hidden: &[],
        pictures: [0; 3],
        details: [0; 3],
        gate: None,
        sexes: &[],
        empty: 3609,
    },
];

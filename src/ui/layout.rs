//! Reading the interface tree from the layout files (`.lyt`).
//!
//! A layout file is a file of the dialect of [`crate::config`]. `[LayoutInfo]` describes the
//! layout itself, every other section is one element made from its `type`. The sections are read
//! in ascending byte order, which is the order of the children, not the order of the file. A
//! section of type `UILayout` names another layout file whose `[LayoutInfo]` gives the keys of the
//! element; the keys of the `UILayout` section itself are ignored but for `layout` and
//! `expansion`.
//!
//! Positions and sizes are kept as the file wrote them (numbers or words); resolving them is
//! done elsewhere. A section without `type`, or of an unknown type, and a `UILayout`
//! section without `layout` stop the load: the layouts that named the file stop too, and a failure
//! that reaches the root drops the whole interface (no elements), with [`Tree::failure`] saying
//! where it stopped. A layout file that cannot be read is an empty layout, with a warning.
//!
//! Pictures (animations and TGA images) are loaded once through the cache of the tree. One that
//! cannot be found or decoded leaves its control without picture and is a warning.

mod load;

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

use crate::anim::{AnimSet, Animation};
use crate::config::{Config, Section, to_flag, to_int};
use crate::tga::Texture;
use crate::vfs::{Vfs, normalise};

use super::resolve::Placed;
use super::widgets::{Bar, Caret, Rows};

/// State flag: not drawn, not hit. Every element starts with it.
pub(crate) const HIDDEN: u32 = 1;
/// State flag: a button is drawn grayed and gets no mouse input.
pub(crate) const DISABLED: u32 = 2;
/// State flag: the pointer is over the element.
pub const HIGHLIGHTED: u32 = 4;
/// State flag: a button is drawn pressed.
pub const SELECTED: u32 = 8;
/// State flag: not shown when its layout is shown.
pub(crate) const NEVER_SHOWN: u32 = 16;
/// State flag: hit testing never returns the element (the tooltip).
pub(crate) const NOT_HIT: u32 = 32;
/// State flag: the canvas must be drawn again.
pub const DIRTY: u32 = 128;
/// State flag: never drawn nor hit.
pub(crate) const TEMPLATE: u32 = 256;
/// State flag: the element has the keyboard focus.
pub(crate) const FOCUSED: u32 = 512;
/// State flag: set by the `timed` key.
pub(crate) const TIMED: u32 = 1024;
/// State flags: a toggle button, a sticky button.
pub(crate) const TOGGLE: u32 = 2048;
pub(crate) const STICKY: u32 = 4096;
/// State flag: the element gets the keyboard focus when clicked.
pub(crate) const TAKES_FOCUS: u32 = 32768;
/// State flag of a list box: the keys that move the selection also run its select action.
pub(crate) const SELECT_ON_KEYS: u32 = 131_072;
/// State flag of a list box: no mouse input.
pub(crate) const READ_ONLY: u32 = 262_144;
/// State flag of a selected toggle button held with the pointer over it: drawn unselected.
pub(crate) const RELEASING: u32 = 8192;
/// State flag of an unselected toggle or sticky button held with the pointer over it: drawn
/// pressed.
pub(crate) const PRESSING: u32 = 16384;

/// The deepest chain of layouts naming layouts. The shipped files go 4 deep.
const MAX_DEPTH: usize = 16;
/// The most elements one tree holds. The full game has 1,138.
const MAX_ELEMENTS: usize = 50_000;

/// A position as the file wrote it. The words are resolved against the reference rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pos {
    Num(i32),
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

/// A size as the file wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dim {
    Num(i32),
    Whole,
    Half,
    Third,
    Quarter,
    FitFont,
}

impl Pos {
    /// The number the original keeps for a word, and for a number the number itself.
    pub(crate) fn code(self) -> i64 {
        match self {
            Pos::Num(n) => i64::from(n),
            Pos::Left => -30001,
            Pos::Right => -30002,
            Pos::Top => -30003,
            Pos::Bottom => -30004,
            Pos::Center => -30005,
        }
    }

    /// The word of a code; any other number stays a number.
    fn from_number(n: i32) -> Pos {
        [Pos::Left, Pos::Right, Pos::Top, Pos::Bottom, Pos::Center]
            .into_iter()
            .find(|p| p.code() == i64::from(n))
            .unwrap_or(Pos::Num(n))
    }
}

impl Dim {
    /// As [`Pos::code`].
    pub(crate) fn code(self) -> i64 {
        match self {
            Dim::Num(n) => i64::from(n),
            Dim::Whole => -31001,
            Dim::Half => -31002,
            Dim::Third => -31003,
            Dim::Quarter => -31004,
            Dim::FitFont => -31005,
        }
    }

    fn from_number(n: i32) -> Dim {
        [
            Dim::Whole,
            Dim::Half,
            Dim::Third,
            Dim::Quarter,
            Dim::FitFont,
        ]
        .into_iter()
        .find(|d| d.code() == i64::from(n))
        .unwrap_or(Dim::Num(n))
    }
}

/// A position value: a number if it starts with `-` or a digit, else the word its first letter
/// says (`l`, `r`, `t`, `b`, `c`); `None` for anything else, which leaves the default. A number
/// equal to the code of a word is that word.
fn pos(value: &[u8]) -> Option<Pos> {
    match value.first()? {
        b'-' | b'0'..=b'9' => to_int(value).map(Pos::from_number),
        b'l' => Some(Pos::Left),
        b'r' => Some(Pos::Right),
        b't' => Some(Pos::Top),
        b'b' => Some(Pos::Bottom),
        b'c' => Some(Pos::Center),
        _ => None,
    }
}

/// A size value, as [`pos`] with `w`, `h`, `t`, `q` and `f`.
fn dim(value: &[u8]) -> Option<Dim> {
    match value.first()? {
        b'-' | b'0'..=b'9' => to_int(value).map(Dim::from_number),
        b'w' => Some(Dim::Whole),
        b'h' => Some(Dim::Half),
        b't' => Some(Dim::Third),
        b'q' => Some(Dim::Quarter),
        b'f' => Some(Dim::FitFont),
        _ => None,
    }
}

/// Red, green, blue.
pub(crate) type Rgb = [u8; 3];

/// A picture of the cache of the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Picture {
    /// An index into [`Pictures::images`].
    Image(usize),
    /// An index into [`Pictures::animations`].
    Animation(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Justify {
    #[default]
    Left,
    Center,
    Right,
}

/// Where a scroll bar sits next to its text or list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    #[default]
    Right,
    Top,
    Bottom,
}

/// What a control shows and how: the keys of every type but `UILayout`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Look {
    /// The `animation` values in order, `None` where one could not be loaded. For a button the
    /// first is the animation of the four states.
    pub animation: Vec<Option<Picture>>,
    /// The `normal` image, read only without `animation` values.
    pub normal: Option<Picture>,
    /// The numbers of the strings that name the font and give its size; both or neither.
    pub font: Option<(i32, i32)>,
    pub text_id: Vec<i32>,
    pub text_name: Vec<u8>,
    pub justify: Justify,
    pub shadow_text: bool,
    pub fore_color: Rgb,
    pub back_color: Rgb,
    pub highlight_color: Rgb,
    pub icon_first: bool,
    /// The picture the game's code put in a button, drawn over its own or under it (`iconfirst`).
    pub icon: Option<Picture>,
    /// The sprite file of an animated icon that is centred in the button and clipped to it.
    pub icon_file: Option<usize>,
    pub color_as_icon: bool,
    pub volatile: bool,
    pub update_while_paused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    /// The images of the states without an animation; each defaults to `normal`.
    pub(crate) highlighted: Option<Picture>,
    pub(crate) selected: Option<Picture>,
    pub(crate) grayed: Option<Picture>,
    pub select_color: Rgb,
    pub gray_color: Rgb,
    /// Any value of the key turns it on, `repeating=0` too, as in the original.
    pub repeating: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub wrappable: bool,
    pub show_border: bool,
    /// The id of the scroll bar, 0 for none.
    pub scroll_bar: i32,
    pub(crate) scroll_bar_pos: Side,
    /// The first line shown, when a scroll bar scrolls the text.
    pub top: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub numeric: bool,
    /// 0: no limit.
    pub char_limit: i32,
    /// Whether the text may be wider than the field (no width check); set at run time.
    pub wide: bool,
    pub(crate) caret: Caret,
    /// The text when the field got the focus: Escape puts it back.
    pub saved: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrollBar {
    pub(crate) up_arrow: Option<Picture>,
    pub(crate) down_arrow: Option<Picture>,
    pub(crate) thumb: Option<Picture>,
    pub horizontal: bool,
    pub(crate) bar: Bar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListBox {
    pub scroll_bar: i32,
    pub(crate) scroll_bar_pos: Side,
    pub spacing: i32,
    /// The left offsets of the columns.
    pub columns: Vec<i32>,
    pub minicon_width: i32,
    pub minicon_height: i32,
    pub select_color: Rgb,
    pub select_back_color: Option<Rgb>,
    pub select_border_color: Option<Rgb>,
    pub highlight_border_color: Option<Rgb>,
    pub grayed_color: Rgb,
    pub toggle_select: bool,
    pub highlight_selection: bool,
    pub handle_double_clicks: bool,
    pub(crate) rows: Rows,
}

/// A scrolling region: a grid of buttons made from a template, filled by the game's code (ui
/// layout 4.10). The copies stand in the layout of the region and are anchored to it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Region {
    pub template: i32,
    pub radio_set: i32,
    pub scroll_bar: i32,
    /// The space between two buttons, across and down.
    pub spacing: (i32, i32),
    /// The first row shown.
    pub top: usize,
    /// The buttons made so far, which a refill reuses.
    pub copies: Vec<usize>,
    /// How many of them hold an item now.
    pub items: usize,
}

/// The pictures and percentages of `UIStatusImage` and `UIImageSet`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Levels {
    pub(crate) images: Vec<Option<Picture>>,
    pub transitions: Vec<i32>,
    /// The `animation` an image set shows (the game's code chooses it).
    pub shown: usize,
    /// The value, 0 to 100, a status bar shows.
    pub value: i32,
    /// The picture of a status bar the game's code chose instead of the value's.
    pub level: Option<usize>,
}

/// The type of an element with the keys of that type. The types that later milestones fill are
/// plain rectangles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Layout,
    Image,
    Button(Button),
    Text(Text),
    EditableText(Edit),
    /// The ids of the member buttons.
    RadioSet(Vec<i32>),
    ScrollBar(ScrollBar),
    ListBox(ListBox),
    StatusImage(Levels),
    ImageSet(Levels),
    ScrollingRegion(Region),
    Graph,
    MapView,
    MiniMap,
    MessageQueue,
    /// `UIMessageQueue`, which no shipped file uses.
    UiMessageQueue,
}

impl Kind {
    /// The name of the type as the files write it.
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Layout => "UILayout",
            Kind::Image => "UIImage",
            Kind::Button(_) => "UIButton",
            Kind::Text(_) => "UIText",
            Kind::EditableText(_) => "UIEditableText",
            Kind::RadioSet(_) => "UIRadioSet",
            Kind::ScrollBar(_) => "UIScrollBar",
            Kind::ListBox(_) => "UIListBox",
            Kind::StatusImage(_) => "UIStatusImage",
            Kind::ImageSet(_) => "UIImageSet",
            Kind::ScrollingRegion(_) => "UIScrollingRegion",
            Kind::Graph => "UIGraph",
            Kind::MapView => "ZTMapView",
            Kind::MiniMap => "ZTMiniMap",
            Kind::MessageQueue => "ZTMessageQueue",
            Kind::UiMessageQueue => "UIMessageQueue",
        }
    }
}

/// One node of the tree: a layout or a widget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub kind: Kind,
    /// The layout that holds it; `None` for the root.
    pub parent: Option<usize>,
    /// In creation order: ascending section names.
    pub children: Vec<usize>,
    pub id: i32,
    /// The state flags: 1 (hidden) and every value of `state`, and 1024 for `timed`.
    pub state: u32,
    pub(crate) x: Pos,
    pub(crate) y: Pos,
    pub(crate) alt_x: Pos,
    pub(crate) alt_y: Pos,
    pub(crate) dx: Dim,
    pub(crate) dy: Dim,
    pub x_is_percent: bool,
    pub y_is_percent: bool,
    /// `anchor1` if present, else `anchor`; 0 for none.
    pub anchor: i32,
    pub anchor2: i32,
    pub layer: i32,
    pub x_border: i32,
    pub y_border: i32,
    /// The tooltip string number first; the others are for the game's code.
    pub help_id: Vec<i32>,
    pub action: i32,
    /// `-1` in the file is the id of the layout.
    pub target: i32,
    /// `-1` in the file is the id of the layout.
    pub alias: i32,
    pub transparent: bool,
    pub dynamic_width: bool,
    pub dynamic_height: bool,
    pub timer_duration: i32,
    pub string_data: Vec<u8>,
    pub(crate) look: Look,
    /// The text the program gave the element, which shows instead of its `textid` or `textname`.
    pub text: Option<Vec<u8>>,
    /// The program switched the element to its alternative position.
    pub alt: bool,
    /// Milliseconds left of a timed element shown; `None` until the first interface update after
    /// it was shown.
    pub countdown: Option<i64>,
    /// The numbers the words of the file resolve to for a window size; `None` before
    /// [`Tree::resolve`].
    pub(crate) placed: Option<Placed>,
}

/// The cache of pictures, owned by the tree. A name is loaded once, found or not.
#[derive(Debug, Default)]
pub struct Pictures {
    pub images: Vec<Texture>,
    pub animations: Vec<Animation>,
    names: HashMap<String, Option<Picture>>,
}

impl Pictures {
    /// The width and height of a picture: the box of an animation, the size of an image.
    pub(crate) fn size(&self, picture: Picture) -> (i32, i32) {
        let side =
            |lo: i32, hi: i32| (i64::from(hi) - i64::from(lo)).clamp(0, i64::from(i32::MAX)) as i32;
        match picture {
            Picture::Image(i) => self.images.get(i).map_or((0, 0), |t| {
                (
                    t.width.min(i32::MAX as usize) as i32,
                    t.height.min(i32::MAX as usize) as i32,
                )
            }),
            Picture::Animation(a) => self.animations.get(a).map_or((0, 0), |a| {
                (side(a.set.x0, a.set.x1), side(a.set.y0, a.set.y1))
            }),
        }
    }

    /// The picture the value names: an image if it ends in `.tga` (any case), else the animation
    /// without extension. A failure is a warning, once per name.
    fn get(&mut self, vfs: &Vfs, value: &[u8], warnings: &mut Vec<String>) -> Option<Picture> {
        let name = crate::vfs::name_from_bytes(value);
        let image =
            name.len() >= 4 && name.as_bytes()[name.len() - 4..].eq_ignore_ascii_case(b".tga");
        let member = match image || name.contains(".ani") {
            true => name.to_string(),
            false => format!("{name}.ani"),
        };
        let key = normalise(&member);
        if let Some(known) = self.names.get(&key) {
            return *known;
        }
        let loaded = if image {
            vfs.read(&member)
                .map_err(|e| e.to_string())
                .and_then(|bytes| Texture::parse(&bytes))
                .map(|texture| {
                    self.images.push(texture);
                    Picture::Image(self.images.len() - 1)
                })
        } else {
            AnimSet::load(vfs, &member)
                .and_then(|set| Animation::open(vfs, set).map_err(|(e, _)| e))
                .map_err(|e| e.to_string())
                .map(|animation| {
                    self.animations.push(animation);
                    Picture::Animation(self.animations.len() - 1)
                })
        };
        let picture = match loaded {
            Ok(picture) => Some(picture),
            Err(why) => {
                warnings.push(format!("picture {member}: {why}"));
                None
            }
        };
        self.names.insert(key, picture);
        picture
    }
}

/// Where a load stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub file: String,
    /// The section, empty if the whole file failed.
    pub section: String,
    pub why: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.section.is_empty() {
            true => write!(f, "{}: {}", self.file, self.why),
            false => write!(f, "{} [{}]: {}", self.file, self.section, self.why),
        }
    }
}

/// The interface tree: elements in an arena, the root first, every layout before its children.
#[derive(Debug, Default)]
pub struct Tree {
    pub elements: Vec<Element>,
    /// The first element registered under each id.
    ids: HashMap<i32, usize>,
    /// The elements that were not registered because their id was taken, in registration order.
    pub duplicates: Vec<usize>,
    pub pictures: Pictures,
    pub warnings: Vec<String>,
    /// Why the load failed, if it did; the tree is then empty.
    pub failure: Option<Failure>,
    /// The expansions installed: bit 1 and bit 2, the `id` of an `xpac*.cfg`.
    expansions: i32,
}

/// The expansions of the install: each `xpac*.cfg` with `[expansion] id` 1 or 2 sets that bit.
pub fn installed_expansions(vfs: &Vfs) -> i32 {
    vfs.list(Some("xpac*.cfg"))
        .iter()
        .filter_map(|member| {
            Config::parse(&vfs.read(member).ok()?)
                .ok()?
                .section("expansion")?
                .get("id")
                .and_then(to_int)
        })
        .filter(|id| matches!(id, 1 | 2))
        .fold(0, |mask, id| mask | id)
}

/// An expansion of the selector of the purchase panels: the `id` of an `xpac*.cfg` and the string
/// of its row (`listid`; entity types 10.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Expansion {
    pub id: i32,
    pub list: i32,
}

/// The `xpac*.cfg` files with an `[expansion] id` and `listid`, by ascending id (one per id).
pub(crate) fn expansion_entries(vfs: &Vfs) -> Vec<Expansion> {
    let mut entries: Vec<Expansion> = vfs
        .list(Some("xpac*.cfg"))
        .iter()
        .filter_map(|member| {
            let config = Config::parse(&vfs.read(member).ok()?).ok()?;
            let section = config.section("expansion")?;
            let number = |key| section.get(key).and_then(to_int);
            Some(Expansion {
                id: number("id")?,
                list: number("listid")?,
            })
        })
        .collect();
    entries.sort_by_key(|e| e.id);
    entries.dedup_by_key(|e| e.id);
    entries
}

impl Tree {
    /// Loads the layout file `root` (`ui/gamescrn.lyt`) and every layout below it, for the
    /// expansions in the mask. Never fails: see [`Tree::failure`].
    pub fn load(vfs: &Vfs, root: &str, expansions: i32) -> Tree {
        let mut tree = Tree {
            expansions,
            ..Tree::default()
        };
        tree.failure = tree.load_layout(vfs, root, None, 0).err();
        if tree.failure.is_some() {
            // A failure that reaches the root drops the whole interface.
            tree.elements.clear();
        }
        // Creation order is depth first with the layout before its children.
        for index in 0..tree.elements.len() {
            let id = tree.elements[index].id;
            match tree.ids.entry(id) {
                Entry::Vacant(slot) => {
                    slot.insert(index);
                }
                Entry::Occupied(_) => tree.duplicates.push(index),
            }
        }
        tree
    }

    /// Shows the layout `id` and everything below it, to any depth: clears the hidden flag of
    /// every element except those that are never shown, and leaves everything inside a layout
    /// that is never shown as it was. The children are shown before their layout. `false` if
    /// there is no such element.
    pub fn show(&mut self, id: i32) -> bool {
        let Some(layout) = self.find(id) else {
            return false;
        };
        self.show_below(layout);
        self.elements[layout].state &= !HIDDEN;
        true
    }

    fn show_below(&mut self, layout: usize) {
        // The nesting of layouts is bounded by MAX_DEPTH.
        for child in self.elements[layout].children.clone() {
            if self.elements[child].state & NEVER_SHOWN == 0 {
                self.show_below(child);
                self.elements[child].state &= !HIDDEN;
            }
        }
    }

    /// The picture the game's files have under `name` (an animation without its extension, or a
    /// `.tga` image), loaded once. A failure is a warning, once per name, and `None`.
    pub(crate) fn picture(&mut self, vfs: &Vfs, name: &str) -> Option<Picture> {
        let before = self.warnings.len();
        let picture = self.pictures.get(vfs, name.as_bytes(), &mut self.warnings);
        for warning in &self.warnings[before..] {
            eprintln!("rustyzoo: warning: {warning}");
        }
        picture
    }

    /// The element registered under `id`.
    pub fn find(&self, id: i32) -> Option<usize> {
        self.ids.get(&id).copied()
    }

    /// Adds a copy of element `template` under the new `id`, as the last child of the layout
    /// `parent` and anchored to element `anchor`, and returns its index. The copy is not a
    /// template any more and keeps the template's state and look.
    pub fn add_copy(&mut self, template: usize, id: i32, parent: usize, anchor: usize) -> usize {
        let mut copy = self.elements[template].clone();
        copy.id = id;
        copy.parent = Some(parent);
        copy.children = Vec::new();
        copy.state &= !TEMPLATE;
        copy.anchor = self.elements[anchor].id;
        copy.anchor2 = 0;
        if let Some(placed) = &mut copy.placed {
            placed.anchor = Some(anchor);
        }
        let index = self.elements.len();
        self.elements.push(copy);
        self.elements[parent].children.push(index);
        self.ids.entry(id).or_insert(index);
        index
    }

    /// How many elements there are of each type.
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for element in &self.elements {
            *counts.entry(element.kind.name()).or_insert(0) += 1;
        }
        counts
    }
}

/// Three values of a repeated colour key; `None` if the key was written fewer than three times.
pub(crate) fn color(s: &Section, key: &str) -> Option<Rgb> {
    match s.get_all(key) {
        [r, g, b, ..] => Some([r, g, b].map(|v| to_int(v).unwrap_or(0).clamp(0, 255) as u8)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

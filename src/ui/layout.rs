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

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

use crate::anim::{AnimSet, Animation};
use crate::config::{Config, Section, to_flag, to_int};
use crate::tga::Texture;
use crate::vfs::{Vfs, normalise};

use super::resolve::Placed;

/// State flag: not drawn, not hit. Every element starts with it.
pub const HIDDEN: u32 = 1;
/// State flag: a button is drawn grayed and gets no mouse input.
pub const DISABLED: u32 = 2;
/// State flag: the pointer is over the element.
pub const HIGHLIGHTED: u32 = 4;
/// State flag: a button is drawn pressed.
pub const SELECTED: u32 = 8;
/// State flag: not shown when its layout is shown.
pub const NEVER_SHOWN: u32 = 16;
/// State flag: the canvas must be drawn again.
pub const DIRTY: u32 = 128;
/// State flag: never drawn nor hit.
pub const TEMPLATE: u32 = 256;
/// State flag: the element has the keyboard focus.
pub const FOCUSED: u32 = 512;
/// State flag: set by the `timed` key.
pub const TIMED: u32 = 1024;
/// State flags: a toggle button, a sticky button.
pub const TOGGLE: u32 = 2048;
pub const STICKY: u32 = 4096;
/// State flag of a selected toggle button held with the pointer over it: drawn unselected.
pub const RELEASING: u32 = 8192;
/// State flag of an unselected toggle or sticky button held with the pointer over it: drawn
/// pressed.
pub const PRESSING: u32 = 16384;

/// The deepest chain of layouts naming layouts. The shipped files go 4 deep.
const MAX_DEPTH: usize = 16;
/// The most elements one tree holds. The full game has 1,138.
const MAX_ELEMENTS: usize = 50_000;

/// A position as the file wrote it. The words are resolved against the reference rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pos {
    Num(i32),
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

/// A size as the file wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dim {
    Num(i32),
    Whole,
    Half,
    Third,
    Quarter,
    FitFont,
}

impl Pos {
    /// The number the original keeps for a word, and for a number the number itself.
    pub fn code(self) -> i64 {
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
    pub fn code(self) -> i64 {
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
pub type Rgb = [u8; 3];

/// A picture of the cache of the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Picture {
    /// An index into [`Pictures::images`].
    Image(usize),
    /// An index into [`Pictures::animations`].
    Animation(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Left,
    Center,
    Right,
}

/// Where a scroll bar sits next to its text or list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    Left,
    #[default]
    Right,
    Top,
    Bottom,
}

/// What a control shows and how: the keys of every type but `UILayout`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Look {
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
    pub color_as_icon: bool,
    pub volatile: bool,
    pub update_while_paused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    /// The images of the states without an animation; each defaults to `normal`.
    pub highlighted: Option<Picture>,
    pub selected: Option<Picture>,
    pub grayed: Option<Picture>,
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
    pub scroll_bar_pos: Side,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub numeric: bool,
    /// 0: no limit.
    pub char_limit: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrollBar {
    pub up_arrow: Option<Picture>,
    pub down_arrow: Option<Picture>,
    pub thumb: Option<Picture>,
    pub horizontal: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListBox {
    pub scroll_bar: i32,
    pub scroll_bar_pos: Side,
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
}

/// The pictures and percentages of `UIStatusImage` and `UIImageSet`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Levels {
    pub images: Vec<Option<Picture>>,
    pub transitions: Vec<i32>,
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
    ScrollingRegion,
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
            Kind::ScrollingRegion => "UIScrollingRegion",
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
    pub x: Pos,
    pub y: Pos,
    pub alt_x: Pos,
    pub alt_y: Pos,
    pub dx: Dim,
    pub dy: Dim,
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
    pub look: Look,
    /// The program switched the element to its alternative position.
    pub alt: bool,
    /// Milliseconds left of a timed element shown; `None` until the first interface update after
    /// it was shown.
    pub countdown: Option<i64>,
    /// The numbers the words of the file resolve to for a window size; `None` before
    /// [`Tree::resolve`].
    pub placed: Option<Placed>,
}

/// The cache of pictures, owned by the tree. A name is loaded once, found or not.
#[derive(Debug, Default)]
pub struct Pictures {
    pub images: Vec<Texture>,
    pub animations: Vec<Animation>,
    names: HashMap<String, Option<Picture>>,
}

impl Pictures {
    /// The picture the value names: an image if it ends in `.tga` (any case), else the animation
    /// without extension. A failure is a warning, once per name.
    fn get(&mut self, vfs: &Vfs, value: &[u8], warnings: &mut Vec<String>) -> Option<Picture> {
        let name = String::from_utf8_lossy(value);
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

    /// The element registered under `id`.
    pub fn find(&self, id: i32) -> Option<usize> {
        self.ids.get(&id).copied()
    }

    /// How many elements there are of each type.
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for element in &self.elements {
            *counts.entry(element.kind.name()).or_insert(0) += 1;
        }
        counts
    }

    fn load_layout(
        &mut self,
        vfs: &Vfs,
        file: &str,
        parent: Option<usize>,
        depth: usize,
    ) -> Result<(), Failure> {
        let stop = |section: &str, why: String| Failure {
            file: file.to_string(),
            section: section.to_string(),
            why,
        };
        if depth > MAX_DEPTH {
            return Err(stop("", "layouts nested too deeply".into()));
        }
        // A file that cannot be read is an empty layout, with a warning.
        let config = vfs
            .read(file)
            .map_err(|e| e.to_string())
            .and_then(|bytes| Config::parse(&bytes).map_err(|e| e.to_string()))
            .unwrap_or_else(|why| {
                self.warnings.push(format!("{file}: {why}"));
                Config::default()
            });
        let empty = Section::default();
        let is_info = |name: &[u8]| name.eq_ignore_ascii_case(b"LayoutInfo");
        let info = config
            .sections()
            .find(|(name, _)| is_info(name))
            .map_or(&empty, |(_, section)| section);
        let this = self.add(vfs, Kind::Layout, info, parent);
        for (name, section) in config.sections() {
            if is_info(name) {
                continue;
            }
            let name = String::from_utf8_lossy(name);
            if let Some(mask) = section.get("expansion").and_then(to_int)
                && mask & self.expansions == 0
            {
                continue;
            }
            if self.elements.len() >= MAX_ELEMENTS {
                return Err(stop(&name, "too many elements".into()));
            }
            let type_name = section.get("type").unwrap_or_default();
            if type_name == b"UILayout" {
                let Some(path) = section.get("layout") else {
                    // The element is made, then the load fails.
                    self.add(vfs, Kind::Layout, &empty, Some(this));
                    return Err(stop(&name, "no layout".into()));
                };
                self.load_layout(vfs, &String::from_utf8_lossy(path), Some(this), depth + 1)?;
                continue;
            }
            let kind =
                self.kind(vfs, type_name, section)
                    .ok_or_else(|| match type_name.is_empty() {
                        true => stop(&name, "no type".into()),
                        false => stop(
                            &name,
                            format!("unknown type {}", String::from_utf8_lossy(type_name)),
                        ),
                    })?;
            self.add(vfs, kind, section, Some(this));
        }
        Ok(())
    }

    /// The type of a section with its own keys; `None` for a name that is not one of the types.
    fn kind(&mut self, vfs: &Vfs, name: &[u8], s: &Section) -> Option<Kind> {
        let mut picture = |key: &str| {
            s.get(key)
                .and_then(|value| self.pictures.get(vfs, value, &mut self.warnings))
        };
        let side = || match s.get("scrollbarPos").map(|v| v.to_ascii_lowercase()) {
            Some(v) if v == b"left" => Side::Left,
            Some(v) if v == b"top" => Side::Top,
            Some(v) if v == b"bottom" => Side::Bottom,
            _ => Side::Right,
        };
        let flag = |key: &str, default: bool| s.get(key).and_then(to_flag).unwrap_or(default);
        let int = |key: &str| s.get(key).and_then(to_int).unwrap_or(0);
        let ints = |key: &str| s.get_all(key).iter().filter_map(|v| to_int(v)).collect();
        Some(match name {
            b"UIImage" => Kind::Image,
            b"UIButton" => {
                let normal = picture("normal");
                Kind::Button(Button {
                    highlighted: picture("highlighted").or(normal),
                    selected: picture("selected").or(normal),
                    grayed: picture("grayed").or(normal),
                    select_color: color(s, "selectcolor").unwrap_or([0; 3]),
                    gray_color: color(s, "graycolor").unwrap_or([0; 3]),
                    repeating: s.get("repeating").is_some(),
                })
            }
            b"UIText" => Kind::Text(Text {
                wrappable: flag("wrappable", true),
                show_border: flag("showborder", false),
                scroll_bar: int("scrollbar"),
                scroll_bar_pos: side(),
            }),
            b"UIEditableText" => Kind::EditableText(Edit {
                numeric: flag("numeric", false),
                char_limit: int("charlimit"),
            }),
            b"UIRadioSet" => Kind::RadioSet(ints("button")),
            b"UIScrollBar" => Kind::ScrollBar(ScrollBar {
                up_arrow: picture("upArrow"),
                down_arrow: picture("downArrow"),
                thumb: picture("thumb"),
                horizontal: flag("horizontal", false),
            }),
            b"UIListBox" => Kind::ListBox(ListBox {
                scroll_bar: int("scrollbar"),
                scroll_bar_pos: side(),
                spacing: int("spacing"),
                columns: ints("column"),
                minicon_width: int("miniconwidth"),
                minicon_height: int("miniconheight"),
                select_color: color(s, "selectcolor")
                    .or(color(s, "highlightcolor"))
                    .unwrap_or([255, 255, 0]),
                select_back_color: color(s, "selectbackcolor"),
                select_border_color: color(s, "selectbordercolor"),
                highlight_border_color: color(s, "highlightbordercolor"),
                grayed_color: color(s, "grayedcolor").unwrap_or([128; 3]),
                toggle_select: flag("toggleselect", false),
                highlight_selection: flag("highlightselection", true),
                handle_double_clicks: flag("handledoubleclicks", false),
            }),
            b"UIStatusImage" | b"UIImageSet" => {
                let levels = Levels {
                    images: s
                        .get_all("image")
                        .iter()
                        .map(|v| self.pictures.get(vfs, v, &mut self.warnings))
                        .collect(),
                    transitions: ints("transition"),
                };
                match name {
                    b"UIImageSet" => Kind::ImageSet(levels),
                    _ => Kind::StatusImage(levels),
                }
            }
            b"UIScrollingRegion" => Kind::ScrollingRegion,
            b"UIGraph" => Kind::Graph,
            b"ZTMapView" => Kind::MapView,
            b"ZTMiniMap" => Kind::MiniMap,
            b"ZTMessageQueue" => Kind::MessageQueue,
            b"UIMessageQueue" => Kind::UiMessageQueue,
            _ => return None,
        })
    }

    /// Creates an element of `kind` from the keys of `s`, as the last child of `parent`.
    fn add(&mut self, vfs: &Vfs, kind: Kind, s: &Section, parent: Option<usize>) -> usize {
        let layout = matches!(kind, Kind::Layout);
        let flag = |key: &str, default: bool| s.get(key).and_then(to_flag).unwrap_or(default);
        let int = |key: &str, default: i32| s.get(key).and_then(to_int).unwrap_or(default);
        let at = |key: &str| s.get(key).and_then(pos);
        let size = |key: &str| s.get(key).and_then(dim);
        let id = int("id", 0);
        let parent_id = parent.map_or(0, |p| self.elements[p].id);
        let in_layout = |value: i32| if value == -1 { parent_id } else { value };
        // The root is not hidden to start with.
        let mut state = if parent.is_some() { HIDDEN } else { 0 };
        for value in s.get_all("state") {
            state |= to_int(value).unwrap_or(0) as u32;
        }
        if flag("timed", false) {
            state |= TIMED;
        }
        let border = int("border", 0);
        let button = matches!(kind, Kind::Button(_));
        let text = matches!(kind, Kind::Text(_));
        let own_background = matches!(kind, Kind::Text(_) | Kind::ListBox(_) | Kind::ScrollBar(_));
        let mut look = Look {
            animation: Vec::new(),
            normal: None,
            font: None,
            text_id: Vec::new(),
            text_name: Vec::new(),
            justify: Justify::Left,
            shadow_text: button || text,
            fore_color: [0; 3],
            back_color: if button { [0; 3] } else { [255; 3] },
            highlight_color: [255, 255, 0],
            icon_first: false,
            color_as_icon: false,
            volatile: true,
            update_while_paused: true,
        };
        if !layout {
            let (animation, normal) = match s.get_all("animation") {
                [] => (
                    Vec::new(),
                    match kind {
                        Kind::Image
                        | Kind::Button(_)
                        | Kind::Text(_)
                        | Kind::ScrollBar(_)
                        | Kind::ListBox(_) => s
                            .get("normal")
                            .and_then(|v| self.pictures.get(vfs, v, &mut self.warnings)),
                        _ => None,
                    },
                ),
                values => (
                    values
                        .iter()
                        .map(|v| self.pictures.get(vfs, v, &mut self.warnings))
                        .collect(),
                    None,
                ),
            };
            look = Look {
                animation,
                normal,
                font: match (
                    s.get("font").and_then(to_int),
                    s.get("fontsize").and_then(to_int),
                ) {
                    (Some(face), Some(size)) => Some((face, size)),
                    _ => None,
                },
                text_id: s
                    .get_all("textid")
                    .iter()
                    .filter_map(|v| to_int(v))
                    .collect(),
                text_name: s.get("textname").unwrap_or_default().to_vec(),
                justify: match s.get("justify").and_then(|v| v.first()) {
                    Some(b'c') => Justify::Center,
                    Some(b'r') => Justify::Right,
                    _ => Justify::Left,
                },
                shadow_text: flag("shadowtext", look.shadow_text),
                fore_color: color(s, "forecolor").unwrap_or(look.fore_color),
                back_color: color(s, "backcolor").unwrap_or(look.back_color),
                highlight_color: color(s, "highlightcolor").unwrap_or(look.highlight_color),
                icon_first: flag("iconfirst", false),
                color_as_icon: flag("colorasicon", false),
                volatile: flag("volatile", true),
                update_while_paused: flag("updateWhilePaused", true),
            };
        }
        let index = self.elements.len();
        self.elements.push(Element {
            kind,
            parent,
            children: Vec::new(),
            id,
            state,
            x: at("x").unwrap_or(Pos::Num(0)),
            y: at("y").unwrap_or(Pos::Num(0)),
            alt_x: at("altx").unwrap_or(Pos::Num(0)),
            alt_y: at("alty").unwrap_or(Pos::Num(0)),
            dx: size("dx").unwrap_or(Dim::Num(0)),
            dy: size("dy").unwrap_or(Dim::Num(0)),
            x_is_percent: flag("xispercent", false),
            y_is_percent: flag("yispercent", false),
            anchor: s
                .get("anchor1")
                .or(s.get("anchor"))
                .and_then(to_int)
                .unwrap_or(0),
            anchor2: int("anchor2", 0),
            layer: int("layer", 0),
            x_border: int("xborder", border),
            y_border: int("yborder", border),
            help_id: s
                .get_all("helpid")
                .iter()
                .filter_map(|v| to_int(v))
                .collect(),
            action: int("action", 0),
            target: in_layout(int("target", id)),
            alias: in_layout(int("alias", 0)),
            transparent: flag("transparent", !own_background),
            dynamic_width: flag("dynamicwidth", false),
            dynamic_height: flag("dynamicheight", false),
            timer_duration: int("timerDuration", 3000),
            string_data: s.get("stringData").unwrap_or_default().to_vec(),
            look,
            alt: false,
            countdown: None,
            placed: None,
        });
        if let Some(parent) = parent {
            self.elements[parent].children.push(index);
        }
        index
    }
}

/// Three values of a repeated colour key; `None` if the key was written fewer than three times.
fn color(s: &Section, key: &str) -> Option<Rgb> {
    match s.get_all(key) {
        [r, g, b, ..] => Some([r, g, b].map(|v| to_int(v).unwrap_or(0).clamp(0, 255) as u8)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

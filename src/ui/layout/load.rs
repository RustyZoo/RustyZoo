//! Loading the layout files into the tree: sections to elements.

use super::*;

impl Tree {
    pub(super) fn load_layout(
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
        self.layout_files.insert(this, file.to_string());
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
                self.load_layout(
                    vfs,
                    &crate::vfs::name_from_bytes(path),
                    Some(this),
                    depth + 1,
                )?;
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
    pub(super) fn kind(&mut self, vfs: &Vfs, name: &[u8], s: &Section) -> Option<Kind> {
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
                top: 0,
            }),
            b"UIEditableText" => Kind::EditableText(Edit {
                numeric: flag("numeric", false),
                char_limit: int("charlimit"),
                wide: false,
                caret: Caret::default(),
                saved: Vec::new(),
            }),
            b"UIRadioSet" => Kind::RadioSet(ints("button")),
            b"UIScrollBar" => Kind::ScrollBar(ScrollBar {
                up_arrow: picture("upArrow"),
                down_arrow: picture("downArrow"),
                thumb: picture("thumb"),
                horizontal: flag("horizontal", false),
                bar: Bar::default(),
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
                rows: Rows::default(),
            }),
            b"UIStatusImage" | b"UIImageSet" => {
                let levels = Levels {
                    images: s
                        .get_all("image")
                        .iter()
                        .map(|v| self.pictures.get(vfs, v, &mut self.warnings))
                        .collect(),
                    transitions: ints("transition"),
                    shown: 0,
                    value: 0,
                    level: None,
                };
                match name {
                    b"UIImageSet" => Kind::ImageSet(levels),
                    _ => Kind::StatusImage(levels),
                }
            }
            b"UIScrollingRegion" => Kind::ScrollingRegion(Region {
                template: int("template"),
                radio_set: int("radioset"),
                scroll_bar: int("scrollbar"),
                spacing: match ints("spacing")[..] {
                    [across, down, ..] => (across, down),
                    [across] => (across, 0),
                    [] => (0, 0),
                },
                ..Region::default()
            }),
            b"UIGraph" => Kind::Graph,
            b"ZTMapView" => Kind::MapView,
            b"ZTMiniMap" => Kind::MiniMap,
            b"ZTMessageQueue" => Kind::MessageQueue,
            b"UIMessageQueue" => Kind::UiMessageQueue,
            _ => return None,
        })
    }

    /// Creates an element of `kind` from the keys of `s`, as the last child of `parent`.
    pub(super) fn add(
        &mut self,
        vfs: &Vfs,
        kind: Kind,
        s: &Section,
        parent: Option<usize>,
    ) -> usize {
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
            icon: None,
            icon_file: None,
            color_as_icon: false,
            volatile: true,
            update_while_paused: true,
            shown: 0,
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
                // Only the first value is used when loading; the others are alternatives the
                // program may pick later (the main menu's background), and one that is not in the
                // game's files (the update screen's second picture) is no warning.
                values => (
                    values
                        .iter()
                        .enumerate()
                        .map(|(n, v)| {
                            let mut unheard = Vec::new();
                            let warnings = if n == 0 {
                                &mut self.warnings
                            } else {
                                &mut unheard
                            };
                            self.pictures.get(vfs, v, warnings)
                        })
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
                icon: None,
                icon_file: None,
                color_as_icon: flag("colorasicon", false),
                volatile: flag("volatile", true),
                update_while_paused: flag("updateWhilePaused", true),
                shown: 0,
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
            queue: MessageLine::read(s),
            string_data: s.get("stringData").unwrap_or_default().to_vec(),
            look,
            text: None,
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

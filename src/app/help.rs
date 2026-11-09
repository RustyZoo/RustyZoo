//! The help window (ui layout 16.7): the topics of the `help*.cfg` files, sorted, shown by the
//! Help button or F1, with the game paused meanwhile.

use super::*;
use crate::config::to_int;
use crate::ui::widgets::Row;

pub(super) const LAYOUT: i32 = 38;
pub(super) const LIST: i32 = 28003;
const TITLE: i32 = 28002;
const TEXT: i32 = 28005;

/// The help window's topics and where it stands.
#[derive(Default)]
pub(crate) struct Help {
    /// The title and the name of the text of each row.
    topics: Vec<(Vec<u8>, Vec<u8>)>,
    /// The topic shown last.
    last: usize,
    shown: bool,
    /// The window paused the game, so closing it unpauses.
    paused: bool,
}

/// How a topic sorts: About, Support and Quick start (by section name) first, then the others.
fn rank(section: &[u8]) -> usize {
    [&b"About"[..], b"Support", b"Quick start"]
        .iter()
        .position(|n| *n == section)
        .unwrap_or(3)
}

/// `text` with its first two `%s` replaced by `first` and `second`.
fn fill(text: &[u8], first: &str, second: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut with = [first, second].into_iter();
    let mut rest = text;
    while let Some(at) = rest.windows(2).position(|w| w == b"%s") {
        out.extend_from_slice(&rest[..at]);
        match with.next() {
            Some(value) => out.extend_from_slice(&to_cp1252(value)),
            None => out.extend_from_slice(b"%s"),
        }
        rest = &rest[at + 2..];
    }
    out.extend_from_slice(rest);
    out
}

impl App<'_> {
    /// The topics of the files `help*.cfg` that have a `[Version] version` above 0 (16.7 point
    /// 3), put in the list box once.
    pub(super) fn help_build(&mut self) {
        let Some(ui) = &mut self.ui else { return };
        let vfs = self.renderer.vfs();
        // Section, title and text of each topic; the first of the topics with one title stays.
        let mut found: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
        for file in vfs.listed("help", ".cfg") {
            let Some(cfg) = vfs.read(&file).ok().and_then(|b| Config::parse(&b).ok()) else {
                continue;
            };
            let version = (cfg.section("Version")).and_then(|s| s.get("version").and_then(to_int));
            let Some(topics) = cfg
                .section("Topics")
                .filter(|_| version.is_some_and(|v| v > 0))
            else {
                continue;
            };
            for name in topics.get_all("topic") {
                let get = |key: &str| cfg.section(name).and_then(|s| s.get(key));
                let title = get("title").and_then(to_int).unwrap_or(0);
                let title = to_cp1252(&ui.string(title));
                let same = |(n, t, _): &(Vec<u8>, Vec<u8>, Vec<u8>)| match rank(name) {
                    3 => rank(n) == 3 && *t == title,
                    _ => n == name,
                };
                if !found.iter().any(same) {
                    found.push((
                        name.clone(),
                        title,
                        get("text").unwrap_or_default().to_vec(),
                    ));
                }
            }
        }
        found.sort_by(|a, b| (rank(&a.0), &a.1).cmp(&(rank(&b.0), &b.1)));
        let rows = (found.iter())
            .map(|(_, title, _)| Row {
                text: title.clone(),
                ..Row::default()
            })
            .collect();
        ui.set_rows(LIST, rows);
        self.help.topics = found
            .into_iter()
            .map(|(_, title, text)| (title, text))
            .collect();
    }

    /// Shows topic `i` (16.7 point 4): the row selected, the title and the text put in place;
    /// the About topic fills in the version and the product id. Nothing when `i` is no row.
    pub(super) fn help_show(&mut self, i: usize) {
        let Some((title, text)) = self.help.topics.get(i) else {
            return;
        };
        let Some(ui) = &mut self.ui else { return };
        ui.select_row(LIST, Some(i));
        ui.set_text(TITLE, title);
        let mut text = ui.text_entry(text);
        if i == 0 {
            let version = crate::startup::product_version(self.renderer.vfs().root())
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
            text = fill(&text, &version, "----");
        }
        ui.set_text(TEXT, &text);
        self.help.last = i;
    }

    /// The Help button or F1 (16.7 point 1): a hidden window opens on topic 0 (the button) or on
    /// the topic shown last; a shown one shows that topic again.
    pub(super) fn help_open(&mut self, button: bool) {
        if button {
            self.help.last = 0;
        }
        let Some(ui) = &mut self.ui else { return };
        match ui.shown(LAYOUT) {
            true => self.help_show(self.help.last),
            false => {
                ui.show(LAYOUT);
            }
        }
    }

    /// Whether the window is shown.
    pub(super) fn help_shown(&self) -> bool {
        self.game.is_some() && self.ui.as_ref().is_some_and(|ui| ui.shown(LAYOUT))
    }

    /// Escape: closes the window.
    pub(super) fn help_close(&mut self) {
        if let Some(ui) = &mut self.ui {
            ui.conceal(LAYOUT, true);
        }
    }

    /// The window shown or hidden since the last look: shown, it pauses a game that runs and
    /// shows the topic; hidden, it unpauses a game it paused (16.7 points 1, 2).
    pub(super) fn help_pass(&mut self) {
        let shown = self.help_shown();
        if shown == std::mem::replace(&mut self.help.shown, shown) {
            return;
        }
        if shown {
            let running = self.game.as_ref().is_some_and(|g| !g.paused);
            self.help.paused = running;
            if running {
                self.pause_to(true);
            }
            self.help_show(self.help.last);
        } else if std::mem::take(&mut self.help.paused) {
            self.pause_to(false);
        }
    }
}

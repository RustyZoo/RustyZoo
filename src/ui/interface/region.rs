//! Scrolling regions at work (ui layout 4.10): the game's code gives a region one icon per item;
//! the region makes a button from its template for each, in a grid of whole rows that a scroll
//! bar and the wheel move. The buttons belong to the layout of the region, are anchored to it and
//! join its radio set; the ones in rows that are not shown are never shown, so showing the
//! layout again does not bring them back. A refill reuses the buttons made before.

use super::Interface;
use crate::ui::layout::{
    HIDDEN, HIGHLIGHTED, Kind, NEVER_SHOWN, PRESSING, Picture, Pos, RELEASING, SELECTED,
};

/// The most items a region holds. The button of item `n` of region `id` has the id
/// `COPY_IDS + id * 1000 + n`.
pub const MAX_ITEMS: usize = 1000;
const COPY_IDS: i32 = 1_000_000;

/// The shape of a region's grid: the columns, the whole rows that fit, one button's size and
/// the distance from a button to the next, and where the first column and row start (the grid
/// is centred in the region).
struct Grid {
    cols: usize,
    fit: usize,
    size: (i32, i32),
    step: (i32, i32),
    left: i32,
    up: i32,
}

impl Interface {
    /// Fills the region `id` with a button for each icon, at most [`MAX_ITEMS`]: none is selected
    /// and the first row shows. The region is hidden when there is no icon. `false` if there is
    /// no such region, or its template is not a button.
    pub fn set_items(&mut self, id: i32, icons: &[Option<Picture>]) -> bool {
        let Some(r) = self.region(id) else {
            return false;
        };
        let Kind::ScrollingRegion(region) = &self.tree.elements[r].kind else {
            return false;
        };
        let (template, radio, mut copies) =
            (region.template, region.radio_set, region.copies.clone());
        let (Some(template), Some(parent)) = (
            self.tree
                .find(template)
                .filter(|&t| matches!(self.tree.elements[t].kind, Kind::Button(_))),
            self.tree.elements[r].parent,
        ) else {
            return false;
        };
        let n = icons.len().min(MAX_ITEMS);
        while copies.len() < n {
            let id = COPY_IDS + id.saturating_mul(1000) + copies.len() as i32;
            copies.push(self.tree.add_copy(template, id, parent, r));
        }
        let layers = self
            .tree
            .elements
            .iter()
            .map(|e| e.layer)
            .collect::<Vec<_>>();
        self.tree.elements[parent]
            .children
            .sort_by_key(|&c| layers[c]);
        for (k, &c) in copies.iter().enumerate() {
            let e = &mut self.tree.elements[c];
            e.look.icon = icons.get(k).copied().flatten().filter(|_| k < n);
            e.state &= !(SELECTED | HIGHLIGHTED | PRESSING | RELEASING);
        }
        let ids: Vec<i32> = copies[..n].iter().map(|&c| self.id(c)).collect();
        if let Some(set) = self.tree.find(radio)
            && let Kind::RadioSet(members) = &mut self.tree.elements[set].kind
        {
            *members = ids;
        }
        if let Kind::ScrollingRegion(region) = &mut self.tree.elements[r].kind {
            (region.copies, region.items, region.top) = (copies, n, 0);
        }
        self.flag(r, HIDDEN, n == 0);
        self.event(format!("items {id} {n}"));
        self.place_region(r);
        self.sync_owner(r);
        true
    }

    /// Selects item `item` of the region `id`, or none, and scrolls to show it. Does not run any
    /// action. `false` if there is no such region or item.
    pub fn select_item(&mut self, id: i32, item: Option<usize>) -> bool {
        let Some(r) = self.region(id) else {
            return false;
        };
        let (Some(grid), Kind::ScrollingRegion(region)) =
            (self.grid(r), &mut self.tree.elements[r].kind)
        else {
            return false;
        };
        if item.is_some_and(|k| k >= region.items) {
            return false;
        }
        if let Some(k) = item {
            let row = k / grid.cols;
            region.top = region.top.clamp((row + 1).saturating_sub(grid.fit), row);
        }
        let copies = region.copies.clone();
        for (k, c) in copies.into_iter().enumerate() {
            self.flag(c, SELECTED, Some(k) == item);
        }
        self.place_region(r);
        self.sync_owner(r);
        true
    }

    /// The first row of the region `id` that shows.
    pub fn top(&self, id: i32) -> usize {
        match self.region(id).map(|r| &self.tree.elements[r].kind) {
            Some(Kind::ScrollingRegion(region)) => region.top,
            _ => 0,
        }
    }

    /// Scrolls the region `id` to start at row `top`, or as far as it can go. Does not touch the
    /// selection.
    pub fn set_top(&mut self, id: i32, top: usize) {
        let Some(r) = self.region(id) else { return };
        let most = self.region_extent(r).map_or(0, |e| e.0);
        if let Kind::ScrollingRegion(region) = &mut self.tree.elements[r].kind {
            region.top = top.min(most);
        }
        self.place_region(r);
        self.sync_owner(r);
    }

    /// The item of region `id` that the button `button` shows.
    pub fn item_of(&self, id: i32, button: i32) -> Option<usize> {
        let Kind::ScrollingRegion(region) = &self.tree.elements[self.region(id)?].kind else {
            return None;
        };
        region.copies[..region.items]
            .iter()
            .position(|&c| self.tree.elements[c].id == button)
    }

    fn region(&self, id: i32) -> Option<usize> {
        self.tree
            .find(id)
            .filter(|&r| matches!(self.tree.elements[r].kind, Kind::ScrollingRegion(_)))
    }

    /// The region whose button element `i` is.
    pub(super) fn region_of(&self, i: usize) -> Option<usize> {
        self.tree.elements.iter().position(
            |e| matches!(&e.kind, Kind::ScrollingRegion(region) if region.copies.contains(&i)),
        )
    }

    /// The grid of region `r` from its size and its template's; `None` for anything else.
    fn grid(&self, r: usize) -> Option<Grid> {
        let e = &self.tree.elements[r];
        let Kind::ScrollingRegion(region) = &e.kind else {
            return None;
        };
        let (rw, rh) = e.placed.map(|p| (p.dx, p.dy))?;
        let template = self.tree.find(region.template)?;
        let (w, h) = self.tree.elements[template].placed.map(|p| (p.dx, p.dy))?;
        let (sx, sy) = (region.spacing.0.max(0), region.spacing.1.max(0));
        let step = (w.saturating_add(sx).max(1), h.saturating_add(sy).max(1));
        let cols = ((rw.saturating_add(sx) / step.0).max(1)) as usize;
        let fit = ((rh.saturating_add(sy) / step.1).max(1)) as usize;
        let used = (step.0 * cols as i32 - sx).min(rw);
        let high = (step.1 * fit as i32 - sy).min(rh);
        Some(Grid {
            cols,
            fit,
            size: (w, h),
            step,
            left: (rw - used).max(0) / 2,
            up: (rh - high).max(0) / 2,
        })
    }

    /// The most the first row can be and how many rows show, for the scroll bar.
    pub(super) fn region_extent(&self, r: usize) -> Option<(usize, usize)> {
        let grid = self.grid(r)?;
        let Kind::ScrollingRegion(region) = &self.tree.elements[r].kind else {
            return None;
        };
        let rows = region.items.div_ceil(grid.cols);
        Some((rows.saturating_sub(grid.fit), grid.fit))
    }

    /// Puts the buttons of region `r` in the rows that show and hides the others.
    pub(super) fn place_region(&mut self, r: usize) {
        let Some(grid) = self.grid(r) else { return };
        let Kind::ScrollingRegion(region) = &self.tree.elements[r].kind else {
            return;
        };
        let (copies, items, top) = (region.copies.clone(), region.items, region.top);
        for (k, c) in copies.into_iter().enumerate() {
            let row = k / grid.cols;
            let shown = k < items && (top..top + grid.fit).contains(&row);
            let at = (
                grid.left + (k % grid.cols) as i32 * grid.step.0,
                grid.up + row.wrapping_sub(top) as i32 * grid.step.1,
            );
            let e = &mut self.tree.elements[c];
            if shown {
                e.state &= !(HIDDEN | NEVER_SHOWN);
                (e.x, e.y) = (Pos::Num(at.0), Pos::Num(at.1));
                if let Some(placed) = &mut e.placed {
                    (placed.x, placed.y) = at;
                    (placed.dx, placed.dy) = grid.size;
                }
            } else {
                e.state |= HIDDEN | NEVER_SHOWN;
            }
        }
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests;

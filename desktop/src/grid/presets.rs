//! The saved column sets and group and sort presets listed in View >
//! Columns and View > Group and Sort by, beside Customize. They come from
//! the layout list (the standard layout and every saved layout): each
//! distinct column set or grouping is listed once, under the name of the
//! first layout that holds it. Choosing one puts that part of the layout in
//! the open layout and leaves the rest of it alone.
use super::layout::{ColumnSetting, GroupSort, Layout};
use super::*;

/// Most presets either menu lists.
pub const MAX_PRESETS: usize = 15;
/// Longest name shown in the menus.
const SHOWN_NAME: usize = 40;

/// Text safe in a menu item: no mnemonic marker, not too long.
pub fn menu_text(name: &str) -> String {
    let shown: String = name.chars().take(SHOWN_NAME).collect();
    let cut = if name.chars().count() > SHOWN_NAME {
        "..."
    } else {
        ""
    };
    format!("{}{cut}", shown.replace('&', "&&"))
}

/// The layouts in list order: the standard layout when no saved layout
/// replaces it, then the saved ones by name.
fn layouts(app: &App) -> Vec<Layout> {
    let mut all = vec![];
    if !app
        .grid
        .library
        .iter()
        .any(|l| l.name == layout::STANDARD_NAME)
    {
        all.push(Layout::default());
    }
    let mut saved = app.grid.library.clone();
    saved.sort_by_key(|l| l.name.to_lowercase());
    all.extend(saved);
    all
}

impl App {
    /// The distinct column sets of the layout list, named by their layout.
    pub(crate) fn column_presets(&self) -> Vec<(String, Vec<ColumnSetting>)> {
        let mut out: Vec<(String, Vec<ColumnSetting>)> = vec![];
        for l in layouts(self) {
            if out.len() < MAX_PRESETS && !out.iter().any(|(_, c)| *c == l.columns) {
                out.push((l.name, l.columns));
            }
        }
        out
    }

    /// The distinct groupings (with their sorts) of the layout list.
    pub(crate) fn group_presets(&self) -> Vec<(String, GroupSort)> {
        let mut out: Vec<(String, GroupSort)> = vec![];
        for l in layouts(self) {
            if out.len() < MAX_PRESETS && !out.iter().any(|(_, g)| *g == l.group_sort) {
                out.push((l.name, l.group_sort));
            }
        }
        out
    }

    /// Puts column set `index` of the list in the open layout.
    pub(crate) fn apply_column_preset(&mut self, index: usize) {
        let Some((name, columns)) = self.column_presets().into_iter().nth(index) else {
            return;
        };
        if self.grid.layout.columns != columns {
            self.grid.layout.columns = columns;
            self.drop_frame_cache();
        }
        self.status = format!("Columns of {name}");
    }

    /// Puts grouping `index` of the list in the open layout.
    pub(crate) fn apply_group_preset(&mut self, index: usize) {
        let Some((name, group_sort)) = self.group_presets().into_iter().nth(index) else {
            return;
        };
        if self.grid.layout.group_sort != group_sort {
            self.grid.layout.group_sort = group_sort;
            self.collapsed.clear();
            self.drop_frame_cache();
        }
        self.status = format!("Group and sort of {name}");
    }
}

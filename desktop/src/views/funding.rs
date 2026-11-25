//! The Funding tab of the Projects view: the open project's `PROJFUND` rows
//! (funding source, amount and fund share), added, deleted and edited in
//! place. Every change goes through `openxer_xer::edit::funding`, the same
//! functions the command line and MCP use, and is one undo step.
use super::*;
use crate::schedule_dialogs::Canvas;
use openxer_xer::edit::funding::{self, Field};

const COLUMNS: [(&str, f32, bool); 3] = [
    ("Funding Source", 200., false),
    ("Amount", 100., true),
    ("Fund Share", 100., true),
];

/// A share as the tab shows it: `40.00%`.
pub fn share_text(v: f64) -> String {
    format!("{v:.2}%")
}

/// The project's funding as the tab lists it: by Funding Source, as the
/// reference sorts the tab. The order is read from the stored rows, so a
/// row moves only when a change is committed, never while a cell is being
/// typed in.
fn listed(p: &ProjectData) -> Vec<funding::Funding> {
    let mut list = funding::list(p);
    list.sort_by_cached_key(|f| (f.source.to_lowercase(), f.row));
    list
}

impl App {
    /// The tab. `editable` is false for another project or an EPS row: the
    /// tab then lists nothing and its buttons are off.
    pub(crate) fn project_funding_tab(&mut self, c: &mut Canvas, w: f32, h: f32, editable: bool) {
        let list = if editable {
            listed(&self.project)
        } else {
            vec![]
        };
        let rows: Vec<Vec<String>> = list
            .iter()
            .map(|f| {
                vec![
                    f.source.clone(),
                    format::cost(f.amount),
                    share_text(f.share),
                ]
            })
            .collect();
        let selected = self
            .views
            .projects
            .funding_row
            .filter(|r| *r < rows.len())
            .or((!rows.is_empty()).then_some(0));
        let (x, y, tw, th) = (12., 12., w - 24., h - 44.);
        if let Some(i) = sheet::table(
            c,
            "project_funding",
            x,
            y,
            tw,
            th,
            &COLUMNS,
            &rows,
            selected,
        ) {
            self.views.projects.funding_row = Some(i);
        }
        if rows.is_empty() {
            let text = if editable {
                "No funding sources are assigned to this project. Add assigns one from Enterprise > Funding Sources."
            } else {
                "Open the project to see and edit its funding."
            };
            c.label(x + 12., y + 28., 18., text, false);
        }
        let sources = funding::sources(&self.project.document);
        let mut edit: Option<(usize, Field, String)> = None;
        let top = selected.and_then(|i| sheet::row_top(c, "project_funding", y, th, i));
        if let (Some(i), Some(top)) = (selected, top) {
            let mut cx = x + 2.;
            for (column, (_, width, right)) in COLUMNS.iter().enumerate() {
                let key = format!("project_funding_cell_{column}");
                if column == 0 {
                    // The source picks from the dictionary; a source another
                    // row already lists is shown but cannot be picked.
                    let items: Vec<(&str, bool)> = sources
                        .iter()
                        .map(|(id, name)| {
                            let free =
                                *id == list[i].fund_id || !list.iter().any(|f| f.fund_id == *id);
                            (name.as_str(), free)
                        })
                        .collect();
                    let mut index = sources
                        .iter()
                        .position(|(id, _)| *id == list[i].fund_id)
                        .unwrap_or(usize::MAX);
                    if c.combo(&key, cx, top, *width, 18., &items, &mut index, true) {
                        if let Some((_, name)) = sources.get(index) {
                            edit = Some((list[i].row, Field::Source, name.clone()));
                        }
                    }
                } else {
                    let align = if *right { Align::Max } else { Align::Min };
                    if let Some(text) = sheet::commit_cell(
                        c,
                        &key,
                        (cx, top, *width, 18.),
                        &rows[i][column],
                        true,
                        align,
                    ) {
                        let field = if column == 1 {
                            Field::Amount
                        } else {
                            Field::Share
                        };
                        edit = Some((list[i].row, field, text));
                    }
                }
                cx += width;
            }
        }
        let clicked = sheet::buttons(
            c,
            "project_funding_buttons",
            11.,
            h - 26.,
            &[
                ("Add", editable),
                ("Delete", editable && selected.is_some()),
            ],
        );
        if let Some((row, field, text)) = edit {
            match funding::set(&mut self.project, row, field, &text) {
                Ok(()) => {
                    // A new source can move the row: the selection follows it.
                    self.views.projects.funding_row =
                        listed(&self.project).iter().position(|f| f.row == row);
                    self.touch_records();
                }
                Err(e) => self.status = e,
            }
        }
        match clicked {
            Some(0) => self.add_project_funding(),
            Some(1) => {
                if let Some(i) = selected {
                    match funding::delete(&mut self.project, list[i].row) {
                        Ok(()) => {
                            self.views.projects.funding_row = Some(i.saturating_sub(1));
                            self.touch_records();
                        }
                        Err(e) => self.status = e,
                    }
                }
            }
            _ => {}
        }
    }

    /// Add: funding from the first source the project does not list yet.
    /// With no funding sources defined, Add opens Enterprise > Funding
    /// Sources to define one.
    pub(crate) fn add_project_funding(&mut self) {
        let sources = funding::sources(&self.project.document);
        if sources.is_empty() {
            self.status = "Define a funding source first, then Add assigns it.".into();
            self.open_enterprise(crate::enterprise::Dialog::Funding);
            return;
        }
        let taken = funding::list(&self.project);
        let Some((id, _)) = sources
            .iter()
            .find(|(id, _)| !taken.iter().any(|f| f.fund_id == *id))
        else {
            self.status = "Every funding source is assigned to this project already.".into();
            return;
        };
        match funding::add(&mut self.project, *id) {
            Ok(row) => {
                self.views.projects.funding_row =
                    listed(&self.project).iter().position(|f| f.row == row);
                self.touch_records();
            }
            Err(e) => self.status = e,
        }
    }
}

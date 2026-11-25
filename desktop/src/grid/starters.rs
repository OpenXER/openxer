//! OpenXER's own starter filters and layouts, written for this project and
//! labeled as samples. A fresh workspace gets them; a workspace that already
//! has its own filters and layouts gets them once, beside the user's items.
//! A sample is never put over an item of the same name, and one the user
//! deletes stays deleted.
use super::filters::{Criterion, FilterSelection, Op, UserFilter};
use super::layout::{ColumnSetting, GroupLevel, GroupSort, Layout, SortKey};
use super::*;
use crate::print::setup::{self, Orientation, PageSetup};

/// Preference listing the starters this workspace has already been given.
pub const LOADED_KEY: &str = "starter_content_v1";
pub const THREE_WEEK_LOOKAHEAD: &str = "Three Week Lookahead (OpenXER sample)";
pub const NEAR_CRITICAL: &str = "Near Critical (OpenXER sample)";
pub const STARTED_LAST_PERIOD: &str = "Started Last Period (OpenXER sample)";
pub const LOOKAHEAD_LAYOUT: &str = "Lookahead (OpenXER sample)";
pub const WBS_SUMMARY_LAYOUT: &str = "WBS Summary (OpenXER sample)";

fn row(column: &str, op: Op, value: &str, high: &str) -> Criterion {
    Criterion {
        column: column.into(),
        op,
        value: value.into(),
        high: high.into(),
        level: 0,
    }
}

fn not_complete() -> Criterion {
    row("activity_status", Op::NotEqual, "Completed", "")
}

/// The starter filters, each with the stable id the loaded list keeps.
pub fn filters() -> Vec<(&'static str, UserFilter)> {
    vec![
        (
            "filter-three-week-lookahead",
            UserFilter {
                name: THREE_WEEK_LOOKAHEAD.into(),
                match_all: true,
                // Not complete, and it overlaps the data date through three
                // weeks after it. The dates move with the data date.
                rows: vec![
                    not_complete(),
                    row("start", Op::LessOrEqual, "DD + 3w", ""),
                    row("finish", Op::GreaterOrEqual, "DD", ""),
                ],
            },
        ),
        (
            "filter-near-critical",
            UserFilter {
                name: NEAR_CRITICAL.into(),
                match_all: true,
                rows: vec![not_complete(), row("total_float", Op::Less, "10d", "")],
            },
        ),
        (
            "filter-started-last-period",
            UserFilter {
                name: STARTED_LAST_PERIOD.into(),
                match_all: true,
                rows: vec![row("actual_start", Op::Within, "DD - 1w", "DD")],
            },
        ),
    ]
}

fn columns(list: &[(&str, f32)]) -> Vec<ColumnSetting> {
    list.iter()
        .map(|(id, width)| ColumnSetting::new(id, *width))
        .collect()
}

/// The starter layouts and the Page Setup each one is given.
pub fn layouts() -> Vec<(&'static str, Layout, Option<PageSetup>)> {
    let lookahead = Layout {
        name: LOOKAHEAD_LAYOUT.into(),
        columns: columns(&[
            ("activity_id", 90.),
            ("activity_name", 250.),
            ("activity_status", 85.),
            ("remaining_duration", 70.),
            ("start", 100.),
            ("finish", 100.),
            ("total_float", 65.),
        ]),
        filter: FilterSelection {
            all_activities: false,
            selected: vec![format!("user:{THREE_WEEK_LOOKAHEAD}")],
            match_all: true,
            highlight: false,
        },
        group_sort: GroupSort {
            levels: vec![GroupLevel {
                column: "wbs".into(),
                to_level: None,
                indent: true,
                interval: String::new(),
            }],
            sort: vec![
                SortKey {
                    column: "start".into(),
                    descending: false,
                },
                SortKey {
                    column: "activity_id".into(),
                    descending: false,
                },
            ],
            ..GroupSort::default()
        },
        ..Layout::default()
    };
    // The printout runs from a week before the data date to three weeks
    // after it, whatever the screen shows.
    let mut page = PageSetup {
        orientation: Orientation::Landscape,
        ..PageSetup::default()
    };
    page.options.same_as_screen = false;
    page.options.timescale_start = "DD-1W".into();
    page.options.timescale_finish = "DD+3W".into();
    let mut wbs_title = ColumnSetting::new("activity_id", 300.);
    wbs_title.title = Some("WBS".into());
    let summary = Layout {
        name: WBS_SUMMARY_LAYOUT.into(),
        columns: std::iter::once(wbs_title)
            .chain(columns(&[
                ("start", 100.),
                ("finish", 100.),
                ("original_duration", 80.),
                ("remaining_duration", 80.),
                ("budgeted_total_cost", 100.),
                ("actual_total_cost", 100.),
            ]))
            .collect(),
        group_sort: GroupSort {
            show_summaries_only: true,
            show_group_totals: true,
            show_grand_totals: true,
            ..GroupSort::default()
        },
        ..Layout::default()
    };
    vec![
        ("layout-lookahead", lookahead, Some(page)),
        ("layout-wbs-summary", summary, None),
    ]
}

impl App {
    /// Gives the workspace the starter filters and layouts it has not had
    /// yet. Items the user already has under the same name are left alone.
    pub(crate) fn install_starter_content(&mut self) {
        let mut loaded: Vec<String> = self
            .store
            .preference(LOADED_KEY)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let (mut filters_changed, mut library_changed, mut any) = (false, false, false);
        for (id, filter) in filters() {
            if loaded.iter().any(|l| l == id) {
                continue;
            }
            loaded.push(id.to_string());
            any = true;
            if !self
                .grid
                .user_filters
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&filter.name))
            {
                self.grid.user_filters.push(filter);
                filters_changed = true;
            }
        }
        for (id, layout, page) in layouts() {
            if loaded.iter().any(|l| l == id) {
                continue;
            }
            loaded.push(id.to_string());
            any = true;
            let taken = self
                .grid
                .library
                .iter()
                .any(|l| l.name.eq_ignore_ascii_case(&layout.name))
                || layout.name.eq_ignore_ascii_case(layout::STANDARD_NAME);
            if taken {
                continue;
            }
            if let Some(page) = page {
                let key = setup::slot_key(setup::PREF_KEY, &layout.name);
                if self.store.preference(&key).ok().flatten().is_none() {
                    if let Ok(text) = serde_json::to_string(&page) {
                        let _ = self.store.set_preference(&key, &text);
                    }
                }
            }
            self.grid.library.push(layout);
            library_changed = true;
        }
        if filters_changed {
            self.save_user_filters();
        }
        if library_changed {
            self.save_library();
        }
        if any {
            if let Ok(text) = serde_json::to_string(&loaded) {
                let _ = self.store.set_preference(LOADED_KEY, &text);
            }
        }
    }
}

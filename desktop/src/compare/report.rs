//! The comparison as a report and as the flat rows of the results window.
//! The report leads with what a superintendent asks first (did the finish
//! move, what was added or dropped, what slipped most), then the detail a
//! project controls lead checks field by field.
use super::diff::{link_text, link_type_caption, Area, Comparison, Kind2};
use crate::reports::doc::{self, Cell, Column, Report, Row, Section, Table, Tone};

fn tone(kind: Kind2) -> Tone {
    match kind {
        Kind2::Added => Tone::Added,
        Kind2::Deleted => Tone::Deleted,
        Kind2::Changed => Tone::Changed,
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn text(s: impl Into<String>) -> Cell {
    Cell::Text(s.into())
}

/// One line per change, for the results window and its filters.
#[derive(Clone, Debug, PartialEq)]
pub struct ResultRow {
    pub kind: Kind2,
    pub area: Area,
    pub code: String,
    pub name: String,
    pub field: String,
    pub old: String,
    pub new: String,
    pub variance: String,
    /// Activity ID to select in the open project.
    pub jump: Option<String>,
}

pub fn rows(c: &Comparison) -> Vec<ResultRow> {
    let mut out = vec![];
    for f in &c.project {
        out.push(ResultRow {
            kind: Kind2::Changed,
            area: Area::Project,
            code: String::new(),
            name: "Project".into(),
            field: f.field.clone(),
            old: f.old.text().into(),
            new: f.new.text().into(),
            variance: f.variance_text.clone(),
            jump: None,
        });
    }
    for w in &c.wbs {
        out.push(ResultRow {
            kind: w.kind,
            area: Area::Wbs,
            code: w.code.clone(),
            name: if w.name.is_empty() {
                w.old_name.clone()
            } else {
                w.name.clone()
            },
            field: if w.kind == Kind2::Changed {
                "WBS Name".into()
            } else {
                "WBS".into()
            },
            old: w.old_name.clone(),
            new: w.name.clone(),
            variance: String::new(),
            jump: None,
        });
    }
    for a in &c.activities {
        let jump = Some(a.code.clone());
        if a.kind == Kind2::Changed {
            for f in &a.changes {
                out.push(ResultRow {
                    kind: Kind2::Changed,
                    area: f.area,
                    code: a.code.clone(),
                    name: a.name.clone(),
                    field: f.field.clone(),
                    old: f.old.text().into(),
                    new: f.new.text().into(),
                    variance: f.variance_text.clone(),
                    jump: jump.clone(),
                });
            }
        } else {
            let summary = a
                .facts
                .iter()
                .map(|c| c.text().to_string())
                .collect::<Vec<_>>();
            let summary = format!(
                "{} | {} to {}",
                summary.first().cloned().unwrap_or_default(),
                summary.get(1).cloned().unwrap_or_default(),
                summary.get(2).cloned().unwrap_or_default()
            );
            let (old, new) = if a.kind == Kind2::Added {
                (String::new(), summary)
            } else {
                (summary, String::new())
            };
            out.push(ResultRow {
                kind: a.kind,
                area: Area::Activities,
                code: a.code.clone(),
                name: a.name.clone(),
                field: "Activity".into(),
                old,
                new,
                variance: String::new(),
                jump: if a.kind == Kind2::Deleted { None } else { jump },
            });
        }
    }
    for l in &c.links {
        out.push(ResultRow {
            kind: l.kind,
            area: Area::Relationships,
            code: format!("{} -> {}", l.pred, l.succ),
            name: format!("{} -> {}", l.pred_name, l.succ_name),
            field: "Relationship".into(),
            old: l.old.as_ref().map(link_text).unwrap_or_default(),
            new: l.new.as_ref().map(link_text).unwrap_or_default(),
            variance: String::new(),
            jump: Some(l.succ.clone()),
        });
    }
    out
}

fn summary_section(c: &Comparison) -> Section {
    let mut paragraphs = vec![];
    let finish = match c.finish_variance {
        Some(v) if v.abs() > 1e-9 => format!(
            "Project finish moved from {} to {} ({}, calendar days).",
            c.finish.0.text(),
            c.finish.1.text(),
            super::diff::signed_days(v)
        ),
        Some(_) => format!("Project finish did not move ({}).", c.finish.1.text()),
        None => "Project finish is not available on both sides (not scheduled).".into(),
    };
    paragraphs.push(finish);
    if c.data_date.0.text() != c.data_date.1.text() {
        paragraphs.push(format!(
            "Data date moved from {} to {}.",
            c.data_date.0.text(),
            c.data_date.1.text()
        ));
    }
    paragraphs.push(format!(
        "Activities: {} added, {} deleted, {} changed. Relationships: {} added, {} deleted, {} changed.",
        c.count(Kind2::Added),
        c.count(Kind2::Deleted),
        c.count(Kind2::Changed),
        c.link_count(Kind2::Added),
        c.link_count(Kind2::Deleted),
        c.link_count(Kind2::Changed),
    ));
    let n = |v: usize| Cell::number(v as f64, v.to_string());
    let delta = |a: usize, b: usize| {
        let d = b as i64 - a as i64;
        Cell::number(
            d as f64,
            if d > 0 {
                format!("+{d}")
            } else {
                d.to_string()
            },
        )
    };
    let mut rows = vec![
        Row::new(
            vec![
                text("Data Date"),
                c.data_date.0.clone(),
                c.data_date.1.clone(),
                text(""),
            ],
            Tone::Normal,
        ),
        Row::new(
            vec![
                text("Project Finish"),
                c.finish.0.clone(),
                c.finish.1.clone(),
                text(
                    c.finish_variance
                        .map_or(String::new(), super::diff::signed_days),
                ),
            ],
            match c.finish_variance {
                Some(v) if v > 1e-9 => Tone::Bad,
                Some(v) if v < -1e-9 => Tone::Good,
                _ => Tone::Normal,
            },
        ),
        Row::new(
            vec![
                text("Activities"),
                n(c.old_activities),
                n(c.new_activities),
                delta(c.old_activities, c.new_activities),
            ],
            Tone::Normal,
        ),
        Row::new(
            vec![
                text("Relationships"),
                n(c.old_links),
                n(c.new_links),
                delta(c.old_links, c.new_links),
            ],
            Tone::Normal,
        ),
    ];
    for area in Area::ACTIVITY_FIELDS {
        if c.options.has(area) {
            let changed = c.changes_in(area);
            rows.push(Row::new(
                vec![
                    text(format!("Changes: {}", area.short())),
                    text(""),
                    n(changed),
                    text(""),
                ],
                Tone::Normal,
            ));
        }
    }
    Section {
        heading: "Summary".into(),
        paragraphs,
        tables: vec![Table {
            name: "Key Figures".into(),
            caption: "Key figures".into(),
            columns: vec![
                Column::left("Item", 30.),
                Column::right("Original", 18.),
                Column::right("Revised", 18.),
                Column::right("Variance", 12.),
            ],
            rows,
        }],
    }
}

fn movements(c: &Comparison) -> Option<Section> {
    let mut moved: Vec<(f64, &super::diff::ActivityDiff, &super::diff::FieldChange)> = c
        .activities
        .iter()
        .filter(|a| a.kind == Kind2::Changed)
        .filter_map(|a| {
            a.changes
                .iter()
                .find(|f| f.area == Area::Dates && f.field == "Finish")
                .and_then(|f| f.variance.map(|v| (v, a, f)))
        })
        .filter(|(v, ..)| v.abs() > 1e-9)
        .collect();
    if moved.is_empty() {
        return None;
    }
    moved.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.code.cmp(&y.1.code)));
    let slips: Vec<_> = moved.iter().filter(|m| m.0 > 0.).take(15).collect();
    let gains: Vec<_> = moved.iter().rev().filter(|m| m.0 < 0.).take(15).collect();
    let table = |name: &str,
                 caption: &str,
                 list: &[&(f64, &super::diff::ActivityDiff, &super::diff::FieldChange)],
                 tone| Table {
        name: name.into(),
        caption: caption.into(),
        columns: vec![
            Column::left("Activity ID", 12.),
            Column::left("Activity Name", 36.),
            Column::right("Original Finish", 16.),
            Column::right("Revised Finish", 16.),
            Column::right("Variance", 10.),
        ],
        rows: list
            .iter()
            .map(|(_, a, f)| {
                Row::new(
                    vec![
                        text(a.code.clone()),
                        text(a.name.clone()),
                        f.old.clone(),
                        f.new.clone(),
                        Cell::number(f.variance.unwrap_or(0.), f.variance_text.clone()),
                    ],
                    tone,
                )
            })
            .collect(),
    };
    let mut tables = vec![];
    if !slips.is_empty() {
        tables.push(table(
            "Finish Slips",
            "Activities finishing later (largest first)",
            &slips,
            Tone::Bad,
        ));
    }
    if !gains.is_empty() {
        tables.push(table(
            "Finish Gains",
            "Activities finishing earlier (largest first)",
            &gains,
            Tone::Good,
        ));
    }
    Some(Section {
        heading: "Largest Finish Date Movements".into(),
        paragraphs: vec![
            "Variance is revised minus original, in calendar days. Positive numbers are slips."
                .into(),
        ],
        tables,
    })
}

fn added_deleted(c: &Comparison) -> Option<Section> {
    let mut tables = vec![];
    for (kind, name, caption) in [
        (
            Kind2::Added,
            "Added Activities",
            "The following Activities have been added to the project",
        ),
        (
            Kind2::Deleted,
            "Deleted Activities",
            "The following Activities have been deleted from the project",
        ),
    ] {
        let rows: Vec<Row> = c
            .activities
            .iter()
            .filter(|a| a.kind == kind)
            .map(|a| {
                let mut cells = vec![
                    text(a.code.clone()),
                    text(a.name.clone()),
                    text(a.wbs.clone()),
                ];
                cells.extend(a.facts.iter().cloned());
                Row::new(cells, tone(kind))
            })
            .collect();
        if !rows.is_empty() {
            tables.push(Table {
                name: name.into(),
                caption: caption.into(),
                columns: vec![
                    Column::left("Activity ID", 12.),
                    Column::left("Activity Name", 36.),
                    Column::left("WBS", 14.),
                    Column::right("Original Duration", 10.),
                    Column::right("Start", 16.),
                    Column::right("Finish", 16.),
                ],
                rows,
            });
        }
    }
    (!tables.is_empty()).then(|| Section {
        heading: "Activities Added/Deleted".into(),
        paragraphs: vec![],
        tables,
    })
}

fn changes(c: &Comparison) -> Option<Section> {
    let changed: Vec<_> = c
        .activities
        .iter()
        .filter(|a| a.kind == Kind2::Changed)
        .collect();
    if changed.is_empty() {
        return None;
    }
    let mut tables = vec![];
    if c.options.group_by_activity {
        let mut rows = vec![];
        for a in &changed {
            rows.push(Row::band(
                3,
                format!("{}  {}   (WBS {})", a.code, a.name, a.wbs),
                5,
            ));
            for f in &a.changes {
                rows.push(Row::new(
                    vec![
                        text(f.field.clone()),
                        text(f.area.short()),
                        f.old.clone(),
                        f.new.clone(),
                        f.variance
                            .map_or(text(""), |v| Cell::number(v, f.variance_text.clone())),
                    ],
                    Tone::Changed,
                ));
            }
        }
        tables.push(Table {
            name: "Activity Changes".into(),
            caption: format!(
                "{} with changes, grouped by activity",
                plural(changed.len(), "activity", "activities")
            ),
            columns: vec![
                Column::left("Field Name", 24.),
                Column::left("Area", 14.),
                Column::left("Original", 24.),
                Column::left("Revised", 24.),
                Column::right("Variance", 10.),
            ],
            rows,
        });
    } else {
        for area in Area::ACTIVITY_FIELDS {
            let rows: Vec<Row> = changed
                .iter()
                .flat_map(|a| {
                    a.changes.iter().filter(|f| f.area == area).map(move |f| {
                        Row::new(
                            vec![
                                text(a.code.clone()),
                                text(a.name.clone()),
                                text(f.field.clone()),
                                f.old.clone(),
                                f.new.clone(),
                                f.variance
                                    .map_or(text(""), |v| Cell::number(v, f.variance_text.clone())),
                            ],
                            Tone::Changed,
                        )
                    })
                })
                .collect();
            if rows.is_empty() {
                continue;
            }
            tables.push(Table {
                name: area.short().into(),
                caption: area.caption().into(),
                columns: vec![
                    Column::left("Activity ID", 12.),
                    Column::left("Activity Name", 30.),
                    Column::left("Field Name", 20.),
                    Column::left("Original", 20.),
                    Column::left("Revised", 20.),
                    Column::right("Variance", 10.),
                ],
                rows,
            });
        }
    }
    Some(Section {
        heading: "Activity Changes".into(),
        paragraphs: vec![
            "Original is the first project or baseline, Revised the second. Dates vary in calendar days, durations and float in work days."
                .into(),
        ],
        tables,
    })
}

fn relationships(c: &Comparison) -> Option<Section> {
    if c.links.is_empty() {
        return None;
    }
    let mut tables = vec![];
    for (kind, name, caption) in [
        (
            Kind2::Added,
            "Added Relationships",
            "The following Relationships have been added to the project",
        ),
        (
            Kind2::Deleted,
            "Deleted Relationships",
            "The following Relationships have been deleted from the project",
        ),
    ] {
        let rows: Vec<Row> = c
            .links
            .iter()
            .filter(|l| l.kind == kind)
            .map(|l| {
                let side = l.new.as_ref().or(l.old.as_ref()).unwrap();
                Row::new(
                    vec![
                        text(l.pred.clone()),
                        text(l.pred_name.clone()),
                        text(l.succ.clone()),
                        text(l.succ_name.clone()),
                        text(link_type_caption(side.link_type)),
                        text(side.lag_text.clone()),
                    ],
                    tone(kind),
                )
            })
            .collect();
        if !rows.is_empty() {
            tables.push(Table {
                name: name.into(),
                caption: caption.into(),
                columns: vec![
                    Column::left("Predecessor Activity Id", 12.),
                    Column::left("Predecessor Activity Name", 28.),
                    Column::left("Successor Activity Id", 12.),
                    Column::left("Successor Activity Name", 28.),
                    Column::left("Relationship Type", 14.),
                    Column::right("Relationship Lag", 9.),
                ],
                rows,
            });
        }
    }
    let rows: Vec<Row> = c
        .links
        .iter()
        .filter(|l| l.kind == Kind2::Changed)
        .map(|l| {
            Row::new(
                vec![
                    text(l.pred.clone()),
                    text(l.pred_name.clone()),
                    text(l.succ.clone()),
                    text(l.succ_name.clone()),
                    text(l.old.as_ref().map(link_text).unwrap_or_default()),
                    text(l.new.as_ref().map(link_text).unwrap_or_default()),
                ],
                Tone::Changed,
            )
        })
        .collect();
    if !rows.is_empty() {
        tables.push(Table {
            name: "Changed Relationships".into(),
            caption: "Relationships whose type or lag changed".into(),
            columns: vec![
                Column::left("Predecessor Activity Id", 12.),
                Column::left("Predecessor Activity Name", 28.),
                Column::left("Successor Activity Id", 12.),
                Column::left("Successor Activity Name", 28.),
                Column::left("Original", 11.),
                Column::left("Revised", 11.),
            ],
            rows,
        });
    }
    Some(Section {
        heading: "Relationships Added/Deleted".into(),
        paragraphs: vec![],
        tables,
    })
}

fn wbs(c: &Comparison) -> Option<Section> {
    if c.wbs.is_empty() {
        return None;
    }
    Some(Section {
        heading: "WBS Added/Deleted".into(),
        paragraphs: vec![],
        tables: vec![Table {
            name: "WBS".into(),
            caption: String::new(),
            columns: vec![
                Column::left("WBS Code", 20.),
                Column::left("Change", 10.),
                Column::left("Original Name", 30.),
                Column::left("Revised Name", 30.),
            ],
            rows: c
                .wbs
                .iter()
                .map(|w| {
                    Row::new(
                        vec![
                            text(w.code.clone()),
                            text(w.kind.caption()),
                            text(w.old_name.clone()),
                            text(w.name.clone()),
                        ],
                        tone(w.kind),
                    )
                })
                .collect(),
        }],
    })
}

fn project(c: &Comparison) -> Option<Section> {
    if c.project.is_empty() {
        return None;
    }
    Some(Section {
        heading: "Project Information".into(),
        paragraphs: vec![],
        tables: vec![Table {
            name: "Project".into(),
            caption: "General Project Data".into(),
            columns: vec![
                Column::left("Field Name", 34.),
                Column::left("Original", 24.),
                Column::left("Revised", 24.),
                Column::right("Variance", 10.),
            ],
            rows: c
                .project
                .iter()
                .map(|f| {
                    Row::new(
                        vec![
                            text(f.field.clone()),
                            f.old.clone(),
                            f.new.clone(),
                            f.variance
                                .map_or(text(""), |v| Cell::number(v, f.variance_text.clone())),
                        ],
                        Tone::Changed,
                    )
                })
                .collect(),
        }],
    })
}

/// Areas that were compared and found no difference, and areas left out.
fn quiet(c: &Comparison) -> Section {
    let mut none = vec![];
    for area in Area::ALL {
        if !c.options.has(area) {
            continue;
        }
        let empty = match area {
            Area::Project => c.project.is_empty(),
            Area::Wbs => c.wbs.is_empty(),
            Area::Activities => c.count(Kind2::Added) + c.count(Kind2::Deleted) == 0,
            Area::Relationships => c.links.is_empty(),
            a => c.changes_in(a) == 0,
        };
        if empty {
            none.push(area.caption());
        }
    }
    let skipped: Vec<&str> = Area::ALL
        .iter()
        .filter(|a| !c.options.has(**a))
        .map(|a| a.caption())
        .collect();
    let mut paragraphs = vec![if none.is_empty() {
        "Every compared section has changes.".to_string()
    } else {
        none.join("; ")
    }];
    if !skipped.is_empty() {
        paragraphs.push(format!("Not compared: {}.", skipped.join("; ")));
    }
    Section {
        heading: "No changes have been reported in the following sections".into(),
        paragraphs,
        tables: vec![],
    }
}

pub fn report(c: &Comparison) -> Report {
    let mut sections = vec![summary_section(c)];
    sections.extend(movements(c));
    sections.extend(added_deleted(c));
    sections.extend(changes(c));
    sections.extend(relationships(c));
    sections.extend(wbs(c));
    sections.extend(project(c));
    sections.push(quiet(c));
    Report {
        title: "Schedule Comparison".into(),
        header: vec![
            "Project Differences Report".into(),
            format!("Comparing {} to {}", c.old_label, c.new_label),
            format!("Executed by {}", doc::user_name()),
            format!("Date {}", doc::run_stamp()),
        ],
        sections,
        footer: format!("Original: {}. Revised: {}.", c.old_label, c.new_label),
    }
}

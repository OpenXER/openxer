//! Totals a scheduler checks in the usage panes: the values of the periods
//! on show plus what falls before and after them add up to each stored
//! quantity, and Cumulative is the running sum from the true start of the
//! data, whatever is scrolled into view.
use super::usage;
use super::*;
use crate::qa_sweep::Driver;
use crate::views::resources as rs;

/// Day periods from two days before the earliest date to two days after
/// the latest one.
fn schedule_days(p: &ProjectData) -> Vec<(i64, i64)> {
    let m = &p.model;
    let first = m
        .activities
        .iter()
        .filter_map(|a| a.act_start.or(a.target_start).or(a.early_start))
        .chain(m.project.data_date)
        .min()
        .unwrap()
        .div_euclid(DAY)
        * DAY
        - 2 * DAY;
    let last = m
        .activities
        .iter()
        .filter_map(|a| a.late_finish.or(a.early_finish).or(a.target_finish))
        .max()
        .unwrap()
        + 2 * DAY;
    let count = (last - first) / DAY + 1;
    (0..count)
        .map(|d| (first + d * DAY, first + (d + 1) * DAY))
        .collect()
}

/// The resource of `with_resource` (24 h on one activity) selected on the
/// Resource Usage Spreadsheet.
fn resource_sheet(name: &str) -> Driver {
    let mut d = Driver::new(name, rs::tests::with_resource());
    d.home();
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.app.panes.usage.resource = Some(7);
    d.frames(3);
    d
}

fn sheet_fields(d: &Driver) -> Vec<usage_fields::Field> {
    usage_fields::fields_of(
        &d.app.project,
        usage_fields::Which::Assignment,
        &d.app.panes.usage.assignment_fields,
    )
}

/// The first period with remaining units, and the number of periods that
/// hold some.
fn busy(values: &[f64]) -> (usize, usize) {
    let first = values.iter().position(|v| *v > 1e-9).expect("some units");
    (first, values.iter().filter(|v| **v > 1e-9).count())
}

#[test]
fn resource_sheet_cumulative_starts_from_the_data_not_the_view() {
    let mut d = resource_sheet("totals-resource-sheet");
    d.app.panes.usage.cumulative = true;
    let all = schedule_days(&d.app.project);
    let afields = sheet_fields(&d);
    let whole = d.app.selection_sheet_rows(&afields, &all).unwrap();
    let (first, count) = busy(&whole[0].values);
    assert!(count > 1, "spread over more than one day");
    // Scroll so the view starts on the second busy day.
    let cut = first + 1;
    let later = d.app.selection_sheet_rows(&afields, &all[cut..]).unwrap();
    let display = d.app.panes.usage.clone();
    for (a, b) in whole.iter().zip(later.iter()) {
        let from_start = display.shown(a.before, &a.values);
        let scrolled = display.shown(b.before, &b.values);
        for k in 0..scrolled.len() {
            assert!(
                (from_start[cut + k] - scrolled[k]).abs() < 1e-9,
                "period {k} after the scroll: {} from the start, {} scrolled",
                from_start[cut + k],
                scrolled[k]
            );
        }
        // Before, the periods and after add up to the stored total.
        let sum = b.before + b.values.iter().sum::<f64>() + b.after;
        assert!((sum - b.total).abs() < 1e-9, "{sum} against {}", b.total);
    }
    assert!((whole[0].total - 24.).abs() < 1e-9);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// Running totals from the start of the data agree, period for period,
/// with those of a view scrolled to start `cut` periods later.
fn assert_scroll_free(whole: (f64, &[f64]), later: (f64, &[f64]), cut: usize, what: &str) {
    let from_start = usage::running(whole.0, whole.1);
    let scrolled = usage::running(later.0, later.1);
    assert_eq!(from_start.len(), scrolled.len() + cut, "{what}");
    for (k, v) in scrolled.iter().enumerate() {
        assert!(
            (from_start[cut + k] - v).abs() < 1e-9,
            "{what}: period {k} after the scroll reads {v}, from the start {}",
            from_start[cut + k]
        );
    }
}

/// What falls before, in and after the periods adds up to `total`.
fn assert_adds_up(values: &[f64], total: f64, what: &str) {
    let (before, inner, after) = usage::unedge(values);
    let sum = before + inner.iter().sum::<f64>() + after;
    assert!((sum - total).abs() < 1e-9, "{what}: {sum} against {total}");
}

#[test]
fn activity_sheet_rows_add_up_and_run_from_the_start_of_the_data() {
    let mut d = resource_sheet("totals-activity-sheet");
    d.app.set_bottom(Some(BottomView::ActivityUsageSheet));
    d.frames(3);
    let all = schedule_days(&d.app.project);
    let fields = usage_fields::fields_of(
        &d.app.project,
        usage_fields::Which::Activity,
        &d.app.panes.usage.activity_fields,
    );
    let serial = d.app.project_serial();
    let ix = d.app.shared_index();
    let mut checked = 0;
    for costs in [false, true] {
        for row in d.app.rows() {
            // The one time-phased field a spreadsheet starts with.
            let line = |periods: &[(i64, i64)]| {
                d.app
                    .activity_sheet_line(row, &fields, &ix, periods, serial, costs)
                    .lines
                    .remove(0)
            };
            let (whole, later) = (line(&all), line(&all[3..]));
            let sum = later.before + later.values.iter().sum::<f64>() + later.after;
            assert!(
                (sum - later.total).abs() < 1e-9,
                "{row:?} {costs}: {sum} against {}",
                later.total
            );
            assert_scroll_free(
                (whole.before, &whole.values),
                (later.before, &later.values),
                3,
                "activity sheet",
            );
            checked += usize::from(later.total > 0.);
        }
    }
    assert!(checked >= 2, "the labor activity, in units and in costs");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn profiles_add_up_and_run_from_the_start_of_the_data() {
    let mut d = Driver::new("totals-profiles", super::tests::costed());
    d.home();
    d.app.set_bottom(Some(BottomView::ActivityUsageProfile));
    d.frames(3);
    let all = schedule_days(&d.app.project);
    let list = d.app.assignment_list(d.app.project_serial());
    let a = list.iter().find(|a| a.rsrc == 7).unwrap().clone();
    let (busy_from, _) = busy(&usage::unedge(&d.app.activity_profile_usage(&all).budget).1);
    let cut = busy_from + 1;
    for costs in [false, true] {
        d.app.panes.usage.costs = costs;
        let mut x = if costs { a.as_costs() } else { a.clone() };
        // Actuals spread from the actual start; this activity has none, so
        // its stored actual cost has no dates to fall in and is not spread.
        if d.app.project.model.activities[a.task].act_start.is_none() {
            x.actual = 0.;
        }
        // Activity Usage Profile: every series.
        let (whole, later) = (
            d.app.activity_profile_usage(&all),
            d.app.activity_profile_usage(&all[cut..]),
        );
        for (name, w, l, total) in [
            ("budget", &whole.budget, &later.budget, x.budget),
            ("actual", &whole.actual, &later.actual, x.actual),
            ("remaining", &whole.remaining, &later.remaining, x.remaining),
            (
                "remaining late",
                &whole.remaining_late,
                &later.remaining_late,
                x.remaining,
            ),
        ] {
            assert_adds_up(l, total, name);
            let (w, l) = (usage::unedge(w), usage::unedge(l));
            assert_scroll_free((w.0, &w.1), (l.0, &l.1), cut, name);
        }
        // Resource Usage Profile: actual plus remaining.
        let (whole, later) = (
            d.app.resource_profile_usage(7, &all),
            d.app.resource_profile_usage(7, &all[cut..]),
        );
        assert_adds_up(&later.actual, x.actual, "resource actual");
        assert_adds_up(&later.remaining, x.remaining, "resource remaining");
        let both = |u: &rs::Usage| -> Vec<f64> {
            u.actual
                .iter()
                .zip(&u.remaining)
                .map(|(a, r)| a + r)
                .collect()
        };
        let (w, l) = (usage::unedge(&both(&whole)), usage::unedge(&both(&later)));
        assert_scroll_free((w.0, &w.1), (l.0, &l.1), cut, "resource profile");
        // The limit is drawn for the periods only, never for an edge.
        if !costs {
            let (b, _, e) = usage::unedge(&later.limit);
            assert_eq!((b, e), (0., 0.));
        }
    }
    assert!(a.budget > 0. && a.remaining > 0.);
    // The curves the profile draws: drawn without error at either scroll.
    for s in d.app.panes.usage.profile.activity.iter_mut() {
        s.cumulative = true;
    }
    d.frames(2);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn show_all_projects_totals_do_not_change_with_the_scroll() {
    // RIVERBEND has 24 h of PIPE, the parked NORTHYARD 16 h.
    let mut d = super::tests::labor_portfolio("totals-all-projects");
    d.home();
    d.f9();
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.app.panes.usage.resource = Some(7);
    d.app.panes.usage.show_all_projects = true;
    d.frames(3);
    let all = schedule_days(&d.app.project);
    let afields = sheet_fields(&d);
    // A window that holds nothing of either project: far past the end.
    let last = all.last().unwrap().1;
    let past: Vec<(i64, i64)> = (0..10)
        .map(|k| (last + (30 + k) * DAY, last + (31 + k) * DAY))
        .collect();
    for periods in [&all[..], &all[all.len() / 2..], &past[..]] {
        let rows = d.app.selection_sheet_rows(&afields, periods).unwrap();
        assert!(
            (rows[0].total - 40.).abs() < 1e-9,
            "the resource row totals both projects: {}",
            rows[0].total
        );
        let other = rows
            .iter()
            .find(|r| r.texts[0] == "Other open projects")
            .expect("the other projects' row, even with none of it on show");
        assert!((other.total - 16.).abs() < 1e-9, "{}", other.total);
        for r in rows.iter() {
            let sum = r.before + r.values.iter().sum::<f64>() + r.after;
            assert!((sum - r.total).abs() < 1e-9, "{:?}: {sum}", r.texts);
        }
    }
    // The Activity Usage Spreadsheet's row for the other projects.
    for periods in [&all[..], &past[..]] {
        let others = d.app.other_projects_usage(periods, false);
        let ((before, values, after), total) = &*others;
        assert!((total - 16.).abs() < 1e-9);
        assert!((before + values.iter().sum::<f64>() + after - 16.).abs() < 1e-9);
    }
    // The Assignments view's resource band, scrolled past the end.
    d.app.views.assignments.show_all_projects = true;
    d.app.views.assignments.first_minute = Some(past[0].0 as f64);
    d.app.show_view(crate::views::MainView::Assignments);
    d.frames(4);
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(
        texts.contains(&crate::format::units(40.)),
        "the band totals both projects with none of it on show: {texts:?}"
    );
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// The numbers after the value column of the row whose cells include
/// `label`, and that value, read back from a written workbook.
fn workbook_row(file: &std::path::Path, sheet: &str, title: &str, label: &str) -> (f64, Vec<f64>) {
    use openxer_interchange::xlsx_import as xi;
    let wb = xi::read_workbook(file).unwrap();
    let rows = &wb.sheets[wb.sheet(sheet).expect("the usage sheet")].rows;
    let header = rows
        .iter()
        .find(|r| r.iter().any(|c| c.text() == title))
        .expect("a header row");
    let at = header.iter().position(|c| c.text() == title).unwrap();
    let row = rows
        .iter()
        .find(|r| r.iter().any(|c| c.text() == label))
        .unwrap_or_else(|| panic!("a row for {label}"));
    let number = |k: usize| row.get(k).and_then(|c| c.text().parse::<f64>().ok());
    let values = (at + 1..header.len())
        .map(|k| number(k).unwrap_or(0.))
        .collect();
    (number(at).unwrap_or(0.), values)
}

#[test]
fn copy_to_excel_writes_every_period_of_the_data_whatever_is_on_show() {
    let mut d = resource_sheet("totals-excel");
    // Scroll the view a year past the project: nothing of it is on show.
    d.app.timeline_origin += 365 * DAY;
    d.frames(3);
    let file = d.dir.join("resource.xlsx");
    for cumulative in [false, true] {
        d.app.panes.usage.cumulative = cumulative;
        d.app.copy_usage_to_excel_to(&file);
        assert!(d.app.error.is_none(), "{:?}", d.app.error);
        let (total, values) = workbook_row(&file, "Resource Usage", "Remaining Units", "PIPE");
        assert!((total - 24.).abs() < 1e-9, "{total}");
        if cumulative {
            let last = values.iter().rev().find(|v| **v != 0.).copied();
            assert_eq!(last, Some(24.), "the running total ends at the total");
        } else {
            let sum: f64 = values.iter().sum();
            assert!((sum - 24.).abs() < 1e-6, "the periods add up: {sum}");
        }
    }
    d.app.panes.usage.cumulative = false;
    // The activity spreadsheet too.
    d.app.set_bottom(Some(BottomView::ActivityUsageSheet));
    d.frames(3);
    let file = d.dir.join("activity.xlsx");
    d.app.copy_usage_to_excel_to(&file);
    let (total, values) = workbook_row(&file, "Activity Usage", "Remaining Labor Units", "RB-1010");
    assert!((total - 24.).abs() < 1e-9);
    assert!((values.iter().sum::<f64>() - 24.).abs() < 1e-6);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn a_value_too_wide_for_its_cell_shows_hashes_and_totals_show_whole() {
    let measure = |t: &str| t.len() as f32 * 7.;
    assert_eq!(
        usage::cell_text("24h", 30., measure).as_deref(),
        Some("24h")
    );
    assert_eq!(
        usage::cell_text("1,234,567h", 30., measure).as_deref(),
        Some("###")
    );
    assert_eq!(
        usage::cell_text("1,234,567h", 10., measure).as_deref(),
        Some("#")
    );
    assert_eq!(usage::cell_text("1,234,567h", 5., measure), None);
    assert_eq!(usage::cell_text("", 30., measure), None);
    // A cost far wider than the default value column.
    let mut p = super::tests::costed();
    p.document
        .table_mut("TASKRSRC")
        .set(0, "remain_cost", "1234567890.5".into());
    let mut d = Driver::new("totals-wide", p);
    d.home();
    d.app.panes.usage.costs = true;
    d.app.panes.usage.resource = Some(7);
    let wide = crate::format::cost(1234567890.5);
    for view in [
        BottomView::ActivityUsageSheet,
        BottomView::ResourceUsageSheet,
    ] {
        d.app.set_bottom(Some(view));
        d.frames(3);
        let out = d.frame(vec![]);
        let column = d.rect("usage_sheet_display");
        assert!(
            column.width() > usage::VALUE_W,
            "{view:?}: the column widens"
        );
        let totals: Vec<Rect> = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == wide => {
                    Some(Rect::from_min_size(t.pos, t.galley.size()))
                }
                _ => None,
            })
            .collect();
        assert!(!totals.is_empty(), "{view:?}: the total is drawn");
        for r in totals {
            assert!(
                r.left() >= column.left() && r.right() <= column.right(),
                "{view:?}: {r:?} lies inside the value column {column:?}"
            );
        }
        // The per-day costs do not fit a day's cell: hashes, not blanks.
        let hashes = out.shapes.iter().any(|s| {
            matches!(&s.shape, egui::Shape::Text(t)
                if t.galley.text().starts_with('#') && t.pos.x > column.right())
        });
        assert!(hashes, "{view:?}: a cell too narrow shows ###");
    }
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// `with_resource` plus two materials: CONC in cubic yards with 80 CY on
/// the labor activity, and REBAR in tons with 3 TON on it.
fn with_materials() -> ProjectData {
    let mut doc = rs::tests::with_resource().document;
    for (id, abbrev) in [("31", "CY"), ("32", "TON")] {
        doc.table_mut("UMEASURE").add(&[
            ("unit_id", id.into()),
            ("unit_abbrev", abbrev.into()),
            ("unit_name", abbrev.into()),
        ]);
    }
    for (id, code, unit, seq) in [("20", "CONC", "31", "5"), ("21", "REBAR", "32", "6")] {
        doc.table_mut("RSRC").add(&[
            ("rsrc_id", id.into()),
            ("clndr_id", "1".into()),
            ("rsrc_short_name", code.into()),
            ("rsrc_name", code.into()),
            ("rsrc_type", "RT_Mat".into()),
            ("unit_id", unit.into()),
            ("active_flag", "Y".into()),
            ("rsrc_seq_num", seq.into()),
        ]);
    }
    for (id, rsrc, qty) in [("40", "20", "80"), ("41", "21", "3")] {
        doc.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", id.into()),
            ("task_id", "1010".into()),
            ("proj_id", "1".into()),
            ("rsrc_id", rsrc.into()),
            ("remain_qty", qty.into()),
            ("target_qty", qty.into()),
            ("rollup_dates_flag", "N".into()),
        ]);
    }
    let key = doc.projects()[0].0;
    let mut p = ProjectData::from_document(doc, key, "materials.xer".into()).unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

#[test]
fn material_quantities_show_in_their_unit_and_do_not_add_across_units() {
    let p = with_materials();
    let list = rs::resources(&p);
    let ids = |v: &[i64]| {
        v.iter()
            .copied()
            .collect::<std::collections::BTreeSet<i64>>()
    };
    assert_eq!(
        usage_list::material_unit(&list, &ids(&[20])).as_deref(),
        Some("CY")
    );
    assert_eq!(usage_list::material_unit(&list, &ids(&[7])), None);
    assert_eq!(usage_list::material_unit(&list, &ids(&[20, 21])), None);
    assert!(!usage_list::mixed_units(&list, &ids(&[20])));
    assert!(
        usage_list::mixed_units(&list, &ids(&[20, 21])),
        "CY and TON"
    );
    assert!(
        usage_list::mixed_units(&list, &ids(&[20, 7])),
        "CY and hours"
    );
    assert_eq!(usage_list::material_text(80., "CY"), "80 CY");
    assert_eq!(usage_list::material_text(2.5, ""), "2.5");
    // The Resource Usage Spreadsheet reads 80 CY, not 80 hours.
    let mut d = Driver::new("totals-material", p);
    d.home();
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.app.panes.usage.resource = Some(20);
    d.frames(4);
    let afields = sheet_fields(&d);
    let rows = d
        .app
        .selection_sheet_rows(&afields, &d.app.panes.sheet_periods)
        .unwrap();
    assert_eq!(rows[0].unit.as_deref(), Some("CY"));
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(texts.contains(&"80 CY".to_string()), "{texts:?}");
    assert!(!texts.contains(&crate::format::units(80.)));
    // The profile's scale counts cubic yards too.
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(3);
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(texts.iter().any(|t| t.ends_with(" CY")), "{texts:?}");
    // Its stacked profile stacks only the materials counted in CY.
    let stack = super::stacked::resource_profile_assignments(&d.app.project, Some(20));
    assert!(stack.iter().all(|a| a.rsrc == 20), "REBAR is in tons");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

//! The list dialogs: Resource Curves (with the curve editor), Resource
//! Shifts, Currencies, Financial Period Calendars and Save Period Actuals.
use super::dialogs::keep_selection;
use super::ops::{self, Interval, CURVE_POINTS};
use super::tree;
use super::ui::{self, RowView};
use super::*;
use crate::dialog_kit::{self as kit, Entry};
use chrono::{Datelike, NaiveDate, Weekday};
use openxer_xer::edit::curves::Kind;

const WEEKDAYS: [(&str, Weekday); 7] = [
    ("Monday", Weekday::Mon),
    ("Tuesday", Weekday::Tue),
    ("Wednesday", Weekday::Wed),
    ("Thursday", Weekday::Thu),
    ("Friday", Weekday::Fri),
    ("Saturday", Weekday::Sat),
    ("Sunday", Weekday::Sun),
];

fn changed(flag: bool) -> Done {
    if flag {
        Done::Changed(None)
    } else {
        Done::Nothing
    }
}

/// Close, Add, Delete, then `extra`, then Help; the clicked suffix.
fn buttons(
    c: &mut Canvas,
    prefix: &str,
    width: f32,
    has: bool,
    extra: &[(&str, &str, bool)],
    busy: bool,
) -> Option<String> {
    let keys: Vec<String> = ["close", "add", "delete", "help"]
        .iter()
        .map(|k| format!("{prefix}_{k}"))
        .collect();
    let extra_keys: Vec<String> = extra
        .iter()
        .map(|(k, _, _)| format!("{prefix}_{k}"))
        .collect();
    let mut entries = vec![
        Entry::Button(&keys[0], "Close", !busy, false),
        Entry::Button(&keys[1], "Add", !busy, true),
        Entry::Button(&keys[2], "Delete", has && !busy, false),
    ];
    for (k, (_, label, enabled)) in extra.iter().enumerate() {
        entries.push(Entry::Button(
            &extra_keys[k],
            label,
            *enabled && !busy,
            k == 0,
        ));
    }
    entries.push(Entry::Button(&keys[3], "Help", !busy, true));
    kit::button_column(c, width, &entries).clicked.map(|k| {
        k.trim_start_matches(prefix)
            .trim_start_matches('_')
            .to_string()
    })
}

/// A small bar chart of a curve's 21 points in a lowered box (empty when
/// nothing is selected).
fn curve_thumbnail(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, points: Option<&[f64; 21]>) {
    c.lowered(x, y, w, h);
    c.fill(x + 1., y + 1., w - 2., h - 2., Color32::WHITE);
    let Some(points) = points else {
        return;
    };
    let top = points.iter().copied().fold(1e-9, f64::max);
    let pitch = (w - 8.) / CURVE_POINTS as f32;
    for (k, v) in points.iter().enumerate() {
        let bar = ((v / top) as f32 * (h - 10.)).round();
        let bx = x + 4. + pitch * k as f32;
        c.fill(
            bx + 1.,
            y + h - 5. - bar,
            (pitch - 2.).max(1.),
            bar,
            Color32::from_rgb(0x00, 0x00, 0xFF),
        );
    }
}

impl App {
    // --------------------------------------------------------------- curves

    pub(super) fn curves_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) -> bool {
        let size = Vec2::new(480., 460.);
        let doc = &self.project.document;
        let curves = ops::curve_rows(doc);
        let ids: Vec<i64> = curves.iter().map(|(k, _)| *k).collect();
        keep_selection(&mut st.selected, &ids);
        let rows: Vec<RowView> = curves
            .iter()
            .map(|(key, c)| {
                let default = c.kind == Kind::Default;
                let preset = c.kind == Kind::Preset;
                RowView {
                    id: *key,
                    cells: vec![c.name.clone(), String::new(), String::new()],
                    editable: vec![!c.locked(), false, false],
                    checks: vec![None, Some(default), Some(preset)],
                    ..Default::default()
                }
            })
            .collect();
        let preview = st
            .selected
            .and_then(|s| curves.iter().find(|(k, _)| *k == s))
            .map(|(_, c)| c.points);
        let selected = st.selected;
        let child = st.curve.is_some();
        let ((clicked, double), closed, actions) =
            kit::window(ctx, "curves", Dialog::Curves.title(), size, |c| {
                kit::strip(c, size.x, size.y);
                let out = ui::grid(
                    c,
                    "curves_list",
                    (5., 5., size.x - 122., size.y - 120.),
                    Some("Resource Curves"),
                    &[
                        ("Resource Curve Name", 200.),
                        ("Default", 70.),
                        ("Preset", size.x - 394.),
                    ],
                    &rows,
                    selected.and_then(|s| ids.iter().position(|i| *i == s)),
                    &mut st.edit,
                    !busy && !child,
                );
                if let Some(r) = out.clicked {
                    st.selected = Some(ids[r]);
                }
                if let Some((id, _, text)) = out.committed {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        Ok(changed(ops::rename_curve(doc, id, &text)?))
                    })));
                }
                // The selected curve's shape, so a curve can be judged
                // before it is opened or picked.
                let (px, py, pw, ph) = (5., size.y - 108., size.x - 122., 103.);
                kit::text(c, px, py, "Shape of the selected curve", true);
                curve_thumbnail(c, px, py + 16., pw, ph - 16., preview.as_ref());
                let has = selected.is_some();
                let deletable = selected.is_some_and(|id| !ops::is_locked_curve(doc, id));
                let clicked = buttons(
                    c,
                    "curves",
                    size.x,
                    deletable,
                    &[("modify", "Modify...", has)],
                    busy || child,
                );
                (clicked, out.double.map(|r| ids[r]))
            });
        self.record_dialog_actions(actions);
        let mut close = closed;
        let open_editor = |st: &mut DialogState, doc: &Document, id: i64| {
            st.curve = Some(CurveEditor {
                id,
                name: ops::curve_name(doc, id),
                points: ops::curve_points(doc, id)
                    .iter()
                    .map(|v| format!("{v}"))
                    .collect(),
                read_only: ops::is_locked_curve(doc, id),
            });
        };
        if let Some(id) = double {
            open_editor(st, &self.project.document, id);
        }
        match clicked.as_deref() {
            Some("close") => close = true,
            Some("help") => self.open_help_for("curves"),
            Some("add") => {
                st.edit = None;
                jobs.push(Job::Doc(Box::new(move |doc| {
                    let id = ops::add_curve(doc, selected);
                    Ok(Done::Changed(Some(Sel::Main(id))))
                })));
            }
            Some("delete") => {
                if let Some(id) = selected {
                    let uses = ops::curve_uses(&self.project.document, id);
                    let text = if uses > 0 {
                        format!("{uses} assignment(s) use this curve. Delete it and leave them without a curve?")
                    } else {
                        "Delete the selected resource curve?".into()
                    };
                    jobs.push(Job::Ask(Confirm::yes_no(
                        "Delete Resource Curve",
                        text,
                        Job::Doc(Box::new(move |doc| {
                            ops::delete_curve(doc, id)?;
                            Ok(Done::Changed(None))
                        })),
                    )));
                }
            }
            Some("modify") => {
                if let Some(id) = selected {
                    open_editor(st, &self.project.document, id);
                }
            }
            _ => {}
        }
        if st.curve.is_some() {
            self.curve_editor(ctx, st, jobs, busy);
        }
        close
    }

    fn curve_editor(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) {
        let size = Vec2::new(760., 340.);
        let Some(editor) = st.curve.as_mut() else {
            return;
        };
        let read_only = editor.read_only;
        let preset = ops::is_preset_curve(&self.project.document, editor.id);
        let title = if read_only {
            "View Resource Curve"
        } else {
            "Edit Resource Curve"
        };
        let ((ok, cancel, help), closed, actions) = kit::window(
            ctx,
            "curve_editor",
            title,
            size,
            |c| {
                kit::strip(c, size.x, size.y);
                kit::text(c, 10., 8., "Resource Curve Name", true);
                c.edit(
                    "curve_name",
                    10.,
                    24.,
                    300.,
                    21.,
                    &mut editor.name,
                    !read_only && !busy,
                    Align::Min,
                );
                if read_only {
                    let note = if preset {
                        "Presets are read only. Add makes an editable copy."
                    } else {
                        "Default curves cannot be changed."
                    };
                    kit::text(c, 330., 28., note, false);
                }
                // The 21 points as bars over 0% to 100% of the duration.
                let (gx, gy, gw, gh) = (10., 56., size.x - 132., 200.);
                c.lowered(gx, gy, gw, gh);
                c.fill(gx + 1., gy + 1., gw - 2., gh - 2., Color32::WHITE);
                let values: Vec<f64> = editor
                    .points
                    .iter()
                    .map(|t| t.trim().parse::<f64>().unwrap_or(0.).max(0.))
                    .collect();
                let top = values.iter().copied().fold(1., f64::max);
                let pitch = (gw - 8.) / CURVE_POINTS as f32;
                for (k, v) in values.iter().enumerate() {
                    let h = ((v / top) as f32 * (gh - 30.)).round();
                    let x = gx + 4. + pitch * k as f32;
                    c.fill(
                        x + 3.,
                        gy + gh - 18. - h,
                        pitch - 6.,
                        h,
                        Color32::from_rgb(0x00, 0x00, 0xFF),
                    );
                    let label = format!("{}", k * 5);
                    kit::text(c, x + 4., gy + gh - 15., &label, true);
                }
                for (k, text) in editor.points.iter_mut().enumerate() {
                    let x = gx + 4. + pitch * k as f32;
                    c.edit(
                        &format!("curve_point_{k}"),
                        x + 1.,
                        gy + gh + 8.,
                        pitch - 2.,
                        21.,
                        text,
                        !read_only && !busy,
                        Align::Max,
                    );
                }
                kit::text(
                c,
                10.,
                gy + gh + 40.,
                "Percent of the units spent in each 5% of the duration; OK scales them to total 100%.",
                true,
            );
                let column = kit::button_column(
                    c,
                    size.x,
                    &[
                        Entry::Button("curve_editor_ok", "OK", !read_only && !busy, false),
                        Entry::Button("curve_editor_cancel", "Cancel", !busy, false),
                        Entry::Button("curve_editor_help", "Help", !busy, true),
                    ],
                );
                let clicked = column.clicked.unwrap_or_default();
                (
                    clicked == "curve_editor_ok",
                    clicked == "curve_editor_cancel",
                    clicked == "curve_editor_help",
                )
            },
        );
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("curve_editor");
        }
        if cancel || closed {
            st.curve = None;
        } else if ok {
            let editor = st.curve.take().unwrap();
            match ops::normalize_points(&editor.points) {
                Ok(points) => jobs.push(Job::Doc(Box::new(move |doc| {
                    Ok(changed(ops::set_curve(
                        doc,
                        editor.id,
                        &editor.name,
                        &points,
                    )?))
                }))),
                Err(e) => {
                    st.notice = Some(e);
                    st.curve = Some(editor);
                }
            }
        }
    }

    // --------------------------------------------------------------- shifts

    pub(super) fn shifts_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) -> bool {
        let size = Vec2::new(500., 440.);
        let doc = &self.project.document;
        let ids = ops::list_ids(doc, &ops::SHIFTS);
        keep_selection(&mut st.selected, &ids);
        let selected = st.selected;
        let rows: Vec<RowView> = ids
            .iter()
            .map(|id| RowView {
                id: *id,
                cells: vec![ops::list_value(doc, &ops::SHIFTS, *id, "shift_name")],
                editable: vec![true],
                ..Default::default()
            })
            .collect();
        let periods = selected.map_or(vec![], |s| ops::shift_periods(doc, s));
        let hours = ops::period_hours(&periods);
        let period_ids: Vec<i64> = periods.iter().map(|p| p.0).collect();
        keep_selection(&mut st.sub, &period_ids);
        let sub = st.sub;
        let period_rows: Vec<RowView> = periods
            .iter()
            .zip(&hours)
            .enumerate()
            .map(|(k, ((id, start), h))| RowView {
                id: *id,
                cells: vec![
                    format!("{start:02}:00"),
                    h.to_string(),
                    format!("Shift {}", k + 1),
                ],
                editable: vec![true, false, false],
                ..Default::default()
            })
            .collect();
        let (clicked, closed, actions) =
            kit::window(ctx, "shifts", Dialog::Shifts.title(), size, |c| {
                kit::strip(c, size.x, size.y);
                let out = ui::grid(
                    c,
                    "shifts_list",
                    (5., 5., size.x - 122., 180.),
                    Some("Resource Shifts"),
                    &[("Shift Name", size.x - 124.)],
                    &rows,
                    selected.and_then(|s| ids.iter().position(|i| *i == s)),
                    &mut st.edit,
                    !busy,
                );
                if let Some(r) = out.clicked {
                    if st.selected != Some(ids[r]) {
                        st.sub = None;
                    }
                    st.selected = Some(ids[r]);
                }
                if let Some((id, _, text)) = out.committed {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        Ok(changed(ops::list_rename(doc, &ops::SHIFTS, id, &text)?))
                    })));
                }
                kit::heading(c, 5., 191., size.x - 122., "Shift Periods");
                let out = ui::grid(
                    c,
                    "shift_periods",
                    (5., 216., size.x - 122., size.y - 221.),
                    None,
                    &[
                        ("Start Time", 120.),
                        ("Hours", 90.),
                        ("Period", size.x - 334.),
                    ],
                    &period_rows,
                    sub.and_then(|s| period_ids.iter().position(|i| *i == s)),
                    &mut st.sub_edit,
                    !busy,
                );
                if let Some(r) = out.clicked {
                    st.sub = Some(period_ids[r]);
                }
                if let (Some((period, _, text)), Some(shift)) = (out.committed, selected) {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        Ok(changed(ops::set_period_start(doc, shift, period, &text)?))
                    })));
                }
                buttons(
                    c,
                    "shifts",
                    size.x,
                    selected.is_some(),
                    &[
                        ("add_period", "Add Period", selected.is_some()),
                        (
                            "delete_period",
                            "Delete Period",
                            sub.is_some() && period_ids.len() > 1,
                        ),
                    ],
                    busy,
                )
            });
        self.record_dialog_actions(actions);
        let mut close = closed;
        match clicked.as_deref() {
            Some("close") => close = true,
            Some("help") => self.open_help_for("shifts"),
            Some("add") => jobs.push(Job::Doc(Box::new(|doc| {
                let id = ops::add_shift(doc);
                Ok(Done::Changed(Some(Sel::Main(id))))
            }))),
            Some("delete") => {
                if let Some(id) = selected {
                    let uses = ops::shift_uses(&self.project.document, id);
                    let text = if uses > 0 {
                        format!("{uses} resource(s) use this shift. Delete it and its periods?")
                    } else {
                        "Delete the selected shift and its periods?".into()
                    };
                    jobs.push(Job::Ask(Confirm::yes_no(
                        "Delete Shift",
                        text,
                        Job::Doc(Box::new(move |doc| {
                            ops::delete_shift(doc, id);
                            Ok(Done::Changed(None))
                        })),
                    )));
                }
            }
            Some("add_period") => {
                if let Some(shift) = selected {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        let id = ops::add_shift_period(doc, shift)?;
                        Ok(Done::Changed(Some(Sel::Sub(id))))
                    })));
                }
            }
            Some("delete_period") => {
                if let (Some(shift), Some(period)) = (selected, sub) {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        ops::delete_shift_period(doc, shift, period)?;
                        Ok(Done::Changed(None))
                    })));
                }
            }
            _ => {}
        }
        close
    }

    // ----------------------------------------------------------- currencies

    pub(super) fn currencies_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) -> bool {
        let size = Vec2::new(600., 470.);
        let doc = &self.project.document;
        let spec = &ops::CURRENCIES;
        let ids = ops::list_ids(doc, spec);
        keep_selection(&mut st.selected, &ids);
        let selected = st.selected;
        let base = ops::base_currency(doc);
        let rows: Vec<RowView> = ids
            .iter()
            .map(|id| RowView {
                id: *id,
                cells: vec![
                    ops::list_value(doc, spec, *id, "curr_short_name"),
                    ops::list_value(doc, spec, *id, "curr_type"),
                    ops::list_value(doc, spec, *id, "base_exch_rate"),
                ],
                bold: Some(*id) == base,
                ..Default::default()
            })
            .collect();
        let (clicked, closed, actions) =
            kit::window(ctx, "currencies", Dialog::Currencies.title(), size, |c| {
                kit::strip(c, size.x, size.y);
                let mut none = None;
                let out = ui::grid(
                    c,
                    "currencies_list",
                    (5., 5., size.x - 122., 210.),
                    Some("Currencies"),
                    &[
                        ("Currency ID", 90.),
                        ("Currency Name", 200.),
                        ("Exchange Rate", size.x - 414.),
                    ],
                    &rows,
                    selected.and_then(|s| ids.iter().position(|i| *i == s)),
                    &mut none,
                    !busy,
                );
                if let Some(r) = out.clicked {
                    st.selected = Some(ids[r]);
                }
                let top = 222.;
                let tabs = ["General", "Appearance"].map(|label| kit::VTab {
                    label,
                    key: label,
                    unavailable: None,
                });
                if let Some(k) = kit::tabs(
                    c,
                    "currencies_tab_",
                    (5., top + 21., size.x - 122., size.y - top - 26.),
                    &tabs,
                    st.tab,
                ) {
                    st.tab = k;
                }
                if let Some(id) = selected {
                    let is_base = Some(id) == base;
                    let mut set = |column: &'static str, text: String| {
                        jobs.push(Job::Doc(Box::new(move |doc| {
                            Ok(changed(ops::set_currency(doc, id, column, &text)?))
                        })));
                    };
                    let y = top + 52.;
                    if st.tab == 0 {
                        for (k, (label, column, enabled)) in [
                            ("Currency ID", "curr_short_name", true),
                            ("Currency Name", "curr_type", true),
                            ("Currency Symbol", "curr_symbol", true),
                            ("Exchange Rate", "base_exch_rate", !is_base),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let x = 14. + 225. * (k % 2) as f32;
                            let row_y = y + 46. * (k / 2) as f32;
                            let current = ops::list_value(doc, spec, id, column);
                            let key = format!("currencies_field_{column}");
                            let enabled = enabled && !busy;
                            kit::text(c, x, row_y - 15., label, enabled);
                            if let Some(text) = kit::commit_edit(
                                c,
                                &key,
                                x,
                                row_y,
                                200.,
                                &current,
                                enabled,
                                Align::Min,
                            ) {
                                set(column, text);
                            }
                        }
                        if is_base {
                            kit::text(
                                c,
                                14.,
                                y + 90.,
                                "Base currency: its exchange rate is always 1.",
                                true,
                            );
                        }
                    } else {
                        for (k, (label, column)) in [
                            ("Decimal Symbol", "decimal_symbol"),
                            ("Digit Grouping Symbol", "digit_group_symbol"),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let current = ops::list_value(doc, spec, id, column);
                            let key = format!("currencies_field_{column}");
                            let x = 14. + 225. * k as f32;
                            kit::text(c, x, y - 15., label, !busy);
                            if let Some(text) =
                                kit::commit_edit(c, &key, x, y, 60., &current, !busy, Align::Min)
                            {
                                set(column, text);
                            }
                        }
                        kit::text(c, 14., y + 31., "Number of Decimal Places", true);
                        let digits = ["0", "1", "2", "3", "4"];
                        let items: Vec<(&str, bool)> = digits.iter().map(|d| (*d, true)).collect();
                        let mut k =
                            records::num(&ops::list_value(doc, spec, id, "decimal_digit_cnt"))
                                .clamp(0, 4) as usize;
                        if c.combo(
                            "currencies_decimals",
                            14.,
                            y + 47.,
                            60.,
                            22.,
                            &items,
                            &mut k,
                            !busy,
                        ) {
                            set("decimal_digit_cnt", k.to_string());
                        }
                        for (n, (label, column, formats)) in [
                            (
                                "Positive Currency Format",
                                "pos_curr_fmt_type",
                                &ops::POSITIVE_FORMATS[..],
                            ),
                            (
                                "Negative Currency Format",
                                "neg_curr_fmt_type",
                                &ops::NEGATIVE_FORMATS[..],
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let x = 14. + 225. * n as f32;
                            kit::text(c, x, y + 77., label, true);
                            let items: Vec<(&str, bool)> =
                                formats.iter().map(|f| (*f, true)).collect();
                            let current = ops::list_value(doc, spec, id, column);
                            let mut k = formats.iter().position(|f| *f == current).unwrap_or(0);
                            if c.combo(
                                &format!("currencies_{column}"),
                                x,
                                y + 93.,
                                120.,
                                22.,
                                &items,
                                &mut k,
                                !busy,
                            ) {
                                set(column, formats[k].to_string());
                            }
                        }
                        kit::text(c, 14., y + 128., "Example", true);
                        kit::field(
                            c,
                            14.,
                            y + 144.,
                            300.,
                            21.,
                            &ops::currency_example(doc, id),
                            false,
                        );
                    }
                }
                let select = [("select", "Select", selected.is_some())];
                buttons(
                    c,
                    "currencies",
                    size.x,
                    selected.is_some() && selected != base,
                    if st.select.is_some() { &select } else { &[] },
                    busy,
                )
            });
        self.record_dialog_actions(actions);
        let mut close = closed;
        match clicked.as_deref() {
            Some("close") => close = true,
            Some("select") => st.chosen = selected,
            Some("help") => self.open_help_for("currencies"),
            Some("add") => jobs.push(Job::Doc(Box::new(|doc| {
                let id = ops::add_currency(doc);
                Ok(Done::Changed(Some(Sel::Main(id))))
            }))),
            Some("delete") => {
                if let Some(id) = selected {
                    let uses = ops::currency_uses(&self.project.document, id);
                    let text = if uses > 0 {
                        format!("{uses} resource(s) are priced in this currency. Delete it and price them in the base currency?")
                    } else {
                        "Delete the selected currency?".into()
                    };
                    jobs.push(Job::Ask(Confirm::yes_no(
                        "Delete Currency",
                        text,
                        Job::Doc(Box::new(move |doc| {
                            ops::delete_currency(doc, id)?;
                            Ok(Done::Changed(None))
                        })),
                    )));
                }
            }
            _ => {}
        }
        close
    }

    // ---------------------------------------------- financial period calendars

    pub(super) fn fin_calendars_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) -> bool {
        let size = Vec2::new(700., 500.);
        let doc = &self.project.document;
        let spec = &ops::FIN_CALENDARS;
        let ids = ops::list_ids(doc, spec);
        keep_selection(&mut st.selected, &ids);
        let selected = st.selected;
        let rows: Vec<RowView> = ids
            .iter()
            .map(|id| RowView {
                id: *id,
                cells: vec![
                    ops::list_value(doc, spec, *id, "fintmpl_name"),
                    String::new(),
                ],
                editable: vec![true, false],
                checks: vec![
                    None,
                    Some(ops::list_value(doc, spec, *id, "default_flag") == "Y"),
                ],
                ..Default::default()
            })
            .collect();
        let periods = selected.map_or(vec![], |s| ops::periods(doc, s));
        let period_ids: Vec<i64> = periods.iter().map(|p| p.id).collect();
        keep_selection(&mut st.sub, &period_ids);
        let sub = st.sub;
        let day = |d: NaiveDate| format::day(d);
        let period_rows: Vec<RowView> = periods
            .iter()
            .map(|p| RowView {
                id: p.id,
                cells: vec![p.name.clone(), day(p.start), day(p.end)],
                editable: vec![true, false, false],
                ..Default::default()
            })
            .collect();
        if st.batch.from.is_empty() {
            let data_date = self.project.model.project.data_date.unwrap_or(0)
                + self.project.model.origin_serial * openxer_engine::calendar::DAY;
            let d = ops::day_of_minutes(data_date);
            let first = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap_or(d);
            st.batch.from = day(first);
            st.batch.to = day(NaiveDate::from_ymd_opt(first.year() + 1, first.month(), 1)
                .map_or(first, |n| n.pred_opt().unwrap_or(n)));
        }
        let (clicked, closed, actions) = kit::window(
            ctx,
            "fin_calendars",
            Dialog::FinCalendars.title(),
            size,
            |c| {
                kit::strip(c, size.x, size.y);
                let out = ui::grid(
                    c,
                    "fin_calendars_list",
                    (5., 5., size.x - 122., 130.),
                    Some("Financial Period Calendars"),
                    &[("Calendar Name", size.x - 204.), ("Default", 80.)],
                    &rows,
                    selected.and_then(|s| ids.iter().position(|i| *i == s)),
                    &mut st.edit,
                    !busy,
                );
                if let Some(r) = out.clicked {
                    if st.selected != Some(ids[r]) {
                        st.sub = None;
                    }
                    st.selected = Some(ids[r]);
                }
                if let Some((r, _)) = out.toggled {
                    let id = ids[r];
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        Ok(changed(ops::set_default_fin_calendar(doc, id)))
                    })));
                }
                if let Some((id, _, text)) = out.committed {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        Ok(changed(ops::list_rename(
                            doc,
                            &ops::FIN_CALENDARS,
                            id,
                            &text,
                        )?))
                    })));
                }
                // Batch Create.
                let top = 142.;
                kit::section(c, 5., top, size.x - 122., 99., "Batch Create");
                let b = &mut st.batch;
                kit::text(c, 14., top + 29., "Start Date", true);
                c.edit(
                    "fin_batch_from",
                    14.,
                    top + 45.,
                    110.,
                    21.,
                    &mut b.from,
                    !busy,
                    Align::Min,
                );
                kit::text(c, 134., top + 29., "End Date", true);
                c.edit(
                    "fin_batch_to",
                    134.,
                    top + 45.,
                    110.,
                    21.,
                    &mut b.to,
                    !busy,
                    Align::Min,
                );
                if kit::radio(
                    c,
                    "fin_batch_monthly",
                    260.,
                    top + 27.,
                    150.,
                    18.,
                    "Month ending on day",
                    b.monthly,
                    !busy,
                ) {
                    b.monthly = true;
                }
                if kit::radio(
                    c,
                    "fin_batch_weekly",
                    260.,
                    top + 52.,
                    150.,
                    18.,
                    "Week ending on",
                    !b.monthly,
                    !busy,
                ) {
                    b.monthly = false;
                }
                let days: Vec<String> = (1..=30)
                    .map(|d| d.to_string())
                    .chain(["Last day".to_string()])
                    .collect();
                let items: Vec<(&str, bool)> = days.iter().map(|d| (d.as_str(), true)).collect();
                c.combo(
                    "fin_batch_day",
                    412.,
                    top + 25.,
                    90.,
                    22.,
                    &items,
                    &mut b.month_day,
                    !busy && b.monthly,
                );
                let items: Vec<(&str, bool)> = WEEKDAYS.iter().map(|(n, _)| (*n, true)).collect();
                c.combo(
                    "fin_batch_weekday",
                    412.,
                    top + 50.,
                    90.,
                    22.,
                    &items,
                    &mut b.weekday,
                    !busy && !b.monthly,
                );
                let batch = kit::button(
                    c,
                    "fin_batch_create",
                    14.,
                    top + 70.,
                    kit::BUTTON.x,
                    kit::BUTTON.y,
                    "Batch Create",
                    !busy && selected.is_some(),
                );
                let out = ui::grid(
                    c,
                    "fin_periods",
                    (5., 247., size.x - 122., size.y - 252.),
                    Some("Financial Periods"),
                    &[
                        ("Period Name", 180.),
                        ("Start Date", 150.),
                        ("End Date", size.x - 454.),
                    ],
                    &period_rows,
                    sub.and_then(|s| period_ids.iter().position(|i| *i == s)),
                    &mut st.sub_edit,
                    !busy,
                );
                if let Some(r) = out.clicked {
                    st.sub = Some(period_ids[r]);
                }
                if let (Some((id, _, text)), Some(cal)) = (out.committed, selected) {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        Ok(changed(ops::rename_period(doc, cal, id, &text)?))
                    })));
                }
                let clicked = buttons(
                    c,
                    "fin_calendars",
                    size.x,
                    selected.is_some(),
                    &[("delete_period", "Delete Period", sub.is_some())],
                    busy,
                );
                if batch {
                    Some("batch".to_string())
                } else {
                    clicked
                }
            },
        );
        self.record_dialog_actions(actions);
        let mut close = closed;
        match clicked.as_deref() {
            Some("close") => close = true,
            Some("help") => self.open_help_for("fin_calendars"),
            Some("add") => jobs.push(Job::Doc(Box::new(|doc| {
                let id = ops::add_fin_calendar(doc)?;
                Ok(Done::Changed(Some(Sel::Main(id))))
            }))),
            Some("delete") => {
                if let Some(id) = selected {
                    let doc = &self.project.document;
                    let uses = ops::fin_calendar_uses(doc, id);
                    let count = ops::periods(doc, id).len();
                    // Stored period performance: refused at once, OK only.
                    if ops::fin_calendar_stored(doc, id) > 0 {
                        st.notice = Some(ops::FIN_CALENDAR_KEPT.into());
                    } else {
                        let text = if uses > 0 {
                            format!("This calendar has {count} period(s) and {uses} project(s) use it. Delete it and its periods?")
                        } else {
                            format!("Delete the selected calendar and its {count} period(s)?")
                        };
                        jobs.push(Job::Ask(Confirm::yes_no(
                            "Delete Financial Period Calendar",
                            text,
                            Job::Doc(Box::new(move |doc| {
                                ops::delete_fin_calendar(doc, id)?;
                                Ok(Done::Changed(None))
                            })),
                        )));
                    }
                }
            }
            Some("delete_period") => {
                if let Some(id) = sub {
                    let stored = openxer_xer::edit::financial_periods::stored_rows(
                        &self.project.document,
                        &[id],
                    );
                    if stored > 0 {
                        // Refused at once with an OK-only message; nothing is asked.
                        st.notice = Some(openxer_xer::edit::financial_periods::refusal(1));
                    } else {
                        jobs.push(Job::Ask(Confirm::yes_no(
                            "Delete Financial Period",
                            "Delete the selected financial period?".into(),
                            Job::Doc(Box::new(move |doc| {
                                ops::delete_periods(doc, &[id])?;
                                Ok(Done::Changed(None))
                            })),
                        )));
                    }
                }
            }
            Some("batch") => {
                let parse = |text: &str| -> tree::Result<NaiveDate> {
                    format::parse_date(0, text, None)?
                        .map(ops::day_of_minutes)
                        .ok_or_else(|| "Enter both dates.".to_string())
                };
                let b = st.batch.clone();
                let interval = if b.monthly {
                    Interval::Monthly(if b.month_day >= 30 {
                        31
                    } else {
                        b.month_day as u32 + 1
                    })
                } else {
                    Interval::Weekly(WEEKDAYS[b.weekday.min(6)].1)
                };
                match (parse(&b.from), parse(&b.to), selected) {
                    (Ok(from), Ok(to), Some(cal)) => jobs.push(Job::Doc(Box::new(move |doc| {
                        let ids = ops::batch_create(doc, cal, from, to, interval)?;
                        Ok(Done::Changed(ids.first().map(|id| Sel::Sub(*id))))
                    }))),
                    (Err(e), _, _) | (_, Err(e), _) => st.notice = Some(e),
                    _ => {}
                }
            }
            _ => {}
        }
        close
    }

    // ------------------------------------------------ store period performance

    /// Tools > Save Period Actuals: the project's financial periods and
    /// Store Now, which stores the actuals since the earlier periods into the
    /// chosen one as one undo step.
    pub(crate) fn store_period_body(&mut self, ctx: &egui::Context) {
        let size = Vec2::new(420., 110.);
        // Store Now updates every open project.
        let open_count = 1 + self.open_set.parked.len();
        let time = ctx.input(|i| i.time);
        let doc = &self.project.document;
        let pid = self.project.model.project.proj_id;
        let calendar = ops::project_fin_calendar(doc, pid);
        let all = calendar.map_or(vec![], |cal| ops::periods(doc, cal));
        // Only the periods User Preferences > Financial Period Data loads.
        let narrowed = !all.is_empty();
        let periods = ops::in_range(all, self.formats.fin_from, self.formats.fin_to);
        let ids: Vec<i64> = periods.iter().map(|p| p.id).collect();
        if !self
            .enterprise
            .store_period
            .is_some_and(|p| ids.contains(&p))
        {
            let data_date = self.project.model.project.data_date.unwrap_or(0)
                + self.project.model.origin_serial * openxer_engine::calendar::DAY;
            let today = ops::day_of_minutes(data_date);
            // The last period that ends before the data date (the one just
            // closed), else the one holding it, else the first.
            self.enterprise.store_period = ops::period_before(&periods, today)
                .or_else(|| calendar.and_then(|cal| ops::period_containing(doc, cal, today)))
                .or_else(|| ids.first().copied());
        }
        let names: Vec<String> = periods
            .iter()
            .map(|p| {
                format!(
                    "{}  ({} - {})",
                    p.name,
                    format::day(p.start),
                    format::day(p.end)
                )
            })
            .collect();
        let mut k = self
            .enterprise
            .store_period
            .and_then(|p| ids.iter().position(|i| *i == p))
            .unwrap_or(0);
        let ((store, cancel, help), closed, actions) =
            kit::window(ctx, "store_period", "Save Period Actuals", size, |c| {
                c.fill(0., 0., size.x, size.y, kit::STRIP);
                c.fill(5., 5., 298., 100., kit::FACE);
                c.lowered(5., 5., 298., 100.);
                c.label(14., 14., 16., "Projects to be updated:", true);
                let mut count = open_count.to_string();
                c.edit(
                    "store_projects",
                    200.,
                    12.,
                    90.,
                    21.,
                    &mut count,
                    false,
                    Align::Center,
                );
                c.label(14., 45., 16., "Financial Period:", true);
                let items: Vec<(&str, bool)> = names.iter().map(|n| (n.as_str(), true)).collect();
                if items.is_empty() {
                    kit::field(c, 14., 64., 276., 21., "", false);
                    let rect = c.r(14., 64., 276., 21.);
                    let tip = if narrowed {
                        "No financial period falls in the range set in User Preferences."
                    } else {
                        "Define financial periods in Admin > Financial Period Calendars first."
                    };
                    c.hit("store_period_none", rect, false).on_hover_text(tip);
                } else {
                    c.combo(
                        "store_period_choice",
                        14.,
                        64.,
                        276.,
                        22.,
                        &items,
                        &mut k,
                        true,
                    );
                }
                let clicked = kit::button_column(
                    c,
                    size.x,
                    &[
                        Entry::Button("store_now", "Store Now", !ids.is_empty(), false),
                        Entry::cancel("store_cancel"),
                        Entry::help("store_help"),
                    ],
                )
                .clicked
                .unwrap_or_default();
                (
                    clicked == "store_now",
                    clicked == "store_cancel",
                    clicked == "store_help",
                )
            });
        self.record_dialog_actions(actions);
        if let Some(id) = ids.get(k) {
            self.enterprise.store_period = Some(*id);
        }
        if help {
            self.open_help_for("store_period");
        }
        if store {
            if let (Some(cal), Some(period)) = (calendar, self.enterprise.store_period) {
                self.store_period_now(cal, period, time);
            }
            self.sched.progress.store_period = false;
        } else if cancel || closed {
            self.sched.progress.store_period = false;
        }
    }
}

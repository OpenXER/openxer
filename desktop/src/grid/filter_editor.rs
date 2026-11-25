//! The Filter editor: a filter name and a grid of criteria rows. Row 0 is
//! the filter's own "(All of the following)" / "(Any of the following)";
//! a row whose parameter is one of those opens a nested group, and the
//! move pad's left and right arrows take rows out of or into the group
//! above. Date values take relative dates (`DD + 3w`) from the Value
//! cell's drop button.
use super::*;
use crate::grid::filters::{self, GROUP_ALL, GROUP_ANY, MAX_LEVEL};
use crate::grid::relative::Anchor;

/// Client size and grid columns (x, width) of the editor.
const CLIENT: Vec2 = Vec2::new(624., 336.);
const GRID: (f32, f32, f32, f32) = (6., 55., 500., 275.);
const COLS: [(&str, f32); 5] = [
    ("Display all rows", 84.),
    ("Parameter", 125.),
    ("Is", 121.),
    ("Value", 89.),
    ("High Value", 81.),
];
/// Indent per nesting level in the first two columns.
const STEP: f32 = 12.;

const ALL_CAPTION: &str = "(All of the following)";
const ANY_CAPTION: &str = "(Any of the following)";

/// Rows `k..end` of the row at `k` and the rows nested under it.
fn subtree(rows: &[Criterion], k: usize) -> std::ops::Range<usize> {
    let level = rows[k].level;
    let end = rows[k + 1..]
        .iter()
        .position(|r| r.level <= level)
        .map_or(rows.len(), |n| k + 1 + n);
    k..end
}

/// The deepest level row `k` may take: one inside a group head above it,
/// otherwise the level of the row above.
fn max_level(rows: &[Criterion], k: usize) -> usize {
    match k.checked_sub(1).map(|j| &rows[j]) {
        None => 0,
        Some(prev) if prev.group().is_some() => (prev.level + 1).min(MAX_LEVEL),
        Some(prev) => prev.level,
    }
}

impl App {
    pub(super) fn filter_editor_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.grid.dialogs.filter_editor.take() else {
            return;
        };
        let catalog = columns::catalog(&self.project);
        let mut params: Vec<(String, String)> = vec![
            (GROUP_ALL.into(), ALL_CAPTION.into()),
            (GROUP_ANY.into(), ANY_CAPTION.into()),
        ];
        let mut sorted: Vec<&Column> = catalog.iter().collect();
        sorted.sort_by_key(|c| c.title.to_lowercase());
        params.extend(sorted.iter().map(|c| (c.id.clone(), c.title.clone())));
        let param_titles: Vec<String> = params.iter().map(|p| p.1.clone()).collect();
        let op_titles: Vec<String> = Op::ALL.iter().map(|o| o.caption().to_string()).collect();
        let anchors: Vec<String> = Anchor::ALL
            .iter()
            .map(|a| format!("{} ({})", a.caption(), a.code()))
            .collect();
        let has_clip = !self.grid.dialogs.filter_clip.is_empty();
        let mut editing = self.grid.dialogs.filter_editing;
        let mut clicks: Vec<&'static str> = vec![];
        let ((), closed, actions) = kit::window(ctx, "grid_filter_editor", "Filter", CLIENT, |c| {
            c.fill(0., 0., CLIENT.x, CLIENT.y, STRIP);
            c.fill(0., 0., 512., CLIENT.y, CHROME);
            c.lowered(5., 5., 502., 31.);
            c.label(9., 9., 21., "Filter Name", true);
            c.edit(
                "filter_name",
                85.,
                9.,
                409.,
                21.,
                &mut e.filter.name,
                true,
                Align::LEFT,
            );
            c.lowered(5., 36., 502., 295.);
            c.fill(6., 37., 500., 18., CHROME);
            c.raised(6., 37., 500., 18.);
            chevron(c, 14., 43.);
            kit::text(c, 24., 40., "Display: Filter", true);
            let (gx, gy, gw, gh) = GRID;
            c.fill(gx, gy, gw, gh, Color32::WHITE);
            c.fill(gx, gy, gw, 18., CHROME);
            c.hline(gx, gy, gw, Color32::WHITE);
            c.hline(gx, gy + 17., gw, INK);
            let mut x = gx;
            for (title, w) in COLS {
                c.vline(x + w - 1., gy, 17., INK);
                kit::text(c, x + 3., gy + 3., title, true);
                x += w;
            }
            let body = c.r(gx, gy + 18., gw, gh - 18.);
            let rows_len = e.filter.rows.len() + 1;
            let reveal = e.selected.map(|s| s + 1);
            let selected = e.selected;
            let mut select = None;
            let mut start_edit = None;
            let filter = &mut e.filter;
            kit::rows_in(
                c,
                "filter_rows",
                body,
                18.,
                rows_len,
                reveal,
                &mut |c, n, rect, _| {
                    let p = c.ui.painter().with_clip_rect(body);
                    let line = Color32::from_gray(0xC0);
                    let mut x = rect.left();
                    let mut cells = [Rect::NOTHING; 5];
                    for (k, (_, w)) in COLS.iter().enumerate() {
                        cells[k] =
                            Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(*w - 1., 17.));
                        p.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(x + w - 1., rect.top()),
                                Vec2::new(1., 18.),
                            ),
                            0.,
                            line,
                        );
                        x += w;
                    }
                    p.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(rect.left(), rect.bottom() - 1.),
                            Vec2::new(x - rect.left(), 1.),
                        ),
                        0.,
                        line,
                    );
                    if n == 0 {
                        // The filter's own match row with its expander box.
                        let b = Rect::from_min_size(rect.min + Vec2::new(4., 4.), Vec2::splat(9.));
                        p.rect_filled(b, 0., Color32::WHITE);
                        p.rect_stroke(
                            b,
                            0.,
                            Stroke::new(1_f32, Color32::from_gray(0x80)),
                            egui::StrokeKind::Inside,
                        );
                        p.rect_filled(
                            Rect::from_min_size(b.min + Vec2::new(2., 4.), Vec2::new(5., 1.)),
                            0.,
                            INK,
                        );
                        let text = if filter.match_all {
                            ALL_CAPTION
                        } else {
                            ANY_CAPTION
                        };
                        let items = vec![ALL_CAPTION.to_string(), ANY_CAPTION.to_string()];
                        let r = super::super::cell_combo::cell_combo(
                            c,
                            "filter_match",
                            cells[1],
                            body,
                            text,
                            &items,
                            editing == Some((0, 1)),
                            false,
                            true,
                        );
                        if r.clicked {
                            start_edit = Some((0, 1));
                        }
                        if let Some(k) = r.picked {
                            filter.match_all = k == 0;
                        }
                        return;
                    }
                    let k = n - 1;
                    let sel = selected == Some(k);
                    let indent = filter.rows[k].level as f32 * STEP;
                    if sel {
                        // The row is selection blue except the editors.
                        p.rect_filled(
                            Rect::from_min_max(rect.min, Pos2::new(x, rect.bottom() - 1.)),
                            0.,
                            kit::SELECTED,
                        );
                    }
                    let where_cell = cells[0];
                    p.text(
                        Pos2::new(
                            where_cell.right() - 7. + indent.min(40.),
                            where_cell.center().y,
                        ),
                        Align2::RIGHT_CENTER,
                        "Where",
                        crate::schedule_dialogs::dialog_font(),
                        if sel { Color32::WHITE } else { INK },
                    );
                    let hit = c.ui.interact(
                        where_cell.intersect(body),
                        c.id("filter_where").with(k),
                        Sense::click(),
                    );
                    c.act(&format!("filter_where_{k}"), where_cell.intersect(body));
                    if hit.clicked() {
                        select = Some(k);
                    }
                    let row = &mut filter.rows[k];
                    let group = row.group();
                    // Parameter.
                    let param_cell = Rect::from_min_max(
                        cells[1].min + Vec2::new(indent.min(80.), 0.),
                        cells[1].max,
                    );
                    let caption = match group {
                        Some(true) => ALL_CAPTION.to_string(),
                        Some(false) => ANY_CAPTION.to_string(),
                        None if row.column.is_empty() => String::new(),
                        None => title_of(&catalog, &row.column),
                    };
                    let r = super::super::cell_combo::cell_combo(
                        c,
                        &format!("filter_param_{k}"),
                        param_cell,
                        body,
                        &caption,
                        &param_titles,
                        editing == Some((n, 1)),
                        sel,
                        true,
                    );
                    if r.clicked {
                        select = Some(k);
                        if sel {
                            start_edit = Some((n, 1));
                        }
                    }
                    if let Some(pick) = r.picked {
                        row.column = params[pick].0.clone();
                    }
                    if group.is_some() || row.group().is_some() {
                        return;
                    }
                    // Is.
                    let r = super::super::cell_combo::cell_combo(
                        c,
                        &format!("filter_op_{k}"),
                        cells[2],
                        body,
                        row.op.caption(),
                        &op_titles,
                        editing == Some((n, 2)),
                        sel,
                        true,
                    );
                    if r.clicked {
                        select = Some(k);
                        if sel {
                            start_edit = Some((n, 2));
                        }
                    }
                    if let Some(pick) = r.picked {
                        row.op = Op::ALL[pick];
                    }
                    // Value and High Value: typed text; date parameters
                    // also offer the relative anchors.
                    let is_date =
                        columns::find(&catalog, &row.column).is_some_and(|c| c.kind == Kind::Date);
                    let uses_high = row.op.uses_high();
                    for (col, key, text) in [
                        (3usize, "filter_value", &mut row.value),
                        (4usize, "filter_high", &mut row.high),
                    ] {
                        if col == 4 && !uses_high {
                            continue;
                        }
                        let cell = cells[col];
                        let edit_w = if is_date {
                            cell.width() - 17.
                        } else {
                            cell.width()
                        };
                        let field = Rect::from_min_size(cell.min, Vec2::new(edit_w, cell.height()))
                            .shrink2(Vec2::new(3., 1.))
                            .intersect(body);
                        if field.width() > 0. && field.height() > 0. {
                            let edit = egui::TextEdit::singleline(text)
                                .id(c.id(key).with(k))
                                .frame(false)
                                .font(crate::schedule_dialogs::dialog_font())
                                .text_color(if sel { Color32::WHITE } else { INK })
                                .margin(egui::Margin::ZERO)
                                .desired_width(field.width());
                            let r = c.ui.put(field, edit);
                            if r.clicked() || r.gained_focus() {
                                select = Some(k);
                            }
                        }
                        c.act(&format!("{key}_{k}"), field);
                        if is_date {
                            let button = Rect::from_min_size(
                                Pos2::new(cell.right() - 17., cell.top()),
                                Vec2::new(17., cell.height()),
                            );
                            if let Some(pick) = super::super::cell_combo::drop_button(
                                c,
                                &format!("{key}_anchor_{k}"),
                                button,
                                body,
                                cell,
                                &anchors,
                            ) {
                                *text = crate::grid::relative::Relative {
                                    anchor: Anchor::ALL[pick],
                                    offset: 0,
                                    unit: crate::grid::relative::Unit::Days,
                                }
                                .text();
                                select = Some(k);
                            }
                        }
                    }
                },
            );
            if let Some(k) = select {
                if e.selected != Some(k) {
                    editing = None;
                }
                e.selected = Some(k);
            }
            if let Some(cell) = start_edit {
                editing = Some(cell);
                let key = match cell {
                    (0, _) => "filter_match".to_string(),
                    (n, 1) => format!("filter_param_{}", n - 1),
                    (n, _) => format!("filter_op_{}", n - 1),
                };
                super::super::cell_combo::set_open(c, &key, true);
            }
            let sel = e.selected.filter(|s| *s < e.filter.rows.len());
            // The move pad: left and right take a row out of or into the
            // group above, up and down move it.
            let can_left = sel.is_some_and(|s| e.filter.rows[s].level > 0);
            let can_right =
                sel.is_some_and(|s| e.filter.rows[s].level < max_level(&e.filter.rows, s));
            let can_up = sel.is_some_and(|s| s > 0);
            let can_down = sel.is_some_and(|s| s + 1 < e.filter.rows.len());
            let entries = [
                Entry::ok("filter_ok"),
                Entry::cancel("filter_cancel"),
                Entry::Button("filter_add", "Add", true, true),
                Entry::Button("filter_delete", "Delete", sel.is_some(), false),
                Entry::Button("filter_cut", "Cut", sel.is_some(), false),
                Entry::Button("filter_copy", "Copy", sel.is_some(), false),
                Entry::Button("filter_paste", "Paste", has_clip, false),
                Entry::Pad("filter", [can_left, can_up, can_down, can_right]),
                Entry::help("filter_help"),
            ];
            let out = kit::button_column(c, CLIENT.x, &entries);
            clicks.extend(clicked_key(&entries, out.clicked.as_deref()));
            clicks.extend(out.moved.map(|m| match m {
                kit::Move::Left => "filter_left",
                kit::Move::Up => "filter_up",
                kit::Move::Down => "filter_down",
                kit::Move::Right => "filter_right",
            }));
        });
        self.record_dialog_actions(actions);
        let mut done = closed.then_some(false);
        let rows = &mut e.filter.rows;
        for key in clicks {
            let sel = e.selected.filter(|s| *s < rows.len());
            match key {
                "filter_ok" => done = Some(true),
                "filter_cancel" => done = Some(false),
                "filter_add" => {
                    let (at, level) = match sel {
                        Some(s) if rows[s].group().is_some() => {
                            (s + 1, (rows[s].level + 1).min(MAX_LEVEL))
                        }
                        Some(s) => (subtree(rows, s).end, rows[s].level),
                        None => (rows.len(), 0),
                    };
                    rows.insert(
                        at,
                        Criterion {
                            level,
                            ..Default::default()
                        },
                    );
                    e.selected = Some(at);
                    editing = None;
                }
                "filter_delete" | "filter_cut" => {
                    if let Some(s) = sel {
                        let range = subtree(rows, s);
                        let cut: Vec<Criterion> = rows.drain(range).collect();
                        if key == "filter_cut" {
                            self.grid.dialogs.filter_clip = cut;
                        }
                        e.selected = None;
                        editing = None;
                    }
                }
                "filter_copy" => {
                    if let Some(s) = sel {
                        self.grid.dialogs.filter_clip = rows[subtree(rows, s)].to_vec();
                    }
                }
                "filter_paste" => {
                    let clip = self.grid.dialogs.filter_clip.clone();
                    if let Some(first) = clip.first() {
                        let (at, level) = match sel {
                            Some(s) => (subtree(rows, s).end, rows[s].level),
                            None => (rows.len(), 0),
                        };
                        let base = first.level;
                        for (n, mut r) in clip.into_iter().enumerate() {
                            r.level = (level + r.level.saturating_sub(base)).min(MAX_LEVEL);
                            rows.insert(at + n, r);
                        }
                        e.selected = Some(at);
                    }
                }
                "filter_left" | "filter_right" => {
                    if let Some(s) = sel {
                        let range = subtree(rows, s);
                        let right = key == "filter_right";
                        if right && rows[s].level < max_level(rows, s) {
                            for r in &mut rows[range] {
                                r.level = (r.level + 1).min(MAX_LEVEL);
                            }
                        } else if !right && rows[s].level > 0 {
                            for r in &mut rows[range] {
                                r.level = r.level.saturating_sub(1);
                            }
                        }
                    }
                }
                "filter_up" => {
                    if let Some(s) = sel.filter(|s| *s > 0) {
                        rows.swap(s - 1, s);
                        e.selected = Some(s - 1);
                    }
                }
                "filter_down" => {
                    if let Some(s) = sel.filter(|s| s + 1 < rows.len()) {
                        rows.swap(s, s + 1);
                        e.selected = Some(s + 1);
                    }
                }
                "filter_help" => self.open_help_for("grid_filter_editor"),
                _ => {}
            }
        }
        if done.is_none() && self.escape(ctx, "filter_editor") {
            done = Some(false);
        }
        match done {
            Some(true) => match self.check_filter(&e, &catalog) {
                Ok(f) => {
                    editing = None;
                    let key = f.key();
                    match self.grid.dialogs.filters.as_mut() {
                        Some(d) => {
                            if let Some(old) = &e.original {
                                let old_key = format!("user:{old}");
                                for k in d.selection.selected.iter_mut() {
                                    if *k == old_key {
                                        *k = key.clone();
                                    }
                                }
                                d.users.retain(|u| &u.name != old);
                            }
                            d.users.push(f);
                            d.current = Some(key);
                        }
                        None => {
                            if let Some(old) = &e.original {
                                self.grid.user_filters.retain(|u| &u.name != old);
                            }
                            self.grid.user_filters.push(f);
                            self.save_user_filters();
                        }
                    }
                }
                Err(err) => {
                    self.error(err);
                    self.grid.dialogs.filter_editor = Some(e);
                }
            },
            Some(false) => editing = None,
            None => self.grid.dialogs.filter_editor = Some(e),
        }
        self.grid.dialogs.filter_editing = editing;
    }

    /// Validate an edited filter: a unique name and values that read as the
    /// parameter's type (dates also as relative dates).
    fn check_filter(&self, e: &FilterEditor, catalog: &[Column]) -> Result<UserFilter, String> {
        let mut f = e.filter.clone();
        f.name = f.name.trim().to_string();
        if f.name.is_empty() {
            return Err("Enter a filter name.".into());
        }
        let users = self
            .grid
            .dialogs
            .filters
            .as_ref()
            .map_or(&self.grid.user_filters, |d| &d.users);
        if users
            .iter()
            .any(|u| u.name == f.name && Some(&u.name) != e.original.as_ref())
        {
            return Err(format!("A filter named {} already exists.", f.name));
        }
        for row in &f.rows {
            let Some(c) = columns::find(catalog, &row.column) else {
                continue;
            };
            if matches!(row.op, Op::Contains | Op::NotContains | Op::StartsWith) {
                continue;
            }
            let mut values = vec![&row.value];
            if row.op.uses_high() {
                values.push(&row.high);
            }
            for v in values {
                filters::check_value(&self.project, c.kind, v)
                    .map_err(|err| format!("{}: {err}", c.title))?;
            }
        }
        f.rows.retain(|r| !r.column.is_empty());
        Ok(f)
    }
}

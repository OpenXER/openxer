//! Drawing for the Activity Details page at fixed sizes: the pane frame and
//! flat tab strip, the header row, sunken fields, combos, check boxes, section boxes
//! and flat command buttons. Coordinates are logical pixels at 96 DPI.
use super::*;
use chrome::pal;

/// Margin around the page control.
pub const MARGIN: Color32 = pal::VIEW_MARGIN;
/// The page control's outer frame and the tab outlines.
pub const FRAME: Color32 = Color32::from_rgb(100, 100, 100);
/// Inner top/left line of a sunken field.
pub const DARK: Color32 = Color32::from_rgb(105, 105, 105);
pub const SHADOW: Color32 = pal::SHADOW;
pub const LIGHT: Color32 = pal::LIGHT;
pub const WHITE: Color32 = Color32::WHITE;
pub const FACE: Color32 = pal::FACE;
pub const DISABLED_TEXT: Color32 = pal::TEXT_DISABLED;
/// Fill of read-only fields (Project, Responsible Manager, floats).
pub const READONLY: Color32 = Color32::from_rgb(248, 248, 248);

pub const SPLITTER_H: f32 = 6.;
pub const MARGIN_W: f32 = 4.;
pub const TAB_H: f32 = 21.;
/// Content inset from the page frame's outer edge.
pub const PAD: f32 = 7.;
/// Content top below the page frame's top edge (under the header row).
pub const CONTENT_TOP: f32 = 33.;
pub const FIELD_H: f32 = 21.;
pub const BUTTON_H: f32 = 21.;
/// Gap between panels, grids and the button row.
pub const GAP: f32 = 5.;
/// Height a button row takes under a grid: the gap and the button.
pub const BUTTON_BAR: f32 = GAP + BUTTON_H;

/// Tab widths from divider to divider, wide enough for each caption with
/// padding (renamed tabs size to their caption).
const TAB_WIDTHS: [(&str, f32); 10] = [
    ("General", 48.),
    ("Status", 41.),
    ("Resources", 63.),
    ("Codes", 41.),
    ("Relationships", 74.),
    ("Notebook", 55.),
    ("Steps", 38.),
    ("Risks", 36.),
    ("Expenses", 58.),
    ("Summary", 55.),
];

pub fn font() -> FontId {
    chrome::table_font()
}

/// Divider-to-divider width of a tab: measured, or the caption plus padding.
pub fn tab_width(ui: &egui::Ui, caption: &str) -> f32 {
    TAB_WIDTHS
        .iter()
        .find(|(c, _)| *c == caption)
        .map(|(_, w)| *w)
        .unwrap_or_else(|| text_width(ui, caption).round() + 9.)
}

pub fn text_width(ui: &egui::Ui, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_string(), font(), INK)
        .size()
        .x
}

pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
}

pub fn fill(p: &egui::Painter, r: Rect, color: Color32) {
    chrome::fill(p, r, color);
}

/// 1 px lowered edge: shadow top/left, white bottom/right.
pub fn lowered(p: &egui::Painter, r: Rect) {
    chrome::hline(p, r.left(), r.right(), r.top(), SHADOW);
    chrome::vline(p, r.left(), r.top(), r.bottom(), SHADOW);
    chrome::hline(p, r.left(), r.right(), r.bottom() - 1., WHITE);
    chrome::vline(p, r.right() - 1., r.top(), r.bottom(), WHITE);
}

/// 1 px raised edge: white top/left, shadow bottom/right.
pub fn raised(p: &egui::Painter, r: Rect) {
    chrome::hline(p, r.left(), r.right(), r.top(), WHITE);
    chrome::vline(p, r.left(), r.top(), r.bottom(), WHITE);
    chrome::hline(p, r.left(), r.right(), r.bottom() - 1., SHADOW);
    chrome::vline(p, r.right() - 1., r.top(), r.bottom(), SHADOW);
}

/// The 2 px sunken border of an edit field around `fill_color`.
pub fn sunken(p: &egui::Painter, r: Rect, fill_color: Color32) {
    fill(p, r, fill_color);
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    chrome::hline(p, l, rt, t, SHADOW);
    chrome::vline(p, l, t, b, SHADOW);
    chrome::hline(p, l + 1., rt - 1., t + 1., DARK);
    chrome::vline(p, l + 1., t + 1., b - 1., DARK);
    chrome::hline(p, l, rt, b - 1., WHITE);
    chrome::vline(p, rt - 1., t, b, WHITE);
    chrome::hline(p, l + 1., rt - 1., b - 2., LIGHT);
    chrome::vline(p, rt - 2., t + 1., b - 1., LIGHT);
}

/// Galley top that puts Arial 8 pt text on the reference baseline inside a
/// control `h` pixels tall at `top`.
pub fn text_top(top: f32, h: f32) -> f32 {
    top + ((h - 13.) / 2.).floor() + 1.
}

/// Text at `x` vertically placed as in a control at `top` of height `h`.
pub fn text(p: &egui::Painter, x: f32, top: f32, h: f32, s: &str, color: Color32) {
    p.text(
        Pos2::new(x, text_top(top, h)),
        Align2::LEFT_TOP,
        s,
        font(),
        color,
    );
}

/// Right-aligned text ending at `right`.
pub fn text_right(p: &egui::Painter, right: f32, top: f32, h: f32, s: &str, color: Color32) {
    p.text(
        Pos2::new(right, text_top(top, h)),
        Align2::RIGHT_TOP,
        s,
        font(),
        color,
    );
}

/// A label for a field whose top is `top`, starting at `x`: one pixel
/// higher than text inside the field.
pub fn label(p: &egui::Painter, x: f32, top: f32, s: &str, enabled: bool) {
    text(
        p,
        x,
        top - 1.,
        FIELD_H,
        s,
        if enabled { INK } else { DISABLED_TEXT },
    );
}

/// A caption placed above a field (General tab): cap line 13 px above
/// the field.
pub fn above(p: &egui::Painter, x: f32, field_top: f32, s: &str) {
    p.text(
        Pos2::new(x, field_top - 14.),
        Align2::LEFT_TOP,
        s,
        font(),
        INK,
    );
}

/// Read-only value in a sunken field.
pub fn value(p: &egui::Painter, r: Rect, s: &str, right: bool, fill_color: Color32) {
    sunken(p, r, fill_color);
    let clip = r.shrink(2.);
    let p = p.with_clip_rect(clip.intersect(p.clip_rect()));
    if right {
        text_right(&p, r.right() - 6., r.top(), r.height(), s, INK);
    } else {
        text(&p, r.left() + 2., r.top(), r.height(), s, INK);
    }
}

/// An editable single-line field in a sunken frame whose text stops
/// `right` pixels before the field's right edge (6, or more for fields with
/// a `...` button).
pub fn edit_with(
    ui: &mut egui::Ui,
    id: egui::Id,
    r: Rect,
    value: &mut String,
    enabled: bool,
    align: Align,
    right: f32,
) -> egui::Response {
    sunken(ui.painter(), r, if enabled { WHITE } else { READONLY });
    let inner = Rect::from_min_max(
        Pos2::new(r.left() + 2., r.top() + 2.),
        Pos2::new(r.right() - right, r.bottom() - 2.),
    );
    let edit = egui::TextEdit::singleline(value)
        .id(id)
        .frame(false)
        .font(font())
        .text_color(INK)
        .horizontal_align(align)
        .vertical_align(Align::Center)
        .margin(egui::Margin::ZERO)
        .desired_width(inner.width());
    ui.put(inner, edit.interactive(enabled))
}

/// An edit that commits on Enter or focus loss (durations, dates,
/// numbers): shows `shown` until the user types, then returns the typed
/// text once when it differs.
pub fn commit_edit(
    ui: &mut egui::Ui,
    id: egui::Id,
    r: Rect,
    shown: &str,
    enabled: bool,
    align: Align,
    right: f32,
) -> (egui::Response, Option<String>) {
    let draft_id = id.with("draft");
    let draft: Option<(String, String)> = ui.ctx().data(|d| d.get_temp(draft_id));
    let mut text = match draft {
        Some((base, typed)) if base == shown => typed,
        _ => shown.to_string(),
    };
    let response = edit_with(ui, id, r, &mut text, enabled, align, right);
    crate::dialog_keys::note_keeps_enter(&response);
    let submit = response.lost_focus()
        || response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if submit {
        ui.ctx()
            .data_mut(|d| d.remove::<(String, String)>(draft_id));
        let changed = (text != shown).then_some(text);
        return (response, changed);
    }
    if response.has_focus() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(draft_id, (shown.to_string(), text)));
    }
    (response, None)
}

/// The raised box of a combo drop button or a `...` button, docked inside
/// the right edge of field `r`.
fn field_button_rect(r: Rect, combo: bool) -> Rect {
    if combo {
        Rect::from_min_max(
            Pos2::new(r.right() - 19., r.top() + 2.),
            Pos2::new(r.right() - 2., r.bottom() - 2.),
        )
    } else {
        Rect::from_min_max(
            Pos2::new(r.right() - 18., r.top() + 3.),
            Pos2::new(r.right() - 2., r.bottom() - 2.),
        )
    }
}

/// Classic combo drop button: light and white top/left, shadow and dark
/// bottom/right, a 7 px black arrow.
pub fn drop_button(p: &egui::Painter, field: Rect, enabled: bool) -> Rect {
    let b = field_button_rect(field, true);
    fill(p, b, FACE);
    chrome::hline(p, b.left(), b.right(), b.top(), LIGHT);
    chrome::vline(p, b.left(), b.top(), b.bottom(), LIGHT);
    chrome::hline(p, b.left() + 1., b.right() - 1., b.top() + 1., WHITE);
    chrome::vline(p, b.left() + 1., b.top() + 1., b.bottom() - 1., WHITE);
    chrome::hline(p, b.left(), b.right(), b.bottom() - 1., DARK);
    chrome::vline(p, b.right() - 1., b.top(), b.bottom(), DARK);
    chrome::hline(p, b.left() + 1., b.right() - 1., b.bottom() - 2., SHADOW);
    chrome::vline(p, b.right() - 2., b.top() + 1., b.bottom() - 1., SHADOW);
    let color = if enabled { INK } else { DISABLED_TEXT };
    let (x, y) = (b.left() + 5., b.top() + 7.);
    for row in 0..4 {
        let r = row as f32;
        chrome::hline(p, x + r, x + 7. - r, y + r, color);
    }
    b
}

/// The `...` button inside a picker or date field.
pub fn ellipsis_button(p: &egui::Painter, field: Rect, enabled: bool) -> Rect {
    let b = field_button_rect(field, false);
    fill(p, b, FACE);
    chrome::hline(p, b.left(), b.right(), b.top(), LIGHT);
    chrome::vline(p, b.left(), b.top(), b.bottom(), LIGHT);
    chrome::hline(p, b.left(), b.right(), b.bottom() - 1., DARK);
    chrome::vline(p, b.right() - 1., b.top(), b.bottom(), DARK);
    chrome::hline(p, b.left() + 1., b.right() - 1., b.bottom() - 2., SHADOW);
    chrome::vline(p, b.right() - 2., b.top() + 1., b.bottom() - 1., SHADOW);
    let color = if enabled { INK } else { DISABLED_TEXT };
    for dx in [4., 7., 10.] {
        fill(p, rect(b.left() + dx, b.top() + 11., 1., 1.), color);
    }
    b
}

/// Single-click calendar drop-down behind a date field's `...` face.
/// Hovering the button or any day/navigation button shows a pointing cursor
/// (never the text I-beam), and clicking a date immediately selects it and
/// closes the popup with no separate Save step.
pub fn calendar_drop_down(
    ui: &mut egui::Ui,
    button: Rect,
    id_salt: &str,
    day: &mut chrono::NaiveDate,
) -> egui::Response {
    use chrono::Datelike;

    let id = ui.make_persistent_id(("openxer_calendar_popup", id_salt));
    let open_id = id.with("open");
    let view_id = id.with("view");
    let mut response = ui.interact(button, id.with("btn"), Sense::click());
    if response.hovered() || ui.rect_contains_pointer(button) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let mut open = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    let mut view: (i32, u32) = ui
        .ctx()
        .data(|d| d.get_temp(view_id))
        .unwrap_or((day.year(), day.month()));

    if response.clicked() {
        open = !open;
        if open {
            view = (day.year(), day.month());
        }
    }

    if open {
        const COL_W: f32 = 26.;
        const ROW_H: f32 = 20.;
        const PAD: f32 = 6.;
        const HEAD_H: f32 = 22.;
        const WDAY_H: f32 = 18.;
        const FOOT_H: f32 = 20.;

        let (year, month) = view;
        let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(*day);
        let next_month_first = if month == 12 {
            chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
        }
        .unwrap_or(first);
        let days_in_month = (next_month_first - first).num_days().max(28) as u32;
        let start_col = first.weekday().num_days_from_sunday();
        let grid_rows = (start_col + days_in_month).div_ceil(7).max(5);

        let popup_w = PAD * 2. + COL_W * 7.;
        let popup_h = PAD * 2. + HEAD_H + WDAY_H + ROW_H * (grid_rows as f32) + 4. + FOOT_H;
        let screen = ui.ctx().content_rect();
        let px = button
            .left()
            .min(screen.right() - popup_w - 4.)
            .max(screen.left() + 4.);
        let py = if button.bottom() + 2. + popup_h <= screen.bottom() {
            button.bottom() + 2.
        } else {
            (button.top() - 2. - popup_h).max(screen.top() + 4.)
        };

        let mut popup_rect = Rect::from_min_size(Pos2::new(px, py), Vec2::new(popup_w, popup_h));
        let clicked_inside = std::cell::Cell::new(false);
        let mut next_view = view;
        let mut picked_date: Option<chrono::NaiveDate> = None;

        let area_resp = egui::Area::new(id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(px, py))
            .interactable(true)
            .show(ui.ctx(), |ui| {
                let (rect, area_sense) =
                    ui.allocate_exact_size(Vec2::new(popup_w, popup_h), Sense::click());
                popup_rect = rect;
                if area_sense.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
                let p = ui.painter();
                p.rect_filled(rect, 0., WHITE);
                p.rect_stroke(
                    rect.shrink(0.5),
                    0.,
                    Stroke::new(1_f32, DARK),
                    egui::StrokeKind::Middle,
                );

                // Header bar: << < Month Year > >>, or < Month Year >
                calendar_header_and_days(
                    ui,
                    id,
                    rect,
                    year,
                    month,
                    *day,
                    true,
                    true,
                    "nav",
                    &mut |delta_months| {
                        clicked_inside.set(true);
                        let total = year * 12 + (month as i32 - 1) + delta_months;
                        let ny = total.div_euclid(12).clamp(1900, 2200);
                        let nm = (total.rem_euclid(12) + 1) as u32;
                        next_view = (ny, nm);
                    },
                    &mut |d| {
                        clicked_inside.set(true);
                        picked_date = Some(d);
                    },
                    &mut Vec::new(),
                );
            });

        view = next_view;
        if let Some(d) = picked_date {
            *day = d;
            response.mark_changed();
            open = false;
        } else if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        } else if !response.clicked()
            && !clicked_inside.get()
            && area_resp.response.clicked_elsewhere()
        {
            let ptr = ui.ctx().input(|i| i.pointer.interact_pos());
            if ptr.is_none_or(|p| !popup_rect.contains(p) && !button.contains(p)) {
                open = false;
            }
        }
    }

    ui.ctx().data_mut(|d| {
        d.insert_temp(open_id, open);
        d.insert_temp(view_id, view);
    });
    response
}

/// Shared calendar popup grid (month/year nav, weekday header, Sunday-first
/// day matrix, optional Today link) used by `calendar_drop_down` and the
/// table cell date editor.
#[allow(clippy::too_many_arguments)]
pub fn calendar_header_and_days(
    ui: &mut egui::Ui,
    id: egui::Id,
    rect: Rect,
    year: i32,
    month: u32,
    selected: chrono::NaiveDate,
    year_nav: bool,
    show_today: bool,
    action_prefix: &str,
    on_shift: &mut dyn FnMut(i32),
    on_pick: &mut dyn FnMut(chrono::NaiveDate),
    actions: &mut Vec<(String, Rect)>,
) {
    use chrono::Datelike;
    const COL_W: f32 = 26.;
    const ROW_H: f32 = 20.;
    const PAD: f32 = 6.;
    const HEAD_H: f32 = 22.;
    const WDAY_H: f32 = 18.;
    const FOOT_H: f32 = 20.;
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const DAYS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

    let p = ui.painter();
    let head_rect = Rect::from_min_size(
        Pos2::new(rect.left() + PAD, rect.top() + PAD),
        Vec2::new(COL_W * 7., HEAD_H),
    );
    p.rect_filled(head_rect, 0., FACE);
    let mut nav: Vec<(&str, &str, f32, i32)> = vec![
        (
            "prev",
            "<",
            head_rect.left() + if year_nav { 24. } else { 2. },
            -1,
        ),
        (
            "next",
            ">",
            head_rect.right() - if year_nav { 42. } else { 22. },
            1,
        ),
    ];
    if year_nav {
        nav.push(("prev_year", "<<", head_rect.left() + 2., -12));
        nav.push(("next_year", ">>", head_rect.right() - 22., 12));
    }
    for (name, sym, nx, delta_months) in nav {
        let nr = Rect::from_min_size(
            Pos2::new(nx, head_rect.top() + 2.),
            Vec2::new(20., HEAD_H - 4.),
        );
        let nr_resp = ui.interact(nr, id.with((action_prefix, name)), Sense::click());
        if nr_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            p.rect_filled(nr, 0., pal::FOCUS_INNER);
        }
        p.text(
            nr.center(),
            Align2::CENTER_CENTER,
            sym,
            crate::dialog_kit::body_font(),
            INK,
        );
        if nr_resp.clicked() {
            on_shift(delta_months);
        }
        actions.push((format!("{action_prefix}_{name}"), nr));
    }
    let title = format!(
        "{} {}",
        MONTHS[(month.saturating_sub(1) as usize).min(11)],
        year
    );
    p.text(
        head_rect.center(),
        Align2::CENTER_CENTER,
        title,
        chrome::bold_font(),
        INK,
    );

    let wday_y = head_rect.bottom();
    for (col, label) in DAYS.iter().enumerate() {
        let dr = Rect::from_min_size(
            Pos2::new(rect.left() + PAD + (col as f32) * COL_W, wday_y),
            Vec2::new(COL_W, WDAY_H),
        );
        p.text(
            dr.center(),
            Align2::CENTER_CENTER,
            *label,
            crate::dialog_kit::body_font(),
            DISABLED_TEXT,
        );
    }

    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(selected);
    let next_month_first = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap_or(first);
    let days_in_month = (next_month_first - first).num_days().max(28) as u32;
    let start_col = first.weekday().num_days_from_sunday();
    let grid_rows = (start_col + days_in_month).div_ceil(7).max(5);
    let grid_y = wday_y + WDAY_H;
    let today = chrono::Local::now().date_naive();
    let lead = start_col as i64;
    let mut day = first - chrono::Duration::days(lead);
    let cells = grid_rows * 7;
    for idx in 0..cells {
        let col = idx % 7;
        let row = idx / 7;
        let cell = Rect::from_min_size(
            Pos2::new(
                rect.left() + PAD + (col as f32) * COL_W,
                grid_y + (row as f32) * ROW_H,
            ),
            Vec2::new(COL_W, ROW_H),
        );
        let in_month = day.month() == first.month();
        let key = format!("{action_prefix}_{}", day.format("%Y-%m-%d"));
        let c_resp = ui.interact(cell, id.with(&key), Sense::click());
        let is_sel = day == selected;
        if c_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if is_sel {
            p.rect_filled(cell.shrink(1.), 0., SELECT);
        } else if c_resp.hovered() {
            p.rect_filled(cell.shrink(1.), 0., pal::FOCUS_INNER);
        }
        if day == today && !is_sel {
            p.rect_stroke(
                cell.shrink(1.5),
                0.,
                Stroke::new(1_f32, SELECT),
                egui::StrokeKind::Middle,
            );
        }
        let color = if is_sel {
            WHITE
        } else if in_month {
            INK
        } else {
            DISABLED_TEXT
        };
        p.text(
            cell.center(),
            Align2::CENTER_CENTER,
            day.day().to_string(),
            crate::dialog_kit::body_font(),
            color,
        );
        if c_resp.clicked() {
            on_pick(day);
        }
        actions.push((key, cell));
        day += chrono::Duration::days(1);
    }

    if show_today {
        let foot_y = grid_y + (grid_rows as f32) * ROW_H + 4.;
        let today_rect = Rect::from_min_size(
            Pos2::new(rect.left() + PAD, foot_y),
            Vec2::new(COL_W * 7., FOOT_H),
        );
        let t_resp = ui.interact(
            today_rect,
            id.with((action_prefix, "today")),
            Sense::click(),
        );
        if t_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            p.rect_filled(today_rect.shrink(1.), 0., pal::FOCUS_INNER);
        }
        p.text(
            today_rect.center(),
            Align2::CENTER_CENTER,
            "Today",
            crate::dialog_kit::body_font(),
            SELECT,
        );
        if t_resp.clicked() {
            on_pick(today);
        }
    }
}

/// A drop-down list over `items`. Returns the picked index when it changed.
pub fn combo(
    ui: &mut egui::Ui,
    id: egui::Id,
    r: Rect,
    items: &[String],
    selected: Option<usize>,
    enabled: bool,
) -> (egui::Response, Option<usize>) {
    let response = ui.interact(
        r,
        id,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let p = ui.painter();
    sunken(p, r, if enabled { WHITE } else { FACE });
    let button = drop_button(p, r, enabled);
    if let Some(s) = selected.and_then(|i| items.get(i)) {
        let clip = Rect::from_min_max(
            r.min + Vec2::splat(2.),
            Pos2::new(button.left(), r.bottom()),
        );
        text(
            &p.with_clip_rect(clip.intersect(p.clip_rect())),
            r.left() + 4.,
            r.top() + 1.,
            FIELD_H,
            s,
            if enabled { INK } else { DISABLED_TEXT },
        );
    }
    let picked = popup_list(&response, r.width(), items, selected);
    (response, picked)
}

/// The drop-down list of a combo or picker: one row per item.
fn popup_list(
    response: &egui::Response,
    width: f32,
    items: &[String],
    selected: Option<usize>,
) -> Option<usize> {
    let mut picked = None;
    egui::Popup::from_toggle_button_response(response)
        .width(width)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            egui::ScrollArea::vertical()
                .max_height(300.)
                .show(ui, |ui| {
                    for (i, item) in items.iter().enumerate() {
                        let row = ui.add(
                            egui::Button::selectable(selected == Some(i), item.as_str())
                                .min_size(Vec2::new(width - 8., 15.)),
                        );
                        if row.clicked() && selected != Some(i) {
                            picked = Some(i);
                        }
                    }
                });
        });
    picked
}

/// A picker field: icon, the current value and a `...` button; a click
/// drops down the choices.
pub fn picker(
    ui: &mut egui::Ui,
    id: egui::Id,
    r: Rect,
    icon: chrome::Icon,
    items: &[String],
    selected: Option<usize>,
    enabled: bool,
) -> (egui::Response, Option<usize>) {
    let shown = selected
        .and_then(|i| items.get(i))
        .map_or("", String::as_str);
    let response = picker_field(ui, id, r, icon, shown, enabled);
    let picked = popup_list(&response, r.width(), items, selected);
    (response, picked)
}

/// A picker field whose click opens a window (Select WBS): the look of
/// [`picker`] without its drop-down. The caller opens the window when the
/// response is clicked.
pub fn picker_field(
    ui: &mut egui::Ui,
    id: egui::Id,
    r: Rect,
    icon: chrome::Icon,
    shown: &str,
    enabled: bool,
) -> egui::Response {
    let response = ui.interact(
        r,
        id,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let p = ui.painter();
    sunken(p, r, if enabled { WHITE } else { READONLY });
    let button = ellipsis_button(p, r, enabled);
    if !shown.is_empty() {
        chrome::paint_icon_at(p, icon, Pos2::new(r.left() + 2., r.top() + 2.), enabled);
        let clip = Rect::from_min_max(
            r.min + Vec2::splat(2.),
            Pos2::new(button.left(), r.bottom()),
        );
        text(
            &p.with_clip_rect(clip.intersect(p.clip_rect())),
            r.left() + 21.,
            r.top() - 1.,
            FIELD_H,
            shown,
            INK,
        );
    }
    response
}

/// A 13 x 13 classic check box with its caption 19 px to the right.
pub fn checkbox(
    ui: &mut egui::Ui,
    id: egui::Id,
    x: f32,
    top: f32,
    caption: &str,
    value: &mut bool,
    enabled: bool,
) -> egui::Response {
    let width = 19. + text_width(ui, caption) + 4.;
    let hit = rect(x, top, width, FIELD_H);
    let mut response = ui.interact(
        hit,
        id,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let p = ui.painter();
    let b = rect(x, top + 3., 13., 13.);
    sunken(p, b, if enabled { WHITE } else { FACE });
    if *value {
        let ink = if enabled { INK } else { DISABLED_TEXT };
        for (i, dy) in [2., 3., 4., 3., 2., 1., 0.].iter().enumerate() {
            fill(
                p,
                rect(b.left() + 3. + i as f32, b.top() + 3. + dy, 1., 3.),
                ink,
            );
        }
    }
    label(p, x + 19., top, caption, enabled);
    response
}

/// A 12 x 12 radio button with its caption.
pub fn radio(
    ui: &mut egui::Ui,
    id: egui::Id,
    x: f32,
    top: f32,
    caption: &str,
    selected: bool,
) -> egui::Response {
    let width = 17. + text_width(ui, caption) + 4.;
    let response = ui.interact(rect(x, top, width, FIELD_H), id, Sense::click());
    const RADIO: [&str; 12] = [
        "....SSSS....",
        "..SSDDDDSS..",
        ".SDDWWWWDDH.",
        ".SDWWWWWWLH.",
        "SDWWWKKWWWLH",
        "SDWWKKKKWWLH",
        "SDWWKKKKWWLH",
        "SDWWWKKWWWLH",
        ".SDWWWWWWLH.",
        ".SLLWWWWLLH.",
        "..HHLLLLHH..",
        "....HHHH....",
    ];
    let p = ui.painter();
    let top_left = Pos2::new(x, top + 4.);
    for (row, line) in RADIO.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let color = match ch {
                'S' => SHADOW,
                'D' => DARK,
                'L' => LIGHT,
                'H' | 'W' => WHITE,
                'K' if selected => INK,
                'K' => WHITE,
                _ => continue,
            };
            fill(
                p,
                rect(top_left.x + col as f32, top_left.y + row as f32, 1., 1.),
                color,
            );
        }
    }
    label(p, x + 17., top, caption, true);
    response
}

/// A section box: lowered frame with a raised caption bar `bar` pixels tall
/// (0 for none), its caption bold, `caption_x` in from the box's left edge.
/// Returns the caption bar.
pub fn section(p: &egui::Painter, r: Rect, caption: &str, bar: f32, caption_x: f32) -> Rect {
    lowered(p, r);
    let b = Rect::from_min_size(r.min + Vec2::splat(1.), Vec2::new(r.width() - 2., bar));
    if bar > 0. {
        raised(p, b);
        p.text(
            Pos2::new(r.left() + caption_x, b.top() + 5.),
            Align2::LEFT_TOP,
            caption,
            chrome::bold_font(),
            INK,
        );
    }
    b
}

/// The open "v" before a collapsible section caption: 9 x 5 at `min`.
pub fn section_chevron(p: &egui::Painter, min: Pos2) {
    let pen = Stroke::new(1_f32, INK);
    let (a, b, c) = (
        min + Vec2::new(0.5, 0.5),
        min + Vec2::new(4.5, 4.5),
        min + Vec2::new(8.5, 0.5),
    );
    p.line_segment([a, b], pen);
    p.line_segment([b, c], pen);
}

/// A vertical splitter between two panels: shadow, white, face, face,
/// shadow, shadow.
pub fn vsplitter(p: &egui::Painter, x: f32, top: f32, bottom: f32) {
    for (dx, color) in [(0., SHADOW), (1., WHITE), (4., SHADOW), (5., SHADOW)] {
        chrome::vline(p, x + dx, top, bottom, color);
    }
    fill(
        p,
        Rect::from_min_max(Pos2::new(x + 2., top), Pos2::new(x + 4., bottom)),
        FACE,
    );
}

/// The icon a command button carries, by caption.
fn command_icon(caption: &str) -> Option<chrome::Icon> {
    use chrome::Icon;
    Some(match caption {
        "Add" | "Add from Template" => Icon::Add,
        "Add Resource" => Icon::AssignResource,
        "Add Role" => Icon::AssignRole,
        "Assign by Role" => Icon::AssignByRole,
        "Assign" => Icon::AssignCode,
        "Remove" | "Delete" => Icon::Delete,
        "GoTo" => Icon::Links,
        "Modify" | "Details" => Icon::Details,
        "Print" => Icon::Print,
        "Copy" => Icon::Copy,
        "Details..." => Icon::Details,
        "^" => Icon::ShiftUp,
        "V" => Icon::ShiftDown,
        _ => return None,
    })
}

/// Measured widths of the pane's command buttons.
fn command_width(ui: &egui::Ui, caption: &str) -> f32 {
    match caption {
        "Add Resource" | "Assign by Role" => 123.,
        "Add from Template" => 147.,
        "Add Role" => 101.,
        "^" | "V" => 40.,
        _ => (text_width(ui, caption).round() + 44.).max(84.),
    }
}

/// A flat 21 px command button: raised 1 px edge, icon 7 px in, caption
/// centered in the rest. Adds itself to the current layout.
pub fn command_button(
    ui: &mut egui::Ui,
    caption: &str,
    width: Option<f32>,
    enabled: bool,
) -> egui::Response {
    command_button_with(ui, caption, width, enabled, true)
}

/// The "add" commands of the details and record tabs.
pub const ADD_COMMANDS: [&str; 5] = [
    "Add",
    "Assign",
    "Add from Template",
    "Add Resource",
    "Add Role",
];

/// An "add" command of a details or record tab ([`ADD_COMMANDS`]). It is
/// enabled even when there is nothing to pick, as in the reference: pressing
/// it then opens its picker empty. `takes` is false only when the record
/// cannot hold such a row at all (a milestone has no resource assignments),
/// never because the list to pick from is empty.
pub fn add_button(ui: &mut egui::Ui, caption: &str, takes: bool) -> egui::Response {
    debug_assert!(
        ADD_COMMANDS.contains(&caption),
        "{caption} is not an add command"
    );
    command_button(ui, caption, None, takes)
}

/// The caption over a notes or notebook text pane while nothing is picked:
/// `(No Notebook Topic selected)`.
pub fn none_selected(what: &str) -> String {
    format!("(No {what} selected)")
}

/// The buttons under every notes or notebook text pane, with their enabled
/// states: Modify while the text can be edited, Print never (notes print
/// with the layout), Copy while there is text to copy. Activity Details,
/// Project and WBS Details, Resource Notes, Cost Accounts and Page Setup
/// all draw these.
pub fn notes_buttons(editable: bool, has_text: bool) -> [(&'static str, bool); 3] {
    [("Modify", editable), ("Print", false), ("Copy", has_text)]
}

/// Why Print under a notes pane is unavailable.
pub const NOTES_PRINT_HINT: &str = "Print the layout to include notebook text.";

/// A command button with its caption only (the Feedback tab's Add).
pub fn text_button(ui: &mut egui::Ui, caption: &str, width: f32, enabled: bool) -> egui::Response {
    command_button_with(ui, caption, Some(width), enabled, false)
}

fn command_button_with(
    ui: &mut egui::Ui,
    caption: &str,
    width: Option<f32>,
    enabled: bool,
    with_icon: bool,
) -> egui::Response {
    let width = width.unwrap_or_else(|| command_width(ui, caption));
    let height = ui.available_height().clamp(BUTTON_H, 25.);
    let (r, response) = ui.allocate_exact_size(
        Vec2::new(width, height),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, caption));
    let p = ui.painter();
    let pressed = enabled && response.is_pointer_button_down_on();
    fill(
        p,
        r,
        if enabled && response.hovered() && !pressed {
            pal::HOT_FILL
        } else {
            FACE
        },
    );
    if pressed {
        lowered(p, r);
    } else {
        raised(p, r);
    }
    let shift = if pressed { 1. } else { 0. };
    let icon = command_icon(caption).filter(|_| with_icon);
    let arrow = matches!(caption, "^" | "V");
    let mut left = r.left() + 2.;
    if let Some(icon) = icon {
        let x = if arrow {
            r.center().x - 8.
        } else {
            r.left() + 7.
        };
        let y = r.top() + ((r.height() - 16.) / 2.).floor();
        chrome::paint_icon_at(p, icon, Pos2::new(x + shift, y + shift), enabled);
        left = x + 16.;
    }
    if !arrow {
        let galley = p.layout_no_wrap(
            caption.to_string(),
            font(),
            if enabled { INK } else { DISABLED_TEXT },
        );
        let x = ((left + r.right() - galley.size().x) / 2.).round() + shift;
        p.galley(
            Pos2::new(x, text_top(r.top(), r.height()) + shift),
            galley,
            INK,
        );
    }
    response
}

/// Lays out a row of command buttons left to right at the bottom of the
/// current area, 5 px apart.
pub fn button_row<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(GAP - ui.spacing().item_spacing.y);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(GAP, 0.);
        add(ui)
    })
    .inner
}

/// Previous/next activity spinner: two stacked raised buttons with small
/// black arrows. Returns (previous clicked, next clicked).
pub fn spinner(ui: &mut egui::Ui, origin: Pos2) -> (egui::Response, egui::Response) {
    let up = rect(origin.x, origin.y, 62., 12.);
    let down = rect(origin.x, origin.y + 12., 62., 12.);
    let a = ui.interact(up, ui.id().with("detail_previous"), Sense::click());
    let b = ui.interact(down, ui.id().with("detail_next"), Sense::click());
    let p = ui.painter();
    for (r, response, pointing_up) in [(up, &a, true), (down, &b, false)] {
        fill(
            p,
            r,
            if response.hovered() {
                pal::HOT_FILL
            } else {
                FACE
            },
        );
        raised(p, r);
        let cx = r.left() + 30.;
        for row in 0..4 {
            let half = row as f32;
            let y = if pointing_up {
                r.top() + 3. + half
            } else {
                r.top() + 7. - half
            };
            chrome::hline(p, cx - half, cx + half + 1., y, INK);
        }
    }
    (a, b)
}

#[cfg(test)]
mod tests {
    /// The details and record tabs draw their add commands through
    /// [`super::add_button`], so the one rule (enabled even with nothing to
    /// pick) holds on every tab.
    #[test]
    fn add_commands_go_through_add_button() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/details");
        let mut hits = vec![];
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                for caption in super::ADD_COMMANDS {
                    if line.contains(&format!("command_button(ui, \"{caption}\"")) {
                        hits.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "add commands drawn past add_button: {hits:?}"
        );
    }
}

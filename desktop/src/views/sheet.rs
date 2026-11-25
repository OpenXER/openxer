//! Parts shared by the view panes: the view frame and its flat header
//! button (`Layout: ...`, `Display: ...`), the details page control (the
//! shared tab strip over a framed sheet), and sheet controls placed
//! at measured coordinates on top of the dialog canvas.
use super::*;
use crate::schedule_dialogs::{dialog_font, Canvas};

pub const MARGIN: Color32 = Color32::from_rgb(216, 216, 216);
pub const FRAME_DARK: Color32 = Color32::from_rgb(100, 100, 100);
pub const SHADOW: Color32 = Color32::from_rgb(160, 160, 160);
pub const TAB_HEIGHT: f32 = crate::details::form::TAB_H;
/// Sheet origin relative to the page control's top-left: inside the page
/// frame's outer line and white highlight, under the tab strip.
pub const SHEET_OFFSET: Vec2 = Vec2::new(2., 23.);

fn fill(p: &egui::Painter, x: f32, y: f32, w: f32, h: f32, color: Color32) {
    p.rect_filled(
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h)),
        0.,
        color,
    );
}

/// Paints the view margin and the dark/white frame line, returning the
/// content rectangle inside it.
pub fn view_frame(ui: &egui::Ui, outer: Rect, left: f32, top: f32) -> Rect {
    let p = ui.painter();
    p.rect_filled(outer, 0., MARGIN);
    let frame = Rect::from_min_max(
        outer.min + Vec2::new(left, top),
        outer.max - Vec2::new(left, 0.),
    );
    fill(p, frame.left(), frame.top(), frame.width(), 1., FRAME_DARK);
    fill(p, frame.left(), frame.top(), 1., frame.height(), FRAME_DARK);
    fill(
        p,
        frame.left() + 1.,
        frame.top() + 1.,
        frame.width() - 1.,
        1.,
        Color32::WHITE,
    );
    fill(
        p,
        frame.left() + 1.,
        frame.top() + 1.,
        1.,
        frame.height() - 1.,
        Color32::WHITE,
    );
    Rect::from_min_max(frame.min + Vec2::splat(1.), frame.max)
}

/// A view's table frame when the view host draws no layout bar above it
/// (every view but Activities): 5 px margins under the host's top line, the
/// `#646464` frame and its white highlight. Returns the area inside the
/// frame and the top-left of the 17 px header strip, which starts under
/// the highlight (`2-37-view-resources.png`, `2-35-view-wbs.png`).
pub fn table_frame(ui: &egui::Ui, outer: Rect) -> (Rect, Pos2) {
    let inner = view_frame(ui, outer, 5., 5.);
    (inner, inner.min + Vec2::new(0., 1.))
}

/// The flat `v <text>` button over a view table: 16 px face, chevron,
/// text, then a `#A0A0A0` line. Returns the click response.
pub fn header_button(ui: &mut egui::Ui, id: &str, rect: Rect, text: &str) -> egui::Response {
    let response = ui.interact(rect, ui.id().with((id, "header_button")), Sense::click());
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    p.rect_filled(rect, 0., CHROME);
    p.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1_f32, SHADOW),
    );
    let face = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.bottom() - 1.));
    if response.hovered() {
        p.rect_filled(
            Rect::from_min_max(face.min, Pos2::new(face.left() + 220., face.bottom())),
            0.,
            Color32::from_rgb(229, 241, 251),
        );
    }
    // Chevron: a 7 px wide open "v".
    let c = Pos2::new(face.left() + 15.5, face.center().y);
    let pen = Stroke::new(1_f32, INK);
    p.line_segment([c + Vec2::new(-3.5, -2.), c + Vec2::new(0., 1.5)], pen);
    p.line_segment([c + Vec2::new(0., 1.5), c + Vec2::new(3.5, -2.)], pen);
    p.text(
        Pos2::new(face.left() + 23., face.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(FONT_SIZE),
        INK,
    );
    response
}

/// Width of the icon-only command toolbar docked at the right of a view.
pub const SIDE_BAR: f32 = 28.;

/// One 24 x 24 icon button of the right-hand command toolbar; the caption
/// shows as hover text.
pub fn side_button(
    ui: &mut egui::Ui,
    icon: chrome::Icon,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(24.),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if response.hovered() && enabled {
        ui.painter().rect(
            rect,
            0.,
            Color32::from_rgb(229, 241, 251),
            Stroke::new(1_f32, Color32::from_rgb(204, 228, 247)),
            egui::StrokeKind::Inside,
        );
    }
    chrome::paint_icon(
        ui.painter(),
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(16.)),
        enabled,
    );
    if enabled {
        response.on_hover_text(label)
    } else {
        response.on_disabled_hover_text(label)
    }
}

/// The details page control in `area`: the shared tab strip
/// (`crate::tab_strip`, the Activities page look) and the framed sheet
/// under it. Records each tab in view as `{prefix}_tab_{i}`. Returns the
/// clicked tab and the sheet's content rectangle (its top-left is the
/// origin of measured field coordinates).
pub fn page_control(
    ui: &mut egui::Ui,
    prefix: &str,
    area: Rect,
    names: &[&str],
    current: usize,
    actions: &mut BTreeMap<String, [f32; 4]>,
) -> (Option<usize>, Rect) {
    ui.painter()
        .with_clip_rect(area.intersect(ui.clip_rect()))
        .rect_filled(
            Rect::from_min_size(area.min, Vec2::new(area.width(), TAB_HEIGHT)),
            0.,
            MARGIN,
        );
    let page = Rect::from_min_max(Pos2::new(area.left(), area.top() + TAB_HEIGHT), area.max);
    // The first tab in view when the tabs overflow, kept per view.
    let first_id = ui.id().with((prefix, "strip_first"));
    let mut first = ui
        .ctx()
        .data(|d| d.get_temp::<usize>(first_id))
        .unwrap_or(0);
    let strip = crate::tab_strip::show(ui, prefix, page, names, current, &mut first);
    ui.ctx().data_mut(|d| d.insert_temp(first_id, first));
    for (i, response) in &strip.tabs {
        let r = response.rect;
        actions.insert(
            format!("{prefix}_tab_{i}"),
            [r.left(), r.top(), r.right(), r.bottom()],
        );
    }
    let sheet = Rect::from_min_max(area.min + SHEET_OFFSET, area.max - Vec2::splat(2.));
    (strip.clicked, sheet)
}

/// Etched section frame with a bold caption strip, as on the project and
/// WBS details tabs. `caption` may be empty for an untitled frame.
pub fn section(c: &Canvas, x: f32, y: f32, w: f32, h: f32, caption: &str) {
    c.hline(x, y, w - 1., SHADOW);
    c.vline(x, y, h - 1., SHADOW);
    c.hline(x + 1., y + 1., w - 2., Color32::WHITE);
    c.vline(x + 1., y + 1., h - 2., Color32::WHITE);
    c.hline(x, y + h - 1., w, Color32::WHITE);
    c.vline(x + w - 1., y, h, Color32::WHITE);
    c.vline(x + w - 2., y + 1., h - 2., SHADOW);
    if !caption.is_empty() {
        c.bold(x + 8., y + 1., 21., caption);
        c.hline(x + 1., y + 21., w - 3., SHADOW);
        c.hline(x + 1., y + 22., w - 3., Color32::WHITE);
    }
}

/// A caption placed above a field whose top is `y`.
pub fn above(c: &Canvas, x: f32, y: f32, text: &str, enabled: bool) {
    c.label(x, y - 15., 13., text, enabled);
}

/// A read-only value shown in a sunken edit (face colored when `dim`).
pub fn value_box(c: &mut Canvas, key: &str, x: f32, y: f32, w: f32, text: &str, right: bool) {
    let mut text = text.to_string();
    c.edit(
        key,
        x,
        y,
        w,
        21.,
        &mut text,
        false,
        if right { Align::Max } else { Align::Min },
    );
}

/// A labeled text field; returns the new text when the user changed it.
#[allow(clippy::too_many_arguments)]
pub fn text_field(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    caption: &str,
    value: &str,
    enabled: bool,
) -> Option<String> {
    if !caption.is_empty() {
        above(c, x, y, caption, true);
    }
    let mut text = value.to_string();
    let response = c.edit(key, x, y, w, 21., &mut text, enabled, Align::Min);
    (enabled && response.changed()).then_some(text)
}

/// A text field that commits on Enter or focus loss (numbers, dates).
/// Returns the committed text.
#[allow(clippy::too_many_arguments)]
pub fn commit_field(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    value: &str,
    enabled: bool,
    align: Align,
) -> Option<String> {
    commit_cell(c, key, (x, y, w, 21.), value, enabled, align)
}

/// A number box (a count, a float, hours, a factor): [`commit_field`]
/// with the value right-aligned, as every number box in the details panes
/// and dialogs is. Returns the committed text.
#[allow(clippy::too_many_arguments)]
pub fn number_field(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    value: &str,
    enabled: bool,
) -> Option<String> {
    commit_field(c, key, x, y, w, value, enabled, NUMBER_ALIGN)
}

/// [`number_field`] over a value shown in place of a blank one
/// ([`commit_field_over`]).
pub fn number_field_over(
    c: &mut Canvas,
    key: &str,
    (x, y, w): (f32, f32, f32),
    (shown, stored): (&str, &str),
    enabled: bool,
) -> Option<String> {
    commit_field_over(c, key, (x, y, w), (shown, stored), enabled, NUMBER_ALIGN)
}

/// How a number sits in its box: against the right edge.
pub const NUMBER_ALIGN: Align = Align::Max;

/// [`commit_field`] at any height (in-place grid cells).
pub fn commit_cell(
    c: &mut Canvas,
    key: &str,
    rect: (f32, f32, f32, f32),
    value: &str,
    enabled: bool,
    align: Align,
) -> Option<String> {
    commit_cell_over(c, key, rect, (value, value), enabled, align)
}

/// [`commit_field`] over a value shown in place of a blank one (a default
/// in use): `(shown, stored)`. Enter on the shown text commits it, so it
/// can be stored as typed.
#[allow(clippy::too_many_arguments)]
pub fn commit_field_over(
    c: &mut Canvas,
    key: &str,
    (x, y, w): (f32, f32, f32),
    (shown, stored): (&str, &str),
    enabled: bool,
    align: Align,
) -> Option<String> {
    commit_cell_over(c, key, (x, y, w, 21.), (shown, stored), enabled, align)
}

fn commit_cell_over(
    c: &mut Canvas,
    key: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    (value, stored): (&str, &str),
    enabled: bool,
    align: Align,
) -> Option<String> {
    let id = c.id(key).with("draft");
    let ctx = c.ui.ctx().clone();
    let draft: Option<(String, String)> = ctx.data(|d| d.get_temp(id));
    let mut text = match &draft {
        Some((base, text)) if base == value => text.clone(),
        _ => value.to_string(),
    };
    let response = c.edit(key, x, y, w, h, &mut text, enabled, align);
    let enter = c.ui.input(|i| i.key_pressed(Key::Enter));
    let submit = response.lost_focus() || response.has_focus() && enter;
    if submit {
        ctx.data_mut(|d| d.remove::<(String, String)>(id));
        let keep = text == value && text != stored && enter;
        return (text != value || keep).then_some(text);
    }
    if response.has_focus() {
        ctx.data_mut(|d| d.insert_temp(id, (value.to_string(), text)));
    }
    None
}

/// A picker: sunken box with an icon, the current value and `...`. Display
/// only; `enabled` draws the face white.
#[allow(clippy::too_many_arguments)]
pub fn picker(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    icon: Option<chrome::Icon>,
    text: &str,
    enabled: bool,
) {
    let rect = c.r(x, y, w, 21.);
    c.sunken(rect, if enabled { Color32::WHITE } else { CHROME });
    let mut left = rect.left() + 4.;
    if let Some(icon) = icon {
        chrome::paint_icon(
            c.ui.painter(),
            icon,
            Rect::from_min_size(
                Pos2::new(rect.left() + 3., rect.top() + 3.),
                Vec2::splat(15.),
            ),
            enabled,
        );
        left += 18.;
    }
    let clip = Rect::from_min_max(
        Pos2::new(left, rect.top()),
        Pos2::new(rect.right() - 21., rect.bottom()),
    );
    c.ui.painter().with_clip_rect(clip).text(
        Pos2::new(left, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        dialog_font(),
        INK,
    );
    c.act(key, rect);
    let _ = c.ellipsis(&format!("{key}_more"), rect, false);
}

/// A [`picker`] whose `...` button works: true when it was clicked.
#[allow(clippy::too_many_arguments)]
pub fn picker_button(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    icon: Option<chrome::Icon>,
    text: &str,
    enabled: bool,
) -> bool {
    let rect = c.r(x, y, w, 21.);
    c.sunken(rect, if enabled { Color32::WHITE } else { CHROME });
    let mut left = rect.left() + 4.;
    if let Some(icon) = icon {
        chrome::paint_icon(
            c.ui.painter(),
            icon,
            Rect::from_min_size(
                Pos2::new(rect.left() + 3., rect.top() + 3.),
                Vec2::splat(15.),
            ),
            enabled,
        );
        left += 18.;
    }
    let clip = Rect::from_min_max(
        Pos2::new(left, rect.top()),
        Pos2::new(rect.right() - 21., rect.bottom()),
    );
    c.ui.painter().with_clip_rect(clip).text(
        Pos2::new(left, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        dialog_font(),
        INK,
    );
    let face = c.hit(key, rect, enabled);
    let more = c.ellipsis(&format!("{key}_more"), rect, enabled);
    enabled && (more.clicked() || face.double_clicked())
}

/// A date field with a calendar drop-down on its `...` button. Returns the
/// new value (outer `Some`) when the user committed a valid change, or an
/// error message.
#[allow(clippy::too_many_arguments)]
pub fn date_field(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    origin: i64,
    value: Option<i64>,
    fallback: i64,
    enabled: bool,
) -> Result<Option<Option<i64>>, String> {
    let shown = format::date(origin, value);
    let mut result = Ok(None);
    if let Some(text) = commit_field(c, key, x, y, 217., &shown, enabled, Align::Min) {
        result = format::parse_date(origin, &text, value).map(|v| (v != value).then_some(v));
    }
    let button = calendar_button_rect(c.r(x, y, 217., 21.));
    if enabled {
        let current = value.unwrap_or(fallback);
        if let Some(v) =
            calendar_button(c, button, key, &format!("{key}_calendar"), origin, current)
        {
            result = Ok(Some(Some(v)));
        }
    } else {
        let _ = c.ellipsis(&format!("{key}_calendar"), c.r(x, y, 217., 21.), false);
    }
    result
}

/// The calendar `...` button of a date edit: inside the edit's frame, flush
/// right, 2 px in from its top, right and bottom.
fn calendar_button_rect(frame: Rect) -> Rect {
    Rect::from_min_size(
        Pos2::new(frame.right() - 19., frame.top() + 2.),
        Vec2::new(17., frame.height() - 4.),
    )
}

/// The calendar drop-down under the classic `...` face at `button`,
/// recorded as `action`, opened on the day of `current` (model minutes).
/// Returns the picked day at `current`'s time of day when it changed.
fn calendar_button(
    c: &mut Canvas,
    button: Rect,
    salt: &str,
    action: &str,
    origin: i64,
    current: i64,
) -> Option<i64> {
    let text = openxer_xer::date_string(origin, Some(current));
    let stamp = chrono::NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M").ok()?;
    let mut day = stamp.date();
    let response = crate::details::form::calendar_drop_down(c.ui, button, salt, &mut day);
    c.act(action, button);
    // The drop-down keeps working; the button shows the classic face.
    c.ellipsis_face(button, true);
    if !response.changed() || day == stamp.date() {
        return None;
    }
    let picked = day
        .and_time(stamp.time())
        .format("%Y-%m-%d %H:%M")
        .to_string();
    openxer_xer::parse_relative(origin, &picked).ok().flatten()
}

/// A date edit over a draft `text` that its dialog commits itself
/// (Schedule, Update Progress, Apply Actuals): the look of [`date_field`],
/// one sunken edit with the calendar `...` inside its frame at the right.
/// `picker` names the button's action. A day picked from the calendar
/// replaces the text, keeping its time of day.
#[allow(clippy::too_many_arguments)]
pub fn date_text_field(
    c: &mut Canvas,
    key: &str,
    picker: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    text: &mut String,
    origin: i64,
    enabled: bool,
) {
    c.edit_with_button(key, x, y, w, h, text, enabled, Align::Center, 21.);
    let button = calendar_button_rect(c.r(x, y, w, h));
    let parsed = format::parse_date(origin, text, None).ok().flatten();
    match parsed.filter(|_| enabled) {
        Some(current) => {
            if let Some(v) = calendar_button(c, button, picker, picker, origin, current) {
                *text = format::date(origin, Some(v));
            }
        }
        None => {
            c.ellipsis_face(button, false);
            c.act(picker, button);
        }
    }
}

/// Labeled combo over `(code, caption)` options. Returns the new code.
#[allow(clippy::too_many_arguments)]
pub fn combo(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    options: &[(&str, &str)],
    code: &str,
    enabled: bool,
) -> Option<String> {
    let items: Vec<(&str, bool)> = options.iter().map(|(_, t)| (*t, true)).collect();
    let current = options.iter().position(|(k, _)| *k == code);
    let mut index = current.unwrap_or(usize::MAX);
    let mut owned;
    let shown: &[(&str, bool)] = if current.is_none() && !code.is_empty() {
        owned = items.clone();
        owned.push((code, false));
        index = owned.len() - 1;
        &owned
    } else {
        &items
    };
    let changed = c.combo(key, x, y, w, 22., shown, &mut index, enabled);
    (changed && index < options.len()).then(|| options[index].0.to_string())
}

/// A push button bar (Add, Delete...) at `y`, buttons 85 x 21 at a 90 px
/// pitch starting at `x`. Returns the index of a clicked enabled button.
pub fn buttons(c: &mut Canvas, key: &str, x: f32, y: f32, names: &[(&str, bool)]) -> Option<usize> {
    let mut clicked = None;
    for (i, (name, enabled)) in names.iter().enumerate() {
        if c.button(
            &format!("{key}_{i}"),
            x + 90. * i as f32,
            y,
            85.,
            21.,
            name,
            false,
            *enabled,
        ) {
            clicked = Some(i);
        }
    }
    clicked
}

/// Modify, Print and Copy under a notes or notebook text pane, as every
/// such pane has them (`details::form::notes_buttons`), keyed
/// `{key}_modify`, `{key}_print` and `{key}_copy`. Returns 0 for Modify and
/// 2 for Copy when clicked.
pub fn notes_buttons(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    editable: bool,
    text: &str,
) -> Option<usize> {
    let spec = crate::details::form::notes_buttons(editable, !text.trim().is_empty());
    let mut clicked = None;
    for (i, (name, enabled)) in spec.iter().enumerate() {
        let id = format!("{key}_{}", name.to_lowercase());
        if c.button(&id, x + 90. * i as f32, y, 85., 21., name, false, *enabled) {
            clicked = Some(i);
        }
    }
    clicked
}

/// A read-only grid: `#F0F0F0` header with black dividers, 18 px rows,
/// white body in a sunken frame. Returns the clicked row.
#[allow(clippy::too_many_arguments)]
pub fn table(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    columns: &[(&str, f32, bool)],
    rows: &[Vec<String>],
    selected: Option<usize>,
) -> Option<usize> {
    table_menu(c, key, x, y, w, h, columns, rows, selected, None)
}

/// Shows `entries` as the context menu of `response`; the key of the item
/// clicked, if any. Built fresh each frame the menu is open.
pub fn entries_menu(
    prefix: &str,
    response: &egui::Response,
    open_at: Option<Pos2>,
    entries: &[crate::context_menu::Entry],
) -> Option<&'static str> {
    let mut chosen = None;
    crate::context_menu::ContextMenu::new(prefix)
        .min_width(120.)
        .open_at(open_at)
        .show(response, &mut chosen, |chosen, m| {
            if let Some(key) = m.entries(entries) {
                *chosen = Some(key);
            }
        });
    chosen
}

/// What a grid's context menu is built over: a row (`Some`) or the empty
/// area below the rows (`None`), a zero-size anchor response for the popup
/// and the position of a right-click there this frame.
pub type TableMenu<'a> = &'a mut dyn FnMut(Option<usize>, &egui::Response, Option<Pos2>);

/// [`table`] with a right-click menu. A right-click on a row selects it
/// (the returned row), even when an edit box of the selected row sits over
/// it; `menu` is called for every row and for the area below them, each
/// frame, and shows its popup on the anchor it is given.
#[allow(clippy::too_many_arguments)]
pub fn table_menu(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    columns: &[(&str, f32, bool)],
    rows: &[Vec<String>],
    selected: Option<usize>,
    mut menu: Option<TableMenu>,
) -> Option<usize> {
    let rect = c.r(x, y, w, h);
    c.sunken(rect, Color32::WHITE);
    let inner = rect.shrink(2.);
    let p =
        c.ui.painter()
            .with_clip_rect(inner.intersect(c.ui.clip_rect()));
    let header = Rect::from_min_size(inner.min, Vec2::new(inner.width(), 19.));
    p.rect_filled(header, 0., CHROME);
    p.hline(
        header.x_range(),
        header.bottom() - 0.5,
        Stroke::new(1_f32, INK),
    );
    let mut left = inner.left();
    let font = chrome::table_font();
    for (title, width, right) in columns {
        let r = Rect::from_min_size(Pos2::new(left, header.top()), Vec2::new(*width, 19.));
        let pos = if *right {
            Pos2::new(r.right() - 4., r.center().y)
        } else {
            Pos2::new(r.left() + 3., r.center().y)
        };
        p.with_clip_rect(r.intersect(p.clip_rect())).text(
            pos,
            if *right {
                Align2::RIGHT_CENTER
            } else {
                Align2::LEFT_CENTER
            },
            *title,
            font.clone(),
            INK,
        );
        p.vline(r.right() - 0.5, r.y_range(), Stroke::new(1_f32, INK));
        left += width;
    }
    let mut clicked = None;
    // A right-click is read from the pointer, not from the row widgets: the
    // selected row has edit boxes over it that would take the click.
    let right_click = menu
        .as_ref()
        .and_then(|_| {
            c.ui.input(|i| {
                i.pointer
                    .button_clicked(egui::PointerButton::Secondary)
                    .then(|| i.pointer.interact_pos())
                    .flatten()
            })
        })
        .filter(|pos| {
            pos.y >= header.bottom() && inner.contains(*pos) && c.ui.rect_contains_pointer(inner)
        });
    // Rows past the space scroll by whole rows: the wheel, a scroll bar,
    // and a newly selected row is brought into view.
    let body = Rect::from_min_max(Pos2::new(inner.left(), header.bottom()), inner.max);
    let visible = visible_rows(h);
    let first = scroll_first(c, key, body, rows.len(), visible, selected);
    let bar = rows.len() > visible;
    let row_right = if bar {
        left.min(inner.right() - TABLE_BAR)
    } else {
        left
    };
    let row_at = right_click.map(|pos| first + ((pos.y - header.bottom()) / 18.).floor() as usize);
    let right_row = row_at.filter(|r| *r < rows.len());
    if let Some(menu) = menu.as_mut() {
        let anchor = c.ui.interact(
            Rect::from_min_size(right_click.unwrap_or(Pos2::ZERO), Vec2::ZERO),
            c.id(&format!("{key}_body_menu")),
            Sense::hover(),
        );
        menu(None, &anchor, right_click.filter(|_| right_row.is_none()));
    }
    if let Some(r) = right_row {
        clicked = Some(r);
    }
    for (i, row) in rows.iter().enumerate().skip(first) {
        let top = header.bottom() + 18. * (i - first) as f32;
        if top > inner.bottom() {
            break;
        }
        let r = Rect::from_min_size(
            Pos2::new(inner.left(), top),
            Vec2::new(row_right - inner.left(), 18.),
        );
        let response = c.hit(&format!("{key}_row_{i}"), r, true);
        if response.clicked() {
            clicked = Some(i);
        }
        if let Some(menu) = menu.as_mut() {
            let anchor = c.ui.interact(
                Rect::from_min_size(right_click.unwrap_or(Pos2::ZERO), Vec2::ZERO),
                c.id(&format!("{key}_row_menu_{i}")),
                Sense::hover(),
            );
            menu(
                Some(i),
                &anchor,
                right_click.filter(|_| right_row == Some(i)),
            );
        }
        let is_selected = selected == Some(i);
        if is_selected {
            p.rect_filled(r, 0., SELECT);
        }
        let mut cx = inner.left();
        for (j, (_, width, right)) in columns.iter().enumerate() {
            let cell = Rect::from_min_size(Pos2::new(cx, top), Vec2::new(*width, 18.));
            let text = row.get(j).map_or("", String::as_str);
            let pos = if *right {
                Pos2::new(cell.right() - 4., cell.center().y)
            } else {
                Pos2::new(cell.left() + 3., cell.center().y)
            };
            p.with_clip_rect(cell.shrink2(Vec2::new(1., 0.)).intersect(p.clip_rect()))
                .text(
                    pos,
                    if *right {
                        Align2::RIGHT_CENTER
                    } else {
                        Align2::LEFT_CENTER
                    },
                    text,
                    font.clone(),
                    if is_selected { Color32::WHITE } else { INK },
                );
            cx += width;
        }
    }
    if bar {
        let track = Rect::from_min_max(Pos2::new(inner.right() - TABLE_BAR, body.top()), body.max);
        let first = scroll_bar(c, key, track, first, visible, rows.len());
        c.ui.ctx()
            .data_mut(|d| d.insert_temp(c.id(&format!("{key}_scroll")), first));
    }
    clicked
}

/// Width of a details table's scroll bar.
const TABLE_BAR: f32 = 14.;

/// Rows a details table `h` tall shows whole.
fn visible_rows(h: f32) -> usize {
    ((h - 4. - 19.) / 18.).floor().max(1.) as usize
}

/// The first row a details table shows this frame: as last scrolled, moved
/// by the wheel over `body`, and moved to bring `selected` into view when
/// it changed. Saved for the next frame and for [`row_top`].
fn scroll_first(
    c: &mut Canvas,
    key: &str,
    body: Rect,
    rows: usize,
    visible: usize,
    selected: Option<usize>,
) -> usize {
    let scroll_id = c.id(&format!("{key}_scroll"));
    let reveal_id = c.id(&format!("{key}_reveal"));
    let max_first = rows.saturating_sub(visible);
    let ctx = c.ui.ctx().clone();
    let mut first = ctx
        .data(|d| d.get_temp::<usize>(scroll_id))
        .unwrap_or(0)
        .min(max_first);
    let last = ctx
        .data(|d| d.get_temp::<Option<usize>>(reveal_id))
        .flatten();
    if selected != last {
        if let Some(r) = selected.filter(|r| *r < rows) {
            if r < first {
                first = r;
            } else if r >= first + visible {
                first = (r + 1 - visible).min(max_first);
            }
        }
        ctx.data_mut(|d| d.insert_temp(reveal_id, selected));
    }
    if rows > visible && c.ui.rect_contains_pointer(body) {
        let wheel = ctx.input(|i| i.smooth_scroll_delta.y);
        if wheel.abs() >= 1. {
            let step = (wheel / 18.).round() as i64;
            let step = if step == 0 {
                wheel.signum() as i64
            } else {
                step
            };
            first = (first as i64 - step).clamp(0, max_first as i64) as usize;
        }
    }
    ctx.data_mut(|d| d.insert_temp(scroll_id, first));
    first
}

/// The scroll bar of a details table in `track`: arrows step a row, the
/// track pages, the thumb drags. Returns the first row shown.
fn scroll_bar(
    c: &mut Canvas,
    key: &str,
    track: Rect,
    mut first: usize,
    visible: usize,
    rows: usize,
) -> usize {
    let max_first = rows.saturating_sub(visible);
    let p = c.ui.painter().clone();
    p.rect_filled(track, 0., MARGIN);
    let arrow = TABLE_BAR.min(track.height() / 3.);
    let up = Rect::from_min_size(track.min, Vec2::new(TABLE_BAR, arrow));
    let down = Rect::from_min_size(
        Pos2::new(track.left(), track.bottom() - arrow),
        Vec2::new(TABLE_BAR, arrow),
    );
    let raised = |r: Rect| {
        p.rect_filled(r, 0., CHROME);
        p.rect_stroke(r, 0., Stroke::new(1_f32, SHADOW), egui::StrokeKind::Inside);
    };
    for (i, r) in [up, down].into_iter().enumerate() {
        raised(r);
        let m = r.center();
        let (tip, base) = if i == 0 { (-2., 2.) } else { (2., -2.) };
        p.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(m.x - 3., m.y + base),
                Pos2::new(m.x + 3., m.y + base),
                Pos2::new(m.x, m.y + tip),
            ],
            INK,
            Stroke::NONE,
        ));
        let name = format!("{key}_scroll_{}", if i == 0 { "up" } else { "down" });
        if c.hit(&name, r, true).clicked() {
            first = if i == 0 {
                first.saturating_sub(1)
            } else {
                (first + 1).min(max_first)
            };
        }
    }
    let span = (track.height() - 2. * arrow).max(1.);
    let thumb_h = (span * visible as f32 / rows.max(1) as f32).clamp(8_f32.min(span), span);
    let thumb_top = track.top() + arrow + (span - thumb_h) * first as f32 / max_first.max(1) as f32;
    let thumb = Rect::from_min_size(
        Pos2::new(track.left(), thumb_top),
        Vec2::new(TABLE_BAR, thumb_h),
    );
    let lane = Rect::from_min_max(
        Pos2::new(track.left(), up.bottom()),
        Pos2::new(track.right(), down.top()),
    );
    let page = c.hit(&format!("{key}_scroll_track"), lane, true);
    if page.clicked() {
        if let Some(pos) = page.interact_pointer_pos() {
            first = if pos.y < thumb.top() {
                first.saturating_sub(visible)
            } else {
                (first + visible).min(max_first)
            };
        }
    }
    raised(thumb);
    let drag =
        c.ui.interact(thumb, c.id(&format!("{key}_scroll_thumb")), Sense::drag());
    c.act(&format!("{key}_scroll_thumb"), thumb);
    if let Some(pointer) = drag.interact_pointer_pos().filter(|_| drag.dragged()) {
        let fraction = ((pointer.y - track.top() - arrow - thumb_h / 2.)
            / (span - thumb_h).max(1.))
        .clamp(0., 1.);
        first = (fraction * max_first as f32).round() as usize;
    }
    first
}

/// Where row `i` of the details table `key` (drawn at `y`, `h` tall in the
/// same canvas) sits as scrolled: the top of its cells, or `None` when it
/// is scrolled out of view. Edit boxes over a row are placed with it.
pub fn row_top(c: &Canvas, key: &str, y: f32, h: f32, i: usize) -> Option<f32> {
    let first =
        c.ui.ctx()
            .data(|d| d.get_temp::<usize>(c.id(&format!("{key}_scroll"))))
            .unwrap_or(0);
    (i >= first && i < first + visible_rows(h)).then(|| y + 2. + 19. + 18. * (i - first) as f32)
}

/// Hover text over a whole tab whose data XER files do not carry.
pub fn not_in_xer(c: &mut Canvas, key: &str, rect: Rect, text: &str) {
    let response = c.ui.interact(rect, c.id(key), Sense::hover());
    c.act(key, rect);
    response.on_hover_text(text);
}

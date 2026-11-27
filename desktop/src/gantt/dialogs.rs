//! Bars, Bar Chart Options, Timescale and Curtain Attachment dialogs, and the
//! Gantt right-click menu. Each dialog edits a draft; Apply and OK validate
//! the draft and hand it to `App::apply_gantt`.
use super::settings::*;
use super::*;
use crate::dialog_kit::{self as kit, Canvas, Col, Entry, VTab};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum BarsTab {
    #[default]
    Style,
    Settings,
    Labels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum OptionsTab {
    #[default]
    General,
    Collapsed,
    DataDate,
    SightLines,
    ProgressLine,
}

pub(crate) struct BarsDraft {
    pub bars: Vec<BarDef>,
    pub selected: usize,
    pub tab: BarsTab,
    pub label: Option<usize>,
    pub filter_edit: Option<(usize, BarFilter)>,
    pub error: Option<String>,
    /// Scroll the list to the selected bar on the next frame.
    pub scroll: bool,
}

pub(crate) struct OptionsDraft {
    pub options: ChartOptions,
    pub row_height: f32,
    pub tab: OptionsTab,
    pub error: Option<String>,
}

pub(crate) struct TimescaleDraft {
    pub timescale: TimescaleSettings,
    pub start_text: String,
    pub ordinal_text: String,
    pub error: Option<String>,
}

pub(crate) struct CurtainDraft {
    /// None while adding a new curtain.
    pub index: Option<usize>,
    pub curtain: Curtain,
    pub start_text: String,
    pub finish_text: String,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MenuState {
    pub pos: Pos2,
    pub minute: Option<i64>,
    pub opened_frame: usize,
    /// Opened by a right-click on this relationship line: when the menu
    /// closes without a command, Edit Relationship opens for it.
    pub link: Option<usize>,
}

/// The Text Attachment dialog's draft.
pub(crate) struct TextDraft {
    /// None while adding a new text.
    pub index: Option<usize>,
    pub text: String,
    pub date: i64,
    pub task_id: i64,
    pub band: Option<String>,
    pub font: TextFont,
    pub error: Option<String>,
}

/// Cache key for `GanttUi::driving`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrivingKey {
    Serial(u64),
    Hash(u64),
}

/// Cache key for per-frame calendar nonwork spans in the Gantt header.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct GanttCalKey {
    pub serial: u64,
    pub cal_start: i64,
    pub cal_finish: i64,
    pub lowest_hour: bool,
    pub shade: bool,
}

pub(crate) struct GanttCalCache {
    pub key: GanttCalKey,
    pub calendars: std::rc::Rc<std::collections::BTreeMap<usize, super::CalendarSpans>>,
    pub shade_cells: std::rc::Rc<std::collections::BTreeMap<usize, Vec<super::TimeSpan>>>,
}

/// Spatial index over relationship links for fast visible-viewport queries.
pub(crate) struct LinkIndexCache {
    pub serial: u64,
    pub rows: std::rc::Rc<Vec<crate::grid::Row>>,
    pub blocks: Vec<Vec<usize>>,
    pub long_links: Vec<(u32, u32, u32)>,
    pub long_max_end: Vec<u32>,
    pub act_offsets: Vec<u32>,
    pub act_links: Vec<u32>,
}

/// Dialog and menu state; also caches that the painter reuses across frames.
#[derive(Default)]
pub(crate) struct GanttUi {
    pub snapshot: String,
    pub driving_key: Option<DrivingKey>,
    pub driving: std::rc::Rc<Vec<Option<bool>>>,
    pub cal_cache: Option<GanttCalCache>,
    pub link_index: Option<LinkIndexCache>,
    pub collapsed_geometry: Vec<BarGeometry>,
    pub bars: Option<BarsDraft>,
    pub options: Option<OptionsDraft>,
    pub timescale: Option<TimescaleDraft>,
    pub curtain: Option<CurtainDraft>,
    pub text: Option<TextDraft>,
    /// Attachments > Text was chosen: the next click on the chart places
    /// the new text box there.
    /// The frame it was chosen in; that frame's click is the menu's.
    pub placing_text: Option<usize>,
    pub menu: Option<MenuState>,
    /// A text attachment being dragged: its index and the pointer travel.
    pub text_drag: Option<(usize, Vec2)>,
    /// A bar being dragged (not-started activities only).
    pub bar_drag: Option<super::BarDrag>,
    /// The relationship line right-clicked: drawn selected while its menu
    /// and then Edit Relationship are up.
    pub picked_link: Option<usize>,
}

/// Bars list column widths from the measured text widths of Name,
/// Timescale, the two user date titles and Filter: Display and Preview keep
/// their sizes, the empty date columns at most 72 px, and Name,
/// Timescale and Filter share the rest of the list's 609 px in proportion
/// to their text (getting all they need when it fits; Name takes any room
/// left over).
pub(crate) fn bar_list_widths(measured: [f32; 5]) -> [f32; 7] {
    const TOTAL: f32 = 609.;
    let (display, preview) = (46., 62.);
    let [name, timescale, start, finish, filter] = measured;
    // Never filled in this list: kept to their titles, at most 72 px.
    let (start, finish) = (start.min(72.), finish.min(72.));
    let room = (TOTAL - display - preview - start - finish).max(90.);
    let want = name + timescale + filter;
    let (name, timescale, filter) = if want <= room {
        (name + room - want, timescale, filter)
    } else {
        let k = room / want;
        (
            (name * k).floor(),
            (timescale * k).floor(),
            room - (name * k).floor() - (timescale * k).floor(),
        )
    };
    [display, name, timescale, start, finish, filter, preview]
}

/// A whole-number spin edit (`kit::number_spin`); the new value when it
/// changed.
fn spin_int(
    c: &mut Canvas,
    key: &str,
    rect: (f32, f32, f32, f32),
    value: i64,
    (lo, hi): (i64, i64),
    enabled: bool,
) -> Option<i64> {
    let mut v = value as f64;
    kit::number_spin(c, key, rect, &mut v, 1., lo as f64..=hi as f64, enabled)
        .then(|| v.round() as i64)
}

/// A drop-down of text choices (`Canvas::combo`); true when the choice
/// changed.
#[allow(clippy::too_many_arguments)]
fn choose<T: Copy + PartialEq>(
    c: &mut Canvas,
    key: &str,
    (x, y, w): (f32, f32, f32),
    value: &mut T,
    choices: &[T],
    label: impl Fn(T) -> String,
    available: impl Fn(T) -> bool,
    enabled: bool,
) -> bool {
    let texts: Vec<String> = choices.iter().map(|c| label(*c)).collect();
    let items: Vec<(&str, bool)> = texts
        .iter()
        .zip(choices)
        .map(|(t, c)| (t.as_str(), available(*c)))
        .collect();
    let mut index = choices.iter().position(|c| c == value).unwrap_or(0);
    if c.combo(key, x, y, w, 21., &items, &mut index, enabled) {
        *value = choices[index];
        return true;
    }
    false
}

/// A drop-down whose field and list show drawn previews
/// (`kit::drawn_combo`); `draw` paints one choice into a rectangle.
#[allow(clippy::too_many_arguments)]
fn choose_drawn<T: Copy + PartialEq>(
    c: &mut Canvas,
    key: &str,
    (x, y, w): (f32, f32, f32),
    value: &mut T,
    choices: &[T],
    hover: impl Fn(T) -> String,
    draw: impl Fn(&egui::Painter, Rect, T),
) -> bool {
    let mut index = choices.iter().position(|c| c == value).unwrap_or(0);
    let changed = kit::drawn_combo(
        c,
        key,
        (x, y, w, 21.),
        choices.len(),
        &mut index,
        true,
        &|k| hover(choices[k]),
        &|p, r, k| draw(p, r, choices[k]),
    );
    if changed {
        *value = choices[index];
    }
    changed
}

/// Edits an anchored date in one row from (x, y): the anchor (Project
/// Start, Data Date or a date), the date of a fixed anchor and an offset in
/// days. A date that does not parse leaves the stored one.
#[allow(clippy::too_many_arguments)]
fn anchored_date(
    c: &mut Canvas,
    key: &str,
    (x, y): (f32, f32),
    date: &mut AnchoredDate,
    text: &mut String,
    origin: i64,
    enabled: bool,
) {
    let mut kind = match date.anchor {
        Anchor::ProjectStart => 0,
        Anchor::DataDate => 1,
        Anchor::Date(_) => 2,
    };
    let anchors = [
        ("Project Start (PS)", true),
        ("Data Date (DD)", true),
        ("Date", true),
    ];
    if c.combo(
        &format!("{key}_anchor"),
        x,
        y,
        116.,
        21.,
        &anchors,
        &mut kind,
        enabled,
    ) {
        date.anchor = match kind {
            0 => Anchor::ProjectStart,
            1 => Anchor::DataDate,
            _ => Anchor::Date(parse_relative(origin, text).ok().flatten().unwrap_or(0)),
        };
    }
    if let Anchor::Date(minute) = &mut date.anchor {
        let edit = c.edit(
            &format!("{key}_date"),
            x + 120.,
            y,
            76.,
            21.,
            text,
            enabled,
            Align::Min,
        );
        if edit.changed() {
            if let Ok(Some(value)) = parse_relative(origin, text) {
                *minute = value;
            }
        }
    }
    if let Some(days) = spin_int(
        c,
        &format!("{key}_offset"),
        (x + 200., y, 46., 21.),
        date.offset_days.into(),
        (-36_600, 36_600),
        enabled,
    ) {
        date.offset_days = days as i32;
    }
    kit::text(c, x + 250., y + 4., "d", enabled);
}

/// Draws an end shape centred in a picker field.
fn draw_end_shape(p: &egui::Painter, r: Rect, shape: EndShape, at_start: bool) {
    let mut def = BarDef::added(1);
    def.middle.shape = BodyShape::None;
    let style = EndStyle::new(shape, Rgb::BLACK);
    if at_start {
        def.start = style;
    } else {
        def.end = style;
    }
    let x = r.center().x.round();
    let top = (r.center().y - 7.5).round();
    bars::paint(p, &bars::layout_bar(&def, r.left(), top, x, x, &[]));
}

/// Draws a middle bar shape across a picker field: outlined shapes white with
/// a black frame, plain ones in gray.
fn draw_body_shape(p: &egui::Painter, r: Rect, (shape, outline): (BodyShape, bool)) {
    let mut def = BarDef::added(1);
    def.middle = MiddleStyle {
        shape,
        outline,
        color: if outline {
            Rgb::WHITE
        } else {
            Rgb(0x80, 0x80, 0x80)
        },
        pattern: Pattern::Solid,
    };
    let top = (r.center().y - 7.5).round();
    let draw = bars::layout_bar(
        &def,
        r.left(),
        top,
        r.left().round() + 2.,
        r.right().round() - 3.,
        &[],
    );
    bars::paint(p, &draw);
}

/// Draws a pattern sample filling a picker field.
fn draw_pattern(p: &egui::Painter, r: Rect, pattern: Pattern) {
    let swatch = r.shrink2(Vec2::new(1., 1.));
    bars::fill_rect(p, swatch, Color32::BLACK, pattern);
    if pattern == Pattern::Hollow {
        p.rect_stroke(
            swatch,
            0.,
            Stroke::new(1_f32, INK),
            egui::StrokeKind::Inside,
        );
    }
}

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

fn parse_relative(origin: i64, text: &str) -> Result<Option<i64>, String> {
    openxer_xer::parse_relative(origin, text)
}

impl App {
    pub(crate) fn open_bars_dialog(&mut self) {
        self.gantt_ui.bars = Some(BarsDraft {
            bars: self.gantt.bars.clone(),
            selected: 0,
            tab: BarsTab::Style,
            label: None,
            filter_edit: None,
            error: None,
            scroll: false,
        });
    }

    pub(crate) fn open_chart_options(&mut self) {
        self.gantt_ui.options = Some(OptionsDraft {
            options: self.gantt.options,
            row_height: self.view.row_height,
            tab: OptionsTab::General,
            error: None,
        });
    }

    pub(crate) fn open_timescale_dialog(&mut self) {
        let origin = self.project.model.origin_serial;
        let mut timescale = self.gantt.timescale.clone();
        if self.view.automatic_units {
            timescale.interval = DateInterval::Automatic;
        }
        let text = |d: AnchoredDate| match d.anchor {
            Anchor::Date(m) => date_string(origin, Some(m)),
            _ => String::new(),
        };
        self.gantt_ui.timescale = Some(TimescaleDraft {
            start_text: text(timescale.start),
            ordinal_text: text(timescale.ordinal_start),
            timescale,
            error: None,
        });
    }

    pub(crate) fn open_curtain(&mut self, index: Option<usize>, at: i64) {
        let origin = self.project.model.origin_serial;
        let curtain = index
            .and_then(|i| self.gantt.curtains.get(i).cloned())
            .unwrap_or(Curtain {
                display: true,
                start: at.div_euclid(DAY) * DAY,
                finish: at.div_euclid(DAY) * DAY + 7 * DAY,
                color: Rgb(0xc0, 0xc0, 0xc0),
                pattern: Pattern::ForwardDiagonal,
            });
        self.gantt_ui.curtain = Some(CurtainDraft {
            index,
            start_text: date_string(origin, Some(curtain.start)),
            finish_text: date_string(origin, Some(curtain.finish)),
            curtain,
            error: None,
        });
    }

    /// A right-click on a bar, the empty chart area or the timescale bands:
    /// the one chart menu.
    pub(crate) fn open_gantt_menu(&mut self, pos: Pos2) {
        let axis = TimeScale::new(
            self.timeline_origin as f64,
            self.pixels_per_day as f64 * 7.,
            self.gantt_left,
        );
        self.gantt_ui.menu = Some(MenuState {
            pos,
            minute: Some(axis.x_date(pos.x).round() as i64),
            opened_frame: self.frame_number,
            link: None,
        });
    }

    /// A right-click on relationship line `index`, as observed in the
    /// reference: the line is drawn selected and the same chart menu opens;
    /// when that menu is closed without a command, Edit Relationship opens.
    pub(crate) fn open_link_menu(&mut self, pos: Pos2, index: usize) {
        self.open_gantt_menu(pos);
        if let Some(menu) = self.gantt_ui.menu.as_mut() {
            menu.link = Some(index);
        }
        self.gantt_ui.picked_link = Some(index);
    }

    /// The activity or grouping band drawn on the chart row at `y`, or the
    /// nearest drawn row.
    pub(crate) fn row_anchor_at(&self, y: f32) -> Option<RowAnchor> {
        let gap = |r: Rect| {
            if (r.top()..r.bottom()).contains(&y) {
                0.
            } else {
                (r.center().y - y).abs()
            }
        };
        self.row_geometry
            .iter()
            .min_by(|a, b| gap(a.1).total_cmp(&gap(b.1)))
            .map(|(anchor, _)| anchor.clone())
    }

    /// The Gantt right-click menu: the native popup, 161 px wide, with the
    /// chart's own commands. Zoom, Progress Line, Relationships and Export
    /// to Excel live in the View menu, the toolbars and the table menu.
    pub(crate) fn gantt_menu(&mut self, ctx: &egui::Context, axis: &TimeScale) {
        let Some(menu) = self.gantt_ui.menu else {
            return;
        };
        let minute = menu
            .minute
            .unwrap_or_else(|| axis.x_date(menu.pos.x).round() as i64);
        let mut open = true;
        let chosen = egui::Popup::new(
            egui::Id::new("gantt_context_menu"),
            ctx.clone(),
            egui::PopupAnchor::Position(menu.pos),
            egui::LayerId::background(),
        )
        .kind(egui::PopupKind::Menu)
        .layout(Layout::top_down_justified(Align::Min))
        .style(chrome::popup_menu_style)
        .gap(0.)
        .open_bool(&mut open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(155.);
            self.chart_context_menu(&mut crate::context_menu::Menu::on(ui, "gantt_menu"), minute)
        });
        let chosen = chosen.is_some_and(|r| r.inner);
        // The opening right-click is not a click outside.
        let mut closed = false;
        if !open && self.frame_number > menu.opened_frame + 1 {
            self.gantt_ui.menu = None;
            closed = true;
        } else if !open {
            self.gantt_ui.menu = Some(menu);
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.gantt_ui.menu = None;
            closed = true;
        }
        if chosen {
            self.gantt_ui.menu = None;
            closed = true;
        }
        // A menu opened on a relationship line: closed without a command,
        // it opens Edit Relationship for the line.
        if closed {
            if let Some(index) = menu.link {
                if !chosen && index < self.project.model.links.len() {
                    self.edit_link(index);
                } else {
                    self.gantt_ui.picked_link = None;
                }
            }
        }
    }

    /// The chart menu of the Activities window: one menu for a bar, the
    /// empty chart area and the timescale, as in the reference. `minute`
    /// places a new curtain. Rectangles are `gantt_menu_<key>`.
    /// Returns whether a command was chosen.
    pub(crate) fn chart_context_menu(
        &mut self,
        m: &mut crate::context_menu::Menu,
        minute: i64,
    ) -> bool {
        use chrome::{Icon, Item};
        let mut chosen = false;
        if m.item("details", Item::new("Activity Details").icon(Icon::Details)) {
            self.set_bottom(Some(crate::panes::BottomView::Details));
            chosen = true;
        }
        m.separator();
        let bars = Item::new("Bars...")
            .icon(Icon::Bars)
            .enabled(self.gantt_on_screen());
        if m.item("bars", bars) {
            self.open_bars_dialog();
            chosen = true;
        }
        if m.item(
            "options",
            Item::new("Bar Chart Options...").icon(Icon::Options),
        ) {
            self.open_chart_options();
            chosen = true;
        }
        if m.item("timescale", Item::new("Timescale...").icon(Icon::Timescale)) {
            self.open_timescale_dialog();
            chosen = true;
        }
        m.submenu("attachments", Item::new("Attachments"), |m| {
            self.attachment_items(m.ui(), "gantt_menu", minute);
        });
        chosen
    }

    /// The Attachments submenu shared by the Gantt menu, the `Layout:`
    /// menu and View: Curtain (Add Curtain, Show All, Hide All, then each
    /// curtain to edit) and Text. `minute` places a new curtain; a new text
    /// box is placed by a click on the chart.
    pub(crate) fn attachment_items(&mut self, ui: &mut egui::Ui, prefix: &str, minute: i64) {
        let (curtain, _) = chrome::submenu(ui, chrome::Item::new("Curtain"), |ui| {
            let add = chrome::Item::new("Add Curtain");
            if self.item(ui, &format!("{prefix}_add_curtain"), add) {
                self.open_curtain(None, minute);
            }
            let any_hidden = self.gantt.curtains.iter().any(|c| !c.display);
            let any_shown = self.gantt.curtains.iter().any(|c| c.display);
            let show = chrome::Item::new("Show All").enabled(any_hidden);
            if self.item(ui, &format!("{prefix}_show_curtains"), show) {
                self.gantt
                    .curtains
                    .iter_mut()
                    .for_each(|c| c.display = true);
            }
            let hide = chrome::Item::new("Hide All").enabled(any_shown);
            if self.item(ui, &format!("{prefix}_hide_curtains"), hide) {
                self.gantt
                    .curtains
                    .iter_mut()
                    .for_each(|c| c.display = false);
            }
            if !self.gantt.curtains.is_empty() {
                // OpenXER also lists each curtain for editing.
                chrome::menu_separator(ui);
                let origin = self.project.model.origin_serial;
                for k in 0..self.gantt.curtains.len() {
                    let c = &self.gantt.curtains[k];
                    let label = format!(
                        "{} - {}...",
                        format::date(origin, Some(c.start)),
                        format::date(origin, Some(c.finish))
                    );
                    if self.item(
                        ui,
                        &format!("{prefix}_curtain_{k}"),
                        chrome::Item::new(&label),
                    ) {
                        self.open_curtain(Some(k), minute);
                    }
                }
            }
        });
        self.action(&format!("{prefix}_curtain"), &curtain);
        if self.item(ui, &format!("{prefix}_text"), chrome::Item::new("Text")) {
            self.begin_text_placement();
        }
    }

    /// The chart's centre date, where commands from the main menu place a
    /// new curtain or text.
    pub(crate) fn chart_center_minute(&self) -> i64 {
        let axis = TimeScale::new(
            self.timeline_origin as f64,
            self.pixels_per_day as f64 * 7.,
            self.gantt_left,
        );
        axis.x_date(self.gantt_left + self.chart_width / 2.).round() as i64
    }

    /// Attachments > Text: the next click on the chart places a new text
    /// box. Click to place is our own arrangement. Escape cancels.
    pub(crate) fn begin_text_placement(&mut self) {
        self.gantt_ui.placing_text = Some(self.frame_number);
        self.status = "Click the chart where the text box goes. Escape cancels.".into();
    }

    /// The click that places a new text box at `minute` on the row `anchor`.
    pub(crate) fn place_text_attachment(&mut self, minute: i64, anchor: RowAnchor) {
        self.gantt_ui.placing_text = None;
        let (task_id, band) = match anchor {
            RowAnchor::Activity(task) => (task, None),
            RowAnchor::Band(key) => (0, Some(key)),
        };
        self.gantt_ui.text = Some(TextDraft {
            index: None,
            text: String::new(),
            date: minute.div_euclid(60) * 60,
            task_id,
            band,
            font: TextFont::default(),
            error: None,
        });
    }

    /// Opens text attachment `index` for editing.
    pub(crate) fn edit_text_attachment(&mut self, index: usize) {
        if let Some(t) = self.gantt.texts.get(index) {
            self.gantt_ui.text = Some(TextDraft {
                index: Some(index),
                text: t.text.clone(),
                date: t.date,
                task_id: t.task_id,
                band: t.band.clone(),
                font: t.font,
                error: None,
            });
        }
    }

    /// Text Attachment: the text, with OK, Cancel, Delete (for a saved
    /// text) and Help. The text belongs to the layout, so it is not an
    /// undo step of the project.
    fn text_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.gantt_ui.text.take() else {
            return;
        };
        let existing = draft.index.is_some();
        let mut pick_font = false;
        const W: f32 = 456.;
        const H: f32 = 190.;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "text_attachment",
            "Text Attachment",
            Vec2::new(W, H),
            |c| {
                c.lowered(5., 5., 339., 180.);
                kit::text(c, 13., 11., "Text", true);
                let f = draft.font;
                let response = kit::text_area(
                    c,
                    "text_attachment_text",
                    (13., 29., 323., 104.),
                    &mut draft.text,
                    f.face.font(f.points, f.bold),
                    f.color.color(),
                );
                if !existing && draft.text.is_empty() && !response.has_focus() {
                    response.request_focus();
                }
                kit::text(c, 13., 148., "Font", true);
                if crate::layout_opts::sample_button(
                    c,
                    "text_attachment_font",
                    (50., 139., 113., 26.),
                    f.face.font(f.points, f.bold),
                    f.italic,
                    f.color.color(),
                ) {
                    pick_font = true;
                }
                kit::text(c, 178., 148., "Color", true);
                kit::swatch(
                    c,
                    "text_attachment_color",
                    (214., 142., 57., 20.),
                    &mut draft.font.color,
                    true,
                );
                if let Some(error) = &draft.error {
                    kit::error_text(c, 13., 167., 323., error);
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("text_attachment_ok"),
                        Entry::cancel("text_attachment_cancel"),
                        Entry::Button("text_attachment_delete", "Delete", existing, true),
                        Entry::help("text_attachment_help"),
                    ],
                )
                .clicked
            },
        );
        if pick_font {
            self.open_text_font_picker(draft.font);
        }
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        let (ok, delete) = (
            clicked == Some("text_attachment_ok"),
            clicked == Some("text_attachment_delete"),
        );
        if clicked == Some("text_attachment_help") {
            self.open_help_for("text_attachment");
        }
        // Escape reaches this window only when the font picker is not open
        // on top of it (the shell gives the key to the top window).
        let picker_open = self.text_font_picker_open();
        if clicked == Some("text_attachment_cancel") || closed {
            if picker_open {
                self.layout_opts.drafts.picker = None;
            }
            return;
        }
        if (delete || ok) && picker_open {
            self.layout_opts.drafts.picker = None;
        }
        let mut settings = self.gantt.clone();
        if delete {
            if let Some(index) = draft.index.filter(|i| *i < settings.texts.len()) {
                settings.texts.remove(index);
                if let Err(error) = self.apply_gantt(settings) {
                    self.status = error;
                }
            }
            return;
        }
        if !ok {
            self.gantt_ui.text = Some(draft);
            return;
        }
        let text = TextAttachment {
            text: draft.text.trim_end().to_owned(),
            date: draft.date,
            task_id: draft.task_id,
            band: draft.band.clone(),
            font: draft.font,
        };
        match draft.index.filter(|i| *i < settings.texts.len()) {
            Some(index) => settings.texts[index] = text,
            None => settings.texts.push(text),
        }
        if let Err(error) = self.apply_gantt(settings) {
            draft.error = Some(error);
            self.gantt_ui.text = Some(draft);
        }
    }

    pub(crate) fn gantt_dialogs(&mut self, ctx: &egui::Context) {
        if self.gantt_ui.placing_text.is_some() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.gantt_ui.placing_text = None;
            self.status.clear();
        }
        self.bars_dialog(ctx);
        self.chart_options_dialog(ctx);
        self.text_dialog(ctx);
        self.timescale_dialog(ctx);
        self.curtain_dialog(ctx);
    }

    fn bars_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.gantt_ui.bars.take() else {
            return;
        };
        // The bar grid over a three-tab page, buttons in the right-hand
        // column, the last OK or Apply error under the page.
        const W: f32 = 766.;
        const H: f32 = 411.;
        let (clicked, closed, actions) = kit::window(ctx, "bars", "Bars", Vec2::new(W, H), |c| {
            self.bar_list(c, &mut draft);
            self.bar_details(c, &mut draft);
            let k = draft.selected;
            let count = draft.bars.len();
            if let Some(error) = &draft.error {
                kit::error_text(c, 8., 393., 640., error);
            }
            kit::strip(c, W, H);
            kit::button_column(
                c,
                W,
                &[
                    Entry::ok("bars_ok"),
                    Entry::cancel("bars_cancel"),
                    Entry::apply("bars_apply", true),
                    Entry::Button("bars_add", "Add", count < MAX_BARS, false),
                    Entry::Button("bars_delete", "Delete", count > 0, false),
                    Entry::Button("bars_up", "Shift up", k > 0, false),
                    Entry::Button("bars_down", "Shift down", k + 1 < count, false),
                    Entry::Button("bars_options", "Options...", true, true),
                    Entry::Button("bars_default", "Default", true, false),
                    Entry::help("bars_help"),
                ],
            )
            .clicked
        });
        self.record_dialog_actions(actions);
        let mut ok = false;
        let mut apply = false;
        // The filter window is modal over Bars: Escape and the close box
        // reach Bars only while it is closed.
        let mut cancel = closed;
        let k = draft.selected;
        match clicked.as_deref() {
            Some("bars_ok") => ok = true,
            Some("bars_cancel") => cancel = true,
            Some("bars_apply") => apply = true,
            Some("bars_add") => {
                let at = (k + 1).min(draft.bars.len());
                draft.bars.insert(at, BarDef::added(draft.bars.len() + 1));
                draft.selected = at;
                draft.scroll = true;
            }
            Some("bars_delete") => {
                draft.bars.remove(k);
                draft.selected = k.min(draft.bars.len().saturating_sub(1));
            }
            Some("bars_up") => {
                draft.bars.swap(k, k - 1);
                draft.selected -= 1;
                draft.scroll = true;
            }
            Some("bars_down") => {
                draft.bars.swap(k, k + 1);
                draft.selected += 1;
                draft.scroll = true;
            }
            Some("bars_options") => self.open_chart_options(),
            Some("bars_default") => {
                draft.bars = default_bars();
                draft.selected = 0;
            }
            Some("bars_help") => self.open_help_for("bars"),
            _ => {}
        }
        self.bar_filter_window(ctx, &mut draft);
        if ok || apply {
            let mut settings = self.gantt.clone();
            settings.bars = draft.bars.clone();
            match self.apply_gantt(settings) {
                Ok(()) => draft.error = None,
                Err(error) => {
                    draft.error = Some(error);
                    ok = false;
                }
            }
        }
        if !(ok || cancel) {
            self.gantt_ui.bars = Some(draft);
        }
    }

    /// The bar grid on the shared list grid (`dialog_kit::list`): 18 px
    /// rows, #0078D7 selection, the measured column widths. The selected
    /// row edits in place: Display box, Name, Timescale; the Filter cell
    /// opens the filter list.
    fn bar_list(&mut self, c: &mut Canvas, draft: &mut BarsDraft) {
        // Name, Timescale and Filter sized to their text; the two user date
        // columns (never filled here) only to their titles.
        let font = kit::body_font();
        let widest = |texts: &mut dyn Iterator<Item = String>| -> f32 {
            texts
                .map(|t| {
                    c.ui.fonts_mut(|f| f.layout_no_wrap(t, font.clone(), INK).size().x)
                })
                .fold(0., f32::max)
                + 10.
        };
        // Short date titles, so the narrow empty columns show them whole.
        let titles = ["Name", "Timescale", "User Start", "User Finish", "Filter"];
        let mut measured = [0.; 5];
        measured[0] = widest(
            &mut draft
                .bars
                .iter()
                .map(|b| b.name.clone())
                .chain(["Name".into()]),
        );
        measured[1] = widest(
            &mut BarSource::ALL
                .iter()
                .map(|s| s.label().to_string())
                .chain(["Timescale".into()]),
        );
        measured[2] = widest(&mut std::iter::once(titles[2].to_string()));
        measured[3] = widest(&mut std::iter::once(titles[3].to_string()));
        measured[4] = widest(
            &mut draft
                .bars
                .iter()
                .map(|b| b.filter.label())
                .chain(["Filter".into()]),
        );
        let w = bar_list_widths(measured);
        let cols = [
            Col {
                title: "Display",
                width: w[0],
            },
            Col {
                title: titles[0],
                width: w[1],
            },
            Col {
                title: titles[1],
                width: w[2],
            },
            Col {
                title: titles[2],
                width: w[3],
            },
            Col {
                title: titles[3],
                width: w[4],
            },
            Col {
                title: titles[4],
                width: w[5],
            },
            Col {
                title: "Preview",
                width: w[6],
            },
        ];
        let selected = draft.selected.min(draft.bars.len().saturating_sub(1));
        draft.scroll = false;
        let mut toggled = None;
        let mut filter = None;
        let mut source = None;
        let names: Vec<String> = draft.bars.iter().map(|b| b.name.clone()).collect();
        let mut name = draft.bars.get(selected).map(|b| b.name.clone());
        let sources: Vec<(&str, bool)> = BarSource::ALL.iter().map(|s| (s.label(), true)).collect();
        let bars = draft.bars.clone();
        let out = kit::list(
            c,
            "bars",
            (5., 5., 643., 177.),
            None,
            &cols,
            draft.bars.len(),
            Some(selected),
            Some(0),
            &mut |c, r, col, cell, sel| {
                let bar = &bars[r];
                let local = cell.min - c.origin.to_vec2();
                // The row cut by the list's bottom edge shows text only.
                let whole = cell.bottom() <= c.origin.y + 181.;
                match col {
                    0..=2 if !whole => {
                        let text = match col {
                            1 => names[r].as_str(),
                            2 => bar.timescale.label(),
                            _ => "",
                        };
                        kit::cell_text(c, cell, text, sel, 3.);
                    }
                    0 => {
                        let x = local.x + ((cell.width() - 13.) / 2.).floor();
                        let y = local.y + 2.;
                        if c.hit(&format!("bars_display_{r}"), c.r(x, y, 13., 13.), true)
                            .clicked()
                        {
                            toggled = Some(r);
                        }
                        kit::check_glyph(c, x, y, bar.display, true);
                    }
                    1 if sel => {
                        // The in-place editor shows once the cell is clicked.
                        let key = format!("bars_name_{r}");
                        let id = c.id(&key);
                        match name.as_mut() {
                            Some(name) if c.ui.memory(|m| m.has_focus(id)) => {
                                c.edit(
                                    &key,
                                    local.x,
                                    local.y,
                                    cell.width() - 1.,
                                    kit::ROW,
                                    name,
                                    true,
                                    Align::Min,
                                );
                            }
                            _ => {
                                kit::cell_text(c, cell, &names[r], sel, 3.);
                                if c.hit(&key, cell, true).clicked() {
                                    c.ui.memory_mut(|m| m.request_focus(id));
                                }
                            }
                        }
                    }
                    1 => kit::cell_text(c, cell, &names[r], sel, 3.),
                    2 if sel => {
                        // The list drops down once the cell is clicked.
                        let key = format!("bars_source_{r}");
                        let open_id = c.id(&key);
                        let open =
                            c.ui.ctx()
                                .data(|d| d.get_temp::<bool>(open_id))
                                .unwrap_or(false);
                        if open {
                            let mut index = BarSource::ALL
                                .iter()
                                .position(|s| *s == bar.timescale)
                                .unwrap_or(0);
                            if c.combo(
                                &key,
                                local.x,
                                local.y,
                                cell.width() - 1.,
                                18.,
                                &sources,
                                &mut index,
                                true,
                            ) {
                                source = Some((r, BarSource::ALL[index]));
                            }
                        } else {
                            kit::cell_text(c, cell, bar.timescale.label(), sel, 3.);
                            if c.hit(&key, cell, true).clicked() {
                                c.ui.ctx().data_mut(|d| d.insert_temp(open_id, true));
                            }
                        }
                    }
                    2 => kit::cell_text(c, cell, bar.timescale.label(), sel, 3.),
                    5 => {
                        kit::cell_text(c, cell, &bar.filter.label(), sel, 3.);
                        let hit = c.hit(&format!("bars_filter_{r}"), cell, true);
                        if hit.double_clicked() || (sel && hit.clicked()) {
                            filter = Some(r);
                        }
                    }
                    6 => {
                        let area = cell.shrink2(Vec2::new(1., 0.));
                        let draw = bars::layout_bar(
                            bar,
                            area.left(),
                            area.top() + 1.,
                            area.left() + 8.,
                            area.right() - 8.,
                            &[],
                        );
                        bars::paint(&c.ui.painter().with_clip_rect(area), &draw);
                    }
                    _ => {}
                }
            },
        );
        if let (Some(name), Some(bar)) = (name, draft.bars.get_mut(selected)) {
            bar.name = name;
        }
        if let Some(r) = toggled {
            draft.bars[r].display = !draft.bars[r].display;
            draft.selected = r;
        }
        if let Some((r, s)) = source {
            draft.bars[r].timescale = s;
        }
        if let Some(r) = filter {
            draft.filter_edit = Some((r, draft.bars[r].filter.clone()));
            draft.selected = r;
        }
        if let Some(r) = out.clicked {
            draft.selected = r;
        }
        // The list body (below the header), for tests that scroll it.
        let body = c.r(6., 24., 641., 157.);
        c.act("bars_list", body);
    }

    fn bar_details(&mut self, c: &mut Canvas, draft: &mut BarsDraft) {
        let tabs = [
            (BarsTab::Style, "Bar Style", "style"),
            (BarsTab::Settings, "Bar Settings", "settings"),
            (BarsTab::Labels, "Bar Labels", "labels"),
        ];
        let vtabs: Vec<VTab> = tabs
            .iter()
            .map(|(_, label, key)| VTab {
                label,
                key,
                unavailable: None,
            })
            .collect();
        let active = tabs.iter().position(|t| t.0 == draft.tab).unwrap_or(0);
        if let Some(k) = kit::tabs_spread(c, "bars_tab_", (5., 210., 643., 178.), &vtabs, active) {
            draft.tab = tabs[k].0;
        }
        let Some(bar) = draft.bars.get_mut(draft.selected) else {
            kit::text(c, 22., 240., "No bars. Use Add or Default.", true);
            return;
        };
        match draft.tab {
            BarsTab::Style => {
                for (y, text) in [
                    (234., "Shape"),
                    (264., "Color"),
                    (295., "Pattern"),
                    (323., "Row"),
                ] {
                    kit::text(c, 22., y, text, true);
                }
                choose_drawn(
                    c,
                    "bars_start_shape",
                    (97., 230., 57.),
                    &mut bar.start.shape,
                    &EndShape::ALL,
                    |s| s.label().into(),
                    |p, r, s| draw_end_shape(p, r, s, true),
                );
                let mut middle = (bar.middle.shape, bar.middle.outline);
                let choices: Vec<(BodyShape, bool)> = BodyShape::ALL
                    .iter()
                    .flat_map(|&s| [(s, false), (s, true)])
                    .collect();
                if choose_drawn(
                    c,
                    "bars_middle_shape",
                    (163., 230., 171.),
                    &mut middle,
                    &choices,
                    |(s, o)| {
                        if o {
                            format!("{}, outlined", s.label())
                        } else {
                            s.label().into()
                        }
                    },
                    draw_body_shape,
                ) {
                    bar.middle.shape = middle.0;
                    bar.middle.outline = middle.1;
                }
                choose_drawn(
                    c,
                    "bars_end_shape",
                    (343., 230., 57.),
                    &mut bar.end.shape,
                    &EndShape::ALL,
                    |s| s.label().into(),
                    |p, r, s| draw_end_shape(p, r, s, false),
                );
                for (key, color, x, w) in [
                    ("bars_start_color", &mut bar.start.color, 97., 57.),
                    ("bars_middle_color", &mut bar.middle.color, 163., 171.),
                    ("bars_end_color", &mut bar.end.color, 343., 57.),
                ] {
                    kit::swatch(c, key, (x, 260., w, 21.), color, true);
                }
                let shown = [
                    bar.start.shape != EndShape::None,
                    bar.middle.shape != BodyShape::None,
                    bar.end.shape != EndShape::None,
                ];
                let fields = [
                    ("bars_start_pattern", &mut bar.start.pattern, 97., 57.),
                    ("bars_middle_pattern", &mut bar.middle.pattern, 163., 171.),
                    ("bars_end_pattern", &mut bar.end.pattern, 343., 57.),
                ];
                for ((key, pattern, x, w), shown) in fields.into_iter().zip(shown) {
                    // A part without a shape shows an empty pattern field.
                    choose_drawn(
                        c,
                        key,
                        (x, 291., w),
                        pattern,
                        &Pattern::ALL,
                        |p| p.label().into(),
                        |p, r, pattern| {
                            if shown {
                                draw_pattern(p, r, pattern)
                            }
                        },
                    );
                }
                if let Some(row) = spin_int(
                    c,
                    "bars_row_spin",
                    (97., 319., 58., 21.),
                    bar.row.into(),
                    (1, MAX_BAR_ROW.into()),
                    true,
                ) {
                    bar.row = row as u8;
                }
            }
            BarsTab::Settings => {
                kit::heading(c, 12., 220., 628., "Grouping Band Settings");
                c.checkbox(
                    "bars_show_collapsed",
                    24.,
                    246.,
                    376.,
                    18.,
                    "Show bar when collapsed",
                    &mut bar.show_when_collapsed,
                    true,
                );
                c.checkbox(
                    "bars_show_bands",
                    24.,
                    268.,
                    376.,
                    18.,
                    "Show bar for grouping bands",
                    &mut bar.show_for_grouping_bands,
                    true,
                );
                kit::heading(c, 12., 296., 628., "Bar Necking Settings");
                c.checkbox(
                    "bars_neck_calendar",
                    24.,
                    322.,
                    376.,
                    18.,
                    "Calendar nonwork time",
                    &mut bar.neck_calendar_nonwork,
                    true,
                );
                let _ = c
                    .checkbox(
                        "bars_neck_activity",
                        24.,
                        344.,
                        376.,
                        18.,
                        "Activity nonwork intervals",
                        &mut bar.neck_activity_nonwork,
                        true,
                    )
                    .on_hover_text("Applies to suspended intervals; none are imported yet.");
            }
            BarsTab::Labels => {
                let labels = bar.labels.clone();
                let cols = [
                    Col {
                        title: "Position",
                        width: 150.,
                    },
                    Col {
                        title: "Label",
                        width: 337.,
                    },
                ];
                let positions: Vec<String> = LabelPosition::ALL
                    .iter()
                    .map(|p| p.label().to_string())
                    .collect();
                let fields: Vec<String> = LabelField::ALL
                    .iter()
                    .map(|f| f.label().to_string())
                    .collect();
                let position_items: Vec<(&str, bool)> =
                    positions.iter().map(|p| (p.as_str(), true)).collect();
                let field_items: Vec<(&str, bool)> =
                    fields.iter().map(|f| (f.as_str(), true)).collect();
                let mut changed: Option<(usize, BarLabel)> = None;
                let selected = draft.label.filter(|k| *k < labels.len());
                let out = kit::list(
                    c,
                    "bars_labels",
                    (12., 218., 508., 160.),
                    None,
                    &cols,
                    labels.len(),
                    selected,
                    None,
                    &mut |c, r, col, cell, sel| {
                        let label = labels[r];
                        if !sel {
                            let text = if col == 0 {
                                label.position.label()
                            } else {
                                label.field.label()
                            };
                            kit::cell_text(c, cell, text, sel, 3.);
                            return;
                        }
                        // The selected label edits in place.
                        let local = cell.min - c.origin.to_vec2();
                        let mut edited = label;
                        if col == 0 {
                            let mut k = LabelPosition::ALL
                                .iter()
                                .position(|p| *p == label.position)
                                .unwrap_or(0);
                            if c.combo(
                                &format!("bars_label_position_{r}"),
                                local.x,
                                local.y,
                                cell.width() - 1.,
                                kit::ROW,
                                &position_items,
                                &mut k,
                                true,
                            ) {
                                edited.position = LabelPosition::ALL[k];
                                changed = Some((r, edited));
                            }
                        } else {
                            let mut k = LabelField::ALL
                                .iter()
                                .position(|f| *f == label.field)
                                .unwrap_or(0);
                            if c.combo(
                                &format!("bars_label_field_{r}"),
                                local.x,
                                local.y,
                                cell.width() - 1.,
                                kit::ROW,
                                &field_items,
                                &mut k,
                                true,
                            ) {
                                edited.field = LabelField::ALL[k];
                                changed = Some((r, edited));
                            }
                        }
                    },
                );
                if let Some((r, label)) = changed {
                    bar.labels[r] = label;
                }
                if let Some(r) = out.clicked {
                    draft.label = Some(r);
                }
                let add = kit::button(
                    c,
                    "bars_label_add",
                    527.,
                    222.,
                    kit::BUTTON.x,
                    kit::BUTTON.y,
                    "Add",
                    bar.labels.len() < MAX_LABELS,
                );
                let delete = kit::button(
                    c,
                    "bars_label_delete",
                    527.,
                    252.,
                    kit::BUTTON.x,
                    kit::BUTTON.y,
                    "Delete",
                    !bar.labels.is_empty(),
                );
                if add {
                    bar.labels.push(BarLabel {
                        position: LabelPosition::Right,
                        field: LabelField::ActivityId,
                    });
                    draft.label = Some(bar.labels.len() - 1);
                }
                if delete && !bar.labels.is_empty() {
                    let k = draft
                        .label
                        .unwrap_or(bar.labels.len() - 1)
                        .min(bar.labels.len() - 1);
                    bar.labels.remove(k);
                    draft.label = None;
                }
            }
        }
    }

    /// The filter of one bar: which activity kinds it draws for. Modal over
    /// Bars, so Escape closes it and leaves Bars open.
    fn bar_filter_window(&mut self, ctx: &egui::Context, draft: &mut BarsDraft) {
        let Some((index, mut filter)) = draft.filter_edit.take() else {
            return;
        };
        const W: f32 = 412.;
        const H: f32 = 234.;
        let (clicked, closed, actions) =
            kit::modal(ctx, "bar_filter", "Filters", Vec2::new(W, H), |c| {
                c.fill(0., 0., W, H, kit::FACE);
                kit::text(c, 14., 12., "Show activities that match", true);
                if c.radio(
                    "bar_filter_all",
                    24.,
                    30.,
                    250.,
                    18.,
                    "All selected filters",
                    filter.match_all,
                    true,
                ) {
                    filter.match_all = true;
                }
                if c.radio(
                    "bar_filter_any",
                    24.,
                    50.,
                    250.,
                    18.,
                    "Any selected filter",
                    !filter.match_all,
                    true,
                ) {
                    filter.match_all = false;
                }
                c.etched(14., 76., 272.);
                for (k, kind) in FilterKind::ALL.into_iter().enumerate() {
                    let (x, y) = (24. + 136. * (k / 7) as f32, 86. + 20. * (k % 7) as f32);
                    let mut on = filter.kinds.contains(&kind);
                    let before = on;
                    c.checkbox(
                        &format!("bar_filter_{kind:?}"),
                        x,
                        y,
                        132.,
                        18.,
                        kind.label(),
                        &mut on,
                        true,
                    );
                    if on && !before {
                        filter.kinds.push(kind);
                    } else if before && !on {
                        filter.kinds.retain(|k| *k != kind);
                    }
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("bar_filter_ok", "OK", !filter.kinds.is_empty(), false),
                        Entry::cancel("bar_filter_cancel"),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("bar_filter_ok") => {
                if let Some(bar) = draft.bars.get_mut(index) {
                    bar.filter = filter;
                }
            }
            Some(_) => {}
            None if closed => {}
            None => draft.filter_edit = Some((index, filter)),
        }
    }

    fn chart_options_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.gantt_ui.options.take() else {
            return;
        };
        let origin = self.project.model.origin_serial;
        // A tab page at x 5-482, y 5-328 (tabs above y 26), and OK, Cancel
        // and Help in the right-hand column.
        const W: f32 = 600.;
        const H: f32 = 334.;
        let (clicked, closed, actions) =
            kit::window(ctx, "chart", "Bar Chart Options", Vec2::new(W, H), |c| {
                let tabs = [
                    (OptionsTab::General, "General", "general"),
                    (OptionsTab::Collapsed, "Collapsed Bars", "collapsed"),
                    (OptionsTab::DataDate, "Data Date", "datadate"),
                    (OptionsTab::SightLines, "Sight Lines", "sightlines"),
                    (OptionsTab::ProgressLine, "Progress Line", "progress"),
                ];
                let vtabs: Vec<VTab> = tabs
                    .iter()
                    .map(|(_, label, key)| VTab {
                        label,
                        key,
                        unavailable: None,
                    })
                    .collect();
                let active = tabs.iter().position(|t| t.0 == draft.tab).unwrap_or(0);
                if let Some(k) =
                    kit::tabs_spread(c, "chart_tab_", (5., 26., 477., 302.), &vtabs, active)
                {
                    draft.tab = tabs[k].0;
                }
                chart_options_tab(c, &mut draft, origin);
                if let Some(error) = &draft.error {
                    kit::error_text(c, 14., 306., 460., error);
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("chart_ok"),
                        Entry::cancel("chart_cancel"),
                        Entry::help("chart_help"),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        if clicked == Some("chart_help") {
            self.open_help_for("chart");
        }
        let mut ok = clicked == Some("chart_ok");
        if ok {
            let mut settings = self.gantt.clone();
            settings.options = draft.options;
            match self.apply_gantt(settings) {
                Ok(()) => {
                    self.view.row_height = draft.row_height.clamp(17., 48.);
                    self.save_view();
                }
                Err(error) => {
                    draft.error = Some(error);
                    ok = false;
                }
            }
        }
        if !(ok || clicked == Some("chart_cancel") || closed) {
            self.gantt_ui.options = Some(draft);
        }
    }

    fn timescale_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.gantt_ui.timescale.take() else {
            return;
        };
        let origin = self.project.model.origin_serial;
        // Controls at fixed client positions; the last OK or Apply error
        // under the panel.
        const W: f32 = 561.;
        const H: f32 = 420.;
        let (clicked, closed, actions) =
            kit::window(ctx, "ts", "Timescale", Vec2::new(W, H), |c| {
                c.lowered(5., 5., 433., 392.);
                let t = &mut draft.timescale;
                kit::heading(c, 11., 14., 421., "Timescale Format");
                if c.radio(
                    "ts_two_lines",
                    24.,
                    44.,
                    100.,
                    16.,
                    "Two Lines",
                    !t.three_lines,
                    true,
                ) {
                    t.three_lines = false;
                }
                if c.radio(
                    "ts_three_lines",
                    156.,
                    44.,
                    104.,
                    16.,
                    "Three Lines",
                    t.three_lines,
                    true,
                ) {
                    t.three_lines = true;
                }
                kit::text(c, 23., 81., "Timescale Start", true);
                anchored_date(
                    c,
                    "ts_start",
                    (166., 77.),
                    &mut t.start,
                    &mut draft.start_text,
                    origin,
                    true,
                );
                kit::text(c, 23., 113., "Font & Color", true);
                let mut points = f64::from(t.font_points);
                if kit::number_spin(
                    c,
                    "ts_font_size",
                    (166., 109., 54., 21.),
                    &mut points,
                    0.5,
                    6.0..=16.0,
                    true,
                ) {
                    t.font_points = points as f32;
                }
                kit::text(c, 224., 113., "pt", true);
                c.checkbox(
                    "ts_font_bold",
                    246.,
                    110.,
                    60.,
                    18.,
                    "Bold",
                    &mut t.font_bold,
                    true,
                );
                kit::swatch(
                    c,
                    "ts_font_color",
                    (310., 109., 57., 21.),
                    &mut t.font_color,
                    true,
                );
                kit::heading(c, 11., 145., 421., "Date Format");
                if c.radio(
                    "ts_primary",
                    24.,
                    172.,
                    176.,
                    16.,
                    "Show Primary Dates",
                    !t.ordinal,
                    true,
                ) {
                    t.ordinal = false;
                }
                let primary = !t.ordinal;
                kit::text(c, 42., 202., "Type", primary);
                choose(
                    c,
                    "ts_type",
                    (177., 198., 130.),
                    &mut t.date_type,
                    &DateType::ALL,
                    |d| d.label().into(),
                    |_| true,
                    primary,
                );
                // Beside Type while it is Fiscal: the fiscal year's first
                // month.
                if t.date_type == DateType::Fiscal {
                    let months: Vec<u32> = (1..=12).collect();
                    choose(
                        c,
                        "ts_fiscal",
                        (312., 198., 120.),
                        &mut t.fiscal_start_month,
                        &months,
                        |m| MONTHS[(m - 1) as usize].into(),
                        |_| true,
                        primary,
                    );
                }
                kit::text(c, 42., 232., "Date Interval", primary);
                choose(
                    c,
                    "ts_interval",
                    (177., 228., 158.),
                    &mut t.interval,
                    &DateInterval::ALL,
                    |d| d.label().into(),
                    |d| d.available(),
                    primary,
                );
                // The first day of the week.
                choose(
                    c,
                    "ts_week_start",
                    (340., 228., 92.),
                    &mut t.week_start,
                    &[Weekday::Sunday, Weekday::Monday],
                    |w| format!("{w:?} weeks"),
                    |_| true,
                    primary,
                );
                kit::text(c, 42., 261., "Financial Period Calendar", false);
                kit::field(c, 177., 256., 130., 21., "", false);
                kit::text(c, 42., 288., "Shift Calendar", false);
                kit::field(c, 177., 284., 130., 21., "", false);
                c.etched(11., 312., 421.);
                if c.radio(
                    "ts_ordinal",
                    24.,
                    318.,
                    176.,
                    16.,
                    "Show Ordinal Dates",
                    t.ordinal,
                    true,
                ) {
                    t.ordinal = true;
                }
                let ordinal = t.ordinal;
                kit::text(c, 42., 345., "Ordinal Start", ordinal);
                anchored_date(
                    c,
                    "ts_ordinal_start",
                    (177., 340.),
                    &mut t.ordinal_start,
                    &mut draft.ordinal_text,
                    origin,
                    ordinal,
                );
                kit::text(c, 42., 371., "Ordinal Date Interval", ordinal);
                choose(
                    c,
                    "ts_ordinal_unit",
                    (177., 366., 130.),
                    &mut t.ordinal_unit,
                    &OrdinalUnit::ALL,
                    |u| u.label().into(),
                    |_| true,
                    ordinal,
                );
                if let Some(error) = &draft.error {
                    kit::error_text(c, 8., 401., 425., error);
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("ts_ok"),
                        Entry::cancel("ts_cancel"),
                        Entry::apply("ts_apply", true),
                        Entry::Button("ts_default_font", "Default Font", true, false),
                        Entry::help("ts_help"),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        match clicked {
            Some("ts_help") => self.open_help_for("ts"),
            Some("ts_default_font") => {
                let t = &mut draft.timescale;
                t.font_points = 8.;
                t.font_bold = false;
                t.font_color = Rgb::BLACK;
            }
            _ => {}
        }
        let mut ok = clicked == Some("ts_ok");
        if ok || clicked == Some("ts_apply") {
            let mut settings = self.gantt.clone();
            let start_changed = settings.timescale.start != draft.timescale.start;
            settings.timescale = draft.timescale.clone();
            match self.apply_gantt(settings) {
                Ok(()) => {
                    draft.error = None;
                    if start_changed {
                        self.timeline_origin = self.resolve_date(self.gantt.timescale.start);
                    }
                }
                Err(error) => {
                    draft.error = Some(error);
                    ok = false;
                }
            }
        }
        if !(ok || clicked == Some("ts_cancel") || closed) {
            self.gantt_ui.timescale = Some(draft);
        }
    }

    fn curtain_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.gantt_ui.curtain.take() else {
            return;
        };
        let origin = self.project.model.origin_serial;
        const W: f32 = 382.;
        const H: f32 = 170.;
        let existing = draft.index.is_some();
        let (clicked, closed, actions) =
            kit::window(ctx, "curtain", "Curtain Attachment", Vec2::new(W, H), |c| {
                let k = &mut draft.curtain;
                c.checkbox(
                    "curtain_display",
                    14.,
                    12.,
                    240.,
                    18.,
                    "Display curtain attachment",
                    &mut k.display,
                    true,
                );
                kit::text(c, 14., 42., "Start Date", true);
                c.edit(
                    "curtain_start",
                    100.,
                    38.,
                    150.,
                    21.,
                    &mut draft.start_text,
                    true,
                    Align::Min,
                );
                kit::text(c, 14., 70., "Finish Date", true);
                c.edit(
                    "curtain_finish",
                    100.,
                    66.,
                    150.,
                    21.,
                    &mut draft.finish_text,
                    true,
                    Align::Min,
                );
                kit::text(c, 14., 98., "Color", true);
                kit::swatch(
                    c,
                    "curtain_color",
                    (100., 94., 57., 21.),
                    &mut k.color,
                    true,
                );
                kit::text(c, 14., 126., "Fill pattern", true);
                choose(
                    c,
                    "curtain_pattern",
                    (100., 122., 150.),
                    &mut k.pattern,
                    &Pattern::ALL,
                    |p| p.label().into(),
                    |_| true,
                    true,
                );
                if let Some(error) = &draft.error {
                    kit::error_text(c, 14., 150., 250., error);
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("curtain_ok"),
                        Entry::cancel("curtain_cancel"),
                        Entry::apply("curtain_apply", true),
                        Entry::Button("curtain_delete", "Delete", existing, false),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        if clicked == Some("curtain_delete") {
            if let Some(index) = draft.index {
                let mut settings = self.gantt.clone();
                settings.curtains.remove(index);
                let _ = self.apply_gantt(settings);
            }
            return;
        }
        let mut ok = clicked == Some("curtain_ok");
        if ok || clicked == Some("curtain_apply") {
            let parse = |text: &str| parse_relative(origin, text).ok().flatten();
            match (parse(&draft.start_text), parse(&draft.finish_text)) {
                (Some(start), Some(finish)) => {
                    draft.curtain.start = start;
                    draft.curtain.finish = finish;
                    let mut settings = self.gantt.clone();
                    match draft.index {
                        Some(index) if index < settings.curtains.len() => {
                            settings.curtains[index] = draft.curtain.clone()
                        }
                        _ => settings.curtains.push(draft.curtain.clone()),
                    }
                    let new_index = draft.index.unwrap_or(settings.curtains.len() - 1);
                    match self.apply_gantt(settings) {
                        Ok(()) => {
                            draft.index = Some(new_index);
                            draft.error = None;
                        }
                        Err(error) => {
                            draft.error = Some(error);
                            ok = false;
                        }
                    }
                }
                _ => {
                    draft.error = Some("Enter dates as YYYY-MM-DD or YYYY-MM-DD HH:MM.".into());
                    ok = false;
                }
            }
        }
        if !(ok || clicked == Some("curtain_cancel") || closed) {
            self.gantt_ui.curtain = Some(draft);
        }
    }
}

/// One page of Bar Chart Options, inside the tab page at (5, 26).
fn chart_options_tab(c: &mut Canvas, draft: &mut OptionsDraft, origin: i64) {
    let o = &mut draft.options;
    match draft.tab {
        OptionsTab::General => {
            c.checkbox(
                "chart_show_relationships",
                14.,
                36.,
                200.,
                18.,
                "Show Relationships",
                &mut o.show_relationships,
                true,
            );
            let on = o.show_relationships;
            let rc = &mut o.relationship_colors;
            kit::text(c, 32., 61., "Driving", on);
            kit::swatch(c, "rel_driving", (76., 59., 30., 18.), &mut rc.driving, on);
            kit::text(c, 116., 61., "Non-driving", on);
            kit::swatch(
                c,
                "rel_non_driving",
                (178., 59., 30., 18.),
                &mut rc.non_driving,
                on,
            );
            kit::text(c, 218., 61., "Critical", on);
            kit::swatch(
                c,
                "rel_critical",
                (260., 59., 30., 18.),
                &mut rc.critical,
                on,
            );
            c.checkbox(
                "chart_dotted_non_driving",
                300.,
                59.,
                170.,
                18.,
                "Dotted non-driving",
                &mut rc.non_driving_dotted,
                on,
            );
            c.checkbox(
                "chart_show_legend",
                14.,
                84.,
                200.,
                18.,
                "Show Legend",
                &mut o.show_legend,
                true,
            );
            c.checkbox(
                "chart_limit_label_chars",
                14.,
                109.,
                190.,
                18.,
                "Limit text label characters to",
                &mut o.limit_label_chars,
                true,
            );
            if let Some(v) = spin_int(
                c,
                "chart_label_chars",
                (206., 107., 60., 21.),
                o.label_chars.into(),
                (1, 999),
                true,
            ) {
                o.label_chars = v as u16;
            }
            kit::section(c, 14., 136., 460., 54., "Default Size For Notebook Items");
            kit::text(c, 24., 166., "Width:", true);
            if let Some(v) = spin_int(
                c,
                "chart_notebook_width",
                (66., 162., 60., 21.),
                o.notebook_width.into(),
                (10, 999),
                true,
            ) {
                o.notebook_width = v as u16;
            }
            kit::text(c, 132., 166., "Pixels", true);
            kit::text(c, 200., 166., "Height:", true);
            if let Some(v) = spin_int(
                c,
                "chart_notebook_height",
                (244., 162., 60., 21.),
                o.notebook_height.into(),
                (10, 999),
                true,
            ) {
                o.notebook_height = v as u16;
            }
            kit::text(c, 310., 166., "Pixels", true);
            kit::section(c, 14., 196., 460., 80., "Nonwork Time");
            c.checkbox(
                "chart_shade_nonwork",
                24.,
                224.,
                180.,
                18.,
                "Shade calendar nonwork time",
                &mut o.nonwork.shade,
                true,
            );
            c.checkbox(
                "chart_hatch_nonwork",
                210.,
                224.,
                110.,
                18.,
                "Diagonal hatch",
                &mut o.nonwork.hatch,
                true,
            );
            kit::text(c, 330., 226., "Color", true);
            kit::swatch(
                c,
                "nonwork_color",
                (366., 224., 30., 18.),
                &mut o.nonwork.color,
                true,
            );
            kit::text(c, 24., 252., "Row height", true);
            let mut height = f64::from(draft.row_height);
            if kit::number_spin(
                c,
                "row_height",
                (100., 248., 60., 21.),
                &mut height,
                1.,
                17.0..=48.0,
                true,
            ) {
                draft.row_height = height.round() as f32;
            }
        }
        OptionsTab::Collapsed => {
            kit::section(c, 14., 36., 460., 124., "Collapsed Bar Formatting");
            let bands = o.collapse == CollapseMode::GroupingBands;
            if c.radio(
                "chart_collapse_bands",
                24.,
                64.,
                400.,
                18.,
                "Collapse to grouping bands",
                bands,
                true,
            ) {
                o.collapse = CollapseMode::GroupingBands;
            }
            c.checkbox(
                "chart_collapsed_relationships",
                42.,
                86.,
                400.,
                18.,
                "Show relationships for collapsed bars",
                &mut o.collapsed_relationships,
                bands,
            );
            if c.radio(
                "chart_collapse_bars",
                24.,
                110.,
                400.,
                18.,
                "Collapse to individual bars to minimize space",
                !bands,
                true,
            ) {
                o.collapse = CollapseMode::IndividualBars;
            }
            let _ = c
                .checkbox(
                    "chart_label_overlap",
                    42.,
                    132.,
                    400.,
                    18.,
                    "Allow overlapping of bar labels",
                    &mut o.allow_label_overlap,
                    false,
                )
                .on_hover_text("Collapsed bars draw without labels.");
        }
        OptionsTab::DataDate => {
            kit::section(c, 14., 36., 460., 112., "Data Date Formatting");
            kit::text(c, 24., 66., "Style", true);
            choose(
                c,
                "dd_style",
                (100., 62., 140.),
                &mut o.data_date.style,
                &LineStyle::ALL,
                |s| s.label().into(),
                |_| true,
                true,
            );
            kit::text(c, 24., 94., "Size", true);
            if let Some(v) = spin_int(
                c,
                "chart_dd_size",
                (100., 90., 60., 21.),
                o.data_date.size.into(),
                (1, 5),
                true,
            ) {
                o.data_date.size = v as u8;
            }
            kit::text(c, 24., 122., "Color", true);
            kit::swatch(
                c,
                "dd_color",
                (100., 118., 57., 21.),
                &mut o.data_date.color,
                true,
            );
        }
        OptionsTab::SightLines => {
            let s = &mut o.sight_lines;
            kit::section(c, 14., 36., 460., 82., "Horizontal Lines");
            let _ = c
                .checkbox(
                    "chart_hmajor",
                    24.,
                    64.,
                    150.,
                    18.,
                    "Show Major Lines",
                    &mut s.horizontal_major.show,
                    true,
                )
                .on_hover_text("Major lines run along grouping bands.");
            choose(
                c,
                "hmaj_style",
                (264., 62., 110.),
                &mut s.horizontal_major.style,
                &LineStyle::ALL,
                |l| l.label().into(),
                |_| true,
                true,
            );
            kit::swatch(
                c,
                "hmaj_color",
                (380., 63., 30., 18.),
                &mut s.horizontal_major.color,
                true,
            );
            c.checkbox(
                "chart_hminor",
                24.,
                91.,
                150.,
                18.,
                "Show Minor Lines every",
                &mut s.horizontal_minor.show,
                true,
            );
            if let Some(v) = spin_int(
                c,
                "chart_hminor_rows",
                (176., 89., 50., 21.),
                s.horizontal_minor.every_rows.into(),
                (1, 999),
                true,
            ) {
                s.horizontal_minor.every_rows = v as u16;
            }
            kit::text(c, 230., 93., "Rows", true);
            choose(
                c,
                "hmin_style",
                (264., 89., 110.),
                &mut s.horizontal_minor.style,
                &LineStyle::ALL,
                |l| l.label().into(),
                |_| true,
                true,
            );
            kit::swatch(
                c,
                "hmin_color",
                (380., 90., 30., 18.),
                &mut s.horizontal_minor.color,
                true,
            );
            kit::section(c, 14., 124., 460., 140., "Vertical Lines");
            for (n, (major, key)) in [(true, "vmajor"), (false, "vminor")]
                .into_iter()
                .enumerate()
            {
                let y = 152. + 56. * n as f32;
                let lines = if major {
                    &mut s.vertical_major
                } else {
                    &mut s.vertical_minor
                };
                c.checkbox(
                    &format!("chart_{key}"),
                    24.,
                    y + 2.,
                    150.,
                    18.,
                    if major {
                        "Show Major Lines every"
                    } else {
                        "Show Minor Lines every"
                    },
                    &mut lines.show,
                    true,
                );
                if let Some(v) = spin_int(
                    c,
                    &format!("chart_{key}_every"),
                    (176., y, 50., 21.),
                    lines.every.into(),
                    (1, 999),
                    true,
                ) {
                    lines.every = v as u16;
                }
                choose(
                    c,
                    &format!("{key}_unit"),
                    (230., y, 80.),
                    &mut lines.unit,
                    &SightUnit::ALL,
                    |u| u.label().into(),
                    |_| true,
                    true,
                );
                choose(
                    c,
                    &format!("{key}_style"),
                    (314., y, 90.),
                    &mut lines.style,
                    &LineStyle::ALL,
                    |l| l.label().into(),
                    |_| true,
                    true,
                );
                kit::swatch(
                    c,
                    &format!("{key}_color"),
                    (410., y + 1., 30., 18.),
                    &mut lines.color,
                    true,
                );
                kit::text(c, 42., y + 30., "Start", lines.show);
                let mut text = match lines.start.anchor {
                    Anchor::Date(m) => date_string(origin, Some(m)),
                    _ => String::new(),
                };
                let show = lines.show;
                anchored_date(
                    c,
                    key,
                    (100., y + 26.),
                    &mut lines.start,
                    &mut text,
                    origin,
                    show,
                );
            }
        }
        OptionsTab::ProgressLine => {
            let p = &mut o.progress_line;
            kit::section(c, 14., 36., 460., 248., "Progress Line");
            c.checkbox(
                "chart_progress_show",
                24.,
                64.,
                200.,
                18.,
                "Display progress line",
                &mut p.show,
                true,
            );
            kit::text(c, 24., 92., "Size", true);
            if let Some(v) = spin_int(
                c,
                "chart_progress_size",
                (60., 88., 50., 21.),
                p.size.into(),
                (1, 5),
                true,
            ) {
                p.size = v as u8;
            }
            kit::text(c, 130., 92., "Color", true);
            kit::swatch(
                c,
                "progress_color",
                (166., 88., 57., 21.),
                &mut p.color,
                true,
            );
            kit::text(
                c,
                24.,
                120.,
                "Baseline to use for calculating Progress Line:",
                true,
            );
            choose(
                c,
                "progress_baseline",
                (24., 136., 160.),
                &mut p.baseline,
                &crate::baselines::Slot::ALL,
                |s| s.label().into(),
                |_| true,
                true,
            );
            kit::text(c, 24., 166., "Draw progress line:", true);
            if c.radio(
                "chart_progress_difference",
                24.,
                184.,
                440.,
                18.,
                "Based on difference between current and baseline activity's",
                p.mode == ProgressMode::Difference,
                true,
            ) {
                p.mode = ProgressMode::Difference;
            }
            choose(
                c,
                "progress_date",
                (42., 206., 160.),
                &mut p.date,
                &[ProgressDate::Start, ProgressDate::Finish],
                |d| d.label().into(),
                |_| true,
                true,
            );
            if c.radio(
                "chart_progress_points",
                24.,
                236.,
                440.,
                18.,
                "By connecting progress points based on activity's",
                p.mode == ProgressMode::Points,
                true,
            ) {
                p.mode = ProgressMode::Points;
            }
            choose(
                c,
                "progress_basis",
                (42., 258., 160.),
                &mut p.basis,
                &[
                    ProgressBasis::PercentComplete,
                    ProgressBasis::RemainingDuration,
                ],
                |b| b.label().into(),
                |_| true,
                true,
            );
        }
    }
}

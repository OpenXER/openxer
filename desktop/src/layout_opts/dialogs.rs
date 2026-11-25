//! Table, Font and Row; Chart Font and Colors; the font picker both open;
//! Collapse To; New Layout. Each edits a draft: OK and Apply commit it,
//! Cancel, the title bar's close button and Escape drop it.
use super::*;
use crate::dialog_kit::{self as kit, Col};
use crate::gantt::settings::{ChartFont, ChartOptions, Rgb, TextFont};
use crate::grid::layout::TableDisplay;
use crate::schedule_dialogs::Canvas;
use chrome::Face;

/// The button column at the right of each dialog.
/// Sizes the font picker offers.
const SIZES: [f32; 11] = [6., 7., 8., 9., 10., 11., 12., 14., 16., 18., 20.];

#[derive(Default)]
pub(crate) struct Drafts {
    pub table: Option<TableDraft>,
    pub chart: Option<ChartDraft>,
    pub collapse: Option<CollapseDraft>,
    pub new_layout: Option<NewLayoutDraft>,
    pub picker: Option<FontPicker>,
}

/// The table Table Font and Row works on: the Activities table, or the
/// WBS or Projects window's table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TableTarget {
    #[default]
    Activities,
    Wbs,
    Projects,
}

/// The band levels Collapse To works on: the Activities table's, or the
/// tree of the WBS or Projects window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CollapseTarget {
    #[default]
    Activities,
    Wbs,
    Projects,
}

pub(crate) struct TableDraft {
    /// Whose table the dialog sets.
    pub target: TableTarget,
    pub display: TableDisplay,
    /// Keep Current Row Heights: OK leaves the row height alone.
    pub keep: bool,
    /// Optimize height by row content (else Select height for all rows).
    pub optimize: bool,
    /// Do not exceed `lines` lines per row.
    pub limit: bool,
    pub lines: String,
    pub height: String,
    /// Why the last OK or Apply was refused, shown beside the row height.
    pub error: Option<String>,
}

impl TableDraft {
    /// The typed row height or line limit, when OK would use it and it is
    /// not a number in range.
    fn invalid(&self) -> Option<String> {
        if self.keep {
            return None;
        }
        if self.optimize {
            let ok = !self.limit
                || self
                    .lines
                    .trim()
                    .parse::<u8>()
                    .is_ok_and(|n| (1..=20).contains(&n));
            return (!ok).then(|| "Lines per row: enter 1 to 20.".to_string());
        }
        let ok = self
            .height
            .trim()
            .parse::<f32>()
            .is_ok_and(|h| (17.0..=48.).contains(&h));
        (!ok).then(|| "Row height: enter 17 to 48.".to_string())
    }
}

pub(crate) struct ChartDraft {
    pub options: ChartOptions,
}

pub(crate) struct CollapseDraft {
    pub levels: Vec<String>,
    pub selected: Option<usize>,
}

pub(crate) struct NewLayoutDraft {
    pub name: String,
    pub from_current: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PickerFor {
    Table,
    Chart,
    /// The font of the text attachment being edited.
    Text,
}

pub(crate) struct FontPicker {
    pub target: PickerFor,
    pub face: Face,
    pub points: f32,
    pub bold: bool,
    pub italic: bool,
    pub color: Rgb,
}

impl FontPicker {
    fn styles(&self) -> &'static [&'static str] {
        match self.target {
            PickerFor::Table | PickerFor::Text => &["Regular", "Italic", "Bold", "Bold Italic"],
            PickerFor::Chart => &["Regular", "Bold"],
        }
    }
    fn style_index(&self) -> usize {
        match self.target {
            PickerFor::Table | PickerFor::Text => {
                usize::from(self.italic) + 2 * usize::from(self.bold)
            }
            PickerFor::Chart => usize::from(self.bold),
        }
    }
    fn set_style(&mut self, index: usize) {
        match self.target {
            PickerFor::Table | PickerFor::Text => {
                self.italic = index % 2 == 1;
                self.bold = index >= 2;
            }
            PickerFor::Chart => {
                self.bold = index == 1;
                self.italic = false;
            }
        }
    }
}

fn rgb(c: [u8; 3]) -> Rgb {
    Rgb(c[0], c[1], c[2])
}
fn bytes(c: Rgb) -> [u8; 3] {
    [c.0, c.1, c.2]
}

/// The raised `AaBbYyZz` button showing a font; true on click.
pub(crate) fn sample_button(
    c: &mut Canvas,
    key: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    font: FontId,
    italic: bool,
    color: Color32,
) -> bool {
    let clicked = c.button_with_icon(key, (x, y, w, h), "", None, None, false, true);
    let mut job = egui::text::LayoutJob::single_section(
        "AaBbYyZz".into(),
        egui::TextFormat {
            font_id: font,
            color,
            italics: italic,
            ..Default::default()
        },
    );
    job.wrap.max_width = f32::INFINITY;
    let galley = c.ui.painter().layout_job(job);
    let rect = c.r(x, y, w, h);
    let pos = rect.center() - galley.size() / 2.;
    c.ui.painter()
        .with_clip_rect(rect.shrink(2.))
        .galley(pos.round(), galley, color);
    clicked
}

impl App {
    pub(crate) fn open_table_font(&mut self) {
        self.layout_opts.drafts.table = Some(TableDraft {
            target: TableTarget::Activities,
            display: self.grid.layout.table.clone(),
            keep: true,
            optimize: self.grid.layout.table.optimize_rows.is_some(),
            limit: self.grid.layout.table.optimize_rows.is_some_and(|n| n > 0),
            lines: self
                .grid
                .layout
                .table
                .optimize_rows
                .filter(|n| *n > 0)
                .unwrap_or(2)
                .to_string(),
            height: format!("{}", self.view.row_height.round()),
            error: None,
        });
    }

    /// Table Font and Row for `target`: the WBS and Projects windows keep
    /// their own font, cell colour and row height (rows there are all one
    /// height).
    pub(crate) fn open_table_font_for(&mut self, target: TableTarget) {
        let (display, height) = match target {
            TableTarget::Activities => return self.open_table_font(),
            TableTarget::Wbs => (&self.views.wbs.table, self.views.wbs.row_height),
            TableTarget::Projects => (&self.views.projects.table, self.views.projects.row_height),
        };
        self.layout_opts.drafts.table = Some(TableDraft {
            target,
            display: display.clone(),
            keep: true,
            optimize: false,
            limit: false,
            lines: "2".into(),
            height: format!("{}", height.round()),
            error: None,
        });
    }

    /// OK or Apply of Table Font and Row on the WBS or Projects window.
    fn commit_view_table_font(&mut self, d: &TableDraft) {
        let (table, height) = match d.target {
            TableTarget::Activities => return,
            TableTarget::Wbs => (&mut self.views.wbs.table, &mut self.views.wbs.row_height),
            TableTarget::Projects => (
                &mut self.views.projects.table,
                &mut self.views.projects.row_height,
            ),
        };
        table.font = d.display.font.clone();
        table.cell_color = d.display.cell_color;
        if !d.keep && !d.optimize {
            if let Ok(h) = d.height.trim().parse::<f32>() {
                *height = h.clamp(17., 48.).round();
            }
        }
    }

    /// The font picker for the text attachment being edited.
    pub(crate) fn open_text_font_picker(&mut self, f: TextFont) {
        self.layout_opts.drafts.picker = Some(FontPicker {
            target: PickerFor::Text,
            face: f.face,
            points: f.points,
            bold: f.bold,
            italic: f.italic,
            color: f.color,
        });
    }

    /// Is the font picker open for a text attachment?
    pub(crate) fn text_font_picker_open(&self) -> bool {
        self.layout_opts
            .drafts
            .picker
            .as_ref()
            .is_some_and(|p| p.target == PickerFor::Text)
    }

    pub(crate) fn open_chart_font(&mut self) {
        self.layout_opts.drafts.chart = Some(ChartDraft {
            options: self.gantt.options,
        });
    }

    pub(crate) fn open_collapse_to(&mut self) {
        let levels = self.outline_levels();
        if levels.is_empty() {
            self.status = "Collapse To needs grouping bands (Group and Sort).".into();
        }
        // The drop-down opens blank, as observed.
        self.layout_opts.drafts.collapse = Some(CollapseDraft {
            selected: None,
            levels,
        });
    }

    pub(crate) fn open_new_layout(&mut self) {
        self.layout_opts.drafts.new_layout = Some(NewLayoutDraft {
            name: "New Layout".into(),
            from_current: false,
        });
    }

    pub(crate) fn layout_dialog_open(&self) -> bool {
        let d = &self.layout_opts.drafts;
        d.table.is_some()
            || d.chart.is_some()
            || d.collapse.is_some()
            || d.new_layout.is_some()
            || d.picker.is_some()
    }

    /// Draw the open dialogs; Escape closes the topmost (each window's
    /// `closed` is true only for the layer on top).
    pub(crate) fn layout_dialogs(&mut self, ctx: &egui::Context) {
        self.table_font_dialog(ctx);
        self.chart_font_dialog(ctx);
        self.collapse_to_dialog(ctx);
        // Collapse To of the WBS or Projects window ends with its dialog.
        if self.layout_opts.drafts.collapse.is_none() {
            self.layout_opts.collapse_for = CollapseTarget::Activities;
        }
        self.new_layout_dialog(ctx);
        self.font_picker_dialog(ctx);
    }

    fn commit_table_font(&mut self, d: &TableDraft) {
        if d.target != TableTarget::Activities {
            return self.commit_view_table_font(d);
        }
        let t = &mut self.grid.layout.table;
        t.font = d.display.font.clone();
        t.cell_color = d.display.cell_color;
        t.show_icons = d.display.show_icons;
        if !d.keep {
            if d.optimize {
                let lines = if d.limit {
                    d.lines.trim().parse::<u8>().unwrap_or(2).clamp(1, 20)
                } else {
                    0
                };
                t.optimize_rows = Some(lines);
            } else {
                t.optimize_rows = None;
                if let Ok(h) = d.height.trim().parse::<f32>() {
                    self.view.row_height = h.clamp(17., 48.).round();
                }
            }
        }
        self.layout_opts.heights.borrow_mut().take();
        self.drop_frame_cache();
    }

    fn table_font_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.layout_opts.drafts.table.take() else {
            return;
        };
        let mut picked = None;
        // 431 x 331 px inside the frame.
        let (clicked, closed, actions) = kit::window(
            ctx,
            "table_font",
            "Table, Font and Row",
            Vec2::new(431., 331.),
            |c| {
                c.fill(0., 0., 431., 331., kit::STRIP);
                c.fill(5., 5., 309., 321., kit::FACE);
                c.lowered(5., 5., 309., 321.);
                kit::section(c, 11., 11., 297., 114., "Font and Color");
                kit::text(c, 27., 50., "Font", true);
                let f = &d.display.font;
                let font = if f.is_default() {
                    FontId::proportional(FONT_SIZE)
                } else {
                    f.face.font(f.points, f.bold)
                };
                if sample_button(
                    c,
                    "table_font_font",
                    (107., 42., 113., 26.),
                    font,
                    f.italic,
                    rgb(f.color).color(),
                ) {
                    picked = Some(());
                }
                kit::text(c, 27., 86., "Color", true);
                let mut cell = rgb(d.display.cell_color);
                kit::swatch(
                    c,
                    "table_font_color",
                    (106., 78., 114., 26.),
                    &mut cell,
                    true,
                );
                d.display.cell_color = bytes(cell);
                kit::section(c, 11., 130., 297., 162., "Row Height");
                c.checkbox(
                    "table_font_keep",
                    29.,
                    164.,
                    259.,
                    17.,
                    "Keep Current Row Heights",
                    &mut d.keep,
                    true,
                );
                let free = !d.keep;
                let activities = d.target == TableTarget::Activities;
                if c.radio(
                    "table_font_optimize",
                    47.,
                    189.,
                    249.,
                    17.,
                    "Optimize height by row content",
                    d.optimize,
                    free && activities,
                ) {
                    d.optimize = true;
                }
                let lines_on = free && d.optimize;
                c.checkbox(
                    "table_font_limit",
                    65.,
                    208.,
                    110.,
                    17.,
                    "Do not exceed",
                    &mut d.limit,
                    lines_on,
                );
                c.spin(
                    "table_font_lines",
                    177.,
                    206.,
                    48.,
                    23.,
                    &mut d.lines,
                    1.,
                    1.0..=20.,
                    lines_on && d.limit,
                );
                c.label(229., 206., 23., "lines per row", lines_on && d.limit);
                if c.radio(
                    "table_font_fixed",
                    47.,
                    236.,
                    264.,
                    17.,
                    "Select height for all rows",
                    !d.optimize,
                    free,
                ) {
                    d.optimize = false;
                }
                c.spin(
                    "table_font_height",
                    65.,
                    256.,
                    57.,
                    23.,
                    &mut d.height,
                    1.,
                    17.0..=48.,
                    free && !d.optimize,
                );
                if let Some(error) = &d.error {
                    kit::error_text(c, 127., 260., 178., error);
                }
                c.lowered(11., 297., 297., 23.);
                c.checkbox(
                    "table_font_icons",
                    28.,
                    301.,
                    217.,
                    14.,
                    "Show Icons",
                    &mut d.display.show_icons,
                    activities,
                );
                kit::button_column(
                    c,
                    431.,
                    &[
                        kit::Entry::ok("table_font_ok"),
                        kit::Entry::cancel("table_font_cancel"),
                        kit::Entry::apply("table_font_apply", true),
                        kit::Entry::Button("table_font_default", "Default", true, true),
                        kit::Entry::help("table_font_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        if picked.is_some() {
            let f = &d.display.font;
            self.layout_opts.drafts.picker = Some(FontPicker {
                target: PickerFor::Table,
                face: f.face,
                points: f.points,
                bold: f.bold,
                italic: f.italic,
                color: rgb(f.color),
            });
        }
        let mut close = closed;
        // OK and Apply refuse a bad row height or line limit and say why
        // beside the field.
        let commit = matches!(clicked, Some("table_font_ok" | "table_font_apply"));
        d.error = if commit { d.invalid() } else { d.error.take() };
        match clicked {
            _ if commit && d.error.is_some() => {}
            Some("table_font_ok") => {
                self.commit_table_font(&d);
                close = true;
            }
            Some("table_font_cancel") => close = true,
            Some("table_font_apply") => {
                self.commit_table_font(&d);
                d.keep = true;
            }
            Some("table_font_default") => {
                let defaults = TableDisplay::default();
                d.display.font = defaults.font;
                d.display.cell_color = defaults.cell_color;
                d.display.show_icons = defaults.show_icons;
                d.keep = false;
                d.optimize = false;
                d.height = format!("{}", gantt_geometry::ROW_HEIGHT.round());
            }
            Some("table_font_help") => self.open_help_for("table_font"),
            _ => {}
        }
        if close {
            if self
                .layout_opts
                .drafts
                .picker
                .as_ref()
                .is_some_and(|p| p.target == PickerFor::Table)
            {
                self.layout_opts.drafts.picker = None;
            }
        } else {
            self.layout_opts.drafts.table = Some(d);
        }
    }

    fn commit_chart_font(&mut self, d: &ChartDraft) {
        let o = &mut self.gantt.options;
        o.font = d.options.font;
        o.background = d.options.background;
        o.nonwork = d.options.nonwork;
        o.sight_lines = d.options.sight_lines;
    }

    fn chart_font_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.layout_opts.drafts.chart.take() else {
            return;
        };
        let mut picked = false;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "chart_font",
            "Chart Font and Colors",
            Vec2::new(431., 300.),
            |c| {
                c.fill(0., 0., 431., 300., kit::STRIP);
                c.fill(5., 5., 309., 290., kit::FACE);
                c.lowered(5., 5., 309., 290.);
                let o = &mut d.options;
                kit::section(c, 11., 11., 297., 104., "Font and Color");
                kit::text(c, 27., 50., "Font", true);
                picked = sample_button(
                    c,
                    "chart_font_font",
                    (107., 42., 113., 26.),
                    o.font.face.font(o.font.points, o.font.bold),
                    false,
                    o.font.color.color(),
                );
                kit::text(c, 27., 84., "Background", true);
                kit::swatch(
                    c,
                    "chart_font_background",
                    (106., 76., 114., 26.),
                    &mut o.background,
                    true,
                );
                kit::section(c, 11., 120., 297., 56., "Nonwork Time");
                c.checkbox(
                    "chart_font_shade",
                    27.,
                    149.,
                    150.,
                    17.,
                    "Shade nonwork time",
                    &mut o.nonwork.shade,
                    true,
                );
                kit::swatch(
                    c,
                    "chart_font_nonwork",
                    (196., 146., 57., 21.),
                    &mut o.nonwork.color,
                    true,
                );
                kit::section(c, 11., 181., 297., 108., "Sight Lines");
                let s = &mut o.sight_lines;
                for (n, (key, caption, color)) in [
                    (
                        "chart_font_h_major",
                        "Horizontal major",
                        &mut s.horizontal_major.color,
                    ),
                    (
                        "chart_font_h_minor",
                        "Horizontal minor",
                        &mut s.horizontal_minor.color,
                    ),
                    (
                        "chart_font_v_major",
                        "Vertical major",
                        &mut s.vertical_major.color,
                    ),
                    (
                        "chart_font_v_minor",
                        "Vertical minor",
                        &mut s.vertical_minor.color,
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let y = 208. + 20. * n as f32;
                    kit::text(c, 27., y + 3., caption, true);
                    kit::swatch(c, key, (196., y, 57., 18.), color, true);
                }
                kit::button_column(
                    c,
                    431.,
                    &[
                        kit::Entry::ok("chart_font_ok"),
                        kit::Entry::cancel("chart_font_cancel"),
                        kit::Entry::apply("chart_font_apply", true),
                        kit::Entry::Button("chart_font_default", "Default", true, true),
                        kit::Entry::help("chart_font_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        if picked {
            let f = d.options.font;
            self.layout_opts.drafts.picker = Some(FontPicker {
                target: PickerFor::Chart,
                face: f.face,
                points: f.points,
                bold: f.bold,
                italic: false,
                color: f.color,
            });
        }
        let mut close = closed;
        match clicked {
            Some("chart_font_ok") => {
                self.commit_chart_font(&d);
                close = true;
            }
            Some("chart_font_cancel") => close = true,
            Some("chart_font_apply") => self.commit_chart_font(&d),
            Some("chart_font_default") => {
                let defaults = ChartOptions::default();
                d.options.font = defaults.font;
                d.options.background = defaults.background;
                d.options.nonwork = defaults.nonwork;
                d.options.sight_lines = defaults.sight_lines;
            }
            Some("chart_font_help") => self.open_help_for("chart_font"),
            _ => {}
        }
        if close {
            if self
                .layout_opts
                .drafts
                .picker
                .as_ref()
                .is_some_and(|p| p.target == PickerFor::Chart)
            {
                self.layout_opts.drafts.picker = None;
            }
        } else {
            self.layout_opts.drafts.chart = Some(d);
        }
    }

    fn font_picker_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut p) = self.layout_opts.drafts.picker.take() else {
            return;
        };
        let styles = p.styles();
        let (clicked, closed, actions) =
            kit::window(ctx, "font_picker", "Font", Vec2::new(428., 262.), |c| {
                c.fill(0., 0., 428., 262., kit::STRIP);
                c.fill(5., 5., 306., 252., kit::FACE);
                c.lowered(5., 5., 306., 252.);
                // Face, style and size lists side by side.
                let faces: Vec<&str> = Face::ALL.iter().map(|f| f.name()).collect();
                let face_at = Face::ALL.iter().position(|f| *f == p.face);
                let out = kit::list(
                    c,
                    "font_picker_face",
                    (11., 11., 120., 128.),
                    None,
                    &[Col {
                        title: "Font",
                        width: 116.,
                    }],
                    faces.len(),
                    face_at,
                    None,
                    &mut |c, i, _, rect, sel| kit::cell_text(c, rect, faces[i], sel, 4.),
                );
                if let Some(i) = out.clicked {
                    p.face = Face::ALL[i];
                }
                let style_at = p.style_index();
                let out = kit::list(
                    c,
                    "font_picker_style",
                    (136., 11., 100., 128.),
                    None,
                    &[Col {
                        title: "Font style",
                        width: 96.,
                    }],
                    styles.len(),
                    Some(style_at),
                    None,
                    &mut |c, i, _, rect, sel| kit::cell_text(c, rect, styles[i], sel, 4.),
                );
                if let Some(i) = out.clicked {
                    p.set_style(i);
                }
                let size_at = SIZES.iter().position(|s| (*s - p.points).abs() < 0.01);
                let out = kit::list(
                    c,
                    "font_picker_size",
                    (241., 11., 64., 128.),
                    None,
                    &[Col {
                        title: "Size",
                        width: 60.,
                    }],
                    SIZES.len(),
                    size_at,
                    None,
                    &mut |c, i, _, rect, sel| {
                        kit::cell_text(c, rect, &format!("{}", SIZES[i]), sel, 4.)
                    },
                );
                if let Some(i) = out.clicked {
                    p.points = SIZES[i];
                }
                kit::text(c, 14., 154., "Color", true);
                kit::swatch(
                    c,
                    "font_picker_color",
                    (60., 150., 57., 21.),
                    &mut p.color,
                    true,
                );
                kit::section(c, 11., 180., 294., 71., "Sample");
                c.fill(18., 207., 280., 38., Color32::WHITE);
                let mut job = egui::text::LayoutJob::single_section(
                    "AaBbYyZz".into(),
                    egui::TextFormat {
                        font_id: p.face.font(p.points, p.bold),
                        color: p.color.color(),
                        italics: p.italic,
                        ..Default::default()
                    },
                );
                job.wrap.max_width = f32::INFINITY;
                let galley = c.ui.painter().layout_job(job);
                let area = c.r(18., 207., 280., 38.);
                c.ui.painter().with_clip_rect(area).galley(
                    (area.center() - galley.size() / 2.).round(),
                    galley,
                    p.color.color(),
                );
                kit::button_column(
                    c,
                    428.,
                    &[
                        kit::Entry::ok("font_picker_ok"),
                        kit::Entry::cancel("font_picker_cancel"),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        match clicked {
            Some("font_picker_ok") => {
                let d = &mut self.layout_opts.drafts;
                match p.target {
                    PickerFor::Table => {
                        if let Some(t) = d.table.as_mut() {
                            t.display.font = TableFont {
                                face: p.face,
                                points: p.points,
                                bold: p.bold,
                                italic: p.italic,
                                color: bytes(p.color),
                            };
                        }
                    }
                    PickerFor::Text => {
                        if let Some(t) = self.gantt_ui.text.as_mut() {
                            t.font = TextFont {
                                face: p.face,
                                points: p.points,
                                bold: p.bold,
                                italic: p.italic,
                                color: p.color,
                            };
                        }
                    }
                    PickerFor::Chart => {
                        if let Some(t) = d.chart.as_mut() {
                            t.options.font = ChartFont {
                                face: p.face,
                                points: p.points,
                                bold: p.bold,
                                color: p.color,
                            };
                        }
                    }
                }
            }
            Some(_) => {}
            None if closed => {}
            None => self.layout_opts.drafts.picker = Some(p),
        }
    }

    fn collapse_to_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.layout_opts.drafts.collapse.take() else {
            return;
        };
        // As the reference draws it: a caption over one drop-down of the
        // band levels, blank until a level is picked, and OK, Cancel, Apply,
        // Help. OK with no level picked closes without a change.
        const W: f32 = 424.;
        const H: f32 = 150.;
        let (clicked, closed, actions) =
            kit::window(ctx, "collapse_to", "Collapse To", Vec2::new(W, H), |c| {
                c.fill(0., 0., W, H, kit::STRIP);
                c.fill(5., 5., 302., H - 10., kit::FACE);
                c.lowered(5., 5., 302., H - 10.);
                kit::text(c, 12., 16., "Band level to collapse to", true);
                let items: Vec<(&str, bool)> =
                    d.levels.iter().map(|l| (l.as_str(), true)).collect();
                let mut pick = d.selected.unwrap_or(usize::MAX);
                let level = "collapse_to_level";
                if c.combo(level, 12., 36., 216., 20., &items, &mut pick, true) {
                    d.selected = Some(pick);
                }
                kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::ok("collapse_to_ok"),
                        kit::Entry::cancel("collapse_to_cancel"),
                        kit::Entry::apply("collapse_to_apply", d.selected.is_some()),
                        kit::Entry::help("collapse_to_help"),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        match clicked {
            Some("collapse_to_ok") => {
                if let Some(i) = d.selected {
                    self.collapse_to(i + 1);
                    self.status = format!("Collapsed to level {}", i + 1);
                }
                return;
            }
            Some("collapse_to_apply") => {
                if let Some(i) = d.selected {
                    self.collapse_to(i + 1);
                    self.status = format!("Collapsed to level {}", i + 1);
                }
            }
            Some("collapse_to_cancel") => return,
            Some("collapse_to_help") => self.open_help_for("collapse_to"),
            _ => {}
        }
        if !closed {
            self.layout_opts.drafts.collapse = Some(d);
        }
    }

    fn new_layout_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.layout_opts.drafts.new_layout.take() else {
            return;
        };
        let (clicked, closed, actions) = kit::window(
            ctx,
            "new_layout",
            "New Layout",
            Vec2::new(424., 140.),
            |c| {
                c.fill(0., 0., 424., 140., kit::STRIP);
                c.fill(5., 5., 302., 130., kit::FACE);
                c.lowered(5., 5., 302., 130.);
                kit::text(c, 14., 18., "Layout Name", true);
                c.edit(
                    "new_layout_name",
                    14.,
                    34.,
                    282.,
                    21.,
                    &mut d.name,
                    true,
                    Align::Min,
                );
                if c.radio(
                    "new_layout_default",
                    14.,
                    72.,
                    280.,
                    17.,
                    "Start from the default layout",
                    !d.from_current,
                    true,
                ) {
                    d.from_current = false;
                }
                if c.radio(
                    "new_layout_current",
                    14.,
                    97.,
                    280.,
                    17.,
                    "Start from the current layout",
                    d.from_current,
                    true,
                ) {
                    d.from_current = true;
                }
                kit::button_column(
                    c,
                    424.,
                    &[
                        kit::Entry::ok("new_layout_ok"),
                        kit::Entry::cancel("new_layout_cancel"),
                        kit::Entry::help("new_layout_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        match clicked {
            Some("new_layout_ok") => {
                if let Err(e) = self.new_layout(&d.name, d.from_current) {
                    self.error(e);
                    self.layout_opts.drafts.new_layout = Some(d);
                }
                return;
            }
            Some("new_layout_cancel") => return,
            Some("new_layout_help") => self.open_help_for("new_layout"),
            _ => {}
        }
        if !closed {
            self.layout_opts.drafts.new_layout = Some(d);
        }
    }
}

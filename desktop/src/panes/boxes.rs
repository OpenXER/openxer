//! Activity boxes shared by Trace Logic and the Activity Network.
//!
//! Geometry: a 160 x 78 box with a 1 px black outline, a 1 px light bevel at the top and left, a
//! 1 px dark bevel at the right and bottom, a 3 px `#737373` drop shadow, a
//! header row 11 px tall over a divider, and a left cell 78 px wide that
//! ends 41 px below the box top. Everything scales with the zoom factor.
use super::*;
use crate::grid::marks::{mark, End};
use crate::theme;

pub const BOX_W: f32 = 160.;
pub const BOX_H: f32 = 78.;
/// Header row height above its divider.
pub const HEADER: f32 = 11.;
/// Left cell: right divider and bottom divider offsets from the box origin.
pub const CELL_RIGHT: f32 = 79.;
pub const CELL_BOTTOM: f32 = 41.;
pub const SHADOW: Color32 = Color32::from_rgb(0x73, 0x73, 0x73);
pub const SHADOW_W: f32 = 3.;
/// Selection frame: 1 px black, 2 px blue, outside the box outline.
pub const SELECT_BLUE: Color32 = Color32::from_rgb(0, 0x78, 0xD7);
/// Normal box: the theme's light blue with navy text and a steel bevel.
pub const NORMAL: BoxColors = BoxColors {
    fill: theme::color(theme::LIGHT_BLUE),
    light: theme::color(0xF2F7FB),
    dark: theme::color(0x7F94AB),
    text: theme::color(theme::NAVY),
};
/// Critical box: the theme's critical red with white text.
pub const CRITICAL_BOX: BoxColors = BoxColors {
    fill: theme::color(theme::CRITICAL),
    light: theme::color(theme::CRITICAL_LIGHT),
    dark: theme::color(theme::CRITICAL_DARK),
    text: Color32::WHITE,
};
/// Which edge of a box a press at `pos` grips: the strip along the left
/// edge is the start side and the strip along the right edge the finish
/// side. Dragging from a strip to another box ties the two activities;
/// dragging from anywhere else on the box moves it.
pub fn edge_at(r: Rect, pos: Pos2, scale: f32) -> Option<Edge> {
    let strip = (BOX_W * scale * 0.1).clamp(6., 16.);
    if pos.y < r.top() || pos.y > r.bottom() {
        return None;
    }
    if pos.x >= r.right() - strip && pos.x <= r.right() + 3. {
        Some(Edge::Finish)
    } else if pos.x <= r.left() + strip && pos.x >= r.left() - 3. {
        Some(Edge::Start)
    } else {
        None
    }
}

/// Relationship arrowhead: 10 px long, 11 px across.
pub const ARROW_LEN: f32 = 10.;
pub const ARROW_HALF: f32 = 5.5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BoxColors {
    pub fill: Color32,
    pub light: Color32,
    pub dark: Color32,
    pub text: Color32,
}

impl BoxColors {
    /// Box colors for a chosen fill: the measured sets for the two default
    /// fills, otherwise bevels half way to white and at half intensity and
    /// black or white text by brightness.
    pub fn for_fill(fill: Color32) -> BoxColors {
        if fill == NORMAL.fill {
            return NORMAL;
        }
        if fill == CRITICAL_BOX.fill {
            return CRITICAL_BOX;
        }
        let mix = |c: u8, to: u16| ((c as u16 + to) / 2) as u8;
        let light = Color32::from_rgb(mix(fill.r(), 255), mix(fill.g(), 255), mix(fill.b(), 255));
        let dark = Color32::from_rgb(fill.r() / 2, fill.g() / 2, fill.b() / 2);
        let brightness = 299 * fill.r() as u32 + 587 * fill.g() as u32 + 114 * fill.b() as u32;
        let text = if brightness >= 128_000 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        BoxColors {
            fill,
            light,
            dark,
            text,
        }
    }
}

/// Text size of the box fields at zoom 1 (Font & Colors can change it).
pub const FONT: f32 = 8.5;

/// Which fields a box shows. `Standard`:
/// ID in the header, start and original duration in the left cell, finish
/// and total float to its right, the name below.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Template {
    Standard,
    /// ID and name only.
    IdAndName,
    /// ID, dates and name, without durations.
    Dates,
}

impl Template {
    pub const ALL: [Template; 3] = [Template::Standard, Template::Dates, Template::IdAndName];
    pub fn key(self) -> &'static str {
        match self {
            Template::Standard => "standard",
            Template::IdAndName => "id_name",
            Template::Dates => "dates",
        }
    }
    pub fn caption(self) -> &'static str {
        match self {
            Template::Standard => "Standard",
            Template::IdAndName => "Activity ID and Name",
            Template::Dates => "Dates",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }
}

/// The texts one box shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxText {
    pub id: String,
    pub name: String,
    pub start: String,
    pub finish: String,
    pub duration: String,
    pub float: String,
}

/// Progress marks drawn over a box when Show progress is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Progress {
    None,
    Started,
    Completed,
}

impl App {
    /// The box texts of activity `i` in the user's formats.
    pub(crate) fn box_text(&self, i: usize) -> BoxText {
        let m = &self.project.model;
        let a = &m.activities[i];
        let periods = format::activity_periods(&self.project, i);
        let od = self
            .project
            .details
            .get(&a.task_id)
            .map_or(a.rem, |d| d.original_duration);
        let start = displayed_start(a, m.options.do_level);
        let finish = displayed_finish(a, m.options.do_level);
        BoxText {
            id: a.task_code.clone(),
            name: self.project.name(i).to_string(),
            start: format::date_marked(m.origin_serial, start, mark(a, End::Start)),
            finish: format::date_marked(m.origin_serial, finish, mark(a, End::Finish)),
            duration: format::duration(od, periods),
            float: format::float(a.total_float, periods),
        }
    }

    /// Critical by the project's definition (the Gantt's rule).
    pub(crate) fn box_critical(&self, i: usize) -> bool {
        grid::columns::is_critical(
            &self.project.model.activities[i],
            self.critical_definition(),
        )
    }

    pub(crate) fn box_progress(&self, i: usize) -> Progress {
        let a = &self.project.model.activities[i];
        if a.is_completed() {
            Progress::Completed
        } else if a.act_start.is_some() {
            Progress::Started
        } else {
            Progress::None
        }
    }
}

/// Paint one box with its top-left outline pixel at `origin`.
#[allow(clippy::too_many_arguments)]
pub fn paint_box(
    p: &egui::Painter,
    origin: Pos2,
    scale: f32,
    text: &BoxText,
    colors: BoxColors,
    template: Template,
    selected: bool,
    progress: Progress,
) -> Rect {
    paint_box_font(
        p, origin, scale, text, colors, template, selected, progress, FONT,
    )
}

/// `paint_box` with the box text `font` points tall at zoom 1.
#[allow(clippy::too_many_arguments)]
pub fn paint_box_font(
    p: &egui::Painter,
    origin: Pos2,
    scale: f32,
    text: &BoxText,
    colors: BoxColors,
    template: Template,
    selected: bool,
    progress: Progress,
    font: f32,
) -> Rect {
    let s = scale;
    let o = Pos2::new(origin.x.round(), origin.y.round());
    let w = (BOX_W * s).round();
    let h = (BOX_H * s).round();
    let outer = Rect::from_min_size(o, Vec2::new(w, h));
    let px = |x: f32, y: f32, ww: f32, hh: f32, c: Color32| {
        chrome::fill(
            p,
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(ww, hh)),
            c,
        );
    };
    if selected {
        px(o.x - 3., o.y - 3., w + 6., h + 6., Color32::BLACK);
        px(o.x - 2., o.y - 2., w + 4., h + 4., SELECT_BLUE);
    } else {
        let sh = (SHADOW_W * s).max(1.).round();
        px(o.x + w, o.y + sh, sh, h, SHADOW);
        px(o.x + sh, o.y + h, w, sh, SHADOW);
    }
    px(o.x, o.y, w, h, Color32::BLACK);
    px(o.x + 1., o.y + 1., w - 2., h - 2., colors.light);
    px(o.x + 2., o.y + 2., w - 3., h - 3., colors.dark);
    px(o.x + 2., o.y + 2., w - 4., h - 4., colors.fill);
    let header = (HEADER * s).round();
    // Header divider and the left cell's right and bottom dividers.
    px(o.x + 1., o.y + header, w - 3., 1., colors.dark);
    let cell_right = (CELL_RIGHT * s).round();
    let cell_bottom = (CELL_BOTTOM * s).round();
    let cells = template != Template::IdAndName;
    if cells {
        px(
            o.x + cell_right,
            o.y + header + 1.,
            1.,
            cell_bottom - header - 1.,
            colors.dark,
        );
        px(
            o.x + 1.,
            o.y + cell_bottom,
            cell_right - 1.,
            1.,
            colors.dark,
        );
    }
    let size = (font * s).max(4.);
    let font = FontId::proportional(size);
    let clip = |r: Rect| p.with_clip_rect(r.intersect(p.clip_rect()));
    if size >= 5. {
        let line = (size * 1.2).round();
        let id_cell = Rect::from_min_max(
            Pos2::new(o.x + 2., o.y + 1.),
            Pos2::new(o.x + w - 2., o.y + header),
        );
        clip(id_cell).text(
            Pos2::new(o.x + 3. * s, o.y + header / 2.),
            Align2::LEFT_CENTER,
            &text.id,
            font.clone(),
            colors.text,
        );
        let mut name_top = o.y + header + 3. * s;
        if cells {
            let left = Rect::from_min_max(
                Pos2::new(o.x + 2., o.y + header + 1.),
                Pos2::new(o.x + cell_right, o.y + cell_bottom),
            );
            let right = Rect::from_min_max(
                Pos2::new(o.x + cell_right + 1., o.y + header + 1.),
                Pos2::new(o.x + w - 2., o.y + cell_bottom),
            );
            let mut lines = [(left, &text.start), (right, &text.finish)].to_vec();
            if template == Template::Standard {
                lines.push((left.translate(Vec2::new(0., line)), &text.duration));
                lines.push((right.translate(Vec2::new(0., line)), &text.float));
            }
            for (cell, value) in lines {
                let bounds = if cell.left() < right.left() {
                    left
                } else {
                    right
                };
                clip(bounds).text(
                    Pos2::new(cell.left() + 2. * s, cell.top() + 2. * s),
                    Align2::LEFT_TOP,
                    value,
                    font.clone(),
                    colors.text,
                );
            }
            name_top = o.y + cell_bottom + 2. * s;
        }
        let name_rect = Rect::from_min_max(
            Pos2::new(o.x + 2., name_top),
            Pos2::new(o.x + w - 3., o.y + h - 2.),
        );
        let galley = p.layout(
            text.name.clone(),
            font,
            colors.text,
            name_rect.width() - 2. * s,
        );
        clip(name_rect).galley(
            Pos2::new(name_rect.left() + 1. * s, name_rect.top()),
            galley,
            colors.text,
        );
    }
    let ink = Stroke::new(1_f32, Color32::BLACK);
    match progress {
        Progress::None => {}
        Progress::Started => {
            p.line_segment([outer.left_bottom(), outer.right_top()], ink);
        }
        Progress::Completed => {
            p.line_segment([outer.left_bottom(), outer.right_top()], ink);
            p.line_segment([outer.left_top(), outer.right_bottom()], ink);
        }
    }
    outer
}

/// A filled arrowhead with its tip at `tip`, pointing along `dir`.
pub fn arrow(p: &egui::Painter, tip: Pos2, dir: Vec2, color: Color32, scale: f32) {
    let d = dir.normalized();
    let n = Vec2::new(-d.y, d.x);
    let len = ARROW_LEN * scale.max(0.5);
    let half = ARROW_HALF * scale.max(0.5);
    let base = tip - d * len;
    p.add(egui::Shape::convex_polygon(
        vec![tip, base + n * half, base - n * half],
        color,
        Stroke::NONE,
    ));
}

/// A relationship polyline on pixel centers, with its arrowhead at the end.
pub fn relationship(p: &egui::Painter, points: &[Pos2], dir: Vec2, color: Color32, scale: f32) {
    let line: Vec<Pos2> = points.iter().map(|q| *q + Vec2::splat(0.5)).collect();
    p.add(egui::Shape::line(line, Stroke::new(1_f32, color)));
    let tip = *points.last().unwrap();
    if (scale - 1.).abs() < 0.01 {
        arrow_pixels(p, tip, dir, color);
        return;
    }
    let shift = if dir.x.abs() > dir.y.abs() {
        Vec2::new(0., 0.5)
    } else {
        Vec2::new(0.5, 0.)
    };
    arrow(p, tip + shift, dir, color, scale);
}

/// The arrowhead at 100% as whole pixels: 11 runs anchored at the base,
/// 10 px for the middle run and two fewer for each step out. `tip` is the
/// first pixel past the point (the box outline the line runs into).
pub fn arrow_pixels(p: &egui::Painter, tip: Pos2, dir: Vec2, color: Color32) {
    let len = ARROW_LEN;
    for d in -5i32..=5 {
        let w = (11 - 2 * d.abs()).min(10) as f32;
        let d = d as f32;
        let r = if dir.x > 0.5 {
            Rect::from_min_size(Pos2::new(tip.x - len, tip.y + d), Vec2::new(w, 1.))
        } else if dir.x < -0.5 {
            Rect::from_min_size(Pos2::new(tip.x + len - w + 1., tip.y + d), Vec2::new(w, 1.))
        } else if dir.y < -0.5 {
            Rect::from_min_size(Pos2::new(tip.x + d, tip.y + len - w), Vec2::new(1., w))
        } else {
            Rect::from_min_size(Pos2::new(tip.x + d, tip.y - len), Vec2::new(1., w))
        };
        chrome::fill(p, r, color);
    }
}

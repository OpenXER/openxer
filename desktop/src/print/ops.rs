//! The print display list: filled shapes, strokes and text runs with their
//! clip rectangles, in page pixels (96 per inch). It is built from the
//! shapes the production painters emit, so the printed table and Gantt are
//! drawn by the same code as the screen. The Print Preview paints it back
//! with egui; the PDF writer turns it into vector operators.
use eframe::egui::{
    self,
    epaint::{ClippedShape, Shape, TextShape},
    Color32, FontFamily, Pos2, Rect, Vec2,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// An axis-aligned filled rectangle.
    Fill { rect: Rect, color: Color32 },
    /// A filled polygon (convex or triangle).
    Polygon { points: Vec<Pos2>, color: Color32 },
    /// A stroked polyline.
    Stroke {
        points: Vec<Pos2>,
        closed: bool,
        width: f32,
        color: Color32,
    },
    /// A dashed line of whole-pixel dashes: `from` and `to` are the ends
    /// of its center line, dashes `on` long with `off` gaps.
    Dash {
        from: Pos2,
        to: Pos2,
        width: f32,
        on: f32,
        off: f32,
        color: Color32,
    },
    /// One run of text on a baseline.
    Text {
        /// Left end of the baseline.
        pos: Pos2,
        text: String,
        /// Font size in pixels (em height).
        size: f32,
        bold: bool,
        color: Color32,
        /// Advance width measured by the screen font.
        width: f32,
    },
}

impl Op {
    pub fn bounds(&self) -> Rect {
        match self {
            Op::Fill { rect, .. } => *rect,
            Op::Polygon { points, .. } | Op::Stroke { points, .. } => {
                Rect::from_points(points).expand(1.)
            }
            Op::Dash {
                from, to, width, ..
            } => Rect::from_two_pos(*from, *to).expand(width / 2. + 0.01),
            Op::Text {
                pos, size, width, ..
            } => Rect::from_min_max(
                Pos2::new(pos.x, pos.y - size),
                Pos2::new(pos.x + width, pos.y + size * 0.25),
            ),
        }
    }
    /// Scales by `s` about the origin, then moves by `offset`.
    pub fn transformed(&self, s: f32, offset: Vec2) -> Op {
        let p = |q: Pos2| Pos2::new(q.x * s, q.y * s) + offset;
        match self {
            Op::Fill { rect, color } => Op::Fill {
                rect: Rect::from_min_max(p(rect.min), p(rect.max)),
                color: *color,
            },
            Op::Polygon { points, color } => Op::Polygon {
                points: points.iter().map(|q| p(*q)).collect(),
                color: *color,
            },
            Op::Stroke {
                points,
                closed,
                width,
                color,
            } => Op::Stroke {
                points: points.iter().map(|q| p(*q)).collect(),
                closed: *closed,
                width: width * s,
                color: *color,
            },
            Op::Dash {
                from,
                to,
                width,
                on,
                off,
                color,
            } => Op::Dash {
                from: p(*from),
                to: p(*to),
                width: width * s,
                on: on * s,
                off: off * s,
                color: *color,
            },
            Op::Text {
                pos,
                text,
                size,
                bold,
                color,
                width,
            } => Op::Text {
                pos: p(*pos),
                text: text.clone(),
                size: size * s,
                bold: *bold,
                color: *color,
                width: width * s,
            },
        }
    }
}

/// An op and the rectangle it is clipped to.
#[derive(Clone, Debug, PartialEq)]
pub struct Clipped {
    pub clip: Rect,
    pub op: Op,
}

/// Ops of one printed page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    /// Page size in pixels at 96 per inch.
    pub size: Vec2,
    pub ops: Vec<Clipped>,
}

impl Page {
    /// Every text run, in paint order.
    #[cfg(test)]
    pub fn texts(&self) -> Vec<&str> {
        self.ops
            .iter()
            .filter_map(|c| match &c.op {
                Op::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
}

/// Copies the ops visible in `source` (canvas pixels) to a page: scaled by
/// `s`, with `source.min` landing on `dest`, clipped to the source.
pub fn place(ops: &[Clipped], source: Rect, s: f32, dest: Pos2, out: &mut Vec<Clipped>) {
    for c in ops {
        place_one(c, source, s, dest, out);
    }
}

fn place_one(c: &Clipped, source: Rect, s: f32, dest: Pos2, out: &mut Vec<Clipped>) {
    let offset = dest.to_vec2() - source.min.to_vec2() * s;
    let clip = c.clip.intersect(source);
    if !clip.is_positive() || !clip.intersects(c.op.bounds()) {
        return;
    }
    out.push(Clipped {
        clip: Rect::from_min_max(
            Pos2::new(clip.min.x * s, clip.min.y * s) + offset,
            Pos2::new(clip.max.x * s, clip.max.y * s) + offset,
        ),
        op: c.op.transformed(s, offset),
    });
}

/// Height of a band of the canvas in `Indexed`.
const BAND: f32 = 512.;

/// The canvas ops with, for each horizontal band of the canvas, the ops that
/// can show in it, so a page picks its ops without testing all of them.
#[derive(Debug, Default)]
pub struct Indexed {
    pub ops: Vec<Clipped>,
    bands: Vec<Vec<u32>>,
}

impl Indexed {
    pub fn new(ops: Vec<Clipped>) -> Self {
        let mut bands: Vec<Vec<u32>> = vec![];
        for (n, c) in ops.iter().enumerate() {
            // A page shows an op only where its clip and its bounds overlap.
            let shown = c.clip.intersect(c.op.bounds());
            let (lo, hi) = (shown.min.y - 1., shown.max.y + 1.);
            if lo.is_nan() || hi.is_nan() || lo > hi {
                continue;
            }
            let (first, last) = (Self::band(lo), Self::band(hi));
            if bands.len() <= last {
                bands.resize(last + 1, vec![]);
            }
            for band in &mut bands[first..=last] {
                band.push(n as u32);
            }
        }
        Self { ops, bands }
    }

    fn band(y: f32) -> usize {
        (y.max(0.) / BAND) as usize
    }

    /// `place` over the ops of the bands `source` touches, in paint order.
    pub fn place(&self, source: Rect, s: f32, dest: Pos2, out: &mut Vec<Clipped>) {
        if self.bands.is_empty() {
            return;
        }
        let first = Self::band(source.min.y - 1.);
        let last = Self::band(source.max.y + 1.).min(self.bands.len() - 1);
        if first > last {
            return;
        }
        let mut found: Vec<u32> = self.bands[first..=last].concat();
        found.sort_unstable();
        found.dedup();
        for n in found {
            place_one(&self.ops[n as usize], source, s, dest, out);
        }
    }
}

fn is_bold(family: &FontFamily) -> bool {
    matches!(family, FontFamily::Name(name) if name.to_ascii_lowercase().contains("bold"))
}

/// Where converted ops go: a list, or the merge of thin fills.
pub trait Sink {
    fn push(&mut self, c: Clipped);
}

impl Sink for Vec<Clipped> {
    fn push(&mut self, c: Clipped) {
        Vec::push(self, c);
    }
}

fn text_ops(text: &TextShape, clip: Rect, out: &mut impl Sink) {
    let galley = &text.galley;
    let job = &galley.job;
    if job.sections.is_empty() {
        return;
    }
    // Sections by char offset into the job text.
    let starts: Vec<(usize, usize)> = job
        .sections
        .iter()
        .enumerate()
        .map(|(i, s)| (job.text[..s.byte_range.start].chars().count(), i))
        .collect();
    let section_at = |char_index: usize| -> usize {
        starts
            .iter()
            .rev()
            .find(|(start, _)| *start <= char_index)
            .map_or(0, |(_, i)| *i)
    };
    let mut char_index = 0;
    for placed in &galley.rows {
        let row = &placed.row;
        let mut run: Option<(usize, Pos2, String, f32)> = None;
        let mut flush = |run: &mut Option<(usize, Pos2, String, f32)>| {
            if let Some((section, pos, string, width)) = run.take() {
                if string.trim().is_empty() {
                    return;
                }
                let format = &job.sections[section].format;
                let mut color = text.override_text_color.unwrap_or(format.color);
                if color == Color32::PLACEHOLDER {
                    color = text.fallback_color;
                }
                out.push(Clipped {
                    clip,
                    op: Op::Text {
                        pos,
                        text: string,
                        size: format.font_id.size,
                        bold: is_bold(&format.font_id.family),
                        color: color.gamma_multiply(text.opacity_factor),
                        width,
                    },
                });
            }
        };
        for glyph in &row.glyphs {
            let section = section_at(char_index);
            char_index += 1;
            let at = text.pos + placed.pos.to_vec2() + glyph.pos.to_vec2();
            match &mut run {
                Some((s, start, string, width))
                    if *s == section && (start.y - at.y).abs() < 0.01 =>
                {
                    string.push(glyph.chr);
                    *width = at.x + glyph.advance_width - start.x;
                }
                _ => {
                    flush(&mut run);
                    run = Some((section, at, glyph.chr.to_string(), glyph.advance_width));
                }
            }
        }
        flush(&mut run);
        if row.ends_with_newline {
            char_index += 1;
        }
    }
}

fn mesh_ops(mesh: &egui::Mesh, clip: Rect, out: &mut impl Sink) {
    // Image textures (icons) are not printed; flat colors use the font
    // atlas's white texel.
    if mesh.texture_id != egui::TextureId::default() {
        return;
    }
    let v = &mesh.vertices;
    let idx = &mesh.indices;
    let mut k = 0;
    while k + 3 <= idx.len() {
        // `add_colored_rect` writes [a, a+1, a+2, a+2, a+1, a+3].
        if k + 6 <= idx.len() {
            let a = idx[k];
            if idx[k + 1] == a + 1
                && idx[k + 2] == a + 2
                && idx[k + 3] == a + 2
                && idx[k + 4] == a + 1
                && idx[k + 5] == a + 3
            {
                let q = [0, 1, 2, 3].map(|i| v[(a + i) as usize]);
                let aligned = q[0].pos.y == q[1].pos.y
                    && q[2].pos.y == q[3].pos.y
                    && q[0].pos.x == q[2].pos.x
                    && q[1].pos.x == q[3].pos.x;
                if aligned && q.iter().all(|x| x.color == q[0].color) {
                    if q[0].color.a() > 0 {
                        out.push(Clipped {
                            clip,
                            op: Op::Fill {
                                rect: Rect::from_two_pos(q[0].pos, q[3].pos),
                                color: q[0].color,
                            },
                        });
                    }
                    k += 6;
                    continue;
                }
            }
        }
        let t = [idx[k], idx[k + 1], idx[k + 2]].map(|i| v[i as usize]);
        if t[0].color.a() > 0 {
            out.push(Clipped {
                clip,
                op: Op::Polygon {
                    points: t.iter().map(|x| x.pos).collect(),
                    color: t[0].color,
                },
            });
        }
        k += 3;
    }
}

fn ellipse(center: Pos2, radius: Vec2) -> Vec<Pos2> {
    (0..32)
        .map(|i| {
            let a = i as f32 / 32. * std::f32::consts::TAU;
            center + Vec2::new(a.cos() * radius.x, a.sin() * radius.y)
        })
        .collect()
}

fn filled(points: Vec<Pos2>, fill: Color32, stroke: egui::Stroke, clip: Rect, out: &mut impl Sink) {
    if fill.a() > 0 && points.len() > 2 {
        out.push(Clipped {
            clip,
            op: Op::Polygon {
                points: points.clone(),
                color: fill,
            },
        });
    }
    if stroke.width > 0. && stroke.color.a() > 0 {
        out.push(Clipped {
            clip,
            op: Op::Stroke {
                points,
                closed: true,
                width: stroke.width,
                color: stroke.color,
            },
        });
    }
}

fn shape_ops(shape: &Shape, clip: Rect, out: &mut impl Sink) {
    match shape {
        Shape::Noop => {}
        Shape::Callback(callback) => {
            // A dashed line kept whole by the capture (see `DashRun`).
            if let Some(run) = callback
                .callback
                .downcast_ref::<crate::gantt::paint::DashRun>()
            {
                if run.color.a() > 0 {
                    run.rects(|rect| {
                        out.push(Clipped {
                            clip,
                            op: Op::Fill {
                                rect,
                                color: run.color,
                            },
                        })
                    });
                }
            }
        }
        Shape::Vec(shapes) => {
            for s in shapes {
                shape_ops(s, clip, out);
            }
        }
        Shape::Mesh(mesh) => mesh_ops(mesh, clip, out),
        Shape::Text(text) => text_ops(text, clip, out),
        Shape::LineSegment { points, stroke } => {
            if stroke.width > 0. && stroke.color.a() > 0 {
                out.push(Clipped {
                    clip,
                    op: Op::Stroke {
                        points: points.to_vec(),
                        closed: false,
                        width: stroke.width,
                        color: stroke.color,
                    },
                });
            }
        }
        Shape::Path(path) => {
            let color = solid(path.stroke.color.clone());
            let fill = path.fill;
            if path.closed {
                filled(
                    path.points.clone(),
                    fill,
                    egui::Stroke::new(path.stroke.width, color),
                    clip,
                    out,
                );
            } else if path.stroke.width > 0. {
                out.push(Clipped {
                    clip,
                    op: Op::Stroke {
                        points: path.points.clone(),
                        closed: false,
                        width: path.stroke.width,
                        color,
                    },
                });
            }
        }
        Shape::Rect(r) => {
            let rect = r.rect;
            if r.fill.a() > 0 {
                out.push(Clipped {
                    clip,
                    op: Op::Fill {
                        rect,
                        color: r.fill,
                    },
                });
            }
            if r.stroke.width > 0. && r.stroke.color.a() > 0 {
                let w = r.stroke.width;
                let line = match r.stroke_kind {
                    egui::StrokeKind::Inside => rect.shrink(w / 2.),
                    egui::StrokeKind::Middle => rect,
                    egui::StrokeKind::Outside => rect.expand(w / 2.),
                };
                out.push(Clipped {
                    clip,
                    op: Op::Stroke {
                        points: vec![
                            line.left_top(),
                            line.right_top(),
                            line.right_bottom(),
                            line.left_bottom(),
                        ],
                        closed: true,
                        width: w,
                        color: r.stroke.color,
                    },
                });
            }
        }
        Shape::Circle(c) => filled(
            ellipse(c.center, Vec2::splat(c.radius)),
            c.fill,
            c.stroke,
            clip,
            out,
        ),
        Shape::Ellipse(e) => filled(ellipse(e.center, e.radius), e.fill, e.stroke, clip, out),
        Shape::QuadraticBezier(b) => {
            let points = b.flatten(Some(0.5));
            if b.closed {
                filled(points, b.fill, solid_stroke(&b.stroke), clip, out);
            } else {
                out.push(Clipped {
                    clip,
                    op: Op::Stroke {
                        points,
                        closed: false,
                        width: b.stroke.width,
                        color: solid(b.stroke.color.clone()),
                    },
                });
            }
        }
        Shape::CubicBezier(b) => {
            let points = b.flatten(Some(0.5));
            if b.closed {
                filled(points, b.fill, solid_stroke(&b.stroke), clip, out);
            } else {
                out.push(Clipped {
                    clip,
                    op: Op::Stroke {
                        points,
                        closed: false,
                        width: b.stroke.width,
                        color: solid(b.stroke.color.clone()),
                    },
                });
            }
        }
    }
}

fn solid(color: egui::epaint::ColorMode) -> Color32 {
    match color {
        egui::epaint::ColorMode::Solid(c) => c,
        egui::epaint::ColorMode::UV(_) => Color32::BLACK,
    }
}

fn solid_stroke(stroke: &egui::epaint::PathStroke) -> egui::Stroke {
    egui::Stroke::new(stroke.width, solid(stroke.color.clone()))
}

/// Converts painted egui shapes to print ops, merging thin fills as
/// `coalesce` does. The merge runs while the shapes are converted, so the
/// unmerged ops of a big printout (millions of dash pieces) never exist at
/// once.
pub fn from_shapes_coalesced(shapes: &[ClippedShape]) -> Vec<Clipped> {
    let mut merge = Coalescer::default();
    for s in shapes {
        shape_ops(&s.shape, s.clip_rect, &mut merge);
    }
    merge.finish()
}

/// Converts painted egui shapes to print ops.
#[cfg(test)]
pub fn from_shapes(shapes: &[ClippedShape]) -> Vec<Clipped> {
    let mut out = vec![];
    for s in shapes {
        shape_ops(&s.shape, s.clip_rect, &mut out);
    }
    out
}

/// A run of thin fills being merged.
struct Run {
    clip: Rect,
    color: Color32,
    vertical: bool,
    /// First rectangle and the latest one.
    first: Rect,
    last: Rect,
    /// Distance between dash starts (0 while only one rectangle is held).
    period: f32,
    count: usize,
}

impl Run {
    fn length(&self) -> f32 {
        if self.vertical {
            self.first.height()
        } else {
            self.first.width()
        }
    }
    fn start(r: &Rect, vertical: bool) -> f32 {
        if vertical {
            r.top()
        } else {
            r.left()
        }
    }
    /// Takes `rect` when it continues this run.
    fn extend(&mut self, clip: Rect, color: Color32, rect: Rect) -> bool {
        if clip != self.clip || color != self.color {
            return false;
        }
        let v = self.vertical;
        let across = if v {
            rect.left() == self.first.left() && rect.right() == self.first.right()
        } else {
            rect.top() == self.first.top() && rect.bottom() == self.first.bottom()
        };
        let len = if v { rect.height() } else { rect.width() };
        if !across || (len - self.length()).abs() > 0.01 {
            return false;
        }
        let step = Self::start(&rect, v) - Self::start(&self.last, v);
        let ok = if self.count == 1 {
            step >= self.length() - 0.01 && step - self.length() <= 6.
        } else {
            (step - self.period).abs() < 0.01
        };
        if !ok {
            return false;
        }
        self.period = step;
        self.last = rect;
        self.count += 1;
        true
    }
    fn finish(self) -> Clipped {
        let whole = self.first.union(self.last);
        let gap = self.period - self.length();
        let op = if self.count == 1 || gap < 0.01 {
            Op::Fill {
                rect: whole,
                color: self.color,
            }
        } else if self.vertical {
            let x = whole.center().x;
            Op::Dash {
                from: Pos2::new(x, whole.top()),
                to: Pos2::new(x, whole.bottom()),
                width: whole.width(),
                on: self.length(),
                off: gap,
                color: self.color,
            }
        } else {
            let y = whole.center().y;
            Op::Dash {
                from: Pos2::new(whole.left(), y),
                to: Pos2::new(whole.right(), y),
                width: whole.height(),
                on: self.length(),
                off: gap,
                color: self.color,
            }
        };
        Clipped {
            clip: self.clip,
            op,
        }
    }
}

/// Merges consecutive thin fills that continue one another, one op at a
/// time.
#[derive(Default)]
struct Coalescer {
    out: Vec<Clipped>,
    run: Option<Run>,
}

impl Sink for Coalescer {
    fn push(&mut self, c: Clipped) {
        self.add(c);
    }
}

impl Coalescer {
    fn add(&mut self, c: Clipped) {
        if let Op::Fill { rect, color } = c.op {
            let thin_v = rect.width() <= 2.01 && rect.height() >= rect.width();
            let thin_h = rect.height() <= 2.01 && rect.width() > rect.height();
            if let Some(r) = self.run.as_mut() {
                if r.extend(c.clip, color, rect) {
                    return;
                }
            }
            if let Some(r) = self.run.take() {
                self.out.push(r.finish());
            }
            if thin_v || thin_h {
                self.run = Some(Run {
                    clip: c.clip,
                    color,
                    vertical: thin_v,
                    first: rect,
                    last: rect,
                    period: 0.,
                    count: 1,
                });
                return;
            }
            self.out.push(c);
            return;
        }
        if let Some(r) = self.run.take() {
            self.out.push(r.finish());
        }
        self.out.push(c);
    }

    fn finish(mut self) -> Vec<Clipped> {
        if let Some(r) = self.run.take() {
            self.out.push(r.finish());
        }
        self.out
    }
}

/// Merges consecutive thin fills that continue one another: touching
/// pieces of a line become one rectangle, evenly spaced dashes one dashed
/// line. Paint order is kept.
#[cfg(test)]
pub fn coalesce(ops: Vec<Clipped>) -> Vec<Clipped> {
    let mut merge = Coalescer {
        out: Vec::with_capacity(ops.len()),
        run: None,
    };
    for c in ops {
        merge.add(c);
    }
    merge.finish()
}

//! Gantt geometry, independent of UI state and scheduling mutations.
//!
//! A bar is placed from the row's top, then offset by the slot pitch and the
//! bar style's offset (not from the row's center). Default body and cap sizes
//! below are chosen for 17 px rows.

use eframe::egui::{Color32, Pos2, Rect, Stroke, Vec2};
use openxer_engine::calendar::{WorkCalendar, DAY};

/// Row pitch: a 17-pixel cell and its 1-pixel line slot.
pub const ROW_HEIGHT: f32 = 18.;
/// Weekend and holiday shading: a very light blue gray, solid.
pub const NONWORK: Color32 = crate::theme::color(crate::theme::NONWORK);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeSpan {
    pub start: i64,
    pub end: i64,
}

/// Build once for each visible calendar, using the complete first/last
/// timescale cells as bounds. Reuse across all rows on that calendar.
/// Complexity is days * shift-count, never minutes * activity-count.
#[derive(Debug, Clone)]
pub struct CalendarSpans {
    pub range: TimeSpan,
    pub work: Vec<TimeSpan>,
    pub nonwork: Vec<TimeSpan>,
}
impl CalendarSpans {
    pub fn new(calendar: &WorkCalendar, start: i64, end: i64) -> Self {
        let mut result = Self {
            range: TimeSpan { start, end },
            work: vec![],
            nonwork: vec![],
        };
        if start >= end {
            return result;
        }
        if calendar.continuous {
            result.work.push(result.range);
            return result;
        }
        // Each day's working time as the engine reads it (CAL-1): the part
        // after midnight of an overnight shift is work on the next day, and
        // unsorted or overlapping shifts are taken in time order, so the
        // shading is exactly the time the schedule does not work.
        let first = start.div_euclid(DAY);
        let last = (end - 1).div_euclid(DAY);
        for day in first..=last {
            let Some(midnight) = day.checked_mul(DAY) else {
                continue;
            };
            for &(shift_start, shift_end) in calendar.work_shifts(day).iter() {
                let (Some(a), Some(b)) = (
                    midnight.checked_add(shift_start),
                    midnight.checked_add(shift_end),
                ) else {
                    continue;
                };
                let span = TimeSpan {
                    start: a.max(start),
                    end: b.min(end),
                };
                if span.start < span.end {
                    result.work.push(span);
                }
            }
        }
        result.work.sort_by_key(|s| (s.start, s.end));
        let mut merged: Vec<TimeSpan> = vec![];
        for span in result.work {
            if let Some(last) = merged.last_mut().filter(|last| span.start <= last.end) {
                last.end = last.end.max(span.end);
            } else {
                merged.push(span);
            }
        }
        result.work = merged;
        let mut cursor = start;
        for span in &result.work {
            if cursor < span.start {
                result.nonwork.push(TimeSpan {
                    start: cursor,
                    end: span.start,
                });
            }
            cursor = span.end;
        }
        if cursor < end {
            result.nonwork.push(TimeSpan { start: cursor, end });
        }
        result
    }

    /// Return whole nonworking cells from the same unit ladder the
    /// timescale and gridlines use. At hour zoom nothing is shaded.
    pub fn nonwork_cells(&self, cells: &[(i64, i64)], hour_zoom: bool) -> Vec<TimeSpan> {
        if hour_zoom {
            return vec![];
        }
        cells
            .iter()
            .filter_map(|&(start, end)| {
                if start >= end || start < self.range.start || end > self.range.end {
                    return None;
                }
                let i = self.work.partition_point(|span| span.end <= start);
                self.work
                    .get(i)
                    .is_none_or(|span| span.start >= end)
                    .then_some(TimeSpan { start, end })
            })
            .collect()
    }

    pub fn pixel_necks(
        &self,
        start: i64,
        end: i64,
        screen_x: impl Fn(i64) -> f32,
    ) -> Vec<(f32, f32)> {
        if start >= end {
            return vec![];
        }
        let first = self.nonwork.partition_point(|span| span.end <= start);
        self.nonwork[first..]
            .iter()
            .take_while(|span| span.start < end)
            .filter_map(|span| {
                let a = screen_x(span.start.max(start));
                let b = screen_x(span.end.min(end));
                (a.is_finite() && b.is_finite() && a < b).then_some((a, b))
            })
            .collect()
    }
}

// The live bar styles build only some shapes; the tests draw every one.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartShape {
    None,
    Rectangle,
    UpperHalf,
    LowerHalf,
    UpperThin,
    CenterThin,
    LowerThin,
    Diamond,
    PointLeft,
    PointRight,
    TriangleDown,
}
impl PartShape {
    /// Bodies that narrow over nonwork time: the full, half and center thin
    /// bodies. End icons such as the milestone diamond never narrow.
    fn allows_necking(self) -> bool {
        matches!(
            self,
            Self::Rectangle | Self::UpperHalf | Self::LowerHalf | Self::CenterThin
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PartStyle {
    pub shape: PartShape,
    pub color: Color32,
    pub stroke: Stroke,
}
impl PartStyle {
    pub fn new(shape: PartShape, color: Color32) -> Self {
        Self {
            shape,
            color,
            stroke: Stroke::NONE,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BarStyle {
    pub start: PartStyle,
    pub middle: PartStyle,
    pub end: PartStyle,
    pub height: f32,
    pub y_offset: f32,
    pub neck_for_calendar_nonwork: bool,
    pub neck_for_activity_nonwork: bool,
    pub summary: bool,
    pub start_source: String,
    pub end_source: String,
    pub percent_source: String,
}
impl BarStyle {
    #[cfg(test)]
    pub fn activity(color: Color32) -> Self {
        Self {
            start: PartStyle::new(PartShape::None, color),
            middle: PartStyle::new(PartShape::Rectangle, color),
            end: PartStyle::new(PartShape::None, color),
            height: 10.,
            y_offset: 0.,
            neck_for_calendar_nonwork: true,
            neck_for_activity_nonwork: false,
            summary: false,
            start_source: "start_date".into(),
            end_source: "end_date".into(),
            percent_source: String::new(),
        }
    }
    #[cfg(test)]
    pub fn milestone(color: Color32) -> Self {
        let mut style = Self::activity(color);
        style.start.shape = PartShape::Diamond;
        style.middle.shape = PartShape::None;
        style.neck_for_calendar_nonwork = false;
        style
    }
    #[cfg(test)]
    pub fn loe(color: Color32) -> Self {
        let mut style = Self::activity(color);
        style.start.shape = PartShape::PointLeft;
        style.end.shape = PartShape::PointRight;
        style.height = 8.;
        style.neck_for_calendar_nonwork = false;
        style
    }
    #[cfg(test)]
    pub fn summary(color: Color32) -> Self {
        let mut style = Self::activity(color);
        style.start.shape = PartShape::TriangleDown;
        style.end.shape = PartShape::TriangleDown;
        style.middle.shape = PartShape::CenterThin;
        style.height = 8.;
        style.summary = true;
        style.neck_for_calendar_nonwork = false;
        style
    }
    pub fn calendar_necking_allowed(&self) -> bool {
        self.neck_for_calendar_nonwork && self.binding_necking_allowed()
    }
    fn binding_necking_allowed(&self) -> bool {
        self.middle.shape.allows_necking()
            && !self.summary
            && self.percent_source.is_empty()
            && !self.start_source.is_empty()
            && !self.start_source.starts_with("user_")
            && !self.start_source.starts_with("end_")
    }
    pub fn activity_necking_allowed(&self) -> bool {
        // The binary uses the calendar gate irrespective of its enabled toggle.
        self.neck_for_activity_nonwork
            && self.binding_necking_allowed()
            && matches!(
                (self.start_source.as_str(), self.end_source.as_str()),
                ("start_date", "end_date")
                    | ("restart_date", "reend_date")
                    | ("act_start_date", "act_progress_date")
            )
    }
}

#[derive(Debug, Clone)]
pub enum Primitive {
    Rect(Rect),
    Polygon(Vec<Pos2>),
}
impl Primitive {
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Rect(rect) => *rect,
            Self::Polygon(points) => Rect::from_points(points),
        }
    }
}
#[derive(Debug, Clone)]
pub struct DrawPart {
    pub primitive: Primitive,
    pub color: Color32,
    pub stroke: Stroke,
    /// Drawn narrowed over nonwork time; read by the tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub necked: bool,
}
#[derive(Debug, Clone)]
pub struct BarParts {
    pub start: Vec<DrawPart>,
    pub middle: Vec<DrawPart>,
    pub end: Vec<DrawPart>,
    pub progress: Vec<DrawPart>,
    pub bounds: Rect,
    pub start_anchor: Pos2,
    pub finish_anchor: Pos2,
}
// Upper shapes keep the top edge and give up the bottom; lower shapes keep
// the bottom edge. Center thin trims a third from each side.
fn body_rect(rect: Rect, shape: PartShape) -> Rect {
    let half = (rect.height() / 2.).trunc();
    let thin = (rect.height() / 3.).trunc();
    match shape {
        PartShape::UpperHalf => {
            Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + half))
        }
        PartShape::LowerHalf => {
            Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - half), rect.max)
        }
        PartShape::UpperThin => {
            Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + thin))
        }
        PartShape::LowerThin => {
            Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - thin), rect.max)
        }
        PartShape::CenterThin => Rect::from_min_max(
            Pos2::new(rect.left(), rect.top() + thin),
            Pos2::new(rect.right(), rect.bottom() - thin),
        ),
        _ => rect,
    }
}
fn part(style: PartStyle, rect: Rect, necked: bool) -> Option<DrawPart> {
    if style.shape == PartShape::None || rect.width() <= 0. || rect.height() <= 0. {
        return None;
    }
    let c = rect.center();
    let primitive = match style.shape {
        PartShape::Diamond => Primitive::Polygon(vec![
            Pos2::new(c.x, rect.top()),
            Pos2::new(rect.right(), c.y),
            Pos2::new(c.x, rect.bottom()),
            Pos2::new(rect.left(), c.y),
        ]),
        PartShape::PointLeft => Primitive::Polygon(vec![
            Pos2::new(rect.left(), c.y),
            rect.right_top(),
            rect.right_bottom(),
        ]),
        PartShape::PointRight => Primitive::Polygon(vec![
            rect.left_top(),
            Pos2::new(rect.right(), c.y),
            rect.left_bottom(),
        ]),
        PartShape::TriangleDown => Primitive::Polygon(vec![
            rect.left_top(),
            rect.right_top(),
            Pos2::new(c.x, rect.bottom()),
        ]),
        _ => Primitive::Rect(body_rect(rect, style.shape)),
    };
    Some(DrawPart {
        primitive,
        color: style.color,
        stroke: style.stroke,
        necked,
    })
}

/// Compose three independent parts. Calendar neck spans are already clipped
/// to the visible time window. Activity necks may represent explicit suspended
/// intervals; they are NOT inferred from progress percentage.
pub fn compose_bar(
    row: Rect,
    start_x: f32,
    finish_x: f32,
    style: &BarStyle,
    calendar_necks: &[(f32, f32)],
    activity_necks: &[(f32, f32)],
    progress: f32,
) -> BarParts {
    let center_y = row.center().y + style.y_offset;
    let mut result = BarParts {
        start: vec![],
        middle: vec![],
        end: vec![],
        progress: vec![],
        bounds: Rect::NOTHING,
        start_anchor: Pos2::new(start_x, center_y),
        finish_anchor: Pos2::new(finish_x, center_y),
    };
    if !row.is_finite()
        || !start_x.is_finite()
        || !finish_x.is_finite()
        || !style.height.is_finite()
        || !center_y.is_finite()
    {
        return result;
    }
    let height = style.height.clamp(1., row.height().max(1.));
    let half = height / 2.;
    let left = start_x.min(finish_x);
    let right = start_x.max(finish_x).max(left + 2.);
    let mut middle_left = left;
    let mut middle_right = right;
    let cap_width = half.min((right - left) / 2.);
    if style.start.shape == PartShape::PointLeft {
        middle_left += cap_width;
    }
    if style.end.shape == PartShape::PointRight {
        middle_right -= cap_width;
    }
    let mut cuts: Vec<(f32, f32)> = vec![];
    for source in [
        if style.calendar_necking_allowed() {
            calendar_necks
        } else {
            &[]
        },
        if style.activity_necking_allowed() {
            activity_necks
        } else {
            &[]
        },
    ] {
        for &(a, b) in source {
            let a = a.max(middle_left);
            let b = b.min(middle_right);
            if a.is_finite() && b.is_finite() && a < b {
                cuts.push((a, b));
            }
        }
    }
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f32, f32)> = vec![];
    for (a, b) in cuts {
        if let Some(last) = merged.last_mut().filter(|last| a <= last.1) {
            last.1 = last.1.max(b);
        } else {
            merged.push((a, b));
        }
    }
    let mut cursor = middle_left;
    let mut body = |a: f32, b: f32, necked: bool| {
        if a >= b {
            return;
        }
        let full = Rect::from_min_max(Pos2::new(a, center_y - half), Pos2::new(b, center_y + half));
        if let Some(mut part) = part(style.middle, full, necked) {
            if necked {
                if let Primitive::Rect(rect) = &mut part.primitive {
                    // A necked body steps in three pixels from each edge, not
                    // a third of the bar height. Keep a one-pixel line for
                    // unusually thin custom bodies rather than invert a rect.
                    *rect = rect.shrink2(Vec2::new(
                        0.,
                        3.0f32.min(((rect.height() - 1.) / 2.).max(0.)),
                    ));
                }
            }
            result.middle.push(part);
        }
    };
    for (a, b) in merged {
        body(cursor, a, false);
        body(a, b, true);
        cursor = b;
    }
    body(cursor, middle_right, false);
    let start_rect = if style.start.shape == PartShape::PointLeft {
        Rect::from_min_max(
            Pos2::new(left, center_y - half),
            Pos2::new(middle_left, center_y + half),
        )
    } else {
        Rect::from_center_size(result.start_anchor, Vec2::splat(height))
    };
    let finish_rect = if style.end.shape == PartShape::PointRight {
        Rect::from_min_max(
            Pos2::new(middle_right, center_y - half),
            Pos2::new(right, center_y + half),
        )
    } else if style.end.shape == PartShape::TriangleDown {
        Rect::from_min_max(
            Pos2::new(finish_x - half, center_y),
            Pos2::new(finish_x + half, center_y + half),
        )
    } else {
        Rect::from_center_size(result.finish_anchor, Vec2::splat(height))
    };
    let start_rect = if style.start.shape == PartShape::TriangleDown {
        Rect::from_min_max(
            Pos2::new(start_x - half, center_y),
            Pos2::new(start_x + half, center_y + half),
        )
    } else {
        start_rect
    };
    if let Some(part) = part(style.start, start_rect, false) {
        result.start.push(part);
    }
    if let Some(part) = part(style.end, finish_rect, false) {
        result.end.push(part);
    }
    if progress.is_finite() && progress > 0. && style.middle.shape != PartShape::None {
        let rect = Rect::from_min_max(
            Pos2::new(left, center_y - half + 1.),
            Pos2::new(
                left + (right - left) * progress.clamp(0., 1.),
                center_y - half + 3.,
            ),
        );
        result.progress.push(DrawPart {
            primitive: Primitive::Rect(rect),
            color: Color32::BLACK,
            stroke: Stroke::NONE,
            necked: false,
        });
    }
    for part in result
        .middle
        .iter()
        .chain(&result.start)
        .chain(&result.end)
        .chain(&result.progress)
    {
        result.bounds = result.bounds.union(part.primitive.bounds());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    fn workweek() -> WorkCalendar {
        WorkCalendar::new(
            "week".into(),
            false,
            0,
            std::array::from_fn(|d| {
                if d < 5 {
                    vec![(480, 720), (780, 1020)]
                } else {
                    vec![]
                }
            }),
            BTreeMap::new(),
        )
    }
    fn row() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(400., ROW_HEIGHT))
    }
    #[test]
    fn calendar_weekend_lunch_and_exceptions_are_disjoint_spans() {
        let mut cal = workweek();
        cal.exc.insert(2, vec![]);
        let spans = CalendarSpans::new(&cal, 0, 7 * DAY);
        assert!(spans.nonwork.contains(&TimeSpan {
            start: 720,
            end: 780
        }));
        assert!(spans.nonwork.contains(&TimeSpan {
            start: DAY + 1020,
            end: 3 * DAY + 480
        }));
        assert!(spans.nonwork.contains(&TimeSpan {
            start: 4 * DAY + 1020,
            end: 7 * DAY
        }));
        let cells: Vec<_> = (0..7).map(|d| (d * DAY, (d + 1) * DAY)).collect();
        assert_eq!(
            spans.nonwork_cells(&cells, false),
            vec![
                TimeSpan {
                    start: 2 * DAY,
                    end: 3 * DAY
                },
                TimeSpan {
                    start: 5 * DAY,
                    end: 6 * DAY
                },
                TimeSpan {
                    start: 6 * DAY,
                    end: 7 * DAY
                }
            ]
        );
        assert!(spans.nonwork_cells(&cells, true).is_empty());
        let total: i64 = spans
            .work
            .iter()
            .chain(&spans.nonwork)
            .map(|s| s.end - s.start)
            .sum();
        assert_eq!(total, 7 * DAY);
        assert_eq!(NONWORK, crate::theme::color(0xEBF0F5));
    }
    #[test]
    fn continuous_and_overnight_periods_do_not_create_false_gaps() {
        let continuous = CalendarSpans::new(&WorkCalendar::continuous(0), -DAY, DAY);
        assert!(continuous.nonwork.is_empty());
        let cal = WorkCalendar::new(
            "night".into(),
            false,
            0,
            std::array::from_fn(|_| vec![(1320, 120)]),
            BTreeMap::new(),
        );
        let spans = CalendarSpans::new(&cal, 0, DAY);
        assert_eq!(
            spans.work,
            vec![
                TimeSpan { start: 0, end: 120 },
                TimeSpan {
                    start: 1320,
                    end: DAY
                }
            ]
        );
        assert_eq!(
            spans.nonwork,
            vec![TimeSpan {
                start: 120,
                end: 1320
            }]
        );
        assert!(CalendarSpans::new(&cal, 10, 5).work.is_empty());
    }
    /// The shading is exactly the complement of the engine's working view
    /// of the days (`WorkCalendar::work_shifts`): an overnight shift's hours
    /// after midnight are work on the next day, unsorted or overlapping
    /// shifts read in time order, and a dated exception replaces only the
    /// shifts that start on its day.
    #[test]
    fn shading_is_the_complement_of_the_engines_working_time() {
        let mut template: [Vec<(i64, i64)>; 7] = Default::default();
        // Day 0: an overnight shift, 22:00 to 06:00 the next morning.
        template[0] = vec![(1320, 360)];
        // Day 1: two shifts stored afternoon first.
        template[1] = vec![(780, 1020), (480, 720)];
        // Day 2: overlapping shifts, and one into the next morning.
        template[2] = vec![(600, 1020), (480, 700), (1380, 120)];
        template[3] = vec![(480, 1020)];
        let mut exc = BTreeMap::new();
        // Day 4 is a dated exception with its own overnight shift; day 5
        // has none of its own but gets that shift's morning.
        exc.insert(4, vec![(1200, 300)]);
        exc.insert(5, vec![]);
        let cal = WorkCalendar::new("mixed".into(), false, 0, template, exc);
        for (start, end) in [(0, 7 * DAY), (DAY + 300, 6 * DAY - 60)] {
            let spans = CalendarSpans::new(&cal, start, end);
            // The engine's working minutes over the range, as spans.
            let mut expected: Vec<TimeSpan> = vec![];
            for day in start.div_euclid(DAY)..=(end - 1).div_euclid(DAY) {
                for &(s, e) in cal.work_shifts(day).iter() {
                    let span = TimeSpan {
                        start: (day * DAY + s).max(start),
                        end: (day * DAY + e).min(end),
                    };
                    if span.start >= span.end {
                        continue;
                    }
                    match expected.last_mut() {
                        Some(last) if span.start <= last.end => last.end = last.end.max(span.end),
                        _ => expected.push(span),
                    }
                }
            }
            assert_eq!(spans.work, expected, "{start}..{end}");
            // Nonwork is exactly what is left.
            let mut cursor = start;
            let mut complement = vec![];
            for w in &expected {
                if cursor < w.start {
                    complement.push(TimeSpan {
                        start: cursor,
                        end: w.start,
                    });
                }
                cursor = w.end;
            }
            if cursor < end {
                complement.push(TimeSpan { start: cursor, end });
            }
            assert_eq!(spans.nonwork, complement, "{start}..{end}");
            // Spot checks: the overnight mornings are work, not shaded.
            for t in [DAY + 60, 3 * DAY + 60, 5 * DAY + 120] {
                if (start..end).contains(&t) {
                    assert!(
                        !spans.nonwork.iter().any(|s| (s.start..s.end).contains(&t)),
                        "{t} is work"
                    );
                }
            }
        }
    }
    #[test]
    fn projection_clips_to_bar_dates_and_shading_follows_supplied_cells() {
        let spans = CalendarSpans::new(&workweek(), 0, 7 * DAY);
        assert_eq!(
            spans.pixel_necks(700, 800, |m| m as f32),
            vec![(720., 780.)]
        );
        assert!(spans.nonwork_cells(&[(0, 7 * DAY)], false).is_empty());
        assert!(spans.nonwork_cells(&[(-DAY, 0)], false).is_empty());
    }
    #[test]
    fn body_necks_only_nonwork_and_keeps_endpoints_and_row_center() {
        let parts = compose_bar(
            row(),
            10.,
            100.,
            &BarStyle::activity(Color32::BLUE),
            &[(40., 60.)],
            &[],
            0.5,
        );
        assert_eq!(parts.middle.len(), 3);
        assert!(!parts.middle[0].necked);
        assert!(parts.middle[1].necked);
        assert_eq!(parts.middle[0].primitive.bounds().height(), 10.);
        assert_eq!(parts.middle[1].primitive.bounds().height(), 4.);
        let mid = ROW_HEIGHT / 2.;
        assert_eq!(parts.start_anchor, Pos2::new(10., mid));
        assert_eq!(parts.finish_anchor, Pos2::new(100., mid));
        assert!(row().contains_rect(parts.bounds));
        assert_eq!(parts.progress[0].primitive.bounds().right(), 55.);
    }
    #[test]
    fn independent_caps_preserve_color_and_milestone_has_no_middle() {
        let mut style = BarStyle::activity(Color32::BLUE);
        style.start = PartStyle::new(PartShape::Diamond, Color32::GREEN);
        style.end = PartStyle::new(PartShape::Diamond, Color32::RED);
        let parts = compose_bar(row(), 20., 80., &style, &[], &[], 0.);
        assert_eq!(parts.start[0].color, Color32::GREEN);
        assert_eq!(parts.middle[0].color, Color32::BLUE);
        assert_eq!(parts.end[0].color, Color32::RED);
        let milestone = compose_bar(
            row(),
            30.,
            30.,
            &BarStyle::milestone(Color32::BLACK),
            &[],
            &[],
            1.,
        );
        assert!(milestone.middle.is_empty());
        assert_eq!(milestone.start.len(), 1);
        assert_eq!(milestone.bounds.center().x, 30.);
        assert_eq!(milestone.bounds.width(), 10.);
    }
    #[test]
    fn necking_uses_field_binding_gates_and_merges_activity_calendar_cuts() {
        let mut style = BarStyle::activity(Color32::BLUE);
        style.neck_for_activity_nonwork = true;
        let parts = compose_bar(row(), 0., 100., &style, &[(20., 40.)], &[(30., 60.)], 0.);
        assert_eq!(parts.middle.len(), 3);
        assert_eq!(
            parts.middle[1].primitive.bounds().x_range(),
            20.0f32..=60.0f32
        );
        for name in ["user_custom_date", "end_date", ""] {
            style.start_source = name.into();
            assert!(!style.calendar_necking_allowed());
            assert!(!style.activity_necking_allowed());
        }
        style.start_source = "early_start_date".into();
        assert!(style.calendar_necking_allowed());
        assert!(!style.activity_necking_allowed());
        style.percent_source = "phys_complete_pct".into();
        assert!(!style.calendar_necking_allowed());
        style.percent_source.clear();
        style.summary = true;
        assert!(!style.calendar_necking_allowed());
    }
    #[test]
    fn narrow_loe_and_summary_are_finite_and_body_kind_insets_follow_binary() {
        for style in [
            BarStyle::loe(Color32::from_rgb(0, 128, 128)),
            BarStyle::summary(Color32::BLACK),
        ] {
            let parts = compose_bar(row(), 20., 21., &style, &[(0., 100.)], &[], 0.);
            assert!(parts.bounds.is_finite());
            assert!(!parts.middle.iter().any(|p| p.necked));
            assert!(row().contains_rect(parts.bounds));
        }
        let rect = Rect::from_min_max(Pos2::new(0., 0.), Pos2::new(100., 12.));
        assert_eq!(body_rect(rect, PartShape::UpperHalf).bottom(), 6.);
        assert_eq!(body_rect(rect, PartShape::LowerHalf).top(), 6.);
        assert_eq!(body_rect(rect, PartShape::UpperThin).bottom(), 4.);
        assert_eq!(body_rect(rect, PartShape::LowerThin).top(), 8.);
        assert_eq!(body_rect(rect, PartShape::CenterThin).height(), 4.);
        assert!(!PartShape::UpperThin.allows_necking());
        assert!(PartShape::LowerHalf.allows_necking());
    }
}

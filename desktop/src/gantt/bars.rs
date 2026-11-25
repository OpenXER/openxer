//! Turns a bar definition and an activity into dates, then into pixels.
//! Pure functions: no UI state, so the default bar set can be tested
//! without rendering.
use super::paint::{self, Span};
use super::settings::{BarDef, BarSource, BodyShape, EndShape, FilterKind, Pattern};
use crate::gantt_geometry::{self, BarStyle, PartShape, PartStyle, Primitive};
use eframe::egui::{Color32, Painter, Pos2, Rect, Stroke};
use openxer_engine::model::{Activity, CstrType, StatusType, TaskType};

/// Distance from the row (slot) top to the 9-pixel bar box, and its height,
/// as measured on the reference Gantt.
pub const BOX_INSET: f32 = 3.;
pub const BOX_HEIGHT: f32 = 9.;
/// Vertical pitch of the second and third bar rows.
pub const SLOT_PITCH: f32 = 12.;
/// Slot rectangle height handed to the shared bar composer.
const SLOT_HEIGHT: f32 = 17.;

/// The facts a filter or a date source needs about one activity.
#[derive(Clone, Copy, Debug)]
pub struct BarSubject<'a> {
    pub activity: &'a Activity,
    pub percent: f64,
    pub leveled: bool,
    pub data_date: Option<i64>,
    /// Start and finish in the project, primary, secondary and tertiary
    /// baselines (matched by Activity ID), when assigned.
    pub baselines: [Option<(i64, i64)>; 4],
    /// Critical by the project's definition (`columns::is_critical`), the
    /// same test the table's columns and filters use.
    pub critical: bool,
}

fn start_constraint(kind: CstrType) -> bool {
    matches!(
        kind,
        CstrType::StartOn
            | CstrType::StartOnBefore
            | CstrType::StartOnAfter
            | CstrType::MandatoryStart
    )
}
fn finish_constraint(kind: CstrType) -> bool {
    matches!(
        kind,
        CstrType::FinishOn
            | CstrType::FinishOnBefore
            | CstrType::FinishOnAfter
            | CstrType::MandatoryFinish
    )
}

pub fn filter_kind_matches(kind: FilterKind, s: &BarSubject) -> bool {
    let a = s.activity;
    match kind {
        FilterKind::AllActivities => true,
        FilterKind::Normal => matches!(a.task_type, TaskType::Task | TaskType::Resource),
        FilterKind::LevelOfEffort => a.task_type == TaskType::Loe,
        FilterKind::Milestone => matches!(a.task_type, TaskType::Mile | TaskType::FinMile),
        FilterKind::Summary => a.task_type == TaskType::Wbs,
        FilterKind::Critical => s.critical,
        FilterKind::NonCritical => !s.critical,
        FilterKind::NegativeFloat => a.total_float.is_some_and(|f| f < 0),
        FilterKind::HasStartConstraint => start_constraint(a.cstr) || start_constraint(a.cstr2),
        FilterKind::HasFinishConstraint => finish_constraint(a.cstr) || finish_constraint(a.cstr2),
        FilterKind::NotStarted => a.status == StatusType::NotStarted,
        FilterKind::InProgress => a.status == StatusType::InProgress,
        FilterKind::Completed => a.status == StatusType::Completed,
    }
}

pub fn bar_applies(def: &BarDef, s: &BarSubject) -> bool {
    let mut kinds = def.filter.kinds.iter().map(|&k| filter_kind_matches(k, s));
    if def.filter.match_all {
        kinds.all(|m| m)
    } else {
        kinds.any(|m| m)
    }
}

fn current_start(a: &Activity, leveled: bool) -> Option<i64> {
    crate::util::displayed_start(a, leveled)
}
fn current_finish(a: &Activity, leveled: bool) -> Option<i64> {
    crate::util::displayed_finish(a, leveled)
}

/// The bar's start and finish minutes, or None when the source has no dates
/// for this activity. Milestones collapse to their single date.
pub fn bar_span(source: BarSource, s: &BarSubject) -> Option<(i64, i64)> {
    let a = s.activity;
    let leveled = s.leveled;
    let span = match source {
        BarSource::CurrentBar => (current_start(a, leveled)?, current_finish(a, leveled)?),
        BarSource::PercentCompleteBar => {
            let start = current_start(a, leveled)?;
            let finish = current_finish(a, leveled)?;
            let fraction = if a.is_completed() {
                1.
            } else {
                (s.percent / 100.).clamp(0., 1.)
            };
            if fraction <= 0. {
                return None;
            }
            (
                start,
                start + ((finish - start) as f64 * fraction).round() as i64,
            )
        }
        BarSource::PlanBar => (a.target_start?, a.target_finish?),
        BarSource::ActualBar => {
            let start = a.act_start?;
            let finish = a.act_finish.or(s.data_date)?;
            (start, finish.max(start))
        }
        BarSource::RemainBar => {
            if a.is_completed() {
                return None;
            }
            let mut start = if leveled { a.restart } else { a.early_start }
                .or_else(|| current_start(a, leveled))?;
            let finish = if leveled { a.reend } else { a.early_finish }
                .or_else(|| current_finish(a, leveled))?;
            if a.act_start.is_some() {
                if let Some(dd) = s.data_date {
                    start = start.max(dd).min(finish);
                }
            }
            (start, finish)
        }
        BarSource::EarlyBar => (a.early_start?, a.early_finish?),
        BarSource::LateBar => (a.late_start?, a.late_finish?),
        BarSource::FloatBar => {
            let finish = current_finish(a, leveled)?;
            let late = a.late_finish?;
            if a.total_float.is_none_or(|f| f <= 0) || late <= finish {
                return None;
            }
            (finish, late)
        }
        BarSource::NegFloatBar => {
            let finish = current_finish(a, leveled)?;
            let late = a.late_finish?;
            if a.total_float.is_none_or(|f| f >= 0) || late >= finish {
                return None;
            }
            (late, finish)
        }
        BarSource::ProjectBaselineBar => s.baselines[0]?,
        BarSource::PrimaryBaselineBar => s.baselines[1]?,
        BarSource::SecondaryBaselineBar => s.baselines[2]?,
        BarSource::TertiaryBaselineBar => s.baselines[3]?,
    };
    let (start, finish) = span;
    let single = source == BarSource::CurrentBar || source.is_baseline();
    Some(match a.task_type {
        TaskType::Mile if single => (start, start),
        TaskType::FinMile if single => (finish, finish),
        _ => (start.min(finish), finish.max(start)),
    })
}

/// One painted piece of a bar.
#[derive(Clone, Debug)]
pub enum Piece {
    Rect {
        rect: Rect,
        color: Color32,
        pattern: Pattern,
        stroke: Stroke,
    },
    /// Whole-pixel rows (y, first column, last column), as the reference's
    /// aliased shapes are drawn. `outline` frames the shape in another color.
    Spans {
        spans: Vec<Span>,
        color: Color32,
        outline: Option<Color32>,
    },
}
impl Piece {
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Rect { rect, .. } => *rect,
            Self::Spans { spans, .. } => spans.iter().fold(Rect::NOTHING, |acc, &(y, a, b)| {
                acc.union(Rect::from_min_max(
                    Pos2::new(a, y),
                    Pos2::new(b + 1., y + 1.),
                ))
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BarDraw {
    pub pieces: Vec<Piece>,
    pub bounds: Rect,
}

/// Body offsets inside the 9-pixel bar box, top and bottom (exclusive).
pub fn body_offsets(shape: BodyShape) -> Option<(f32, f32)> {
    Some(match shape {
        BodyShape::None => return None,
        BodyShape::Full => (0., 9.),
        BodyShape::UpperHalf => (0., 5.),
        BodyShape::LowerHalf => (4., 9.),
        BodyShape::CenterHalf => (2., 7.),
        BodyShape::UpperThin => (0., 3.),
        BodyShape::CenterThin => (3., 6.),
        BodyShape::LowerThin => (6., 9.),
    })
}

fn outline(def: &BarDef) -> Stroke {
    if def.middle.outline {
        Stroke::new(1_f32, Color32::BLACK)
    } else {
        Stroke::NONE
    }
}

/// Rows of a cap shape centred on the date's pixel column `x`.
fn cap_spans(shape: EndShape, at_start: bool, x: f32, top: f32) -> Vec<Span> {
    let c = x;
    let rows = |widths: &[(f32, f32)], first: f32| -> Vec<Span> {
        widths
            .iter()
            .enumerate()
            .map(|(k, &(a, b))| (first + k as f32, a, b))
            .collect()
    };
    match shape {
        EndShape::None => vec![],
        EndShape::Diamond => paint::diamond(x, top),
        EndShape::TriangleDown => (0..5)
            .map(|k| {
                let half = 4. - k as f32;
                (top + 4. + k as f32, c - half, c + half)
            })
            .collect(),
        EndShape::TriangleUp => (0..5)
            .map(|k| (top + k as f32, c - k as f32, c + k as f32))
            .collect(),
        EndShape::PointLeft => (0..9i32)
            .map(|k| {
                let half = 4 - (k - 4i32).abs();
                (top + k as f32, c + 4. - half as f32, c + 4.)
            })
            .collect(),
        EndShape::PointRight => (0..9i32)
            .map(|k| {
                let half = 4 - (k - 4i32).abs();
                (top + k as f32, c - 4., c - 4. + half as f32)
            })
            .collect(),
        EndShape::Line => (0..9).map(|k| (top + k as f32, c - 1., c)).collect(),
        EndShape::Flag => {
            // A pole with a pennant pointing along the bar.
            let along = if at_start { 1. } else { -1. };
            let mut flag: Vec<Span> = (0..9).map(|k| (top + k as f32, c, c)).collect();
            for (k, length) in [4., 3., 2., 1.].into_iter().enumerate() {
                let tip = c + along * length;
                flag.push((top + k as f32, c.min(tip), c.max(tip)));
            }
            flag
        }
        // Measured: a cap four rows tall under each end of the summary body,
        // narrowing from four pixels to one, the same at both ends.
        EndShape::SummaryCap => rows(
            &[(x - 2., x + 1.), (x - 1., x + 1.), (x - 1., x), (x, x)],
            top + 5.,
        ),
    }
}

/// Lays out one bar in the slot whose top pixel is `slot_top`. `x0` is the
/// start date's pixel column and `x1` the finish date's; the body covers both
/// columns, as measured.
pub fn layout_bar(
    def: &BarDef,
    left: f32,
    slot_top: f32,
    x0: f32,
    x1: f32,
    calendar_necks: &[(f32, f32)],
) -> BarDraw {
    let top = slot_top + BOX_INSET;
    let stroke = outline(def);
    let mut pieces = vec![];
    let tapered_start = def.start.shape == EndShape::SummaryCap;
    let tapered_end = def.end.shape == EndShape::SummaryCap;
    if let Some((a, b)) = body_offsets(def.middle.shape) {
        let color = def.middle.color.color();
        if tapered_start || tapered_end {
            // Measured summary body: four pixels past each date, its ends
            // stepping inward one pixel every two rows (left on the third and
            // fifth rows, right on the second and fourth).
            let x1 = x1.max(x0);
            let spans = (0..(b - a) as i32)
                .map(|k| {
                    let left = if tapered_start {
                        x0 - 4. + (k / 2) as f32
                    } else {
                        x0
                    };
                    let right = if tapered_end {
                        x1 + 4. - ((k + 1) / 2) as f32
                    } else {
                        x1
                    };
                    (top + a + k as f32, left, right)
                })
                .collect();
            pieces.push(Piece::Spans {
                spans,
                color,
                outline: (stroke != Stroke::NONE).then_some(Color32::BLACK),
            });
        } else {
            let slot = Rect::from_min_max(
                Pos2::new(left, slot_top),
                Pos2::new(left.max(x1) + 2., slot_top + SLOT_HEIGHT),
            );
            let (start_source, end_source) = def.timescale.source_fields();
            let style = BarStyle {
                start: PartStyle::new(PartShape::None, color),
                middle: PartStyle {
                    shape: PartShape::Rectangle,
                    color,
                    stroke,
                },
                end: PartStyle::new(PartShape::None, color),
                height: b - a,
                y_offset: top + (a + b) / 2. - slot.center().y,
                neck_for_calendar_nonwork: def.neck_calendar_nonwork,
                neck_for_activity_nonwork: def.neck_activity_nonwork,
                summary: false,
                start_source: start_source.into(),
                end_source: end_source.into(),
                percent_source: String::new(),
            };
            // The finish column is part of the bar.
            let parts =
                gantt_geometry::compose_bar(slot, x0, x1 + 1., &style, calendar_necks, &[], 0.);
            for part in parts.middle {
                if let Primitive::Rect(rect) = part.primitive {
                    pieces.push(Piece::Rect {
                        rect,
                        color: part.color,
                        pattern: def.middle.pattern,
                        stroke: part.stroke,
                    });
                }
            }
        }
    }
    for (style, at_start, x) in [(def.start, true, x0), (def.end, false, x1)] {
        let spans = cap_spans(style.shape, at_start, x, top);
        if spans.is_empty() {
            continue;
        }
        let color = style.color.color();
        // Caps with a light fill (baseline milestones) take a black frame.
        let framed = style.shape != EndShape::SummaryCap
            && style.shape != EndShape::Flag
            && style.shape != EndShape::Line
            && color != Color32::BLACK;
        pieces.push(Piece::Spans {
            spans,
            color,
            outline: framed.then_some(Color32::BLACK),
        });
    }
    let bounds = pieces
        .iter()
        .fold(Rect::NOTHING, |acc, piece| acc.union(piece.bounds()));
    BarDraw { pieces, bounds }
}

fn hatch(painter: &Painter, area: Rect, pattern: Pattern, color: Color32) {
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(area));
    let pen = Stroke::new(1_f32, color);
    let step = 4.;
    let (horizontal, vertical, forward, backward) = match pattern {
        Pattern::Horizontal => (true, false, false, false),
        Pattern::Vertical => (false, true, false, false),
        Pattern::ForwardDiagonal => (false, false, true, false),
        Pattern::BackwardDiagonal => (false, false, false, true),
        Pattern::Cross => (true, true, false, false),
        Pattern::DiagonalCross => (false, false, true, true),
        Pattern::Solid | Pattern::Hollow => return,
    };
    if horizontal {
        let mut y = area.top() + 0.5;
        while y < area.bottom() {
            painter.line_segment([Pos2::new(area.left(), y), Pos2::new(area.right(), y)], pen);
            y += step;
        }
    }
    if vertical {
        let mut x = area.left() + 0.5;
        while x < area.right() {
            painter.line_segment([Pos2::new(x, area.top()), Pos2::new(x, area.bottom())], pen);
            x += step;
        }
    }
    let h = area.height();
    for (enabled, sign) in [(forward, 1.), (backward, -1.)] {
        if !enabled {
            continue;
        }
        let mut x = area.left() - h;
        while x < area.right() + h {
            let (a, b) = if sign > 0. {
                (Pos2::new(x, area.bottom()), Pos2::new(x + h, area.top()))
            } else {
                (Pos2::new(x, area.top()), Pos2::new(x + h, area.bottom()))
            };
            painter.line_segment([a, b], pen);
            x += step;
        }
    }
}

/// Fills a rectangle with a color and pattern. Hatches draw lines only so
/// the background shows through, as a hatched brush does.
pub fn fill_rect(painter: &Painter, rect: Rect, color: Color32, pattern: Pattern) {
    match pattern {
        Pattern::Solid => {
            painter.rect_filled(rect, 0., color);
        }
        Pattern::Hollow => {}
        _ => hatch(painter, rect, pattern, color),
    }
}

pub fn paint(painter: &Painter, draw: &BarDraw) {
    for piece in &draw.pieces {
        match piece {
            Piece::Rect {
                rect,
                color,
                pattern,
                stroke,
            } => {
                fill_rect(painter, *rect, *color, *pattern);
                let stroke = if *pattern == Pattern::Hollow && *stroke == Stroke::NONE {
                    Stroke::new(1_f32, *color)
                } else {
                    *stroke
                };
                if stroke != Stroke::NONE {
                    painter.rect_stroke(*rect, 0., stroke, eframe::egui::StrokeKind::Inside);
                }
            }
            Piece::Spans {
                spans,
                color,
                outline,
            } => match outline {
                Some(frame) => paint::outlined_spans(painter, spans, *color, *frame),
                None => paint::spans(painter, spans, *color),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::settings::{default_bars, GanttSettings};
    use super::*;
    use openxer_engine::calendar::DAY;

    fn activity(kind: TaskType, status: StatusType, float: i64) -> Activity {
        Activity {
            task_type: kind,
            status,
            early_start: Some(10 * DAY),
            early_finish: Some(14 * DAY),
            late_start: Some(12 * DAY),
            late_finish: Some(16 * DAY),
            total_float: Some(float),
            ..Default::default()
        }
    }
    fn subject(a: &Activity) -> BarSubject<'_> {
        BarSubject {
            activity: a,
            percent: 0.,
            leveled: false,
            data_date: Some(11 * DAY),
            baselines: [None; 4],
            critical: crate::grid::columns::is_critical(a, (false, 0.)),
        }
    }
    fn drawn(a: &Activity) -> Vec<String> {
        default_bars()
            .into_iter()
            .filter(|b| {
                b.display
                    && bar_applies(b, &subject(a))
                    && bar_span(b.timescale, &subject(a)).is_some()
            })
            .map(|b| b.name)
            .collect()
    }

    #[test]
    fn default_bars_pick_activity_kinds_like_the_reference() {
        let open = activity(TaskType::Task, StatusType::NotStarted, 2 * DAY);
        assert_eq!(drawn(&open), ["Remaining Work", "Current Bar Labels"]);
        let critical = activity(TaskType::Task, StatusType::NotStarted, 0);
        assert_eq!(
            drawn(&critical),
            ["Critical Remaining Work", "Current Bar Labels"]
        );
        let mut started = activity(TaskType::Task, StatusType::InProgress, 2 * DAY);
        started.act_start = Some(9 * DAY);
        assert_eq!(
            drawn(&started),
            ["Actual Work", "Remaining Work", "Current Bar Labels"]
        );
        let s = subject(&started);
        assert_eq!(
            bar_span(BarSource::ActualBar, &s),
            Some((9 * DAY, 11 * DAY))
        );
        assert_eq!(
            bar_span(BarSource::RemainBar, &s),
            Some((11 * DAY, 14 * DAY))
        );
        let mile = activity(TaskType::FinMile, StatusType::NotStarted, 0);
        assert_eq!(drawn(&mile), ["Milestone"]);
        assert_eq!(
            bar_span(BarSource::CurrentBar, &subject(&mile)),
            Some((14 * DAY, 14 * DAY))
        );
        let loe = activity(TaskType::Loe, StatusType::NotStarted, 0);
        assert_eq!(
            drawn(&loe),
            ["Remaining Level of Effort", "Current Bar Labels"]
        );
        let wbs = activity(TaskType::Wbs, StatusType::NotStarted, 0);
        assert_eq!(drawn(&wbs), ["Summary"]);
        assert_eq!(
            bar_span(BarSource::FloatBar, &subject(&open)),
            Some((14 * DAY, 16 * DAY))
        );
        assert_eq!(
            bar_span(BarSource::PrimaryBaselineBar, &subject(&open)),
            None
        );
        // With a baseline the bars span its dates; milestones take one date.
        let mut s = subject(&open);
        s.baselines = [
            Some((8 * DAY, 12 * DAY)),
            None,
            Some((9 * DAY, 13 * DAY)),
            None,
        ];
        assert_eq!(
            bar_span(BarSource::ProjectBaselineBar, &s),
            Some((8 * DAY, 12 * DAY))
        );
        assert_eq!(bar_span(BarSource::PrimaryBaselineBar, &s), None);
        assert_eq!(
            bar_span(BarSource::SecondaryBaselineBar, &s),
            Some((9 * DAY, 13 * DAY))
        );
        let mut s = subject(&mile);
        s.baselines[1] = Some((12 * DAY, 12 * DAY + 60));
        assert_eq!(
            bar_span(BarSource::PrimaryBaselineBar, &s),
            Some((12 * DAY + 60, 12 * DAY + 60))
        );
    }

    #[test]
    fn measured_geometry_places_bars_three_pixels_below_the_row_top() {
        let bars = GanttSettings::default().bars;
        let work = bars.iter().find(|b| b.name == "Remaining Work").unwrap();
        let draw = layout_bar(work, 0., 100., 20., 60., &[]);
        assert_eq!(
            draw.bounds,
            Rect::from_min_max(Pos2::new(20., 103.), Pos2::new(61., 112.)),
            "the finish column belongs to the bar"
        );
        let loe = bars
            .iter()
            .find(|b| b.name == "Remaining Level of Effort")
            .unwrap();
        let draw = layout_bar(loe, 0., 100., 20., 60., &[]);
        assert_eq!(draw.bounds.top(), 105., "LOE bars are centred, rows 5 to 9");
        assert_eq!(draw.bounds.bottom(), 110.);
        let mile = bars.iter().find(|b| b.name == "Milestone").unwrap();
        let draw = layout_bar(mile, 0., 100., 40., 40., &[]);
        assert_eq!(
            draw.bounds,
            Rect::from_min_max(Pos2::new(36., 103.), Pos2::new(45., 112.)),
            "a 9x9 diamond centred on the date column"
        );
        let summary = bars.iter().find(|b| b.name == "Summary").unwrap();
        let draw = layout_bar(summary, 0., 100., 20., 60., &[]);
        assert_eq!(draw.bounds.top(), 103.);
        assert_eq!(draw.bounds.bottom(), 112., "caps reach the box bottom");
        assert_eq!(draw.pieces.len(), 3);
        // Measured summary shape: body four pixels past both dates, tapering;
        // identical caps under each date.
        let Piece::Spans { spans, .. } = &draw.pieces[0] else {
            panic!("summary body is drawn in pixel rows");
        };
        assert_eq!(
            spans,
            &[
                (103., 16., 64.),
                (104., 16., 63.),
                (105., 17., 63.),
                (106., 17., 62.),
                (107., 18., 62.)
            ]
        );
        let Piece::Spans { spans: cap, .. } = &draw.pieces[2] else {
            panic!("summary caps are drawn in pixel rows");
        };
        assert_eq!(
            cap,
            &[
                (108., 58., 61.),
                (109., 59., 61.),
                (110., 59., 60.),
                (111., 60., 60.)
            ]
        );
    }
}

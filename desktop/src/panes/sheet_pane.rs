//! The Resource Usage Spreadsheet's lines drawn as the pane beside a view
//! table (`views::grid::Pane`): the Resource Assignments window's
//! time-phased spreadsheet. The lines are the Resource Usage Spreadsheet's
//! (`usage::ResRow`, one per table row, one per chosen time-phased field),
//! painted by its painters (`usage::paint_values`), with its cell selection
//! (`usage_cells`), so typing a period value, Fill Down and Across and the
//! Range commands work the same in both. The header keeps the window's own
//! week over day timescale, which scrolls and zooms like the Gantt's.
use super::usage::{self, ResRow};
use super::usage_cells::Range;
use crate::chrome::{self, CHROME, FONT_SIZE, INK, SELECT};
use crate::timescale::{LabelOptions, TimeScale, Unit};
use crate::views::chart;
use crate::views::grid::{self, Pane};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use std::rc::Rc;

/// Width of the value column with one time-phased field.
pub const VALUE_WIDTH: f32 = 64.;
/// Width of the value column when the lines name their fields.
pub const LABELLED_WIDTH: f32 = 150.;
/// The upper timescale tier's height, which a drag pans.
const TIER: f32 = 16.;

/// Day periods covering `[first, last]` model minutes.
pub fn day_periods(first: i64, last: i64) -> Vec<(i64, i64)> {
    let day = openxer_engine::calendar::DAY;
    let start = first.div_euclid(day) * day;
    let count = ((last - start) / day + 1).clamp(1, 3660);
    (0..count)
        .map(|d| (start + d * day, start + (d + 1) * day))
        .collect()
}

/// The pane: the lines, the periods and the selected cells.
pub(crate) struct SheetPane {
    pub axis: TimeScale,
    pub origin: i64,
    pub options: LabelOptions,
    pub periods: Vec<(i64, i64)>,
    /// One line per table row.
    pub lines: Rc<Vec<ResRow>>,
    pub value_title: String,
    /// The value column names each line's field (several fields).
    pub labelled: bool,
    pub selected_cells: Option<Range>,
    /// Set to scroll and zoom the spreadsheet like the Gantt.
    pub timeline: Option<crate::timeline::Spec>,
    pub place: crate::timeline::Place,
    /// Each line's row as drawn this frame: (line, rect).
    pub drawn: Vec<(usize, Rect)>,
}

impl SheetPane {
    pub fn value_width(&self) -> f32 {
        if self.labelled {
            LABELLED_WIDTH
        } else {
            VALUE_WIDTH
        }
    }

    /// The left edge of the period cells (after the header was drawn).
    pub fn cells_left(&self) -> f32 {
        self.axis.left
    }

    /// The cell of `period` in a row drawn at `row`.
    pub fn cell(&self, row: Rect, (a, b): (i64, i64)) -> Option<Rect> {
        let left = self.cells_left();
        let x0 = self.axis.x(a as f64).round().max(left);
        let x1 = self.axis.x(b as f64).round();
        let top = Pos2::new(x0, row.top());
        let bottom = Pos2::new(x1, row.bottom() - 1.);
        (x1 > x0).then_some(Rect::from_min_max(top, bottom))
    }

    /// The period under `x`, by its index.
    pub fn period_at(&self, x: f32) -> Option<usize> {
        if x < self.cells_left() {
            return None;
        }
        let at = self.axis.x_date(x) as i64;
        self.periods.iter().position(|(a, b)| at >= *a && at < *b)
    }
}

impl Pane for SheetPane {
    fn header(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let painter = ui.painter();
        let w = self.value_width();
        let value = Rect::from_min_size(rect.min, Vec2::new(w, rect.height()));
        painter.rect_filled(rect, 0., CHROME);
        let galley = painter.layout(
            self.value_title.clone(),
            FontId::proportional(FONT_SIZE),
            INK,
            w - 6.,
        );
        painter.galley(value.min + Vec2::new(3., 3.), galley, INK);
        painter.vline(
            value.right() - 0.5,
            value.y_range(),
            Stroke::new(1_f32, INK),
        );
        let cells = Rect::from_min_max(Pos2::new(value.right(), rect.top()), rect.max);
        self.axis.left = cells.left();
        chart::timescale_header(
            &painter.with_clip_rect(cells.intersect(painter.clip_rect())),
            cells,
            &self.axis,
            self.origin,
            &self.options,
            Unit::Week,
            Unit::Day,
        );
        painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1_f32, INK));
    }

    fn body(&mut self, painter: &egui::Painter, rect: Rect, _rows: &[(usize, Rect)]) {
        painter.rect_filled(rect, 0., Color32::WHITE);
    }

    fn timeline(&mut self) -> Option<grid::PaneTimeline<'_>> {
        let spec = self.timeline?;
        let inset = self.value_width();
        Some(grid::PaneTimeline {
            axis: &mut self.axis,
            spec,
            inset,
            pan_height: TIER + 1.,
            place: &mut self.place,
        })
    }

    fn row(&mut self, painter: &egui::Painter, rect: Rect, index: usize, selected: bool) {
        let Some(line) = self.lines.get(index) else {
            return;
        };
        self.drawn.push((index, rect));
        let w = self.value_width();
        let cells_left = rect.left() + w;
        // A band line in the top band level's fill (`grid::BAND`).
        let fill = if line.band {
            grid::BAND
        } else {
            Color32::WHITE
        };
        let body = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.bottom() - 1.));
        painter.rect_filled(body, 0., fill);
        let value = Rect::from_min_max(rect.min, Pos2::new(cells_left - 1., rect.bottom() - 1.));
        if selected && !line.band {
            painter.rect_filled(value, 0., SELECT);
        }
        // The selected cells.
        if let Some((lines, ks)) = self.selected_cells.as_ref() {
            if lines.contains(&index) {
                for k in ks.clone() {
                    if let Some(c) = self.periods.get(k).and_then(|q| self.cell(rect, *q)) {
                        painter.rect_filled(c, 0., usage::SELECTED_CELLS);
                    }
                }
            }
        }
        let pen = Stroke::new(1_f32, grid::LINE);
        painter.vline(cells_left - 0.5, rect.y_range(), pen);
        painter.hline(rect.x_range(), rect.bottom() - 0.5, pen);
        let ink = if line.band {
            grid::BAND_TEXT
        } else if selected {
            Color32::WHITE
        } else {
            INK
        };
        let values = if line.cumulative {
            usage::running(line.before, &line.values)
        } else {
            line.values.clone()
        };
        let unit = line.unit.as_deref();
        let texts: Vec<String> = values
            .iter()
            .map(|v| usage::value_text(*v, unit, false))
            .collect();
        let font = chrome::table_font();
        // The period cells keep their own ink; the value column follows the
        // row's selection.
        let cell_ink = if line.band { grid::BAND_TEXT } else { INK };
        usage::paint_period_cells(
            painter,
            body,
            &self.axis,
            &self.periods,
            cells_left,
            &texts,
            &font,
            cell_ink,
        );
        usage::paint_values(
            &painter.with_clip_rect(value.intersect(painter.clip_rect())),
            body,
            &self.axis,
            &[],
            (rect.left(), cells_left),
            self.labelled.then_some(line.label.as_str()),
            usage::value_text(line.total, unit, false),
            &[],
            &font,
            ink,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_periods_cover_whole_days() {
        let day = openxer_engine::calendar::DAY;
        let p = day_periods(day + 5, 3 * day);
        let want = vec![(day, 2 * day), (2 * day, 3 * day), (3 * day, 4 * day)];
        assert_eq!(p, want);
    }
}

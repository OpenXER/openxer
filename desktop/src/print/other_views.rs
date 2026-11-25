//! Printing the WBS, Projects, Resources and Resource Assignments views.
//! Each view is drawn by its own painter into the private print context on
//! a screen tall enough to show every row and wide enough to show its whole
//! chart or spreadsheet, without selection or details pane; the table and
//! its pane are cut out of that screen and paginated like the Activities
//! view (rows are never cut, the column header repeats on every page).
use super::compose::{capture, Canvas, Paint};
use super::ops;
use super::paginate::{self, Area, Extent, RowSlot};
use super::setup::PageSetup;
use crate::views::{chart, grid as vgrid, sheet, MainView};
use crate::*;

/// Widest and tallest virtual screen a view is drawn on.
const MAX_WIDTH: f32 = 200_000.;
const MAX_HEIGHT: f32 = 4_000_000.;
/// Room for bar labels after the last bar.
const LABEL_ROOM: f32 = 320.;
/// Room for start labels before the first bar (the chart starts earlier,
/// see `SummaryGantt::room_for_start_labels`).
const START_LABEL_ROOM: f32 = 160.;

/// Details panes hidden while printing and shown again afterwards.
#[derive(Default)]
struct Hidden {
    wbs: bool,
    projects: bool,
    resources: bool,
    assignments: bool,
}

impl App {
    /// The grid id, Page Setup name and caption of a printable view other
    /// than Activities.
    pub(crate) fn print_view(&self) -> Option<(&'static str, &'static str)> {
        Some(match self.views.main {
            MainView::Wbs => ("wbs", "WBS"),
            MainView::Projects => ("projects", "Projects"),
            MainView::Resources => ("resources", "Resources"),
            MainView::Assignments => ("assignments", "Resource Assignments"),
            _ => return None,
        })
    }

    fn hide_for_print(&mut self) -> Hidden {
        let v = &mut self.views;
        Hidden {
            wbs: std::mem::take(&mut v.wbs.show_details),
            projects: std::mem::take(&mut v.projects.show_details),
            resources: std::mem::take(&mut v.resources.show_bottom),
            assignments: std::mem::take(&mut v.assignments.show_details),
        }
    }

    fn show_after_print(&mut self, h: Hidden) {
        let v = &mut self.views;
        v.wbs.show_details = h.wbs;
        v.projects.show_details = h.projects;
        v.resources.show_bottom = h.resources;
        v.assignments.show_details = h.assignments;
    }

    /// Width of the virtual screen: the table, the splitter and the whole
    /// chart or spreadsheet, plus the view's frame and command bar.
    fn print_view_width(&mut self) -> f32 {
        let day_px = chart::WEEK_PITCH as f32 / 7.;
        let span_days = |a: Option<i64>, b: Option<i64>| match (a, b) {
            (Some(a), Some(b)) if b > a => ((b - a) / DAY) as f32 + 14.,
            _ => 60.,
        };
        let content = match self.views.main {
            MainView::Wbs => {
                let (start, finish) = self.print_project_span();
                self.views.wbs.table_width
                    + vgrid::SPLITTER
                    + span_days(Some(start), Some(finish)) * day_px
                    + START_LABEL_ROOM
                    + LABEL_ROOM
            }
            MainView::Projects => {
                let list = self.workspace_projects();
                let start = list.iter().filter_map(|p| p.start).min();
                let finish = list.iter().filter_map(|p| p.finish).max();
                self.views.projects.table_width
                    + vgrid::SPLITTER
                    + span_days(start, finish) * day_px
                    + START_LABEL_ROOM
                    + LABEL_ROOM
            }
            MainView::Resources => self.views.resources.columns.widths.iter().sum::<f32>() + 20.,
            MainView::Assignments => {
                let m = &self.project.model;
                let first = self.views.assignments.first_minute.map_or_else(
                    || m.project.data_date.map(|d| (d.div_euclid(DAY) - 1) * DAY),
                    |f| Some(f as i64),
                );
                let last = m
                    .activities
                    .iter()
                    .filter_map(|a| displayed_finish(a, m.options.do_level))
                    .max();
                self.views.assignments.table_width
                    + vgrid::SPLITTER
                    + crate::panes::sheet_pane::VALUE_WIDTH
                    + span_days(first, last) * (crate::views::assignments::SHEET_WEEK as f32 / 7.)
            }
            _ => 800.,
        };
        // Frame margins, the WBS command bar and some slack.
        (content + sheet::SIDE_BAR + 60.).clamp(400., MAX_WIDTH)
    }

    /// The printed canvas of the current view: ops with the column header
    /// at the top, its size, header height, row slots and body height.
    pub(super) fn compose_view(
        &mut self,
        ctx: &egui::Context,
        setup: &PageSetup,
        area: Area,
        paint: Paint,
    ) -> Result<Canvas, String> {
        let (id, _) = self.print_view().ok_or("This view cannot be printed.")?;
        let o = &setup.options;
        if !o.activity_table && !o.gantt_chart {
            return Err("Choose Activity Table, Gantt Chart or both on the Options tab.".into());
        }
        if paint != Paint::Now {
            return Ok(Canvas::unpainted());
        }
        let hidden = self.hide_for_print();
        self.print.painting = true;
        let width = self.print_view_width();
        let mut height = 1200.;
        let result = loop {
            self.action_rects.clear();
            let size = Vec2::new(width, height);
            let shapes = capture(ctx, size, |ui| self.other_view(ui));
            let rows = grid_rects(&self.action_rects, &format!("{id}_row_"));
            let cols = grid_rects(&self.action_rects, &format!("{id}_col_"));
            let last = rows.last().map_or(0., |r| r.bottom());
            // Every row is shown once the last one ends well above the
            // screen's bottom edge.
            if last + 2. * vgrid::BAND_HEIGHT < height || height >= MAX_HEIGHT {
                break (shapes, rows, cols);
            }
            height = (height * 2.).min(MAX_HEIGHT);
        };
        self.print.painting = false;
        self.show_after_print(hidden);
        let (shapes, rows, cols) = result;
        let Some(head) = cols.iter().copied().reduce(|a, b| a.union(b)) else {
            return Err("The view has no columns to print.".into());
        };
        let table_left = head.left();
        let header_h = head.height();
        let pane = !matches!(self.views.main, MainView::Resources);
        let table_right = if pane {
            let w = match self.views.main {
                MainView::Wbs => self.views.wbs.table_width,
                MainView::Projects => self.views.projects.table_width,
                _ => self.views.assignments.table_width,
            };
            table_left + w.max(150.)
        } else {
            head.right()
        };
        let left = if o.activity_table || !pane {
            table_left
        } else {
            table_right + vgrid::SPLITTER
        };
        let right = if o.gantt_chart && pane {
            let edge = width - sheet::SIDE_BAR - 20.;
            match self.views.main {
                MainView::Wbs | MainView::Projects => {
                    whole_weeks(self.action_rects.get(&format!("{id}_splitter")), edge)
                }
                _ => edge,
            }
        } else {
            table_right
        };
        let body_top = head.bottom();
        let body_h = rows.last().map_or(0., |r| r.bottom() - body_top).max(0.);
        let crop = Rect::from_min_max(
            Pos2::new(left, head.top()),
            Pos2::new(right.max(left + 1.), body_top + body_h),
        );
        let mut canvas = vec![];
        ops::place(&shapes, crop, 1., Pos2::ZERO, &mut canvas);
        let size = crop.size();
        let slots: Vec<RowSlot> = rows
            .iter()
            .map(|r| RowSlot::new(r.top() - body_top, r.height()))
            .collect();
        let plan = paginate::plan_with(
            setup.effective_scaling(),
            area,
            Extent {
                table_width: size.x,
                span_days: 0.,
                header_height: header_h,
                body_height: body_h,
            },
            self.pixels_per_day,
            paginate::Flow {
                one_wide: !setup.allow_tiling,
                ..paginate::Flow::default()
            },
        );
        paginate::check_pages(size.x, header_h, body_h, plan.scale, area)?;
        Ok(Canvas {
            ops: Some(std::rc::Rc::new(ops::Indexed::new(canvas))),
            size,
            header_h,
            slots,
            body_h,
            plan,
            timescale: None,
        })
    }
}

/// The right edge of a printed view chart: the last whole week cell of the
/// chart at or before `edge`, so the timescale's last cell is not cut at the
/// page edge (the WBS and Projects charts print at their default week pitch,
/// starting on a week boundary).
fn whole_weeks(splitter: Option<&[f32; 4]>, edge: f32) -> f32 {
    // The chart's axis starts one pixel right of the splitter.
    let Some(&[_, _, splitter_right, _]) = splitter else {
        return edge;
    };
    let left = splitter_right + 1.;
    let pitch = chart::WEEK_PITCH as f32;
    let weeks = ((edge - left) / pitch).floor();
    if weeks < 1. {
        return edge;
    }
    (left + weeks * pitch + 1.).min(edge)
}

#[test]
fn a_view_chart_prints_to_its_last_whole_week() {
    let pitch = chart::WEEK_PITCH as f32;
    let splitter = [400., 0., 404., 50.];
    // The chart starts at 405: ten whole weeks and part of an eleventh.
    let edge = 405. + 10.5 * pitch;
    assert_eq!(whole_weeks(Some(&splitter), edge), 405. + 10. * pitch + 1.);
    assert_eq!(whole_weeks(None, edge), edge);
    assert_eq!(whole_weeks(Some(&splitter), 420.), 420.);
}

/// The rectangles recorded under `prefix` followed by a number, in number
/// order.
fn grid_rects(actions: &BTreeMap<String, [f32; 4]>, prefix: &str) -> Vec<Rect> {
    let mut found: Vec<(usize, Rect)> = actions
        .iter()
        .filter_map(|(name, r)| {
            let k = name.strip_prefix(prefix)?.parse::<usize>().ok()?;
            Some((
                k,
                Rect::from_min_max(Pos2::new(r[0], r[1]), Pos2::new(r[2], r[3])),
            ))
        })
        .filter(|(_, r)| r.is_positive())
        .collect();
    found.sort_by_key(|(k, _)| *k);
    found.into_iter().map(|(_, r)| r).collect()
}

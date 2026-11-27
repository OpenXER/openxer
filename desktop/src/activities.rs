use super::*;
use crate::grid::Row;

#[cfg(test)]
#[path = "activities_tests.rs"]
pub(crate) mod tests;

/// Width of the splitter between the activity table and the Gantt chart.
pub(crate) const SPLITTER: f32 = 6.;

/// Width of the table pane for a pane `width` wide split at `fraction`.
/// The splitter goes from the far left (no table) to the far right (the
/// table fills the pane up to the splitter and the Gantt's vertical scroll
/// bar), so either side can be shown almost alone.
/// The scroll offset that brings the row from `top` to `bottom` into a
/// view `height` tall now scrolled to `current`, moving it the least; None
/// when the row is already in view. With no scroll state yet, the row goes
/// to the top.
pub(crate) fn reveal_offset(
    current: Option<f32>,
    height: f32,
    top: f32,
    bottom: f32,
) -> Option<f32> {
    let Some(current) = current else {
        return Some(top);
    };
    if top < current {
        Some(top)
    } else if bottom > current + height {
        Some((bottom - height).min(top))
    } else {
        None
    }
}

pub(crate) fn table_width(width: f32, fraction: f32) -> f32 {
    let most = (width - SPLITTER - hscroll::BAR_HEIGHT).max(0.);
    let fraction = if fraction.is_finite() { fraction } else { 0.6 };
    (width * fraction).clamp(0., most).round()
}

/// Paints a raised splitter in `rect` (6 px across): shadow, white, face,
/// face, shadow, shadow from the left edge (vertical splitter) or the top
/// edge (horizontal splitter). `rect` should sit on whole pixels.
pub(crate) fn paint_splitter(p: &egui::Painter, rect: Rect, vertical: bool) {
    let rect = Rect::from_min_max(rect.min.round(), rect.max.round());
    p.rect_filled(rect, 0., CHROME);
    for (offset, color) in [
        (0., chrome::pal::SHADOW),
        (1., Color32::WHITE),
        (4., chrome::pal::SHADOW),
        (5., chrome::pal::SHADOW),
    ] {
        if vertical {
            p.vline(
                rect.left() + offset + 0.5,
                rect.y_range(),
                Stroke::new(1_f32, color),
            );
        } else {
            p.hline(
                rect.x_range(),
                rect.top() + offset + 0.5,
                Stroke::new(1_f32, color),
            );
        }
    }
}

impl App {
    /// Zoom To Fit: the whole project in view, in readable units even after
    /// a manual choice of timescale units.
    pub(super) fn fit(&mut self) {
        self.fit_dates();
        // Fit chooses readable units for the full project even after a manual band choice.
        (self.view.upper, self.view.lower) =
            timescale::bands_for_scale(self.pixels_per_day as f64 * 7.);
        self.update_timescale_units();
    }

    /// Show the whole project's dates, as when a project is opened. The
    /// timescale units the user chose (Timescale dialog) stay.
    pub(super) fn fit_project(&mut self) {
        self.fit_dates();
        if self.view.automatic_units {
            self.update_timescale_units();
        } else {
            self.pixels_per_day = self.pixels_per_day.max((self.minimum_scale() / 7.) as f32);
        }
    }

    fn fit_dates(&mut self) {
        let m = &self.shown().model;
        let start = m
            .activities
            .iter()
            .filter_map(|a| displayed_start(a, m.options.do_level))
            .min()
            .or(m.project.data_date)
            .unwrap_or(0);
        let finish = m
            .activities
            .iter()
            .filter_map(|a| displayed_finish(a, m.options.do_level))
            .max()
            .unwrap_or(start + 60 * DAY);
        self.timeline_origin = start.div_euclid(DAY) * DAY - 2 * DAY;
        self.pixels_per_day =
            (self.chart_width / ((finish - start) / DAY + 8).max(8) as f32).clamp(1. / 7., 40.);
    }
    pub(super) fn activity_view(&mut self, host: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::WHITE))
            .show_inside(host, |ui| {
                // Show on Top > Table: the table alone, full width.
                let chart_on = self.panes.top != panes::TopView::Table;
                self.table_and_gantt(ui, chart_on, false);
            });
    }

    /// The activity table, with the Gantt Chart beside it when `chart_on`,
    /// filling `ui`. The top pane draws it with the layout's columns; the
    /// bottom pane (`bottom`) with its own columns swapped in, its own
    /// scroll position, and without taking the scroll-to-selection request
    /// the top pane answers.
    pub(crate) fn table_and_gantt(&mut self, ui: &mut egui::Ui, chart_on: bool, bottom: bool) {
        let ctx = &ui.ctx().clone();
        ui.spacing_mut().item_spacing.y = 0.;
        // The view caption, dock tabs and Layout/Filter bar are
        // drawn by views::docks; rebuild the grid's frame cache here.
        self.drop_frame_cache();
        // Nothing changes the project until the rows are painted: the
        // project is compared with its undo snapshot once for the pass.
        let own_pin = self.pinned_serial().is_none();
        if own_pin {
            self.pin_serial();
        }
        let rows = self.shown_rows_shared();
        self.layout_opts.hint = None;
        let width = ui.available_width();
        let vbar = hscroll::BAR_HEIGHT;
        let body_w = (width - vbar).max(0.);
        let table_w = if chart_on {
            let table_w = table_width(width, self.grid_fraction);
            // The bottom pane's own width while its state is swapped in.
            self.chart_width = (body_w - table_w - SPLITTER).max(0.);
            table_w
        } else {
            body_w
        };
        if !bottom {
            // What a printout can copy: the top pane's chart, when shown.
            self.print.screen_seen = chart_on;
        }
        // Line Numbers: a gutter before the first column.
        let gutter = self.line_number_width(rows.len()).min(table_w);
        self.measure_row_heights(ctx);
        // The horizontal scroll bars sit in a 16 px strip at the bottom of the
        // pane. The table's bar is there when its columns are wider than the
        // pane and the Gantt's whenever the chart is shown. They are handled
        // before the header and rows so a drag shows in the same frame.
        let view_w = (table_w - gutter).max(0.);
        let table_scrolls = self.table_content_width() > view_w + 0.5;
        let strip = if chart_on || table_scrolls {
            hscroll::BAR_HEIGHT
        } else {
            0.
        };
        let bars = (strip > 0.).then(|| {
            hscroll::Bars::new(
                ui.max_rect(),
                table_w,
                view_w.max(0.),
                (body_w - table_w - SPLITTER).max(0.),
                chart_on,
            )
        });
        if let Some(bars) = &bars {
            self.hscroll_interact(ui, bars, bottom);
        }
        let (head, _) =
            ui.allocate_exact_size(Vec2::new(width, self.gantt_header_height()), Sense::hover());
        let content_right = (head.right() - vbar).max(head.left());
        let p = ui.painter().clone();
        p.rect_filled(head, 0., CHROME);
        if gutter > 0. {
            self.paint_line_number_header(
                &p,
                Rect::from_min_size(head.min, Vec2::new(gutter, head.height())),
            );
        }
        let table_head = Rect::from_min_max(
            head.min + Vec2::new(gutter, 0.),
            Pos2::new(content_right, head.bottom()),
        );
        self.grid_header(ui, ctx, table_head, table_w - gutter);
        // The table pane ends where the splitter starts and the chart starts
        // after it; the two never overlap the splitter or the vertical scroll bar.
        let chart_x = if chart_on {
            table_w + SPLITTER
        } else {
            table_w
        };
        let chart = Rect::from_min_max(
            Pos2::new(head.left() + chart_x, head.top()),
            Pos2::new(content_right, head.bottom()),
        );
        // Corner cap above the vertical scroll bar.
        p.line_segment(
            [
                Pos2::new(content_right, head.top()),
                Pos2::new(content_right, head.bottom()),
            ],
            Stroke::new(1_f32, Color32::from_gray(160)),
        );
        p.line_segment(
            [Pos2::new(content_right, head.bottom()), head.right_bottom()],
            Stroke::new(1_f32, Color32::from_gray(160)),
        );
        if let Some(bar) = bars.as_ref().and_then(|b| b.gantt) {
            self.gantt_wheel_pan(ctx, &[chart, bar]);
        }
        let gantt = chart_on.then(|| {
            p.line_segment(
                [
                    Pos2::new(head.left() + chart_x, head.bottom()),
                    Pos2::new(content_right, head.bottom()),
                ],
                Stroke::new(1_f32, Color32::from_gray(160)),
            );
            self.gantt_header(ui, ctx, chart, &rows)
        });
        // Band rows are taller for bigger band fonts: the row offsets
        // are computed once per frame and the Gantt rows follow them.
        let metrics = self.row_metrics_shared(&rows);
        let total_h = *metrics.offsets.last().unwrap_or(&0.);
        let mut body_chart = gantt.as_ref().map(|g| g.body);
        let salt = if bottom {
            "bottom_activities_and_gantt"
        } else {
            "activities_and_gantt"
        };
        let view_height = (ui.available_height() - strip).max(0.);
        let scroll_id = ui.make_persistent_id(egui::Id::new(salt));
        let vscroll_rect = Rect::from_min_max(
            Pos2::new(content_right, head.bottom()),
            Pos2::new(head.right(), head.bottom() + view_height),
        );
        let vscroll_id = ui.id().with((salt, "vscroll"));
        let current_y = egui::scroll_area::State::load(ctx, scroll_id).map_or(0., |s| s.offset.y);
        let vrange = hscroll::Range {
            total: f64::from(total_h),
            view: f64::from(view_height),
            pos: f64::from(current_y),
        };
        let vmove = hscroll::interact_vertical(ui, vscroll_id, vscroll_rect, vrange);
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt(salt)
            .auto_shrink([false, false])
            .max_width(body_w)
            .max_height(view_height)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            // Wheel and scroll bar only: a mouse drag in the table
            // must not scroll it, or a click made while the pointer
            // moves lands on another row.
            .scroll_source(egui::scroll_area::ScrollSource {
                mouse_wheel: !ctx.input(|i| i.modifiers.ctrl),
                drag: false,
                ..Default::default()
            });
        if let Some(mv) = vmove {
            let line = self.gantt_row_height().max(18.);
            let page = (view_height - line).max(line);
            let max_y = vrange.max() as f32;
            let next_y = match mv {
                hscroll::Move::To(pos) => (pos as f32).clamp(0., max_y),
                hscroll::Move::Line(dir) => (current_y + dir as f32 * line).clamp(0., max_y),
                hscroll::Move::Page(dir) => (current_y + dir as f32 * page).clamp(0., max_y),
            };
            scroll = scroll.vertical_scroll_offset(next_y);
        } else if self.scroll_to_selected {
            if let Some(index) = rows
                .iter()
                .position(|row| matches!(row,Row::Activity(i) if Some(*i)==self.selected))
            {
                // Only as far as needed to show the row: a row already in
                // view stays where it is.
                let current = egui::scroll_area::State::load(ctx, scroll_id).map(|s| s.offset.y);
                if let Some(offset) = reveal_offset(
                    current,
                    view_height,
                    metrics.offsets[index],
                    metrics.offsets[index] + metrics.heights[index],
                ) {
                    scroll = scroll.vertical_scroll_offset(offset);
                }
            }
            // The top pane, drawn after the bottom one, answers it when
            // it shows the table.
            let top_table = matches!(
                self.panes.top,
                panes::TopView::Gantt | panes::TopView::Table
            );
            if !bottom || !top_table {
                self.scroll_to_selected = false;
            }
        }
        self.grid.cells.band_rects.clear();
        let scrolled = scroll.show_viewport(ui, |ui, viewport| {
            ui.spacing_mut().item_spacing.y = 0.;
            ui.set_height(total_h);
            self.grid.body_top = ui.max_rect().top();
            let first = metrics.row_at(viewport.min.y);
            let end = (metrics.row_at(viewport.max.y) + 1).min(rows.len());
            self.grid.page_rows = (first..end)
                .filter(|&k| {
                    metrics.offsets[k] >= viewport.min.y - 0.5
                        && metrics.offsets[k] + metrics.heights[k] <= viewport.max.y + 0.5
                })
                .count()
                .max(1);
            ui.add_space(metrics.offsets[first]);
            // Keep row ids stable while scrolling.
            ui.skip_ahead_auto_ids(first);
            let range = first..end;
            for row_index in range {
                let row_height = metrics.heights[row_index];
                // Drag senses carry selected activities onto a band.
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(body_w, row_height), Sense::click_and_drag());
                let chart_row = Rect::from_min_max(
                    Pos2::new(rect.left() + chart_x, rect.top()),
                    Pos2::new(content_right, rect.bottom()),
                );
                let pane_right = rect.left() + table_w;
                if chart_on {
                    body_chart =
                        Some(body_chart.map_or(chart_row, |old: Rect| old.union(chart_row)));
                }
                let table_row = Rect::from_min_max(rect.min + Vec2::new(gutter, 0.), rect.max);
                // Band and selection fills stop at the last column.
                let fill_right = self.columns_right(table_row.left(), pane_right);
                let number = Rect::from_min_size(rect.min, Vec2::new(gutter, row_height));
                let painter = ui.painter().clone();
                match rows[row_index] {
                    Row::Group(id) => {
                        self.grid_band(&painter, table_row, fill_right, id, &response);
                        self.band_menu(&response, id, bottom, table_row, pane_right);
                        if let Some(gantt) = &gantt {
                            self.gantt_group_row(&painter, gantt, chart_row, row_index, id);
                        }
                        if gutter > 0. {
                            self.paint_line_number(&painter, number, row_index + 1);
                        }
                    }
                    Row::Activity(i) => {
                        let a = self.shown().model.activities[i].clone();
                        let selected = self.is_selected(i);
                        painter.rect_filled(rect, 0., Color32::WHITE);
                        if selected {
                            painter.rect_filled(
                                Rect::from_min_max(
                                    rect.min,
                                    Pos2::new(fill_right.max(rect.left() + gutter), rect.bottom()),
                                ),
                                0.,
                                SELECT,
                            );
                        }
                        let current = self.selected == Some(i) && selected;
                        if response.clicked() {
                            self.click_row(i, ctx.input(|x| x.modifiers));
                        } else if response.secondary_clicked() && !selected {
                            self.click_row(i, Default::default());
                        }
                        let in_table = ctx
                            .input(|x| x.pointer.press_origin())
                            .is_some_and(|pos| pos.x < pane_right);
                        if response.drag_started() && in_table && self.edit_cell.is_none() {
                            self.start_row_drag(i);
                        }
                        let periods = format::activity_periods(self.shown(), i);
                        self.grid.cells.row_current = current;
                        self.grid_cells(
                            ui,
                            ctx,
                            &painter,
                            table_row,
                            table_w - gutter,
                            i,
                            metrics.depths[row_index],
                            selected,
                            &response,
                        );
                        if let Some(gantt) = &gantt {
                            self.gantt_activity_row(
                                ui, &painter, gantt, rect, chart_row, row_index, i, &a, selected,
                            );
                        }
                        if gutter > 0. {
                            self.paint_line_number(&painter, number, row_index + 1);
                        }
                        // Hint Help: the whole value of a truncated
                        // cell, or the activity over its bar. A marked
                        // Start or Finish names its constraint either way.
                        let constraint_tip = self.grid.cells.constraint_hint.take();
                        let hint_help = self.grid.layout.table.hint_help;
                        let in_table = ctx
                            .input(|i| i.pointer.hover_pos().is_some_and(|pos| pos.x < pane_right));
                        if in_table {
                            let hint = self.layout_opts.hint.clone().filter(|_| hint_help);
                            let text = match (hint, constraint_tip) {
                                (Some(h), Some(t)) => Some(format!("{h}\n{t}")),
                                (h, t) => h.or(t),
                            };
                            if let Some(text) = text {
                                response.clone().on_hover_text(text);
                            }
                        } else if hint_help {
                            response.clone().on_hover_text(format!(
                                "{} - {}\n{}\nRemaining {}",
                                a.task_code,
                                self.shown().name(i),
                                status_label(a.status),
                                format::duration(a.rem, periods)
                            ));
                        }
                        // Shift+F10 and the Menu key open the menu of the
                        // current row, just below it.
                        let at = (!bottom && current && self.ctx_menu.take()).then(|| {
                            Pos2::new(
                                (table_row.left() + 40.).min(pane_right - 20.),
                                rect.bottom(),
                            )
                        });
                        if at.is_some()
                            || ctx.input(|i| {
                                i.pointer
                                    .interact_pos()
                                    .is_some_and(|pos| pos.x < pane_right)
                            })
                            || response.context_menu_opened()
                        {
                            self.table_menu_popup_at(&response, "ctx", at);
                        }
                    }
                }
            }
        });
        let content_bottom = scrolled.inner_rect.top() - scrolled.state.offset.y
            + metrics.offsets.last().copied().unwrap_or(0.);
        self.empty_area_menu(ui, bottom, scrolled.inner_rect, content_bottom, table_w);
        self.grid.row_offsets = metrics.offsets.clone();
        self.finish_row_drag(ctx);
        self.commit_grid_edit();
        if let (Some(clip), Some(gantt)) = (body_chart, &gantt) {
            self.gantt_overlay(ui, ctx, gantt, clip, Some(&rows));
        }
        if chart_on {
            let splitter = Rect::from_min_max(
                Pos2::new(head.left() + table_w, head.top()),
                Pos2::new(head.left() + chart_x, ui.max_rect().bottom()),
            );
            paint_splitter(ui.painter(), splitter, true);
            let drag = ui
                .interact(splitter, ui.id().with("gantt_splitter"), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            self.action(
                if bottom {
                    "bottom_splitter"
                } else {
                    "splitter"
                },
                &drag,
            );
            if drag.dragged() {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    self.grid_fraction =
                        ((pos.x - SPLITTER / 2. - head.left()) / width.max(1.)).clamp(0., 1.);
                }
            }
        }
        if let Some(bars) = &bars {
            self.hscroll_paint(ui, bars, bottom);
            // Corner square where the horizontal and vertical scroll bars meet.
            let corner = Rect::from_min_max(
                Pos2::new(content_right, head.bottom() + view_height),
                Pos2::new(head.right(), head.bottom() + view_height + strip),
            );
            chrome::fill(ui.painter(), corner, chrome::pal::FACE);
        }
        let painted_vrange = hscroll::Range {
            total: f64::from(total_h),
            view: f64::from(view_height),
            pos: f64::from(scrolled.state.offset.y),
        };
        let vgeo = hscroll::paint_vertical(ui, vscroll_id, vscroll_rect, painted_vrange);
        let vname = if bottom {
            "bottom_vscroll_activities"
        } else {
            "vscroll_activities"
        };
        self.register_vbar(vname, &vgeo, &painted_vrange);
        // The pass is over. Edits made in it (cells, row drags, new
        // relationships) already ended the pin through their setters.
        if own_pin {
            self.unpin_serial();
        }
        if !rows.iter().any(|r| matches!(r, Row::Activity(_))) {
            // Inside the table body below any bands, clear of the scroll
            // strip under it.
            let below = self.grid.row_offsets.last().copied().unwrap_or(0.);
            ui.painter().text(
                Pos2::new(head.left() + gutter + 8., self.grid.body_top + below + 8.),
                egui::Align2::LEFT_TOP,
                self.empty_table_text(),
                egui::FontId::proportional(crate::chrome::FONT_SIZE),
                Color32::from_gray(80),
            );
        }
    }

    /// What an empty table says: a project with no activities is not a
    /// filter that matches none.
    pub(crate) fn empty_table_text(&self) -> &'static str {
        if self.shown().model.activities.is_empty() {
            "This project has no activities. Press Insert to add one."
        } else {
            "No activities match the current filter."
        }
    }

    /// Right-click on a group band: the band is selected first, then its
    /// menu opens. Shift+F10 opens it for the selected band.
    fn band_menu(
        &mut self,
        response: &egui::Response,
        index: usize,
        bottom: bool,
        table_row: Rect,
        pane_right: f32,
    ) {
        let Some(key) = self.grid.bands.borrow().get(index).map(|b| b.key.clone()) else {
            return;
        };
        let is_selected = |app: &App| app.grid.cells.selected_band.as_deref() == Some(key.as_str());
        if response.secondary_clicked() && !is_selected(self) {
            self.select_band(&key);
        }
        let ctx = &response.ctx;
        let at = (!bottom && is_selected(self) && self.ctx_menu.take()).then(|| {
            Pos2::new(
                (table_row.left() + 40.).min(pane_right - 20.),
                table_row.bottom(),
            )
        });
        if at.is_some()
            || ctx.input(|i| {
                i.pointer
                    .interact_pos()
                    .is_some_and(|pos| pos.x < pane_right)
            })
            || response.context_menu_opened()
        {
            self.band_menu_popup_at(response, at);
        }
    }

    /// Right-click below the last row: nothing is selected, so the menu
    /// shows the commands that need a selection disabled and the
    /// table-wide ones enabled. Without a selection, Shift+F10 opens it
    /// near the top of the table.
    fn empty_area_menu(
        &mut self,
        ui: &mut egui::Ui,
        bottom: bool,
        visible: Rect,
        content_bottom: f32,
        table_w: f32,
    ) {
        let top = visible.top().max(content_bottom).min(visible.bottom());
        let area = Rect::from_min_max(
            Pos2::new(visible.left(), top),
            Pos2::new(
                (visible.left() + table_w).min(visible.right()),
                visible.bottom(),
            ),
        );
        let id = ui.id().with(("table_empty_area", bottom));
        let response = ui.interact(area, id, Sense::click());
        if response.secondary_clicked() {
            self.selected = None;
            self.editing.selection.clear();
            self.grid.cells.selected_band = None;
            self.edit_cell = None;
            self.date_edits.clear();
        }
        let nothing = self.selected.is_none() && self.grid.cells.selected_band.is_none();
        let at = (!bottom && nothing && self.ctx_menu.take())
            .then(|| Pos2::new(visible.left() + 40., visible.top() + 24.));
        self.table_menu_popup_at(&response, "ctx", at);
    }
}

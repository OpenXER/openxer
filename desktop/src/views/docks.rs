//! Main-window docks and view frame: the 28 px icon-only vertical toolbars on
//! both sides, the view host with its focus edge, the 23 px view caption with
//! its close button, the bold dock tabs of the open views, and the 16 px
//! `Layout: ... | Filter: ...` bar. Geometry is in client pixels at 96 DPI.
use super::*;
use crate::details::assignments::ResourceAssign;
use chrome::{fill, hline, pal, vline, Icon, Item};

pub const DOCK_WIDTH: f32 = 28.;
/// View host frame: caption, tabs, margin and layout bar (y 81-159).
pub const HEADER_HEIGHT: f32 = 79.;
const FRAME_LEFT: f32 = 9.;
const FRAME_RIGHT: f32 = 9.;
const FRAME_BOTTOM: f32 = 9.;

/// One vertical dock toolbar button: (action, icon, tooltip, enabled).
pub(crate) type DockButton = (&'static str, Icon, &'static str, bool);

impl App {
    /// Name shown after `Layout:` for the Activities view.
    pub(crate) fn current_layout_name(&self) -> String {
        self.grid.layout.name.clone()
    }

    /// Name shown after `Filter:` for the Activities view.
    pub(crate) fn current_filter_name(&self) -> String {
        self.grid.layout.filter.summary(&self.grid.user_filters)
    }

    /// What the layout bar's `Filter:` names: the activity critical paths
    /// while they show (they filter the table), else the layout's filter.
    pub(crate) fn filter_bar_name(&self) -> String {
        self.critical_path_filter_name()
            .unwrap_or_else(|| self.current_filter_name())
    }

    /// The left and right docks, then the floating bars, then the action a
    /// directory or edit bar button asked for. Runs before the view area so
    /// the groups fit between.
    pub(super) fn side_docks(&mut self, ctx: &egui::Context) {
        self.edge_dock(ctx, crate::dockbars::Side::Left);
        self.edge_dock(ctx, crate::dockbars::Side::Right);
        self.floating_bars(ctx);
        if let Some(action) = self.views.bars.rt.clicked.take() {
            self.dispatch_dock(ctx, action);
        }
    }

    /// The button lists of the directory and edit bars for this frame.
    pub(crate) fn dock_specs(&self) -> BTreeMap<&'static str, Vec<DockButton>> {
        let mut specs: BTreeMap<_, _> = self.left_bars().into_iter().collect();
        specs.extend(self.right_bars());
        specs
    }

    /// What a directory or edit bar button does.
    fn dispatch_dock(&mut self, ctx: &egui::Context, action: &'static str) {
        let left = ["Standard", "Enterprise", "Project"].iter().any(|bar| {
            self.left_bars()
                .iter()
                .any(|(name, buttons)| name == bar && buttons.iter().any(|b| b.0 == action))
        });
        if left {
            self.dispatch_left(action);
        } else {
            self.dispatch_right(ctx, action);
        }
    }

    /// View caption, dock tabs, layout bar and the host frame of tab group
    /// `g` inside `ui`. Runs before the view's own panels so they nest inside
    /// the frame. Only the focused group draws the blue focus edge.
    pub(super) fn view_frame(&mut self, ui: &mut egui::Ui, g: usize, focused: bool) {
        let (outer, inner) = if focused {
            (pal::FOCUS_OUTER, pal::FOCUS_INNER)
        } else {
            (Color32::WHITE, pal::CAPTION_BG)
        };
        let id = |name: &str| {
            if g == 0 {
                egui::Id::new(name)
            } else {
                egui::Id::new((name, g))
            }
        };
        self.view_header(ui, id("view_header"), g, (outer, inner));
        let frame = egui::Frame::NONE.fill(pal::VIEW_MARGIN);
        egui::TopBottomPanel::bottom(id("view_frame_bottom"))
            .exact_height(FRAME_BOTTOM)
            .show_separator_line(false)
            .frame(frame)
            .show_inside(ui, |ui| {
                let r = ui.max_rect();
                let p = ui.painter();
                hline(p, r.left(), r.right(), r.bottom() - 3., outer);
                hline(p, r.left(), r.right(), r.bottom() - 2., pal::CAPTION_BG);
                hline(p, r.left(), r.right(), r.bottom() - 1., pal::SHADOW);
                frame_sides(p, r, (outer, inner));
                if focused {
                    let y = r.bottom() - 3.;
                    focus_corner(p, Pos2::new(r.left() + 1., y), -1., false);
                    focus_corner(p, Pos2::new(r.right() - 2., y), -1., true);
                }
                // The view area's raised inner edge closes under it: the left
                // shadow line reaches this row and a white line runs to the
                // right edge's white column.
                let inner_left = r.left() + FRAME_LEFT;
                let inner_right = r.right() - FRAME_RIGHT;
                fill(
                    p,
                    Rect::from_min_size(Pos2::new(inner_left - 1., r.top()), Vec2::splat(1.)),
                    pal::SHADOW,
                );
                hline(p, inner_left, inner_right + 1., r.top(), Color32::WHITE);
            });
        egui::SidePanel::left(id("view_frame_left"))
            .exact_width(FRAME_LEFT)
            .resizable(false)
            .show_separator_line(false)
            .frame(frame)
            .show_inside(ui, |ui| {
                let r = ui.max_rect();
                let p = ui.painter();
                vline(p, r.left(), r.top(), r.bottom(), Color32::WHITE);
                vline(p, r.left() + 1., r.top(), r.bottom(), outer);
                vline(p, r.left() + 2., r.top(), r.bottom(), inner);
                vline(p, r.right() - 1., r.top(), r.bottom(), pal::SHADOW);
            });
        egui::SidePanel::right(id("view_frame_right"))
            .exact_width(FRAME_RIGHT)
            .resizable(false)
            .show_separator_line(false)
            .frame(frame)
            .show_inside(ui, |ui| {
                let r = ui.max_rect();
                let p = ui.painter();
                // The view area reaches the panel; its white edge is the
                // panel's first column.
                vline(p, r.left(), r.top(), r.bottom(), Color32::WHITE);
                vline(p, r.right() - 3., r.top(), r.bottom(), inner);
                vline(p, r.right() - 2., r.top(), r.bottom(), outer);
                vline(p, r.right() - 1., r.top(), r.bottom(), pal::SHADOW);
            });
    }

    /// The Standard, Enterprise and Project bars.
    fn left_bars(&self) -> Vec<(&'static str, Vec<DockButton>)> {
        // As File > Close All: enabled while more than one project is open.
        let close_all = self.open_set.multi();
        let bars = vec![
            (
                "Standard",
                vec![
                    ("new", Icon::New, "New", true),
                    ("open", Icon::Open, "Open", true),
                    ("close_all", Icon::CloseAll, "Close All", close_all),
                ],
            ),
            (
                "Enterprise",
                vec![
                    ("directory_projects", Icon::Projects, "Projects", true),
                    ("directory_resources", Icon::Resources, "Resources", true),
                    ("dock_reports", Icon::Reports, "Reports", true),
                ],
            ),
            (
                "Project",
                vec![
                    ("directory_activities", Icon::Activities, "Activities", true),
                    (
                        "dock_additional_activities",
                        Icon::Activities,
                        "Additional Activities View",
                        true,
                    ),
                    ("directory_wbs", Icon::Wbs, "WBS", true),
                    (
                        "dock_resource_assignments",
                        Icon::AssignResource,
                        "Resource Assignments",
                        true,
                    ),
                    ("dock_wps_docs", Icon::WorkProducts, "Documents", true),
                    ("dock_expenses", Icon::Expenses, "Expenses", true),
                    ("dock_risks", Icon::Risks, "Risks", true),
                ],
            ),
        ];
        bars
    }

    fn dispatch_left(&mut self, action: &str) {
        match Some(action) {
            Some("new") => self.open_new_project(),
            Some("open") => self.open_project_dialog(),
            Some("close_all") => self.close_all_projects(),
            Some("directory_projects") => self.show_view(MainView::Projects),
            Some("directory_resources") => self.show_view(MainView::Resources),
            Some("directory_activities") => self.show_view(MainView::Activities),
            Some("dock_additional_activities") => self.open_additional_activities(),
            Some("directory_wbs") => self.show_view(MainView::Wbs),
            Some("dock_resource_assignments") => self.open_assignment_window(),
            Some("dock_reports") => self.show_view(MainView::Reports),
            Some("dock_wps_docs") => self.show_view(MainView::Documents),
            Some("dock_expenses") => self.show_view(MainView::Expenses),
            Some("dock_risks") => self.show_view(MainView::Risks),
            _ => {}
        }
    }

    /// The Edit, Assign and Move bars, acting on the current view.
    fn right_bars(&self) -> Vec<(&'static str, Vec<DockButton>)> {
        let activities = self.views.main.is_activities();
        let wbs = self.views.main == MainView::Wbs && self.views.wbs.selected.is_some();
        let selected = activities && self.selected.is_some();
        // Every view, the Activities view included: the commands of its
        // right-click menu and the Edit menu (`ViewEdit`).
        let view = |edit| self.view_edit_enabled(edit) == Some(true);
        let bars = vec![
            (
                "Edit",
                vec![
                    ("add", Icon::Add, "Add", view(ViewEdit::Add)),
                    (
                        "dock_delete",
                        Icon::Delete,
                        "Delete",
                        view(ViewEdit::Delete),
                    ),
                    ("dock_cut", Icon::Cut, "Cut", view(ViewEdit::Cut)),
                    ("dock_copy", Icon::Copy, "Copy", view(ViewEdit::Copy)),
                    ("dock_paste", Icon::Paste, "Paste", view(ViewEdit::Paste)),
                ],
            ),
            (
                "Assign",
                vec![
                    (
                        "dock_assign_resources",
                        Icon::AssignResource,
                        "Resources",
                        selected,
                    ),
                    (
                        "dock_assign_by_role",
                        Icon::AssignByRole,
                        "Resources by Role",
                        selected,
                    ),
                    ("dock_assign_roles", Icon::AssignRole, "Roles", selected),
                    (
                        "dock_assign_codes",
                        Icon::AssignCode,
                        "Activity Codes",
                        selected,
                    ),
                    (
                        "dock_assign_predecessors",
                        Icon::Predecessors,
                        "Predecessors",
                        selected,
                    ),
                    (
                        "dock_assign_successors",
                        Icon::Successors,
                        "Successors",
                        selected,
                    ),
                    ("dock_assign_steps", Icon::Steps, "Steps", selected),
                ],
            ),
            (
                "Move",
                vec![
                    ("dock_move_up", Icon::ShiftUp, "Move Up", wbs),
                    ("dock_move_down", Icon::ShiftDown, "Move Down", wbs),
                    ("dock_move_left", Icon::ShiftLeft, "Move Left", wbs),
                    ("dock_move_right", Icon::ShiftRight, "Move Right", wbs),
                ],
            ),
        ];
        bars
    }

    fn dispatch_right(&mut self, ctx: &egui::Context, action: &str) {
        let edit = match action {
            "add" => Some(ViewEdit::Add),
            "dock_delete" => Some(ViewEdit::Delete),
            "dock_cut" => Some(ViewEdit::Cut),
            "dock_copy" => Some(ViewEdit::Copy),
            "dock_paste" => Some(ViewEdit::Paste),
            _ => None,
        };
        if let Some(edit) = edit.filter(|e| self.view_edit_enabled(*e).is_some()) {
            self.view_edit(edit, ctx.input(|i| i.time));
            return;
        }
        let wbs_command = match action {
            // The edit commands ran above.
            "add" | "dock_delete" | "dock_cut" | "dock_copy" | "dock_paste" => None,
            "dock_move_up" => Some("Shift up"),
            "dock_move_down" => Some("Shift down"),
            "dock_move_left" => Some("Shift left"),
            "dock_move_right" => Some("Shift right"),
            "dock_assign_codes" => {
                self.open_assign(DetailTab::Codes);
                None
            }
            "dock_assign_predecessors" => {
                self.open_assign(DetailTab::Predecessors);
                None
            }
            "dock_assign_successors" => {
                self.open_assign(DetailTab::Successors);
                None
            }
            "dock_assign_steps" => {
                self.open_assign_steps();
                None
            }
            "dock_assign_resources" => {
                self.open_assign_resource(ResourceAssign::Resources);
                None
            }
            "dock_assign_by_role" => {
                self.open_assign_resource(ResourceAssign::ByRole);
                None
            }
            "dock_assign_roles" => {
                self.open_assign_resource(ResourceAssign::Roles);
                None
            }
            _ => {
                self.show_detail_tab(DetailTab::Resources);
                None
            }
        };
        if let Some(command) = wbs_command {
            self.wbs_command(command);
        }
    }

    /// View caption with close button, dock tabs, view margin and the
    /// Layout/Filter bar, framed by the host's focus edge.
    fn view_header(
        &mut self,
        ui: &mut egui::Ui,
        id: egui::Id,
        g: usize,
        (outer, inner): (Color32, Color32),
    ) {
        egui::TopBottomPanel::top(id)
            .exact_height(self.header_height())
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(pal::CAPTION_BG))
            .show_inside(ui, |ui| {
                let r = ui.max_rect();
                let (l, t, right) = (r.left(), r.top(), r.right());
                let p = ui.painter().clone();
                // Focus edge of the active dock site.
                fill(
                    &p,
                    Rect::from_min_max(r.min, Pos2::new(right, t + 1.)),
                    Color32::WHITE,
                );
                vline(&p, l, t, r.bottom(), Color32::WHITE);
                hline(&p, l + 1., right - 1., t + 1., outer);
                hline(&p, l + 2., right - 2., t + 2., inner);
                vline(&p, l + 1., t + 1., r.bottom(), outer);
                vline(&p, l + 2., t + 2., r.bottom(), inner);
                vline(&p, right - 1., t, r.bottom(), pal::SHADOW);
                vline(&p, right - 2., t + 1., r.bottom(), outer);
                vline(&p, right - 3., t + 2., r.bottom(), inner);
                if outer == pal::FOCUS_OUTER {
                    focus_corner(&p, Pos2::new(l + 1., t + 1.), 1., false);
                    focus_corner(&p, Pos2::new(right - 2., t + 1.), 1., true);
                }
                let (cl, cr) = (l + 3., right - 3.);
                // Caption bar.
                hline(&p, cl, cr, t + 3., pal::CAPTION_LINE);
                hline(&p, cl, cr, t + 25., pal::CAPTION_LINE);
                vline(&p, cl, t + 3., t + 26., pal::CAPTION_LINE);
                vline(&p, cr - 1., t + 3., t + 26., pal::CAPTION_LINE);
                p.text(
                    Pos2::new(cl + 4., t + 7.),
                    Align2::LEFT_TOP,
                    self.view_caption(self.views.main),
                    chrome::caption_font(),
                    INK,
                );
                let close = Rect::from_min_size(Pos2::new(right - 20., t + 9.), Vec2::splat(12.));
                let response = ui.interact(close, ui.id().with("view_close"), Sense::click());
                self.action("view_close", &response);
                paint_close(&p, close, response.hovered());
                if response.on_hover_text("Close").clicked() {
                    let view = self.views.groups.groups[g].view();
                    self.request_close(view);
                }
                // Dock tabs.
                let strip = Rect::from_min_max(Pos2::new(cl, t + 29.), Pos2::new(cr, t + 52.));
                fill(&p, strip, pal::FACE);
                hline(&p, cl, cr, t + 52., Color32::WHITE);
                hline(&p, cl, cr, t + 53., pal::LIGHT);
                let mut x = cl;
                for view in self.views.groups.groups[g].tabs.clone() {
                    let active = view == self.views.main;
                    let galley =
                        p.layout_no_wrap(self.view_caption(view), chrome::caption_font(), INK);
                    // A report tab carries its own close button.
                    let report = matches!(view, MainView::Report(_));
                    let extra = if report { 16. } else { 0. };
                    let width = (galley.size().x + if active { 14. } else { 10. } + extra).round();
                    let tab =
                        Rect::from_min_max(Pos2::new(x, t + 29.), Pos2::new(x + width, t + 56.));
                    let response =
                        ui.interact(tab, ui.id().with(("dock_tab", view.key())), Sense::click());
                    self.action(&format!("dock_tab_{}", view.key()), &response);
                    if active {
                        fill(&p, tab, pal::CAPTION_BG);
                        hline(&p, tab.left(), tab.right(), tab.top(), pal::TAB_BORDER);
                        vline(&p, tab.left(), tab.top(), tab.bottom(), pal::TAB_BORDER);
                        vline(
                            &p,
                            tab.right() - 1.,
                            tab.top(),
                            tab.bottom(),
                            pal::TAB_BORDER,
                        );
                        p.galley(Pos2::new(tab.left() + 8., t + 32.), galley, INK);
                    } else {
                        let body = Rect::from_min_max(
                            Pos2::new(tab.left(), t + 31.),
                            Pos2::new(tab.right(), t + 52.),
                        );
                        fill(&p, body, pal::TAB_INACTIVE);
                        hline(&p, body.left(), body.right(), body.top(), pal::TAB_BORDER);
                        vline(
                            &p,
                            body.right() - 1.,
                            body.top(),
                            body.bottom(),
                            pal::TAB_BORDER,
                        );
                        p.galley(Pos2::new(tab.left() + 5., t + 34.), galley, INK);
                    }
                    let mut closed = false;
                    if let MainView::Report(id) = view {
                        let y = if active { t + 38. } else { t + 40. };
                        let mark = Rect::from_center_size(
                            Pos2::new(tab.right() - 11., y),
                            Vec2::splat(12.),
                        );
                        let close = ui.interact(
                            mark,
                            ui.id().with(("report_tab_close", id.0)),
                            Sense::click(),
                        );
                        self.action(&format!("report_tab_close_{}", id.0), &close);
                        paint_tab_close(&p, mark, close.hovered());
                        closed =
                            close.on_hover_text("Close").clicked() || response.middle_clicked();
                        if closed {
                            self.request_close(view);
                        }
                        crate::context_menu::ContextMenu::new("report_tab_menu")
                            .min_width(190.)
                            .show(&response, self, |app, m| app.report_tab_menu(m, view));
                    }
                    if response.clicked() && !active && !closed {
                        self.show_view(view);
                    }
                    x += width;
                }
                // View margin and the layout bar.
                fill(
                    &p,
                    Rect::from_min_max(Pos2::new(cl, t + 56.), Pos2::new(cr, r.bottom())),
                    pal::VIEW_MARGIN,
                );
                let bar =
                    Rect::from_min_max(Pos2::new(cl + 5., t + 61.), Pos2::new(cr - 6., t + 79.));
                if self.views.main.is_activities() {
                    self.layout_bar(ui, bar);
                } else {
                    // The view's own `Layout:` or `Display:` strip heads its
                    // table; only the view frame's top line is drawn here,
                    // up to the frame's white right edge.
                    hline(&p, bar.left(), bar.right() + 2., bar.top(), pal::SHADOW);
                }
            });
    }

    /// Height of the view host header. Only the Activities views have the
    /// layout bar; the other views head their tables with their own strip,
    /// so their header ends under the view frame's top line.
    fn header_height(&self) -> f32 {
        if self.views.main.is_activities() {
            HEADER_HEIGHT
        } else {
            HEADER_HEIGHT - 17.
        }
    }

    /// The 16 px bar under the dock tabs: a flat `Layout:` header button
    /// (options menu) and, in the Activities view, `Filter:` and a find box.
    fn layout_bar(&mut self, ui: &mut egui::Ui, bar: Rect) {
        let p = ui.painter().clone();
        fill(&p, bar, pal::FACE);
        hline(&p, bar.left(), bar.right(), bar.top(), pal::SHADOW);
        hline(&p, bar.left(), bar.right(), bar.bottom() - 1., pal::SHADOW);
        vline(&p, bar.left(), bar.top(), bar.bottom(), pal::SHADOW);
        vline(&p, bar.right(), bar.top(), bar.bottom(), pal::SHADOW);
        vline(
            &p,
            bar.right() + 1.,
            bar.top(),
            bar.bottom(),
            Color32::WHITE,
        );
        hline(
            &p,
            bar.left() + 1.,
            bar.right(),
            bar.top() + 1.,
            Color32::WHITE,
        );
        vline(
            &p,
            bar.left() + 1.,
            bar.top() + 1.,
            bar.bottom() - 1.,
            Color32::WHITE,
        );
        let activities = self.views.main.is_activities();
        // Only the Activities views have this bar (see `header_height`).
        let (prefix, name) = ("Layout: ", self.current_layout_name());
        let button = Rect::from_min_max(
            Pos2::new(bar.left() + 1., bar.top() + 1.),
            Pos2::new(bar.left() + 341., bar.bottom() - 1.),
        );
        let response = ui.interact(button, ui.id().with("layout_button"), Sense::click());
        self.action("layout_button", &response);
        if response.hovered() {
            fill(&p, button.shrink(1.), pal::HOT_FILL);
        }
        vline(
            &p,
            button.right(),
            button.top(),
            button.bottom(),
            pal::SHADOW,
        );
        vline(
            &p,
            button.right() + 2.,
            button.top(),
            button.bottom(),
            Color32::WHITE,
        );
        // Outlined down triangle (lit from the top left), then the caption.
        let tri = Pos2::new(button.left() + 11., bar.top() + 7.);
        let dot = |dx: f32, dy: f32, color: Color32| {
            fill(
                &p,
                Rect::from_min_size(tri + Vec2::new(dx, dy), Vec2::splat(1.)),
                color,
            )
        };
        let (mid, light) = (Color32::from_gray(0x7D), Color32::from_gray(0xBE));
        dot(0., 0., mid);
        hline(&p, tri.x + 1., tri.x + 8., tri.y, light);
        dot(8., 0., INK);
        for step in 1..4 {
            let s = step as f32;
            dot(s, s, mid);
            dot(8. - s, s, INK);
        }
        dot(4., 4., INK);
        p.with_clip_rect(button).text(
            Pos2::new(button.left() + 24., bar.top() + 3.5),
            Align2::LEFT_TOP,
            format!("{prefix}{name}"),
            chrome::table_font(),
            INK,
        );
        if activities {
            egui::Popup::menu(&response)
                .style(chrome::popup_menu_style)
                .show(|ui| self.layout_options_menu(ui));
            // A usage pane filtering the table (Display Activities for
            // selected Resource or Time Period) adds to the caption, in
            // blue, with a Clear button at the end.
            let usage = self.usage_filter_caption();
            let clear_w = if usage.is_some() { 48. } else { 0. };
            let filter = Rect::from_min_max(
                Pos2::new(button.right() + 3., bar.top() + 1.),
                Pos2::new(bar.right() - 1. - clear_w, bar.bottom() - 1.),
            );
            let response = ui.interact(filter, ui.id().with("filter_button"), Sense::click());
            self.action("filter_button", &response);
            if response.hovered() {
                fill(&p, filter, pal::HOT_FILL);
            }
            let (caption, ink) = match &usage {
                Some(u) => (
                    format!("Filter: {} + {u}", self.filter_bar_name()),
                    chrome::BLUE,
                ),
                None => (format!("Filter: {}", self.filter_bar_name()), INK),
            };
            p.with_clip_rect(filter).text(
                Pos2::new(filter.left() + 1., bar.top() + 3.5),
                Align2::LEFT_TOP,
                caption,
                chrome::table_font(),
                ink,
            );
            if response.clicked() {
                self.open_filters_dialog();
            }
            if usage.is_some() {
                let clear = Rect::from_min_max(
                    Pos2::new(filter.right() + 1., bar.top() + 1.),
                    Pos2::new(bar.right() - 1., bar.bottom() - 1.),
                );
                let response = ui
                    .interact(clear, ui.id().with("filter_usage_clear"), Sense::click())
                    .on_hover_text(
                        "Show every activity again (turns off Display Activities for selected)",
                    );
                self.action("filter_usage_clear", &response);
                fill(
                    &p,
                    clear,
                    if response.hovered() {
                        pal::HOT_FILL
                    } else {
                        pal::LIGHT
                    },
                );
                vline(&p, clear.left(), clear.top(), clear.bottom(), pal::SHADOW);
                p.with_clip_rect(clear).text(
                    clear.center(),
                    Align2::CENTER_CENTER,
                    "Clear",
                    chrome::table_font(),
                    INK,
                );
                if response.clicked() {
                    self.clear_usage_table_filter();
                }
            }
        }
    }

    /// The popup behind the `Layout:` button, in the native popup style.
    fn layout_options_menu(&mut self, ui: &mut egui::Ui) {
        chrome::submenu(ui, Item::new("&Layout"), |ui| self.layout_file_items(ui));
        chrome::menu_separator(ui);
        if self.item(ui, "layout_bars", Item::new("&Bars...").icon(Icon::Bars)) {
            self.open_bars_dialog();
        }
        if self.item(
            ui,
            "view_options",
            Item::new("Bar C&hart Options...").icon(Icon::Options),
        ) {
            self.open_chart_options();
        }
        if self.item(
            ui,
            "menu_columns",
            Item::new("&Columns...").icon(Icon::Columns),
        ) {
            self.open_columns_dialog();
        }
        if self.item(
            ui,
            "layout_timescale",
            Item::new("&Timescale...").icon(Icon::Timescale),
        ) {
            self.open_timescale_dialog();
        }
        let spotlight = self.gantt.options.spotlight;
        if self.item(
            ui,
            "layout_spotlight",
            Item::new("Progress S&potlight").checked(spotlight),
        ) {
            self.toggle_spotlight();
        }
        let line = self.gantt.options.progress_line.show;
        if self.item(
            ui,
            "layout_progress_line",
            Item::new("Progress L&ine").checked(line),
        ) {
            self.gantt.options.progress_line.show = !line;
        }
        chrome::submenu(ui, Item::new("Attach&ments"), |ui| {
            let minute = self.chart_center_minute();
            self.attachment_items(ui, "layout_attachments", minute);
        });
        if self.item(
            ui,
            "layout_table_font",
            Item::new("Table &Font and Row...").icon(Icon::TableFont),
        ) {
            self.open_table_font();
        }
        let network = self.panes.top == crate::panes::TopView::Network;
        chrome::submenu(ui, Item::new("Activity &Network").enabled(network), |ui| {
            self.network_menu_items(ui)
        });
        if self.item(
            ui,
            "layout_bottom_options",
            Item::new("Bottom Layout &Options..."),
        ) {
            self.open_bottom_layout_options();
        }
        chrome::menu_separator(ui);
        if self.item(
            ui,
            "menu_filters",
            Item::new("Filte&rs...").icon(Icon::Filter),
        ) {
            self.open_filters_dialog();
        }
        if self.item(
            ui,
            "menu_group_sort",
            Item::new("&Group and Sort...").icon(Icon::GroupSort),
        ) {
            self.open_group_sort_dialog();
        }
        chrome::menu_separator(ui);
        chrome::submenu(ui, Item::new("Show on &Top"), |ui| self.top_menu_items(ui));
        chrome::submenu(ui, Item::new("Show on Botto&m"), |ui| {
            self.bottom_menu_items(ui, "Activity Details")
        });
        chrome::menu_separator(ui);
        let hints = self.grid.layout.table.hint_help;
        if self.item(
            ui,
            "layout_hint_help",
            Item::new("Cell Tips").checked(hints),
        ) {
            self.toggle_hint_help();
        }
        chrome::menu_separator(ui);
        if self.item(
            ui,
            "layout_expand_all",
            Item::new("&Expand All")
                .key("Ctrl+Num +")
                .icon(Icon::ExpandAll),
        ) {
            self.expand_all_bands();
        }
        if self.item(
            ui,
            "layout_collapse_all",
            Item::new("Co&llapse All")
                .key("Ctrl+Num -")
                .icon(Icon::CollapseAll),
        ) {
            self.collapse_all_bands();
        }
        if self.item(
            ui,
            "layout_collapse_to",
            Item::new("Collapse &To...").icon(Icon::CollapseTo),
        ) {
            self.open_collapse_to();
        }
    }

    /// Closes the tab of `view` and shows what is left of its group. The
    /// Activities view always stays open.
    pub(crate) fn close_tab(&mut self, view: MainView) {
        if self.views.groups.remove_view(view) {
            if let MainView::Report(id) = view {
                self.free_report_tab(id);
            }
            let next = self.views.groups.current();
            self.show_view(next);
        }
    }

    /// Asks for the tab of `view` to close once the view area is drawn.
    pub(crate) fn request_close(&mut self, view: MainView) {
        if view != MainView::Activities && !self.views.close_queue.contains(&view) {
            self.views.close_queue.push(view);
        }
    }
}

/// The small close mark of a report tab: a cross that turns red when hot.
fn paint_tab_close(p: &egui::Painter, rect: Rect, hot: bool) {
    let c = rect.center();
    let ink = if hot { pal::CLOSE_RED } else { INK };
    if hot {
        p.rect_filled(rect.shrink(1.), 0., Color32::from_rgb(0xFB, 0xDD, 0xDD));
    }
    for (a, b) in [((-3., -3.), (3., 3.)), ((-3., 3.), (3., -3.))] {
        p.line_segment(
            [c + Vec2::new(a.0, a.1), c + Vec2::new(b.0, b.1)],
            Stroke::new(1.2_f32, ink),
        );
    }
}

/// The caption's close button: a 12 px red square with a white X.
fn paint_close(p: &egui::Painter, rect: Rect, hot: bool) {
    let red = if hot {
        Color32::from_rgb(0xE0, 0x43, 0x43)
    } else {
        pal::CLOSE_RED
    };
    fill(p, rect, red);
    let o = rect.min + Vec2::new(3., 3.);
    for (dx, dy) in [
        (0., 0.),
        (1., 0.),
        (3., 0.),
        (4., 0.),
        (1., 1.),
        (2., 1.),
        (3., 1.),
        (2., 2.),
        (1., 3.),
        (2., 3.),
        (3., 3.),
        (0., 4.),
        (1., 4.),
        (3., 4.),
        (4., 4.),
    ] {
        fill(
            p,
            Rect::from_min_size(o + Vec2::new(dx, dy), Vec2::splat(1.)),
            Color32::WHITE,
        );
    }
}

/// The host's left and right edges where a full-width panel crosses them.
/// One rounded corner of the focus edge: the reference blends the two blue
/// lines into the white around them over three pixels. `corner` is the
/// outer line's corner pixel; `dy` points down the side (1 at the top, -1
/// at the bottom) and `right` mirrors it. The bottom edge has no inner line.
fn focus_corner(p: &egui::Painter, corner: Pos2, dy: f32, right: bool) {
    const ROUND: [(f32, f32, u32); 8] = [
        (0., 0., 0xF7F8F9),
        (1., 0., 0xDDEFFD),
        (0., 1., 0xDDEFFD),
        (2., 0., 0xCEE9FF),
        (0., 2., 0xCEE9FF),
        (1., 1., 0xD5ECFF),
        (1., 2., 0xE3F2FF),
        (2., 1., 0xE3F2FF),
    ];
    let dx = if right { -1. } else { 1. };
    for (x, y, rgb) in ROUND {
        if dy < 0. && (x, y) == (2., 1.) {
            continue;
        }
        let color = Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
        let at = corner + Vec2::new(x * dx, y * dy);
        fill(p, Rect::from_min_size(at, Vec2::splat(1.)), color);
    }
}

fn frame_sides(p: &egui::Painter, r: Rect, (outer, inner): (Color32, Color32)) {
    vline(p, r.left(), r.top(), r.bottom(), Color32::WHITE);
    vline(p, r.left() + 1., r.top(), r.bottom() - 2., outer);
    vline(p, r.left() + 2., r.top(), r.bottom() - 3., inner);
    vline(p, r.right() - 1., r.top(), r.bottom(), pal::SHADOW);
    vline(p, r.right() - 2., r.top(), r.bottom() - 2., outer);
    vline(p, r.right() - 3., r.top(), r.bottom() - 3., inner);
}

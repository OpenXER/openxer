//! Drawing and mouse handling for the movable bars: the four dock areas, the
//! floating bar windows, the grips that pick a bar up, and the gray outline
//! that follows the pointer while a bar is carried. The layout they work
//! from is in `dockbars.rs`.
use super::*;
use crate::dockbars::{BarGeo, Drag, Drop, Geo, RowGeo, Side, Target, MENU_BAR, MIN_FLOAT_WIDTH};
use crate::toolbars::Chevron;
use chrome::pal;

/// Height of a floating bar's title strip.
const FLOAT_TITLE: f32 = 16.;
/// The 1 px frame around a floating bar.
const FLOAT_BORDER: f32 = 1.;
/// Space between a floating bar's frame and its buttons.
const FLOAT_PAD: f32 = 2.;
/// Width of the strip along a floating bar's right edge that resizes it.
const FLOAT_GRAB: f32 = 5.;
/// The outline drawn while a bar is carried.
const DRAG_GRAY: Color32 = Color32::from_rgb(0x80, 0x80, 0x80);
/// Width of the menu bar's titles side by side.
const MENU_NATURAL: f32 = 337.;
/// Pitch of stacked toolbar buttons.
const PITCH: f32 = chrome::TOOL_PITCH;
/// Width (and, along a column, height) of a bar's chevron button.
const CHEVRON_WIDTH: f32 = 11.;

/// How a bar is laid out where it is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Dock(Side),
    Float,
}

impl App {
    /// Remembers how wide toolbar button `action` draws, for sizing bars.
    pub(crate) fn note_button_width(&mut self, action: &str, width: f32) {
        let widths = &mut self.views.bars.rt.button_width;
        if widths.get(action) != Some(&width) {
            widths.insert(action.to_string(), width);
        }
    }

    /// Is bar `name` shown? The menu bar always is.
    pub(crate) fn bar_visible(&self, name: &str) -> bool {
        name == MENU_BAR || self.toolbar_visible(name)
    }

    /// Start of a frame: the geometry of the last one is dropped and the
    /// buttons of the directory and edit bars are listed.
    pub(crate) fn begin_bars(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let status = if self.views.show_status_bar { 18. } else { 0. };
        let client = Rect::from_min_max(
            screen.min,
            Pos2::new(screen.right(), (screen.bottom() - status).max(screen.top())),
        );
        let specs = self.dock_specs();
        let locked = self.views.toolbars_locked;
        let rt = &mut self.views.bars.rt;
        rt.geo = Geo::new(client);
        rt.specs = specs;
        rt.clicked = None;
        rt.hot = None;
        if locked {
            rt.drag = None;
        }
    }

    /// View > Toolbars > Reset Toolbars: every bar back at its default
    /// place, none floating.
    pub(crate) fn reset_toolbar_layout(&mut self) {
        self.views.bars.layout = Default::default();
        self.views.bars.rt.drag = None;
    }

    // ------------------------------------------------------------- sizes

    /// The buttons of bar `name` that are shown, with their widths.
    fn shown_widths(&self, name: &str) -> Vec<f32> {
        let widths = &self.views.bars.rt.button_width;
        crate::toolbars::TOOLBAR_BUTTONS
            .iter()
            .filter(|(bar, _)| *bar == name)
            .flat_map(|(_, buttons)| buttons.iter())
            .filter(|(action, _)| self.button_visible(action))
            .map(|(action, _)| widths.get(*action).copied().unwrap_or(PITCH))
            .collect()
    }

    /// Width of bar `name`'s buttons side by side in one row.
    fn natural_width(&self, name: &str) -> f32 {
        if name == MENU_BAR {
            return MENU_NATURAL;
        }
        self.shown_widths(name).iter().sum()
    }

    /// Thickness of a column holding bar `name` in a left or right dock.
    fn bar_thickness(&self, name: &str) -> f32 {
        if name == MENU_BAR {
            return chrome::TITLE_COLUMN_WIDTH + 4.;
        }
        let measured = self.views.bars.rt.thickness.get(name).copied();
        let widest = self.shown_widths(name).into_iter().fold(PITCH, f32::max);
        measured.unwrap_or(widest + 4.).max(widest + 4.)
    }

    /// Length of bar `name` along a dock, grip and chevron included.
    fn bar_length(&self, name: &str, vertical: bool) -> f32 {
        if !vertical {
            return chrome::BAR_LEAD + self.natural_width(name) + chrome::BAR_TAIL;
        }
        let buttons = if name == MENU_BAR {
            8.
        } else {
            self.shown_widths(name).len() as f32
        };
        let each = if name == MENU_BAR { 21. } else { PITCH };
        chrome::BAR_LEAD + buttons * each + 1. + chrome::DOCK_BAR_TAIL
    }

    /// The size of bar `name`'s floating window at button width `content`.
    fn float_size(&self, name: &str, content: f32) -> Vec2 {
        let natural = self.natural_width(name).max(1.);
        let rows = (natural / content.max(1.)).ceil().max(1.);
        let chrome = 2. * (FLOAT_BORDER + FLOAT_PAD);
        Vec2::new(
            content + chrome,
            2. * FLOAT_BORDER + FLOAT_TITLE + 2. * FLOAT_PAD + rows * PITCH - 1.,
        )
    }

    /// The widest button of bar `name` (the narrowest a floating bar goes).
    fn widest_button(&self, name: &str) -> f32 {
        if name == MENU_BAR {
            return chrome::TITLE_COLUMN_WIDTH;
        }
        self.shown_widths(name)
            .into_iter()
            .fold(MIN_FLOAT_WIDTH, f32::max)
    }

    /// The button width a floating bar `name` really gets.
    fn float_content_width(&self, name: &str, wanted: f32) -> f32 {
        let natural = self.natural_width(name).max(self.widest_button(name));
        wanted.clamp(self.widest_button(name), natural)
    }

    // ------------------------------------------------------------- docks

    /// The rows of `side` that have a bar to show: (layout row, [(index in
    /// the row, bar)]).
    fn visible_rows(&self, side: Side) -> Vec<(usize, Vec<(usize, String)>)> {
        self.views
            .bars
            .layout
            .rows(side)
            .iter()
            .enumerate()
            .filter_map(|(row, bars)| {
                let shown: Vec<(usize, String)> = bars
                    .iter()
                    .enumerate()
                    .filter(|(_, bar)| self.bar_visible(bar))
                    .map(|(index, bar)| (index, bar.clone()))
                    .collect();
                (!shown.is_empty()).then_some((row, shown))
            })
            .collect()
    }

    /// Height of a row (width of a column) holding `bars`.
    fn row_size(&self, side: Side, bars: &[(usize, String)]) -> f32 {
        if side.vertical() {
            return bars
                .iter()
                .map(|(_, bar)| self.bar_thickness(bar))
                .fold(crate::views::DOCK_WIDTH, f32::max);
        }
        if bars.iter().all(|(_, bar)| bar == MENU_BAR) {
            menu::MENU_ROW
        } else {
            menu::TOOLBAR_ROW
        }
    }

    /// One dock: a panel along the window edge holding the rows of bars the
    /// layout puts there. Nothing is drawn when no bar there is shown.
    pub(crate) fn edge_dock(&mut self, ctx: &egui::Context, side: Side) {
        let rows = self.visible_rows(side);
        if rows.is_empty() {
            return;
        }
        let sizes: Vec<f32> = rows
            .iter()
            .map(|(_, bars)| self.row_size(side, bars))
            .collect();
        let total: f32 = sizes.iter().sum();
        let frame = egui::Frame::NONE.fill(Color32::WHITE);
        match side {
            Side::Top => {
                egui::TopBottomPanel::top("menu_and_toolbar")
                    .exact_height(total)
                    .show_separator_line(false)
                    .frame(frame)
                    .show(ctx, |ui| self.dock_contents(ui, ctx, side, &rows, &sizes));
            }
            Side::Bottom => {
                egui::TopBottomPanel::bottom("bottom_dock")
                    .exact_height(total)
                    .show_separator_line(false)
                    .frame(frame)
                    .show(ctx, |ui| self.dock_contents(ui, ctx, side, &rows, &sizes));
            }
            Side::Left => {
                egui::SidePanel::left("left_dock")
                    .exact_width(total)
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(frame)
                    .show(ctx, |ui| self.dock_contents(ui, ctx, side, &rows, &sizes));
            }
            Side::Right => {
                egui::SidePanel::right("right_dock")
                    .exact_width(total)
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(frame)
                    .show(ctx, |ui| self.dock_contents(ui, ctx, side, &rows, &sizes));
            }
        }
    }

    fn dock_contents(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        side: Side,
        rows: &[(usize, Vec<(usize, String)>)],
        sizes: &[f32],
    ) {
        let dock = ui.max_rect();
        self.toolbar_context_menu(ui, dock, side.key());
        self.views.bars.rt.geo.dock[side.index()] = Some(dock);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let mut at = 0.;
        for (k, (row, bars)) in rows.iter().enumerate() {
            let size = sizes[k];
            let rect = if side.vertical() {
                Rect::from_min_max(
                    Pos2::new(dock.left() + at, dock.top()),
                    Pos2::new(dock.left() + at + size, dock.bottom()),
                )
            } else {
                Rect::from_min_max(
                    Pos2::new(dock.left(), dock.top() + at),
                    Pos2::new(dock.right(), dock.top() + at + size),
                )
            };
            at += size;
            let p = ui.painter();
            match side {
                Side::Top => chrome::hline(
                    p,
                    rect.left(),
                    rect.right(),
                    rect.bottom() - 1.,
                    pal::SHADOW,
                ),
                Side::Bottom => {
                    chrome::hline(p, rect.left(), rect.right(), rect.top(), pal::SHADOW)
                }
                Side::Left => {
                    chrome::vline(p, rect.right() - 1., rect.top(), rect.bottom(), pal::SHADOW)
                }
                Side::Right if k > 0 => {
                    chrome::vline(p, rect.left(), rect.top(), rect.bottom(), pal::SHADOW)
                }
                Side::Right => {}
            }
            let geo = if side.vertical() {
                self.dock_column(ui, ctx, side, rect, bars)
            } else {
                self.dock_row(ui, ctx, side, *row, rect, bars)
            };
            self.views.bars.rt.geo.rows[side.index()].push(RowGeo {
                row: *row,
                rect,
                bars: geo,
            });
        }
        if side == Side::Left {
            chrome::hline(
                ui.painter(),
                dock.left(),
                dock.right() - 1.,
                dock.bottom() - 1.,
                pal::CAPTION_LINE,
            );
        }
    }

    /// A row of a top or bottom dock: its bars side by side.
    fn dock_row(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        side: Side,
        row: usize,
        rect: Rect,
        bars: &[(usize, String)],
    ) -> Vec<BarGeo> {
        let mut content = rect;
        match side {
            Side::Top => content.max.y -= 1.,
            _ => content.min.y += 1.,
        }
        let menu_only = bars.iter().all(|(_, bar)| bar == MENU_BAR);
        let mut geos = vec![];
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(content)
                .layout(Layout::left_to_right(Align::Min)),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                if side == Side::Top && row == 2 {
                    ui.add_space(1.);
                }
                for (index, name) in bars {
                    geos.push(self.dock_hbar(ui, ctx, side, name, *index, menu_only, content));
                }
            },
        );
        geos
    }

    /// One bar in a row: grip, buttons, then the customize chevron.
    #[allow(clippy::too_many_arguments)]
    fn dock_hbar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        side: Side,
        name: &str,
        index: usize,
        menu_only: bool,
        row: Rect,
    ) -> BarGeo {
        let start = ui.cursor().min;
        ui.add_space(chrome::BAR_LEAD);
        let hit = Rect::from_min_size(start, Vec2::new(chrome::BAR_LEAD, row.height()));
        let length = if menu_only { 18. } else { 22. };
        self.bar_grip(ui, name, hit, (start + Vec2::new(4., 3.), length, false));
        ui.vertical(|ui| {
            ui.add_space(2.);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                self.bar_contents(ui, ctx, name, Flow::Dock(side));
            });
        });
        let end = ui.cursor().min.x;
        ui.add_space(chrome::BAR_TAIL);
        let (glyph, hit) = if menu_only {
            // The menu bar's customize glyph sits at the far right.
            (
                Pos2::new(row.right() - 10., start.y + 16.),
                Rect::from_min_size(
                    Pos2::new(row.right() - 13., start.y + 2.),
                    Vec2::new(CHEVRON_WIDTH, 21.),
                ),
            )
        } else {
            (
                Pos2::new(end + 3., start.y + 19.),
                Rect::from_min_size(
                    Pos2::new(end, start.y + 2.),
                    Vec2::new(CHEVRON_WIDTH, chrome::TOOL_HEIGHT),
                ),
            )
        };
        let align = if side == Side::Bottom {
            egui::RectAlign::TOP_START
        } else {
            egui::RectAlign::BOTTOM_START
        };
        self.bar_chevron(
            ui,
            Chevron {
                bar: name,
                key: "bar_chevron",
                hit,
                glyph,
                down: true,
                align,
            },
        );
        let right = if menu_only {
            row.right()
        } else {
            end + chrome::BAR_TAIL
        };
        BarGeo {
            name: name.into(),
            index,
            rect: Rect::from_min_max(start, Pos2::new(right, row.bottom())),
        }
    }

    /// A column of a left or right dock: its bars one under another.
    fn dock_column(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        side: Side,
        rect: Rect,
        bars: &[(usize, String)],
    ) -> Vec<BarGeo> {
        let mut content = rect;
        if side == Side::Left {
            content.max.x -= 1.;
        }
        let mut geos = vec![];
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(content)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (index, name) in bars {
                    geos.push(self.dock_vbar(ui, ctx, side, name, *index, content));
                }
            },
        );
        geos
    }

    /// One bar in a column: grip, buttons stacked, then the chevron.
    fn dock_vbar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        side: Side,
        name: &str,
        index: usize,
        column: Rect,
    ) -> BarGeo {
        let start = ui.cursor().min;
        ui.add_space(chrome::BAR_LEAD);
        let hit = Rect::from_min_size(start, Vec2::new(column.width(), chrome::BAR_LEAD));
        self.bar_grip(ui, name, hit, (start + Vec2::new(3., 4.), 22., true));
        let body = Rect::from_min_size(
            Pos2::new(column.left() + 2., ui.cursor().min.y),
            Vec2::new(column.width() - 2., 10_000.),
        );
        let used = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(body)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0., PITCH - chrome::TOOL_HEIGHT);
                    self.bar_contents(ui, ctx, name, Flow::Dock(side));
                },
            )
            .response
            .rect;
        let thick = used.width() + 4.;
        let known = self.views.bars.rt.thickness.get(name).copied();
        if known != Some(thick) {
            self.views.bars.rt.thickness.insert(name.to_string(), thick);
            ctx.request_repaint();
        }
        let height = if used.height() > 0. {
            used.height() + 1.
        } else {
            0.
        };
        let end = start.y + chrome::BAR_LEAD + height;
        let align = if side == Side::Right {
            egui::RectAlign::LEFT_START
        } else {
            egui::RectAlign::RIGHT_START
        };
        self.bar_chevron(
            ui,
            Chevron {
                bar: name,
                key: "bar_chevron",
                hit: Rect::from_min_size(
                    Pos2::new(column.left() + 1., end),
                    Vec2::splat(CHEVRON_WIDTH),
                ),
                glyph: Pos2::new(column.left() + 5., end + 3.),
                down: false,
                align,
            },
        );
        let next = end + chrome::DOCK_BAR_TAIL;
        let gap = next - ui.cursor().min.y;
        if gap > 0. {
            ui.add_space(gap);
        }
        BarGeo {
            name: name.into(),
            index,
            rect: Rect::from_min_max(start, Pos2::new(column.right(), next)),
        }
    }

    /// The content of bar `name`: the menu titles or the buttons.
    fn bar_contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, name: &str, flow: Flow) {
        if name != MENU_BAR {
            self.bar_buttons(ui, ctx, name);
            return;
        }
        let (align, width) = match flow {
            Flow::Dock(Side::Bottom) => (egui::RectAlign::TOP_START, None),
            Flow::Dock(Side::Left) => (
                egui::RectAlign::RIGHT_START,
                Some(chrome::TITLE_COLUMN_WIDTH),
            ),
            Flow::Dock(Side::Right) => (
                egui::RectAlign::LEFT_START,
                Some(chrome::TITLE_COLUMN_WIDTH),
            ),
            _ => (egui::RectAlign::BOTTOM_START, None),
        };
        self.menu_bar(ui, ctx, chrome::TitleStyle { align, width });
    }

    // -------------------------------------------------------------- grips

    /// The grip of a docked bar: its dots (unless the bars are locked) and
    /// the area that picks the bar up. `hit` is the grip's area, which starts
    /// at the bar's corner; `dots` is (first dot, length, along x).
    fn bar_grip(&mut self, ui: &mut egui::Ui, name: &str, hit: Rect, dots: (Pos2, f32, bool)) {
        if self.views.toolbars_locked {
            return;
        }
        chrome::grip(ui.painter(), dots.0, dots.1, dots.2);
        let response = ui.interact(
            hit,
            egui::Id::new(("bar_grip", name)),
            Sense::click_and_drag(),
        );
        self.action(&format!("bar_grip_{name}"), &response);
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
            self.views.bars.rt.hot = Some(name.to_string());
        }
        if response.double_clicked() {
            // Float the bar where it stands.
            let width = self.float_content_width(name, f32::MAX);
            let at = hit.min - Vec2::new(0., FLOAT_TITLE + FLOAT_BORDER);
            self.views.bars.layout.float(name, at, width);
            self.views.bars.rt.drag = None;
        } else if response.drag_started() {
            let press = ui
                .input(|i| i.pointer.press_origin())
                .unwrap_or(hit.center());
            self.views.bars.rt.drag = Some(Drag {
                name: name.to_string(),
                from_float: false,
                grab: press - hit.min,
                target: None,
                outline: None,
            });
        }
    }

    // ----------------------------------------------------------- floating

    /// The floating bars: small tool windows above the main window.
    pub(crate) fn floating_bars(&mut self, ctx: &egui::Context) {
        let floating: Vec<(String, crate::dockbars::Floater)> = self
            .views
            .bars
            .layout
            .floating
            .iter()
            .filter(|(name, _)| self.bar_visible(name))
            .map(|(name, f)| (name.clone(), f.clone()))
            .collect();
        for (name, floater) in floating {
            self.floating_bar(ctx, &name, &floater);
        }
    }

    fn floating_bar(
        &mut self,
        ctx: &egui::Context,
        name: &str,
        floater: &crate::dockbars::Floater,
    ) {
        let locked = self.views.toolbars_locked;
        let content = self.float_content_width(name, floater.width);
        let size = self.float_size(name, content);
        let client = self.views.bars.rt.geo.client;
        let pos = clamp_into(client, floater.pos, size);
        let mut action = FloatAction::None;
        let shown = egui::Area::new(egui::Id::new(("dock_float", name)))
            .order(egui::Order::Middle)
            .fixed_pos(pos)
            .interactable(true)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                self.float_window(ui, name, pos, content, locked, &mut action)
            });
        let window = shown.inner;
        self.views
            .bars
            .rt
            .geo
            .floats
            .insert(name.to_string(), window);
        match action {
            FloatAction::None => {}
            FloatAction::Close => self.toggle_toolbar(name),
            FloatAction::Redock => self.views.bars.layout.redock(name),
            FloatAction::Width(width) => {
                if let Some(f) = self.views.bars.layout.floating.get_mut(name) {
                    f.width = width;
                }
            }
        }
        if let Some(f) = self.views.bars.layout.floating.get_mut(name) {
            if (f.pos - pos).length() > 0.5 {
                f.pos = pos;
            }
        }
    }

    /// Draws one floating bar's frame, title strip and buttons; returns its
    /// rectangle.
    fn float_window(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        pos: Pos2,
        content: f32,
        locked: bool,
        action: &mut FloatAction,
    ) -> Rect {
        let ctx = ui.ctx().clone();
        let frame_shape = ui.painter().add(egui::Shape::Noop);
        let width = content + 2. * (FLOAT_BORDER + FLOAT_PAD);
        // Title strip.
        let title = Rect::from_min_size(
            pos + Vec2::splat(FLOAT_BORDER),
            Vec2::new(width - 2. * FLOAT_BORDER, FLOAT_TITLE),
        );
        let title_response = ui.interact(
            title,
            egui::Id::new(("float_title", name)),
            if locked {
                Sense::hover()
            } else {
                Sense::click_and_drag()
            },
        );
        self.action(&format!("float_title_{name}"), &title_response);
        let painter = ui.painter().clone();
        chrome::fill(&painter, title, pal::HOT_FILL);
        // The options chevron, left of the close mark when there is one.
        let closable = name != MENU_BAR && !locked;
        let chevron = Rect::from_min_size(
            Pos2::new(
                title.right() - if closable { 28. } else { 14. },
                title.top() + 2.,
            ),
            Vec2::splat(12.),
        );
        let caption = Rect::from_min_max(
            Pos2::new(title.left() + 2., title.top()),
            Pos2::new(chevron.left() - 1., title.bottom()),
        );
        painter.with_clip_rect(caption).text(
            Pos2::new(title.left() + 4., title.center().y),
            Align2::LEFT_CENTER,
            name,
            chrome::ui_font(),
            INK,
        );
        self.bar_chevron(
            ui,
            Chevron {
                bar: name,
                key: "float_chevron",
                hit: chevron,
                glyph: chevron.min + Vec2::new(4., 5.),
                down: true,
                align: egui::RectAlign::BOTTOM_START,
            },
        );
        // Close: not for the menu bar, not while the bars are locked.
        if closable {
            let close = Rect::from_min_size(
                Pos2::new(title.right() - 14., title.top() + 2.),
                Vec2::splat(12.),
            );
            let response = ui.interact(close, egui::Id::new(("float_close", name)), Sense::click());
            self.action(&format!("float_close_{name}"), &response);
            paint_close(&painter, close, response.hovered());
            if response.on_hover_text("Close").clicked() {
                *action = FloatAction::Close;
            }
        }
        if !locked {
            if title_response.hovered() || title_response.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::Move);
            }
            if title_response.double_clicked() {
                *action = FloatAction::Redock;
                self.views.bars.rt.drag = None;
            } else if title_response.drag_started() {
                let press = ui
                    .input(|i| i.pointer.press_origin())
                    .unwrap_or(title.center());
                self.views.bars.rt.drag = Some(Drag {
                    name: name.to_string(),
                    from_float: true,
                    grab: press - pos,
                    target: None,
                    outline: None,
                });
            }
        }
        // Buttons, wrapped to the window's width.
        let body = Rect::from_min_size(
            Pos2::new(pos.x + FLOAT_BORDER + FLOAT_PAD, title.bottom() + FLOAT_PAD),
            Vec2::new(content, 4000.),
        );
        let used = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(body)
                    .layout(Layout::left_to_right(Align::Min).with_main_wrap(true)),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0., PITCH - chrome::TOOL_HEIGHT);
                    self.bar_contents(ui, &ctx, name, Flow::Float);
                },
            )
            .response
            .rect;
        let bottom = used.bottom().max(body.top() + chrome::TOOL_HEIGHT) + FLOAT_PAD + FLOAT_BORDER;
        let window = Rect::from_min_max(pos, Pos2::new(pos.x + width, bottom));
        ui.allocate_rect(window, Sense::hover());
        // The white body goes under the title and the buttons.
        painter.set(
            frame_shape,
            egui::Shape::rect_filled(window, 0., Color32::WHITE),
        );
        painter.rect_stroke(
            window,
            0.,
            Stroke::new(FLOAT_BORDER, pal::SHADOW),
            egui::StrokeKind::Inside,
        );
        // The right edge resizes: more or fewer buttons per row.
        if !locked {
            let grab = Rect::from_min_max(
                Pos2::new(window.right() - FLOAT_GRAB, title.bottom()),
                window.max,
            );
            let response = ui.interact(grab, egui::Id::new(("float_resize", name)), Sense::drag());
            self.action(&format!("float_resize_{name}"), &response);
            if response.hovered() || response.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if response.dragged() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    let wanted = pointer.x - body.left();
                    *action = FloatAction::Width(self.float_content_width(name, wanted));
                }
            }
        }
        window
    }

    // ------------------------------------------------------------ dragging

    /// Where a bar carried by the pointer at `pos` would land.
    fn drag_target(&self, drag: &Drag, pos: Pos2, ctrl: bool) -> Target {
        let bars = &self.views.bars;
        if !ctrl {
            if let Some(drop) = bars.rt.geo.drop_at(&bars.layout, &drag.name, pos) {
                return Target::Dock(drop);
            }
        }
        let content = if drag.from_float {
            let width = bars
                .layout
                .floating
                .get(&drag.name)
                .map_or(f32::MAX, |f| f.width);
            self.float_content_width(&drag.name, width)
        } else {
            self.float_content_width(&drag.name, f32::MAX)
        };
        let size = self.float_size(&drag.name, content);
        let corner = if drag.from_float {
            pos - drag.grab
        } else {
            // The pointer holds the title strip near where it held the grip.
            let x = drag.grab.x.clamp(8., (size.x - 20.).max(8.));
            pos - Vec2::new(x, FLOAT_TITLE / 2. + FLOAT_BORDER)
        };
        Target::Float(clamp_into(bars.rt.geo.client, corner, size))
    }

    /// The gray outline of a target.
    fn drag_outline(&self, drag: &Drag, target: Target) -> Rect {
        let geo = &self.views.bars.rt.geo;
        match target {
            Target::Float(corner) => {
                let width = self
                    .views
                    .bars
                    .layout
                    .floating
                    .get(&drag.name)
                    .map_or(f32::MAX, |f| f.width);
                let content = self.float_content_width(&drag.name, width);
                Rect::from_min_size(corner, self.float_size(&drag.name, content))
            }
            Target::Dock(drop) => {
                let side = match drop {
                    Drop::Row(slot) => slot.side,
                    Drop::NewRow { side, .. } => side,
                };
                let vertical = side.vertical();
                let length = self.bar_length(&drag.name, vertical);
                let size = if vertical {
                    Vec2::new(self.bar_thickness(&drag.name), length)
                } else {
                    Vec2::new(length, menu::TOOLBAR_ROW)
                };
                geo.outline(drop, &drag.name, size)
            }
        }
    }

    /// Carries a bar with the pointer while it is down: the gray outline of
    /// where it will land (a dock place near an edge, else a floating
    /// window), and on release the move itself. Ctrl keeps a bar from
    /// docking; Escape puts it back.
    pub(crate) fn bar_drag(&mut self, ctx: &egui::Context) {
        let Some(mut drag) = self.views.bars.rt.drag.take() else {
            return;
        };
        if self.views.toolbars_locked {
            return;
        }
        let (pos, down, ctrl, escape) = ctx.input(|i| {
            (
                i.pointer.latest_pos(),
                i.pointer.primary_down(),
                i.modifiers.ctrl,
                i.key_pressed(Key::Escape),
            )
        });
        let Some(pos) = pos.filter(|_| !escape) else {
            return;
        };
        ctx.set_cursor_icon(egui::CursorIcon::Move);
        let target = self.drag_target(&drag, pos, ctrl);
        let outline = self.drag_outline(&drag, target);
        drag.target = Some(target);
        drag.outline = Some(outline);
        if down {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("bar_drag_outline"),
            ));
            painter.rect_stroke(
                outline,
                0.,
                Stroke::new(3.0_f32, DRAG_GRAY),
                egui::StrokeKind::Inside,
            );
            self.views.bars.rt.drag = Some(drag);
            ctx.request_repaint();
            return;
        }
        let width = self.float_content_width(&drag.name, f32::MAX);
        let layout = &mut self.views.bars.layout;
        match target {
            Target::Dock(drop) => layout.apply(&drag.name, drop),
            Target::Float(corner) if drag.from_float => {
                if let Some(f) = layout.floating.get_mut(&drag.name) {
                    f.pos = corner;
                }
            }
            Target::Float(corner) => layout.float(&drag.name, corner, width),
        }
        ctx.request_repaint();
    }
}

/// What a floating bar's window asked for this frame.
enum FloatAction {
    None,
    Close,
    Redock,
    Width(f32),
}

/// `pos` moved so a window of `size` stays inside `area` when it fits.
fn clamp_into(area: Rect, pos: Pos2, size: Vec2) -> Pos2 {
    Pos2::new(
        pos.x.min(area.right() - size.x).max(area.left()),
        pos.y.min(area.bottom() - size.y).max(area.top()),
    )
}

/// The close mark of a floating bar: a small cross that turns red when hot.
fn paint_close(p: &egui::Painter, rect: Rect, hot: bool) {
    let c = rect.center();
    let ink = if hot { pal::CLOSE_RED } else { INK };
    if hot {
        p.rect_filled(rect, 0., Color32::from_rgb(0xFB, 0xDD, 0xDD));
    }
    for (a, b) in [((-3., -3.), (3., 3.)), ((-3., 3.), (3., -3.))] {
        p.line_segment(
            [c + Vec2::new(a.0, a.1), c + Vec2::new(b.0, b.1)],
            Stroke::new(1.2_f32, ink),
        );
    }
}

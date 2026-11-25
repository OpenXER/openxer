//! The flow-layout dialog window of the dialog kit. Windows whose controls
//! are laid out by egui (lists, forms, confirmations, help) get the frame of
//! the fixed-layout dialogs, painted by `dialog_kit::frame_shapes`: drop
//! shadow, 1 px border, square corners, a 31 px title bar dragged to move
//! the window, the close box, and a `#F0F0F0` body. Nothing is left of
//! egui's own window chrome. Also the kit's one message box.
use super::*;
use crate::dialog_kit::BUTTON;
use egui::epaint::Shape;
use egui::{Response, Ui};

pub(crate) use crate::dialog_kit::TITLE_H;
const CLOSE_W: f32 = 46.;

fn title_font() -> FontId {
    FontId::new(12., egui::FontFamily::Name("Segoe UI".into()))
}

/// Builder for a kit-styled `egui::Window`. It forwards the sizing and
/// placement calls the dialogs use and draws the frame itself.
pub struct KitWindow<'a> {
    title: String,
    id: egui::Id,
    inner: egui::Window<'static>,
    open: Option<&'a mut bool>,
    margin: i8,
    widgets: bool,
    resizable: bool,
    order: egui::Order,
    /// The caller chose where the window first appears.
    placed: bool,
    /// Fixed in place (anchored): the title bar does not move it.
    anchored: bool,
    /// Where `pos` of [`KitWindow::movable_at`] puts the window this frame.
    at: Option<Pos2>,
    /// The point of the window egui places (its pivot).
    pivot: Align2,
}

macro_rules! forward {
    ($(#[$doc:meta])* $name:ident($($arg:ident: $ty:ty),*)) => {
        $(#[$doc])*
        pub fn $name(mut self, $($arg: $ty),*) -> Self {
            self.inner = self.inner.$name($($arg),*);
            self
        }
    };
}

impl<'a> KitWindow<'a> {
    /// A window whose body has no margin: the caller lays out its own
    /// panels and button column to the edge.
    pub fn flush(title: impl AsRef<str>) -> Self {
        Self::new(title.as_ref(), 0, false)
    }

    /// A window with an 8 px margin around its body and kit-styled
    /// push buttons.
    pub fn padded(title: impl AsRef<str>) -> Self {
        Self::new(title.as_ref(), 8, true)
    }

    fn new(title: &str, margin: i8, widgets: bool) -> Self {
        Self {
            title: title.to_owned(),
            id: egui::Id::new(title),
            inner: egui::Window::new(title.to_owned()),
            open: None,
            margin,
            widgets,
            resizable: true,
            order: egui::Order::Middle,
            placed: false,
            anchored: false,
            at: None,
            pivot: Align2::LEFT_TOP,
        }
    }

    pub fn id(mut self, id: egui::Id) -> Self {
        self.id = id;
        self
    }

    /// The window shows while `*open` is true; its close box sets it false.
    pub fn open(mut self, open: &'a mut bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Kept for source compatibility: kit windows never collapse.
    pub fn collapsible(self, _collapsible: bool) -> Self {
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self.inner = self.inner.resizable(resizable);
        self
    }

    pub fn order(mut self, order: egui::Order) -> Self {
        self.order = order;
        self.inner = self.inner.order(order);
        self
    }

    pub fn anchor(mut self, align: Align2, offset: impl Into<Vec2>) -> Self {
        self.placed = true;
        self.anchored = true;
        self.inner = self.inner.anchor(align, offset);
        self
    }

    pub fn default_pos(mut self, pos: impl Into<Pos2>) -> Self {
        self.placed = true;
        self.inner = self.inner.default_pos(pos);
        self
    }

    /// A window the user moves, placed at `pos` this frame when given
    /// (where it last was otherwise).
    pub fn movable_at(mut self, pos: Option<Pos2>) -> Self {
        self.placed = true;
        self.at = pos;
        self
    }

    forward!(default_size(size: impl Into<Vec2>));
    forward!(min_width(width: f32));

    pub fn show<R>(
        self,
        ctx: &egui::Context,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        let Self {
            title,
            id,
            inner,
            open,
            margin,
            widgets,
            resizable,
            order,
            placed,
            anchored,
            at,
            pivot,
        } = self;
        if open.as_deref() == Some(&false) {
            return None;
        }
        // A window nobody placed opens centered on the screen, as dialogs do.
        let (inner, pivot) = if placed {
            (inner, pivot)
        } else {
            (
                inner
                    .pivot(Align2::CENTER_CENTER)
                    .default_pos(ctx.content_rect().center()),
                Align2::CENTER_CENTER,
            )
        };
        // The title bar moves the window (as the fixed-layout shell does);
        // egui's own background drag does not.
        let drag_id = id.with("kit_drag");
        let inner = inner.movable(false);
        let inner = match at {
            Some(pos) => {
                ctx.data_mut(|d| d.remove::<Pos2>(drag_id));
                inner.current_pos(pos)
            }
            None => match ctx.data(|d| d.get_temp::<Pos2>(drag_id)) {
                Some(pos) if !anchored => inner.current_pos(pos),
                _ => inner,
            },
        };
        let mut closed = false;
        let escape_open = open.is_some() && escape_goes_to(ctx, id, order);
        let stretched = if resizable {
            ctx.memory(|m| m.area_rect(id)).map(|r| r.size())
        } else {
            None
        };
        let response = inner
            .id(id)
            .title_bar(false)
            .collapsible(false)
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                if widgets {
                    kit_widgets(ui);
                }
                let slot = ui.painter().add(Shape::Noop);
                let title_width = ui
                    .painter()
                    .layout_no_wrap(title.clone(), title_font(), INK)
                    .size()
                    .x;
                let space = ui.spacing().item_spacing;
                let column = ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.set_min_width(title_width + CLOSE_W + 24.);
                    // The title bar runs from 1 px in to 1 + TITLE_H.
                    ui.add_space(TITLE_H + 1.);
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::same(margin))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = space;
                            add(ui)
                        })
                        .inner
                });
                let mut full = column.response.rect;
                if let Some(size) = stretched {
                    // A window the user stretched is larger than its content.
                    full.max = full.max.max(full.min + size);
                }
                let (bar, close) = crate::dialog_kit::frame_rects(full);
                let close_response = ui.interact(close, id.with("kit_close"), Sense::click());
                closed = close_response.clicked();
                let shapes = crate::dialog_kit::frame_shapes(
                    ui.painter(),
                    full,
                    &title,
                    close_response.hovered(),
                );
                ui.painter().set(slot, Shape::Vec(shapes));
                crate::dialog_kit::note_framed(ui);
                if !anchored {
                    let grip = Rect::from_min_max(bar.min, Pos2::new(close.left(), bar.bottom()));
                    let drag = ui.interact(grip, id.with("kit_title"), Sense::drag());
                    if drag.dragged() {
                        let moved = full.translate(drag.drag_delta());
                        let to = pivot.pos_in_rect(&moved);
                        ui.ctx().data_mut(|d| d.insert_temp(drag_id, to));
                        ui.ctx().request_repaint();
                    }
                }
                crate::dialog_kit::note_client(
                    ui.ctx(),
                    ui.layer_id(),
                    &title,
                    Rect::from_min_max(
                        Pos2::new(full.left() + 1., bar.bottom()),
                        Pos2::new(full.right() - 1., full.bottom() - 1.),
                    ),
                );
                column.inner
            });
        if closed || escape_open {
            match open {
                Some(open) => *open = false,
                None => request_escape(ctx),
            }
        }
        response
    }
}

/// Whether Escape was pressed and the layer `id` of the given order is the
/// one it belongs to: the topmost visible layer of its order, with no
/// foreground layer (message box, Help, drop-down list) above a window. A
/// layer drawn for the first frame does not count as visible yet, so it
/// leaves the key to the one beneath.
pub(crate) fn escape_goes_to(ctx: &egui::Context, id: egui::Id, order: egui::Order) -> bool {
    if !ctx.input(|i| i.key_pressed(Key::Escape)) {
        return false;
    }
    // An open drop-down list takes Escape for itself.
    if combo_open(ctx) {
        return false;
    }
    ctx.memory(|m| {
        let top = |order: egui::Order| {
            m.layer_ids()
                .filter(|l| l.order == order && m.areas().is_visible(l))
                .last()
        };
        let foreground_above =
            order != egui::Order::Foreground && top(egui::Order::Foreground).is_some();
        !foreground_above && top(order).is_none_or(|l| l.id == id)
    })
}

fn combo_pass_id() -> egui::Id {
    egui::Id::new("kit_combo_open_pass")
}

/// A dialog drop-down list is open this pass (it draws above the dialog in
/// a layer of its own, so the layer order does not show it).
pub(crate) fn note_combo_open(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(combo_pass_id(), pass));
}

/// A dialog drop-down list was open in the pass before this one (or is in
/// this one): Escape and Enter belong to it.
pub(crate) fn combo_open(ctx: &egui::Context) -> bool {
    let now = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<u64>(combo_pass_id()))
        .is_some_and(|pass| pass + 1 >= now)
}

/// Square flat push buttons with the dialog palette.
fn kit_widgets(ui: &mut Ui) {
    let visuals = ui.visuals_mut();
    for (state, fill, border) in [
        (
            &mut visuals.widgets.inactive,
            chrome::pal::FACE,
            chrome::pal::SHADOW,
        ),
        (
            &mut visuals.widgets.hovered,
            chrome::pal::HOT_FILL,
            chrome::pal::HOT_BORDER,
        ),
        (
            &mut visuals.widgets.active,
            chrome::pal::PRESSED_FILL,
            chrome::pal::PRESSED_BORDER,
        ),
    ] {
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1_f32, border);
        state.expansion = 0.;
    }
}

/// A close box on a window without an `open` flag stands for Escape, which
/// every dialog already treats as Cancel. The key is delivered at the start
/// of the next frame (see [`deliver_close_requests`]).
fn request_escape(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("kit_escape_request"), true));
    ctx.request_repaint();
}

/// Turns a pending close-box click into an Escape key press. Call it first
/// in the frame, before any dialog reads its input.
pub(crate) fn deliver_close_requests(ctx: &egui::Context) {
    let pending = ctx.data_mut(|d| {
        d.remove_temp::<bool>(egui::Id::new("kit_escape_request"))
            .unwrap_or(false)
    });
    if pending {
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            });
        });
    }
}

/// The picture on a message box.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MessageIcon {
    Question,
    Error,
    Info,
}

/// Every question the app asks before it acts, with the button Enter
/// presses (the one that has the keyboard when the box opens). This table
/// is the one place a confirmation's default is decided: the message box
/// callers and the few confirmations drawn outside it read it here.
///
/// The rule: the default follows the behaviour observed in the reference
/// scheduler, even where that is Yes on a destructive action; where no
/// behaviour was observed, No (or the answer that keeps the user's data).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Question {
    /// Edit > Delete on activities. Observed: No.
    DeleteActivities,
    /// Edit > Dissolve. Observed: Yes.
    Dissolve,
    /// Delete a baseline. Observed: Yes.
    DeleteBaseline,
    /// Restore a baseline as a project of its own. Observed: Yes.
    RestoreBaseline,
    /// Revert the project to a baseline. Not observed: No.
    RevertToBaseline,
    /// Delete a calendar. Observed: No.
    DeleteCalendar,
    /// Throw away the edits in the calendar editor. Not observed: No.
    DiscardCalendarChanges,
    /// Delete several rows of a records view (resources and the like).
    /// Observed for resources: No.
    DeleteRecords,
    /// Delete a project. Observed: No.
    DeleteProject,
    /// A delete in an enterprise dictionary (EPS, OBS, roles, curves and
    /// the rest). Observed for an empty EPS node: No, the last button.
    EnterpriseDelete,
    /// A delete in a project dictionary (codes, cost accounts, fields,
    /// step templates). Not observed: the last button (No or Cancel).
    DictionaryDelete,
    /// Save the changes to the layout before another opens. Observed: Yes.
    SaveLayoutChanges,
    /// Apply Actuals up to a data date after today. Observed: Yes.
    FutureActuals,
    /// A relationship change that puts an activity out of sequence.
    /// Observed: Yes.
    OutOfSequence,
    /// WBS Delete or Merge on an element with contents. Observed: the
    /// delete choice, which leads to the second question.
    WbsMergeOrDelete,
    /// The question that deletes WBS elements. Observed: Yes.
    WbsDelete,
    /// Undo Last Import. Not observed: Cancel.
    UndoImport,
    /// The project is already in the workspace. Not observed: Open Copy.
    ReplaceCopy,
    /// Unsaved changes found at start. Not observed: Recover.
    RecoverChanges,
    /// Saving failed while closing. Not observed: Try Again.
    SaveFailed,
    /// Open an attached document with another program. Enter is idle.
    OpenDocument,
    /// Close Update Baseline after changing its options: keep them?
    /// Observed: Yes.
    SaveUpdateBaselineSettings,
    /// A typed Start or a dropped bar: hold the activity there with a Start
    /// On or After constraint? Observed for a typed Start (Enter adds the
    /// constraint): Yes. A dropped bar asks with the same box.
    StartHold,
    /// Delete the selected line in Modify Global Change. Observed (on the
    /// filter line delete, the same question): Yes.
    GlobalChangeDeleteLine,
    /// Remove a relationship from the Predecessors, Successors or
    /// Relationships tab. Observed (Predecessors tab): Yes.
    RemoveRelationship,
    /// Delete a layout in Open Layout. Observed: Yes.
    DeleteLayout,
    /// Delete a filter in Filters. Observed: Yes.
    DeleteFilter,
    /// Remove a resource assignment on the Resources tab. Observed: No.
    RemoveAssignment,
    /// The second question when the assignment is the activity's primary
    /// resource. Observed: No.
    RemovePrimaryResource,
    /// Remove an activity code assignment. Observed (in the code assign
    /// window): Yes.
    RemoveCode,
    /// Schedule with several open projects whose scheduling options differ:
    /// use the Default Project's options for all? Observed: Yes.
    ConflictingScheduleOptions,
    /// Chat: sign in to ChatGPT, which sends schedule data to OpenAI? Our
    /// own question: Cancel, the answer that sends nothing.
    ChatSignIn,
    /// Chat: send the first message to an API key service on another
    /// computer? Our own question: Cancel, the answer that sends nothing.
    ChatSendToService,
    /// Chat: the one-time ChatGPT plan notice. Got it.
    ChatPlanWelcome,
    /// Close Schedule Options after a change: save the changes? Observed:
    /// Yes.
    SaveScheduleOptions,
    /// Clear (or another curve) on an assignment with a Manual curve:
    /// remove its typed period values? Seen with Yes and No; the default
    /// was not observed: No.
    RemoveManualCurve,
    /// Default on Schedule Options: reset every scheduling option?
    /// Observed: Yes.
    ResetScheduleOptions,
    /// Cancel on Duplicates during a paste: go back and resolve the
    /// duplicate IDs (No pastes nothing)? Observed: Yes.
    ResolveDuplicates,
    /// Tick Cost Units Linked again on an assignment: recalculate its costs
    /// from its units? Not observed: No.
    RelinkCosts,
    /// Delete in the Resource Assignments window. Observed: No.
    DeleteResourceAssignment,
    /// Delete in the Edit Relationship dialog. Not observed there; the
    /// same question on the Relationships tab was: Yes.
    DeleteRelationship,
    /// Remove a project code from the project (Project Details Codes tab,
    /// Assign Project Codes). Not observed for a project; the same question
    /// for an activity code was: Yes.
    RemoveProjectCode,
}

impl Question {
    /// Every question, for the tests.
    #[cfg(test)]
    pub(crate) const ALL: [Question; 42] = [
        Question::DeleteActivities,
        Question::Dissolve,
        Question::DeleteBaseline,
        Question::RestoreBaseline,
        Question::RevertToBaseline,
        Question::DeleteCalendar,
        Question::DiscardCalendarChanges,
        Question::DeleteRecords,
        Question::DeleteProject,
        Question::EnterpriseDelete,
        Question::DictionaryDelete,
        Question::SaveLayoutChanges,
        Question::FutureActuals,
        Question::OutOfSequence,
        Question::WbsMergeOrDelete,
        Question::WbsDelete,
        Question::UndoImport,
        Question::ReplaceCopy,
        Question::RecoverChanges,
        Question::SaveFailed,
        Question::OpenDocument,
        Question::SaveUpdateBaselineSettings,
        Question::StartHold,
        Question::GlobalChangeDeleteLine,
        Question::RemoveRelationship,
        Question::DeleteLayout,
        Question::DeleteFilter,
        Question::RemoveAssignment,
        Question::RemovePrimaryResource,
        Question::RemoveCode,
        Question::ConflictingScheduleOptions,
        Question::ChatSignIn,
        Question::ChatSendToService,
        Question::ChatPlanWelcome,
        Question::SaveScheduleOptions,
        Question::RemoveManualCurve,
        Question::ResetScheduleOptions,
        Question::ResolveDuplicates,
        Question::RelinkCosts,
        Question::DeleteResourceAssignment,
        Question::DeleteRelationship,
        Question::RemoveProjectCode,
    ];

    /// The caption of the button Enter presses, or `None` when Enter is
    /// idle. `Last` stands for the last button of a box whose buttons vary.
    pub(crate) const fn default_caption(self) -> Option<&'static str> {
        use Question::*;
        match self {
            DeleteActivities
            | RevertToBaseline
            | DeleteCalendar
            | DiscardCalendarChanges
            | DeleteRecords
            | DeleteProject
            | RemoveAssignment
            | RemovePrimaryResource
            | RemoveManualCurve
            | RelinkCosts
            | DeleteResourceAssignment => Some("No"),
            Dissolve
            | StartHold
            | DeleteBaseline
            | RestoreBaseline
            | SaveLayoutChanges
            | FutureActuals
            | OutOfSequence
            | SaveUpdateBaselineSettings
            | GlobalChangeDeleteLine
            | RemoveRelationship
            | DeleteLayout
            | DeleteFilter
            | RemoveCode
            | RemoveProjectCode
            | ConflictingScheduleOptions
            | SaveScheduleOptions
            | ResetScheduleOptions
            | ResolveDuplicates
            | DeleteRelationship => Some("Yes"),
            EnterpriseDelete | DictionaryDelete => Some(LAST),
            WbsMergeOrDelete => Some("Delete All"),
            WbsDelete => Some("Delete"),
            UndoImport | ChatSignIn | ChatSendToService => Some("Cancel"),
            ChatPlanWelcome => Some("Got it"),
            ReplaceCopy => Some("Open Copy"),
            RecoverChanges => Some("Recover"),
            SaveFailed => Some("Try Again"),
            OpenDocument => None,
        }
    }

    /// The index in `buttons` of the button Enter presses: the argument a
    /// message box takes.
    pub(crate) fn default_in(self, buttons: &[&str]) -> Option<usize> {
        let caption = self.default_caption()?;
        let found = if caption == LAST {
            buttons.len().checked_sub(1)
        } else {
            buttons.iter().position(|b| b.replace('&', "") == caption)
        };
        debug_assert!(found.is_some(), "{self:?}: no {caption:?} in {buttons:?}");
        found
    }

    /// Whether the button captioned `caption` is the default, for a
    /// confirmation drawn outside the message box.
    pub(crate) fn is_default(self, caption: &str) -> bool {
        self.default_caption() == Some(caption)
    }
}

/// [`Question::default_caption`] for "the last button".
const LAST: &str = "\u{0}last";

/// The kit's one message box: icon at the left, the wrapped text, and
/// standard 108 x 25 buttons, 10 px apart, centered under them. Returns the index of the clicked
/// button. `rects` collects each button's rectangle under `<id>_<caption>`.
///
/// `default` is the button Enter presses, framed as the default. Every
/// caller names it, so Enter never falls on a choice by its position. A
/// question takes it from [`Question::default_in`], the one table of
/// confirmation defaults; a notice with only OK names OK. `None` leaves
/// Enter idle, for a question that must be answered with a click.
#[allow(clippy::too_many_arguments)]
pub(crate) fn message_box(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    icon: MessageIcon,
    text: &str,
    buttons: &[&str],
    default: Option<usize>,
    rects: &mut Vec<(String, Rect)>,
) -> Option<usize> {
    message_box_with_lines(ctx, id, title, icon, text, &[], buttons, default, rects)
}

/// A message box with a scrolling list of detail lines under the text.
#[allow(clippy::too_many_arguments)]
pub(crate) fn message_box_with_lines(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    icon: MessageIcon,
    text: &str,
    lines: &[String],
    buttons: &[&str],
    default: Option<usize>,
    rects: &mut Vec<(String, Rect)>,
) -> Option<usize> {
    debug_assert!(
        default.is_none_or(|k| k < buttons.len()),
        "message box {id}: default {default:?} names no button"
    );
    let mut clicked = None;
    KitWindow::padded(title)
        .id(egui::Id::new(("kit_message", id)))
        .order(egui::Order::Foreground)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            if default.is_none() {
                crate::dialog_keys::mark_enter_idle(ui);
            }
            let row = buttons.len() as f32 * BUTTON.x + (buttons.len() as f32 - 1.) * 10.;
            ui.set_width(if lines.is_empty() { 360_f32 } else { 460. }.max(row));
            ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(32.), Sense::hover());
                paint_message_icon(ui.painter(), r, icon);
                ui.add_space(10.);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(text).wrap());
                });
            });
            if !lines.is_empty() {
                ui.add_space(6.);
                // The detail lines start under the first line, past the icon.
                ui.horizontal_top(|ui| {
                    ui.add_space(42.);
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("kit_message_lines", id))
                            .max_height(220.)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for line in lines {
                                    ui.add(egui::Label::new(line).wrap());
                                }
                            });
                    });
                });
            }
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - row) / 2.).max(0.));
                ui.spacing_mut().item_spacing.x = 10.;
                for (k, caption) in buttons.iter().enumerate() {
                    let (rect, response) = ui.allocate_exact_size(BUTTON, Sense::click());
                    let named = default == Some(k);
                    paint_plain_button(ui, rect, &response, caption, named);
                    crate::dialog_keys::note_button(ui, &response, caption, true);
                    if named {
                        crate::dialog_keys::mark_default(ui, &response);
                    }
                    rects.push((format!("{id}_{}", caption.to_lowercase()), rect));
                    if response.clicked() {
                        clicked = Some(k);
                    }
                }
            });
        });
    clicked
}

/// A raised push button without an icon, as the message boxes draw them:
/// the kit's push button painter. `default` frames it as the button Enter
/// presses.
pub(crate) fn paint_plain_button(
    ui: &Ui,
    rect: Rect,
    response: &Response,
    caption: &str,
    default: bool,
) {
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, caption));
    crate::dialog_kit::paint_push_button(
        ui.painter(),
        rect,
        caption,
        None,
        crate::dialog_kit::Push::of(response, true, default),
    );
}

/// OpenXER's own message box pictures: a blue question mark, a red cross
/// and a blue letter i, each 32 px.
pub(crate) fn paint_message_icon(painter: &egui::Painter, r: Rect, icon: MessageIcon) {
    let c = r.center();
    let bold = FontId::new(20., egui::FontFamily::Name("Arial Bold".into()));
    match icon {
        MessageIcon::Question | MessageIcon::Info => {
            painter.circle_filled(c, 15., Color32::from_rgb(0x1F, 0x5F, 0xBF));
            painter.text(
                c,
                Align2::CENTER_CENTER,
                if icon == MessageIcon::Question {
                    "?"
                } else {
                    "i"
                },
                bold,
                Color32::WHITE,
            );
        }
        MessageIcon::Error => {
            painter.circle_filled(c, 15., Color32::from_rgb(0xC4, 0x2B, 0x1C));
            for (a, b) in [
                (Vec2::new(-6., -6.), Vec2::new(6., 6.)),
                (Vec2::new(-6., 6.), Vec2::new(6., -6.)),
            ] {
                painter.line_segment([c + a, c + b], Stroke::new(3_f32, Color32::WHITE));
            }
        }
    }
}

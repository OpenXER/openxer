//! The one implementation of the Assign dialogs: Assign Resources, Roles and
//! Resources by Role; Assign Predecessors and Successors; Assign Activity
//! Codes, Notebook Topic, Work Products and Documents, and Risks. Each is a
//! window with its button column (Close, the dialog's own buttons, Help, as
//! observed in the reference) around a list of choices. In the list a click selects one choice,
//! Ctrl+click adds or drops one and Shift+click selects the run from the
//! last plain or Ctrl click, as in the activity table; Assign takes every
//! selected choice at once. Also the Add Code Value dialog that New opens
//! from Assign Activity Codes.
use super::*;
use crate::chrome::DialogButton;
use crate::dialog_kit::{self as kit, Entry};

/// One button of an Assign dialog's column, before Close and Help.
pub(crate) struct AssignButton {
    pub key: &'static str,
    pub caption: &'static str,
    pub enabled: bool,
}

/// What an Assign dialog shows besides its body.
pub(crate) struct AssignWindow<'a> {
    /// The window id, also the key of its list's selection.
    pub id: &'static str,
    pub title: &'a str,
    pub buttons: Vec<AssignButton>,
    /// Action keys of Close and Help.
    pub close: &'static str,
    pub help: &'static str,
}

/// What a click in the list of choices did.
#[derive(Default)]
pub(crate) struct ListOut {
    /// The line clicked (a band line too).
    pub clicked: Option<usize>,
    /// The line double-clicked, and its choice when it is one.
    pub double_row: Option<usize>,
    pub double: Option<i64>,
}

/// The Add Code Value dialog while open: a new value for code type
/// `type_id`, typed in place. `project`: a project code (Assign Project
/// Codes), else an activity code.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewCode {
    pub type_id: i64,
    pub value: String,
    pub description: String,
    pub project: bool,
}

impl App {
    /// Draw an Assign dialog around `body`. Returns whether it stays open
    /// (Close, the close box and Escape close it) and the key of the button
    /// of `w.buttons` clicked. Help opens the dialog's help topic.
    pub(super) fn assign_window(
        &mut self,
        ctx: &egui::Context,
        w: &AssignWindow,
        body: impl FnOnce(&mut App, &mut egui::Ui),
    ) -> (bool, Option<&'static str>) {
        // As observed in the reference: Close first, then the dialog's own
        // verb (Assign) and its other buttons, Help last. Enter presses
        // Assign, or Close while nothing can be assigned.
        let mut keys: Vec<&'static str> = vec![w.close];
        let mut buttons: Vec<DialogButton> = vec![DialogButton::new("Close", None)];
        for b in &w.buttons {
            keys.push(b.key);
            buttons.push(DialogButton::new(b.caption, None).enabled(b.enabled));
        }
        keys.push(w.help);
        buttons.push(DialogButton::help());
        let verb = w.buttons.first().filter(|b| b.enabled).map(|_| 1);
        let mut open = true;
        let mut clicked = None;
        crate::chrome::dialog_window(w.title)
            .id(egui::Id::new(w.id))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                let (_, button, responses) =
                    crate::chrome::dialog_with_buttons(ui, &buttons, |ui| body(self, ui));
                for (key, r) in keys.iter().zip(&responses) {
                    self.action(key, r);
                }
                if let Some(r) = verb.and_then(|k| responses.get(k)) {
                    crate::dialog_keys::mark_default(ui, r);
                }
                clicked = button.map(|k| keys[k]);
            });
        if clicked == Some(w.help) {
            self.open_help_for(w.id);
        }
        // Escape belongs to the Add Code Value dialog or a Remove question
        // while one is open.
        let escape = self.detail.new_code.is_none()
            && self.detail.removal.is_none()
            && ctx.input(|i| i.key_pressed(Key::Escape));
        let open = open && clicked != Some(w.close) && !escape;
        if !open {
            self.detail.assign_rows.remove(w.id);
            self.detail.replace_from = None;
        }
        let clicked = clicked.filter(|k| *k != w.close && *k != w.help);
        (open, clicked)
    }

    /// The list of choices of Assign dialog `dialog`: one line per entry of
    /// `keys` (`None`: a band line, which is not a choice), `selected` the
    /// current choice. `paint` draws line `r`, column `c` (`sel`: painted as
    /// selected).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assign_list(
        &mut self,
        ui: &mut egui::Ui,
        dialog: &'static str,
        grid_id: &str,
        cols: &[(&str, f32)],
        keys: &[Option<i64>],
        selected: &mut Option<i64>,
        size: Vec2,
        paint: &mut dyn FnMut(&mut egui::Ui, usize, usize, bool),
    ) -> ListOut {
        self.assign_list_with_menu(
            ui,
            dialog,
            grid_id,
            cols,
            keys,
            selected,
            size,
            paint,
            &mut |_, _| {},
        )
    }

    /// [`Self::assign_list`] with a right-click context menu handler (`row` is
    /// the clicked line, or `None` for the empty area).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assign_list_with_menu(
        &mut self,
        ui: &mut egui::Ui,
        dialog: &'static str,
        grid_id: &str,
        cols: &[(&str, f32)],
        keys: &[Option<i64>],
        selected: &mut Option<i64>,
        size: Vec2,
        paint: &mut dyn FnMut(&mut egui::Ui, usize, usize, bool),
        menu: &mut dyn FnMut(&mut egui::Ui, Option<usize>),
    ) -> ListOut {
        let rows = self.detail.assign_rows.entry(dialog).or_default();
        let shown = rows.shown(selected, keys);
        let out = grid_marked(
            ui,
            grid_id,
            cols,
            keys.len(),
            shown.current,
            &shown.rows,
            size,
            &mut |ui, ev, sel| match ev {
                Ev::Cell(r, c) => paint(ui, r, c, sel),
                Ev::Menu(row) => menu(ui, row),
            },
        );
        if let Some(Some(key)) = out.clicked.and_then(|r| keys.get(r)) {
            let rows = self.detail.assign_rows.entry(dialog).or_default();
            *selected = Some(rows.click(selected.as_ref(), *key, out.modifiers, false));
        }
        ListOut {
            clicked: out.clicked,
            double_row: out.double,
            double: out.double.and_then(|r| keys.get(r).copied().flatten()),
        }
    }

    /// The choices Assign takes in dialog `dialog`, in list order: every
    /// selected one, or the current one alone.
    pub(super) fn assign_picked(&self, dialog: &'static str, selected: Option<i64>) -> Vec<i64> {
        match self.detail.assign_rows.get(dialog) {
            Some(rows) => rows.picked(selected.as_ref()),
            None => selected.into_iter().collect(),
        }
    }

    /// Assign Activity Codes, New: ask for a value of code type `type_id`.
    pub(super) fn open_new_code(&mut self, type_id: i64) {
        self.detail.new_code = Some(NewCode {
            type_id,
            ..Default::default()
        });
    }

    /// Add a value with `value` and `description` to activity code type
    /// (project code type when `project`) `type_id`, at the top level after
    /// the others, as one undo step. Returns its id. Nothing is assigned.
    pub(crate) fn add_code_value_from_assign(
        &mut self,
        project: bool,
        type_id: i64,
        value: &str,
        description: &str,
        time: f64,
    ) -> Result<i64, String> {
        use crate::dictionaries::ops::{self, CodeKind, ValueField};
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let doc = &mut self.project.document;
        let kind = if project {
            CodeKind::Project
        } else {
            CodeKind::Activity
        };
        let made = ops::add_code_value(doc, kind, type_id, None);
        let made = made.and_then(|id| {
            ops::set_code_value(doc, kind, type_id, id, ValueField::Short, value)?;
            ops::set_code_value(doc, kind, type_id, id, ValueField::Name, description)?;
            Ok(id)
        });
        match made {
            Ok(id) => {
                self.edit_step(before, before_revision, time);
                Ok(id)
            }
            Err(e) => {
                self.project = before;
                self.records_revision = before_revision;
                Err(e)
            }
        }
    }

    /// The Add Code Value dialog: a code value and its description; OK
    /// (once a value is typed) adds it to the code type Assign Activity
    /// Codes shows and selects it there, without assigning it.
    pub(super) fn new_code_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.detail.new_code.take() else {
            return;
        };
        const W: f32 = 549.;
        const H: f32 = 143.;
        let mut done = None;
        let mut help = false;
        let ((), closed, actions) = kit::window(
            ctx,
            "add_code_value",
            "Add Code Value",
            Vec2::new(W, H),
            |c| {
                kit::strip(c, W, H);
                kit::text(c, 10., 10., "Type a value and its description.", true);
                let left = W - kit::STRIP_WIDTH - 20.;
                kit::text(c, 10., 36., "Code Value", true);
                c.edit(
                    "new_code_value",
                    10.,
                    51.,
                    120.,
                    21.,
                    &mut d.value,
                    true,
                    Align::Min,
                );
                kit::text(c, 140., 36., "Code Value Description", true);
                c.edit(
                    "new_code_description",
                    140.,
                    51.,
                    left - 130.,
                    21.,
                    &mut d.description,
                    true,
                    Align::Min,
                );
                let out = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("new_code_ok", "OK", !d.value.trim().is_empty(), false),
                        Entry::cancel("new_code_cancel"),
                        Entry::help("new_code_help"),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("new_code_ok") => done = Some(true),
                    Some("new_code_cancel") => done = Some(false),
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("assign_record");
        }
        if closed || ctx.input(|i| i.key_pressed(Key::Escape)) {
            done = done.or(Some(false));
        }
        match done {
            Some(true) => {
                let time = ctx.input(|i| i.time);
                let (value, text) = (&d.value, &d.description);
                let made = self.add_code_value_from_assign(d.project, d.type_id, value, text, time);
                match made {
                    Ok(id) if d.project => {
                        if let Some(p) = self.detail.notes.code_picker.as_mut() {
                            p.type_id = Some(d.type_id);
                            p.selected = Some(id);
                        }
                        self.status = format!("Code value {} added", d.value.trim());
                    }
                    Ok(id) => {
                        let picker = self.detail.picker.as_mut();
                        if let Some(Picker::Code { type_id, selected }) = picker {
                            *type_id = Some(d.type_id);
                            *selected = Some(id);
                        }
                        self.status = format!("Code value {} added", d.value.trim());
                    }
                    Err(e) => {
                        self.error(e);
                        self.detail.new_code = Some(d);
                    }
                }
            }
            Some(false) => {}
            None => self.detail.new_code = Some(d),
        }
    }
}

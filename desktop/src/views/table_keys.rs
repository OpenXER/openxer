//! Row keys of the view tables built on `grid` (every main view but the
//! Activities window): Ctrl+A and Edit > Select All, Up, Down, PageUp,
//! PageDown, Home and End, with Shift extending the selection from its
//! anchor. Also the edit commands every such view offers the same way
//! (`ViewEdit`): the Edit menu, the Delete and Insert keys and
//! Ctrl+X / Ctrl+C / Ctrl+V run the same command as the view's right-click
//! menu, with the same confirmation. They act on the shown main view; the
//! caller checks that no dialog is up and no text field has the keyboard.
use super::grid::{self, RowSelection, Step};
use super::projects::ProjectRow;
use super::*;

/// An edit command of a view table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewEdit {
    Add,
    Delete,
    Cut,
    Copy,
    Paste,
}

/// What a key asks of a table's selection.
#[derive(Clone, Copy)]
enum Op {
    All,
    Step(Step, bool),
}

fn apply<K: Ord + Clone>(rows: &mut RowSelection<K>, current: Option<&K>, op: Op) -> Option<K> {
    match op {
        Op::All => rows.select_all(current),
        Op::Step(step, extend) => rows.step(current, step, extend),
    }
}

/// A record list: its current record follows the key.
fn list(s: &mut record_view::ListState, op: Op) {
    let current = s.selected;
    if let Some(next) = apply(&mut s.rows, current.as_ref(), op) {
        if current != Some(next) {
            s.sub_row = None;
        }
        s.selected = Some(next);
    }
}

impl App {
    /// The shown main view is a table built on `grid` that takes the row
    /// keys now (none of its own windows is open).
    pub(crate) fn grid_view_keys_ready(&self) -> bool {
        let table = matches!(
            self.views.main,
            MainView::Wbs
                | MainView::Projects
                | MainView::Resources
                | MainView::Reports
                | MainView::Assignments
                | MainView::Expenses
                | MainView::Documents
                | MainView::Risks
                | MainView::Issues
                | MainView::Thresholds
        );
        table
            && !self.record_dialog_open()
            && self.views.resources.merge.is_none()
            && self.views.wbs.pending.is_none()
    }

    /// Ctrl+A or Edit > Select All in a view table: every row shown.
    pub(crate) fn grid_view_select_all(&mut self) {
        self.grid_view_op(Op::All);
    }

    /// The row and edit keys pressed this frame, for the shown view table.
    pub(crate) fn grid_view_keys(&mut self, ctx: &egui::Context) {
        if let Some((step, extend)) = ctx.input(grid::step_keys) {
            self.grid_view_op(Op::Step(step, extend));
        }
        let keys = ctx.input(|i| {
            let command = |key| i.modifiers.command && i.key_pressed(key);
            let event = |want: fn(&egui::Event) -> bool| i.events.iter().any(want);
            [
                (ViewEdit::Add, i.key_pressed(Key::Insert)),
                (ViewEdit::Delete, i.key_pressed(Key::Delete)),
                (
                    ViewEdit::Cut,
                    command(Key::X) || event(|e| matches!(e, egui::Event::Cut)),
                ),
                (
                    ViewEdit::Copy,
                    command(Key::C) || event(|e| matches!(e, egui::Event::Copy)),
                ),
                (
                    ViewEdit::Paste,
                    command(Key::V) || event(|e| matches!(e, egui::Event::Paste(_))),
                ),
            ]
        });
        let time = ctx.input(|i| i.time);
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::E)) && self.view_fill_ready() {
            self.view_fill_down(time);
        }
        for (edit, pressed) in keys {
            if pressed && self.view_edit_enabled(edit) == Some(true) {
                self.view_edit(edit, time);
            }
        }
    }

    /// Whether the shown view table offers `edit` now (`None`: the view has
    /// no such command). The Edit menu, the keys and the right dock follow
    /// it.
    pub(crate) fn view_edit_enabled(&self, edit: ViewEdit) -> Option<bool> {
        use ViewEdit::*;
        let views = &self.views;
        if views.main.is_activities() {
            // The activity table's own commands (`editing::Command`).
            let selected = self.selected.is_some();
            return Some(match edit {
                Add => true,
                Delete | Cut | Copy => selected,
                Paste => self.editing.clipboard.is_some(),
            });
        }
        match views.main {
            MainView::Resources => {
                let (add, delete, copy, paste) = self.resource_commands_enabled();
                Some(match edit {
                    Add => add,
                    Delete => delete,
                    Cut | Copy => copy,
                    Paste => paste,
                })
            }
            MainView::Wbs => {
                let element = views
                    .wbs
                    .selected
                    .is_some_and(|id| !wbs::is_project_node(&self.project, id));
                Some(match edit {
                    Add => true,
                    Delete => self.wbs_can_delete(),
                    Cut | Copy => element,
                    Paste => views.wbs.clipboard.is_some(),
                })
            }
            MainView::Projects => {
                let p = &views.projects;
                // As the view worked them out when it last drew.
                if let Some(flags) = p.edit_enabled {
                    return Some(match edit {
                        Add => flags[0],
                        Delete => flags[1],
                        Cut => flags[2],
                        Copy => flags[3],
                        Paste => flags[4],
                    });
                }
                let selected = p.selected.is_some();
                Some(match edit {
                    Add => true,
                    Delete | Cut | Copy => selected,
                    Paste => p.cut.is_some() || p.copy.is_some(),
                })
            }
            MainView::Reports => {
                // A built-in report cannot be deleted, cut or copied.
                let selected = self
                    .report_selected_entry()
                    .is_some_and(|e| !e.is_builtin());
                Some(match edit {
                    Add => true,
                    Delete | Cut | Copy => selected,
                    Paste => self.reports.clipboard.is_some(),
                })
            }
            MainView::Expenses
            | MainView::Documents
            | MainView::Risks
            | MainView::Issues
            | MainView::Thresholds => {
                let (add, delete) = self.record_edit_commands();
                match edit {
                    Add => Some(add),
                    Delete => Some(delete),
                    Cut | Copy | Paste => None,
                }
            }
            // Resource Assignments: Add to the selected assignment's
            // activity, Delete on the selected assignments.
            MainView::Assignments => match edit {
                Add => Some(self.assignments_add_target().is_some()),
                Delete => Some(!self.assignments_picked().is_empty()),
                Cut | Copy | Paste => None,
            },
            _ => None,
        }
    }

    /// Run `edit` on the shown view table, as its right-click menu does.
    pub(crate) fn view_edit(&mut self, edit: ViewEdit, time: f64) {
        use ViewEdit::*;
        if self.views.main.is_activities() {
            // The same commands Ctrl+X / C / V, Insert and Delete queue in
            // the activity table: Cut and Delete ask before removing.
            use crate::editing::Command;
            self.editing.pending = Some(match edit {
                Add => Command::Add,
                Delete => Command::Delete,
                Cut => Command::Cut,
                Copy => Command::Copy,
                Paste => Command::Paste,
            });
            return;
        }
        match self.views.main {
            MainView::Resources => {
                let command = match edit {
                    Add => "Add",
                    Delete => "Delete",
                    Cut => "Cut",
                    Copy => "Copy",
                    Paste => "Paste",
                };
                self.resource_command(command, time);
            }
            MainView::Wbs => match edit {
                Add => self.wbs_command("Add"),
                Delete => self.wbs_delete_selected(),
                Cut => self.wbs_command("Cut"),
                Copy => self.wbs_command("Copy"),
                Paste => self.wbs_command("Paste"),
            },
            MainView::Projects => {
                self.views.projects.command = Some(match edit {
                    Add => "add",
                    Delete => "delete",
                    Cut => "cut",
                    Copy => "copy",
                    Paste => "paste",
                });
            }
            MainView::Reports => self.reports_command(match edit {
                Add => "add",
                Delete => "delete",
                Cut => "cut",
                Copy => "copy",
                Paste => "paste",
            }),
            MainView::Expenses
            | MainView::Documents
            | MainView::Risks
            | MainView::Issues
            | MainView::Thresholds => match edit {
                Add => self.record_edit_command(true),
                Delete => self.record_edit_command(false),
                Cut | Copy | Paste => {}
            },
            MainView::Assignments => match edit {
                Add => self.assignments_add(),
                Delete => self.assignments_delete(),
                Cut | Copy | Paste => {}
            },
            _ => {}
        }
    }

    fn grid_view_op(&mut self, op: Op) {
        let views = &mut self.views;
        match views.main {
            MainView::Resources => {
                let s = &mut views.resources;
                if let Some(next) = apply(&mut s.rows, s.selected.as_ref(), op) {
                    s.selected = Some(next);
                }
            }
            MainView::Wbs => {
                let s = &mut views.wbs;
                let current = s.selected;
                if let Some(next) = apply(&mut s.rows, Some(&current), op) {
                    if next != current {
                        s.code_edit = None;
                    }
                    s.selected = next;
                }
            }
            MainView::Projects => {
                let s = &mut views.projects;
                let current = match (&s.selected, s.selected_eps) {
                    (Some((source, key)), _) => Some(ProjectRow::Project(source.clone(), *key)),
                    (None, Some(id)) => Some(ProjectRow::Eps(id)),
                    (None, None) => None,
                };
                match apply(&mut s.rows, current.as_ref(), op) {
                    Some(ProjectRow::Eps(id)) => {
                        s.selected = None;
                        s.selected_eps = Some(id);
                    }
                    Some(ProjectRow::Project(source, key)) => s.selected = Some((source, key)),
                    None => {}
                }
            }
            MainView::Assignments => {
                let s = &mut views.assignments;
                if let Some(next) = apply(&mut s.rows, s.selected.as_ref(), op) {
                    s.selected = Some(next);
                }
            }
            MainView::Reports => {
                let s = &mut views.reports;
                if let Some(next) = apply(&mut s.rows, s.selected.as_ref(), op) {
                    s.selected = Some(next);
                }
            }
            MainView::Expenses => list(&mut views.records.expenses.list, op),
            MainView::Documents => list(&mut views.records.documents, op),
            MainView::Risks => list(&mut views.records.risks, op),
            MainView::Issues => list(&mut views.records.issues.list, op),
            MainView::Thresholds => list(&mut views.records.thresholds.list, op),
            _ => {}
        }
    }
}

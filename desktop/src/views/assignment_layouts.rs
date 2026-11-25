//! Named layouts of the Resource Assignments view, saved and opened the way
//! Activities layouts are (the same Open Layout and Save Layout As windows,
//! the same name rules and save-changes question): the columns, details
//! pane, table width, spreadsheet zoom and Show All Projects. Layouts are
//! user-level preferences, never project data.
use super::assignments::AssignmentsState;
use super::*;
use crate::grid::layout;
use serde_json::{json, Value};

/// The view's own layout, always listed.
pub const STANDARD: &str = "Activity Resource Assignments";
/// Preference row holding the saved layouts.
const KEY: &str = "assignment_layouts_v1";

/// One saved layout: its name and the view settings it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct AssignmentLayout {
    pub name: String,
    pub view: Value,
}

/// The stored form: `[{"name": ..., "view": {...}}]`. Entries with a bad
/// name or no settings object are dropped.
pub fn library_from_json(text: &str) -> Vec<AssignmentLayout> {
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(text) else {
        return vec![];
    };
    let mut out: Vec<AssignmentLayout> = vec![];
    for item in list {
        let (Some(name), Some(view)) = (item["name"].as_str(), item.get("view")) else {
            continue;
        };
        let Ok(name) = layout::check_name(name) else {
            continue;
        };
        if view.is_object() && !out.iter().any(|l| l.name.eq_ignore_ascii_case(&name)) {
            out.push(AssignmentLayout {
                name,
                view: view.clone(),
            });
        }
    }
    out
}

pub fn library_to_json(list: &[AssignmentLayout]) -> String {
    Value::Array(
        list.iter()
            .map(|l| json!({"name": l.name, "view": l.view}))
            .collect(),
    )
    .to_string()
}

impl AssignmentsState {
    /// What a layout keeps: the view settings without the layout's name.
    pub fn layout_json(&self) -> Value {
        let mut v = self.json();
        if let Some(map) = v.as_object_mut() {
            map.remove("layout_name");
        }
        v
    }
    /// Puts `view` in place: the view's defaults first, so a setting the
    /// layout does not hold goes back to its default.
    pub fn apply_layout(&mut self, view: &Value) {
        let defaults = AssignmentsState::default().layout_json();
        self.restore(&defaults);
        self.restore(view);
    }
}

impl App {
    pub(crate) fn assignment_layouts(&self) -> Vec<AssignmentLayout> {
        match self.store.preference(KEY) {
            Ok(Some(text)) => library_from_json(&text),
            _ => vec![],
        }
    }

    fn save_assignment_library(&mut self, list: &[AssignmentLayout]) -> Result<(), String> {
        self.store
            .set_preference(KEY, &library_to_json(list))
            .map_err(|e| format!("Cannot save layouts: {e}"))
    }

    /// The names the Open Layout window lists: the view's own layout, then
    /// the saved ones by name.
    pub(crate) fn assignment_layout_names(&self) -> Vec<String> {
        let saved = self.assignment_layouts();
        let mut names = vec![];
        if !saved.iter().any(|l| l.name == STANDARD) {
            names.push(STANDARD.to_string());
        }
        let mut rest: Vec<String> = saved.into_iter().map(|l| l.name).collect();
        rest.sort_by_key(|n| n.to_lowercase());
        names.extend(rest);
        names
    }

    /// Save the view's settings under `name`, replacing a layout of that name.
    pub(crate) fn save_assignment_layout_as(&mut self, name: &str) -> Result<(), String> {
        let name = layout::check_name(name)?;
        let view = self.views.assignments.layout_json();
        let mut list = self.assignment_layouts();
        match list.iter_mut().find(|l| l.name.eq_ignore_ascii_case(&name)) {
            Some(slot) => {
                slot.name = name.clone();
                slot.view = view;
            }
            None => list.push(AssignmentLayout {
                name: name.clone(),
                view,
            }),
        }
        self.save_assignment_library(&list)?;
        self.views.assignments.layout_name = name.clone();
        self.status = format!("Layout {name} saved");
        Ok(())
    }

    /// Open a saved layout (or the view's own one) by name.
    pub(crate) fn open_assignment_layout(&mut self, name: &str) -> Result<(), String> {
        let view = match self
            .assignment_layouts()
            .into_iter()
            .find(|l| l.name == name)
        {
            Some(l) => l.view,
            None if name == STANDARD => AssignmentsState::default().layout_json(),
            None => return Err(format!("No layout named {name}.")),
        };
        let s = &mut self.views.assignments;
        s.apply_layout(&view);
        s.layout_name = name.to_string();
        s.first_minute = None;
        self.status = format!("Layout {name} opened");
        Ok(())
    }

    pub(crate) fn delete_assignment_layout(&mut self, name: &str) -> Result<(), String> {
        let mut list = self.assignment_layouts();
        let before = list.len();
        list.retain(|l| l.name != name);
        if list.len() == before {
            return Err(format!("No saved layout named {name}."));
        }
        self.save_assignment_library(&list)
    }

    /// Does the view differ from the saved copy of its layout?
    pub(crate) fn assignment_layout_changed(&self) -> bool {
        let s = &self.views.assignments;
        let saved = self
            .assignment_layouts()
            .into_iter()
            .find(|l| l.name == s.layout_name)
            .map(|l| l.view)
            .or_else(|| {
                (s.layout_name == STANDARD).then(|| AssignmentsState::default().layout_json())
            });
        saved.is_none_or(|v| {
            let mut probe = AssignmentsState::default();
            probe.apply_layout(&v);
            probe.layout_json() != s.layout_json()
        })
    }

    /// Layout > Open... of the assignments spreadsheet.
    pub(crate) fn open_assignment_layout_dialog(&mut self) {
        self.open_layout_dialog();
        self.grid.dialogs.layout_target = crate::grid::dialogs::LayoutTarget::Assignments;
        if let Some(d) = self.grid.dialogs.open_layout.as_mut() {
            d.selected = Some(self.views.assignments.layout_name.clone());
        }
    }

    /// Layout > Save As... of the assignments spreadsheet.
    pub(crate) fn open_assignment_save_as(&mut self) {
        self.grid.dialogs.save_as = Some(self.views.assignments.layout_name.clone());
        self.grid.dialogs.layout_target = crate::grid::dialogs::LayoutTarget::Assignments;
    }

    /// Layout > Save: under the layout's own name.
    pub(crate) fn save_assignment_layout(&mut self) {
        let name = self.views.assignments.layout_name.clone();
        if let Err(e) = self.save_assignment_layout_as(&name) {
            self.error(e);
        }
    }
}

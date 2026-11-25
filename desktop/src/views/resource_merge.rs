//! Resources view > Del. / Merge: delete the selected resource, or merge it
//! (and any other resources ticked in the list) into a target resource.
//! The dialog previews what moves where before anything changes; applying
//! is one undo step. The edit itself is the shared edit layer's
//! (`openxer_xer::edit::resources`), so the command line and MCP
//! `merge_resources` and `delete_resource` operations behave the same.
use super::resources::{resources, Resource};
use super::*;
use crate::dialog_kit::{self, Col};
use openxer_xer::edit::resources as merge;
use std::collections::BTreeSet;

pub const SALT: &str = "resource_merge";
pub const TITLE: &str = "Delete or Merge Resources";
/// Client size: the list and preview with the standard button column.
const W: f32 = 720.;
const H: f32 = 430.;
/// Lines the preview box shows.
const PREVIEW_LINES: usize = 7;

/// The open Del. / Merge dialog.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeDialog {
    /// The resource the dialog was opened on.
    pub resource: i64,
    /// Merge (true) or delete (false).
    pub merging: bool,
    /// Other resources ticked to merge as well.
    pub also: BTreeSet<i64>,
    /// The resource everything moves to.
    pub target: Option<i64>,
    pub error: Option<String>,
}

impl MergeDialog {
    /// The resources that go away: the opened one and the ticked ones.
    pub fn sources(&self) -> Vec<i64> {
        let mut out = vec![self.resource];
        out.extend(self.also.iter().copied().filter(|&id| id != self.resource));
        out
    }
}

/// Resources of the same type as `rsrc`, other than it, in Resource ID order.
pub fn candidates(p: &ProjectData, rsrc: i64) -> Vec<Resource> {
    let list = resources(p);
    let Some(kind) = list.iter().find(|r| r.id == rsrc).map(|r| r.kind) else {
        return vec![];
    };
    let mut out: Vec<Resource> = list
        .into_iter()
        .filter(|r| r.id != rsrc && r.kind == kind)
        .collect();
    out.sort_by_key(|a| a.code.to_lowercase());
    out
}

/// The likely duplicate to merge into: the first resource of the same type
/// with the same name, else none.
pub fn likely_target(p: &ProjectData, rsrc: i64) -> Option<i64> {
    let name = resources(p)
        .into_iter()
        .find(|r| r.id == rsrc)
        .map(|r| r.name.trim().to_lowercase())?;
    candidates(p, rsrc)
        .into_iter()
        .find(|r| r.name.trim().to_lowercase() == name)
        .map(|r| r.id)
}

/// Splits a preview line at spaces into lines of at most `width` characters.
fn wrap(line: &str, width: usize) -> Vec<String> {
    let mut out = vec![];
    let mut current = String::new();
    for word in line.split(' ') {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    out.push(current);
    out
}

impl App {
    /// Opens Del. / Merge on the selected resource.
    pub(crate) fn open_resource_merge(&mut self) {
        let Some(id) = self
            .views
            .resources
            .selected
            .filter(|id| merge::row(&self.project.document, *id).is_some())
        else {
            self.status = "Select a resource to delete or merge.".into();
            return;
        };
        let assigned = merge::assignment_count(&self.project.document, id) > 0;
        self.views.resources.merge = Some(MergeDialog {
            resource: id,
            merging: assigned,
            target: likely_target(&self.project, id),
            ..Default::default()
        });
    }

    /// Apply the dialog: delete or merge, one undo step.
    fn apply_resource_merge(&mut self, d: &MergeDialog, time: f64) -> Result<String, String> {
        let before = self.project.clone();
        let revision = self.records_revision;
        let code = |app: &Self, id: i64| merge::code(&app.project.document, id);
        let result = if d.merging {
            let target = d.target.ok_or("Pick the resource to merge into.")?;
            let sources = d.sources();
            merge::merge_project(&mut self.project, &sources, target).map(|report| {
                self.views.resources.selected = Some(target);
                let names: Vec<&str> = report.from.iter().map(|(_, c)| c.as_str()).collect();
                format!("Merged {} into {}", names.join(", "), report.into_code)
            })
        } else {
            let name = code(self, d.resource);
            merge::delete(&mut self.project.document, d.resource).map(|()| {
                self.project.refresh_resource_inputs();
                self.views.resources.selected = None;
                format!("Resource {name} deleted")
            })
        };
        match result {
            Ok(status) => {
                self.edit_step(before, revision, time);
                Ok(status)
            }
            Err(e) => {
                self.project = before;
                Err(e)
            }
        }
    }

    /// The Del. / Merge dialog, when open.
    pub(crate) fn resource_merge_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.views.resources.merge.clone() else {
            return;
        };
        let doc = &self.project.document;
        if merge::row(doc, d.resource).is_none() {
            self.views.resources.merge = None;
            return;
        }
        let code = merge::code(doc, d.resource);
        // The lists and the preview are kept while the project, the
        // resources merged and the target are unchanged: the preview merges
        // a copy of the whole file.
        let serial = self.project_serial();
        let name = self
            .usage_resources()
            .iter()
            .find(|r| r.id == d.resource)
            .map(|r| r.name.clone())
            .unwrap_or_default();
        let assigned = merge::assignment_count(doc, d.resource);
        let has_children = doc.table("RSRC").is_some_and(|t| {
            t.rows
                .iter()
                .any(|r| records::num(t.value(r, "parent_rsrc_id")) == d.resource)
        });
        let can_delete = assigned == 0 && !has_children;
        if !can_delete {
            d.merging = true;
        }
        let list = self.memo(serial, "merge_candidates", d.resource, || {
            candidates(&self.project, d.resource)
        });
        d.also.retain(|id| list.iter().any(|r| r.id == *id));
        if d.target.is_some_and(|t| d.also.contains(&t)) {
            d.also.remove(&d.target.unwrap_or(0));
        }
        let preview: Vec<String> = if d.merging {
            match d.target {
                Some(t) => {
                    let preview = self.memo(serial, "merge_preview", (d.sources(), t), || {
                        merge::preview(doc, &d.sources(), t).map(|report| report.lines())
                    });
                    match preview.as_ref() {
                        Ok(lines) => lines.clone(),
                        Err(e) => vec![e.clone()],
                    }
                }
                None => vec!["Pick the resource to merge into in the list (click its row).".into()],
            }
        } else {
            vec![format!(
                "Delete {code} with its rates, roles, codes and field values. It has no assignments."
            )]
        };
        let wrapped: Vec<String> = preview.iter().flat_map(|l| wrap(l, 98)).collect();
        let target_row = d.target.and_then(|t| list.iter().position(|r| r.id == t));
        let ((apply, cancel, help, picked, toggled), closed, actions) =
            dialog_kit::window(ctx, SALT, TITLE, Vec2::new(W, H), |c| {
                c.label(
                    12.,
                    8.,
                    17.,
                    &format!(
                        "Resource {code}  {name}: {assigned} assignment{}",
                        if assigned == 1 { "" } else { "s" }
                    ),
                    true,
                );
                if c.radio(
                    "resource_merge_delete",
                    12.,
                    32.,
                    400.,
                    17.,
                    if can_delete {
                        "Delete the resource"
                    } else {
                        "Delete the resource (it has assignments or child resources)"
                    },
                    !d.merging,
                    can_delete,
                ) {
                    d.merging = false;
                }
                if c.radio(
                    "resource_merge_merge",
                    12.,
                    54.,
                    400.,
                    17.,
                    "Merge it into the resource picked below",
                    d.merging,
                    true,
                ) {
                    d.merging = true;
                }
                let cols = [
                    Col {
                        title: "Merge too",
                        width: 70.,
                    },
                    Col {
                        title: "Resource ID",
                        width: 150.,
                    },
                    Col {
                        title: "Resource Name",
                        width: 212.,
                    },
                    Col {
                        title: "Assignments",
                        width: 72.,
                    },
                ];
                let mut toggled = None;
                let enabled = d.merging;
                let out = dialog_kit::list(
                    c,
                    "resource_merge_list",
                    (12., 78., 506., 190.),
                    None,
                    &cols,
                    list.len(),
                    target_row,
                    Some(1),
                    &mut |c, row, col, rect, selected| {
                        let r = &list[row];
                        let text = match col {
                            0 => {
                                let on = d.also.contains(&r.id);
                                let key = format!("resource_merge_also_{row}");
                                let boxed =
                                    Rect::from_center_size(rect.center(), Vec2::new(13., 13.));
                                // The target row is the one merged into.
                                if selected {
                                    return;
                                }
                                if c.hit(&key, rect, enabled).clicked() {
                                    toggled = Some(r.id);
                                }
                                let at = boxed.min - c.origin;
                                dialog_kit::check_glyph(c, at.x.round(), at.y.round(), on, enabled);
                                return;
                            }
                            1 => r.code.clone(),
                            2 => r.name.clone(),
                            _ => r.assignments.to_string(),
                        };
                        dialog_kit::cell_text(c, rect, &text, selected, 3.);
                    },
                );
                c.label(
                    12.,
                    272.,
                    17.,
                    "Click a row to merge into it; tick Merge too for more duplicates to remove.",
                    enabled,
                );
                c.lowered(12., 294., 590., 128.);
                c.fill(13., 295., 588., 126., Color32::WHITE);
                let mut y = 298.;
                // Seven lines fit; a longer preview says how much is left.
                let room = PREVIEW_LINES - usize::from(d.error.is_some());
                let shown = if wrapped.len() > room {
                    room - 1
                } else {
                    wrapped.len()
                };
                for line in &wrapped[..shown] {
                    c.label(18., y, 17., line, true);
                    y += 17.;
                }
                if wrapped.len() > shown {
                    let more = wrapped.len() - shown;
                    let text = format!("and {more} more line{}", if more == 1 { "" } else { "s" });
                    c.label(18., y, 17., &text, true);
                    y += 17.;
                }
                if let Some(e) = &d.error {
                    c.label(18., y, 17., e, true);
                }
                let label = if d.merging { "Merge" } else { "Delete" };
                dialog_kit::strip(c, W, H);
                let column = dialog_kit::button_column(
                    c,
                    W,
                    &[
                        dialog_kit::Entry::Button(
                            "resource_merge_apply",
                            label,
                            !d.merging || d.target.is_some(),
                            false,
                        ),
                        dialog_kit::Entry::cancel("resource_merge_cancel"),
                        dialog_kit::Entry::help("resource_merge_help"),
                    ],
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                let (apply, cancel, help) = (
                    clicked("resource_merge_apply"),
                    clicked("resource_merge_cancel"),
                    clicked("resource_merge_help"),
                );
                let picked = out
                    .clicked
                    .filter(|_| enabled)
                    .and_then(|i| list.get(i))
                    .map(|r| r.id);
                (apply, cancel, help, picked, toggled)
            });
        self.record_dialog_actions(actions);
        if let Some(id) = toggled {
            if !d.also.remove(&id) {
                d.also.insert(id);
            }
        } else if let Some(id) = picked {
            d.target = Some(id);
            d.also.remove(&id);
        }
        if help {
            self.open_help_for(SALT);
        }
        if apply {
            let time = ctx.input(|i| i.time);
            match self.apply_resource_merge(&d, time) {
                Ok(status) => {
                    self.status = status;
                    self.views.resources.merge = None;
                    return;
                }
                Err(e) => d.error = Some(e),
            }
        }
        if cancel || closed || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.views.resources.merge = None;
        } else {
            self.views.resources.merge = Some(d);
        }
    }
}

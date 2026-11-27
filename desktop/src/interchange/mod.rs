//! File > Import and File > Export wizards, the spreadsheet import preview, and
//! Export to Excel from the activity table and Gantt context menus.
//!
//! The data work lives in the `openxer-interchange` crate; this module is the UI
//! and the workspace hand-off (backup, one-transaction commit, undo last import).
use super::*;
use crate::grid::Row;
use openxer_interchange::columns::{Column, DurationUnit, Lookup};
use openxer_interchange::xer_export::{export_document, XER_VERSIONS};
use openxer_interchange::xer_merge::{add_into_project, merge_project, DataType, ImportConfig};
use openxer_interchange::xlsx_export::{write_modern, write_tables, ExportOptions};
use openxer_interchange::xlsx_import as xi;

#[cfg(test)]
mod dictionary_tests;
#[cfg(test)]
mod safety_tests;
pub(crate) mod sheet_setters;
#[cfg(test)]
mod sheet_tests;
pub(crate) mod templates;
#[cfg(test)]
mod tests;
pub(crate) mod workspace;
use workspace::{WorkspaceRow, Write};

const CHANGED: Color32 = Color32::from_rgb(255, 242, 204);
const NEW_ROW: Color32 = Color32::from_rgb(226, 239, 218);
const ERROR_ROW: Color32 = Color32::from_rgb(255, 199, 206);
const DELETE_ROW: Color32 = Color32::from_rgb(242, 220, 219);
/// Automatic workspace backups taken before imports that stay in `backups`.
const KEEP_BACKUPS: usize = 10;

/// A one-line File Name edit drawn with the standard sunken edit frame, so it
/// reads as editable.
fn file_name_edit(ui: &mut egui::Ui, text: &mut String, width: f32, hint: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 21.), egui::Sense::hover());
    crate::schedule_dialogs::paint_sunken(ui.painter(), rect, Color32::WHITE);
    let mut edit = egui::TextEdit::singleline(text)
        .frame(false)
        .margin(egui::Margin::ZERO)
        .vertical_align(egui::Align::Center)
        .desired_width(width - 6.);
    if !hint.is_empty() {
        edit = edit.hint_text(hint);
    }
    ui.put(rect.shrink2(egui::vec2(3., 2.)), edit);
}

#[derive(Default)]
pub(crate) struct Interchange {
    pub xer: Option<XerImport>,
    pub export: Option<ExportWizard>,
    pub sheet: Option<SheetImport>,
    pub message: Option<Message>,
    /// Undo Last Import's question, while it is asked.
    pub undo_prompt: Option<UndoPrompt>,
}

/// What Undo Last Import would do, asked before it is done.
pub(crate) struct UndoPrompt {
    pub text: String,
    pub lines: Vec<String>,
}

/// Import/export result in the reference style: a short statement, the warnings,
/// and where the backup or file went.
pub(crate) struct Message {
    pub title: String,
    pub text: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportAction {
    CreateNew,
    UpdateExisting,
    ReplaceExisting,
    AddInto,
    Ignore,
}
impl ImportAction {
    const ALL: [ImportAction; 5] = [
        ImportAction::CreateNew,
        ImportAction::UpdateExisting,
        ImportAction::ReplaceExisting,
        ImportAction::AddInto,
        ImportAction::Ignore,
    ];
    fn label(self) -> &'static str {
        match self {
            ImportAction::CreateNew => "Create New Project",
            ImportAction::UpdateExisting => "Update Existing Project",
            ImportAction::ReplaceExisting => "Replace Existing Project",
            ImportAction::AddInto => "Add Into Existing Project",
            ImportAction::Ignore => "Ignore this Project",
        }
    }
    /// Actions that write into a chosen workspace project.
    fn has_target(self) -> bool {
        matches!(
            self,
            ImportAction::UpdateExisting | ImportAction::ReplaceExisting | ImportAction::AddInto
        )
    }
    /// Actions that merge with the chosen project (the configuration page applies).
    fn merges(self) -> bool {
        matches!(self, ImportAction::UpdateExisting | ImportAction::AddInto)
    }
}

pub(crate) struct ProjectRow {
    pub file: usize,
    pub pid: i64,
    pub short: String,
    pub activities: usize,
    pub action: ImportAction,
    pub target: Option<i64>,
}

pub(crate) struct Prepared {
    pub label: String,
    pub write: Option<Write>,
    pub summary: Vec<String>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
    /// What it does to a shared workspace dictionary (the import report).
    pub dictionary: Vec<String>,
}

pub(crate) struct XerImport {
    /// 0 format, 1 file, 2 projects, 3 configuration, 4 summary.
    pub step: usize,
    pub spreadsheet: bool,
    /// The project XML format was chosen on the first page (the file page
    /// then offers XML files; any file is read by its contents).
    pub xml: bool,
    pub path_text: String,
    pub files: Vec<(PathBuf, Document)>,
    pub rows: Vec<ProjectRow>,
    pub config: ImportConfig,
    pub workspace: Vec<WorkspaceRow>,
    pub prepared: Vec<Prepared>,
    pub open_after: bool,
    /// EPS folder the new projects (Create New Project) are filed in.
    pub folder: i64,
    pub error: Option<String>,
    pub last_import: Option<String>,
    /// A workspace with a shared dictionary: per kind of dictionary row
    /// (table), what a row whose name the workspace has with different
    /// content does (a copy unless chosen otherwise).
    pub choices: openxer_automation::workspace_db::Choices,
    /// The kinds of rows the file brings with such a name clash.
    pub clashes: BTreeSet<String>,
    /// The selected row of the Import Project Options list.
    pub selected: usize,
    /// The selected row of the Update Project Options list.
    pub selected_type: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    Xer,
    /// Project XML (`openxer_interchange::project_xml`).
    Xml,
    Modern,
    Tables,
}
impl ExportFormat {
    /// The formats that write whole projects (several at once, baselines).
    fn is_schedule(self) -> bool {
        matches!(self, ExportFormat::Xer | ExportFormat::Xml)
    }
    fn extension(self) -> &'static str {
        match self {
            ExportFormat::Xer => "xer",
            ExportFormat::Xml => "xml",
            ExportFormat::Modern | ExportFormat::Tables => "xlsx",
        }
    }
}

pub(crate) struct ExportWizard {
    /// 0 format, 1 projects, 2 options, 3 file.
    pub step: usize,
    pub format: ExportFormat,
    pub workspace: Vec<WorkspaceRow>,
    pub chosen: Vec<bool>,
    /// Per workspace row: stored baselines, and whether they go with it.
    pub baseline_counts: Vec<usize>,
    pub baselines: Vec<bool>,
    pub current: Option<i64>,
    pub version: String,
    pub unit: DurationUnit,
    pub date_format: String,
    pub group_wbs: bool,
    /// Saved spreadsheet export templates, the one chosen (the table layout
    /// when none) and the editor while it is open.
    pub templates: Vec<openxer_interchange::export_template::ExportTemplate>,
    pub template: Option<usize>,
    pub template_notes: Vec<String>,
    pub editor: Option<templates::TemplateEditor>,
    pub path: String,
    /// The existing file the user agreed to replace (chosen with Browse,
    /// which asks, or Finish clicked again after the warning).
    pub replace_ok: Option<String>,
    pub error: Option<String>,
}

pub(crate) struct SheetImport {
    /// 0 file, 1 columns, 2 preview.
    pub step: usize,
    pub path: String,
    pub wb: Option<xi::Workbook>,
    pub detected: Option<xi::Detected>,
    pub mapping: Option<xi::Mapping>,
    pub plan: Option<xi::ImportPlan>,
    /// 0 all rows, 1 changes only, 2 errors only.
    pub filter: usize,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Nav {
    None,
    Back,
    Next,
    Finish,
    Cancel,
}

fn documents() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Documents"))
        .unwrap_or_else(std::env::temp_dir)
}

pub(crate) fn safe_file_name(s: &str) -> String {
    let name: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_ .".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim().to_string();
    if name.is_empty() {
        "project".into()
    } else {
        name
    }
}

/// The current layout's visible columns that the export format carries, in
/// layout order. Columns the export has no field for are left out.
pub(crate) fn layout_columns(columns: &[crate::grid::Column]) -> Vec<Column> {
    use crate::grid::columns::Source;
    columns
        .iter()
        .filter_map(|c| match c.source {
            Source::Code(id) => Some(Column::ActivityCode(id)),
            Source::Udf(id) => Some(Column::Udf(id)),
            Source::Notebook(_) => None,
            Source::Field(_) => Some(match c.id.as_str() {
                "activity_id" => Column::ActivityId,
                "activity_name" => Column::ActivityName,
                "activity_status" => Column::ActivityStatus,
                "activity_type" => Column::ActivityType,
                "wbs" => Column::WbsCode,
                "wbs_name" => Column::WbsName,
                "calendar" => Column::Calendar,
                "original_duration" => Column::OriginalDuration,
                "remaining_duration" => Column::RemainingDuration,
                "start" => Column::Start,
                "finish" => Column::Finish,
                "actual_start" => Column::ActualStart,
                "actual_finish" => Column::ActualFinish,
                "early_start" => Column::EarlyStart,
                "early_finish" => Column::EarlyFinish,
                "late_start" => Column::LateStart,
                "late_finish" => Column::LateFinish,
                "total_float" => Column::TotalFloat,
                "free_float" => Column::FreeFloat,
                "physical_percent_complete" => Column::PercentComplete,
                "primary_constraint" => Column::PrimaryConstraint,
                "primary_constraint_date" => Column::PrimaryConstraintDate,
                "secondary_constraint" => Column::SecondaryConstraint,
                "secondary_constraint_date" => Column::SecondaryConstraintDate,
                "critical" => Column::Critical,
                _ => return None,
            }),
        })
        .collect()
}

/// A registry column's values for the activities `rows`, as the spreadsheet
/// export writes them: dates as dates, durations in the export's unit,
/// units, costs and percents as numbers.
fn registry_extra(
    p: &ProjectData,
    ix: &crate::grid::columns::Index,
    column: &crate::grid::Column,
    rows: &[usize],
) -> openxer_interchange::columns::Extra {
    use crate::grid::columns::{self as gc, Kind as G};
    use openxer_interchange::columns::{Kind as X, Value as V};
    let origin = p.model.origin_serial * openxer_engine::calendar::DAY;
    let kind = match column.kind {
        G::Text => X::Text,
        G::Date => X::Date,
        G::Duration => X::Duration,
        G::Percent => X::Percent,
        G::Units | G::Cost | G::Number => X::Number,
        G::Flag => X::Bool,
    };
    let values = rows
        .iter()
        .map(|&i| {
            let v = match gc::value(p, ix, column, i) {
                gc::Value::Empty => V::Empty,
                gc::Value::Text(t) => V::Text(t),
                gc::Value::Date { minute, .. } => V::Date(minute + origin),
                gc::Value::Duration(m) => V::Duration(m),
                gc::Value::Percent(n)
                | gc::Value::Units(n)
                | gc::Value::Cost(n)
                | gc::Value::Number(n) => V::Number(n),
                gc::Value::Flag(b) => V::Bool(b),
            };
            (i, v)
        })
        .collect();
    openxer_interchange::columns::Extra {
        title: column.title.clone(),
        kind,
        values,
    }
}

impl App {
    /// Paint a wizard in the reference anatomy: a #D0D0D0 client, the page
    /// panel lowered at 10,10 with a large bold heading and a one-line
    /// instruction, the body under it, Cancel at the bottom left and Prev /
    /// Next / Finish (88 x 25 on a 93 px pitch) at the bottom right.
    #[allow(clippy::too_many_arguments)]
    fn wizard(
        &mut self,
        ctx: &egui::Context,
        salt: &'static str,
        title: &str,
        heading: &str,
        subtitle: &str,
        client: Vec2,
        (back, next, finish): (bool, bool, bool),
        body: impl FnOnce(&mut Self, &mut egui::Ui),
    ) -> Nav {
        use crate::dialog_kit as kit;
        let (w, h) = (client.x, client.y);
        let (clicked, closed, actions) =
            crate::schedule_dialogs::modal(ctx, salt, title, client, |c| {
                kit::wizard_page(c, w, h, h - 35., heading);
                kit::text(c, 16., 57., subtitle, true);
                let body_rect = c.r(16., 75., w - 27., h - 75. - 46.);
                c.ui.scope_builder(egui::UiBuilder::new().max_rect(body_rect), |ui| {
                    ui.set_clip_rect(body_rect);
                    ui.spacing_mut().item_spacing = Vec2::new(6., 5.);
                    body(self, ui);
                });
                // The wizard row: Cancel alone at the left, Prev, Next and
                // the committing Finish at the right.
                match kit::wizard_buttons(c, salt, w - 5., h - 30., (back, next, finish)) {
                    Some(kit::WizardNav::Back) => Nav::Back,
                    Some(kit::WizardNav::Next) => Nav::Next,
                    Some(kit::WizardNav::Finish) => Nav::Finish,
                    Some(kit::WizardNav::Cancel) => Nav::Cancel,
                    None => Nav::None,
                }
            });
        self.record_dialog_actions(actions);
        if closed {
            Nav::Cancel
        } else {
            clicked
        }
    }

    fn current_row(&self, rows: &[WorkspaceRow]) -> Option<i64> {
        rows.iter()
            .find(|r| {
                r.source == self.project.source_name
                    && r.project_key == self.project.model.project.proj_id
            })
            .map(|r| r.id)
    }

    // ------------------------------------------------------------ entry points

    /// File > Import...
    pub(crate) fn open_import_wizard(&mut self) {
        let workspace = workspace::rows(&self.store).unwrap_or_default();
        let last_import = workspace::last_import(&self.store)
            .ok()
            .flatten()
            .map(|(_, n, when)| {
                format!(
                    "{} on {when}",
                    crate::util::count(n, "workspace project", "workspace projects")
                )
            });
        let folder = self.default_import_folder();
        self.interchange.xer = Some(XerImport {
            step: 0,
            spreadsheet: false,
            xml: false,
            path_text: String::new(),
            files: vec![],
            rows: vec![],
            config: ImportConfig::default(),
            workspace,
            prepared: vec![],
            open_after: true,
            folder,
            error: None,
            last_import,
            choices: Default::default(),
            clashes: BTreeSet::new(),
            selected: 0,
            selected_type: 0,
        });
    }

    /// File > Export...
    pub(crate) fn open_export_wizard(&mut self) {
        // Every open project is saved, so the export reads what is shown.
        if self.any_dirty() {
            self.save();
        }
        let workspace = workspace::rows(&self.store).unwrap_or_default();
        let current = self.current_row(&workspace);
        let open = self.open_ids();
        let chosen = workspace
            .iter()
            .map(|r| Some(r.id) == current || open.contains(&r.id))
            .collect();
        let baseline_counts: Vec<usize> = workspace
            .iter()
            .map(|r| crate::baselines::store::count(self.store.connection(), r.id))
            .collect();
        let baselines = vec![false; workspace.len()];
        let (templates, template_notes) = self.saved_export_templates();
        self.interchange.export = Some(ExportWizard {
            step: 0,
            format: ExportFormat::Xer,
            workspace,
            chosen,
            baseline_counts,
            baselines,
            current,
            version: XER_VERSIONS[0].into(),
            unit: DurationUnit::Days,
            date_format: "dd-mmm-yy hh:mm".into(),
            group_wbs: true,
            templates,
            template: None,
            template_notes,
            editor: None,
            path: String::new(),
            replace_ok: None,
            error: None,
        });
    }

    pub(crate) fn open_sheet_import(&mut self, path: String) {
        self.interchange.sheet = Some(SheetImport {
            step: 0,
            path,
            wb: None,
            detected: None,
            mapping: None,
            plan: None,
            filter: 0,
            error: None,
        });
    }

    /// Activities in the table's current order and filter, ignoring collapsed bands.
    fn visible_activities(&mut self) -> Vec<usize> {
        let collapsed = std::mem::take(&mut self.collapsed);
        let rows = self.rows();
        self.collapsed = collapsed;
        rows.into_iter()
            .filter_map(|r| match r {
                Row::Activity(i) => Some(i),
                Row::Group(_) => None,
            })
            .collect()
    }

    fn layout_export_options(&mut self, unit: DurationUnit) -> ExportOptions {
        let shown = self.visible_columns(&crate::grid::columns::catalog(&self.project));
        let rows = self.visible_activities();
        // Columns the spreadsheet format has no field for (earned value,
        // percent completes, counts, notebooks...) go out as the table
        // computes them, under the table's title.
        let ix = self.grid_index();
        let mut extra = vec![];
        let columns = shown
            .iter()
            .map(|c| {
                layout_columns(std::slice::from_ref(c))
                    .pop()
                    .unwrap_or_else(|| {
                        extra.push(registry_extra(&self.project, &ix, c, &rows));
                        Column::Extra((extra.len() - 1) as u16)
                    })
            })
            .collect();
        ExportOptions {
            columns,
            unit,
            rows: Some(rows),
            extra,
            ..Default::default()
        }
    }

    /// Export to Excel (context menus): the visible columns and rows, modern format.
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn export_excel(&mut self) {
        let name = format!("{}.xlsx", safe_file_name(&self.project.model.project.name));
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Excel workbook", &["xlsx"])
            .set_file_name(name)
            .save_file()
        {
            self.export_excel_to(&path);
        }
    }

    pub(crate) fn export_excel_to(&mut self, path: &Path) {
        let options = self.layout_export_options(DurationUnit::Days);
        match write_modern(&self.project, &options, path) {
            Ok(s) => {
                self.status = format!(
                    "Exported {} activities and {} relationships to {}",
                    s.activities,
                    s.relationships,
                    path.display()
                )
            }
            Err(e) => self.error(e),
        }
    }

    // ------------------------------------------------------------ dialogs

    pub(crate) fn interchange_dialogs(&mut self, ctx: &egui::Context) {
        if self.interchange.xer.is_some() {
            self.xer_import_dialog(ctx);
        }
        if self.interchange.sheet.is_some() {
            self.sheet_import_dialog(ctx);
        }
        if self.interchange.export.is_some() {
            self.export_dialog(ctx);
            self.template_editor_dialog(ctx);
        }
        if self.interchange.message.is_some() {
            self.message_dialog(ctx);
        }
        if self.interchange.undo_prompt.is_some() {
            self.undo_prompt_dialog(ctx);
        }
    }

    fn message_dialog(&mut self, ctx: &egui::Context) {
        let Some(m) = self.interchange.message.take() else {
            return;
        };
        let mut rects = vec![];
        let answer = crate::kit_window::message_box_with_lines(
            ctx,
            "interchange_message",
            &m.title,
            crate::kit_window::MessageIcon::Info,
            &m.text,
            &m.lines,
            &["OK"],
            Some(0),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let keep = answer.is_none();
        if keep && !ctx.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Escape)) {
            self.interchange.message = Some(m);
        }
    }

    // ------------------------------------------------------------ XER import

    fn xer_load_files(&mut self) {
        let Some(w) = self.interchange.xer.as_mut() else {
            return;
        };
        w.error = None;
        // Choices made for the projects of the same files are kept when the
        // files are read again (Back to the file page, then Next).
        let kept: Vec<(PathBuf, i64, ImportAction, Option<i64>)> = w
            .rows
            .iter()
            .filter_map(|r| {
                let path = w.files.get(r.file)?.0.clone();
                Some((path, r.pid, r.action, r.target))
            })
            .collect();
        w.files.clear();
        w.rows.clear();
        let paths: Vec<PathBuf> = w
            .path_text
            .split(';')
            .map(|s| s.trim().trim_matches('"'))
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect();
        if paths.is_empty() {
            w.error = Some(if w.xml {
                "Select an XML file to import".into()
            } else {
                "Select an XER file to import".into()
            });
            return;
        }
        for path in paths {
            // XER or project XML, told apart by the contents.
            match openxer_interchange::project_xml::read_file(&path) {
                Ok(doc) => {
                    let file = w.files.len();
                    for (pid, short) in openxer_interchange::baseline::schedule_projects(&doc) {
                        let activities = doc.table("TASK").map_or(0, |t| {
                            t.rows
                                .iter()
                                .filter(|r| {
                                    t.value(r, "proj_id").trim().parse::<i64>().ok() == Some(pid)
                                })
                                .count()
                        });
                        let target = w
                            .workspace
                            .iter()
                            .find(|r| r.project_key == pid && r.title.eq_ignore_ascii_case(&short))
                            .map(|r| r.id);
                        let (action, target) = kept
                            .iter()
                            .find(|k| k.0 == path && k.1 == pid)
                            .map_or((ImportAction::CreateNew, target), |k| (k.2, k.3));
                        w.rows.push(ProjectRow {
                            file,
                            pid,
                            short,
                            activities,
                            action,
                            target,
                        });
                    }
                    w.files.push((path, doc));
                }
                Err(e) => {
                    w.error = Some(format!("{}: {e}", path.display()));
                    return;
                }
            }
        }
        if w.rows.is_empty() {
            w.error = Some("No projects were found in the selected file".into());
        }
    }

    /// Check the Import Action grid; returns an error to show.
    fn xer_validate_rows(w: &XerImport) -> Option<String> {
        if w.rows.iter().all(|r| r.action == ImportAction::Ignore) {
            return Some("Every project is set to Ignore this Project".into());
        }
        let mut targets = BTreeSet::new();
        for r in &w.rows {
            if r.action.has_target() {
                let Some(t) = r.target else {
                    return Some(format!("Choose the Import To project for {}", r.short));
                };
                if !targets.insert(t) {
                    return Some("Conflicting import actions: two projects update or replace the same workspace project".into());
                }
            }
        }
        None
    }

    fn xer_prepare(&mut self) {
        let Some(w) = self.interchange.xer.as_ref() else {
            return;
        };
        let current = self.current_row(&w.workspace);
        let mut prepared = vec![];
        for r in &w.rows {
            let (path, doc) = &w.files[r.file];
            let label = format!(
                "{} ({})",
                r.short,
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            let mut item = Prepared {
                label: format!("{}: {label}", r.action.label()),
                write: None,
                summary: vec![],
                warnings: vec![],
                error: None,
                dictionary: vec![],
            };
            let target_title = |id: Option<i64>| {
                w.workspace
                    .iter()
                    .find(|x| Some(x.id) == id)
                    .map_or(String::new(), |x| x.title.clone())
            };
            let result: Result<(), String> = (|| {
                match r.action {
                    ImportAction::Ignore => item.summary.push("Not imported".into()),
                    ImportAction::CreateNew => {
                        let p = ProjectData::from_document(
                            doc.clone(),
                            r.pid,
                            path.display().to_string(),
                        )?;
                        p.model.validate()?;
                        item.summary.push(format!(
                            "A new workspace project with {} activities and {} relationships",
                            p.model.activities.len(),
                            p.model.links.len()
                        ));
                        item.warnings = p.warnings.clone();
                        item.write = Some(Write::Insert(p));
                    }
                    ImportAction::ReplaceExisting => {
                        let id = r.target.ok_or("No Import To project")?;
                        // The open copy, unsaved changes and all, when the
                        // project is open (active or not).
                        let old = if Some(id) == current {
                            self.project.clone()
                        } else {
                            self.open_or_stored(id)?
                        };
                        let mut p = ProjectData::from_document(
                            doc.clone(),
                            r.pid,
                            old.source_name.clone(),
                        )?;
                        p.model.validate()?;
                        p.model.options.do_level = old.model.options.do_level;
                        item.summary.push(format!(
                            "Replaces {} ({} activities) with {} activities and {} relationships from the file",
                            target_title(Some(id)),
                            old.model.activities.len(),
                            p.model.activities.len(),
                            p.model.links.len()
                        ));
                        item.warnings = p.warnings.clone();
                        item.write = Some(Write::Replace(id, p));
                    }
                    ImportAction::UpdateExisting => {
                        let id = r.target.ok_or("No Import To project")?;
                        // The open copy, unsaved changes and all, when the
                        // project is open (active or not).
                        let old = if Some(id) == current {
                            self.project.clone()
                        } else {
                            self.open_or_stored(id)?
                        };
                        let tpid = old.model.project.proj_id;
                        let (merged, report) =
                            merge_project(&old.updated_document(), tpid, doc, r.pid, &w.config)?;
                        let mut p =
                            ProjectData::from_document(merged, tpid, old.source_name.clone())?;
                        p.document.windows_encoding = old.document.windows_encoding;
                        p.model.options.do_level = old.model.options.do_level;
                        item.summary.push(format!(
                            "Updates {} from {}",
                            target_title(Some(id)),
                            r.short
                        ));
                        item.summary.extend(report.summary_lines());
                        item.warnings = report.messages;
                        item.write = Some(Write::Overwrite(id, p));
                    }
                    ImportAction::AddInto => {
                        let id = r.target.ok_or("No Import To project")?;
                        // The open copy, unsaved changes and all, when the
                        // project is open (active or not).
                        let old = if Some(id) == current {
                            self.project.clone()
                        } else {
                            self.open_or_stored(id)?
                        };
                        let tpid = old.model.project.proj_id;
                        let (merged, report) =
                            add_into_project(&old.updated_document(), tpid, doc, r.pid, &w.config)?;
                        let mut p =
                            ProjectData::from_document(merged, tpid, old.source_name.clone())?;
                        p.document.windows_encoding = old.document.windows_encoding;
                        p.model.options.do_level = old.model.options.do_level;
                        let added = p.model.activities.len() - old.model.activities.len();
                        item.summary.push(format!(
                            "Adds {added} activities from {} into {}",
                            r.short,
                            target_title(Some(id))
                        ));
                        item.summary.extend(report.summary_lines());
                        item.warnings = report.messages;
                        item.write = Some(Write::Overwrite(id, p));
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                item.error = Some(e);
                item.write = None;
            }
            prepared.push(item);
        }
        // A shared dictionary: what each project does to it, without writing.
        let choices = w.choices.clone();
        let writes: Vec<&Write> = prepared.iter().filter_map(|p| p.write.as_ref()).collect();
        let mut clashes = BTreeSet::new();
        match workspace::preview(&self.store, &writes, &choices) {
            Ok(events) => {
                let mut events = events.into_iter();
                for item in prepared.iter_mut().filter(|p| p.write.is_some()) {
                    let e = events.next().unwrap_or_default();
                    clashes.extend(openxer_automation::workspace_db::clash_tables(&e));
                    item.dictionary = openxer_automation::workspace_db::import_report(&e);
                }
            }
            Err(e) => {
                if let Some(item) = prepared.iter_mut().find(|p| p.write.is_some()) {
                    item.error = Some(format!("The workspace dictionary: {e}"));
                }
            }
        }
        if let Some(w) = self.interchange.xer.as_mut() {
            w.prepared = prepared;
            w.clashes = clashes;
        }
    }

    fn xer_finish(&mut self) {
        let Some(mut w) = self.interchange.xer.take() else {
            return;
        };
        // Every open project is saved first: the import merged their open
        // copies, and the backup taken next must hold their changes.
        if self.any_dirty() {
            self.save();
            if self.any_dirty() {
                self.interchange.xer = Some(w);
                if let Some(e) = self.error.take() {
                    self.error(format!(
                        "The import was not started because the open projects could not be saved first. {e}"
                    ));
                }
                return;
            }
        }
        // The file and project of each write, in write order.
        let origins: Vec<(usize, i64)> = w
            .rows
            .iter()
            .zip(&w.prepared)
            .filter(|(_, p)| p.write.is_some())
            .map(|(r, _)| (r.file, r.pid))
            .collect();
        // The label of each write, in write order, for the dictionary report.
        let labels: Vec<String> = w
            .prepared
            .iter()
            .filter(|p| p.write.is_some())
            .map(|p| p.label.clone())
            .collect();
        let writes: Vec<Write> = w
            .prepared
            .iter_mut()
            .filter_map(|p| p.write.take())
            .collect();
        let inserted: Vec<bool> = writes
            .iter()
            .map(|x| matches!(x, Write::Insert(_)))
            .collect();
        let warnings: Vec<String> = w
            .prepared
            .iter()
            .flat_map(|p| p.warnings.iter().map(move |x| format!("{}: {x}", p.label)))
            .collect();
        let backup = match workspace::backup(&self.store) {
            Ok(path) => {
                // The newest automatic import backups stay; older ones go.
                if let Some(dir) = path.parent() {
                    crate::util::prune_old(dir, "openxer-before-import-", "db", KEEP_BACKUPS);
                }
                path
            }
            Err(e) => {
                self.error(format!(
                    "The import was not started because the workspace could not be backed up. {e}"
                ));
                return;
            }
        };
        match workspace::commit_with(&mut self.store, writes, &w.choices, Some(w.folder)) {
            Ok((ids, events)) => {
                let report: Vec<String> = labels
                    .iter()
                    .zip(&events)
                    .flat_map(|(label, e)| {
                        openxer_automation::workspace_db::import_report(e)
                            .into_iter()
                            .map(move |line| format!("{label}: {line}"))
                    })
                    .collect();
                let new: Vec<i64> = ids
                    .iter()
                    .zip(&inserted)
                    .filter(|(_, n)| **n)
                    .map(|(id, _)| *id)
                    .collect();
                if let Err(e) = self.file_imported_projects(&new, w.folder) {
                    self.error(e);
                }
                let attached = self.import_file_baselines(&ids, &origins, &w.files);
                self.saved_projects = self.store.list().unwrap_or_default();
                // Open projects the import wrote take what it wrote, so no
                // older open copy is saved over it later.
                self.reload_open_copies(&ids);
                if let Some(id) = ids.first().copied().filter(|_| w.open_after) {
                    if self.is_open(id) {
                        self.activate_id(id);
                    } else {
                        match self.store.load(id) {
                            Ok(p) => self.load(p),
                            Err(e) => self.error(e),
                        }
                    }
                }
                let warned = !warnings.is_empty();
                let mut lines = report;
                lines.extend(warnings);
                if attached > 0 {
                    lines.push(format!(
                        "{} stored with their projects.",
                        crate::util::count(attached, "baseline", "baselines")
                    ));
                }
                lines.push(format!(
                    "Backup of the workspace before the import: {}",
                    backup.display()
                ));
                lines.push(
                    "File > Import, Undo Last Import restores the projects this import changed."
                        .into(),
                );
                self.interchange.message = Some(Message {
                    title: "Import".into(),
                    text: if warned {
                        format!(
                            "The import is complete. {} imported. Warnings were recorded:",
                            crate::util::count(ids.len(), "project", "projects")
                        )
                    } else {
                        format!(
                            "The import is complete. {} imported.",
                            crate::util::count(ids.len(), "project", "projects")
                        )
                    },
                    lines,
                });
                self.status = format!(
                    "Imported {}",
                    crate::util::count(ids.len(), "project", "projects")
                );
            }
            Err(e) => self.error(format!(
                "The import was not applied; the workspace is unchanged. {e}"
            )),
        }
    }

    /// Where a newly imported project is filed: the folder selected in the
    /// Projects view (or the selected project's folder), else the first
    /// top-level folder.
    pub(crate) fn default_import_folder(&mut self) -> i64 {
        let ws = self.workspace().clone();
        let nodes = ws.eps();
        let selected_project = self
            .views
            .projects
            .selected
            .clone()
            .and_then(|(source, key)| {
                self.store
                    .id_of(&source, key)
                    .map(|id| ws.node_of(Some(id)))
            });
        selected_project
            .or(self.views.projects.selected_eps)
            .filter(|id| nodes.iter().any(|n| n.id == *id))
            .or_else(|| nodes.iter().find(|n| n.depth == 0).map(|n| n.id))
            .unwrap_or(0)
    }

    /// File the projects an import created under `folder`. Undo Last Import
    /// takes the placements back with the projects.
    pub(crate) fn file_imported_projects(
        &mut self,
        ids: &[i64],
        folder: i64,
    ) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut ws = self.workspace().clone();
        for id in ids {
            ws.place(*id, folder)?;
        }
        self.store
            .set_preference(crate::enterprise::workspace::KEY, &ws.to_text())?;
        self.enterprise.cache = Some(ws);
        Ok(())
    }

    /// Undo Last Import was clicked: say what it does and ask first.
    pub(crate) fn ask_undo_last_import(&mut self) {
        let entries = match workspace::last_import_entries(&self.store) {
            Ok(e) if !e.is_empty() => e,
            Ok(_) => {
                self.error("There is no import to undo".into());
                return;
            }
            Err(e) => {
                self.error(e);
                return;
            }
        };
        let when = workspace::last_import(&self.store)
            .ok()
            .flatten()
            .map(|(_, _, when)| when)
            .unwrap_or_default();
        let unsaved = |id: i64| {
            (Some(id) == self.open_set.active && self.dirty)
                || self.open_set.parked.iter().any(|p| p.id == id && p.dirty)
        };
        let mut lost = false;
        let lines: Vec<String> = entries
            .iter()
            .map(|e| {
                let what = if e.existed {
                    "goes back to how it was before the import"
                } else {
                    "was added by the import and is deleted"
                };
                let since = match e.changed {
                    _ if unsaved(e.row_id) => {
                        lost = true;
                        "; it has unsaved changes, which are lost"
                    }
                    Some(true) => {
                        lost = true;
                        "; it was changed after the import, and those changes are lost"
                    }
                    Some(false) => "",
                    None => "; any change made to it after the import is lost",
                };
                format!("{}: {what}{since}.", e.title)
            })
            .collect();
        let text = format!(
            "Undo the import of {when}? {}\n\nThe workspace is copied into backups first, so nothing is gone for good. Cancel leaves everything as it is.",
            if lost {
                "Changes made since the import to the projects below are lost from the workspace."
            } else {
                "The projects below go back to how they were before it."
            }
        );
        self.interchange.undo_prompt = Some(UndoPrompt { text, lines });
    }

    /// The question [`App::ask_undo_last_import`] asks.
    fn undo_prompt_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.interchange.undo_prompt.as_ref() else {
            return;
        };
        let (text, lines) = (prompt.text.clone(), prompt.lines.clone());
        let mut rects = vec![];
        let answer = crate::kit_window::message_box_with_lines(
            ctx,
            "undo_import",
            "Undo Last Import",
            crate::kit_window::MessageIcon::Question,
            &text,
            &lines,
            &["Undo Import", "Cancel"],
            crate::kit_window::Question::UndoImport.default_in(&["Undo Import", "Cancel"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = crate::kit_window::escape_goes_to(
            ctx,
            egui::Id::new(("kit_message", "undo_import")),
            egui::Order::Foreground,
        );
        match answer {
            Some(0) => {
                self.interchange.undo_prompt = None;
                self.undo_last_import();
            }
            Some(_) => self.interchange.undo_prompt = None,
            None if escape => self.interchange.undo_prompt = None,
            None => {}
        }
    }

    /// Undo the last import (after [`App::ask_undo_last_import`]): every open
    /// project is saved and the workspace backed up first, then the rows the
    /// import wrote are put back and the open copies take them.
    pub(crate) fn undo_last_import(&mut self) {
        if self.any_dirty() {
            self.save();
            if self.any_dirty() {
                if let Some(e) = self.error.take() {
                    self.error(format!(
                        "The import was not undone because the open projects could not be saved first. {e}"
                    ));
                }
                return;
            }
        }
        let backup = match workspace::backup_named(&self.store, workspace::UNDO_BACKUP) {
            Ok(path) => {
                if let Some(dir) = path.parent() {
                    crate::util::prune_old(dir, workspace::UNDO_BACKUP, "db", KEEP_BACKUPS);
                }
                path
            }
            Err(e) => {
                self.error(format!(
                    "The import was not undone because the workspace could not be backed up. {e}"
                ));
                return;
            }
        };
        // Open projects may use imported dictionary rows without a save yet.
        let mut open: Vec<(Option<i64>, &openxer_xer::Document)> =
            vec![(self.open_set.active, &self.project.document)];
        open.extend(
            self.open_set
                .parked
                .iter()
                .map(|p| (Some(p.id), &p.project.document)),
        );
        let undone = workspace::undo_last_import(&mut self.store, &open);
        match undone {
            Ok((ids, kept)) => {
                if !kept.is_empty() {
                    self.error(format!(
                        "The last import was undone. In the workspace dictionary: {}",
                        kept.join(" ")
                    ));
                }
                // Projects the import created are gone: so are their folder
                // placements.
                let gone: Vec<i64> = ids
                    .iter()
                    .copied()
                    .filter(|id| self.store.source(*id).is_err())
                    .collect();
                if !gone.is_empty() {
                    let mut ws = self.workspace().clone();
                    for id in &gone {
                        ws.forget(*id);
                    }
                    if let Err(e) = self
                        .store
                        .set_preference(crate::enterprise::workspace::KEY, &ws.to_text())
                    {
                        self.error(e);
                    }
                    self.enterprise.cache = Some(ws);
                }
                self.baselines_changed();
                self.saved_projects = self.store.list().unwrap_or_default();
                // Open projects take the rows as they are now, so no open
                // copy of what the import wrote is saved back over them.
                if !self.reload_open_copies(&ids) {
                    // The open project was removed; reopen another workspace
                    // project rather than a fresh starter that would overwrite one.
                    let next = self
                        .store
                        .list()
                        .and_then(|l| l.first().map(|r| r.0).ok_or_else(|| "empty".into()))
                        .and_then(|first| self.store.load(first));
                    let next = match next {
                        Ok(p) => p,
                        Err(_) => {
                            let mut p = openxer_xer::starter();
                            p.source_name = format!(
                                "Starter project {}",
                                chrono::Local::now().format("%Y%m%d%H%M%S")
                            );
                            p
                        }
                    };
                    self.dirty = false;
                    self.load(next);
                }
                self.status = format!(
                    "Undid the last import ({} restored); the workspace as it was is kept in backups\\{}",
                    crate::util::count(ids.len(), "workspace project", "workspace projects"),
                    backup
                        .file_name()
                        .map_or(String::new(), |f| f.to_string_lossy().into_owned())
                );
                if let Some(w) = self.interchange.xer.as_mut() {
                    w.workspace = workspace::rows(&self.store).unwrap_or_default();
                    w.last_import =
                        workspace::last_import(&self.store)
                            .ok()
                            .flatten()
                            .map(|(_, n, when)| {
                                format!(
                                    "{} on {when}",
                                    crate::util::count(
                                        n,
                                        "workspace project",
                                        "workspace projects"
                                    )
                                )
                            });
                }
            }
            Err(e) => self.error(e),
        }
    }

    fn xer_import_dialog(&mut self, ctx: &egui::Context) {
        let step = self.interchange.xer.as_ref().map_or(0, |w| w.step);
        let (heading, subtitle) = match step {
            0 => ("Import Format", "Select the import format."),
            1 => ("Select XER File", "Select the file(s) to import."),
            2 => (
                "Import Project Options",
                "Choose an import action and the Import To project for each project.",
            ),
            3 => (
                "Update Project Options",
                "Modify the import configuration: the action for data that exists in both.",
            ),
            _ => (
                "Ready to Begin Importing Data",
                "Review what will change, then click Finish.",
            ),
        };
        let can_finish = step == 4
            && self.interchange.xer.as_ref().is_some_and(|w| {
                w.prepared.iter().all(|p| p.error.is_none())
                    && w.prepared.iter().any(|p| p.write.is_some())
            });
        let nav = self.wizard(
            ctx,
            "xer_import",
            "Import",
            heading,
            subtitle,
            if step == 0 {
                Vec2::new(500., 328.)
            } else {
                Vec2::new(718., 467.)
            },
            (step > 0, step < 4, can_finish),
            |app, ui| {
                app.xer_import_page(ui, step);
            },
        );
        let Some(w) = self.interchange.xer.as_mut() else {
            return;
        };
        match nav {
            Nav::Cancel => self.interchange.xer = None,
            Nav::Back => {
                w.error = None;
                w.step = match w.step {
                    4 if !w.rows.iter().any(|r| r.action.merges()) => 2,
                    s => s.saturating_sub(1),
                };
            }
            Nav::Next => match w.step {
                0 => {
                    if w.spreadsheet {
                        let path = w.path_text.clone();
                        self.interchange.xer = None;
                        self.open_sheet_import(path);
                    } else {
                        w.step = 1;
                    }
                }
                1 => {
                    self.xer_load_files();
                    if let Some(w) = self.interchange.xer.as_mut() {
                        if w.error.is_none() {
                            w.step = 2;
                        }
                    }
                }
                2 => {
                    w.error = Self::xer_validate_rows(w);
                    if w.error.is_none() {
                        if w.rows.iter().any(|r| r.action.merges()) {
                            w.step = 3;
                        } else {
                            w.step = 4;
                            self.xer_prepare();
                        }
                    }
                }
                3 => {
                    w.step = 4;
                    self.xer_prepare();
                }
                _ => {}
            },
            Nav::Finish => self.xer_finish(),
            Nav::None => {}
        }
    }

    /// The pages of File > Import on the dialog kit, in client coordinates
    /// (the body runs from y 75 to 421, x 16 to 702): labels, edits with
    /// the `...` inside their frame, kit lists with in-cell drop-downs on
    /// the selected row, kit drop-downs and check boxes.
    fn xer_import_page(&mut self, ui: &mut egui::Ui, step: usize) {
        use crate::dialog_kit::{self as kit, Col};
        use crate::grid::cell_combo::cell_combo;
        use openxer_xer::workspace_scope::merge::Choice;
        let mut browse = false;
        let mut undo = false;
        let mut rerun = false;
        let folders: Vec<(i64, usize, String)> = {
            let ws = self.workspace().clone();
            ws.eps()
                .iter()
                .map(|n| (n.id, n.depth, ws.eps_value(n.id, "eps_name")))
                .collect()
        };
        let Some(w) = self.interchange.xer.as_mut() else {
            return;
        };
        // Client coordinates, as the first page and the wizard frame use.
        let origin = ui.max_rect().min - Vec2::new(16., 75.);
        let mut c = crate::schedule_dialogs::Canvas {
            ui,
            origin,
            salt: "xer_import",
            actions: vec![],
        };
        // The body's right edge and bottom.
        let (right, bottom) = (702., 421.);
        match step {
            0 => {
                let chosen = (w.spreadsheet, w.xml);
                let picks = [(false, false), (false, true), (true, false)];
                let rows = [
                    ("import_format_xer", "Schedule - (XER)"),
                    ("import_format_xml", "Project - (XML)"),
                    ("import_format_xlsx", "Spreadsheet - (XLSX)"),
                ];
                for (n, ((key, label), pick)) in rows.into_iter().zip(picks).enumerate() {
                    let y = 96. + 23. * n as f32;
                    if c.radio(key, 16., y, 309., 17., label, chosen == pick, true) {
                        (w.spreadsheet, w.xml) = pick;
                    }
                }
                c.etched(16., 190., 468.);
                match &w.last_import {
                    Some(text) => {
                        kit::text(&c, 16., 200., &format!("Last import: {text}."), true);
                        undo = kit::button(
                            &mut c,
                            "undo_last_import",
                            16.,
                            218.,
                            140.,
                            25.,
                            "Undo Last Import",
                            true,
                        );
                    }
                    None => kit::text(
                        &c,
                        16.,
                        200.,
                        "Imports are backed up and can be undone from this page.",
                        true,
                    ),
                }
            }
            1 => {
                // File Name: the edit with its `...` inside the frame.
                c.label(16., 82., 16., "File Name", true);
                let (x, y, fw, fh) = (16., 101., 470., 21.);
                c.edit(
                    "xer_import_file",
                    x,
                    y,
                    fw,
                    fh,
                    &mut w.path_text,
                    true,
                    egui::Align::Min,
                );
                let frame = c.r(x, y, fw, fh);
                browse = c.ellipsis("xer_import_browse", frame, true).clicked();
                let kind = if w.xml { "XML" } else { "XER" };
                let note = format!(
                    "Several {kind} files can be imported at once: separate their names with a semicolon (;)."
                );
                kit::text(&c, 16., 132., &note, true);
                kit::text(
                    &c,
                    16.,
                    150.,
                    "Each project in the files is listed on the next page.",
                    true,
                );
            }
            2 => {
                // One row per project: Import Action, Project ID, its
                // activities and file, Import To. The selected row's
                // Import Action and Import To cells are drop-downs.
                let (lx, ly, lw, lh) = (16., 80., right - 16., 232.);
                let fixed = [150., 100., 64., 200.];
                let to_width = lw - 2. - fixed.iter().sum::<f32>();
                let cols = [
                    Col {
                        title: "Import Action",
                        width: fixed[0],
                    },
                    Col {
                        title: "Project ID",
                        width: fixed[1],
                    },
                    Col {
                        title: "Activities",
                        width: fixed[2],
                    },
                    Col {
                        title: "File",
                        width: fixed[3],
                    },
                    Col {
                        title: "Import To",
                        width: to_width,
                    },
                ];
                let actions: Vec<String> = ImportAction::ALL
                    .iter()
                    .map(|a| a.label().to_string())
                    .collect();
                let targets: Vec<String> = w
                    .workspace
                    .iter()
                    .map(|x| format!("{}  ({})", x.title, x.project_key))
                    .collect();
                let files: Vec<String> = w
                    .rows
                    .iter()
                    .map(|r| {
                        let path = &w.files[r.file].0;
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect();
                let body = c.r(lx + 1., ly + 19., lw - 2., lh - 20.);
                let selected = w.selected.min(w.rows.len().saturating_sub(1));
                let mut picked_row = None;
                let rows = &mut w.rows;
                let workspace = &w.workspace;
                let out = kit::list(
                    &mut c,
                    "xer_import_projects",
                    (lx, ly, lw, lh),
                    None,
                    &cols,
                    rows.len(),
                    Some(selected),
                    None,
                    &mut |c, n, k, cell, sel| {
                        let r = &mut rows[n];
                        let inner = Rect::from_min_max(cell.min, cell.max - Vec2::new(1., 1.));
                        let target = if r.action.has_target() {
                            workspace
                                .iter()
                                .find(|x| Some(x.id) == r.target)
                                .map_or("(choose a project)".to_string(), |x| x.title.clone())
                        } else {
                            "(new project)".into()
                        };
                        let picker = match k {
                            0 => Some((format!("import_action_{n}"), r.action.label(), &actions)),
                            4 if r.action.has_target() => {
                                Some((format!("import_to_{n}"), target.as_str(), &targets))
                            }
                            _ => None,
                        };
                        match picker {
                            Some((key, text, items)) if sel => {
                                let out =
                                    cell_combo(c, &key, inner, body, text, items, true, true, true);
                                if out.clicked {
                                    picked_row = Some(n);
                                }
                                match (k, out.picked) {
                                    (0, Some(i)) => r.action = ImportAction::ALL[i],
                                    (_, Some(i)) => r.target = workspace.get(i).map(|x| x.id),
                                    _ => {}
                                }
                            }
                            _ => {
                                let text = match k {
                                    0 => r.action.label().to_string(),
                                    1 => r.short.clone(),
                                    2 => r.activities.to_string(),
                                    3 => files[n].clone(),
                                    _ => target.clone(),
                                };
                                kit::cell_text(c, cell, &text, sel, 3.);
                            }
                        }
                    },
                );
                if let Some(n) = out.clicked.or(picked_row) {
                    w.selected = n;
                }
                // Where new projects go, and what follows the import.
                let creates = w.rows.iter().any(|r| r.action == ImportAction::CreateNew);
                let y = ly + lh + 14.;
                c.label(16., y, 21., "EPS folder for new projects", creates);
                let names: Vec<String> = folders
                    .iter()
                    .map(|(_, depth, name)| format!("{}{name}", "    ".repeat(*depth)))
                    .collect();
                let items: Vec<(&str, bool)> = names.iter().map(|s| (s.as_str(), true)).collect();
                let mut index = folders
                    .iter()
                    .position(|(id, _, _)| *id == w.folder)
                    .unwrap_or(usize::MAX);
                let key = "import_folder";
                if c.combo(key, 200., y, 300., 21., &items, &mut index, creates) {
                    if let Some((id, _, _)) = folders.get(index) {
                        w.folder = *id;
                    }
                }
                c.checkbox(
                    "import_open_after",
                    16.,
                    y + 33.,
                    480.,
                    17.,
                    "Open the imported project when the import finishes",
                    &mut w.open_after,
                    true,
                );
            }
            3 => {
                // What the import does with data in both the file and the
                // project it updates: one row per kind of data.
                kit::text(
                    &c,
                    16.,
                    80.,
                    "Delete removes project data that the file does not have.",
                    true,
                );
                let (lx, ly, lw, lh) = (16., 100., 500., 210.);
                let cols = [
                    Col {
                        title: "Data Type",
                        width: 260.,
                    },
                    Col {
                        title: "Import Action",
                        width: 170.,
                    },
                    Col {
                        title: "Delete",
                        width: lw - 2. - 430.,
                    },
                ];
                let types = DataType::ALL;
                let body = c.r(lx + 1., ly + 19., lw - 2., lh - 20.);
                let selected = w.selected_type.min(types.len() - 1);
                let mut picked_row = None;
                let config = &mut w.config;
                let out = kit::list(
                    &mut c,
                    "xer_import_config",
                    (lx, ly, lw, lh),
                    None,
                    &cols,
                    types.len(),
                    Some(selected),
                    None,
                    &mut |c, n, k, cell, sel| {
                        let t = types[n];
                        let mut chosen = config.get(t);
                        match k {
                            0 => {
                                let global = if t.is_global() { "  (global)" } else { "" };
                                let text = format!("{}{global}", t.label());
                                kit::cell_text(c, cell, &text, sel, 3.);
                            }
                            1 if sel => {
                                let items: Vec<String> =
                                    t.actions().iter().map(|a| a.label().to_string()).collect();
                                let key = format!("config_{}", t.key());
                                let at = Rect::from_min_max(cell.min, cell.max - Vec2::new(1., 1.));
                                let text = chosen.action.label();
                                let out =
                                    cell_combo(c, &key, at, body, text, &items, true, true, true);
                                if let Some(i) = out.picked {
                                    chosen.action = t.actions()[i];
                                }
                                if out.clicked {
                                    picked_row = Some(n);
                                }
                            }
                            1 => kit::cell_text(c, cell, chosen.action.label(), sel, 3.),
                            _ if t.supports_delete() => {
                                let at = cell.center() - c.origin - Vec2::splat(6.5);
                                let (x, y) = (at.x.round(), at.y.round());
                                kit::check_glyph(c, x, y, chosen.delete, true);
                                let key = format!("config_delete_{}", t.key());
                                if c.hit(&key, cell, true).clicked() {
                                    chosen.delete = !chosen.delete;
                                    picked_row = Some(n);
                                }
                            }
                            _ => {}
                        }
                        config.set(t, chosen.action, chosen.delete);
                    },
                );
                if let Some(n) = out.clicked.or(picked_row) {
                    w.selected_type = n;
                }
            }
            _ => {
                // A shared dictionary: what a row whose name the workspace
                // has with different content does, per kind.
                let mut y = 80.;
                if !w.clashes.is_empty() {
                    kit::text(
                        &c,
                        16.,
                        y,
                        "Dictionary rows the workspace has under the same name with other values:",
                        true,
                    );
                    y += 22.;
                    let labels: Vec<(&str, bool)> =
                        Choice::ALL.iter().map(|ch| (ch.label(), true)).collect();
                    for table in w.clashes.clone() {
                        let kind = openxer_automation::workspace_db::kind_label(&table);
                        c.label(32., y, 21., kind, true);
                        let current = w.choices.get(&table).copied().unwrap_or_default();
                        let mut index = Choice::ALL
                            .iter()
                            .position(|ch| *ch == current)
                            .unwrap_or(0);
                        let key = format!("import_dictionary_{}", table.to_ascii_lowercase());
                        if c.combo(&key, 200., y, 220., 21., &labels, &mut index, true) {
                            if let Some(choice) = Choice::ALL.get(index) {
                                w.choices.insert(table.clone(), *choice);
                                rerun = true;
                            }
                        }
                        y += 27.;
                    }
                    y += 4.;
                }
                // What the import does, a line each, in a kit list.
                let warn = Color32::from_rgb(156, 87, 0);
                let mut lines: Vec<(String, Color32, bool)> = vec![];
                for p in &w.prepared {
                    lines.push((p.label.clone(), INK, true));
                    for s in &p.summary {
                        lines.push((format!("    {s}"), INK, false));
                    }
                    for s in &p.dictionary {
                        lines.push((format!("    Dictionary: {s}"), INK, false));
                    }
                    for s in &p.warnings {
                        lines.push((format!("    Warning: {s}"), warn, false));
                    }
                    if let Some(e) = &p.error {
                        lines.push((format!("    Cannot import: {e}"), CRITICAL, false));
                    }
                }
                let h = bottom - 42. - y;
                let cols = [Col {
                    title: "What the import does",
                    width: right - 16. - 2.,
                }];
                kit::list(
                    &mut c,
                    "xer_import_summary",
                    (16., y, right - 16., h),
                    None,
                    &cols,
                    lines.len(),
                    None,
                    None,
                    &mut |c, n, _, cell, _| {
                        let (text, color, bold) = &lines[n];
                        let font = if *bold {
                            kit::heading_font()
                        } else {
                            kit::body_font()
                        };
                        let pos = Pos2::new(cell.left() + 3., cell.center().y);
                        c.ui.painter().with_clip_rect(cell).text(
                            pos,
                            egui::Align2::LEFT_CENTER,
                            text,
                            font,
                            *color,
                        );
                    },
                );
                kit::text(
                    &c,
                    16.,
                    bottom - 34.,
                    "The workspace is backed up before anything is written, and the import is one step you can undo.",
                    true,
                );
            }
        }
        if let Some(e) = &w.error {
            kit::error_text(&c, 16., bottom - 16., right - 16., e);
        }
        for (name, r) in c.actions {
            self.action_rects
                .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
        }
        if browse {
            let xml = self.interchange.xer.as_ref().is_some_and(|w| w.xml);
            let (filter, ext) = if xml {
                ("XML files", "xml")
            } else {
                ("XER files", "xer")
            };
            if let Some(files) = rfd::FileDialog::new()
                .add_filter(filter, &[ext])
                .pick_files()
            {
                if let Some(w) = self.interchange.xer.as_mut() {
                    w.path_text = files
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join("; ");
                }
            }
        }
        if undo {
            self.ask_undo_last_import();
        }
        // A changed choice changes what the import does: preview it again.
        if rerun {
            self.xer_prepare();
        }
    }

    // ------------------------------------------------------------ export

    fn export_dialog(&mut self, ctx: &egui::Context) {
        let Some(step) = self.interchange.export.as_ref().map(|w| w.step) else {
            return;
        };
        let (heading, subtitle) = match step {
            0 => ("Export Format", "Select the export format."),
            1 => ("Projects To Export", "Select the projects to export."),
            2 => (
                "Export Options",
                "Choose the version or units for the file.",
            ),
            _ => (
                "File Name",
                "Type the name and location of the export file.",
            ),
        };
        let nav = self.wizard(
            ctx,
            "export",
            "Export",
            heading,
            subtitle,
            if step == 0 {
                Vec2::new(500., 328.)
            } else {
                Vec2::new(638., 427.)
            },
            (step > 0, step < 3, step == 3),
            |app, ui| app.export_page(ui, step),
        );
        let Some(w) = self.interchange.export.as_mut() else {
            return;
        };
        match nav {
            Nav::Cancel => self.interchange.export = None,
            Nav::Back => {
                w.error = None;
                w.step = w.step.saturating_sub(1);
            }
            Nav::Next => {
                w.error = None;
                if w.step == 1 {
                    let n = w.chosen.iter().filter(|c| **c).count();
                    if n == 0 {
                        w.error = Some("Mark the Export checkbox for at least one project".into());
                        return;
                    }
                    if !w.format.is_schedule() && n > 1 {
                        w.error =
                            Some("A spreadsheet holds one project; choose one project".into());
                        return;
                    }
                    let first = w
                        .workspace
                        .iter()
                        .zip(&w.chosen)
                        .find(|(_, c)| **c)
                        .map(|(r, _)| r.title.clone())
                        .unwrap_or_default();
                    let ext = w.format.extension();
                    w.path = documents()
                        .join(format!("{}.{ext}", safe_file_name(&first)))
                        .display()
                        .to_string();
                }
                w.step += 1;
            }
            Nav::Finish => self.export_finish(),
            Nav::None => {}
        }
    }

    fn export_page(&mut self, ui: &mut egui::Ui, step: usize) {
        let mut browse = false;
        // 0 New..., 1 Edit..., 2 Delete on the options page.
        let mut template_action = None;
        let Some(w) = self.interchange.export.as_mut() else {
            return;
        };
        match step {
            0 => {
                let origin = ui.max_rect().min - Vec2::new(16., 75.);
                let mut c = crate::schedule_dialogs::Canvas {
                    ui,
                    origin,
                    salt: "export",
                    actions: vec![],
                };
                for (f, key, y, label) in [
                    (
                        ExportFormat::Xer,
                        "export_format_xer",
                        104.,
                        "Schedule - (XER)",
                    ),
                    (
                        ExportFormat::Xml,
                        "export_format_xml",
                        127.,
                        "Project - (XML)",
                    ),
                    (
                        ExportFormat::Modern,
                        "export_format_modern",
                        150.,
                        "Spreadsheet - (XLSX), OpenXER layout: real dates, outline, frozen header",
                    ),
                    (
                        ExportFormat::Tables,
                        "export_format_tables",
                        173.,
                        "Spreadsheet - (XLSX), XER table layout: TASK / TASKPRED sheets",
                    ),
                ] {
                    if c.radio(key, 18., y, 450., 17., label, w.format == f, true) {
                        w.format = f;
                    }
                }
                for (name, r) in c.actions {
                    self.action_rects
                        .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
                }
            }
            1 => {
                egui::ScrollArea::vertical()
                    .max_height(300.)
                    .show(ui, |ui| {
                        let xer = w.format.is_schedule();
                        egui::Grid::new("export_projects")
                            .striped(true)
                            .num_columns(4)
                            .spacing([14., 4.])
                            .show(ui, |ui| {
                                for h in ["Export", "Project ID", "Baselines to Export", "Source"] {
                                    ui.label(RichText::new(h).strong());
                                }
                                ui.end_row();
                                for (k, (r, c)) in
                                    w.workspace.iter().zip(w.chosen.iter_mut()).enumerate()
                                {
                                    crate::dialog_kit::flow_checkbox(ui, c, "", true);
                                    ui.label(if Some(r.id) == w.current {
                                        format!("{}  (open)", r.title)
                                    } else {
                                        r.title.clone()
                                    });
                                    let count = w.baseline_counts.get(k).copied().unwrap_or(0);
                                    if let Some(b) = w.baselines.get_mut(k) {
                                        let text = if count == 0 {
                                            "None".to_string()
                                        } else {
                                            format!("All ({count})")
                                        };
                                        let r = crate::dialog_kit::flow_checkbox(
                                            ui,
                                            b,
                                            &text,
                                            xer && count > 0 && *c,
                                        );
                                        self.action_rects.insert(
                                            format!("export_baselines_{k}"),
                                            [
                                                r.rect.left(),
                                                r.rect.top(),
                                                r.rect.right(),
                                                r.rect.bottom(),
                                            ],
                                        );
                                    }
                                    ui.label(&r.source);
                                    ui.end_row();
                                }
                            });
                    });
            }
            2 => match w.format {
                ExportFormat::Xer => {
                    ui.horizontal(|ui| {
                        ui.label("Version");
                        let sel = XER_VERSIONS.iter().position(|&v| v == w.version);
                        if let (_, Some(idx)) = crate::dialog_kit::flow_combo(
                            ui,
                            "xer_version",
                            120.,
                            &XER_VERSIONS,
                            sel,
                            true,
                        ) {
                            w.version = XER_VERSIONS[idx].to_string();
                        }
                    });
                    ui.label("The version label is written to the file header. Choose an older version for an older recipient.");
                }
                ExportFormat::Xml => {
                    ui.label("Project XML has no version choice. Projects with their schedule options, WBS, activities, relationships, calendars, resources, assignments, codes, UDFs, expenses, notebooks, the OBS and currencies are written; other XER data is left out.");
                }
                _ => {
                    let templated = w.format == ExportFormat::Modern && w.template.is_some();
                    if w.format == ExportFormat::Modern {
                        ui.horizontal(|ui| {
                            ui.label("Template");
                            let shown = w
                                .template
                                .and_then(|k| w.templates.get(k))
                                .map_or("Current table layout", |t| t.name.as_str());
                            crate::dialog_kit::flow_combo_with(
                                ui,
                                "export_template",
                                220.,
                                shown,
                                true,
                                |ui| {
                                    if crate::dialog_kit::flow_selectable(
                                        ui,
                                        w.template.is_none(),
                                        "Current table layout",
                                        216.,
                                    )
                                    .clicked()
                                    {
                                        w.template = None;
                                        ui.close();
                                    }
                                    for (k, t) in w.templates.iter().enumerate() {
                                        if crate::dialog_kit::flow_selectable(
                                            ui,
                                            w.template == Some(k),
                                            &t.name,
                                            216.,
                                        )
                                        .clicked()
                                        {
                                            w.template = Some(k);
                                            ui.close();
                                        }
                                    }
                                },
                            );
                            for (key, label, enabled, n) in [
                                ("export_template_new", "New...", true, 0),
                                ("export_template_edit", "Edit...", w.template.is_some(), 1),
                                ("export_template_delete", "Delete", w.template.is_some(), 2),
                            ] {
                                let b =
                                    chrome::command_button(ui, label, Vec2::new(80., 21.), enabled);
                                self.action_rects.insert(
                                    key.to_string(),
                                    [b.rect.left(), b.rect.top(), b.rect.right(), b.rect.bottom()],
                                );
                                if b.clicked() {
                                    template_action = Some(n);
                                }
                            }
                        });
                        for note in &w.template_notes {
                            ui.add(egui::Label::new(RichText::new(note).color(CRITICAL)).wrap());
                        }
                    }
                    if templated {
                        ui.label("The template sets the columns, rows, unit and date format.");
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Durations, float and lag in");
                            crate::dialog_kit::flow_radio(
                                ui,
                                &mut w.unit,
                                DurationUnit::Days,
                                "Days",
                                true,
                            );
                            crate::dialog_kit::flow_radio(
                                ui,
                                &mut w.unit,
                                DurationUnit::Hours,
                                "Hours",
                                true,
                            );
                        });
                    }
                    if w.format == ExportFormat::Modern && !templated {
                        const EXPORT_DATE_FORMATS: [&str; 5] = [
                            "dd-mmm-yy hh:mm",
                            "dd-mmm-yy",
                            "yyyy-mm-dd hh:mm",
                            "mm/dd/yyyy hh:mm",
                            "mm/dd/yyyy",
                        ];
                        ui.horizontal(|ui| {
                            ui.label("Date format");
                            let sel = EXPORT_DATE_FORMATS.iter().position(|&f| f == w.date_format);
                            if let (_, Some(idx)) = crate::dialog_kit::flow_combo(
                                ui,
                                "xlsx_date_format",
                                140.,
                                &EXPORT_DATE_FORMATS,
                                sel,
                                true,
                            ) {
                                w.date_format = EXPORT_DATE_FORMATS[idx].to_string();
                            }
                        });
                        crate::dialog_kit::flow_checkbox(
                            ui,
                            &mut w.group_wbs,
                            "Group by WBS (bold band rows with outline levels)",
                            true,
                        );
                        ui.label("Columns follow the activity table layout. The unit is written in each header and on the About sheet.");
                    }
                }
            },
            _ => {
                ui.label("File Name");
                ui.horizontal(|ui| {
                    file_name_edit(ui, &mut w.path, 470., "");
                    browse = chrome::command_button(ui, "Browse...", Vec2::new(80., 21.), true)
                        .clicked();
                });
            }
        }
        if let Some(e) = &w.error {
            ui.add_space(6.);
            ui.label(RichText::new(e).color(CRITICAL));
        }
        if browse {
            let ext = w.format.extension();
            if let Some(path) = rfd::FileDialog::new().add_filter(ext, &[ext]).save_file() {
                w.path = path.display().to_string();
                // The file dialog asked before an existing file was chosen.
                w.replace_ok = Some(w.path.clone());
            }
        }
        match template_action {
            Some(0) => self.open_template_editor(None),
            Some(1) => {
                let at = self.interchange.export.as_ref().and_then(|w| w.template);
                self.open_template_editor(at);
            }
            Some(_) => self.delete_chosen_template(),
            None => {}
        }
    }

    pub(crate) fn export_finish(&mut self) {
        let Some(w) = self.interchange.export.as_ref() else {
            return;
        };
        let path = PathBuf::from(w.path.trim());
        if w.path.trim().is_empty() {
            if let Some(w) = self.interchange.export.as_mut() {
                w.error = Some("Type a file name".into());
            }
            return;
        }
        // A typed or suggested name of a file that is there already is
        // replaced only when Finish is clicked again after this warning.
        let typed = w.path.trim().to_string();
        if path.is_dir() {
            if let Some(w) = self.interchange.export.as_mut() {
                w.error = Some(format!(
                    "{} is a folder. Type a file name in it.",
                    path.display()
                ));
            }
            return;
        }
        if path.exists() && w.replace_ok.as_deref() != Some(typed.as_str()) {
            if let Some(w) = self.interchange.export.as_mut() {
                w.error = Some(format!(
                    "{} already exists. Click Finish again to replace it, or type another file name.",
                    path.display()
                ));
                w.replace_ok = Some(typed);
            }
            return;
        }
        let Some(w) = self.interchange.export.as_ref() else {
            return;
        };
        let ids: Vec<i64> = w
            .workspace
            .iter()
            .zip(&w.chosen)
            .filter(|(_, c)| **c)
            .map(|(r, _)| r.id)
            .collect();
        let with_baselines: Vec<i64> = w
            .workspace
            .iter()
            .enumerate()
            .filter(|(k, _)| w.chosen.get(*k) == Some(&true) && w.baselines.get(*k) == Some(&true))
            .map(|(_, r)| r.id)
            .collect();
        let (format, version, unit, date_format, group, current) = (
            w.format,
            w.version.clone(),
            w.unit,
            w.date_format.clone(),
            w.group_wbs,
            w.current,
        );
        let mut projects = vec![];
        for id in &ids {
            let p = if Some(*id) == current {
                Ok(self.project.clone())
            } else {
                self.open_or_stored(*id)
            };
            match p {
                Ok(p) => projects.push(p),
                Err(e) => {
                    if let Some(w) = self.interchange.export.as_mut() {
                        w.error = Some(e);
                    }
                    return;
                }
            }
        }
        let result = match format {
            ExportFormat::Xer | ExportFormat::Xml => {
                let xml = format == ExportFormat::Xml;
                let refs: Vec<&ProjectData> = projects.iter().collect();
                let mut baselines = 0;
                let shared = self.dictionary_shared();
                export_document(&refs, &version)
                    .map(|doc| {
                        // A shared dictionary holds every project's calendars
                        // and resources; the file gets the ones these use.
                        if shared {
                            let pids: Vec<i64> =
                                refs.iter().map(|p| p.model.project.proj_id).collect();
                            openxer_xer::workspace_scope::used_subset_of(&doc, &pids)
                        } else {
                            doc
                        }
                    })
                    .and_then(|mut doc| {
                        for (id, p) in ids.iter().zip(&projects) {
                            if with_baselines.contains(id) {
                                baselines += crate::baselines::export_baselines(
                                    self.store.connection(),
                                    &mut doc,
                                    *id,
                                    p.model.project.proj_id,
                                )?;
                            }
                        }
                        if xml {
                            openxer_interchange::project_xml::write_file(&doc, &path)
                        } else {
                            doc.write(&path)
                        }
                    })
                    .map(|_| {
                        let with = if baselines > 0 {
                            format!(
                                " with {}",
                                crate::util::count(baselines, "baseline", "baselines")
                            )
                        } else {
                            String::new()
                        };
                        let what = if xml {
                            "as project XML".to_string()
                        } else {
                            format!("(version {version})")
                        };
                        format!(
                            "{}{with} exported to {} {what}.",
                            crate::util::count(refs.len(), "project", "projects"),
                            path.display()
                        )
                    })
            }
            ExportFormat::Modern => {
                let chosen = self
                    .interchange
                    .export
                    .as_ref()
                    .and_then(|w| w.template.and_then(|k| w.templates.get(k)).cloned());
                let options = match chosen {
                    Some(t) => t.options(&projects[0]),
                    None => {
                        let mut options = if ids.first() == current.as_ref() {
                            self.layout_export_options(unit)
                        } else {
                            ExportOptions {
                                unit,
                                ..Default::default()
                            }
                        };
                        options.date_format = date_format;
                        options.group_by_wbs = group;
                        Ok(options)
                    }
                };
                options
                    .and_then(|options| write_modern(&projects[0], &options, &path))
                    .map(|s| {
                        format!(
                            "{} activities, {} relationships and {} assignments exported to {}.",
                            s.activities,
                            s.relationships,
                            s.assignments,
                            path.display()
                        )
                    })
            }
            ExportFormat::Tables => write_tables(&projects[0], unit, &path).map(|s| {
                format!(
                    "{} activities and {} relationships exported to {} in the tables layout.",
                    s.activities,
                    s.relationships,
                    path.display()
                )
            }),
        };
        match result {
            Ok(text) => {
                self.interchange.export = None;
                self.status = text.clone();
                self.interchange.message = Some(Message {
                    title: "Export".into(),
                    text: "The export is complete.".into(),
                    lines: vec![text],
                });
            }
            Err(e) => {
                if let Some(w) = self.interchange.export.as_mut() {
                    w.error = Some(e);
                }
            }
        }
    }

    // ------------------------------------------------------------ spreadsheet import

    pub(crate) fn sheet_load(&mut self) {
        let lookup = Lookup::new(&self.project);
        let Some(w) = self.interchange.sheet.as_mut() else {
            return;
        };
        w.error = None;
        // The column mapping chosen for a workbook read before (Back to the
        // file page, then Next) is kept when the columns are the same.
        let previous = w
            .detected
            .as_ref()
            .map(|d| xi::auto_map(d, &lookup))
            .zip(w.mapping.clone());
        let path = PathBuf::from(w.path.trim().trim_matches('"'));
        let result = xi::read_workbook(&path).and_then(|wb| {
            let detected = xi::detect(&wb, &lookup)?;
            let mapping = xi::auto_map(&detected, &lookup);
            Ok((wb, detected, mapping))
        });
        match result {
            Ok((wb, detected, mapping)) => {
                let mapping = match previous {
                    Some((auto, chosen)) if auto == mapping => chosen,
                    _ => mapping,
                };
                w.wb = Some(wb);
                w.detected = Some(detected);
                w.mapping = Some(mapping);
                w.step = 1;
            }
            Err(e) => w.error = Some(e),
        }
    }

    /// Build the preview. Every changed cell is tried through the activity
    /// setters on a copy of the project, so a cell the table would refuse is
    /// refused here with the same message.
    pub(crate) fn sheet_plan(&mut self) {
        let Some(mut w) = self.interchange.sheet.take() else {
            return;
        };
        if let (Some(wb), Some(d), Some(m)) = (&w.wb, &w.detected, &w.mapping) {
            let snapshot = self.project.clone();
            let saved = self.save_edit_state();
            let mut setters = sheet_setters::AppSetters::new(self);
            let mut plan = xi::plan_with(wb, d, m, &snapshot, &mut setters);
            let out_of_sequence = std::mem::take(&mut setters.out_of_sequence);
            self.restore_edit_state(saved);
            if !out_of_sequence.is_empty() && self.warns_out_of_sequence() {
                plan.notes.push(format!(
                    "Out of sequence after the import: {}",
                    out_of_sequence
                        .iter()
                        .map(sheet_setters::OutOfSequence::line)
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
            w.plan = Some(plan);
            w.step = 2;
        }
        self.interchange.sheet = Some(w);
    }

    pub(crate) fn sheet_apply(&mut self, time: f64) {
        let Some(w) = self.interchange.sheet.as_ref() else {
            return;
        };
        let Some(plan) = w.plan.clone() else {
            return;
        };
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let saved = self.save_edit_state();
        let mut work = self.project.clone();
        let mut setters = sheet_setters::AppSetters::new(self);
        let applied = xi::apply_with(&plan, &mut work, &mut setters);
        let out_of_sequence = std::mem::take(&mut setters.out_of_sequence);
        self.restore_edit_state(saved);
        match applied {
            Ok(report) => {
                self.project = work;
                self.edit_step(before, before_revision, time);
                self.selected = self
                    .selected
                    .filter(|&i| i < self.project.model.activities.len());
                self.interchange.sheet = None;
                if !out_of_sequence.is_empty() && self.warns_out_of_sequence() {
                    self.interchange.message = Some(Message {
                        title: "Spreadsheet Import".into(),
                        text: "The import left activities out of sequence.".into(),
                        lines: out_of_sequence
                            .iter()
                            .map(sheet_setters::OutOfSequence::line)
                            .collect(),
                    });
                }
                self.status = format!(
                    "Spreadsheet imported: {} changed, {} new, {} deleted activities; {} relationships added, {} changed, {} removed; {} resource assignments added, {} changed, {} removed. Ctrl+Z undoes the import.",
                    report.changed, report.added, report.deleted, report.links_added, report.links_changed, report.links_deleted,
                    report.assignments_added, report.assignments_changed, report.assignments_deleted
                );
            }
            Err(e) => {
                if let Some(w) = self.interchange.sheet.as_mut() {
                    w.error = Some(format!("Nothing was changed: {e}"));
                }
            }
        }
    }

    fn sheet_import_dialog(&mut self, ctx: &egui::Context) {
        let Some(w) = self.interchange.sheet.as_ref() else {
            return;
        };
        let step = w.step;
        let (heading, subtitle) = match step {
            0 => (
                "Select XLSX File",
                "Select the spreadsheet to import into the open project.",
            ),
            1 => (
                "Match Columns",
                "Check which project field each spreadsheet column updates.",
            ),
            _ => (
                "Review Changes",
                "Every row is listed. Nothing changes until you click Finish.",
            ),
        };
        let can_finish = step == 2 && w.plan.as_ref().is_some_and(|p| p.has_changes());
        let nav = self.wizard(
            ctx,
            "sheet_import",
            "Spreadsheet Import",
            heading,
            subtitle,
            Vec2::new(898., 567.),
            (step > 0, step < 2, can_finish),
            |app, ui| app.sheet_page(ui, step),
        );
        match nav {
            Nav::Cancel => self.interchange.sheet = None,
            Nav::Back => {
                if let Some(w) = self.interchange.sheet.as_mut() {
                    w.error = None;
                    w.step = w.step.saturating_sub(1);
                }
            }
            Nav::Next => match step {
                0 => self.sheet_load(),
                _ => self.sheet_plan(),
            },
            Nav::Finish => self.sheet_apply(ctx.input(|i| i.time)),
            Nav::None => {}
        }
    }

    fn sheet_page(&mut self, ui: &mut egui::Ui, step: usize) {
        let lookup = Lookup::new(&self.project);
        let project_name = self.project.model.project.name.clone();
        let mut browse = false;
        let Some(w) = self.interchange.sheet.as_mut() else {
            return;
        };
        match step {
            0 => {
                ui.label(format!("Import To: {project_name} (the open project)"));
                ui.label("File Name");
                ui.horizontal(|ui| {
                    file_name_edit(ui, &mut w.path, 700., "C:\\Schedules\\update.xlsx");
                    browse = chrome::command_button(ui, "Browse...", Vec2::new(80., 21.), true)
                        .clicked();
                });
                ui.add_space(8.);
                ui.label("Accepted: OpenXER spreadsheets, the tables layout (TASK, TASKPRED and TASKRSRC sheets), or any sheet whose first row has column headers including Activity ID. A sheet with Activity ID and Resource ID columns updates resource assignments.");
                ui.label("Columns are matched by their header text, not by position. You can adjust the matching on the next page.");
            }
            1 => {
                let (Some(d), Some(m)) = (w.detected.as_ref(), w.mapping.as_mut()) else {
                    return;
                };
                ui.label(format!(
                    "Detected: {}. Sheet \"{}\", headers on row {}.",
                    d.format.label(),
                    w.wb.as_ref()
                        .map_or("", |wb| wb.sheets[d.activities.sheet].name.as_str()),
                    d.activities.header_row + 1
                ));
                for n in &d.notes {
                    ui.label(RichText::new(n).color(Color32::from_rgb(156, 87, 0)));
                }
                ui.horizontal(|ui| {
                    ui.label("Hours per day for day units");
                    crate::dialog_kit::flow_number_edit(
                        ui,
                        "import_hours_per_day",
                        &mut m.hours_per_day,
                        60.,
                        1.0..=24.0,
                        true,
                    );
                    ui.label("Relationship lag unit");
                    crate::dialog_kit::flow_radio(
                        ui,
                        &mut m.link_unit,
                        DurationUnit::Days,
                        "Days",
                        true,
                    );
                    crate::dialog_kit::flow_radio(
                        ui,
                        &mut m.link_unit,
                        DurationUnit::Hours,
                        "Hours",
                        true,
                    );
                });
                let choices = xi::target_choices(&lookup);
                egui::ScrollArea::vertical()
                    .max_height(380.)
                    .show(ui, |ui| {
                        egui::Grid::new("sheet_columns")
                            .striped(true)
                            .num_columns(4)
                            .spacing([14., 3.])
                            .show(ui, |ui| {
                                for h in ["Spreadsheet column", "Field name", "Updates", "Unit"] {
                                    ui.label(RichText::new(h).strong());
                                }
                                ui.end_row();
                                for (i, header) in d.activities.headers.iter().enumerate() {
                                    if i >= m.activity.len() {
                                        break;
                                    }
                                    ui.label(header);
                                    ui.label(
                                        d.activities.fields.get(i).cloned().unwrap_or_default(),
                                    );
                                    let current = choices
                                        .iter()
                                        .find(|(t, _)| *t == m.activity[i])
                                        .map_or("(ignore)".to_string(), |(_, l)| l.clone());
                                    let current = match m.activity[i] {
                                        xi::Target::Column(c) if !c.editable() => {
                                            format!("{current} (calculated, not imported)")
                                        }
                                        _ => current,
                                    };
                                    crate::dialog_kit::flow_combo_with(
                                        ui,
                                        ("map", i),
                                        260.,
                                        &current,
                                        true,
                                        |ui| {
                                            for (t, label) in &choices {
                                                if crate::dialog_kit::flow_selectable(
                                                    ui,
                                                    m.activity[i] == *t,
                                                    label,
                                                    256.,
                                                )
                                                .clicked()
                                                {
                                                    m.activity[i] = *t;
                                                    ui.close();
                                                }
                                            }
                                        },
                                    );
                                    match m.activity[i] {
                                        xi::Target::Column(c)
                                            if c.kind(&lookup)
                                                == openxer_interchange::columns::Kind::Duration =>
                                        {
                                            let sel = Some(usize::from(
                                                m.units[i] == DurationUnit::Hours,
                                            ));
                                            if let (_, Some(idx)) = crate::dialog_kit::flow_combo(
                                                ui,
                                                ("unit", i),
                                                70.,
                                                &["Days", "Hours"],
                                                sel,
                                                true,
                                            ) {
                                                m.units[i] = if idx == 1 {
                                                    DurationUnit::Hours
                                                } else {
                                                    DurationUnit::Days
                                                };
                                            }
                                        }
                                        _ => {
                                            ui.label("");
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                        if let Some(a) = &d.assignments {
                            ui.add_space(8.);
                            ui.label(
                                RichText::new(format!(
                                    "Resource assignments: sheet \"{}\"",
                                    w.wb.as_ref()
                                        .map_or("", |wb| wb.sheets[a.sheet].name.as_str())
                                ))
                                .strong(),
                            );
                            egui::Grid::new("sheet_assignment_columns")
                                .striped(true)
                                .num_columns(3)
                                .spacing([14., 3.])
                                .show(ui, |ui| {
                                    for h in ["Spreadsheet column", "Updates", "Unit"] {
                                        ui.label(RichText::new(h).strong());
                                    }
                                    ui.end_row();
                                    for (i, header) in a.headers.iter().enumerate() {
                                        if i >= m.assignment.len() {
                                            break;
                                        }
                                        ui.label(header);
                                        crate::dialog_kit::flow_combo_with(
                                            ui,
                                            ("assign_map", i),
                                            200.,
                                            m.assignment[i].label(),
                                            true,
                                            |ui| {
                                                for t in xi::AssignTarget::ALL {
                                                    if crate::dialog_kit::flow_selectable(
                                                        ui,
                                                        m.assignment[i] == t,
                                                        t.label(),
                                                        196.,
                                                    )
                                                    .clicked()
                                                    {
                                                        m.assignment[i] = t;
                                                        ui.close();
                                                    }
                                                }
                                            },
                                        );
                                        if m.assignment[i].is_units() {
                                            let units = [DurationUnit::Hours, DurationUnit::Days];
                                            let labels = [units[0].label(), units[1].label()];
                                            let sel = units
                                                .iter()
                                                .position(|u| *u == m.assignment_units[i]);
                                            if let (_, Some(idx)) = crate::dialog_kit::flow_combo(
                                                ui,
                                                ("assign_unit", i),
                                                70.,
                                                &labels,
                                                sel,
                                                true,
                                            ) {
                                                m.assignment_units[i] = units[idx];
                                            }
                                        } else {
                                            ui.label("");
                                        }
                                        ui.end_row();
                                    }
                                });
                        }
                    });
            }
            _ => {
                let Some(plan) = w.plan.as_ref() else {
                    return;
                };
                let c = plan.counts();
                ui.label(format!(
                    "Activities: {} changed, {} new, {} to delete, {} unchanged, {} with errors.   Relationships: {} new, {} changed, {} to delete, {} with errors.",
                    c.changed, c.new, c.delete, c.unchanged, c.errors, c.links_new, c.links_changed, c.links_deleted, c.link_errors
                ));
                if !plan.assignments.is_empty() {
                    ui.label(format!(
                        "Resource assignments: {} new, {} changed, {} to delete, {} unchanged, {} with errors.",
                        c.assignments_new, c.assignments_changed, c.assignments_deleted, c.assignments_unchanged, c.assignment_errors
                    ));
                }
                for n in &plan.notes {
                    ui.label(RichText::new(n).color(Color32::from_rgb(156, 87, 0)));
                }
                ui.horizontal(|ui| {
                    ui.label("Show");
                    crate::dialog_kit::flow_radio(ui, &mut w.filter, 0, "All rows", true);
                    crate::dialog_kit::flow_radio(ui, &mut w.filter, 1, "Changes", true);
                    crate::dialog_kit::flow_radio(ui, &mut w.filter, 2, "Errors", true);
                });
                let show = |k: xi::RowKind, filter: usize| match filter {
                    1 => !matches!(k, xi::RowKind::Unchanged | xi::RowKind::Skipped),
                    2 => k == xi::RowKind::Error,
                    _ => true,
                };
                let filter = w.filter;
                egui::ScrollArea::vertical()
                    .max_height(360.)
                    .show(ui, |ui| {
                        egui::Grid::new("sheet_preview")
                            .num_columns(4)
                            .spacing([10., 2.])
                            .min_col_width(40.)
                            .show(ui, |ui| {
                                for h in [
                                    "Row",
                                    "Activity ID",
                                    "Result",
                                    "Changes (old -> new) or reason",
                                ] {
                                    ui.label(RichText::new(h).strong());
                                }
                                ui.end_row();
                                for r in plan.rows.iter().filter(|r| show(r.kind, filter)) {
                                    let fill = match r.kind {
                                        xi::RowKind::Changed => Some(CHANGED),
                                        xi::RowKind::New => Some(NEW_ROW),
                                        xi::RowKind::Error => Some(ERROR_ROW),
                                        xi::RowKind::Delete => Some(DELETE_ROW),
                                        _ => None,
                                    };
                                    ui.label(r.sheet_row.to_string());
                                    ui.label(&r.activity_id);
                                    let kind = RichText::new(r.kind.label());
                                    ui.label(match fill {
                                        Some(f) => kind.background_color(f),
                                        None => kind,
                                    });
                                    ui.vertical(|ui| {
                                        for ch in &r.changes {
                                            ui.horizontal(|ui| {
                                                ui.label(format!("{}:", ch.column));
                                                if !ch.old.is_empty() {
                                                    ui.label(
                                                        RichText::new(&ch.old)
                                                            .strikethrough()
                                                            .color(Color32::from_gray(110)),
                                                    );
                                                    ui.label("->");
                                                }
                                                ui.label(
                                                    RichText::new(&ch.new)
                                                        .background_color(fill.unwrap_or(CHANGED)),
                                                );
                                            });
                                        }
                                        for msg in &r.messages {
                                            ui.label(RichText::new(msg).color(
                                                if r.kind == xi::RowKind::Error {
                                                    CRITICAL
                                                } else {
                                                    Color32::from_gray(90)
                                                },
                                            ));
                                        }
                                    });
                                    ui.end_row();
                                }
                                if !plan.links.is_empty() {
                                    ui.label(RichText::new("Relationships").strong());
                                    ui.end_row();
                                }
                                for l in plan.links.iter().filter(|l| show(l.kind, filter)) {
                                    ui.label(l.sheet_row.to_string());
                                    ui.label(format!("{} -> {}", l.predecessor, l.successor));
                                    let fill = match l.kind {
                                        xi::RowKind::Changed => Some(CHANGED),
                                        xi::RowKind::New => Some(NEW_ROW),
                                        xi::RowKind::Error => Some(ERROR_ROW),
                                        xi::RowKind::Delete => Some(DELETE_ROW),
                                        _ => None,
                                    };
                                    let kind = RichText::new(l.kind.label());
                                    ui.label(match fill {
                                        Some(f) => kind.background_color(f),
                                        None => kind,
                                    });
                                    ui.label(if l.message.is_empty() {
                                        l.change.clone()
                                    } else {
                                        l.message.clone()
                                    });
                                    ui.end_row();
                                }
                                if !plan.assignments.is_empty() {
                                    ui.label(RichText::new("Resource Assignments").strong());
                                    ui.end_row();
                                }
                                for a in plan.assignments.iter().filter(|a| show(a.kind, filter)) {
                                    ui.label(a.sheet_row.to_string());
                                    ui.label(format!("{} / {}", a.activity_id, a.resource_id));
                                    let fill = match a.kind {
                                        xi::RowKind::Changed => Some(CHANGED),
                                        xi::RowKind::New => Some(NEW_ROW),
                                        xi::RowKind::Error => Some(ERROR_ROW),
                                        xi::RowKind::Delete => Some(DELETE_ROW),
                                        _ => None,
                                    };
                                    let kind = RichText::new(a.kind.label());
                                    ui.label(match fill {
                                        Some(f) => kind.background_color(f),
                                        None => kind,
                                    });
                                    let changes: Vec<String> = a
                                        .changes
                                        .iter()
                                        .map(|ch| {
                                            if ch.old.is_empty() {
                                                format!("{}: {}", ch.column, ch.new)
                                            } else {
                                                format!("{}: {} -> {}", ch.column, ch.old, ch.new)
                                            }
                                        })
                                        .chain((!a.message.is_empty()).then(|| a.message.clone()))
                                        .collect();
                                    ui.label(changes.join("; "));
                                    ui.end_row();
                                }
                            });
                    });
                if !plan.has_changes() {
                    ui.label("There is nothing to import: no row changes the project.");
                }
            }
        }
        if let Some(e) = &w.error {
            ui.add_space(6.);
            ui.label(RichText::new(e).color(CRITICAL));
        }
        if browse {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Excel workbook", &["xlsx"])
                .pick_file()
            {
                w.path = path.display().to_string();
            }
        }
    }
}

#[cfg(test)]
impl App {
    /// File > Import open on page `step` (0 format, 1 file, 2 project
    /// options, 3 update options, 4 summary) with the two-project sample
    /// portfolio as its file, for renders and the dialog sweep. From page 3
    /// the file's first project updates the first workspace project.
    pub(crate) fn open_import_wizard_at(&mut self, step: usize) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let n = CALLS.fetch_add(1, Ordering::Relaxed);
        let name = format!("openxer-import-pages-{}-{n}.xer", std::process::id());
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, openxer_xer::starter_portfolio().text()).unwrap();
        self.open_import_wizard();
        let Some(w) = self.interchange.xer.as_mut() else {
            return;
        };
        w.path_text = path.display().to_string();
        if step == 0 {
            return;
        }
        w.step = 1;
        if step == 1 {
            return;
        }
        self.xer_load_files();
        let Some(w) = self.interchange.xer.as_mut() else {
            return;
        };
        if step >= 3 {
            let target = w.workspace.first().map(|r| r.id);
            if let Some(row) = w.rows.first_mut() {
                row.action = ImportAction::UpdateExisting;
                row.target = target;
            }
        }
        w.step = step.min(4);
        if step >= 4 {
            self.xer_prepare();
        }
    }
}

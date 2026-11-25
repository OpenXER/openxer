//! Enterprise and Admin dictionaries: Activity, Project and Resource Codes,
//! User Defined Fields, Cost Accounts and Admin Categories. The dialogs edit
//! the XER dictionary tables kept in the project document (see `ops`), so the
//! Activity Details tabs and grid columns that read them follow at once, and
//! every change is one undo step that survives save, reopen and export.
use super::*;
use crate::chrome::{Icon, Item};
use crate::details::{cell, grid, Ev};
use crate::dialog_kit::{self as kit, Entry, Move};
pub use crate::enterprise::ui::CellEdit;

mod accounts;
mod admin;
mod codes;
pub mod ops;
pub mod steps;
#[cfg(test)]
mod tests;
mod udf;
mod widgets;

pub use ops::{Category, CodeKind, Scope};

/// A dictionary dialog reachable from the Enterprise or Admin menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dictionary {
    Codes(CodeKind),
    Udf,
    CostAccounts,
    AdminCategories,
    StepTemplates,
}

impl Dictionary {
    pub const RESOURCE_CODES: Self = Dictionary::Codes(CodeKind::Resource);
    pub const PROJECT_CODES: Self = Dictionary::Codes(CodeKind::Project);
    pub const ACTIVITY_CODES: Self = Dictionary::Codes(CodeKind::Activity);
    pub const ASSIGNMENT_CODES: Self = Dictionary::Codes(CodeKind::Assignment);
    fn menu_label(self) -> &'static str {
        match self {
            Dictionary::Codes(CodeKind::Resource) => "Resource Code&s...",
            Dictionary::Codes(CodeKind::Project) => "Pro&ject Codes...",
            Dictionary::Codes(CodeKind::Activity) => "Activity &Codes...",
            Dictionary::Codes(CodeKind::Assignment) => "&Assignment Codes...",
            Dictionary::Udf => "User Defined Fields...",
            Dictionary::CostAccounts => "Cost &Accounts...",
            Dictionary::AdminCategories => "Categories and &Lists...",
            Dictionary::StepTemplates => "Acti&vity Step Templates...",
        }
    }
    fn icon(self) -> Option<Icon> {
        Some(match self {
            Dictionary::Codes(CodeKind::Resource) => Icon::ResourceCodes,
            Dictionary::Codes(CodeKind::Project) => Icon::ProjectCodes,
            Dictionary::Codes(CodeKind::Activity) => Icon::ActivityCodes,
            Dictionary::Codes(CodeKind::Assignment) => Icon::AssignmentCodes,
            Dictionary::Udf => Icon::Udf,
            Dictionary::CostAccounts => Icon::CostAccounts,
            Dictionary::AdminCategories => Icon::AdminCategories,
            Dictionary::StepTemplates => Icon::StepTemplates,
        })
    }
    /// Action name of the menu item.
    pub fn action(self) -> &'static str {
        match self {
            Dictionary::Codes(CodeKind::Resource) => "menu_resource_codes",
            Dictionary::Codes(CodeKind::Project) => "menu_project_codes",
            Dictionary::Codes(CodeKind::Activity) => "menu_activity_codes",
            Dictionary::Codes(CodeKind::Assignment) => "menu_assignment_codes",
            Dictionary::Udf => "menu_udf",
            Dictionary::CostAccounts => "menu_cost_accounts",
            Dictionary::AdminCategories => "menu_admin_categories",
            Dictionary::StepTemplates => "menu_step_templates",
        }
    }
    /// The dictionary a Dictionaries toolbar button opens.
    pub fn from_tool(action: &str) -> Option<Self> {
        Some(match action {
            "tool_resource_codes" => Dictionary::Codes(CodeKind::Resource),
            "tool_project_codes" => Dictionary::Codes(CodeKind::Project),
            "tool_activity_codes" => Dictionary::Codes(CodeKind::Activity),
            "tool_assignment_codes" => Dictionary::Codes(CodeKind::Assignment),
            "tool_udf" => Dictionary::Udf,
            "tool_cost_accounts" => Dictionary::CostAccounts,
            "tool_step_templates" => Dictionary::StepTemplates,
            _ => return None,
        })
    }
}

/// An edit of a dictionary. Each one that succeeds is one undo step.
#[derive(Clone, Debug)]
pub enum Cmd {
    AddType(CodeKind, Scope),
    RenameType(CodeKind, i64, String),
    TypeLength(CodeKind, i64, String),
    DeleteType(CodeKind, i64),
    ShiftType(CodeKind, Scope, i64, i32),
    WidenType(CodeKind, i64, Scope),
    AddValue(CodeKind, i64, Option<i64>),
    SetValue(CodeKind, i64, i64, ops::ValueField, String),
    DeleteValue(CodeKind, i64, i64),
    /// Shift up/down/left/right in a tree.
    Move(ops::TreeSpec, Option<i64>, i64, Move),
    Paste(ops::TreeSpec, Option<i64>, Option<i64>),
    AddUdf(&'static str),
    UdfTitle(i64, String),
    UdfType(i64, &'static str),
    DeleteUdf(i64),
    AddAccount(Option<i64>),
    SetAccount(i64, ops::AccountField, String),
    DeleteAccount(i64, Option<i64>),
    AddCategory(Category, Option<i64>),
    SetCategory(Category, i64, &'static str, String),
    DeleteCategory(Category, i64),
    AddTemplate,
    RenameTemplate(i64, String),
    DeleteTemplate(i64),
    AddTemplateStep(i64),
    SetTemplateStep(i64, steps::StepField, String),
    DeleteTemplateStep(i64),
}

/// A question waiting for an answer before a delete runs.
pub struct Confirm {
    pub title: String,
    pub text: String,
    pub buttons: &'static [&'static str],
    /// Command per button (same order); `None` only closes the box.
    pub then: Vec<Option<Cmd>>,
    /// For a cost account in use: Yes opens the replacement picker.
    pub pick_replacement: Option<i64>,
}

#[derive(Default)]
pub struct DictionaryState {
    pub codes: Option<codes::CodesState>,
    pub udf: Option<udf::UdfState>,
    pub accounts: Option<accounts::AccountsState>,
    pub admin: Option<admin::AdminState>,
    pub steps: Option<steps::StepsState>,
    /// Steps tab > Add from Template while open.
    pub template_pick: Option<steps::TemplatePick>,
    /// The last Cut or Copy in a tree dictionary.
    pub clip: Option<ops::Clip>,
    pub confirm: Option<Confirm>,
    pub notice: Option<String>,
    /// Resource Assignments > Codes: the selected row and the Assign picker.
    pub assignment_code_row: Option<usize>,
    pub assignment_pick: Option<crate::views::assignment_codes::AssignPick>,
    /// Select mode: the choice a dialog made (picker, resource, key), set on
    /// the resource after the dialog is drawn.
    pub chosen: Option<(crate::views::resources::Pick, i64, String)>,
    /// Rectangles of controls drawn this frame (for tests), by action name.
    rects: Vec<(String, Rect)>,
}

impl DictionaryState {
    fn busy(&self) -> bool {
        self.confirm.is_some() || self.notice.is_some()
    }
}

impl App {
    /// A menu item that opens a dictionary. One line per item in the menus.
    pub(crate) fn dictionary_item(&mut self, ui: &mut egui::Ui, which: Dictionary) {
        let mut item = Item::new(which.menu_label());
        item.icon = which.icon();
        if self.item(ui, which.action(), item) {
            self.open_dictionary(which);
        }
    }

    /// A dictionaries toolbar button: it opens a dictionary dialog here, or
    /// an enterprise dialog for the buttons that belong to those.
    pub(crate) fn dictionary_tool(
        &mut self,
        ui: &mut egui::Ui,
        action: &str,
        icon: crate::chrome::Icon,
        tip: &str,
    ) {
        match Dictionary::from_tool(action) {
            Some(which) => {
                if self.tool(ui, action, icon, tip, true) {
                    self.open_dictionary(which);
                }
            }
            None => self.enterprise_tool(ui, action, icon, tip),
        }
    }

    pub(crate) fn open_dictionary(&mut self, which: Dictionary) {
        let st = &mut self.dictionaries;
        st.codes = None;
        st.udf = None;
        st.accounts = None;
        st.admin = None;
        st.steps = None;
        st.confirm = None;
        st.notice = None;
        match which {
            Dictionary::Codes(kind) => {
                let state = codes::CodesState::open(&self.project, kind);
                if let Some(text) = state.empty_notice(&self.project) {
                    self.dictionaries.notice = Some(text);
                }
                self.dictionaries.codes = Some(state);
            }
            Dictionary::Udf => self.dictionaries.udf = Some(udf::UdfState::default()),
            Dictionary::CostAccounts => {
                self.dictionaries.accounts = Some(accounts::AccountsState::default())
            }
            Dictionary::AdminCategories => {
                self.dictionaries.admin = Some(admin::AdminState::default())
            }
            Dictionary::StepTemplates => {
                self.dictionaries.steps = Some(steps::StepsState::default())
            }
        }
    }

    pub(crate) fn dictionary_open(&self) -> bool {
        let st = &self.dictionaries;
        st.codes.is_some()
            || st.udf.is_some()
            || st.accounts.is_some()
            || st.admin.is_some()
            || st.steps.is_some()
    }

    /// Draw whichever dictionary dialog is open, with its message boxes.
    pub(crate) fn dictionary_dialogs(&mut self, ctx: &egui::Context) {
        if !self.dictionary_open() {
            return;
        }
        let mut st = std::mem::take(&mut self.dictionaries);
        st.rects.clear();
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let mut cmds = vec![];
        self.dictionary_messages(ctx, &mut st, &mut cmds, escape);
        // While a message box is up the dialog behind it takes no clicks, so
        // only the message box's buttons are offered as actions.
        let modal = st.busy().then_some(st.rects.len());
        let escape = escape && !st.busy();
        if st.codes.is_some() {
            self.codes_dialog(ctx, &mut st, &mut cmds, escape);
        } else if st.udf.is_some() {
            self.udf_dialog(ctx, &mut st, &mut cmds, escape);
        } else if st.accounts.is_some() {
            self.accounts_dialog(ctx, &mut st, &mut cmds, escape);
        } else if st.admin.is_some() {
            self.admin_dialog(ctx, &mut st, &mut cmds, escape);
        } else if st.steps.is_some() {
            self.steps_dialog(ctx, &mut st, &mut cmds, escape);
        }
        if let Some(keep) = modal {
            st.rects.truncate(keep);
        }
        // Commands read and clear the clipboard through `self`.
        self.dictionaries.clip = st.clip.take();
        for cmd in cmds {
            match self.run_dictionary(cmd) {
                Ok(Some(id)) => st.select(id),
                Ok(None) => {}
                Err(e) => st.notice = Some(e),
            }
        }
        st.clip = self.dictionaries.clip.take();
        for (name, rect) in std::mem::take(&mut st.rects) {
            self.action_rects
                .insert(name, [rect.left(), rect.top(), rect.right(), rect.bottom()]);
        }
        let chosen = st.chosen.take();
        self.dictionaries = st;
        self.after_codes_dictionary();
        if let Some((pick, rsrc, key)) = chosen {
            self.apply_resource_pick(pick, rsrc, key);
        }
    }

    fn dictionary_messages(
        &mut self,
        ctx: &egui::Context,
        st: &mut DictionaryState,
        cmds: &mut Vec<Cmd>,
        escape: bool,
    ) {
        if let Some(text) = st.notice.clone() {
            let title = st.title();
            let answer = kit::message_box(
                ctx,
                "dictionary_notice",
                &title,
                kit::MessageIcon::Info,
                &text,
                &["OK"],
                Some(0),
                &mut st.rects,
            );
            if answer.is_some() || escape {
                st.notice = None;
            }
            return;
        }
        let Some(confirm) = &st.confirm else {
            return;
        };
        let answer = kit::message_box(
            ctx,
            "dictionary_confirm",
            &confirm.title,
            kit::MessageIcon::Question,
            &confirm.text,
            confirm.buttons,
            kit::Question::DictionaryDelete.default_in(confirm.buttons),
            &mut st.rects,
        );
        let answer = match answer {
            Some(a) => a,
            // Escape answers No (or Cancel when there is one).
            None if escape => confirm.buttons.len() - 1,
            None => return,
        };
        let confirm = st.confirm.take().unwrap();
        if answer == 0 {
            if let Some(id) = confirm.pick_replacement {
                if let Some(a) = st.accounts.as_mut() {
                    a.replace = Some((id, None));
                }
                return;
            }
        }
        if let Some(Some(cmd)) = confirm.then.get(answer) {
            cmds.push(cmd.clone());
        }
    }

    /// Run one edit; on success it becomes one undo step. Returns the id the
    /// dialog should select afterwards.
    pub(crate) fn run_dictionary(&mut self, cmd: Cmd) -> ops::Result<Option<i64>> {
        // A baseline type is used by baselines kept in the workspace; their
        // change goes into the same undo step.
        if let Cmd::DeleteCategory(Category::Baseline, id) = cmd {
            return self.delete_baseline_type(id);
        }
        let pid = self.project.model.project.proj_id;
        // EPS codes go under the workspace EPS node of the project.
        let active = self.open_set.active;
        let eps = Some(self.workspace().node_of(active));
        let doc = &mut self.project.document;
        let mut rename: Option<(String, String)> = None;
        let selected = match cmd {
            Cmd::AddType(kind, scope) => {
                Some(ops::add_code_type_at(doc, kind, scope, pid, eps, None)?)
            }
            Cmd::RenameType(kind, id, name) => {
                let old = ops::code_type(doc, kind, id).map(|t| t.name);
                if old.as_deref() == Some(name.trim()) {
                    return Ok(None);
                }
                ops::rename_code_type(doc, kind, id, &name)?;
                if let (CodeKind::Activity, Some(old)) = (kind, old) {
                    rename = Some((format!("code:{old}"), format!("code:{}", name.trim())));
                }
                None
            }
            Cmd::TypeLength(kind, id, text) => {
                let len = text
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "Max Length must be a whole number.".to_string())?;
                if ops::code_type(doc, kind, id).is_some_and(|t| t.max_len == len) {
                    return Ok(None);
                }
                ops::set_code_length(doc, kind, id, len)?;
                None
            }
            Cmd::DeleteType(kind, id) => {
                ops::delete_code_type(doc, kind, id);
                None
            }
            Cmd::ShiftType(kind, scope, id, by) => {
                if !ops::shift_code_type(doc, kind, scope, pid, id, by) {
                    return Ok(None);
                }
                None
            }
            Cmd::WidenType(kind, id, scope) => {
                ops::set_code_scope_at(doc, kind, id, scope, pid, eps)?;
                None
            }
            Cmd::AddValue(kind, type_id, after) => {
                Some(ops::add_code_value(doc, kind, type_id, after)?)
            }
            Cmd::SetValue(kind, type_id, id, field, text) => {
                let current = ops::code_values(doc, kind, type_id)
                    .into_iter()
                    .find(|v| v.id == id)
                    .map(|v| match field {
                        ops::ValueField::Short => v.short,
                        ops::ValueField::Name => v.name,
                        ops::ValueField::Color => v.color,
                    });
                if current.as_deref() == Some(text.trim()) {
                    return Ok(None);
                }
                ops::set_code_value(doc, kind, type_id, id, field, &text)?;
                None
            }
            Cmd::DeleteValue(kind, type_id, id) => {
                ops::delete_code_value(doc, kind, type_id, id);
                None
            }
            Cmd::Move(spec, owner, id, dir) => {
                let changed = match dir {
                    Move::Up => ops::shift(doc, &spec, owner, id, -1),
                    Move::Down => ops::shift(doc, &spec, owner, id, 1),
                    Move::Left => ops::outdent(doc, &spec, owner, id)?,
                    Move::Right => ops::indent(doc, &spec, owner, id)?,
                };
                if !changed {
                    return Ok(None);
                }
                Some(id)
            }
            Cmd::Paste(spec, owner, after) => {
                let Some(clip) = self.dictionaries_clip() else {
                    return Ok(None);
                };
                let mut spec = spec;
                if let Some(owner) = owner.filter(|_| spec.owner.is_some()) {
                    // Code values: keep the type's length limit.
                    for kind in [
                        CodeKind::Activity,
                        CodeKind::Project,
                        CodeKind::Resource,
                        CodeKind::Assignment,
                    ] {
                        if kind.tables().value.table == spec.table {
                            spec = ops::value_spec(&self.project.document, kind, owner);
                        }
                    }
                }
                let doc = &mut self.project.document;
                let ids = ops::paste(doc, &spec, owner, after, &clip)?;
                if clip.cut {
                    self.dictionaries.clip = None;
                }
                ids.first().copied()
            }
            Cmd::AddUdf(table) => Some(ops::add_udf(doc, table)),
            Cmd::UdfTitle(id, title) => {
                let table = ops::udfs(doc, "TASK");
                let old = table.iter().find(|u| u.id == id).map(|u| u.title.clone());
                let current = ops::SUBJECT_AREAS
                    .iter()
                    .flat_map(|(_, t)| ops::udfs(doc, t))
                    .find(|u| u.id == id)
                    .map(|u| u.title);
                if current.as_deref() == Some(title.trim()) {
                    return Ok(None);
                }
                ops::set_udf_title(doc, id, &title)?;
                if let Some(old) = old {
                    rename = Some((format!("udf:{old}"), format!("udf:{}", title.trim())));
                }
                None
            }
            Cmd::UdfType(id, logical) => {
                ops::set_udf_type(doc, id, logical)?;
                None
            }
            Cmd::DeleteUdf(id) => {
                ops::delete_udf(doc, id);
                None
            }
            Cmd::AddAccount(after) => Some(ops::add_account(doc, after)?),
            Cmd::SetAccount(id, field, text) => {
                let current = match field {
                    ops::AccountField::Id => ops::value(doc, &ops::ACCOUNT, id, "acct_short_name"),
                    ops::AccountField::Name => ops::value(doc, &ops::ACCOUNT, id, "acct_name"),
                    ops::AccountField::Description => ops::account_description(doc, id),
                };
                let same = if field == ops::AccountField::Description {
                    current == text
                } else {
                    current == text.trim()
                };
                if same {
                    return Ok(None);
                }
                ops::set_account(doc, id, field, &text)?;
                None
            }
            Cmd::DeleteAccount(id, replacement) => {
                ops::delete_account(doc, id, replacement)?;
                None
            }
            Cmd::AddCategory(category, after) => ops::add_category(doc, category, after)?,
            Cmd::SetCategory(category, id, column, text) => {
                let current = ops::value(doc, &category.spec().tree, id, column);
                if current == text.trim() {
                    return Ok(None);
                }
                ops::set_category(doc, category, id, column, &text)?;
                None
            }
            Cmd::DeleteCategory(category, id) => {
                ops::delete_category(doc, category, id);
                None
            }
            Cmd::AddTemplate => Some(steps::add_template(doc, None)?),
            Cmd::RenameTemplate(id, name) => {
                let same = steps::templates(doc)
                    .iter()
                    .any(|t| t.id == id && t.name == name.trim());
                if same {
                    return Ok(None);
                }
                steps::rename_template(doc, id, &name)?;
                None
            }
            Cmd::DeleteTemplate(id) => {
                steps::delete_template(doc, id);
                None
            }
            Cmd::AddTemplateStep(group) => {
                steps::add_template_step(doc, group, "New Step", 1., "");
                Some(group)
            }
            Cmd::SetTemplateStep(id, field, text) => {
                let same = steps::templates(doc)
                    .iter()
                    .flat_map(|t| steps::template_steps(doc, t.id))
                    .find(|s| s.id == id)
                    .is_some_and(|s| match field {
                        steps::StepField::Name => s.name == text.trim(),
                        steps::StepField::Weight => {
                            text.trim().parse::<f64>().ok() == Some(s.weight)
                        }
                    });
                if same {
                    return Ok(None);
                }
                steps::set_template_step(doc, id, field, &text)?;
                None
            }
            Cmd::DeleteTemplateStep(id) => {
                steps::delete_template_step(doc, id);
                None
            }
        };
        if let Some((old, new)) = rename {
            self.rename_layout_column(&old, &new);
        }
        self.touch_records();
        Ok(selected)
    }

    fn dictionaries_clip(&self) -> Option<ops::Clip> {
        self.dictionaries.clip.clone()
    }

    /// Keep a renamed code or field (the grid names those columns by title)
    /// in every layout that uses it: the working layout and each saved
    /// layout (columns, groups, sorts and the bottom pane's columns kept
    /// with them), the bottom table now shown, and the user filters.
    fn rename_layout_column(&mut self, old: &str, new: &str) {
        rename_in_layout(&mut self.grid.layout, old, new);
        let mut library = false;
        for l in &mut self.grid.library {
            library |= rename_in_layout(l, old, new);
        }
        if library {
            self.save_library();
        }
        for c in &mut self.panes.grid.columns {
            if c.id == old {
                c.id = new.to_string();
            }
        }
        let mut filters = false;
        for f in &mut self.grid.user_filters {
            for r in &mut f.rows {
                if r.column == old {
                    r.column = new.to_string();
                    filters = true;
                }
            }
        }
        if filters {
            self.save_user_filters();
        }
    }
}

/// Rename column `old` to `new` in one layout; true when anything changed.
fn rename_in_layout(layout: &mut crate::grid::layout::Layout, old: &str, new: &str) -> bool {
    let mut changed = false;
    let mut swap = |id: &mut String| {
        if id == old {
            *id = new.to_string();
            changed = true;
        }
    };
    for c in &mut layout.columns {
        swap(&mut c.id);
    }
    for g in &mut layout.group_sort.levels {
        swap(&mut g.column);
    }
    for s in &mut layout.group_sort.sort {
        swap(&mut s.column);
    }
    // The bottom table's columns saved with the layout's pane options.
    if let Some(columns) = layout
        .panes
        .as_mut()
        .and_then(|p| p.get_mut("bottom_columns"))
        .and_then(|c| c.as_array_mut())
    {
        for id in columns.iter_mut().filter_map(|c| c.get_mut("id")) {
            if id.as_str() == Some(old) {
                *id = serde_json::Value::String(new.to_string());
                changed = true;
            }
        }
    }
    changed
}

impl DictionaryState {
    fn title(&self) -> String {
        if let Some(c) = &self.codes {
            c.kind.title().into()
        } else if self.udf.is_some() {
            "User Defined Fields".into()
        } else if self.accounts.is_some() {
            "Cost Accounts".into()
        } else if self.steps.is_some() {
            "Activity Step Templates".into()
        } else {
            "Categories and Lists".into()
        }
    }
    fn select(&mut self, id: i64) {
        if let Some(c) = self.codes.as_mut() {
            if c.modify.is_some() {
                c.modify_selected = Some(id);
            } else {
                c.selected = Some(id);
            }
        } else if let Some(u) = self.udf.as_mut() {
            u.selected = Some(id);
        } else if let Some(a) = self.accounts.as_mut() {
            a.selected = Some(id);
        } else if let Some(a) = self.admin.as_mut() {
            a.selected = Some(id);
        } else if let Some(s) = self.steps.as_mut() {
            s.selected = Some(id);
        }
    }
    fn rect(&mut self, name: impl Into<String>, rect: Rect) {
        self.rects.push((name.into(), rect));
    }
}

/// Grid id that keeps the window's sizing pass from fixing column widths.
fn grid_id(ui: &egui::Ui, id: &str) -> String {
    if ui.is_sizing_pass() {
        format!("{id}_sizing")
    } else {
        id.to_string()
    }
}

/// Draw a text cell that turns into an editor when `edit` targets it.
/// Returns the committed text when the editor loses focus.
fn editable_cell(
    ui: &mut egui::Ui,
    edit: &mut Option<CellEdit>,
    id: i64,
    column: usize,
    shown: &str,
) -> Option<String> {
    match edit {
        Some(e) if e.id == id && e.column == column => {
            crate::details::editor_cell(ui);
            let r = ui.add(
                egui::TextEdit::singleline(&mut e.text)
                    .id(egui::Id::new(("dictionary_cell_edit", id, column)))
                    .desired_width(f32::INFINITY)
                    .frame(false),
            );
            crate::dialog_keys::note_keeps_enter(&r);
            if std::mem::take(&mut e.focus) {
                r.request_focus();
            }
            if r.lost_focus() {
                let escape = ui.input(|i| i.key_pressed(Key::Escape));
                let text = e.text.clone();
                *edit = None;
                if !escape {
                    return Some(text);
                }
            }
            None
        }
        _ => {
            cell(ui, shown);
            None
        }
    }
}

/// The cell a click on the selected row landed in, from the cell rectangles
/// recorded while drawing.
fn clicked_column(ui: &egui::Ui, cells: &[(usize, usize, Rect)], row: usize) -> Option<usize> {
    let pos = ui.input(|i| i.pointer.interact_pos())?;
    cells
        .iter()
        .find(|(r, _, rect)| *r == row && rect.contains(pos))
        .map(|(_, c, _)| *c)
}

/// A dialog body in a 5 px margin with the kit's right-hand button column
/// beside it: [`kit::strip`] and [`kit::button_column`] on a Canvas as tall
/// as the body. Returns the body's value, what the column reported and the
/// column's control rectangles (by the entries' action names).
fn with_column<R>(
    ui: &mut egui::Ui,
    salt: &'static str,
    entries: &[Entry],
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, kit::Column, Vec<(String, Rect)>) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let body = ui.vertical(|ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(5))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4., 3.);
                    body(ui)
                })
                .inner
        });
        let height = body.response.rect.height();
        let (column, rects) =
            kit::flow_canvas(ui, salt, Vec2::new(kit::STRIP_WIDTH, height), |c| {
                kit::strip(c, kit::STRIP_WIDTH, height);
                kit::button_column(c, kit::STRIP_WIDTH, entries)
            });
        (body.inner, column, rects)
    })
    .inner
}

/// A section heading across a flow body: [`kit::heading`].
fn heading(ui: &mut egui::Ui, salt: &'static str, width: f32, title: &str) {
    kit::flow_canvas(ui, salt, Vec2::new(width, 21.), |c| {
        kit::heading(c, 0., 0., width, title)
    });
}

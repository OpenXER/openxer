//! The Report Editor: the report as an outline of its page header and footer,
//! sections, data sources, rows and cells, with the commands to add a row, a
//! text, line or image cell and a data source, to copy, paste, move and
//! delete, and Properties. This editor is form-based: it changes the
//! definition through the outline and the Properties dialog rather than on
//! a drawing canvas. Every change can be undone.
use super::rpt::edit::{self, CellSpec, Kind, NewCell, Path};
use super::rpt::model::Report;
use super::rpt::store;
use super::rpt::stream::{Component, Stream};
use super::rpt_props::{Props, PropsResult};
use crate::dialog_kit as kit;
use crate::App;
use eframe::egui::{self, Key, Vec2};

/// Undo steps kept.
const UNDO_LIMIT: usize = 200;
/// Indent of one outline level.
const INDENT: f32 = 14.;

pub struct Editor {
    pub id: i64,
    pub name: String,
    pub stream: Stream,
    pub undo: Vec<Stream>,
    pub redo: Vec<Stream>,
    /// Selected outline row.
    pub selected: usize,
    pub dirty: bool,
    pub error: Option<String>,
    pub note: Option<String>,
    pub clip: Option<Component>,
    pub props: Option<Props>,
}

impl Editor {
    pub fn new(id: i64, name: String, stream: Stream) -> Editor {
        Editor {
            id,
            name,
            stream,
            undo: vec![],
            redo: vec![],
            selected: 0,
            dirty: false,
            error: None,
            note: None,
            clip: None,
            props: None,
        }
    }

    /// Runs `f` on the definition as one undo step; on an error nothing
    /// changes.
    pub fn step<R>(&mut self, f: impl FnOnce(&mut Stream) -> Result<R, String>) -> Option<R> {
        let before = self.stream.clone();
        match f(&mut self.stream) {
            Ok(r) => {
                self.undo.push(before);
                if self.undo.len() > UNDO_LIMIT {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.dirty = true;
                self.error = None;
                Some(r)
            }
            Err(e) => {
                self.stream = before;
                self.error = Some(e);
                None
            }
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.stream, prev));
            self.dirty = true;
            self.fix_selection();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.stream, next));
            self.dirty = true;
            self.fix_selection();
        }
    }

    fn fix_selection(&mut self) {
        let n = edit::outline(&self.stream).len();
        self.selected = self.selected.min(n.saturating_sub(1));
    }

    pub fn outline(&self) -> Vec<(Path, usize, String)> {
        edit::outline(&self.stream)
    }

    pub fn selected_path(&self) -> Path {
        self.outline()
            .get(self.selected)
            .map(|(p, _, _)| p.clone())
            .unwrap_or_default()
    }

    /// Selects the outline row of `path`.
    pub fn select(&mut self, path: &[usize]) {
        if let Some(i) = self.outline().iter().position(|(p, _, _)| p == path) {
            self.selected = i;
        }
    }

    fn kind_at(&self, path: &[usize]) -> Option<Kind> {
        edit::get(&self.stream.root, path).map(edit::kind)
    }

    /// The row holder (header, footer or section) a new row goes in.
    fn holder_for_row(&self) -> Option<Path> {
        let mut at = self.selected_path();
        loop {
            match self.kind_at(&at)? {
                Kind::ReportHeader
                | Kind::PageHeader
                | Kind::PageFooter
                | Kind::ReportFooter
                | Kind::GroupHeader
                | Kind::GroupFooter
                | Kind::Detail => return Some(at),
                Kind::Report => return edit::container_path(&self.stream, Kind::PageHeader),
                _ => {
                    at.pop();
                }
            }
        }
    }

    /// The row a new cell goes in.
    fn row_for_cell(&self) -> Option<Path> {
        let mut at = self.selected_path();
        loop {
            match self.kind_at(&at)? {
                Kind::Row => return Some(at),
                Kind::Text | Kind::Line | Kind::Image => {
                    at.pop();
                }
                _ => return None,
            }
        }
    }

    /// The section a new data source goes in.
    fn section_for_source(&self) -> Option<Path> {
        let mut at = self.selected_path();
        loop {
            match self.kind_at(&at)? {
                Kind::Detail => return Some(at),
                Kind::Report => return edit::container_path(&self.stream, Kind::Detail),
                Kind::Group => {
                    return edit::group_part(&self.stream, &at, Kind::Detail);
                }
                _ => {
                    at.pop();
                }
            }
        }
    }

    pub fn add_row(&mut self) {
        let Some(holder) = self.holder_for_row() else {
            self.error = Some("Select a header, footer or section first.".into());
            return;
        };
        if let Some(path) = self.step(|s| edit::add_row(s, &holder, 20)) {
            self.select(&path);
        }
    }

    pub fn add_cell(&mut self, what: NewCell, width: i64) {
        let Some(row) = self.row_for_cell() else {
            self.error = Some("Select a row first.".into());
            return;
        };
        // After the last cell of the row.
        let left = edit::get(&self.stream.root, &row)
            .map(|r| {
                r.children()
                    .map(|c| c.int("Left").unwrap_or(0) + c.int("Width").unwrap_or(0))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let spec = CellSpec::new(what, left, width);
        if let Some(path) = self.step(|s| edit::add_cell(s, &row, &spec)) {
            self.select(&path);
        }
    }

    pub fn add_source(&mut self) {
        let Some(section) = self.section_for_source() else {
            self.error = Some("Select a section first.".into());
            return;
        };
        // A source inside another source is joined to its record.
        let nested = section.len() > 1;
        let (table, join) = if nested {
            ("TASK", Some("TASK"))
        } else {
            ("TASK", None)
        };
        if let Some(path) = self.step(|s| edit::add_group(s, &section, table, join, false)) {
            self.select(&path);
        }
    }

    pub fn delete_selected(&mut self) {
        let path = self.selected_path();
        if self.step(|s| edit::delete(s, &path)).is_some() {
            self.fix_selection();
        }
    }

    pub fn copy_selected(&mut self) {
        let path = self.selected_path();
        match edit::get(&self.stream.root, &path) {
            Some(c)
                if matches!(
                    edit::kind(c),
                    Kind::Row | Kind::Group | Kind::Text | Kind::Line | Kind::Image
                ) =>
            {
                self.clip = Some(c.clone());
                self.note = Some("Copied".into());
            }
            _ => self.error = Some("Select a row, cell or data source to copy.".into()),
        }
    }

    pub fn paste(&mut self) {
        let Some(clip) = self.clip.clone() else {
            self.error = Some("Copy something first.".into());
            return;
        };
        // Into the selection, or the place it belongs for the copy's kind.
        let mut at = self.selected_path();
        let want = |k: Kind| match edit::kind(&clip) {
            Kind::Row => matches!(
                k,
                Kind::ReportHeader
                    | Kind::PageHeader
                    | Kind::PageFooter
                    | Kind::ReportFooter
                    | Kind::GroupHeader
                    | Kind::GroupFooter
                    | Kind::Detail
            ),
            Kind::Group => k == Kind::Detail,
            _ => k == Kind::Row,
        };
        loop {
            match self.kind_at(&at) {
                Some(k) if want(k) => break,
                Some(_) if !at.is_empty() => {
                    at.pop();
                }
                _ => {
                    self.error = Some("That cannot be pasted here.".into());
                    return;
                }
            }
        }
        if let Some(path) = self.step(|s| edit::paste(s, &at, &clip)) {
            self.select(&path);
        }
    }

    pub fn shift_selected(&mut self, by: isize) {
        let path = self.selected_path();
        if let Some(p) = self.step(|s| edit::shift(s, &path, by)) {
            self.select(&p);
        }
    }

    /// Replaces the definition (Properties OK) as one undo step.
    pub fn replace(&mut self, stream: Stream) {
        if stream != self.stream {
            self.step(|s| {
                *s = stream;
                Ok(())
            });
        }
    }

    /// Saves to the workspace.
    pub fn save(&mut self, app: &App) -> Result<(), String> {
        Report::from_stream(&self.stream)?;
        let db = app.store.connection();
        store::update_data(db, self.id, &self.stream.to_bytes())?;
        let name = store::name_of(&self.stream);
        if !name.is_empty() {
            store::rename(db, self.id, &name)?;
            self.name = name;
        }
        self.dirty = false;
        Ok(())
    }
}

impl App {
    pub(crate) fn open_report_editor(&mut self, id: i64) {
        let name = self
            .stored_reports()
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.name)
            .unwrap_or_default();
        let loaded =
            store::load(self.store.connection(), id).and_then(|bytes| Stream::parse_bytes(&bytes));
        match loaded.and_then(|s| Report::from_stream(&s).map(|_| s)) {
            Ok(stream) => self.reports.editor = Some(Editor::new(id, name, stream)),
            Err(e) => self.error(format!("This report cannot be edited: {e}")),
        }
    }

    /// Report Editor.
    pub(crate) fn report_editor_window(&mut self, ctx: &egui::Context) {
        let Some(mut ed) = self.reports.editor.take() else {
            return;
        };
        // Properties sits on top and takes Escape first.
        if let Some(p) = ed.props.take() {
            match self.report_properties(ctx, p) {
                PropsResult::Open(p) => ed.props = Some(*p),
                PropsResult::Done(stream) => ed.replace(*stream),
                PropsResult::Cancelled => {}
            }
        }
        let props_open = ed.props.is_some();
        if self.reports.take_escape() {
            return;
        }
        const W: f32 = 780.;
        // Tall enough for the standard button column of 18 buttons.
        const H: f32 = 600.;
        let outline = ed.outline();
        ed.selected = ed.selected.min(outline.len().saturating_sub(1));
        #[derive(Default)]
        struct Clicks {
            ok: bool,
            cancel: bool,
            save: bool,
            row: bool,
            text: bool,
            line: bool,
            image: bool,
            source: bool,
            props: bool,
            copy: bool,
            paste: bool,
            delete: bool,
            up: bool,
            down: bool,
            undo: bool,
            redo: bool,
            preview: bool,
            help: bool,
            clicked: Option<usize>,
        }
        let sel_kind = outline
            .get(ed.selected)
            .and_then(|(p, _, _)| edit::get(&ed.stream.root, p))
            .map(edit::kind);
        let copyable = matches!(
            sel_kind,
            Some(Kind::Row | Kind::Group | Kind::Text | Kind::Line | Kind::Image)
        );
        let (k, closed, actions) = kit::window(
            ctx,
            "report_editor",
            "Report Editor",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                let out = kit::list(
                    c,
                    "report_editor_rows",
                    (6., 6., W - kit::STRIP_WIDTH - 12., H - 40.),
                    Some("Display: Report"),
                    &[kit::Col {
                        title: "Report Layout",
                        width: W - kit::STRIP_WIDTH - 34.,
                    }],
                    outline.len(),
                    Some(ed.selected),
                    Some(0),
                    &mut |c, i, _col, rect, s| {
                        let (_, depth, text) = &outline[i];
                        kit::cell_text(c, rect, text, s, 3. + INDENT * *depth as f32);
                    },
                );
                k.clicked = out.clicked;
                let status = ed
                    .error
                    .clone()
                    .or_else(|| ed.note.clone())
                    .unwrap_or_else(|| {
                        if ed.dirty {
                            "Changed".to_string()
                        } else {
                            String::new()
                        }
                    });
                kit::text(c, 8., H - 24., &status, true);
                kit::strip(c, W, H);
                use kit::Entry::Button as B;
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::ok("report_editor_ok"),
                        kit::Entry::cancel("report_editor_cancel"),
                        B("report_editor_save", "Save", ed.dirty, false),
                        B("report_editor_add_row", "Add Row", true, true),
                        B("report_editor_add_text", "Add Text Cell", true, false),
                        B("report_editor_add_line", "Add Line Cell", true, false),
                        B("report_editor_add_image", "Add Image Cell", true, false),
                        B("report_editor_add_source", "Add Data Source", true, false),
                        B("report_editor_properties", "Properties...", true, true),
                        B("report_editor_copy", "Copy", copyable, false),
                        B("report_editor_paste", "Paste", ed.clip.is_some(), false),
                        B("report_editor_delete", "Delete", copyable, false),
                        B("report_editor_up", "Move Up", copyable, false),
                        B("report_editor_down", "Move Down", copyable, false),
                        B("report_editor_undo", "Undo", !ed.undo.is_empty(), true),
                        B("report_editor_redo", "Redo", !ed.redo.is_empty(), false),
                        B("report_editor_preview", "Preview...", true, true),
                        kit::Entry::help("report_editor_help"),
                    ],
                );
                match col.clicked.as_deref() {
                    Some("report_editor_ok") => k.ok = true,
                    Some("report_editor_cancel") => k.cancel = true,
                    Some("report_editor_save") => k.save = true,
                    Some("report_editor_add_row") => k.row = true,
                    Some("report_editor_add_text") => k.text = true,
                    Some("report_editor_add_line") => k.line = true,
                    Some("report_editor_add_image") => k.image = true,
                    Some("report_editor_add_source") => k.source = true,
                    Some("report_editor_properties") => k.props = true,
                    Some("report_editor_copy") => k.copy = true,
                    Some("report_editor_paste") => k.paste = true,
                    Some("report_editor_delete") => k.delete = true,
                    Some("report_editor_up") => k.up = true,
                    Some("report_editor_down") => k.down = true,
                    Some("report_editor_undo") => k.undo = true,
                    Some("report_editor_redo") => k.redo = true,
                    Some("report_editor_preview") => k.preview = true,
                    Some("report_editor_help") => k.help = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.cancel || closed {
            return;
        }
        // Keys that belong to the editor while no child dialog is open.
        let (ctrl_z, ctrl_y, del) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(Key::Z),
                i.modifiers.command && i.key_pressed(Key::Y),
                false,
            )
        });
        let _ = del;
        if let Some(i) = k.clicked {
            ed.selected = i.min(outline.len().saturating_sub(1));
            ed.error = None;
            ed.note = None;
        }
        if !props_open {
            if k.row {
                ed.add_row();
            }
            if k.text {
                ed.add_cell(NewCell::Text("Text".into()), 100);
            }
            if k.line {
                ed.add_cell(NewCell::Line, 100);
            }
            if k.image {
                ed.add_cell(NewCell::Image(String::new()), 100);
            }
            if k.source {
                ed.add_source();
            }
            if k.copy {
                ed.copy_selected();
            }
            if k.paste {
                ed.paste();
            }
            if k.delete {
                ed.delete_selected();
            }
            if k.up {
                ed.shift_selected(-1);
            }
            if k.down {
                ed.shift_selected(1);
            }
            if k.undo || ctrl_z {
                ed.undo();
            }
            if k.redo || ctrl_y {
                ed.redo();
            }
            if k.props {
                let path = ed.selected_path();
                ed.props = Some(Props::new(&ed.stream, path));
            }
        }
        if k.help {
            self.open_help_for("report_editor");
        }
        if k.preview {
            let ix = self.grid_index();
            match super::rpt::generate(&ed.stream, &self.project, &ix, &self.run_options("")) {
                Ok(g) => {
                    self.reports.page_preview =
                        Some(super::rpt_run::PagePreview::new(&g, self.viewer_start()));
                }
                Err(e) => ed.error = Some(e),
            }
        }
        if k.save || k.ok {
            match ed.save(self) {
                Ok(()) => {
                    ed.note = Some("Saved".into());
                    if k.ok {
                        self.status = format!("Saved report {}", ed.name);
                        return;
                    }
                }
                Err(e) => ed.error = Some(e),
            }
        }
        self.reports.editor = Some(ed);
    }
}

//! Import and Export of report definitions: a definition file, or the
//! reports of a copy of the reference database (chosen from a list), and
//! export of a report to a definition file.
use super::doc::documents;
use super::groups;
use super::rpt::store::{self, Reference};
use crate::dialog_kit as kit;
use crate::App;
use eframe::egui::{self, Align, Vec2};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub struct ImportDialog {
    pub group: Option<u32>,
    /// Import from a database copy rather than a definition file.
    pub from_db: bool,
    pub path: String,
    pub error: Option<String>,
    /// The database's reports once it is opened.
    pub db: Option<DbList>,
}

pub struct DbList {
    pub reference: Reference,
    pub checked: BTreeSet<i64>,
    pub show_all: bool,
    pub selected: Option<usize>,
}

impl DbList {
    /// Reports shown: the user-made type, or all of them.
    pub fn shown(&self) -> Vec<usize> {
        self.reference
            .reports
            .iter()
            .enumerate()
            .filter(|(_, r)| self.show_all || r.kind == store::REFERENCE_USER_TYPE)
            .map(|(i, _)| i)
            .collect()
    }
}

pub struct ExportDialog {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub error: Option<String>,
}

impl App {
    pub(crate) fn open_report_import(&mut self, group: Option<u32>) {
        self.reports.import = Some(ImportDialog {
            group,
            from_db: false,
            path: String::new(),
            error: None,
            db: None,
        });
    }

    pub(crate) fn open_report_export(&mut self, id: i64) {
        let name = self
            .stored_reports()
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.name)
            .unwrap_or_default();
        let path = documents()
            .join(format!("{}.rpt", groups::stem_of(&name)))
            .display()
            .to_string();
        self.reports.export = Some(ExportDialog {
            id,
            name,
            path,
            error: None,
        });
    }

    /// Imports a definition file as a new report; returns its name.
    pub(crate) fn import_definition_file(
        &mut self,
        path: &Path,
        group: Option<u32>,
    ) -> Result<String, String> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("{} could not be read: {e}", path.display()))?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let id = store::import_file(self.store.connection(), &stem, &bytes, group)
            .map_err(|e| format!("{} is not a report definition: {e}", path.display()))?;
        Ok(self
            .stored_reports()
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.name)
            .unwrap_or_default())
    }

    /// Imports the chosen reports of a database copy with their groups and
    /// batches.
    pub(crate) fn import_reference(
        &mut self,
        reference: &Reference,
        chosen: &[i64],
    ) -> Result<store::Imported, String> {
        let mut tree = self.report_groups();
        let done = store::import_reference(self.store.connection(), reference, chosen, &mut tree)?;
        if done.groups > 0 {
            self.save_report_groups(&tree);
        }
        Ok(done)
    }

    /// Writes a stored report to a definition file.
    pub(crate) fn export_stored(&mut self, id: i64, path: &Path) -> Result<(), String> {
        let bytes = store::export(self.store.connection(), id)?;
        super::rpt_run::write_file(path, &bytes)
    }

    pub(crate) fn report_import_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.import.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        if d.db.is_some() {
            self.report_database_list(ctx, d);
            return;
        }
        const W: f32 = 520.;
        const H: f32 = 210.;
        #[derive(Default)]
        struct Clicks {
            cancel: bool,
            ok: bool,
            browse: bool,
            help: bool,
            pick: Option<bool>,
        }
        let (k, closed, actions) = kit::window(
            ctx,
            "import_reports",
            "Import Reports",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                kit::section(c, 12., 10., 380., 72., "Import From");
                if kit::radio(
                    c,
                    "import_reports_file",
                    24.,
                    34.,
                    350.,
                    20.,
                    "A report &definition file",
                    !d.from_db,
                    true,
                ) {
                    k.pick = Some(false);
                }
                if kit::radio(
                    c,
                    "import_reports_db",
                    24.,
                    56.,
                    350.,
                    20.,
                    "The reports in a &database copy",
                    d.from_db,
                    true,
                ) {
                    k.pick = Some(true);
                }
                kit::text(c, 14., 88., "File", true);
                c.edit(
                    "import_reports_path",
                    14.,
                    104.,
                    W - 140.,
                    21.,
                    &mut d.path,
                    true,
                    Align::Min,
                );
                let field = c.r(14., 104., W - 140., 21.);
                k.browse = c.ellipsis("import_reports_browse", field, true).clicked();
                if let Some(e) = &d.error {
                    kit::text(c, 14., 140., e, true);
                }
                kit::strip(c, W, H);
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::Button(
                            "import_reports_ok",
                            "OK",
                            !d.path.trim().is_empty(),
                            false,
                        ),
                        kit::Entry::cancel("import_reports_cancel"),
                        kit::Entry::help("import_reports_help"),
                    ],
                );
                match col.clicked.as_deref() {
                    Some("import_reports_ok") => k.ok = true,
                    Some("import_reports_cancel") => k.cancel = true,
                    Some("import_reports_help") => k.help = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.cancel || closed {
            return;
        }
        if let Some(db) = k.pick {
            d.from_db = db;
        }
        if k.browse {
            let mut dialog = rfd::FileDialog::new();
            dialog = if d.from_db {
                dialog.add_filter("Database", &["db", "sqlite", "sqlite3"])
            } else {
                dialog.add_filter("Report definition", &["rpt", "txt"])
            };
            if let Some(f) = dialog.pick_file() {
                d.path = f.display().to_string();
            }
        }
        if k.help {
            self.open_help_for("import_reports");
        }
        if k.ok {
            let path = PathBuf::from(d.path.trim());
            if d.from_db {
                match store::read_reference(&path) {
                    Ok(reference) => {
                        let shown = reference
                            .reports
                            .iter()
                            .any(|r| r.kind == store::REFERENCE_USER_TYPE);
                        d.db = Some(DbList {
                            checked: BTreeSet::new(),
                            show_all: !shown,
                            selected: None,
                            reference,
                        });
                        d.error = None;
                    }
                    Err(e) => d.error = Some(e),
                }
            } else {
                match self.import_definition_file(&path, d.group) {
                    Ok(name) => {
                        self.status = format!("Imported report {name}");
                        return;
                    }
                    Err(e) => d.error = Some(e),
                }
            }
        }
        self.reports.import = Some(d);
    }

    /// The reports of a database copy, to pick from.
    fn report_database_list(&mut self, ctx: &egui::Context, mut d: ImportDialog) {
        const W: f32 = 640.;
        const H: f32 = 420.;
        #[derive(Default)]
        struct Clicks {
            cancel: bool,
            import: bool,
            toggle: Option<usize>,
            all: bool,
            none: bool,
            help: bool,
        }
        let Some(mut db) = d.db.take() else { return };
        let shown = db.shown();
        let mut show_all = db.show_all;
        let (k, closed, actions) = kit::window(
            ctx,
            "import_database",
            "Import Reports from a Database",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                let origin = c.origin;
                let out = kit::list(
                    c,
                    "import_database_rows",
                    (6., 6., W - 124., H - 42.),
                    Some("Display: Reports"),
                    &[
                        kit::Col {
                            title: "Report Name",
                            width: 330.,
                        },
                        kit::Col {
                            title: "Type",
                            width: 110.,
                        },
                    ],
                    shown.len(),
                    db.selected,
                    None,
                    &mut |c, i, col, rect, s| {
                        let r = &db.reference.reports[shown[i]];
                        if col == 0 {
                            kit::check_glyph(
                                c,
                                rect.left() - origin.x + 3.,
                                rect.top() - origin.y + 2.,
                                db.checked.contains(&r.id),
                                true,
                            );
                        }
                        let text = if col == 0 {
                            r.name.clone()
                        } else {
                            r.kind.clone()
                        };
                        kit::cell_text(c, rect, &text, s, if col == 0 { 20. } else { 3. });
                    },
                );
                k.toggle = out.clicked;
                c.checkbox(
                    "import_database_all_types",
                    8.,
                    H - 28.,
                    300.,
                    18.,
                    "Show reports of every type",
                    &mut show_all,
                    true,
                );
                kit::strip(c, W, H);
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::Button(
                            "import_database_import",
                            "Import",
                            !db.checked.is_empty(),
                            false,
                        ),
                        kit::Entry::cancel("import_database_cancel"),
                        kit::Entry::Button(
                            "import_database_select_all",
                            "Select All",
                            !shown.is_empty(),
                            true,
                        ),
                        kit::Entry::Button(
                            "import_database_clear_all",
                            "Clear All",
                            !db.checked.is_empty(),
                            false,
                        ),
                        kit::Entry::help("import_database_help"),
                    ],
                );
                match col.clicked.as_deref() {
                    Some("import_database_import") => k.import = true,
                    Some("import_database_cancel") => k.cancel = true,
                    Some("import_database_select_all") => k.all = true,
                    Some("import_database_clear_all") => k.none = true,
                    Some("import_database_help") => k.help = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.cancel || closed {
            return;
        }
        db.show_all = show_all;
        if let Some(i) = k.toggle {
            if let Some(&row) = shown.get(i) {
                let id = db.reference.reports[row].id;
                if !db.checked.remove(&id) {
                    db.checked.insert(id);
                }
                db.selected = Some(i);
            }
        }
        if k.all {
            db.checked
                .extend(shown.iter().map(|&i| db.reference.reports[i].id));
        }
        if k.none {
            db.checked.clear();
        }
        if k.help {
            self.open_help_for("import_database");
        }
        if k.import {
            let chosen: Vec<i64> = db.checked.iter().copied().collect();
            match self.import_reference(&db.reference, &chosen) {
                Ok(done) => {
                    let mut text = format!(
                        "Imported {} report{}",
                        done.reports,
                        if done.reports == 1 { "" } else { "s" }
                    );
                    if done.groups > 0 {
                        text.push_str(&format!(", {} group(s)", done.groups));
                    }
                    if done.batches > 0 {
                        text.push_str(&format!(", {} batch(es)", done.batches));
                    }
                    if !done.skipped.is_empty() {
                        text.push_str(&format!("; {} could not be read", done.skipped.len()));
                    }
                    self.status = text;
                    return;
                }
                // The import was rolled back; say why and keep the list.
                Err(e) => self.error(e),
            }
        }
        d.db = Some(db);
        self.reports.import = Some(d);
    }

    pub(crate) fn report_export_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.export.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        const W: f32 = 520.;
        const H: f32 = 150.;
        #[derive(Default)]
        struct Clicks {
            cancel: bool,
            ok: bool,
            browse: bool,
            help: bool,
        }
        let (k, closed, actions) = kit::window(
            ctx,
            "export_report",
            "Export Report",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                kit::text(c, 14., 12., &format!("Report: {}", d.name), true);
                kit::text(c, 14., 40., "File", true);
                c.edit(
                    "export_report_path",
                    14.,
                    56.,
                    W - 140.,
                    21.,
                    &mut d.path,
                    true,
                    Align::Min,
                );
                let field = c.r(14., 56., W - 140., 21.);
                k.browse = c.ellipsis("export_report_browse", field, true).clicked();
                if let Some(e) = &d.error {
                    kit::text(c, 14., 96., e, true);
                }
                kit::strip(c, W, H);
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::Button(
                            "export_report_ok",
                            "OK",
                            !d.path.trim().is_empty(),
                            false,
                        ),
                        kit::Entry::cancel("export_report_cancel"),
                        kit::Entry::help("export_report_help"),
                    ],
                );
                match col.clicked.as_deref() {
                    Some("export_report_ok") => k.ok = true,
                    Some("export_report_cancel") => k.cancel = true,
                    Some("export_report_help") => k.help = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.cancel || closed {
            return;
        }
        if k.browse {
            if let Some(f) = rfd::FileDialog::new()
                .add_filter("Report definition", &["rpt"])
                .set_file_name(format!("{}.rpt", groups::stem_of(&d.name)))
                .save_file()
            {
                d.path = f.display().to_string();
            }
        }
        if k.help {
            self.open_help_for("export_report");
        }
        if k.ok {
            let path = PathBuf::from(d.path.trim());
            match self.export_stored(d.id, &path) {
                Ok(()) => {
                    self.status = format!("Exported {} to {}", d.name, path.display());
                    return;
                }
                Err(e) => d.error = Some(e),
            }
        }
        self.reports.export = Some(d);
    }
}

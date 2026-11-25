//! Maintain Baselines and Assign Baselines, with the Add New Baseline choice,
//! Select Project, Update Baseline, the baseline lookups of Assign Baselines,
//! Revert Project to Baseline and the confirmations and messages, on the
//! dialog kit.
use super::store::{self, BaselineRow, Choice, UserBaselines};
use super::*;
use crate::chrome::{self, Icon};
use crate::dialog_kit::{self as kit, Canvas, Col, Entry};

use crate::grid::band as grid_band;

/// The project band of the baseline list: the top band level's style, in
/// the list's fixed row pitch.
fn band_style() -> crate::grid::layout::BandStyle {
    crate::views::grid::band_style(0, crate::views::grid::BandPitch::Fixed(kit::ROW))
}

/// Maintain Baselines: the selected row and the detail panel's edits.
#[derive(Default)]
pub(crate) struct Maintain {
    /// The selected baseline; `None` is a project band.
    pub selected: Option<i64>,
    /// The selected band's workspace project (`None`: the first band).
    pub band: Option<i64>,
    /// The baseline the detail fields were loaded from.
    pub loaded: Option<i64>,
    /// Baseline ID, Baseline Name and the Baseline Type choice.
    pub short: String,
    pub name: String,
    pub kind: usize,
    pub error: Option<String>,
}

/// Assign Baselines draft for one open project.
pub(crate) struct Assign {
    /// The project the dialog assigns for (workspace row).
    pub owner: Option<i64>,
    pub project: Choice,
    pub initial: Choice,
    pub users: UserBaselines,
}

/// The lookup behind one Assign Baselines field: 0 is the project
/// baseline, 1 to 3 the user baselines.
pub(crate) struct Lookup {
    pub slot: usize,
    pub selected: Option<i64>,
}

/// Add New Baseline: copy the open project, or convert another one.
#[derive(Default)]
pub(crate) struct AddChoice {
    pub convert: bool,
}

/// Select Project (Convert): the workspace projects, read when it opens.
#[derive(Default)]
pub(crate) struct SelectProject {
    pub selected: Option<i64>,
    pub projects: Option<Vec<(i64, String, String)>>,
}

/// Project > Revert Project to Baseline.
#[derive(Default)]
pub(crate) struct Revert {
    pub selected: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Confirm {
    Delete(i64),
    Restore(i64),
    Revert(i64),
}

/// One project band of Maintain Baselines: an open project and its
/// baselines.
pub(crate) struct Band {
    /// Workspace row (`None` for a project not saved yet).
    pub id: Option<i64>,
    pub project_id: String,
    pub name: String,
    pub rows: Vec<BaselineRow>,
}

/// A line of the baseline list.
#[derive(Clone, Copy)]
enum Line {
    Band(usize),
    /// Band, row in the band, number among all baseline lines.
    Baseline(usize, usize, usize),
}

/// A date stored as `YYYY-MM-DD HH:MM`, in the user's date format.
fn stored_date(s: &str) -> String {
    openxer_xer::parse_absolute(s).map_or(String::new(), |m| format::date(0, Some(m)))
}

fn choice_text(choice: Choice, rows: &[BaselineRow]) -> String {
    match choice {
        Choice::None => "<None>".into(),
        Choice::Current => "<Current Project>".into(),
        Choice::Baseline(id) => rows
            .iter()
            .find(|r| r.id == id)
            .map_or_else(|| "<None>".into(), |r| r.name.clone()),
    }
}

/// Records `name` over the part of a list row that is in view (the row
/// cut by the list's bottom edge is drawn but cannot be clicked there).
fn name_row(c: &mut Canvas, name: &str, cell: Rect, width: f32, body_bottom: f32) {
    let row = Rect::from_min_size(cell.min, Vec2::new(width, kit::ROW));
    if row.bottom() <= body_bottom + 0.5 {
        c.act(name, row);
    }
}

/// The action name of band `k`'s row.
fn band_key(k: usize) -> String {
    if k == 0 {
        "baselines_row_project".into()
    } else {
        format!("baselines_row_project_{k}")
    }
}

/// A lookup field: the baseline icon, the text and the `...` button.
/// Returns true when the field or its button is clicked.
fn lookup_field(c: &mut Canvas, key: &str, x: f32, y: f32, w: f32, text: &str) -> bool {
    let rect = c.r(x, y, w, 21.);
    c.sunken(rect, Color32::WHITE);
    let body = c.hit(
        key,
        Rect::from_min_max(rect.min, rect.max - Vec2::new(20., 0.)),
        true,
    );
    chrome::paint_icon_at(
        c.ui.painter(),
        Icon::Baselines,
        rect.min + Vec2::new(3., 2.),
        true,
    );
    kit::cell_text(
        c,
        Rect::from_min_size(rect.min, Vec2::new(w - 20., 21.)),
        text,
        false,
        22.,
    );
    let button = c.ellipsis(&format!("{key}_button"), rect, true);
    body.clicked() || button.clicked()
}

impl App {
    /// Project menu: Assign Baselines, Maintain Baselines and Revert Project
    /// to Baseline (OpenXER's roll-back, separate from Maintain Baselines'
    /// Restore).
    pub(crate) fn baseline_menu_items(&mut self, ui: &mut egui::Ui) {
        let assign = chrome::Item::new("Assi&gn Baselines...").icon(Icon::Baselines);
        if self.item(ui, "menu_assign_baselines", assign) {
            self.open_assign_baselines();
        }
        let maintain = chrome::Item::new("&Maintain Baselines...").icon(Icon::Baselines);
        if self.item(ui, "menu_maintain_baselines", maintain) {
            self.open_maintain_baselines();
        }
        let revert = chrome::Item::new("Re&vert Project to Baseline...").icon(Icon::Baselines);
        if self.item(ui, "menu_revert_baseline", revert) {
            self.open_revert_baseline();
        }
    }

    pub(crate) fn open_maintain_baselines(&mut self) {
        self.baselines.maintain = Some(Maintain::default());
    }

    pub(crate) fn open_assign_baselines(&mut self) {
        let owner = self.workspace_row();
        let (project, users) = self.baseline_choices();
        self.baselines.assign = Some(Assign {
            owner,
            project,
            initial: project,
            users,
        });
    }

    pub(crate) fn open_revert_baseline(&mut self) {
        self.baselines.revert = Some(Revert::default());
    }

    /// The bands of Maintain Baselines: every open project with its
    /// baselines, in Project ID order.
    pub(crate) fn baseline_bands(&self) -> Vec<Band> {
        let c = self.store.connection();
        let rows = |id: Option<i64>| id.and_then(|p| store::list(c, p).ok()).unwrap_or_default();
        let mut out = vec![Band {
            id: self.workspace_row(),
            project_id: self.project.model.project.name.clone(),
            name: views::project_title(&self.project),
            rows: rows(self.workspace_row()),
        }];
        for p in &self.open_set.parked {
            out.push(Band {
                id: Some(p.id),
                project_id: p.project.model.project.name.clone(),
                name: views::project_title(&p.project),
                rows: rows(Some(p.id)),
            });
        }
        out.sort_by_key(|b| b.project_id.to_lowercase());
        out
    }

    /// The open projects the Assign Baselines Project combo offers:
    /// (workspace row, "ID : name"), in Project ID order.
    fn assign_projects(&self) -> Vec<(Option<i64>, String)> {
        self.baseline_bands()
            .into_iter()
            .map(|b| (b.id, format!("{} : {}", b.project_id, b.name)))
            .collect()
    }

    /// Draw every open baseline dialog. Escape closes the one on top (the
    /// kit's shell gives it to that window); on Maintain Baselines it leaves
    /// a name or type not yet saved as stored.
    pub(crate) fn baseline_dialogs(&mut self, ctx: &egui::Context) {
        if !self.baselines.any_open() {
            return;
        }
        self.maintain_dialog(ctx);
        self.assign_dialog(ctx);
        self.lookup_dialog(ctx);
        self.add_dialog(ctx);
        self.select_dialog(ctx);
        self.update_dialog(ctx);
        self.filter_pick_dialog(ctx);
        self.preview_dialog(ctx);
        self.save_prompt(ctx);
        self.update_done_dialog(ctx);
        self.log_view_dialog(ctx);
        self.revert_dialog(ctx);
        self.confirm_dialog(ctx);
        self.notice_dialog(ctx);
    }

    /// Save a changed ID, name or type from the detail panel.
    fn commit_baseline_edits(&mut self, time: f64) {
        let Some(d) = self.baselines.maintain.as_ref() else {
            return;
        };
        let Some(id) = d.loaded else {
            return;
        };
        let types = self.baseline_type_list();
        let kind = types.get(d.kind).map(|t| t.0.clone()).unwrap_or_default();
        let (short, name) = (d.short.clone(), d.name.clone());
        if let Err(e) = self.set_baseline_fields(id, Some(&short), Some(&name), Some(&kind), time) {
            if let Some(d) = self.baselines.maintain.as_mut() {
                d.error = Some(e.message().to_string());
            }
        }
    }

    fn maintain_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.baselines.maintain.take() else {
            return;
        };
        let time = ctx.input(|i| i.time);
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let bands = self.baseline_bands();
        let mut lines = vec![];
        let mut n = 0;
        for (b, band) in bands.iter().enumerate() {
            lines.push(Line::Band(b));
            for r in 0..band.rows.len() {
                lines.push(Line::Baseline(b, r, n));
                n += 1;
            }
        }
        let find = |id: i64| {
            bands
                .iter()
                .flat_map(|b| b.rows.iter())
                .find(|r| r.id == id)
                .cloned()
        };
        if d.selected.is_some_and(|id| find(id).is_none()) {
            d.selected = None;
        }
        let types = self.baseline_type_list();
        // Load the detail fields when the selection moves.
        if d.loaded != d.selected {
            d.loaded = d.selected;
            let row = d.selected.and_then(find);
            d.short = row.as_ref().map_or(String::new(), |r| r.short_name.clone());
            d.name = row.as_ref().map_or(String::new(), |r| r.name.clone());
            d.kind = row
                .as_ref()
                .and_then(|r| {
                    let id = type_in_document(&self.project.document, &r.baseline_type);
                    types.iter().position(|t| t.0 == id)
                })
                .unwrap_or(0);
        }
        #[derive(Default)]
        struct Clicks {
            column: Option<String>,
            /// A baseline row, or a band (by its workspace row).
            select: Option<(Option<i64>, Option<i64>)>,
            commit: bool,
        }
        const W: f32 = 900.;
        const H: f32 = 331.;
        let selected_line = match d.selected {
            Some(id) => lines.iter().position(|l| match *l {
                Line::Baseline(b, r, _) => bands[b].rows[r].id == id,
                Line::Band(_) => false,
            }),
            None => lines.iter().position(|l| match *l {
                Line::Band(b) => d.band.is_none_or(|w| bands[b].id == Some(w)),
                Line::Baseline(..) => false,
            }),
        }
        .unwrap_or(0);
        let (clicks, closed, actions) = kit::window(
            ctx,
            "baselines_maintain",
            "Maintain Baselines",
            Vec2::new(W, H),
            |c| {
                let mut out = Clicks::default();
                let cols = [
                    Col {
                        title: "Project ID/Baseline ID",
                        width: 199.,
                    },
                    Col {
                        title: "Baseline Name",
                        width: 250.,
                    },
                    Col {
                        title: "Data Date",
                        width: 80.,
                    },
                    Col {
                        title: "Date Added",
                        width: 80.,
                    },
                    Col {
                        title: "Last Update Date",
                        width: 100.,
                    },
                ];
                let width: f32 = cols.iter().map(|c| c.width).sum();
                let (lx, ly, lw, lh) = (5., 5., 778., 206.);
                let body_bottom = c.origin.y + ly + lh - 1.;
                let list = kit::list(
                    c,
                    "baselines_list",
                    (lx, ly, lw, lh),
                    None,
                    &cols,
                    lines.len(),
                    Some(selected_line),
                    None,
                    &mut |c, r, col, cell, sel| match lines[r] {
                        Line::Band(b) => {
                            let band = &bands[b];
                            let style = band_style();
                            if !sel {
                                c.ui.painter().rect_filled(
                                    cell,
                                    0.,
                                    grid_band::fill(&style, false),
                                );
                            }
                            if col == 0 {
                                name_row(c, &band_key(b), cell, width, body_bottom);
                                chrome::paint_icon_at(
                                    c.ui.painter(),
                                    Icon::ProjectOpen,
                                    cell.min + Vec2::new(4., 1.),
                                    true,
                                );
                                // The project's ID and name in the band
                                // caption font, across the columns.
                                let row = Rect::from_min_size(cell.min, Vec2::new(width, kit::ROW));
                                grid_band::caption(
                                    c.ui.painter(),
                                    row,
                                    cell.left() + 24.,
                                    cell.top(),
                                    &style,
                                    &format!("{}  {}", band.project_id, band.name),
                                    sel,
                                );
                            }
                        }
                        Line::Baseline(b, k, n) => {
                            let row = &bands[b].rows[k];
                            match col {
                                0 => {
                                    name_row(
                                        c,
                                        &format!("baselines_row_{n}"),
                                        cell,
                                        width,
                                        body_bottom,
                                    );
                                    c.ui.painter().rect_filled(
                                        Rect::from_min_size(cell.min, Vec2::new(13., kit::ROW)),
                                        0.,
                                        grid_band::fill(&band_style(), false),
                                    );
                                    chrome::paint_icon_at(
                                        c.ui.painter(),
                                        Icon::Baselines,
                                        cell.min + Vec2::new(28., 1.),
                                        true,
                                    );
                                    kit::cell_text(c, cell, &row.short_name, sel, 46.);
                                }
                                1 => kit::cell_text(c, cell, &row.name, sel, 3.),
                                2 => kit::cell_text(c, cell, &stored_date(&row.data_date), sel, 3.),
                                3 => kit::cell_text(c, cell, &stored_date(&row.added), sel, 3.),
                                _ => kit::cell_text(c, cell, &stored_date(&row.updated), sel, 3.),
                            }
                        }
                    },
                );
                if let Some(r) = list.clicked {
                    out.select = Some(match lines[r] {
                        Line::Band(b) => (None, bands[b].id),
                        Line::Baseline(b, k, _) => (Some(bands[b].rows[k].id), bands[b].id),
                    });
                }
                // The selected baseline's details.
                c.lowered(10., 221., 768., 100.);
                c.raised(11., 222., 766., 98.);
                let current = d.selected.and_then(find);
                let on = current.is_some();
                kit::text(c, 21., 231., "Baseline ID", on);
                kit::text(c, 341., 231., "Baseline Name", on);
                kit::text(c, 21., 275., "Baseline Type", on);
                kit::text(c, 313., 275., "Data Date", on);
                kit::text(c, 512., 275., "Last Update Date", on);
                // Escape leaves the stored ID and name.
                let edit = c.edit(
                    "baselines_id",
                    21.,
                    246.,
                    301.,
                    21.,
                    &mut d.short,
                    on,
                    Align::Min,
                );
                if edit.lost_focus() && !escape {
                    out.commit = true;
                }
                let edit = c.edit(
                    "baselines_name",
                    341.,
                    246.,
                    353.,
                    21.,
                    &mut d.name,
                    on,
                    Align::Min,
                );
                if edit.lost_focus() && !escape {
                    out.commit = true;
                }
                let names: Vec<(&str, bool)> = types.iter().map(|t| (t.1.as_str(), true)).collect();
                if c.combo(
                    "baselines_type",
                    21.,
                    290.,
                    273.,
                    21.,
                    &names,
                    &mut d.kind,
                    on,
                ) {
                    out.commit = true;
                }
                kit::field(
                    c,
                    313.,
                    290.,
                    178.,
                    21.,
                    &current
                        .as_ref()
                        .map_or(String::new(), |r| stored_date(&r.data_date)),
                    false,
                );
                kit::field(
                    c,
                    512.,
                    290.,
                    182.,
                    21.,
                    &current
                        .as_ref()
                        .map_or(String::new(), |r| stored_date(&r.updated)),
                    false,
                );
                if let Some(e) = &d.error {
                    kit::error_text(c, 21., 303., 680., e);
                }
                kit::strip(c, W, H);
                out.column = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::close("baselines_close"),
                        Entry::Button("baselines_add", "Add", true, true),
                        Entry::Button("baselines_delete", "Delete", on, false),
                        Entry::Button("baselines_copy", "Copy", on, false),
                        Entry::Button("baselines_update", "Update...", on, true),
                        Entry::Button("baselines_restore", "Restore", on, true),
                        Entry::help("baselines_help"),
                    ],
                )
                .clicked;
                out
            },
        );
        self.record_dialog_actions(actions);
        let selected = d.selected.and_then(find);
        let band = match &selected {
            Some(r) => Some(r.parent),
            None => d.band.or_else(|| bands.first().and_then(|b| b.id)),
        };
        self.baselines.maintain = Some(d);
        let clicked = clicks.column.as_deref();
        let close = clicked == Some("baselines_close") || closed;
        // Escape closes without saving a half-typed ID, name or type.
        let cancelled = closed && escape;
        if !cancelled && (clicks.commit || clicks.select.is_some() || close) {
            self.commit_baseline_edits(time);
        }
        if let (Some((sel, owner)), Some(d)) = (clicks.select, self.baselines.maintain.as_mut()) {
            d.selected = sel;
            d.band = owner;
            d.error = None;
        }
        if close {
            self.baselines.maintain = None;
            return;
        }
        match clicked {
            Some("baselines_help") => self.open_help_for("baselines_maintain"),
            Some("baselines_add") => {
                // Add works on the selected band's project.
                if let Some(owner) = band {
                    self.activate_owner(owner);
                }
                self.baselines.add = Some(AddChoice::default());
            }
            _ => {}
        }
        if let Some(row) = selected {
            let id = row.id;
            match clicked {
                Some("baselines_delete") => {
                    // A baseline in use is refused before any question.
                    if self.baseline_in_use(&row) {
                        self.baselines.notice = Some(ops::DELETE_ASSIGNED.into());
                    } else {
                        self.baselines.confirm = Some(Confirm::Delete(id));
                    }
                }
                Some("baselines_copy") => match self.copy_baseline(id, time) {
                    Ok(new) => {
                        if let Some(m) = self.baselines.maintain.as_mut() {
                            m.selected = Some(new);
                        }
                    }
                    Err(e) => self.baseline_failed(e),
                },
                Some("baselines_restore") => {
                    if self.baseline_in_use(&row) {
                        self.baselines.notice = Some(ops::RESTORE_ASSIGNED.into());
                    } else {
                        self.baselines.confirm = Some(Confirm::Restore(id));
                    }
                }
                Some("baselines_update") => {
                    self.activate_owner(row.parent);
                    self.open_update_baseline(id);
                }
                _ => {}
            }
        }
    }

    fn assign_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.baselines.assign.take() else {
            return;
        };
        let time = ctx.input(|i| i.time);
        let rows = d
            .owner
            .and_then(|p| store::list(self.store.connection(), p).ok())
            .unwrap_or_default();
        let projects = self.assign_projects();
        let mut open_lookup = None;
        let mut picked_project = None;
        const W: f32 = 500.;
        const H: f32 = 323.;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "baselines_assign",
            "Assign Baselines",
            Vec2::new(W, H),
            |c| {
                c.lowered(5., 5., 378., 313.);
                c.etched(6., 118., 376.);
                kit::text(c, 14., 14., "Project", true);
                let names: Vec<(&str, bool)> =
                    projects.iter().map(|p| (p.1.as_str(), true)).collect();
                let mut index = projects.iter().position(|p| p.0 == d.owner).unwrap_or(0);
                if c.combo(
                    "assign_project",
                    14.,
                    30.,
                    353.,
                    22.,
                    &names,
                    &mut index,
                    true,
                ) {
                    picked_project = Some(projects[index].0);
                }
                kit::text(c, 14., 75., "Project Baseline", true);
                kit::text(c, 14., 136., "User Baselines", true);
                let fields: [(usize, &str, (f32, f32, f32)); 4] = [
                    (0, "", (14., 91., 353.)),
                    (1, "Primary", (30., 179., 338.)),
                    (2, "Secondary", (30., 235., 338.)),
                    (3, "Tertiary", (30., 291., 338.)),
                ];
                for (k, label, (x, y, w)) in fields {
                    if !label.is_empty() {
                        kit::text(c, x, y - 19., label, true);
                    }
                    let current = if k == 0 { d.project } else { d.users.0[k - 1] };
                    let text = choice_text(current, &rows);
                    if lookup_field(c, &format!("assign_lookup_{k}"), x, y, w, &text) {
                        open_lookup = Some(k);
                    }
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("assign_ok"),
                        Entry::cancel("assign_cancel"),
                        Entry::help("assign_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        if let Some(owner) = picked_project {
            if owner != d.owner {
                let (project, users) = owner
                    .map(|o| self.baseline_choices_of(o))
                    .unwrap_or((Choice::Current, UserBaselines::default()));
                d = Assign {
                    owner,
                    project,
                    initial: project,
                    users,
                };
            }
        }
        if let Some(slot) = open_lookup {
            let current = if slot == 0 {
                d.project
            } else {
                d.users.0[slot - 1]
            };
            self.baselines.lookup = Some(Lookup {
                slot,
                selected: match current {
                    Choice::Baseline(id) => Some(id),
                    _ => None,
                },
            });
        }
        match clicked.as_deref() {
            Some("assign_help") => self.open_help_for("baselines_assign"),
            Some("assign_ok") => {
                if let Some(owner) = d.owner {
                    self.activate_owner(owner);
                }
                let project = (d.project != d.initial).then_some(d.project);
                if let Err(e) = self.assign_baselines(project, d.users, time) {
                    self.baseline_failed(e);
                }
                self.baselines.lookup = None;
                return;
            }
            Some("assign_cancel") => {
                self.baselines.lookup = None;
                return;
            }
            _ => {}
        }
        if closed {
            self.baselines.lookup = None;
        } else {
            self.baselines.assign = Some(d);
        }
    }

    /// The lookup of one Assign Baselines field: the project's baselines by
    /// name. Select puts the picked one in the field, Remove sets it back to
    /// <Current Project> (project baseline, primary) or <None>.
    fn lookup_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut l) = self.baselines.lookup.take() else {
            return;
        };
        let Some(owner) = self.baselines.assign.as_ref().map(|a| a.owner) else {
            return;
        };
        let rows = owner
            .and_then(|p| store::list(self.store.connection(), p).ok())
            .unwrap_or_default();
        let title = match l.slot {
            0 => "Select Project Baseline",
            1 => "Select Primary Baseline",
            2 => "Select Secondary Baseline",
            _ => "Select Tertiary Baseline",
        };
        const W: f32 = 400.;
        const H: f32 = 270.;
        let (clicked, closed, actions) =
            kit::window(ctx, "baselines_lookup", title, Vec2::new(W, H), |c| {
                let (lx, ly, lw, lh) = (5., 5., W - 122., H - 10.);
                let body_bottom = c.origin.y + ly + lh - 1.;
                let selected = rows.iter().position(|r| l.selected == Some(r.id));
                let list = kit::list(
                    c,
                    "lookup_list",
                    (lx, ly, lw, lh),
                    Some("Display: Project Baselines"),
                    &[Col {
                        title: "Baseline Name",
                        width: lw - 19.,
                    }],
                    rows.len(),
                    selected,
                    None,
                    &mut |c, r, _, cell, sel| {
                        name_row(c, &format!("lookup_row_{r}"), cell, lw - 19., body_bottom);
                        chrome::paint_icon_at(
                            c.ui.painter(),
                            Icon::Baselines,
                            cell.min + Vec2::new(3., 1.),
                            true,
                        );
                        kit::cell_text(c, cell, &rows[r].name, sel, 23.);
                    },
                );
                if let Some(r) = list.clicked {
                    l.selected = Some(rows[r].id);
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("lookup_select", "Select", l.selected.is_some(), false),
                        Entry::close("lookup_close"),
                        Entry::Button("lookup_remove", "Remove", true, true),
                        Entry::help("lookup_help"),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        let set = |app: &mut App, choice: Choice| {
            if let Some(a) = app.baselines.assign.as_mut() {
                if l.slot == 0 {
                    a.project = choice;
                } else {
                    a.users.0[l.slot - 1] = choice;
                }
            }
        };
        match clicked.as_deref() {
            Some("lookup_help") => self.open_help_for("baselines_lookup"),
            Some("lookup_select") => {
                if let Some(id) = l.selected {
                    set(self, Choice::Baseline(id));
                }
                return;
            }
            Some("lookup_remove") => {
                let empty = if l.slot <= 1 {
                    Choice::Current
                } else {
                    Choice::None
                };
                set(self, empty);
                return;
            }
            Some("lookup_close") => return,
            _ => {}
        }
        if !closed {
            self.baselines.lookup = Some(l);
        }
    }

    fn add_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.baselines.add.take() else {
            return;
        };
        let time = ctx.input(|i| i.time);
        const W: f32 = 500.;
        const H: f32 = 110.;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "baselines_add",
            "Add New Baseline",
            Vec2::new(W, H),
            |c| {
                c.lowered(5., 5., 378., 100.);
                kit::text(c, 14., 14., "What would you like to do?", true);
                if c.radio(
                    "add_copy",
                    24.,
                    36.,
                    356.,
                    18.,
                    "Save a copy of the current project as a new baseline",
                    !d.convert,
                    true,
                ) {
                    d.convert = false;
                }
                if c.radio(
                    "add_convert",
                    24.,
                    62.,
                    356.,
                    18.,
                    "Convert another project to a new baseline of the current project",
                    d.convert,
                    true,
                ) {
                    d.convert = true;
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("add_ok"),
                        Entry::cancel("add_cancel"),
                        Entry::help("add_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("add_help") => self.open_help_for("baselines_add"),
            Some("add_ok") => {
                if d.convert {
                    self.baselines.select = Some(SelectProject::default());
                } else {
                    match self.add_baseline_copy(time) {
                        Ok(id) => {
                            if let Some(m) = self.baselines.maintain.as_mut() {
                                m.selected = Some(id);
                            }
                        }
                        Err(e) => self.baseline_failed(e),
                    }
                }
                return;
            }
            Some("add_cancel") => return,
            _ => {}
        }
        if !closed {
            self.baselines.add = Some(d);
        }
    }

    /// Select Project for Convert: every workspace project. Picking an open
    /// one is refused with a message and the list stays open.
    fn select_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.baselines.select.take() else {
            return;
        };
        let time = ctx.input(|i| i.time);
        let projects = d
            .projects
            .get_or_insert_with(|| self.convertible_projects())
            .clone();
        const W: f32 = 500.;
        const H: f32 = 300.;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "baselines_select",
            "Select Project",
            Vec2::new(W, H),
            |c| {
                let (lx, ly, lw, lh) = (5., 5., 378., 290.);
                let body_bottom = c.origin.y + ly + lh - 1.;
                let selected = projects.iter().position(|p| d.selected == Some(p.0));
                let list = kit::list(
                    c,
                    "select_project_list",
                    (lx, ly, lw, lh),
                    None,
                    &[
                        Col {
                            title: "Project ID",
                            width: 140.,
                        },
                        Col {
                            title: "Project Name",
                            width: 219.,
                        },
                    ],
                    projects.len(),
                    selected,
                    None,
                    &mut |c, r, col, cell, sel| {
                        if col == 0 {
                            name_row(c, &format!("select_project_{r}"), cell, 359., body_bottom);
                            chrome::paint_icon_at(
                                c.ui.painter(),
                                Icon::ProjectClosed,
                                cell.min + Vec2::new(3., 1.),
                                true,
                            );
                            kit::cell_text(c, cell, &projects[r].1, sel, 23.);
                        } else {
                            kit::cell_text(c, cell, &projects[r].2, sel, 3.);
                        }
                    },
                );
                if let Some(r) = list.clicked {
                    d.selected = Some(projects[r].0);
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("select_ok", "Select", d.selected.is_some(), false),
                        Entry::close("select_close"),
                        Entry::help("select_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("select_help") => self.open_help_for("baselines_select"),
            Some("select_ok") => {
                if let Some(project) = d.selected {
                    match self.convert_to_baseline(project, time) {
                        Ok(id) => {
                            if let Some(m) = self.baselines.maintain.as_mut() {
                                m.selected = Some(id);
                            }
                            return;
                        }
                        Err(e) => self.baseline_failed(e),
                    }
                }
            }
            Some("select_close") => return,
            _ => {}
        }
        if !closed {
            self.baselines.select = Some(d);
        }
    }

    /// Project > Revert Project to Baseline: pick one of the current
    /// project's baselines; Revert asks first.
    fn revert_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.baselines.revert.take() else {
            return;
        };
        let rows = self
            .workspace_row()
            .and_then(|p| store::list(self.store.connection(), p).ok())
            .unwrap_or_default();
        if d.selected
            .is_some_and(|id| !rows.iter().any(|r| r.id == id))
        {
            d.selected = None;
        }
        const W: f32 = 620.;
        const H: f32 = 240.;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "baselines_revert",
            "Revert Project to Baseline",
            Vec2::new(W, H),
            |c| {
                kit::text(
                    c,
                    8.,
                    8.,
                    "Replace the data of the current project with the baseline selected below:",
                    true,
                );
                let (lx, ly, lw, lh) = (5., 26., W - 122., H - 31.);
                let body_bottom = c.origin.y + ly + lh - 1.;
                let cols = [
                    Col {
                        title: "Baseline ID",
                        width: 170.,
                    },
                    Col {
                        title: "Baseline Name",
                        width: 220.,
                    },
                    Col {
                        title: "Data Date",
                        width: lw - 409.,
                    },
                ];
                let width: f32 = cols.iter().map(|c| c.width).sum();
                let selected = rows.iter().position(|r| d.selected == Some(r.id));
                let list = kit::list(
                    c,
                    "revert_list",
                    (lx, ly, lw, lh),
                    None,
                    &cols,
                    rows.len(),
                    selected,
                    None,
                    &mut |c, r, col, cell, sel| {
                        let row = &rows[r];
                        match col {
                            0 => {
                                name_row(c, &format!("revert_row_{r}"), cell, width, body_bottom);
                                chrome::paint_icon_at(
                                    c.ui.painter(),
                                    Icon::Baselines,
                                    cell.min + Vec2::new(3., 1.),
                                    true,
                                );
                                kit::cell_text(c, cell, &row.short_name, sel, 23.);
                            }
                            1 => kit::cell_text(c, cell, &row.name, sel, 3.),
                            _ => kit::cell_text(c, cell, &stored_date(&row.data_date), sel, 3.),
                        }
                    },
                );
                if let Some(r) = list.clicked {
                    d.selected = Some(rows[r].id);
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("revert_ok", "Revert", d.selected.is_some(), false),
                        Entry::close("revert_close"),
                        Entry::help("revert_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("revert_help") => self.open_help_for("baselines_revert"),
            Some("revert_ok") => {
                if let Some(id) = d.selected {
                    self.baselines.confirm = Some(Confirm::Revert(id));
                }
            }
            Some("revert_close") => return,
            _ => {}
        }
        if !closed {
            self.baselines.revert = Some(d);
        }
    }

    /// Delete, Restore and Revert ask first, in the kit's message box, with
    /// the default from `kit::Question`.
    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(d) = self.baselines.confirm else {
            return;
        };
        let time = ctx.input(|i| i.time);
        let (text, question) = match d {
            Confirm::Delete(_) => (ops::DELETE_CONFIRM, kit::Question::DeleteBaseline),
            Confirm::Restore(_) => (ops::RESTORE_CONFIRM, kit::Question::RestoreBaseline),
            Confirm::Revert(_) => (ops::REVERT_CONFIRM, kit::Question::RevertToBaseline),
        };
        let mut rects = vec![];
        let clicked = kit::message_box(
            ctx,
            "confirm",
            "Confirmation",
            kit::MessageIcon::Question,
            text,
            &["Yes", "No"],
            question.default_in(&["Yes", "No"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        match clicked {
            Some(0) => {
                self.baselines.confirm = None;
                let result = match d {
                    Confirm::Delete(id) => self.delete_baseline(id, time).map(|()| {
                        self.baselines.notice = Some(ops::DELETED.into());
                    }),
                    Confirm::Restore(id) => self.restore_baseline(id, time).map(|_| ()),
                    Confirm::Revert(id) => self.revert_to_baseline(id, time).map(|()| {
                        self.baselines.revert = None;
                    }),
                };
                if let Err(e) = result {
                    self.baseline_failed(e);
                }
            }
            Some(_) => self.baselines.confirm = None,
            None if escape => self.baselines.confirm = None,
            None => {}
        }
    }

    /// A message answered with OK only: a refusal, or Delete's done note.
    fn notice_dialog(&mut self, ctx: &egui::Context) {
        let Some(text) = self.baselines.notice.clone() else {
            return;
        };
        let mut rects = vec![];
        let clicked = kit::message_box(
            ctx,
            "baseline_notice",
            "Baselines",
            kit::MessageIcon::Info,
            &text,
            &["OK"],
            Some(0),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        if clicked.is_some() || escape {
            self.baselines.notice = None;
        }
    }
}

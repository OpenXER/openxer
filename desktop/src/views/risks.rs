//! Risks view (Project > Risks): the project risk register (PROJRISK, or a
//! RISK table when a file carries one) with category, type, status, owner,
//! response and pre/post-response values as XER carries them, and the
//! activities each risk is assigned to (TASKRISK). Files without risk tables
//! show an empty register; Add creates the table.
use super::grid::Row;
use super::record_view::{self, ListSpec, PickTarget, RowKey};
use super::sheet;
use super::*;
use crate::records::{self, num};
use crate::schedule_dialogs::Canvas;
pub use openxer_xer::edit::project_records::{
    add_risk, assign_risk as assign, delete_risk, risk_activities, unassign_risk as unassign,
    RISK_KIND as KIND, RISK_RESPONSE as RESPONSE, RISK_STATUS as STATUS,
};

pub const TITLES: [&str; 10] = [
    "Risk ID",
    "Risk Name",
    "Risk Category",
    "Risk Type",
    "Risk Status",
    "Risk Owner",
    "Identified On",
    "Identified By",
    "Response Type",
    "Project",
];
pub const WIDTHS: [f32; 10] = [90., 220., 130., 90., 110., 140., 95., 140., 100., 90.];
pub const TABS: [&str; 7] = [
    "General",
    "Impact",
    "Activities",
    "Description",
    "Cause",
    "Effect",
    "Notes",
];

/// One risk as the view lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Risk {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub category: String,
    pub kind: String,
    pub status: String,
    pub owner: String,
    pub identified_on: String,
    pub identified_by: String,
    pub response: String,
}

fn resource_name(p: &ProjectData, id: &str) -> String {
    p.resource_names.get(&num(id)).cloned().unwrap_or_default()
}

/// The open project's risks, by Risk ID.
pub fn risks(p: &ProjectData) -> Vec<Risk> {
    let doc = &p.document;
    let table = records::risk_table(doc);
    let pid = p.model.project.proj_id;
    let categories = record_view::dictionary(doc, "RISKTYPE", "risk_type_id", "risk_type");
    let mut list: Vec<Risk> = record_view::project_rows(doc, table, pid)
        .into_iter()
        .map(|r| {
            let v = |k: &str| records::get(doc, table, r, k);
            Risk {
                id: num(&v("risk_id")),
                code: v("risk_code"),
                name: v("risk_name"),
                category: record_view::name_of(&categories, &v("risk_type_id")),
                kind: record_view::label_of(&KIND, &v("risk_to_type")),
                status: record_view::label_of(&STATUS, &v("status_code")),
                owner: resource_name(p, &v("rsrc_id")),
                identified_on: v("add_date"),
                identified_by: resource_name(p, &v("identified_by_id")),
                response: record_view::label_of(&RESPONSE, &v("response_type")),
            }
        })
        .collect();
    list.sort_by(|a, b| a.code.cmp(&b.code).then(a.id.cmp(&b.id)));
    list
}

impl App {
    pub(super) fn risks_view(&mut self, host: &mut egui::Ui) {
        let list = risks(&self.project);
        let origin = self.project.model.origin_serial;
        let mut rows = vec![];
        let mut keys = vec![];
        for r in &list {
            keys.push(RowKey::Record(r.id));
            rows.push(Row {
                icon: Some(chrome::Icon::Risks),
                cells: vec![
                    r.code.clone(),
                    r.name.clone(),
                    r.category.clone(),
                    r.kind.clone(),
                    r.status.clone(),
                    r.owner.clone(),
                    record_view::show_date(origin, &r.identified_on),
                    r.identified_by.clone(),
                    r.response.clone(),
                    self.project.model.project.name.clone(),
                ],
                ..Default::default()
            });
        }
        record_view::keep_selection(&mut self.views.records.risks, &keys);
        let selected = self.views.records.risks.selected;
        let spec = ListSpec {
            id: "risks",
            display: "All Risks".into(),
            titles: &TITLES,
            right: &[],
            tabs: &TABS,
            commands: vec![
                ("Add", chrome::Icon::Add, true),
                ("Delete", chrome::Icon::Delete, selected.is_some()),
            ],
            empty: "This project has no risks. Add one with the Add button.",
            details: "Risk Details",
            customize: "Risk",
            extras: vec![],
        };
        let command = self.record_list_view(
            host,
            &spec,
            |v| &mut v.records.risks,
            rows,
            keys,
            Box::new(move |app, c, size, tab| app.risk_details(c, size, tab, selected)),
            &mut |_, _| {},
        );
        match command {
            Some("Add") => self.risk_command_add(),
            Some("Delete") => self.risk_command_delete(),
            _ => {}
        }
    }

    pub(super) fn risk_command_add(&mut self) {
        let id = add_risk(&mut self.project, &record_view::today());
        self.views.records.risks.selected = Some(id);
        self.touch_records();
        self.status = "Risk added".into();
    }

    pub(super) fn risk_command_delete(&mut self) {
        self.record_delete(MainView::Risks, false);
    }

    fn set_risk_field(&mut self, id: i64, column: &str, text: String) {
        let table = records::risk_table(&self.project.document);
        if record_view::set_value(
            &mut self.project.document,
            table,
            "risk_id",
            id,
            column,
            text,
        ) {
            self.touch_records();
        }
    }

    fn risk_details(&mut self, c: &mut Canvas, size: Vec2, tab: usize, id: Option<i64>) {
        let (w, h) = (size.x, size.y);
        let Some(id) = id else {
            c.label(12., 12., 13., "Select a risk.", true);
            return;
        };
        let origin = self.project.model.origin_serial;
        let doc = &self.project.document;
        let table = records::risk_table(doc);
        let v = |k: &str| record_view::value(doc, table, "risk_id", id, k);
        let half = ((w - 60.) / 2.).clamp(160., 420.);
        let x2 = 20. + half + 20.;
        match TABS[tab.min(TABS.len() - 1)] {
            "General" => {
                let categories =
                    record_view::dictionary(doc, "RISKTYPE", "risk_type_id", "risk_type");
                let mut people: Vec<(String, String)> = self
                    .project
                    .resource_names
                    .iter()
                    .map(|(k, n)| (k.to_string(), n.clone()))
                    .collect();
                people.sort_by(|a, b| a.1.cmp(&b.1));
                let fields = [
                    v("risk_code"),
                    v("risk_name"),
                    v("risk_type_id"),
                    v("risk_to_type"),
                    v("status_code"),
                    v("rsrc_id"),
                    v("identified_by_id"),
                    v("add_date"),
                ];
                if let Some(t) =
                    sheet::text_field(c, "risk_code", 20., 25., 160., "ID", &fields[0], true)
                {
                    self.set_risk_field(id, "risk_code", records::clean(&t));
                }
                if let Some(t) = sheet::text_field(
                    c,
                    "risk_name",
                    200.,
                    25.,
                    w - 220.,
                    "Name",
                    &fields[1],
                    true,
                ) {
                    self.set_risk_field(id, "risk_name", records::clean(&t));
                }
                if let Some(code) = record_view::dictionary_field(
                    c,
                    "risk_category",
                    20.,
                    70.,
                    half,
                    "Category",
                    &categories,
                    &fields[2],
                    true,
                ) {
                    self.set_risk_field(id, "risk_type_id", code);
                }
                if let Some(code) = record_view::combo_field(
                    c,
                    "risk_type",
                    x2,
                    70.,
                    half,
                    "Type",
                    &KIND,
                    &fields[3],
                    true,
                ) {
                    self.set_risk_field(id, "risk_to_type", code);
                }
                if let Some(code) = record_view::combo_field(
                    c,
                    "risk_status",
                    20.,
                    115.,
                    half,
                    "Status",
                    &STATUS,
                    &fields[4],
                    true,
                ) {
                    self.set_risk_field(id, "status_code", code);
                }
                if let Some(code) = record_view::dictionary_field(
                    c,
                    "risk_owner",
                    x2,
                    115.,
                    half,
                    "Owner",
                    &people,
                    &fields[5],
                    true,
                ) {
                    self.set_risk_field(id, "rsrc_id", code);
                }
                if let Some(code) = record_view::dictionary_field(
                    c,
                    "risk_identified_by",
                    20.,
                    160.,
                    half,
                    "Identified By",
                    &people,
                    &fields[6],
                    true,
                ) {
                    self.set_risk_field(id, "identified_by_id", code);
                }
                if let Some(t) = record_view::date_text(
                    c,
                    "risk_identified_on",
                    x2,
                    160.,
                    half.min(217.),
                    "Identified On",
                    origin,
                    &fields[7],
                    true,
                ) {
                    self.set_risk_field(id, "add_date", t);
                }
            }
            "Impact" => {
                let third = ((w - 80.) / 3.).clamp(100., 220.);
                let col = |k: usize| 30. + k as f32 * (third + 10.);
                let values: Vec<String> = [
                    "pre_rsp_prblty",
                    "pre_rsp_schd_prblty",
                    "pre_rsp_cost_prblty",
                    "post_rsp_prblty",
                    "post_rsp_schd_prblty",
                    "post_rsp_cost_prblty",
                    "response_type",
                    "response_text",
                ]
                .iter()
                .map(|k| v(k))
                .collect();
                let width = 3. * third + 40.;
                sheet::section(c, 10., 6., width, 70., "Pre-Response");
                sheet::section(c, 10., 150., width, 70., "Post-Response");
                for (k, (group, caption)) in [
                    (0usize, "Probability"),
                    (0, "Schedule"),
                    (0, "Cost"),
                    (1, "Probability"),
                    (1, "Schedule"),
                    (1, "Cost"),
                ]
                .into_iter()
                .enumerate()
                {
                    let column = [
                        "pre_rsp_prblty",
                        "pre_rsp_schd_prblty",
                        "pre_rsp_cost_prblty",
                        "post_rsp_prblty",
                        "post_rsp_schd_prblty",
                        "post_rsp_cost_prblty",
                    ][k];
                    let y = if group == 0 { 46. } else { 190. };
                    if let Some(t) = sheet::text_field(
                        c,
                        &format!("risk_{column}"),
                        col(k % 3),
                        y,
                        third,
                        caption,
                        &values[k],
                        true,
                    ) {
                        self.set_risk_field(id, column, records::clean(&t));
                    }
                }
                if let Some(code) = record_view::combo_field(
                    c,
                    "risk_response",
                    20.,
                    100.,
                    third,
                    "Response Type",
                    &RESPONSE,
                    &values[6],
                    true,
                ) {
                    self.set_risk_field(id, "response_type", code);
                }
                if let Some(t) = sheet::text_field(
                    c,
                    "risk_response_text",
                    30. + third,
                    100.,
                    (w - 50. - third).max(160.),
                    "Response Description",
                    &values[7],
                    true,
                ) {
                    self.set_risk_field(id, "response_text", records::clean(&t));
                }
            }
            "Activities" => self.risk_activities_tab(c, w, h, id),
            tab => {
                let (key, column, html) = match tab {
                    "Description" => ("risk_description", "risk_descr", true),
                    "Cause" => ("risk_cause", "risk_cause", false),
                    "Effect" => ("risk_effect", "risk_effect", false),
                    _ => ("risk_notes", "notes", false),
                };
                let stored = v(column);
                if let Some(t) = record_view::memo(c, key, 6., 6., w - 12., h - 12., &stored, html)
                {
                    self.set_risk_field(id, column, t);
                }
            }
        }
    }

    fn risk_activities_tab(&mut self, c: &mut Canvas, w: f32, h: f32, id: i64) {
        let pid = self.project.model.project.proj_id;
        let origin = self.project.model.origin_serial;
        let tasks = risk_activities(&self.project.document, pid, id);
        let rows: Vec<Vec<String>> = tasks
            .iter()
            .map(|t| {
                let (code, name) = record_view::activity_label(&self.project, *t);
                let a = self
                    .project
                    .model
                    .activities
                    .iter()
                    .find(|a| a.task_id == *t);
                vec![
                    code,
                    name,
                    format::date(origin, a.and_then(|a| a.act_start.or(a.early_start))),
                    format::date(origin, a.and_then(|a| a.act_finish.or(a.early_finish))),
                ]
            })
            .collect();
        let cols = [
            ("Activity ID", 120., false),
            ("Activity Name", 280., false),
            ("Start", 110., false),
            ("Finish", 110., false),
        ];
        let sub = self.views.records.risks.sub_row.filter(|&r| r < rows.len());
        if let Some(r) = sheet::table(
            c,
            "risk_activities",
            6.,
            6.,
            w - 12.,
            h - 40.,
            &cols,
            &rows,
            sub,
        ) {
            self.views.records.risks.sub_row = Some(r);
        }
        match sheet::buttons(
            c,
            "risk_activity_buttons",
            6.,
            h - 28.,
            &[("Assign", true), ("Remove", sub.is_some())],
        ) {
            Some(0) => self.open_picker(PickTarget::RiskActivity(id)),
            Some(1) => {
                if let Some(t) = sub.and_then(|r| tasks.get(r)) {
                    if unassign(&mut self.project.document, pid, id, *t) {
                        self.views.records.risks.sub_row = None;
                        self.touch_records();
                    }
                }
            }
            _ => {}
        }
    }
}

//! The README screenshots: the app on a synthetic construction schedule (a
//! made-up process unit expansion), rendered through the software painter
//! with no window. Regenerate them all with
//! `cargo test -p openxer-desktop --bin OpenXER readme -- --ignored`
//! (`render_readme_hero` writes `docs/images/hero.png`,
//! `render_readme_screens` the rest). Every project, crew, name and date
//! here is made up for the pictures.
use super::*;
use crate::grid::layout::{ColumnSetting, GroupLevel};
use crate::headless_render::SoftwarePainter;
use crate::panes::BottomView;
use std::collections::HashMap;

/// A crew: resource id, short name, name, crew limit (people).
type Crew = (i64, &'static str, &'static str, f64);

const CREWS: [Crew; 5] = [
    (1, "CIV", "Civil crew", 10.),
    (2, "IW", "Ironworkers", 8.),
    (3, "PF", "Pipefitters", 12.),
    (4, "EL", "Electricians", 8.),
    (5, "IT", "Instrument technicians", 6.),
];

/// The crew working an activity and its size, if any. Deliveries, shop
/// fabrication, X-ray, insulation and commissioning are left unassigned.
fn crew_of(id: &str) -> Option<(i64, f64)> {
    let size = match id {
        "CIV-1000" | "STL-2010" | "PIP-3010" | "PIP-3020" | "PIP-3030" | "PIP-3140"
        | "PIP-3180" => return None,
        // Two pipefitter crews of five work the rack, so the hours already
        // worked stay within the limit of twelve.
        "PIP-3100" | "PIP-3110" => 5.,
        // The tie-in crew starts at the data date and overloads the pipefitters.
        "PIP-3130" | "ELE-4030" => 6.,
        _ if id.starts_with("COM") => return None,
        _ if id.starts_with("CIV") || id.starts_with("STL") => 6.,
        _ => 4.,
    };
    let resource = match &id[..3] {
        "CIV" => 1,
        "STL" => 2,
        "PIP" => 3,
        "ELE" => 4,
        _ => 5,
    };
    Some((resource, size))
}

/// A WBS element: id, parent, code, name.
type WbsRow = (i64, Option<i64>, &'static str, &'static str);

const WBS: [WbsRow; 12] = [
    (1, None, "U4.CIV", "Civil"),
    (11, Some(1), "U4.CIV.1", "Site preparation and foundations"),
    (12, Some(1), "U4.CIV.2", "Underground and concrete"),
    (2, None, "U4.STL", "Structural steel"),
    (21, Some(2), "U4.STL.1", "Pipe rack"),
    (22, Some(2), "U4.STL.2", "Equipment platforms"),
    (3, None, "U4.PIP", "Piping"),
    (31, Some(3), "U4.PIP.1", "Shop fabrication"),
    (32, Some(3), "U4.PIP.2", "Field erection and testing"),
    (4, None, "U4.ELE", "Electrical"),
    (5, None, "U4.INS", "Instrumentation"),
    (6, None, "U4.COM", "Commissioning and turnover"),
];

/// Link types: FS, SS, FF.
const FS: &str = "PR_FS";
const SS: &str = "PR_SS";
const FF: &str = "PR_FF";

/// One activity: id, name, WBS, working days, predecessors (id, type, lag days).
type Spec = (
    &'static str,
    &'static str,
    i64,
    i64,
    &'static [(&'static str, &'static str, i64)],
);

const ACTIVITIES: &[Spec] = &[
    ("CIV-1000", "Notice to proceed", 11, 0, &[]),
    (
        "CIV-1010",
        "Mobilize and set up site offices",
        11,
        5,
        &[("CIV-1000", FS, 0)],
    ),
    (
        "CIV-1020",
        "Survey and stake foundation layout",
        11,
        3,
        &[("CIV-1010", FS, 0)],
    ),
    (
        "CIV-1030",
        "Clear and grade Unit 4 area",
        11,
        10,
        &[("CIV-1010", FS, 0)],
    ),
    (
        "CIV-1040",
        "Excavate rack and equipment footings",
        11,
        8,
        &[("CIV-1020", FS, 0), ("CIV-1030", FS, 0)],
    ),
    (
        "CIV-1050",
        "Place rebar and pour rack piers",
        11,
        10,
        &[("CIV-1040", FS, 0)],
    ),
    (
        "CIV-1100",
        "Install underground process sewer",
        12,
        15,
        &[("CIV-1030", FS, 0)],
    ),
    (
        "CIV-1110",
        "Install underground electrical duct bank",
        12,
        12,
        &[("CIV-1030", FS, 0)],
    ),
    (
        "CIV-1120",
        "Backfill and compact underground",
        12,
        6,
        &[("CIV-1100", FS, 0), ("CIV-1110", FS, 0)],
    ),
    (
        "CIV-1130",
        "Place rebar and forms - pump pads",
        12,
        6,
        &[("CIV-1120", FS, 0)],
    ),
    (
        "CIV-1140",
        "Pour pump pads and equipment pads",
        12,
        5,
        &[("CIV-1130", FS, 0)],
    ),
    (
        "CIV-1150",
        "Cure concrete and strip forms",
        12,
        7,
        &[("CIV-1140", FS, 0)],
    ),
    (
        "CIV-1160",
        "Grout pump baseplates",
        12,
        4,
        &[("CIV-1150", FS, 0)],
    ),
    (
        "STL-2010",
        "Deliver structural steel to site",
        21,
        20,
        &[("CIV-1000", FS, 0)],
    ),
    (
        "STL-2020",
        "Set pipe rack steel - Area 1",
        21,
        8,
        &[("CIV-1050", FS, 0), ("STL-2010", FS, 0)],
    ),
    (
        "STL-2030",
        "Set pipe rack steel - Area 2",
        21,
        8,
        &[("STL-2020", SS, 3), ("STL-2010", FS, 0)],
    ),
    (
        "STL-2040",
        "Bolt up and plumb rack - Area 1",
        21,
        4,
        &[("STL-2020", FS, 0)],
    ),
    (
        "STL-2050",
        "Bolt up and plumb rack - Area 2",
        21,
        4,
        &[("STL-2030", FS, 0), ("STL-2040", FS, 0)],
    ),
    (
        "STL-2100",
        "Set pump area platforms and ladders",
        22,
        6,
        &[("CIV-1150", FS, 0)],
    ),
    (
        "STL-2110",
        "Install handrail and grating",
        22,
        5,
        &[("STL-2100", FS, 0)],
    ),
    (
        "STL-2120",
        "Set support steel - Reactor R-410",
        22,
        5,
        &[("CIV-1150", FS, 0)],
    ),
    (
        "PIP-3010",
        "Fabricate spools - Area 1",
        31,
        15,
        &[("CIV-1000", FS, 10)],
    ),
    (
        "PIP-3020",
        "Fabricate spools - Area 2",
        31,
        15,
        &[("CIV-1000", FS, 15)],
    ),
    (
        "PIP-3030",
        "Deliver spools to laydown yard",
        31,
        3,
        &[("PIP-3010", FS, 0), ("PIP-3020", FS, 0)],
    ),
    (
        "PIP-3100",
        "Erect pipe rack piping - Area 1",
        32,
        15,
        &[("PIP-3030", FS, 0), ("STL-2040", FS, 0)],
    ),
    (
        "PIP-3110",
        "Erect pipe rack piping - Area 2",
        32,
        15,
        &[("PIP-3100", SS, 5), ("STL-2050", FS, 0)],
    ),
    (
        "PIP-3120",
        "Install pump suction and discharge piping",
        32,
        10,
        &[("CIV-1160", FS, 0), ("PIP-3030", FS, 0)],
    ),
    (
        "PIP-3130",
        "Weld tie-ins to existing header",
        32,
        5,
        &[("PIP-3100", FS, 0)],
    ),
    (
        "PIP-3140",
        "X-ray tie-in welds",
        32,
        3,
        &[("PIP-3130", FS, 0)],
    ),
    (
        "PIP-3160",
        "Hydrotest Area 1 piping",
        32,
        4,
        &[("PIP-3100", FS, 0), ("PIP-3140", FS, 0)],
    ),
    (
        "PIP-3170",
        "Hydrotest Area 2 piping",
        32,
        4,
        &[
            ("PIP-3110", FS, 0),
            ("PIP-3120", FS, 0),
            ("PIP-3160", FS, 0),
        ],
    ),
    (
        "PIP-3180",
        "Insulate and field paint piping",
        32,
        8,
        &[("PIP-3170", FS, 0)],
    ),
    (
        "ELE-4010",
        "Set motor control center",
        4,
        4,
        &[("CIV-1110", FS, 0)],
    ),
    (
        "ELE-4020",
        "Pull power cable - pump motors",
        4,
        8,
        &[("ELE-4010", FS, 0), ("CIV-1160", FS, 0)],
    ),
    (
        "ELE-4030",
        "Install cable tray on pipe rack",
        4,
        10,
        &[("STL-2040", FS, 0)],
    ),
    (
        "ELE-4040",
        "Pull cable in rack tray",
        4,
        10,
        &[("ELE-4030", FS, 0)],
    ),
    (
        "ELE-4050",
        "Terminate and megger power cables",
        4,
        6,
        &[("ELE-4020", FS, 0), ("ELE-4040", FS, 0)],
    ),
    (
        "ELE-4060",
        "Install area lighting and receptacles",
        4,
        7,
        &[("ELE-4030", FS, 0)],
    ),
    (
        "INS-5010",
        "Install field instruments - Area 1",
        5,
        8,
        &[("PIP-3100", FS, 0)],
    ),
    (
        "INS-5020",
        "Install field instruments - Area 2",
        5,
        8,
        &[("PIP-3110", FS, 0)],
    ),
    (
        "INS-5030",
        "Run instrument air tubing",
        5,
        7,
        &[("INS-5010", SS, 3), ("INS-5020", FF, 0)],
    ),
    (
        "INS-5040",
        "Pull and terminate instrument wiring",
        5,
        10,
        &[
            ("INS-5010", FS, 0),
            ("INS-5020", FS, 0),
            ("ELE-4030", FS, 0),
        ],
    ),
    (
        "INS-5050",
        "Calibrate and loop check - Area 1",
        5,
        6,
        &[
            ("INS-5030", FS, 0),
            ("INS-5040", FS, 0),
            ("ELE-4050", FS, 0),
        ],
    ),
    (
        "INS-5060",
        "Calibrate and loop check - Area 2",
        5,
        6,
        &[("INS-5050", FS, 0)],
    ),
    (
        "COM-6010",
        "Mechanical completion walkdown",
        6,
        3,
        &[
            ("PIP-3180", FS, 0),
            ("INS-5060", FS, 0),
            ("ELE-4060", FS, 0),
            ("STL-2110", FS, 0),
            ("STL-2120", FS, 0),
        ],
    ),
    (
        "COM-6020",
        "Punch list repairs",
        6,
        6,
        &[("COM-6010", FS, 0)],
    ),
    (
        "COM-6030",
        "Pre-startup safety review",
        6,
        2,
        &[("COM-6020", FS, 0)],
    ),
    (
        "COM-6040",
        "Run pumps on water",
        6,
        4,
        &[("COM-6030", FS, 0)],
    ),
    (
        "COM-6050",
        "Turnover to operations",
        6,
        0,
        &[("COM-6040", FS, 0)],
    ),
];

/// Actual progress for one activity: start, finish (if done) and the
/// remaining working days (if started and not done).
struct Actual {
    start: String,
    finish: Option<String>,
    remaining_days: i64,
}

fn project_document(data_date: &str, actuals: &HashMap<&str, Actual>) -> Document {
    let mut doc = Document {
        header: "ERMHDR\t25.12\t2026-03-30\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD".into(),
        tables: vec![],
        windows_encoding: true,
        warnings: vec![],
        lone_quotes: Default::default(),
        id_floor: Default::default(),
    };
    doc.table_mut("PROJECT").add(&[
        ("proj_id", "1".into()),
        ("proj_short_name", "UNIT4-EXP".into()),
        ("last_recalc_date", data_date.into()),
        ("plan_start_date", "2026-01-05 08:00".into()),
        ("clndr_id", "1".into()),
        ("rem_target_link_flag", "Y".into()),
    ]);
    let week = (1..=7)
        .map(|d| {
            if (2..=6).contains(&d) {
                format!("(0||{d}()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))")
            } else {
                format!("(0||{d}()())")
            }
        })
        .collect::<String>();
    doc.table_mut("CALENDAR").add(&[
        ("clndr_id", "1".into()),
        ("clndr_name", "Standard 5-day / 8-hour".into()),
        (
            "clndr_data",
            format!("(0||CalendarData()((0||DaysOfWeek()({week}))(0||Exceptions()())))"),
        ),
    ]);
    // The project's own WBS node names the project.
    doc.table_mut("PROJWBS").add(&[
        ("wbs_id", "100".into()),
        ("proj_id", "1".into()),
        ("wbs_short_name", "UNIT4-EXP".into()),
        ("wbs_name", "Unit 4 Expansion".into()),
        ("proj_node_flag", "Y".into()),
        ("seq_num", "0".into()),
    ]);
    for (seq, (id, parent, code, name)) in WBS.iter().enumerate() {
        doc.table_mut("PROJWBS").add(&[
            ("wbs_id", id.to_string()),
            ("proj_id", "1".into()),
            ("wbs_short_name", (*code).into()),
            ("wbs_name", (*name).into()),
            ("parent_wbs_id", parent.unwrap_or(100).to_string()),
            ("seq_num", (seq + 1).to_string()),
        ]);
    }
    let task_id = |code: &str| 1000 + ACTIVITIES.iter().position(|a| a.0 == code).unwrap() as i64;
    for (id, name, wbs, days, _) in ACTIVITIES {
        let milestone = *days == 0;
        let mut row = vec![
            ("task_id", task_id(id).to_string()),
            ("proj_id", "1".into()),
            ("wbs_id", wbs.to_string()),
            ("clndr_id", "1".into()),
            ("task_code", (*id).into()),
            ("task_name", (*name).into()),
            (
                "task_type",
                match (milestone, *id == "CIV-1000") {
                    (true, true) => "TT_Mile",
                    (true, false) => "TT_FinMile",
                    _ => "TT_Task",
                }
                .into(),
            ),
            ("duration_type", "DT_FixedDUR2".into()),
            ("target_drtn_hr_cnt", (days * 8).to_string()),
        ];
        match actuals.get(id) {
            None => {
                row.push(("status_code", "TK_NotStart".into()));
                row.push(("remain_drtn_hr_cnt", (days * 8).to_string()));
            }
            Some(a) => {
                row.push(("act_start_date", a.start.clone()));
                match &a.finish {
                    Some(finish) => {
                        row.push(("status_code", "TK_Complete".into()));
                        row.push(("act_end_date", finish.clone()));
                        row.push(("remain_drtn_hr_cnt", "0".into()));
                        row.push(("phys_complete_pct", "100".into()));
                    }
                    None => {
                        row.push(("status_code", "TK_Active".into()));
                        row.push(("remain_drtn_hr_cnt", (a.remaining_days * 8).to_string()));
                        let done = (days - a.remaining_days) as f64 / *days as f64 * 100.;
                        row.push(("phys_complete_pct", format!("{done:.0}")));
                    }
                }
            }
        }
        doc.table_mut("TASK").add(&row);
        if let Some((rsrc, size)) = crew_of(id) {
            let hours = |d: i64| format!("{}", size * 8. * d as f64);
            let (remaining, actual) = match actuals.get(id) {
                None => (*days, 0),
                Some(a) => (a.remaining_days, days - a.remaining_days),
            };
            let mut row = vec![
                ("taskrsrc_id", task_id(id).to_string()),
                ("task_id", task_id(id).to_string()),
                ("proj_id", "1".into()),
                ("rsrc_id", rsrc.to_string()),
                ("target_qty", hours(*days)),
                ("target_qty_per_hr", size.to_string()),
                ("remain_qty", hours(remaining)),
                ("remain_qty_per_hr", size.to_string()),
                ("act_reg_qty", hours(actual)),
                ("rollup_dates_flag", "Y".into()),
            ];
            if let Some(a) = actuals.get(id) {
                row.push(("act_start_date", a.start.clone()));
                if let Some(finish) = &a.finish {
                    row.push(("act_end_date", finish.clone()));
                }
            }
            doc.table_mut("TASKRSRC").add(&row);
        }
    }
    for (seq, (id, code, name, limit)) in CREWS.iter().enumerate() {
        doc.table_mut("RSRC").add(&[
            ("rsrc_id", id.to_string()),
            ("clndr_id", "1".into()),
            ("rsrc_short_name", (*code).into()),
            ("rsrc_name", (*name).into()),
            ("rsrc_type", "RT_Labor".into()),
            ("def_qty_per_hr", "1".into()),
            ("active_flag", "Y".into()),
            ("rsrc_seq_num", (seq + 1).to_string()),
        ]);
        doc.table_mut("RSRCRATE").add(&[
            ("rsrc_rate_id", id.to_string()),
            ("rsrc_id", id.to_string()),
            ("max_qty_per_hr", limit.to_string()),
            ("start_date", "2026-01-01 00:00".into()),
        ]);
    }
    let mut next = 1;
    for (id, _, _, _, preds) in ACTIVITIES {
        for (pred, kind, lag) in *preds {
            doc.table_mut("TASKPRED").add(&[
                ("task_pred_id", next.to_string()),
                ("proj_id", "1".into()),
                ("pred_proj_id", "1".into()),
                ("pred_task_id", task_id(pred).to_string()),
                ("task_id", task_id(id).to_string()),
                ("pred_type", (*kind).into()),
                ("lag_hr_cnt", (lag * 8).to_string()),
            ]);
            next += 1;
        }
    }
    doc
}

fn load(doc: Document) -> ProjectData {
    let mut project =
        ProjectData::from_document(doc, 1, "Unit 4 sample".into()).expect("sample project loads");
    schedule_model(&mut project.model, &project.resources).unwrap();
    project
}

/// The sample at its original plan, and the same logic after nine weeks of
/// work: what the plan had finished is complete, what straddles the data
/// date is in progress, and a few of those have slipped.
pub(crate) fn sample_schedules() -> (ProjectData, ProjectData) {
    let plan = load(project_document("2026-01-05 08:00", &HashMap::new()));
    let m = &plan.model;
    let data_date = "2026-03-30 08:00";
    let dd = openxer_xer::parse_absolute(data_date).unwrap() - m.origin_serial * DAY;
    let when = |v: i64| date_string(m.origin_serial, Some(v));
    let mut actuals = HashMap::new();
    for (spec, a) in ACTIVITIES.iter().zip(&m.activities) {
        let (start, finish) = (a.early_start.unwrap(), a.early_finish.unwrap());
        if finish <= dd {
            actuals.insert(
                spec.0,
                Actual {
                    start: when(start),
                    finish: Some(when(finish)),
                    remaining_days: 0,
                },
            );
        } else if start < dd {
            let planned = ((finish - dd) as f64 / (finish - start) as f64 * spec.3 as f64)
                .round()
                .max(1.) as i64;
            // Three of the activities in progress are running behind.
            let slip = match spec.0 {
                "PIP-3110" => 3,
                "PIP-3120" | "ELE-4020" => 2,
                _ => 0,
            };
            actuals.insert(
                spec.0,
                Actual {
                    start: when(start),
                    finish: None,
                    remaining_days: (planned + slip).min(spec.3),
                },
            );
        }
    }
    (plan, load(project_document(data_date, &actuals)))
}

/// The app on the updated sample, its original plan assigned as the
/// project baseline, in a headless painter at 1600 x 900.
pub(crate) struct Shot {
    pub(crate) ctx: egui::Context,
    pub(crate) app: App,
    pub(crate) painter: SoftwarePainter,
    pub(crate) time: f64,
    pub(crate) dir: PathBuf,
}

impl Shot {
    pub(crate) fn new(name: &str) -> Self {
        let (plan, current) = sample_schedules();
        // The status bar shows the workspace file name and the user name.
        std::env::set_var("USERNAME", "scheduler");
        let dir =
            std::env::temp_dir().join(format!("openxer-readme-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::open(dir.join("workspace.db")).unwrap();
        store.save(&plan).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, plan);
        // The original plan becomes the project baseline, then progress is
        // entered over it.
        let id = app.add_baseline_copy(1.).unwrap();
        let users = crate::baselines::store::UserBaselines([
            crate::baselines::store::Choice::None,
            crate::baselines::store::Choice::None,
            crate::baselines::store::Choice::None,
        ]);
        app.assign_baselines(
            Some(crate::baselines::store::Choice::Baseline(id)),
            users,
            2.,
        )
        .unwrap();
        // The project row names its baseline; carry that onto the updated project.
        let mut current = current;
        let baseline = openxer_interchange::baseline::project_value(
            &app.project.document,
            1,
            "sum_base_proj_id",
        );
        openxer_interchange::baseline::set_project_value(
            &mut current.document,
            1,
            "sum_base_proj_id",
            &baseline,
        );
        app.project = current;
        app.baselines_changed();
        let mut shot = Self {
            ctx,
            app,
            painter: SoftwarePainter::default(),
            time: 0.,
            dir,
        };
        shot.columns(&[
            ("activity_id", 82.),
            ("activity_name", 246.),
            ("original_duration", 62.),
            ("remaining_duration", 70.),
            ("start", 98.),
            ("finish", 98.),
            ("total_float", 62.),
        ]);
        let app = &mut shot.app;
        app.grid.layout.group_sort.levels = vec![GroupLevel {
            column: "wbs".into(),
            to_level: None,
            indent: true,
            interval: String::new(),
        }];
        app.grid.layout.group_sort.show_code = false;
        app.gantt.options.progress_line.show = true;
        for bar in &mut app.gantt.bars {
            match bar.name.as_str() {
                // The baseline bar sits under a half-height bar.
                "Project Baseline Bar" => bar.display = true,
                "Actual Work" | "Remaining Work" | "Critical Remaining Work" => {
                    bar.middle.shape = crate::gantt::settings::BodyShape::UpperHalf;
                }
                _ => {}
            }
        }
        app.dirty = false;
        app.status = "Ready".into();
        app.grid_fraction = 0.475;
        shot.painter.screen = Some(Vec2::new(1600., 900.));
        shot.frames(4);
        shot
    }

    pub(crate) fn columns(&mut self, columns: &[(&str, f32)]) {
        self.app.grid.layout.columns = columns
            .iter()
            .map(|&(id, w)| ColumnSetting::new(id, w))
            .collect();
    }

    pub(crate) fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let _ = self
                .painter
                .frame(&mut self.app, &self.ctx, &mut self.time, vec![], false);
        }
    }

    /// Shows `view` in the bottom pane, `height` pixels tall.
    pub(crate) fn bottom(&mut self, view: BottomView, height: f32) {
        self.app.set_bottom(Some(view));
        self.frames(2);
        let id = self.app.activity_pane_id();
        self.ctx.data_mut(|d| {
            d.insert_persisted(
                id,
                egui::containers::panel::PanelState {
                    rect: Rect::from_min_size(
                        Pos2::new(0., 890. - height),
                        Vec2::new(1600., height),
                    ),
                },
            )
        });
        self.frames(2);
    }

    /// Folds the WBS bands whose label holds one of `labels`, and every band
    /// that is all complete, then fits the timescale to the rest.
    pub(crate) fn fold(&mut self, labels: &[&str]) {
        self.frames(2);
        let _ = self.app.rows();
        let app = &self.app;
        let folded: std::collections::BTreeSet<String> = app
            .grid
            .bands
            .borrow()
            .iter()
            .filter(|b| {
                labels.iter().any(|l| b.label.contains(l))
                    || b.members
                        .iter()
                        .all(|&i| app.project.model.activities[i].status == StatusType::Completed)
            })
            .map(|b| b.key.clone())
            .collect();
        self.app.collapsed = folded;
        self.app.fit();
        // Leave room right of the last bars for their labels.
        let width = self.app.chart_width;
        self.app.pixels_per_day *= ((width - 250.) / width).max(0.5);
        self.app.update_timescale_units();
    }

    pub(crate) fn select(&mut self, code: &str) {
        let at = self
            .app
            .project
            .model
            .activities
            .iter()
            .position(|a| a.task_code == code)
            .unwrap();
        self.app.select_activities(&[at]);
    }

    pub(crate) fn click(&mut self, action: &str) {
        self.painter
            .click_action(&mut self.app, &self.ctx, &mut self.time, action);
        self.frames(2);
    }

    /// Paints the window as it is now.
    pub(crate) fn picture(&mut self, name: &str) -> image::RgbaImage {
        self.frames(4);
        let (picture, triangles) = self
            .painter
            .frame(&mut self.app, &self.ctx, &mut self.time, vec![], true)
            .unwrap();
        assert!(triangles > 1000, "{name}: {triangles} triangles");
        assert!(self.app.error.is_none(), "{name}: {:?}", self.app.error);
        picture
    }

    /// Paints the window and writes `docs/images/<name>.png` as compressed RGB.
    fn save(&mut self, name: &str) {
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("docs/images");
        let picture = self.picture(name);
        write_png(&picture, &out.join(format!("{name}.png")));
    }
}

/// Writes `picture` as a compressed RGB PNG at `path`.
pub(crate) fn write_png(picture: &image::RgbaImage, path: &Path) {
    use image::ImageEncoder;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    let rgb = image::DynamicImage::ImageRgba8(picture.clone()).into_rgb8();
    let file = std::fs::File::create(path).unwrap();
    image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )
    .unwrap();
}

impl Drop for Shot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The sample the screenshots show has crews on the work and some of it in
/// progress.
#[test]
fn readme_sample_has_crews_and_progress() {
    let (_, current) = sample_schedules();
    let m = &current.model;
    let pipefitters = |m: &Model| {
        m.activities
            .iter()
            .filter(|a| a.assignments.iter().any(|x| x.rsrc_id == 3))
            .count()
    };
    assert_eq!(pipefitters(m), 6);
    assert!(m
        .activities
        .iter()
        .any(|a| a.status == StatusType::InProgress));
}

#[test]
#[ignore = "regenerates docs/images/hero.png"]
fn render_readme_hero() {
    let mut s = Shot::new("hero");
    s.bottom(BottomView::Details, 240.);
    s.app.detail.tab = DetailTab::Relationships;
    // Fold the finished groups, Structural steel and Electrical, so the rows that matter fit.
    s.fold(&["Electrical", "Structural steel"]);
    s.select("PIP-3110");
    s.save("hero");
}

#[test]
#[ignore = "regenerates the other docs/images screenshots"]
fn render_readme_screens() {
    // Resources: the pipefitters' usage profile against their crew limit,
    // then the same after leveling.
    let mut s = Shot::new("resources");
    s.app.panes.usage.resource = Some(3);
    s.bottom(BottomView::ResourceUsageProfile, 330.);
    s.fold(&["Civil", "Structural steel", "Electrical", "Commissioning"]);
    s.select("PIP-3120");
    s.save("resource-usage");
    s.app.open_level_resources();
    s.frames(3);
    s.click("level_run");
    s.fold(&["Civil", "Structural steel", "Electrical", "Commissioning"]);
    s.select("PIP-3130");
    s.save("leveled");

    // Logic: Trace Logic under the table for an activity on the critical path.
    let mut s = Shot::new("trace");
    s.bottom(BottomView::TraceLogic, 330.);
    s.fold(&["Civil", "Structural steel", "Commissioning"]);
    s.select("INS-5040");
    s.save("trace-logic");

    // Check Schedule results.
    let mut s = Shot::new("check");
    s.app.set_bottom(None);
    s.fold(&["Electrical", "Structural steel"]);
    s.select("PIP-3110");
    // The report file goes to the sample's folder, and its path stays off the picture.
    s.app.sched.check_report_path = s.dir.join("check.html").display().to_string();
    s.app.open_check_schedule();
    s.frames(3);
    s.click("check_run");
    s.app.sched.check_report_written = None;
    s.save("check-schedule");

    // Progress: Update Progress over the table with the status columns.
    let mut s = Shot::new("progress");
    s.columns(&[
        ("activity_id", 82.),
        ("activity_name", 236.),
        ("activity_status", 90.),
        ("actual_start", 98.),
        ("actual_finish", 98.),
        ("remaining_duration", 70.),
        ("physical_percent_complete", 70.),
    ]);
    s.app.grid_fraction = 0.485;
    s.app.set_bottom(None);
    s.fold(&["Civil", "Commissioning"]);
    s.app.toggle_spotlight();
    s.select("PIP-3120");
    s.app.open_update_progress();
    s.save("update-progress");

    // Baselines: the current dates against the baseline, with variance.
    let mut s = Shot::new("baseline");
    s.columns(&[
        ("activity_id", 82.),
        ("activity_name", 236.),
        ("bl_project_start", 98.),
        ("bl_project_finish", 98.),
        ("start", 98.),
        ("finish", 98.),
        ("variance_bl_project_finish", 72.),
    ]);
    s.app.grid_fraction = 0.52;
    s.app.set_bottom(None);
    s.fold(&["Civil", "Commissioning"]);
    s.save("baseline");

    // Print Preview of the Gantt layout.
    let mut s = Shot::new("print");
    s.app.set_bottom(None);
    s.fold(&["Electrical", "Structural steel"]);
    s.click("tool_print_preview");
    // Fit the page width, so the printed text reads, and show its top.
    let preview = s.app.print.preview.as_mut().unwrap();
    preview.view.fit = crate::print::viewer::Fit::Width;
    s.frames(2);
    s.app.print.preview.as_mut().unwrap().nav.scroll = Vec2::ZERO;
    s.save("print-preview");
}

#[test]
#[ignore = "regenerates docs/images/ai-chat.png and docs/images/ai-skills.png"]
fn render_readme_ai() {
    use crate::chat::backend::{AgentEvent, ChangeRow, FakeBackend, Mode, Usage};

    // AI Chat docked beside the activity table and Gantt chart on the synthetic sample.
    let mut s = Shot::new("ai-chat");
    s.bottom(BottomView::Details, 210.);
    s.app.detail.tab = DetailTab::Relationships;
    s.fold(&["Electrical", "Structural steel"]);
    s.select("PIP-3110");
    let (fake, _log) = FakeBackend::new();
    s.app.chat.backend = Box::new(fake);
    s.app.chat.docked = true;
    s.app.chat.dock_width = 440.;
    s.app.chat.mode = Mode::Edit;
    s.app.chat.open = true;
    s.frames(3);
    let c = &mut s.app.chat.conv;
    c.send("Why does PIP-3110 have zero float?");
    for e in [
        AgentEvent::ToolStarted {
            call: "1".into(),
            tool: "inspect".into(),
            target: "UNIT4-EXP".into(),
        },
        AgentEvent::ToolFinished {
            call: "1".into(),
            ok: true,
            millis: 240,
        },
        AgentEvent::ToolStarted {
            call: "2".into(),
            tool: "trace logic".into(),
            target: "PIP-3110".into(),
        },
        AgentEvent::ToolFinished {
            call: "2".into(),
            ok: true,
            millis: 95,
        },
        AgentEvent::Message(
            "PIP-3110 Erect pipe rack piping - Area 2 is on the longest path. It finishes 07-Apr-26 17:00 and drives INS-5020 Install field instruments - Area 2 (FS, 0d lag), which starts the next morning.\n\nFrom there the chain runs through instrument wiring and loop checks to the 09-Jun-26 finish with no slack. Its other successor, PIP-3170, has 13d of float."
                .into(),
        ),
        AgentEvent::TurnFinished {
            usage: Some(Usage {
                input: 1180,
                output: 142,
            }),
        },
    ] {
        c.apply(e);
    }
    c.send("Mark it 80% complete as of the data date");
    for e in [
        AgentEvent::ToolStarted {
            call: "3".into(),
            tool: "set progress".into(),
            target: "PIP-3110".into(),
        },
        AgentEvent::ToolFinished {
            call: "3".into(),
            ok: true,
            millis: 80,
        },
        AgentEvent::Proposed {
            proposal: 1,
            rows: vec![
                ChangeRow {
                    activity: "PIP-3110".into(),
                    field: "Remaining Duration".into(),
                    old: "7d".into(),
                    new: "3d".into(),
                },
                ChangeRow {
                    activity: "PIP-3110".into(),
                    field: "Duration % Complete".into(),
                    old: "53%".into(),
                    new: "80%".into(),
                },
            ],
        },
        AgentEvent::TurnFinished { usage: None },
    ] {
        c.apply(e);
    }
    s.frames(4);
    s.save("ai-chat");

    // AI Agents > Skills library showing the built-in scheduling skills.
    let mut s = Shot::new("ai-skills");
    s.app.set_bottom(None);
    s.fold(&["Electrical", "Structural steel"]);
    s.select("PIP-3110");
    s.app.skills.dir = Some(s.dir.join("skills"));
    s.app.open_skills();
    if let Some(d) = s.app.skills.list.as_mut() {
        if let Some(i) = d
            .catalog
            .entries
            .iter()
            .position(|e| e.skill.id == "logic-health-check")
        {
            d.selected = Some(i);
        }
    }
    s.frames(4);
    s.save("ai-skills");
}

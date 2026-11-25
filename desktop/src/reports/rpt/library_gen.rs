//! Builds OpenXER's own starter reports with the editing functions. The
//! committed files under `assets/openxer_reports/` are this output; a test
//! checks they are current (set `OPENXER_WRITE_LIBRARY=1` to rewrite them).
use super::edit::{self, CellSpec, Kind, NewCell, Path};
use super::filter::{self, Filter, Op, SortKey};
use super::stream::Stream;

const WIDTH: i64 = 960;

struct Col {
    field: &'static str,
    title: &'static str,
    width: i64,
    right: bool,
}

const fn col(field: &'static str, title: &'static str, width: i64, right: bool) -> Col {
    Col {
        field,
        title,
        width,
        right,
    }
}

struct Rb {
    s: Stream,
}

fn cmp(field: &str, op: Op, value: &str) -> Filter {
    Filter::Cmp {
        field: field.into(),
        op,
        values: vec![value.into()],
    }
}

fn sort(field: &str, descending: bool) -> SortKey {
    SortKey {
        field: field.into(),
        descending,
    }
}

impl Rb {
    /// A landscape report with the title block, a note line and the column
    /// titles in its page header.
    fn new(title: &str, note: &str, cols: &[Col]) -> Rb {
        let mut s = edit::new_report(title, true);
        let head = edit::container_path(&s, Kind::PageHeader).expect("page header");
        let r = edit::add_row(&mut s, &head, 28).expect("row");
        edit::add_cell(
            &mut s,
            &r,
            &CellSpec::new(NewCell::Variable("[title]".into()), 0, WIDTH)
                .heading()
                .size(19),
        )
        .expect("cell");
        let r = edit::add_row(&mut s, &head, 18).expect("row");
        edit::add_cell(
            &mut s,
            &r,
            &CellSpec::new(
                NewCell::Variable("Project: [project_code]  [project_name]".into()),
                0,
                560,
            ),
        )
        .expect("cell");
        edit::add_cell(
            &mut s,
            &r,
            &CellSpec::new(NewCell::Variable("Data date: [data_date]".into()), 560, 400).right(),
        )
        .expect("cell");
        if !note.is_empty() {
            let r = edit::add_row(&mut s, &head, 18).expect("row");
            edit::add_cell(
                &mut s,
                &r,
                &CellSpec::new(NewCell::Text(note.into()), 0, WIDTH),
            )
            .expect("cell");
        }
        let rb = Rb { s };
        let mut rb = rb;
        rb.titles(&head, cols);
        rb
    }

    fn titles(&mut self, holder: &Path, cols: &[Col]) {
        let r = edit::add_row(&mut self.s, holder, 20).expect("row");
        let mut left = 0;
        for c in cols {
            let mut spec = CellSpec::new(
                NewCell::Title {
                    field: c.field.into(),
                    title: c.title.into(),
                },
                left,
                c.width,
            )
            .heading()
            .underlined();
            spec.right = c.right;
            edit::add_cell(&mut self.s, &r, &spec).expect("cell");
            left += c.width;
        }
    }

    fn detail(&self) -> Path {
        edit::container_path(&self.s, Kind::Detail).expect("section")
    }

    /// A row of data fields.
    fn data_row(&mut self, holder: &Path, cols: &[Col], height: i64) {
        let r = edit::add_row(&mut self.s, holder, height).expect("row");
        let mut left = 0;
        for c in cols {
            let mut spec = CellSpec::new(NewCell::Field(c.field.into()), left, c.width);
            spec.right = c.right;
            edit::add_cell(&mut self.s, &r, &spec).expect("cell");
            left += c.width;
        }
    }

    fn set_group(&mut self, g: &Path, filter: Option<&Filter>, sorts: &[SortKey]) {
        let c = edit::get_mut(&mut self.s.root, g).expect("group");
        if let Some(f) = filter.filter(|f| !f.is_empty()) {
            edit::set_lines(c, "Filter.Strings", &filter::write(f));
        }
        edit::set_lines(c, "SortBy.Strings", &filter::write_sort(sorts));
    }

    /// A one-record section with a heading line, for reports that show
    /// several lists under headings.
    fn heading_section(&mut self, text: &str) -> Path {
        let detail = self.detail();
        let g = edit::add_group(&mut self.s, &detail, "PROJECT", None, false).expect("group");
        let h = edit::group_part(&self.s, &g, Kind::GroupHeader).expect("header");
        let r = edit::add_row(&mut self.s, &h, 24).expect("row");
        edit::add_cell(
            &mut self.s,
            &r,
            &CellSpec::new(NewCell::Text(text.into()), 0, WIDTH)
                .heading()
                .size(16),
        )
        .expect("cell");
        edit::group_part(&self.s, &g, Kind::Detail).expect("detail")
    }

    fn tasks_in(
        &mut self,
        holder: &Path,
        cols: &[Col],
        filter: &Filter,
        sorts: &[SortKey],
    ) -> Path {
        let g = edit::add_group(&mut self.s, holder, "TASK", None, false).expect("group");
        self.set_group(&g, Some(filter), sorts);
        let body = edit::group_part(&self.s, &g, Kind::Detail).expect("detail");
        self.data_row(&body, cols, 18);
        g
    }

    fn finish(mut self) -> Stream {
        // Group heights are only hints; keep the layout order tidy.
        let d = self.detail();
        if let Some(c) = edit::get_mut(&mut self.s.root, &d) {
            edit::renumber(c);
        }
        self.s.eol = "\n".into();
        self.s.trailer = "\n".into();
        self.s
    }
}

fn not_complete() -> Filter {
    cmp("status_code", Op::Ne, "TK_Complete")
}

/// Activities that overlap the weeks after the data date, by WBS.
fn lookahead(weeks: i64, title: &str) -> Stream {
    let cols = [
        col("task_code", "Activity ID", 90, false),
        col("task_name", "Activity Name", 300, false),
        col("start_date", "Start", 120, true),
        col("end_date", "Finish", 120, true),
        col("remain_drtn_hr_cnt", "Remaining Duration", 140, true),
        col("total_float_hr_cnt", "Total Float", 90, true),
        col("activity_status", "Status", 100, false),
    ];
    let note = format!(
        "Activities not yet complete that start or continue within {weeks} weeks of the data date."
    );
    let mut rb = Rb::new(title, &note, &cols);
    let detail = rb.detail();
    let wbs = edit::add_group(&mut rb.s, &detail, "PROJWBS", None, true).expect("group");
    {
        let c = edit::get_mut(&mut rb.s.root, &wbs).expect("group");
        c.set_flag("HideRowIfNoChildren", true);
        c.set_raw("IncludeParents", "True");
    }
    let h = edit::group_part(&rb.s, &wbs, Kind::GroupHeader).expect("header");
    let r = edit::add_row(&mut rb.s, &h, 20).expect("row");
    let cell = edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("wbs_name".into()), 0, WIDTH).heading(),
    )
    .expect("cell");
    edit::get_mut(&mut rb.s.root, &cell)
        .expect("cell")
        .set_flag("ShowIndent", true);
    let body = edit::group_part(&rb.s, &wbs, Kind::Detail).expect("detail");
    let tasks = edit::add_group(&mut rb.s, &body, "PROJWBS", Some("TASK"), false).expect("group");
    let window = Filter::And(vec![
        not_complete(),
        cmp("start_date", Op::Le, &format!("DD+{}W", weeks)),
        cmp("end_date", Op::Ge, "DD"),
    ]);
    rb.set_group(
        &tasks,
        Some(&window),
        &[sort("start_date", false), sort("task_code", false)],
    );
    let tb = edit::group_part(&rb.s, &tasks, Kind::Detail).expect("detail");
    rb.data_row(&tb, &cols, 18);
    rb.finish()
}

fn by_status() -> Stream {
    let cols = [
        col("task_code", "Activity ID", 90, false),
        col("task_name", "Activity Name", 280, false),
        col("start_date", "Start", 120, true),
        col("end_date", "Finish", 120, true),
        col("complete_pct", "% Complete", 90, true),
        col("total_float_hr_cnt", "Total Float", 90, true),
        col("wbs_name", "WBS", 170, false),
    ];
    let mut rb = Rb::new(
        "Activities by Status with Float",
        "Not started, in progress and completed activities, each with its total float.",
        &cols,
    );
    for (label, status) in [
        ("In Progress", "TK_Active"),
        ("Not Started", "TK_NotStart"),
        ("Completed", "TK_Complete"),
    ] {
        let section = rb.heading_section(label);
        rb.tasks_in(
            &section,
            &cols,
            &cmp("status_code", Op::Eq, status),
            &[sort("start_date", false), sort("task_code", false)],
        );
    }
    rb.finish()
}

fn critical() -> Stream {
    let cols = [
        col("task_code", "Activity ID", 90, false),
        col("task_name", "Activity Name", 280, false),
        col("start_date", "Start", 120, true),
        col("end_date", "Finish", 120, true),
        col("remain_drtn_hr_cnt", "Remaining Duration", 140, true),
        col("total_float_hr_cnt", "Total Float", 90, true),
        col("wbs_name", "WBS", 120, false),
    ];
    let mut rb = Rb::new(
        "Critical Activities",
        "Activities on the critical path that are not complete, in start order.",
        &cols,
    );
    let detail = rb.detail();
    rb.tasks_in(
        &detail,
        &cols,
        &Filter::And(vec![cmp("critical_flag", Op::Eq, "Y"), not_complete()]),
        &[sort("start_date", false), sort("task_code", false)],
    );
    rb.finish()
}

/// Activities that are critical or have less than ten working days of float.
fn critical_and_near() -> Stream {
    let cols = [
        col("task_code", "Activity ID", 90, false),
        col("task_name", "Activity Name", 280, false),
        col("start_date", "Start", 120, true),
        col("end_date", "Finish", 120, true),
        col("remain_drtn_hr_cnt", "Remaining Duration", 140, true),
        col("total_float_hr_cnt", "Total Float", 90, true),
        col("wbs_name", "WBS", 120, false),
    ];
    let mut rb = Rb::new(
        "Critical and Near Critical",
        "Activities that are not complete and are critical or have less than 10 days of total float, least float first.",
        &cols,
    );
    let detail = rb.detail();
    rb.tasks_in(
        &detail,
        &cols,
        &Filter::And(vec![
            not_complete(),
            Filter::Or(vec![
                cmp("critical_flag", Op::Eq, "Y"),
                // Hours: ten days of an eight hour day.
                cmp("total_float_hr_cnt", Op::Lt, "80"),
            ]),
        ]),
        &[sort("total_float_hr_cnt", false), sort("task_code", false)],
    );
    rb.finish()
}

fn progress() -> Stream {
    let cols = [
        col("task_code", "Activity ID", 90, false),
        col("task_name", "Activity Name", 270, false),
        col("act_start_date", "Actual Start", 120, true),
        col("act_end_date", "Actual Finish", 120, true),
        col("complete_pct", "% Complete", 90, true),
        col("remain_drtn_hr_cnt", "Remaining Duration", 140, true),
        col("activity_status", "Status", 130, false),
    ];
    let mut rb = Rb::new(
        "Progress This Period",
        "Activities that started or finished in the seven days before the data date.",
        &cols,
    );
    let detail = rb.detail();
    rb.tasks_in(
        &detail,
        &cols,
        &Filter::Or(vec![
            cmp("act_start_date", Op::Ge, "DD-7D"),
            cmp("act_end_date", Op::Ge, "DD-7D"),
        ]),
        &[sort("act_start_date", false), sort("task_code", false)],
    );
    rb.finish()
}

fn relationships() -> Stream {
    let mut rb = Rb::new(
        "Predecessors and Successors by Activity",
        "Each activity with the activities before and after it, and the date they finish or start.",
        &[
            col("task_code", "Activity ID", 110, false),
            col("task_name", "Activity Name", 380, false),
            col("pred_type", "Type", 60, false),
            col("lag_hr_cnt", "Lag", 80, true),
            col("end_date", "Date", 140, true),
        ],
    );
    let detail = rb.detail();
    let acts = edit::add_group(&mut rb.s, &detail, "TASK", None, false).expect("group");
    rb.set_group(&acts, None, &[sort("task_code", false)]);
    let h = edit::group_part(&rb.s, &acts, Kind::GroupHeader).expect("header");
    let r = edit::add_row(&mut rb.s, &h, 22).expect("row");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("task_code".into()), 0, 110).heading(),
    )
    .expect("cell");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("task_name".into()), 110, 380).heading(),
    )
    .expect("cell");
    let body = edit::group_part(&rb.s, &acts, Kind::Detail).expect("detail");
    for (path, label, other, own) in [
        ("TASKPREDPREDS", "Predecessor", "PREDTASK", "TASK"),
        ("TASKPREDSUCCS", "Successor", "TASK", "PREDTASK"),
    ] {
        let _ = own;
        let g = edit::add_group(&mut rb.s, &body, "TASKPRED", Some(path), false).expect("group");
        rb.set_group(&g, None, &[sort(&format!("{other}|task_code"), false)]);
        let b = edit::group_part(&rb.s, &g, Kind::Detail).expect("detail");
        let r = edit::add_row(&mut rb.s, &b, 18).expect("row");
        let cells: [(NewCell, i64, i64, bool); 6] = [
            (
                NewCell::Text(format!(
                    "{label} {}",
                    if other == "PREDTASK" {
                        "(finishes)"
                    } else {
                        "(starts)"
                    }
                )),
                10,
                135,
                false,
            ),
            (NewCell::Field(format!("{other}|task_code")), 145, 90, false),
            (
                NewCell::Field(format!("{other}|task_name")),
                235,
                255,
                false,
            ),
            (NewCell::Field("pred_type".into()), 490, 60, false),
            (NewCell::Field("lag_hr_cnt".into()), 550, 80, true),
            (
                NewCell::Field(if other == "PREDTASK" {
                    "PREDTASK|end_date".into()
                } else {
                    "TASK|start_date".into()
                }),
                630,
                110,
                true,
            ),
        ];
        for (what, left, width, right) in cells {
            let mut spec = CellSpec::new(what, left, width);
            spec.right = right;
            edit::add_cell(&mut rb.s, &r, &spec).expect("cell");
        }
    }
    rb.finish()
}

fn open_ends() -> Stream {
    let cols = [
        col("task_code", "Activity ID", 90, false),
        col("task_name", "Activity Name", 300, false),
        col("start_date", "Start", 120, true),
        col("end_date", "Finish", 120, true),
        col("total_float_hr_cnt", "Total Float", 90, true),
        col("wbs_name", "WBS", 240, false),
    ];
    let mut rb = Rb::new(
        "Open Ends and Logic Gaps",
        "Activities that are not complete and have no predecessor or no successor.",
        &cols,
    );
    for (label, field) in [
        ("No predecessor", "predecessor_count"),
        ("No successor", "successor_count"),
    ] {
        let section = rb.heading_section(label);
        rb.tasks_in(
            &section,
            &cols,
            &Filter::And(vec![cmp(field, Op::Eq, "0"), not_complete()]),
            &[sort("start_date", false), sort("task_code", false)],
        );
    }
    rb.finish()
}

fn resource_usage() -> Stream {
    let mut rb = Rb::new(
        "Resource Usage by Week",
        "Remaining units of each resource by activity for ten weeks from the data date.",
        &[],
    );
    let head = edit::container_path(&rb.s, Kind::PageHeader).expect("page header");
    let r = edit::add_row(&mut rb.s, &head, 20).expect("row");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Text("Resource / Activity".into()), 0, 230)
            .heading()
            .underlined(),
    )
    .expect("cell");
    let week = edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Variable("[column_date]".into()), 230, 70)
            .heading()
            .right()
            .underlined(),
    )
    .expect("cell");
    edit::get_mut(&mut rb.s.root, &week)
        .expect("cell")
        .set_flag("ShowTimeDistrib", true);
    {
        let root = &mut rb.s.root;
        root.set_raw("DateUnits", "utWeek");
        root.set_string("StartDate", "DD");
        root.set_string("FinishDate", "DD+10W");
    }
    let detail = rb.detail();
    let rs = edit::add_group(&mut rb.s, &detail, "RSRC", None, false).expect("group");
    rb.set_group(&rs, None, &[sort("rsrc_short_name", false)]);
    {
        let c = edit::get_mut(&mut rb.s.root, &rs).expect("group");
        c.set_flag("HideRowIfNoChildren", true);
    }
    let h = edit::group_part(&rb.s, &rs, Kind::GroupHeader).expect("header");
    let r = edit::add_row(&mut rb.s, &h, 22).expect("row");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("rsrc_short_name".into()), 0, 90).heading(),
    )
    .expect("cell");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("rsrc_name".into()), 90, 140).heading(),
    )
    .expect("cell");
    let body = edit::group_part(&rb.s, &rs, Kind::Detail).expect("detail");
    let asg =
        edit::add_group(&mut rb.s, &body, "TASKRSRC", Some("TASKRSRC"), false).expect("group");
    rb.set_group(
        &asg,
        None,
        &[
            sort("TASK|start_date", false),
            sort("TASK|task_code", false),
        ],
    );
    let ab = edit::group_part(&rb.s, &asg, Kind::Detail).expect("detail");
    let r = edit::add_row(&mut rb.s, &ab, 18).expect("row");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("TASK|task_code".into()), 10, 90),
    )
    .expect("cell");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("TASK|task_name".into()), 100, 130),
    )
    .expect("cell");
    let units = edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("remain_qty".into()), 230, 70).right(),
    )
    .expect("cell");
    edit::get_mut(&mut rb.s.root, &units)
        .expect("cell")
        .set_flag("ShowTimeDistrib", true);
    let f = edit::group_part(&rb.s, &rs, Kind::GroupFooter).expect("footer");
    let r = edit::add_row(&mut rb.s, &f, 20).expect("row");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Text("Total".into()), 10, 220).heading(),
    )
    .expect("cell");
    let total = edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(
            NewCell::Summary {
                field: "remain_qty".into(),
                path: "TASKRSRC-RSRC:".into(),
                mode: "amSum".into(),
            },
            230,
            70,
        )
        .heading()
        .right(),
    )
    .expect("cell");
    edit::get_mut(&mut rb.s.root, &total)
        .expect("cell")
        .set_flag("ShowTimeDistrib", true);
    rb.finish()
}

fn baseline_variance() -> Stream {
    let cols = [
        col("task_code", "Activity ID", 80, false),
        col("task_name", "Activity Name", 190, false),
        col("start_date", "Start", 110, true),
        col("bl_project_start", "Baseline Start", 110, true),
        col("variance_bl_project_start", "Start Variance", 105, true),
        col("end_date", "Finish", 110, true),
        col("bl_project_finish", "Baseline Finish", 110, true),
        col("variance_bl_project_finish", "Finish Variance", 105, true),
    ];
    let mut rb = Rb::new(
        "Baseline Variance",
        "Current dates against the project baseline, largest finish slip first. Blank without an assigned baseline.",
        &cols,
    );
    let detail = rb.detail();
    rb.tasks_in(
        &detail,
        &cols,
        &Filter::And(vec![]),
        &[
            sort("variance_bl_project_finish", true),
            sort("task_code", false),
        ],
    );
    rb.finish()
}

fn milestones() -> Stream {
    let cols = [
        col("task_code", "Activity ID", 90, false),
        col("task_name", "Milestone", 300, false),
        col("task_type", "Type", 120, false),
        col("start_date", "Date", 120, true),
        col("total_float_hr_cnt", "Total Float", 90, true),
        col("activity_status", "Status", 100, false),
        col("wbs_name", "WBS", 140, false),
    ];
    let mut rb = Rb::new(
        "Milestones",
        "Start and finish milestones in date order.",
        &cols,
    );
    let detail = rb.detail();
    rb.tasks_in(
        &detail,
        &cols,
        &cmp("task_type", Op::Like, "%Milestone"),
        &[sort("start_date", false), sort("task_code", false)],
    );
    rb.finish()
}

fn wbs_summary() -> Stream {
    let cols = [
        col("wbs_name", "WBS Element", 330, false),
        col("task_cnt", "Activities", 80, true),
        col("start_date", "Start", 120, true),
        col("end_date", "Finish", 120, true),
        col("target_cost", "Budgeted Cost", 100, true),
        col("act_cost", "Actual Cost", 100, true),
        col("remain_cost", "Remaining Cost", 110, true),
    ];
    let mut rb = Rb::new(
        "WBS Summary",
        "Each WBS element with its dates and the costs of the activities below it.",
        &cols,
    );
    let detail = rb.detail();
    let wbs = edit::add_group(&mut rb.s, &detail, "PROJWBS", None, true).expect("group");
    let body = edit::group_part(&rb.s, &wbs, Kind::Detail).expect("detail");
    let _ = body;
    let h = edit::group_part(&rb.s, &wbs, Kind::GroupHeader).expect("header");
    let r = edit::add_row(&mut rb.s, &h, 20).expect("row");
    let mut left = 0;
    for (k, c) in cols.iter().enumerate() {
        let mut spec = CellSpec::new(NewCell::Field(c.field.into()), left, c.width);
        spec.right = c.right;
        if k == 0 {
            spec = spec.heading();
        }
        let cell = edit::add_cell(&mut rb.s, &r, &spec).expect("cell");
        if k == 0 {
            edit::get_mut(&mut rb.s.root, &cell)
                .expect("cell")
                .set_flag("ShowIndent", true);
        }
        left += c.width;
    }
    rb.finish()
}

fn assignments() -> Stream {
    let cols = [
        col("TASK|task_code", "Activity ID", 90, false),
        col("TASK|task_name", "Activity Name", 270, false),
        col("target_qty", "Budgeted Units", 110, true),
        col("act_qty", "Actual Units", 100, true),
        col("remain_qty", "Remaining Units", 120, true),
        col("remain_cost", "Remaining Cost", 120, true),
        col("start_date", "Start", 120, true),
    ];
    let mut rb = Rb::new(
        "Resource Assignments by Resource",
        "The activities each resource is assigned to, with units and remaining cost.",
        &cols,
    );
    let detail = rb.detail();
    let rs = edit::add_group(&mut rb.s, &detail, "RSRC", None, false).expect("group");
    rb.set_group(&rs, None, &[sort("rsrc_short_name", false)]);
    edit::get_mut(&mut rb.s.root, &rs)
        .expect("group")
        .set_flag("HideRowIfNoChildren", true);
    let h = edit::group_part(&rb.s, &rs, Kind::GroupHeader).expect("header");
    let r = edit::add_row(&mut rb.s, &h, 22).expect("row");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("rsrc_short_name".into()), 0, 100).heading(),
    )
    .expect("cell");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Field("rsrc_name".into()), 100, 300).heading(),
    )
    .expect("cell");
    let body = edit::group_part(&rb.s, &rs, Kind::Detail).expect("detail");
    let asg =
        edit::add_group(&mut rb.s, &body, "TASKRSRC", Some("TASKRSRC"), false).expect("group");
    rb.set_group(&asg, None, &[sort("TASK|start_date", false)]);
    let ab = edit::group_part(&rb.s, &asg, Kind::Detail).expect("detail");
    rb.data_row(&ab, &cols, 18);
    let f = edit::group_part(&rb.s, &rs, Kind::GroupFooter).expect("footer");
    let r = edit::add_row(&mut rb.s, &f, 20).expect("row");
    edit::add_cell(
        &mut rb.s,
        &r,
        &CellSpec::new(NewCell::Text("Total".into()), 0, 360).heading(),
    )
    .expect("cell");
    let mut left = 360;
    for c in &cols[2..6] {
        edit::add_cell(
            &mut rb.s,
            &r,
            &CellSpec::new(
                NewCell::Summary {
                    field: c.field.into(),
                    path: "TASKRSRC-RSRC:".into(),
                    mode: "amSum".into(),
                },
                left,
                c.width,
            )
            .heading()
            .right(),
        )
        .expect("cell");
        left += c.width;
    }
    rb.finish()
}

/// Every starter report: file stem and definition.
pub fn all() -> Vec<(&'static str, Stream)> {
    vec![
        (
            "lookahead-2-week-by-wbs",
            lookahead(2, "2-Week Lookahead by WBS"),
        ),
        (
            "lookahead-3-week-by-wbs",
            lookahead(3, "3-Week Lookahead by WBS"),
        ),
        ("activities-by-status", by_status()),
        ("critical-activities", critical()),
        ("critical-and-near-critical", critical_and_near()),
        ("progress-this-period", progress()),
        ("predecessors-and-successors", relationships()),
        ("open-ends-and-logic-gaps", open_ends()),
        ("resource-usage-by-week", resource_usage()),
        ("baseline-variance", baseline_variance()),
        ("milestones", milestones()),
        ("wbs-summary", wbs_summary()),
        ("assignments-by-resource", assignments()),
    ]
}

#[test]
fn library_files_are_current() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("assets/openxer_reports");
    let write = std::env::var_os("OPENXER_WRITE_LIBRARY").is_some();
    if write {
        std::fs::create_dir_all(&dir).unwrap();
    }
    for (id, s) in all() {
        let path = dir.join(format!("{id}.rpt"));
        let text = s.write();
        if write {
            std::fs::write(&path, &text).unwrap();
        } else {
            let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "{} is missing ({e}); set OPENXER_WRITE_LIBRARY=1",
                    path.display()
                )
            });
            assert_eq!(
                on_disk, text,
                "{id}.rpt is out of date; set OPENXER_WRITE_LIBRARY=1"
            );
        }
    }
}

//! Shared native command layer for CLI, MCP, and future in-process chat.
//! The desktop currently calls the same engine directly.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SCHEMA_VERSION: &str = "1.0";

pub mod agent_host;
pub mod analysis;
pub mod baseline_cmd;
pub mod baseline_ops;
pub mod baseline_store;
pub mod baselines;
pub mod chat_prompt;
pub mod chat_tools;
pub mod checks;
pub mod cli;
pub mod connect;
pub mod documents;
pub mod edit;
pub mod eps;
pub mod global_change;
mod interchange;
pub mod load;
pub mod locate;
pub mod mcp;
pub mod pdf;
pub mod playbooks;
mod registers;
pub mod reports;
pub mod schedule_check;
pub mod skills;
mod storage;
pub mod usage;
pub mod user_prefs;
pub mod workspace_cmd;
pub mod workspace_db;

use openxer_engine::{
    model::{Activity, Link, LinkType},
    options::{FloatType, LagCalendar, OutOfSequence},
    scheduler,
};
use openxer_xer::{date_string, parse_relative, ProjectData};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{fmt, path::PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct AutomationError {
    pub code: String,
    pub message: String,
}
impl AutomationError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl fmt::Display for AutomationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for AutomationError {}
pub type Result<T> = std::result::Result<T, AutomationError>;

fn nullable<'de, D, T>(d: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SchedulePatch {
    pub data_date: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub must_finish: Option<Option<String>>,
    pub retained_logic: Option<bool>,
    pub progress_override: Option<bool>,
    pub lag_from_early_start: Option<bool>,
    pub use_expected_finish: Option<bool>,
    pub level_resources: Option<bool>,
    pub lag_calendar: Option<String>,
    pub float_type: Option<String>,
    pub ignore_external_relationships: Option<bool>,
    pub open_ends_critical: Option<bool>,
    pub use_must_finish: Option<bool>,
    pub multiple_float_paths: Option<bool>,
    pub float_paths_use_total: Option<bool>,
    pub max_float_paths: Option<usize>,
    #[serde(deserialize_with = "nullable")]
    pub float_paths_end_activity: Option<Option<i64>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Inspect {
        input: PathBuf,
    },
    /// Where the user's schedules are: the workspace this copy opens with
    /// its projects, and the schedule files in `folder`; read-only.
    FindSchedules {
        #[serde(default)]
        folder: Option<PathBuf>,
    },
    /// Move a workspace to a shared workspace dictionary (schema 3): a dry
    /// run reports the plan, otherwise the migrated copy goes to `output`.
    WorkspaceMigrate {
        input: PathBuf,
        #[serde(default)]
        output: Option<PathBuf>,
        #[serde(default)]
        dry_run: bool,
    },
    /// The shared workspace dictionary's rows (of one table when given),
    /// with the projects that use each.
    WorkspaceDictionary {
        input: PathBuf,
        #[serde(default)]
        table: Option<String>,
    },
    /// The projects that use one row of the shared workspace dictionary.
    WorkspaceDictionaryUsage {
        input: PathBuf,
        table: String,
        #[serde(default)]
        id: Option<i64>,
        #[serde(default)]
        name: Option<String>,
    },
    /// The workspace's Assistance preferences (whether Add opens the New
    /// Activity wizard), or one of them set in a copy written to `output`.
    WorkspacePreference {
        input: PathBuf,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        value: Option<bool>,
        #[serde(default)]
        output: Option<PathBuf>,
        #[serde(default)]
        dry_run: bool,
    },
    Import {
        input: PathBuf,
        output: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        /// Existing workspace to import into (copied to `output`).
        #[serde(default)]
        workspace: Option<PathBuf>,
        /// `new` (default), `update`, `replace` or `add` (Add Into Existing
        /// Project).
        #[serde(default)]
        action: Option<String>,
        #[serde(default)]
        target_workspace_id: Option<i64>,
        /// Per-type actions, for example `activities=update+delete,calendars=keep`.
        #[serde(default)]
        config: Option<String>,
        /// A workspace with a shared dictionary: what a dictionary row whose
        /// name the workspace has with other values does, per kind, for
        /// example `calendar=keep,resource=update` (default copy).
        #[serde(default)]
        dictionary: Option<String>,
        /// `xer` or `xml`: the format the input must be in. Without it the
        /// format is told from the file's contents.
        #[serde(default)]
        format: Option<String>,
    },
    Load {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        #[serde(default)]
        offset: usize,
        #[serde(default = "default_limit")]
        limit: usize,
        #[serde(default)]
        relationship_offset: usize,
        #[serde(default = "default_limit")]
        relationship_limit: usize,
        /// Only these activity fields (`task_code` is always kept); every
        /// field when omitted.
        #[serde(default)]
        fields: Option<Vec<String>>,
        /// Only these Activity IDs.
        #[serde(default)]
        activities: Option<Vec<String>>,
        /// Only the activities under this WBS element (code path), at any
        /// depth.
        #[serde(default)]
        wbs: Option<String>,
        /// Only the critical activities, by the project's own definition.
        #[serde(default)]
        critical: bool,
        /// Order the activities by this field, `-` first for descending.
        #[serde(default)]
        sort: Option<String>,
    },
    /// Tools > Check Schedule: the results rows, and one row's activities
    /// or relationships; read-only.
    CheckSchedule {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// A check's name: also list its Activity IDs or relationships.
        #[serde(default)]
        check: Option<String>,
        /// Most items listed (default 50, at most 500).
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        long_lag_hours: Option<f64>,
        #[serde(default)]
        large_float_hours: Option<f64>,
        #[serde(default)]
        large_duration_hours: Option<f64>,
    },
    /// The scheduling playbooks: the list, or one by name.
    Playbook {
        #[serde(default)]
        name: Option<String>,
    },
    /// The resource curves an assignment can use: the file's own and the
    /// presets, each with its 21 points.
    Curves {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
    },
    /// Time-phased units (or costs) of an activity's or a resource's
    /// assignments by day, week or month, spread as the usage views spread
    /// them (resource curves included); read-only.
    Usage {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// Only this activity's assignments (Activity ID).
        #[serde(default)]
        activity: Option<String>,
        /// Only this resource's assignments (short name).
        #[serde(default)]
        resource: Option<String>,
        /// `day`, `week` (default, from Monday) or `month`.
        #[serde(default)]
        interval: Option<String>,
        /// Project-local date range; by default every assignment window.
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        to: Option<String>,
        /// Costs instead of units.
        #[serde(default)]
        costs: bool,
    },
    Schedule {
        input: PathBuf,
        output: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        #[serde(default)]
        options: SchedulePatch,
    },
    Relationship {
        input: PathBuf,
        output: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        edit: RelationshipEdit,
    },
    Export {
        input: PathBuf,
        output: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// Version label for the XER header, for example `19.12`.
        #[serde(default)]
        version: Option<String>,
        /// Comma separated project ids (workspace ids for a workspace input) to
        /// export together; only these projects are written.
        #[serde(default)]
        projects: Option<String>,
        /// `xer` or `xml`; without it an output ending in `.xml` is XML and
        /// any other is XER.
        #[serde(default)]
        format: Option<String>,
    },
    ExportXlsx {
        input: PathBuf,
        output: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// `modern` (default) or `tables`.
        #[serde(default)]
        layout: Option<String>,
        /// `days` (default) or `hours`.
        #[serde(default)]
        unit: Option<String>,
        /// Comma separated column keys or titles for the modern layout.
        #[serde(default)]
        columns: Option<String>,
        /// A saved export template: its name in the workspace, or the path of
        /// a template `.json` file. Sets the columns, rows and options.
        #[serde(default)]
        template: Option<String>,
    },
    /// Print the project's layout to a new PDF with the desktop renderer.
    ExportPdf {
        input: PathBuf,
        output: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// Saved layout name; the workspace's current layout when omitted.
        #[serde(default)]
        layout: Option<String>,
        /// `gantt` or `network` on top.
        #[serde(default)]
        view: Option<String>,
        #[serde(default)]
        paper: Option<String>,
        /// `portrait` or `landscape`.
        #[serde(default)]
        orientation: Option<String>,
    },
    /// Apply an `openxer-changes/1` batch, all or nothing, to a new file.
    Apply {
        input: PathBuf,
        #[serde(default)]
        output: Option<PathBuf>,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// The changes document itself.
        #[serde(default)]
        changes: Option<Value>,
        /// A changes `.json` file.
        #[serde(default)]
        changes_file: Option<PathBuf>,
        #[serde(default)]
        dry_run: bool,
        /// Overrides the file's `recalculate` (false writes values exactly).
        #[serde(default)]
        recalculate: Option<bool>,
    },
    /// One edit operation (the same operations as `apply`) to a new file.
    Edit {
        input: PathBuf,
        #[serde(default)]
        output: Option<PathBuf>,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        operation: Value,
        #[serde(default)]
        dry_run: bool,
        #[serde(default)]
        recalculate: Option<bool>,
    },
    /// The workspace EPS with every project's and folder's rollups.
    EpsList {
        input: PathBuf,
    },
    /// One EPS edit (folders, filing, a new project) to a new workspace.
    Eps {
        input: PathBuf,
        #[serde(default)]
        output: Option<PathBuf>,
        #[serde(default)]
        dry_run: bool,
        operation: eps::EpsOperation,
    },
    /// The stored baselines of each workspace project (or one) with the
    /// Assign Baselines choices; read-only.
    BaselineList {
        input: PathBuf,
        #[serde(default)]
        workspace_id: Option<i64>,
    },
    /// One Maintain / Assign Baselines or Revert Project to Baseline edit
    /// of a workspace project, to a new workspace.
    Baseline {
        input: PathBuf,
        #[serde(default)]
        output: Option<PathBuf>,
        #[serde(default)]
        dry_run: bool,
        workspace_id: i64,
        operation: baseline_cmd::BaselineOperation,
    },
    /// Maintain Baselines > Update of one baseline by the Update Baseline
    /// options, to a new workspace; a dry run returns the preview.
    BaselineUpdate {
        input: PathBuf,
        #[serde(default)]
        output: Option<PathBuf>,
        #[serde(default)]
        dry_run: bool,
        workspace_id: i64,
        baseline: String,
        #[serde(default)]
        options: openxer_interchange::baseline_update::UpdateOptions,
        #[serde(default)]
        activities: Option<Vec<String>>,
    },
    /// The project's work products and documents with their links to
    /// activities and WBS elements; read-only.
    Documents {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// Only the documents linked to this activity (Activity ID).
        #[serde(default)]
        activity: Option<String>,
        /// Only the documents linked to this WBS element (path) itself.
        #[serde(default)]
        wbs: Option<String>,
    },
    /// The text of a document's linked file, paged; read-only.
    ReadDocument {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// Reference number, title or document id.
        document: String,
        /// `private` or `public`.
        #[serde(default)]
        location: Option<String>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        page: Option<usize>,
        #[serde(default)]
        pages: Option<usize>,
    },
    /// The project's expense items, issues, risks or thresholds with their
    /// fields; read-only.
    Records {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// `expenses`, `issues`, `risks` or `thresholds`.
        kind: String,
        /// Only the records of this activity (Activity ID).
        #[serde(default)]
        activity: Option<String>,
    },
    /// Earned value per activity and for the project, as the earned value
    /// columns compute it; read-only.
    EarnedValue {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// `cost` (default) or `labor_units`.
        #[serde(default)]
        basis: Option<String>,
        #[serde(default)]
        activity: Option<String>,
    },
    /// The S-curve workbook (Tools > Reports > S-Curve): time-phased
    /// budgeted, actual and remaining per period with a native chart, the
    /// breakdown matrices and an Info sheet, to a new .xlsx.
    ExportScurve {
        input: PathBuf,
        output: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// `auto` (default), `labor`, `cost` or `duration`.
        #[serde(default)]
        measure: Option<String>,
        /// `day`, `week` (default), `month` or `financial`.
        #[serde(default)]
        period: Option<String>,
        /// `total`, `resource` (default when there are resources), `wbs1`
        /// or `wbs2`.
        #[serde(default)]
        breakdown: Option<String>,
        /// Add the late dates curve.
        #[serde(default)]
        late: bool,
        /// `sunday` (default) or `monday`.
        #[serde(default)]
        week_start: Option<String>,
    },
    /// Cumulative earned hours to the data date (Tools > Reports > Earned
    /// Hours), to a new .xlsx.
    ExportEarnedHours {
        input: PathBuf,
        output: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// `resource` (default), `wbs` or `activity`.
        #[serde(default)]
        breakdown: Option<String>,
    },
    ImportXlsx {
        input: PathBuf,
        spreadsheet: PathBuf,
        #[serde(default)]
        output: Option<PathBuf>,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        #[serde(default)]
        dry_run: bool,
    },
    /// The float paths into an activity or the project finish; read-only.
    Paths {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        #[serde(default)]
        activity: Option<String>,
        #[serde(default = "default_paths")]
        count: usize,
    },
    /// The project against a baseline or an earlier file; read-only.
    Compare {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        /// An earlier XER or workspace to compare with.
        #[serde(default)]
        reference: Option<PathBuf>,
        #[serde(default)]
        reference_project_id: Option<i64>,
        #[serde(default)]
        reference_workspace_id: Option<i64>,
        /// `project` or `primary`: a stored baseline of a workspace project.
        #[serde(default)]
        baseline: Option<String>,
    },
    /// The work in the next weeks by WBS, ready or waiting; read-only.
    Lookahead {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        #[serde(default = "default_weeks")]
        weeks: u32,
        #[serde(default)]
        from: Option<String>,
    },
    /// Every milestone with date, float, baseline and driver; read-only.
    Milestones {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
    },
    /// Counts and spreads of activities, durations, calendars, names and
    /// WBS; read-only.
    ScheduleStats {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        #[serde(default)]
        update_period_days: Option<f64>,
    },
    /// Each resource's units per period against its limit; read-only.
    ResourceLoading {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        #[serde(default)]
        interval: Option<String>,
    },
    /// The schedule rolled up by WBS to a level; read-only.
    WbsSummary {
        input: PathBuf,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        workspace_id: Option<i64>,
        #[serde(default = "default_level")]
        level: usize,
    },
    /// The skill catalog: built-ins and the user's skills.
    Skills {},
    /// One skill's instructions by id.
    Skill {
        id: String,
    },
    /// Check a skill file (or every user skill when no file is given).
    SkillValidate {
        #[serde(default)]
        file: Option<PathBuf>,
    },
    /// Write a new skill file from the template into the user's folder.
    SkillNew {
        id: String,
    },
    /// The user's skill folder.
    SkillsPath {},
}
fn default_paths() -> usize {
    5
}
fn default_weeks() -> u32 {
    3
}
fn default_level() -> usize {
    2
}
fn default_limit() -> usize {
    200
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipEdit {
    pub action: EditAction,
    pub predecessor: String,
    pub successor: String,
    pub relationship_type: RelationshipType,
    #[serde(default)]
    pub lag_minutes: Option<i64>,
    #[serde(default)]
    pub new_type: Option<RelationshipType>,
}
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditAction {
    Add,
    Update,
    Remove,
}
#[derive(Debug, Clone, Copy, Deserialize)]
pub enum RelationshipType {
    FS,
    SS,
    FF,
    SF,
}
impl From<RelationshipType> for LinkType {
    fn from(t: RelationshipType) -> Self {
        match t {
            RelationshipType::FS => Self::FS,
            RelationshipType::SS => Self::SS,
            RelationshipType::FF => Self::FF,
            RelationshipType::SF => Self::SF,
        }
    }
}

pub fn parse_command(value: Value) -> Result<Command> {
    serde_json::from_value(value)
        .map_err(|e| AutomationError::new("invalid_arguments", e.to_string()))
}
pub fn response(result: Result<Value>) -> Value {
    match result {
        Ok(result) => json!({"schema_version":SCHEMA_VERSION,"ok":true,"result":result}),
        Err(error) => json!({"schema_version":SCHEMA_VERSION,"ok":false,"error":error}),
    }
}

/// Shared dispatcher. All source reads are read-only; output publication is atomic
/// and refuses an existing destination. There is no process-global project state.
pub fn dispatch(command: Command) -> Result<Value> {
    match command {
        Command::Inspect { input } => storage::inspect(&input),
        Command::FindSchedules { folder } => locate::find(folder.as_deref()),
        Command::EpsList { input } => eps::list(&input),
        Command::Records {
            input,
            project_id,
            workspace_id,
            kind,
            activity,
        } => registers::records(&input, project_id, workspace_id, &kind, activity.as_deref()),
        Command::EarnedValue {
            input,
            project_id,
            workspace_id,
            basis,
            activity,
        } => registers::earned_value(
            &input,
            project_id,
            workspace_id,
            basis.as_deref(),
            activity.as_deref(),
        ),
        Command::Eps {
            input,
            output,
            dry_run,
            operation,
        } => eps::edit(&input, output, dry_run, &operation),
        Command::BaselineList {
            input,
            workspace_id,
        } => baseline_cmd::list(&input, workspace_id),
        Command::Baseline {
            input,
            output,
            dry_run,
            workspace_id,
            operation,
        } => baseline_cmd::edit(&input, output, dry_run, workspace_id, &operation),
        Command::BaselineUpdate {
            input,
            output,
            dry_run,
            workspace_id,
            baseline,
            options,
            activities,
        } => baseline_cmd::update(
            &input,
            output,
            dry_run,
            workspace_id,
            &baseline,
            &options,
            activities.as_deref(),
        ),
        Command::WorkspaceMigrate {
            input,
            output,
            dry_run,
        } => storage::workspace_migrate(&input, output.as_deref(), dry_run),
        Command::WorkspaceDictionary { input, table } => {
            workspace_cmd::dictionary(&input, table.as_deref())
        }
        Command::WorkspaceDictionaryUsage {
            input,
            table,
            id,
            name,
        } => workspace_cmd::dictionary_usage(&input, &table, id, name.as_deref()),
        Command::WorkspacePreference {
            input,
            name,
            value,
            output,
            dry_run,
        } => user_prefs::preference(&input, name.as_deref(), value, output, dry_run),
        Command::Import {
            input,
            output,
            project_id,
            workspace,
            action,
            target_workspace_id,
            config,
            dictionary,
            format,
        } => {
            interchange::check_input_format(&input, format.as_deref())?;
            match workspace {
                Some(workspace) => interchange::import_into_workspace(
                    &input,
                    &workspace,
                    &output,
                    project_id,
                    action.as_deref().unwrap_or("new"),
                    target_workspace_id,
                    config.as_deref(),
                    dictionary.as_deref(),
                ),
                None => {
                    if action.as_deref().is_some_and(|a| a != "new")
                        || target_workspace_id.is_some()
                        || config.is_some()
                        || dictionary.is_some()
                    {
                        return Err(AutomationError::new(
                            "invalid_arguments",
                            "action, target_workspace_id, config and dictionary need workspace",
                        ));
                    }
                    storage::import(&input, &output, project_id)
                }
            }
        }
        Command::Load {
            input,
            project_id,
            workspace_id,
            offset,
            limit,
            relationship_offset,
            relationship_limit,
            fields,
            activities,
            wbs,
            critical,
            sort,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            let mut value = load::load(
                &loaded.project,
                &load::Request {
                    offset,
                    limit,
                    relationship_offset,
                    relationship_limit,
                    fields: fields.as_deref(),
                    activities: activities.as_deref(),
                    wbs: wbs.as_deref(),
                    critical,
                    sort: sort.as_deref(),
                },
            )?;
            value["workspace_id"] = json!(loaded.workspace_id);
            Ok(value)
        }
        Command::CheckSchedule {
            input,
            project_id,
            workspace_id,
            check,
            limit,
            long_lag_hours,
            large_float_hours,
            large_duration_hours,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            let ctx = baselines::context(&input, loaded.workspace_id, &loaded.project)?;
            checks::check_schedule_with(
                &loaded.project,
                Some(&ctx),
                check.as_deref(),
                limit,
                &analysis::Thresholds {
                    long_lag_hours,
                    large_float_hours,
                    large_duration_hours,
                },
            )
        }
        Command::Playbook { name } => playbooks::playbook(name.as_deref()),
        Command::Curves {
            input,
            project_id,
            workspace_id,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            Ok(curves_json(&loaded.project.document))
        }
        Command::Usage {
            input,
            project_id,
            workspace_id,
            activity,
            resource,
            interval,
            from,
            to,
            costs,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            usage::usage(
                &loaded.project,
                &usage::Request {
                    activity: activity.as_deref(),
                    resource: resource.as_deref(),
                    interval: interval.as_deref(),
                    from: from.as_deref(),
                    to: to.as_deref(),
                    costs,
                },
            )
        }
        Command::Schedule {
            input,
            output,
            project_id,
            workspace_id,
            options,
        } => {
            storage::check_destination(&output)?;
            let mut loaded = storage::load(&input, project_id, workspace_id)?;
            let report = schedule_project(&mut loaded.project, &options)?;
            storage::save(&input, &output, &loaded)?;
            Ok(
                json!({"output":output,"project":project_json(&loaded.project),"schedule":report,"warnings":loaded.project.warnings}),
            )
        }
        Command::Relationship {
            input,
            output,
            project_id,
            workspace_id,
            edit,
        } => {
            storage::check_destination(&output)?;
            let mut loaded = storage::load(&input, project_id, workspace_id)?;
            let report = edit_relationship(&mut loaded.project, &edit)?;
            storage::save(&input, &output, &loaded)?;
            Ok(
                json!({"output":output,"project":project_json(&loaded.project),"relationship":report,
                      "schedule_recalculated":false,"stored_dates":"unchanged; schedule the new output to recalculate"}),
            )
        }
        Command::Apply {
            input,
            output,
            project_id,
            workspace_id,
            changes,
            changes_file,
            dry_run,
            recalculate,
        } => {
            let document = match (changes, changes_file) {
                (Some(value), None) => value,
                (None, Some(path)) => {
                    let text = std::fs::read_to_string(&path).map_err(|e| {
                        AutomationError::new(
                            "invalid_arguments",
                            format!("Cannot read changes file {}: {e}", path.display()),
                        )
                    })?;
                    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| {
                        AutomationError::new(
                            "invalid_changes",
                            format!("Invalid changes JSON: {e}"),
                        )
                    })?
                }
                _ => {
                    return Err(AutomationError::new(
                        "invalid_arguments",
                        "apply needs exactly one of changes or changes_file",
                    ))
                }
            };
            let changes = edit::parse_changes(document)?;
            let recalculate = recalculate.or(changes.recalculate).unwrap_or(true);
            run_edits(
                &input,
                output,
                project_id,
                workspace_id,
                &changes.operations,
                dry_run,
                recalculate,
            )
        }
        Command::Edit {
            input,
            output,
            project_id,
            workspace_id,
            operation,
            dry_run,
            recalculate,
        } => {
            let op = edit::operation(operation)
                .map_err(|e| AutomationError::new("invalid_arguments", e))?;
            // A batch replaying stored rows may carry a value whose owner is
            // gone (reported, not written); one edit on its own needs one.
            if let edit::Operation::SetUdf {
                activity: None,
                resource: None,
                wbs: None,
                expense: None,
                issue: None,
                risk: None,
                document: None,
                project,
                ..
            } = &op
            {
                if *project != Some(true) {
                    return Err(AutomationError::new(
                        "invalid_arguments",
                        "set_udf needs an activity or a resource",
                    ));
                }
            }
            run_edits(
                &input,
                output,
                project_id,
                workspace_id,
                std::slice::from_ref(&op),
                dry_run,
                recalculate.unwrap_or(true),
            )
        }
        Command::Export {
            input,
            output,
            project_id,
            workspace_id,
            version,
            projects,
            format,
        } if version.is_some()
            || projects.is_some()
            || interchange::writes_xml(format.as_deref(), &output)? =>
        {
            let xml = interchange::writes_xml(format.as_deref(), &output)?;
            let projects = match projects {
                Some(list) => {
                    if project_id.is_some() || workspace_id.is_some() {
                        return Err(AutomationError::new(
                            "invalid_arguments",
                            "Use projects or project_id/workspace_id, not both",
                        ));
                    }
                    list
                }
                None => {
                    // Only a version label: the export is the plain one (other
                    // projects and unknown tables of the source are kept) with
                    // that header version.
                    return interchange::export_with_version(
                        &input,
                        &output,
                        project_id,
                        workspace_id,
                        version.as_deref().unwrap_or(""),
                        xml,
                    );
                }
            };
            interchange::export_projects(
                &input,
                &output,
                &projects,
                version.as_deref().unwrap_or(""),
                xml,
            )
        }
        Command::ExportXlsx {
            input,
            output,
            project_id,
            workspace_id,
            layout,
            unit,
            columns,
            template,
        } => interchange::export_xlsx(
            &input,
            &output,
            project_id,
            workspace_id,
            layout.as_deref(),
            unit.as_deref(),
            columns.as_deref(),
            template.as_deref(),
        ),
        Command::ExportScurve {
            input,
            output,
            project_id,
            workspace_id,
            measure,
            period,
            breakdown,
            late,
            week_start,
        } => reports::export_scurve(
            &input,
            &output,
            project_id,
            workspace_id,
            &reports::CurveRequest {
                measure: measure.as_deref(),
                period: period.as_deref(),
                breakdown: breakdown.as_deref(),
                late,
                week_start: week_start.as_deref(),
            },
        ),
        Command::ExportEarnedHours {
            input,
            output,
            project_id,
            workspace_id,
            breakdown,
        } => reports::export_earned_hours(
            &input,
            &output,
            project_id,
            workspace_id,
            breakdown.as_deref(),
        ),
        Command::ExportPdf {
            input,
            output,
            project_id,
            workspace_id,
            layout,
            view,
            paper,
            orientation,
        } => pdf::export_pdf(
            &input,
            &output,
            project_id,
            workspace_id,
            layout.as_deref(),
            view.as_deref(),
            paper.as_deref(),
            orientation.as_deref(),
        ),
        Command::Documents {
            input,
            project_id,
            workspace_id,
            activity,
            wbs,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            documents::list(&loaded.project, activity.as_deref(), wbs.as_deref())
        }
        Command::ReadDocument {
            input,
            project_id,
            workspace_id,
            document,
            location,
            offset,
            limit,
            page,
            pages,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            let paging = documents::Paging {
                offset,
                limit,
                page,
                pages,
            };
            documents::read(
                &loaded.project,
                &input,
                &document,
                location.as_deref(),
                paging,
            )
        }
        Command::ImportXlsx {
            input,
            spreadsheet,
            output,
            project_id,
            workspace_id,
            dry_run,
        } => interchange::import_xlsx(
            &input,
            &spreadsheet,
            output.as_deref(),
            project_id,
            workspace_id,
            dry_run,
        ),
        Command::Paths {
            input,
            project_id,
            workspace_id,
            activity,
            count,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            analysis::paths(&loaded.project, activity.as_deref(), count)
        }
        Command::Compare {
            input,
            project_id,
            workspace_id,
            reference,
            reference_project_id,
            reference_workspace_id,
            baseline,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            let reference = match (reference, baseline.as_deref()) {
                (Some(path), None) => {
                    let r = storage::load(&path, reference_project_id, reference_workspace_id)?;
                    analysis::Reference {
                        name: path.display().to_string(),
                        kind: "file",
                        project: r.project,
                    }
                }
                (None, Some(which @ ("project" | "primary"))) => {
                    if reference_project_id.is_some() || reference_workspace_id.is_some() {
                        return Err(AutomationError::new(
                            "invalid_arguments",
                            "reference_project_id and reference_workspace_id go with reference",
                        ));
                    }
                    let primary = which == "primary";
                    let (name, project) = baselines::stored_project(
                        &input,
                        loaded.workspace_id,
                        &loaded.project,
                        primary,
                    )?
                    .ok_or_else(|| {
                        AutomationError::new(
                            "no_baseline",
                            format!("The project has no {which} baseline assigned in this workspace; give reference, an earlier file, instead"),
                        )
                    })?;
                    analysis::Reference {
                        name,
                        kind: if primary {
                            "primary baseline"
                        } else {
                            "project baseline"
                        },
                        project,
                    }
                }
                _ => {
                    return Err(AutomationError::new(
                        "invalid_arguments",
                        "compare needs exactly one of reference (a file) or baseline (project or primary)",
                    ))
                }
            };
            analysis::compare(&loaded.project, &reference)
        }
        Command::Lookahead {
            input,
            project_id,
            workspace_id,
            weeks,
            from,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            analysis::lookahead(&loaded.project, weeks, from.as_deref())
        }
        Command::Milestones {
            input,
            project_id,
            workspace_id,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            let ctx = baselines::context(&input, loaded.workspace_id, &loaded.project)?;
            analysis::milestones(&loaded.project, &ctx)
        }
        Command::ScheduleStats {
            input,
            project_id,
            workspace_id,
            update_period_days,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            analysis::schedule_stats(&loaded.project, update_period_days)
        }
        Command::ResourceLoading {
            input,
            project_id,
            workspace_id,
            interval,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            analysis::resource_loading(&loaded.project, interval.as_deref())
        }
        Command::WbsSummary {
            input,
            project_id,
            workspace_id,
            level,
        } => {
            let loaded = storage::load(&input, project_id, workspace_id)?;
            let ctx = baselines::context(&input, loaded.workspace_id, &loaded.project)?;
            analysis::wbs_summary(&loaded.project, &ctx, level)
        }
        Command::Skills {} => {
            let dir = skills::user_dir();
            Ok(skills::catalog_json(
                &skills::catalog_in(Some(&dir)),
                Some(&dir),
            ))
        }
        Command::Skill { id } => {
            let catalog = skills::catalog();
            let entry = catalog.get(&id).ok_or_else(|| {
                AutomationError::new(
                    "not_found",
                    format!("No skill {id:?}; openxer_skills lists them"),
                )
            })?;
            let mut v = skills::detail_json(entry);
            v["rules"] = json!(skills::RULES);
            Ok(v)
        }
        Command::SkillValidate { file } => skills_validate(file),
        Command::SkillNew { id } => {
            let dir = skills::user_dir();
            let text = skills::template(&id);
            skills::parse(&text, Some(&id)).map_err(|e| {
                AutomationError::new("invalid_arguments", format!("id {id:?}: {e}"))
            })?;
            let path = skills::write_user(&dir, &id, &text, false)
                .map_err(|e| AutomationError::new("write_failed", e.to_string()))?;
            Ok(
                json!({"id": id, "path": path, "built_in_replaced": skills::BUILT_IN.iter().any(|(b, _)| *b == id)}),
            )
        }
        Command::SkillsPath {} => {
            let dir = skills::user_dir();
            Ok(json!({"folder": dir, "exists": dir.is_dir(), "data_folder": locate::data_folder()}))
        }
        Command::Export {
            input,
            output,
            project_id,
            workspace_id,
            ..
        } => {
            if storage::is_database_extension(&output) {
                return Err(AutomationError::new(
                    "invalid_arguments",
                    "export requires an .xer or .xml output",
                ));
            }
            storage::check_destination(&output)?;
            let loaded = storage::load(&input, project_id, workspace_id)?;
            storage::save(&input, &output, &loaded)?;
            Ok(
                json!({"output":output,"project":project_json(&loaded.project),"warnings":loaded.project.warnings}),
            )
        }
    }
}

/// `skills validate`: one file, or every user skill and the folder's
/// problems.
fn skills_validate(file: Option<PathBuf>) -> Result<Value> {
    let Some(path) = file else {
        let dir = skills::user_dir();
        let catalog = skills::catalog_in(Some(&dir));
        let mine: Vec<Value> = catalog
            .entries
            .iter()
            .filter(|e| e.source != skills::Source::BuiltIn)
            .map(|e| json!({"id": e.skill.id, "path": e.path, "source": e.source.key(), "valid": true, "warnings": e.warnings}))
            .collect();
        let bad: Vec<Value> = catalog
            .problems
            .iter()
            .map(|p| json!({"path": p.path, "valid": false, "error": p.error}))
            .collect();
        return Ok(
            json!({"folder": dir, "valid": bad.is_empty(), "skills": mine, "problems": bad}),
        );
    };
    let text = std::fs::read_to_string(&path).map_err(|e| {
        AutomationError::new(
            "invalid_arguments",
            format!("Cannot read {}: {e}", path.display()),
        )
    })?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    match skills::validate(&text, stem.as_deref()) {
        Ok((skill, warnings)) => Ok(json!({"path": path, "valid": true, "id": skill.id,
            "kind": skill.kind.key(), "warnings": warnings})),
        Err(e) => Err(AutomationError::new("invalid_skill", e.to_string())),
    }
}

/// Load, apply edit operations all or nothing, and write a new file unless
/// this is a dry run. A failure writes nothing.
fn run_edits(
    input: &std::path::Path,
    output: Option<PathBuf>,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
    operations: &[edit::Operation],
    dry_run: bool,
    recalculate: bool,
) -> Result<Value> {
    match (&output, dry_run) {
        (Some(_), true) => {
            return Err(AutomationError::new(
                "invalid_arguments",
                "dry_run writes nothing; omit output",
            ))
        }
        (None, false) => {
            return Err(AutomationError::new(
                "invalid_arguments",
                "output is required unless dry_run is true",
            ))
        }
        (Some(path), false) => storage::check_destination(path)?,
        (None, true) => {}
    }
    // A global change named by its saved name runs that definition.
    let operations = &global_change::resolve(input, operations)?;
    let mut loaded = storage::load(input, project_id, workspace_id)?;
    // Monitor Thresholds reads the workspace's stored baselines.
    let ev = if operations
        .iter()
        .any(|o| matches!(o, edit::Operation::MonitorThresholds { .. }))
    {
        baselines::context(input, loaded.workspace_id, &loaded.project)?
    } else {
        Default::default()
    };
    let report = edit::apply_with(&mut loaded.project, operations, recalculate, &ev)?;
    if let Some(path) = &output {
        storage::save(input, path, &loaded)?;
    }
    Ok(
        json!({"output":output,"dry_run":dry_run,"recalculate":recalculate,
              "project":project_json(&loaded.project),"report":report,
              "warnings":loaded.project.warnings}),
    )
}

/// In-process entrypoint for a future desktop chat. Failure leaves `project`
/// unchanged; scheduling itself is implemented entirely by openxer-engine.
pub fn schedule_project(project: &mut ProjectData, patch: &SchedulePatch) -> Result<Value> {
    project
        .model
        .validate()
        .map_err(|e| AutomationError::new("invalid_source", e))?;
    let mut next = project.clone();
    let before: Vec<_> = next
        .model
        .activities
        .iter()
        .map(|a| calculated_json(&next, a))
        .collect();
    let origin = next.model.origin_serial;
    if let Some(date) = &patch.data_date {
        next.model.project.data_date = Some(
            parse_relative(origin, date)
                .map_err(|e| AutomationError::new("invalid_arguments", e))?
                .ok_or_else(|| {
                    AutomationError::new("invalid_arguments", "data_date cannot be empty")
                })?,
        );
    }
    if let Some(date) = &patch.must_finish {
        next.model.project.plan_end = match date {
            Some(value) => Some(
                parse_relative(origin, value)
                    .map_err(|e| AutomationError::new("invalid_arguments", e))?
                    .ok_or_else(|| {
                        AutomationError::new(
                            "invalid_arguments",
                            "must_finish must be a date or null",
                        )
                    })?,
            ),
            None => None,
        };
    }
    if let Some(Some(id)) = patch.float_paths_end_activity {
        if !next.model.activities.iter().any(|a| a.task_id == id) {
            return Err(AutomationError::new(
                "invalid_arguments",
                format!(
                    "float_paths_end_activity {id} is not the task_id of an activity in the project"
                ),
            ));
        }
    }
    let o = &mut next.model.options;
    macro_rules! set {
        ($field:ident, $patch:ident) => {
            if let Some(value) = patch.$patch {
                o.$field = value;
            }
        };
    }
    set!(retained_logic, retained_logic);
    set!(progress_override, progress_override);
    set!(lag_from_early_start, lag_from_early_start);
    set!(use_expected_finish, use_expected_finish);
    set!(do_level, level_resources);
    set!(open_end_uses_early, open_ends_critical);
    set!(use_must_finish, use_must_finish);
    set!(multiple_float_paths, multiple_float_paths);
    set!(float_paths_use_total, float_paths_use_total);
    set!(max_float_paths, max_float_paths);
    set!(float_paths_end_activity, float_paths_end_activity);
    if patch.max_float_paths == Some(0) {
        return Err(AutomationError::new(
            "invalid_arguments",
            "max_float_paths must be a positive integer",
        ));
    }
    if o.retained_logic && o.progress_override {
        return Err(AutomationError::new(
            "invalid_arguments",
            "retained_logic and progress_override cannot both be true",
        ));
    }
    if let Some(ignore) = patch.ignore_external_relationships {
        o.out_of_sequence = if ignore {
            OutOfSequence::None
        } else {
            OutOfSequence::Both
        };
    }
    if let Some(value) = patch.lag_calendar.as_deref() {
        o.lag_calendar = match value {
            "predecessor" => LagCalendar::PredCal,
            "successor" => LagCalendar::SuccCal,
            "24_hour" => LagCalendar::Hour24,
            "project" => LagCalendar::ProjCal,
            _ => {
                return Err(AutomationError::new(
                    "invalid_arguments",
                    "lag_calendar must be predecessor, successor, 24_hour, or project",
                ))
            }
        };
    }
    if let Some(value) = patch.float_type.as_deref() {
        o.float_type = match value {
            "start" => FloatType::Start,
            "finish" => FloatType::Finish,
            "minimum" => FloatType::Min,
            _ => {
                return Err(AutomationError::new(
                    "invalid_arguments",
                    "float_type must be start, finish, or minimum",
                ))
            }
        };
    }
    if !o.do_level
        && next
            .model
            .activities
            .iter()
            .any(|a| a.task_type == openxer_engine::model::TaskType::Resource)
    {
        return Err(AutomationError::new("unsupported_schedule", "Resource-dependent activities require level_resources=true; nonleveled resource-calendar scheduling is not yet supported"));
    }
    next.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut next.model, &next.resources)
        .map_err(|e| AutomationError::new("schedule_rejected", e))?;
    // Exported assignments carry the new dates (xer::assignments).
    next.write_assignment_schedule();
    let mut changes = vec![];
    let mut activity_count = 0;
    let mut generated_count = 0;
    let mut changed_count = 0;
    for (a, old) in next.model.activities.iter().zip(before) {
        let after = calculated_json(&next, a);
        let mut generated = Map::new();
        let mut changed = Map::new();
        for (field, value) in after.as_object().expect("calculated fields are an object") {
            let prior = &old[field];
            if prior != value {
                if prior.is_null() && !value.is_null() {
                    generated.insert(field.clone(), value.clone());
                    generated_count += 1;
                } else {
                    changed.insert(field.clone(), json!({"before":prior,"after":value}));
                    changed_count += 1;
                }
            }
        }
        if !generated.is_empty() || !changed.is_empty() {
            activity_count += 1;
            if changes.len() < 200 {
                changes.push(json!({"task_id":a.task_id,"task_code":a.task_code,"generated":generated,"changed":changed}));
            }
        }
    }
    let report = json!({"changed_activity_count":activity_count,"changes_truncated":activity_count > changes.len(),"change_limit":200,"generated_field_count":generated_count,
                       "changed_field_count":changed_count,"changes":changes});
    *project = next;
    Ok(report)
}

/// Validates endpoints, duplicate identity and acyclicity before changing the
/// caller's project. It does not implicitly schedule or change stored dates.
pub fn edit_relationship(project: &mut ProjectData, edit: &RelationshipEdit) -> Result<Value> {
    project
        .model
        .validate()
        .map_err(|e| AutomationError::new("invalid_source", e))?;
    if edit
        .lag_minutes
        .is_some_and(|lag| !(-6_000_000..=6_000_000).contains(&lag))
    {
        return Err(AutomationError::new(
            "invalid_arguments",
            "lag_minutes must be between -6000000 and 6000000",
        ));
    }
    let mut next = project.clone();
    let find = |code: &str| -> Result<usize> {
        let matches: Vec<_> = next
            .model
            .activities
            .iter()
            .enumerate()
            .filter(|(_, a)| a.task_code == code)
            .map(|(i, _)| i)
            .collect();
        if matches.len() != 1 {
            return Err(AutomationError::new(
                "invalid_relationship",
                format!("Activity code {code:?} must identify exactly one activity"),
            ));
        }
        Ok(matches[0])
    };
    let pred = find(&edit.predecessor)?;
    let succ = find(&edit.successor)?;
    if pred == succ {
        return Err(AutomationError::new(
            "invalid_relationship",
            "An activity cannot depend on itself",
        ));
    }
    let kind = LinkType::from(edit.relationship_type);
    let found: Vec<_> = next
        .model
        .links
        .iter()
        .enumerate()
        .filter(|(_, l)| l.pred == pred && l.succ == succ && l.link_type == kind)
        .map(|(i, _)| i)
        .collect();
    if found.len() > 1 {
        return Err(AutomationError::new(
            "ambiguous_relationship",
            "Multiple links share the requested endpoints and type",
        ));
    }
    let before = found
        .first()
        .map(|&i| link_json(&next, &next.model.links[i]));
    match edit.action {
        EditAction::Add => {
            if !found.is_empty() || edit.new_type.is_some() {
                return Err(AutomationError::new(
                    "invalid_relationship",
                    "Add requires a new endpoint/type combination and no new_type",
                ));
            }
            next.model.links.push(Link {
                pred,
                succ,
                link_type: kind,
                lag: edit.lag_minutes.unwrap_or(0),
                driving_path: false,
            });
        }
        EditAction::Update => {
            let index = *found.first().ok_or_else(|| {
                AutomationError::new(
                    "relationship_not_found",
                    "Relationship to update does not exist",
                )
            })?;
            let kind = edit.new_type.map(LinkType::from).unwrap_or(kind);
            if next
                .model
                .links
                .iter()
                .enumerate()
                .any(|(i, l)| i != index && l.pred == pred && l.succ == succ && l.link_type == kind)
            {
                return Err(AutomationError::new(
                    "invalid_relationship",
                    "Updated relationship would duplicate an existing type",
                ));
            }
            next.model.links[index].link_type = kind;
            if let Some(lag) = edit.lag_minutes {
                next.model.links[index].lag = lag;
            }
        }
        EditAction::Remove => {
            if edit.lag_minutes.is_some() || edit.new_type.is_some() {
                return Err(AutomationError::new(
                    "invalid_arguments",
                    "Remove does not accept lag_minutes or new_type",
                ));
            }
            let index = *found.first().ok_or_else(|| {
                AutomationError::new(
                    "relationship_not_found",
                    "Relationship to remove does not exist",
                )
            })?;
            next.model.links.remove(index);
        }
    }
    next.model
        .rebuild_links()
        .map_err(|e| AutomationError::new("invalid_relationship", e))?;
    next.model
        .validate()
        .map_err(|e| AutomationError::new("invalid_relationship", e))?;
    let after = if edit.action == EditAction::Remove {
        None
    } else {
        next.model
            .links
            .iter()
            .find(|l| {
                l.pred == pred
                    && l.succ == succ
                    && l.link_type == edit.new_type.map(LinkType::from).unwrap_or(kind)
            })
            .map(|l| link_json(&next, l))
    };
    *project = next;
    Ok(json!({"before":before,"after":after}))
}

fn date(p: &ProjectData, value: Option<i64>) -> Value {
    value.map_or(Value::Null, |v| {
        json!(date_string(p.model.origin_serial, Some(v)))
    })
}
pub fn calculated_json(p: &ProjectData, a: &Activity) -> Value {
    json!({"early_start":date(p,a.early_start),"early_finish":date(p,a.early_finish),
           "late_start":date(p,a.late_start),"late_finish":date(p,a.late_finish),
           "remaining_start":date(p,a.restart),"remaining_finish":date(p,a.reend),
           "remaining_duration_minutes":a.rem,"total_float_minutes":a.total_float,"free_float_minutes":a.free_float,
           "driving_path":a.driving_path,"float_path":a.float_path,"float_path_order":a.float_path_order})
}
fn activity_json(p: &ProjectData, a: &Activity) -> Value {
    let mut value = calculated_json(p, a);
    let map = value.as_object_mut().expect("activity JSON is an object");
    map.insert("task_id".into(), json!(a.task_id));
    map.insert("task_code".into(), json!(a.task_code));
    map.insert(
        "name".into(),
        json!(p
            .details
            .get(&a.task_id)
            .map(|d| d.name.as_str())
            .unwrap_or("")),
    );
    map.insert("task_type".into(), json!(a.task_type.as_str()));
    map.insert("status".into(), json!(openxer_xer::status_label(a.status)));
    map.insert("calendar_id".into(), json!(p.model.calendars[a.cal].id));
    value
}
fn link_json(p: &ProjectData, l: &Link) -> Value {
    json!({"predecessor":p.model.activities[l.pred].task_code,"successor":p.model.activities[l.succ].task_code,
           "relationship_type":l.link_type.as_str().trim_start_matches("PR_"),"lag_minutes":l.lag})
}
/// The curve list of `curves`: file curves, then presets not in the file.
pub fn curves_json(doc: &openxer_xer::Document) -> Value {
    use openxer_xer::edit::curves::{self, Kind};
    let list: Vec<Value> = curves::list(doc)
        .into_iter()
        .map(|c| {
            let preset = curves::preset(&c.name).filter(|_| c.kind == Kind::Preset);
            json!({
                "name": c.name,
                "curv_id": c.id,
                "kind": match c.kind { Kind::File => "file", Kind::Default => "default", Kind::Preset => "preset" },
                "in_file": c.id.is_some(),
                "editable": !c.locked(),
                "assignments": c.id.map_or(0, |id| curves::uses(doc, id)),
                "shape": preset.map(|p| p.shape),
                "formula": preset.map(|p| p.formula),
                "points": c.points,
            })
        })
        .collect();
    json!({
        "points_meaning": "pct_usage_0..pct_usage_20: percent of the assignment's units spent in each 5% step of its duration; point 0 is the start, point k covers (k-1)*5% to k*5%",
        "applied_to_spreading": true,
        "applies_to": "assignments on activities with duration type DT_FixedDrtn (Fixed Duration and Units/Time) or DT_FixedDUR2 (Fixed Duration and Units); other types spread evenly",
        "spreading_rules": "budgeted and remaining units and costs follow the curve over the assignment's own dates (with its lag); actuals follow it from the actual start to the data date; point 0 counts only for actuals; segments are positions in working time. OpenXER's reading of the documented behavior, not yet measured against other software. See the usage command.",
        "curves": list,
    })
}

pub fn project_json(p: &ProjectData) -> Value {
    let o = &p.model.options;
    json!({"project_id":p.model.project.proj_id,"name":p.model.project.name,"data_date":date(p,p.model.project.data_date),
           "scheduled_finish":date(p,p.model.project.scd_end),"must_finish":date(p,p.model.project.plan_end),
           "activity_count":p.model.activities.len(),"relationship_count":p.model.links.len(),
           "units":{"durations":"integer working minutes","dates":"project-local YYYY-MM-DD HH:MM; no timezone conversion"},
           "options":{"retained_logic":o.retained_logic,"progress_override":o.progress_override,
             "level_resources":o.do_level,"lag_from_early_start":o.lag_from_early_start,"use_expected_finish":o.use_expected_finish,
             "lag_calendar":match o.lag_calendar { LagCalendar::PredCal=>"predecessor",LagCalendar::SuccCal=>"successor",LagCalendar::Hour24=>"24_hour",LagCalendar::ProjCal=>"project" },
             "float_type":match o.float_type { FloatType::Start=>"start",FloatType::Finish=>"finish",FloatType::Min=>"minimum" },
             "ignore_external_relationships":o.out_of_sequence==OutOfSequence::None,"open_ends_critical":o.open_end_uses_early,
             "use_must_finish":o.use_must_finish,"multiple_float_paths":o.multiple_float_paths,
             "float_paths_use_total":o.float_paths_use_total,"max_float_paths":o.max_float_paths,
             "float_paths_end_activity":o.float_paths_end_activity}})
}

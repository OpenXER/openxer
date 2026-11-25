//! Arena model: activity and link indices stay stable during scheduling.
use crate::{
    calendar::{CalendarAdapter, WorkCalendar, DAY},
    options::ScheduleOptions,
};
use std::collections::HashSet;
/// CON-9 (UNVERIFIED against the reference; probes 1401 to 1404 pending): the
/// one switch for whether a Mandatory Start or Mandatory Finish pins work
/// already in progress the way it pins work not started (CON-7, CON-8).
///
/// `false`: CON-5 decides, as for every other constraint. The passes schedule
/// started work by its logic from the data date over its remaining duration;
/// a Mandatory Finish still caps the late finish (late finish constraints
/// bind anything not completed), a Mandatory Start binds nothing.
///
/// `true`: started work is placed by the mandatory pre-pass exactly like
/// work not started, early and late dates on the constraint date.
pub const MANDATORY_PINS_STARTED_WORK: bool = false;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskType {
    #[default]
    Task,
    Mile,
    FinMile,
    Loe,
    Wbs,
    Hamwk,
    Resource,
}
impl TaskType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Task => "TT_Task",
            Self::Mile => "TT_Mile",
            Self::FinMile => "TT_FinMile",
            Self::Loe => "TT_LOE",
            Self::Wbs => "TT_WBS",
            Self::Hamwk => "TT_Hamwk",
            Self::Resource => "TT_Rsrc",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum StatusType {
    #[default]
    NotStarted = 0,
    InProgress = 1,
    Completed = 2,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkType {
    #[default]
    FS,
    SS,
    FF,
    SF,
    None,
}
impl LinkType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FS => "PR_FS",
            Self::SS => "PR_SS",
            Self::FF => "PR_FF",
            Self::SF => "PR_SF",
            Self::None => "PR_None",
        }
    }
}
/// Constraint types as stored. `early_gate`, `late_gate` and `ef_replaces`
/// say which types move early dates, cap late dates and take an Expected
/// Finish date (CON-1 to CON-3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum CstrType {
    #[default]
    None = 0,
    StartOn = 1,
    StartOnBefore = 2,
    StartOnAfter = 3,
    Alap = 4,
    FinishOn = 5,
    FinishOnBefore = 6,
    FinishOnAfter = 7,
    MandatoryStart = 8,
    MandatoryFinish = 9,
}
impl CstrType {
    pub fn early_gate(self) -> bool {
        matches!(
            self,
            Self::StartOn
                | Self::StartOnAfter
                | Self::FinishOn
                | Self::FinishOnAfter
                | Self::MandatoryStart
                | Self::MandatoryFinish
        )
    }
    pub fn late_gate(self) -> bool {
        matches!(
            self,
            Self::StartOn
                | Self::StartOnBefore
                | Self::FinishOn
                | Self::FinishOnBefore
                | Self::MandatoryStart
                | Self::MandatoryFinish
        )
    }
    pub fn ef_replaces(self) -> bool {
        matches!(
            self,
            Self::FinishOn | Self::FinishOnBefore | Self::FinishOnAfter | Self::MandatoryFinish
        )
    }
    pub fn from_code(n: i64) -> Option<Self> {
        Some(match n {
            0 => Self::None,
            1 => Self::StartOn,
            2 => Self::StartOnBefore,
            3 => Self::StartOnAfter,
            4 => Self::Alap,
            5 => Self::FinishOn,
            6 => Self::FinishOnBefore,
            7 => Self::FinishOnAfter,
            8 => Self::MandatoryStart,
            9 => Self::MandatoryFinish,
            _ => return None,
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Assignment {
    pub rsrc_id: i64,
    pub rollup: bool,
    pub rem: i64,
    pub remain_lag: i64,
    pub relag: i64,
    // Whether the source flag was present and nonempty (kept for fixtures).
    // Only `rollup` selects the date-driving assignments (RES-1).
    pub rollup_present: bool,
    pub rem_late_start: Option<i64>,
    pub rem_late_finish: Option<i64>,
    /// The resource's calendar for an assignment with units on a Resource
    /// Dependent activity, which then works on it; `None` otherwise.
    pub cal: Option<usize>,
    /// The remaining duration in hours exactly as units / units per hour
    /// give it (`f64::to_bits`), when it came from units. Only work counted on
    /// round-the-clock time uses it (RES-4); everything else uses `rem`.
    pub rem_hours_bits: Option<u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub task_id: i64,
    pub task_code: String,
    pub task_type: TaskType,
    pub status: StatusType,
    pub act_start: Option<i64>,
    pub act_finish: Option<i64>,
    pub resume: Option<i64>,
    pub expect_end: Option<i64>,
    pub cstr: CstrType,
    pub cstr_date: Option<i64>,
    pub cstr2: CstrType,
    pub cstr_date2: Option<i64>,
    pub rem: i64,
    pub cal: usize,
    pub ext_early_start: Option<i64>,
    pub ext_late_end: Option<i64>,
    pub restart: Option<i64>,
    pub reend: Option<i64>,
    pub rem_restart: Option<i64>,
    pub rem_reend: Option<i64>,
    pub early_start: Option<i64>,
    pub early_finish: Option<i64>,
    pub late_start: Option<i64>,
    pub late_finish: Option<i64>,
    pub rem_late_start: Option<i64>,
    pub rem_late_finish: Option<i64>,
    pub total_float: Option<i64>,
    pub free_float: Option<i64>,
    pub driving_path: bool,
    pub preds: Vec<usize>,
    pub succs: Vec<usize>,
    pub assignments: Vec<Assignment>,
    pub target_start: Option<i64>,
    pub target_finish: Option<i64>,
    pub float_path: i64,
    pub float_path_order: i64,
}
impl Activity {
    /// True when the activity's status is Completed.
    pub fn is_completed(&self) -> bool {
        self.status == StatusType::Completed
    }
    /// True when Expected Finish dates apply: the activity has one, the option
    /// is on, leveling is not running and the constraint is not As Late As
    /// Possible (EXP-1).
    pub fn uses_expected_finish(&self, opt: &ScheduleOptions, leveling: bool) -> bool {
        self.expect_end.is_some()
            && opt.use_expected_finish
            && !leveling
            && self.cstr != CstrType::Alap
    }
    /// True when the primary constraint is Mandatory Start or Mandatory
    /// Finish (CON-8).
    pub fn uses_mandatory_constraint(&self) -> bool {
        matches!(
            self.cstr,
            CstrType::MandatoryStart | CstrType::MandatoryFinish
        )
    }
    /// CON-7, CON-9: true when the mandatory pre-pass places this activity and
    /// the forward and backward passes leave it alone. Work in progress is
    /// pinned only when `MANDATORY_PINS_STARTED_WORK` says so; otherwise the
    /// passes schedule it by its logic like any started activity (CON-5).
    /// Completed work keeps the pre-pass handling.
    pub fn mandatory_pinned(&self) -> bool {
        self.uses_mandatory_constraint()
            && (self.status != StatusType::InProgress || MANDATORY_PINS_STARTED_WORK)
    }
    /// CON-5, CON-9: whether a constraint (the primary one when `primary`)
    /// that binds only work not yet started (any early constraint, a late
    /// start constraint) binds this activity: always when it is not started,
    /// and for started work only a pinning primary Mandatory constraint.
    pub fn unstarted_constraint_binds(&self, primary: bool) -> bool {
        self.status == StatusType::NotStarted
            || (primary && self.status == StatusType::InProgress && self.mandatory_pinned())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub pred: usize,
    pub succ: usize,
    pub link_type: LinkType,
    pub lag: i64,
    pub driving_path: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectSettings {
    pub proj_id: i64,
    pub name: String,
    pub data_date: Option<i64>,
    pub scd_end: Option<i64>,
    pub plan_start: Option<i64>,
    pub plan_end: Option<i64>,
    pub last_schedule: Option<i64>,
}
impl ProjectSettings {
    /// The limit the backward pass starts from: the planned (must-finish) date
    /// when the option is on and one is set, else the scheduled finish (FLT-1).
    pub fn late_limit(&self, use_must_finish: bool) -> Option<i64> {
        if use_must_finish {
            self.plan_end.or(self.scd_end)
        } else {
            self.scd_end
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub origin_serial: i64,
    pub project_calendar: usize,
    pub calendars: Vec<CalendarAdapter>,
    pub activities: Vec<Activity>,
    pub links: Vec<Link>,
    pub project: ProjectSettings,
    pub options: ScheduleOptions,
}
/// Validate stored shifts without sorting, merging, or altering them.
/// Raw wire shifts may end before they start (overnight) or at the same clock
/// minute (24 hours); normalized shifts must already have a positive duration.
pub fn validate_shifts(shifts: &[(i64, i64)], normalized: bool) -> Result<(), String> {
    let mut total = 0_i64;
    for (index, &(start, end)) in shifts.iter().enumerate() {
        if !(0..DAY).contains(&start) || end < 0 {
            return Err(format!(
                "Shift {index} has an invalid clock minute ({start}, {end})"
            ));
        }
        let finish = if !normalized && end <= start {
            end.checked_add(DAY)
                .ok_or("Shift normalization overflows")?
        } else {
            end
        };
        let duration = finish
            .checked_sub(start)
            .ok_or("Shift duration overflows")?;
        if !(1..=DAY).contains(&duration) {
            return Err(format!(
                "Shift {index} must have a positive duration of at most one day"
            ));
        }
        total = total
            .checked_add(duration)
            .ok_or("Daily calendar work overflows")?;
    }
    Ok(())
}

/// Structural calendar validation; all-closed calendars remain useful for
/// bounded fixture operations, so recurring work is checked by their callers.
pub fn validate_calendar(calendar: &WorkCalendar) -> Result<(), String> {
    for (day, shifts) in calendar.template.iter().enumerate() {
        validate_shifts(shifts, true)
            .map_err(|error| format!("Calendar {} weekday {day}: {error}", calendar.name))?;
    }
    for (serial, shifts) in &calendar.exc {
        validate_shifts(shifts, true)
            .map_err(|error| format!("Calendar {} exception {serial}: {error}", calendar.name))?;
    }
    Ok(())
}

pub fn has_recurring_work(calendar: &WorkCalendar) -> bool {
    calendar.continuous || calendar.template.iter().any(|shifts| !shifts.is_empty())
}

/// Check day arithmetic, including the preceding day scanned by prev_work_time
/// and an overnight period's end. Reject overflow, not an arbitrary horizon.
pub fn validate_minute(minute: i64, origin_serial: i64) -> Result<(), String> {
    let day = minute.div_euclid(DAY);
    let midnight = day.checked_mul(DAY).ok_or("Date's midnight overflows")?;
    midnight
        .checked_sub(DAY)
        .ok_or("Date's preceding midnight overflows")?;
    midnight
        .checked_add(2 * DAY)
        .ok_or("Date's overnight period end overflows")?;
    minute
        .checked_sub(1)
        .ok_or("Date's preceding minute overflows")?;
    minute
        .checked_add(DAY - 1)
        .ok_or("Date's ceiling day overflows")?;
    origin_serial
        .checked_add(day)
        .ok_or("Calendar date serial overflows")?;
    Ok(())
}

/// Earliest and latest calendar day a schedule date may fall on, as day
/// serials counted from 30 Dec 1899 (the XER importer's count): 1 Jan 1000
/// and 31 Dec 9999, the years a four-digit XER date can carry. Dates outside
/// cannot come from a file and only make the day arithmetic run for ages.
pub const DATE_SERIAL_RANGE: std::ops::RangeInclusive<i64> = -328_716..=2_958_465;

/// A schedule date must name a day in [`DATE_SERIAL_RANGE`].
pub fn validate_date_horizon(minute: i64, origin_serial: i64) -> Result<(), String> {
    let serial = origin_serial
        .checked_add(minute.div_euclid(DAY))
        .ok_or("Calendar date serial overflows")?;
    if DATE_SERIAL_RANGE.contains(&serial) {
        Ok(())
    } else {
        Err("Date is outside the years 1000 to 9999".into())
    }
}

impl Activity {
    /// Every date field, for moving a model to another origin.
    pub fn dates_mut(&mut self) -> [&mut Option<i64>; 20] {
        [
            &mut self.act_start,
            &mut self.act_finish,
            &mut self.resume,
            &mut self.expect_end,
            &mut self.cstr_date,
            &mut self.cstr_date2,
            &mut self.ext_early_start,
            &mut self.ext_late_end,
            &mut self.restart,
            &mut self.reend,
            &mut self.rem_restart,
            &mut self.rem_reend,
            &mut self.early_start,
            &mut self.early_finish,
            &mut self.late_start,
            &mut self.late_finish,
            &mut self.rem_late_start,
            &mut self.rem_late_finish,
            &mut self.target_start,
            &mut self.target_finish,
        ]
    }
    /// Shift every date by `minutes` (a change of origin).
    pub fn shift_dates(&mut self, minutes: i64) {
        for date in self.dates_mut().into_iter().flatten() {
            *date += minutes;
        }
        for a in &mut self.assignments {
            for date in [&mut a.rem_late_start, &mut a.rem_late_finish]
                .into_iter()
                .flatten()
            {
                *date += minutes;
            }
        }
    }
}
impl ProjectSettings {
    /// Shift every date by `minutes` (a change of origin).
    pub fn shift_dates(&mut self, minutes: i64) {
        for date in [
            &mut self.data_date,
            &mut self.scd_end,
            &mut self.plan_start,
            &mut self.plan_end,
            &mut self.last_schedule,
        ]
        .into_iter()
        .flatten()
        {
            *date += minutes;
        }
    }
}
impl Model {
    /// The same model counted from origin day `origin`: every date names the
    /// same absolute moment and every calendar the same work time.
    pub fn rebase(&mut self, origin: i64) {
        let minutes = (self.origin_serial - origin) * DAY;
        for a in &mut self.activities {
            a.shift_dates(minutes);
        }
        self.project.shift_dates(minutes);
        for c in &mut self.calendars {
            c.cal = c.cal.rebased(origin);
        }
        self.origin_serial = origin;
    }
    /// Rebuild derived adjacency only after all endpoints have been checked.
    /// An invalid link leaves existing adjacency unchanged.
    pub fn rebuild_links(&mut self) -> Result<(), String> {
        for (i, link) in self.links.iter().enumerate() {
            if link.pred >= self.activities.len() || link.succ >= self.activities.len() {
                return Err(format!("Link {i} references a missing activity"));
            }
        }
        for activity in &mut self.activities {
            activity.preds.clear();
            activity.succs.clear();
        }
        for (i, link) in self.links.iter().enumerate() {
            self.activities[link.pred].succs.push(i);
            self.activities[link.succ].preds.push(i);
        }
        Ok(())
    }

    /// Validate application inputs before the scheduling arithmetic, which
    /// walks days without a fixed horizon. Adjacency is derived; only `links` is authoritative.
    pub fn validate(&self) -> Result<(), String> {
        let mut successors = vec![Vec::new(); self.activities.len()];
        let mut predecessors = vec![Vec::new(); self.activities.len()];
        let mut count = vec![0; self.activities.len()];
        for (i, link) in self.links.iter().enumerate() {
            if link.pred >= count.len() || link.succ >= count.len() {
                return Err(format!("Link {i} references a missing activity"));
            }
            link.lag
                .checked_neg()
                .ok_or("Relationship lag cannot be negated")?;
            // PASS-5: level-of-effort and summary activities take no part in
            // the passes, so a loop that closes only through one of them is
            // not a loop for the passes (the reference schedules it).
            if [link.pred, link.succ]
                .iter()
                .any(|&i| matches!(self.activities[i].task_type, TaskType::Loe | TaskType::Wbs))
            {
                continue;
            }
            successors[link.pred].push(link.succ);
            predecessors[link.succ].push(link.pred);
            count[link.succ] += 1;
        }
        // An empty project is a valid no-op, even before choosing a calendar.
        if self.activities.is_empty() {
            return Ok(());
        }
        if self.project.data_date.is_none() {
            return Err("A data date is required".into());
        }
        if self.project_calendar >= self.calendars.len() {
            return Err("Project calendar is missing".into());
        }
        let mut referenced = HashSet::from([self.project_calendar]);
        let mut activity_ids = HashSet::new();
        for activity in &self.activities {
            if !activity_ids.insert(activity.task_id) {
                return Err(format!("Duplicate activity id {}", activity.task_id));
            }
            if activity.cal >= self.calendars.len() {
                return Err(format!(
                    "Activity {} calendar is missing",
                    activity.task_code
                ));
            }
            referenced.insert(activity.cal);
            activity
                .rem
                .checked_neg()
                .ok_or("Activity duration cannot be negated")?;
            // A negative remaining duration would put the early finish before
            // the early start.
            if activity.rem < 0 {
                return Err(format!(
                    "Activity {} has a negative remaining duration ({} h); it must be zero or more",
                    activity.task_code,
                    activity.rem as f64 / 60.0
                ));
            }
            for assignment in &activity.assignments {
                assignment
                    .rem
                    .checked_add(assignment.remain_lag)
                    .and_then(i64::checked_neg)
                    .ok_or("Assignment start duration overflows")?;
                assignment
                    .rem
                    .checked_add(assignment.relag)
                    .and_then(i64::checked_neg)
                    .ok_or("Assignment finish duration overflows")?;
                if let Some(c) = assignment.cal {
                    if c >= self.calendars.len() {
                        return Err(format!(
                            "Activity {} has an assignment calendar index out of range",
                            activity.task_code
                        ));
                    }
                    referenced.insert(c);
                }
            }
            for minute in [
                activity.act_start,
                activity.act_finish,
                activity.resume,
                activity.expect_end,
                activity.cstr_date,
                activity.cstr_date2,
                activity.ext_early_start,
                activity.ext_late_end,
                activity.restart,
                activity.reend,
                activity.rem_restart,
                activity.rem_reend,
                activity.early_start,
                activity.early_finish,
                activity.late_start,
                activity.late_finish,
                activity.rem_late_start,
                activity.rem_late_finish,
                activity.target_start,
                activity.target_finish,
            ]
            .into_iter()
            .flatten()
            {
                validate_minute(minute, self.origin_serial)
                    .and_then(|()| validate_date_horizon(minute, self.origin_serial))
                    .map_err(|error| format!("Activity {}: {error}", activity.task_code))?;
            }
        }
        for index in referenced {
            let calendar = &self.calendars[index];
            validate_calendar(&calendar.cal)?;
            if calendar.cal.origin_serial != self.origin_serial {
                return Err(format!(
                    "Calendar {} has a different date origin",
                    calendar.id
                ));
            }
            if !has_recurring_work(&calendar.cal) {
                return Err(format!("Calendar {} has no recurring work", calendar.id));
            }
        }
        for minute in [
            self.project.data_date,
            self.project.scd_end,
            self.project.plan_start,
            self.project.plan_end,
            self.project.last_schedule,
        ]
        .into_iter()
        .flatten()
        {
            validate_minute(minute, self.origin_serial)?;
            validate_date_horizon(minute, self.origin_serial)?;
        }
        if self.options.multiple_float_paths {
            if self.options.max_float_paths == 0 {
                return Err("max_float_paths must be positive".into());
            }
            if let Some(id) = self.options.float_paths_end_activity {
                if !activity_ids.contains(&id) {
                    return Err("The float-path ending activity is missing".into());
                }
            }
        }
        let mut ready: Vec<_> = count
            .iter()
            .enumerate()
            .filter_map(|(i, &n)| (n == 0).then_some(i))
            .collect();
        let mut seen = 0;
        while let Some(index) = ready.pop() {
            seen += 1;
            for &successor in &successors[index] {
                count[successor] -= 1;
                if count[successor] == 0 {
                    ready.push(successor);
                }
            }
        }
        if seen != count.len() {
            let cycle = self.find_cycle(&count, &predecessors);
            return Err(format!(
                "The activity network contains a circular relationship: {cycle}"
            ));
        }
        Ok(())
    }

    /// One loop among the activities the ordering could not reach, named by
    /// Activity ID in relationship order, e.g. `A1 -> A2 -> A1`. Every such
    /// activity still has an unreached predecessor, so walking predecessors
    /// must come back to an activity already visited.
    fn find_cycle(&self, count: &[usize], predecessors: &[Vec<usize>]) -> String {
        const SHOWN: usize = 25;
        let Some(start) = count.iter().position(|&n| n > 0) else {
            return String::new();
        };
        let mut step = vec![None; count.len()];
        let mut path = Vec::new();
        let mut at = start;
        while step[at].is_none() {
            step[at] = Some(path.len());
            path.push(at);
            match predecessors[at].iter().find(|&&p| count[p] > 0) {
                Some(&p) => at = p,
                None => break,
            }
        }
        let first = step[at].unwrap_or(0);
        // Walked backwards, so reverse to read in relationship order.
        let mut ring: Vec<usize> = path[first..].iter().rev().copied().collect();
        if ring.len() > SHOWN {
            ring.truncate(SHOWN);
            let names: Vec<&str> = ring
                .iter()
                .map(|&i| self.activities[i].task_code.as_str())
                .collect();
            return format!("{} -> ...", names.join(" -> "));
        }
        ring.push(ring[0]);
        ring.iter()
            .map(|&i| self.activities[i].task_code.as_str())
            .collect::<Vec<_>>()
            .join(" -> ")
    }
}

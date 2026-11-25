//! Strict line-based fixtures shared by CLI and integration tests; no external crates.
use crate::{calendar::*, model::*, options::*, scheduler::Scheduler};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

/// The folder of comparison fixtures. They hold client data, so they live
/// outside the repository: `OPENXER_FIXTURES` names the folder. When it is not
/// set the old in-repo `engine/fixtures` is used.
pub fn fixtures_root() -> PathBuf {
    std::env::var_os("OPENXER_FIXTURES")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures"))
}

/// `rel` inside [`fixtures_root`], or `None` after printing why a test that
/// needs it is skipped. An empty `rel` is the root itself.
pub fn fixture_path(rel: &str) -> Option<PathBuf> {
    let root = fixtures_root();
    let path = if rel.is_empty() { root } else { root.join(rel) };
    if path.exists() {
        return Some(path);
    }
    let set = std::env::var_os("OPENXER_FIXTURES").is_some_and(|v| !v.is_empty());
    skip(&if set {
        format!(
            "fixture {} not found under OPENXER_FIXTURES",
            path.display()
        )
    } else {
        format!(
            "OPENXER_FIXTURES is not set, so fixture {} is not available (set it to the fixtures folder)",
            if rel.is_empty() { "folder" } else { rel }
        )
    });
    None
}

/// The lines of a private list file `rel` inside [`fixtures_root`], with
/// blank lines and `#` comments dropped and each line split on whitespace,
/// or `None` after printing why a test that needs it is skipped. Lists that
/// must not be published (corpus file names that identify a job, words the
/// sources must not contain) live there instead of in the repository.
pub fn fixture_list(rel: &str) -> Option<Vec<Vec<String>>> {
    let path = fixture_path(rel)?;
    match fs::read_to_string(&path) {
        Ok(text) => Some(
            text.lines()
                .map(|l| l.split('#').next().unwrap_or(""))
                .map(|l| l.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
                .filter(|f| !f.is_empty())
                .collect(),
        ),
        Err(e) => {
            skip(&format!("fixture {} is not readable: {e}", path.display()));
            None
        }
    }
}

/// The private corpus file names listed under `key` in the fixture
/// `corpus-names.txt` (lines `key file [note ...]`), as (file, note) pairs,
/// or `None` after printing why a test that needs them is skipped. Corpus
/// files whose names identify a client or job are found this way, so the
/// names never appear in the repository.
pub fn corpus_names(key: &str) -> Option<Vec<(String, String)>> {
    Some(
        fixture_list("corpus-names.txt")?
            .into_iter()
            .filter(|f| f.len() >= 2 && f[0] == key)
            .map(|f| (f[1].clone(), f[2..].join(" ")))
            .collect(),
    )
}

/// Print `SKIPPED <test>: <reason>` for a test that returns early. It goes
/// straight to standard error, which the test harness does not capture, so
/// the line shows in a normal `cargo test` run, not only with `--nocapture`.
pub fn skip(reason: &str) {
    use std::io::Write;
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("test");
    let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {reason}");
}
#[derive(Clone, Debug)]
pub struct Record {
    pub line: usize,
    pub fields: Vec<String>,
}
impl Record {
    fn at(&self, i: usize) -> Result<&str, String> {
        self.fields
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| format!("line {}: missing field {i}", self.line))
    }
    fn int(&self, i: usize) -> Result<i64, String> {
        self.at(i)?
            .parse()
            .map_err(|_| format!("line {}: invalid integer", self.line))
    }
    fn boolean(&self, i: usize) -> Result<bool, String> {
        match self.int(i)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(format!("line {}: boolean must be 0 or 1", self.line)),
        }
    }
    fn date(&self, i: usize) -> Result<Option<i64>, String> {
        if self.at(i)? == "-" {
            Ok(None)
        } else {
            self.int(i).map(Some)
        }
    }
    fn len(&self, n: usize) -> Result<(), String> {
        if self.fields.len() == n {
            Ok(())
        } else {
            Err(format!(
                "line {}: expected {n} fields, got {}",
                self.line,
                self.fields.len()
            ))
        }
    }
}
pub fn records(text: &str) -> Vec<Record> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let fields: Vec<_> = l
                .split('#')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            (!fields.is_empty()).then_some(Record {
                line: i + 1,
                fields,
            })
        })
        .collect()
}
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"._-".contains(&b) {
            out.push(b as char);
        } else {
            write!(&mut out, "%{b:02X}").unwrap();
        }
    }
    out
}
pub fn decode(s: &str) -> Result<String, String> {
    let mut out = vec![];
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = s.get(i + 1..i + 3).ok_or("Incomplete percent escape")?;
            out.push(u8::from_str_radix(h, 16).map_err(|_| "Invalid percent escape")?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "Invalid UTF-8 token".into())
}
fn shifts(s: &str) -> Result<Shifts, String> {
    if s == "-" {
        return Ok(vec![]);
    }
    let shifts: Shifts = s
        .split(',')
        .map(|x| {
            let (a, b) = x.split_once(':').ok_or("Invalid shift")?;
            Ok((
                a.parse().map_err(|_| "Invalid shift start")?,
                b.parse().map_err(|_| "Invalid shift end")?,
            ))
        })
        .collect::<Result<_, String>>()?;
    validate_shifts(&shifts, false)?;
    Ok(shifts)
}
fn calendars(rows: &[Record]) -> Result<Vec<CalendarAdapter>, String> {
    let mut out: Vec<CalendarAdapter> = vec![];
    let mut ids = HashMap::new();
    let mut seen = vec![];
    for r in rows {
        match r.at(0)? {
            "cal" => {
                r.len(4)?;
                let name = r.at(1)?.to_owned();
                if ids.contains_key(&name) {
                    return Err(format!("Duplicate calendar {name}"));
                }
                let flag = r.int(2)?;
                if !matches!(flag, 0 | 1) {
                    return Err("Invalid calendar boolean".into());
                }
                ids.insert(name.clone(), out.len());
                out.push(CalendarAdapter::new(WorkCalendar::new(
                    name,
                    flag == 1,
                    r.int(3)?,
                    std::array::from_fn(|_| vec![]),
                    BTreeMap::new(),
                )));
                seen.push([false; 7]);
            }
            "wk" | "exc" => {
                r.len(4)?;
                let id = *ids.get(r.at(1)?).ok_or("Undefined calendar")?;
                let day = r.int(2)?;
                let sh = shifts(r.at(3)?)?;
                if r.at(0)? == "wk" {
                    if !(0..7).contains(&day) {
                        return Err("Week index out of range".into());
                    }
                    if seen[id][day as usize] {
                        return Err("Duplicate weekday".into());
                    }
                    out[id].cal.template[day as usize] = sh;
                    seen[id][day as usize] = true;
                } else if out[id].cal.exc.insert(day, sh).is_some() {
                    return Err("Duplicate exception".into());
                }
            }
            _ => {}
        }
    }
    for (i, c) in out.iter_mut().enumerate() {
        if seen[i].contains(&false) {
            return Err(format!("Calendar {} needs 7 weekdays", c.id));
        }
        c.cal = WorkCalendar::new(
            c.cal.name.clone(),
            c.cal.continuous,
            c.cal.origin_serial,
            c.cal.template.clone(),
            c.cal.exc.clone(),
        );
        validate_calendar(&c.cal)?;
    }
    Ok(out)
}
#[derive(Clone, Debug)]
pub struct Case {
    pub line: usize,
    pub operation: String,
    pub calendar: usize,
    pub args: Vec<i64>,
    pub expected: i64,
}
#[derive(Debug, Default)]
pub struct Report {
    pub exact: usize,
    pub total: usize,
    pub mismatches: Vec<String>,
}
impl Report {
    pub fn compare(&mut self, label: String, expected: Option<i64>, actual: Option<i64>) {
        self.total += 1;
        if expected == actual {
            self.exact += 1;
        } else if self.mismatches.len() < 10 {
            self.mismatches
                .push(format!("{label}: expected={expected:?}, actual={actual:?}"));
        }
    }
    pub fn merge(&mut self, other: Self) {
        self.exact += other.exact;
        self.total += other.total;
        for m in other.mismatches {
            if self.mismatches.len() < 10 {
                self.mismatches.push(m);
            }
        }
    }
}
pub fn parse(text: &str) -> Result<(Vec<CalendarAdapter>, Vec<Case>), String> {
    let rows = records(text);
    let cals = calendars(&rows)?;
    let mut cases = vec![];
    for r in rows {
        match r.at(0)? {
            "cal" | "wk" | "exc" => continue,
            "op" => {}
            x => return Err(format!("Unknown fixture record {x}")),
        }
        let op = r.at(1)?;
        let arity = operation_arity(op)?;
        r.len(5 + arity)?;
        if r.at(3 + arity)? != "=" {
            return Err("Expected '='".into());
        }
        let id = cals
            .iter()
            .position(|c| c.id == r.at(2).unwrap_or(""))
            .ok_or("Unknown calendar")?;
        cases.push(Case {
            line: r.line,
            operation: op.into(),
            calendar: id,
            args: (0..arity).map(|i| r.int(3 + i)).collect::<Result<_, _>>()?,
            expected: r.int(4 + arity)?,
        });
    }
    if cases.is_empty() {
        return Err("No calendar cases".into());
    }
    Ok((cals, cases))
}
fn operation_arity(operation: &str) -> Result<usize, String> {
    match operation {
        "is_work" | "work_at" | "snap_start" | "snap_finish" | "start_at" | "finish_at" => Ok(1),
        "add" | "sub" | "add_start" | "between" | "duration" | "finish_date" | "start_date"
        | "next_work_time" | "next_non_work_time" | "prev_work_time" | "prev_non_work_time" => {
            Ok(2)
        }
        _ => Err(format!("Unknown operation {operation}")),
    }
}

// Exact prefix arithmetic used only to validate work-coordinate overflow before
// invoking the calendar arithmetic. Negative days have no prefix.
fn checked_work_position(calendar: &WorkCalendar, minute: i64) -> Result<i64, String> {
    validate_minute(minute, calendar.origin_serial)?;
    let day = minute.div_euclid(DAY);
    let full = day.max(0);
    // Weekly totals must use the template, not exceptions on the origin week.
    let template: Vec<i128> = if calendar.continuous {
        vec![i128::from(DAY); 7]
    } else {
        calendar
            .template
            .iter()
            .map(|shifts| shifts.iter().map(|&(s, e)| i128::from(e - s)).sum())
            .collect()
    };
    let mut total = i128::from(full / 7) * template.iter().sum::<i128>()
        + template.iter().take((full % 7) as usize).sum::<i128>();
    if !calendar.continuous {
        for (&serial, shifts) in &calendar.exc {
            let d = i128::from(serial) - i128::from(calendar.origin_serial);
            if d >= 0 && d < i128::from(full) {
                total += shifts.iter().map(|&(s, e)| i128::from(e - s)).sum::<i128>()
                    - template[(d % 7) as usize];
            }
        }
    }
    let tod = minute.rem_euclid(DAY);
    total += calendar
        .shifts_on(day)
        .iter()
        .map(|&(s, e)| i128::from((e.min(tod) - s).max(0)))
        .sum::<i128>();
    i64::try_from(total).map_err(|_| "Cumulative calendar work overflows".into())
}

fn check_inverse(calendar: &WorkCalendar, work: i64, start: bool) -> Result<(), String> {
    if !has_recurring_work(calendar) && (if start { work >= 0 } else { work > 0 }) {
        return Err("Calendar has no recurring work for this unbounded date operation".into());
    }
    Ok(())
}

fn check_finish_date(calendar: &WorkCalendar, start: i64, minutes: i64) -> Result<(), String> {
    if calendar.continuous {
        start.checked_add(minutes).ok_or("Finish date overflows")?;
        return Ok(());
    }
    if !has_recurring_work(calendar) {
        return Err("Calendar has no recurring work for finish_date".into());
    }
    let mut day = start.div_euclid(DAY);
    let mut remaining = minutes;
    let mut current = start;
    let mut bound = (day + 1).checked_mul(DAY).ok_or("Finish date overflows")?;
    let mut available = calendar.duration(start, bound);
    while remaining > available {
        remaining -= available;
        day = day.checked_add(1).ok_or("Finish date overflows")?;
        current = day.checked_mul(DAY).ok_or("Finish date overflows")?;
        bound = current.checked_add(DAY).ok_or("Finish date overflows")?;
        validate_minute(current, calendar.origin_serial)?;
        available = calendar.total_hours_on_date(day);
    }
    // The day totals and the per-day search read the same working shifts
    // (overnight parts moved to the next day, overlaps merged), so this
    // cannot trigger; it stays as a guard against a search that would not
    // end.
    if remaining > calendar.duration(current, bound) {
        return Err("finish_date exceeds the terminal day's midnight-clipped working time".into());
    }
    calendar
        .next_work_time(current, bound)
        .checked_add(remaining)
        .ok_or("Finish date overflows")?;
    Ok(())
}

fn validate_case(calendar: &WorkCalendar, case: &Case) -> Result<(), String> {
    let arity = operation_arity(&case.operation)?;
    if case.args.len() != arity {
        return Err(format!(
            "line {}: {} expects {arity} arguments",
            case.line, case.operation
        ));
    }
    let a = case.args[0];
    let b = case.args.get(1).copied().unwrap_or(0);
    if matches!(case.operation.as_str(), "start_at" | "finish_at") {
        return check_inverse(calendar, a, case.operation == "start_at");
    }
    validate_minute(a, calendar.origin_serial)?;
    match case.operation.as_str() {
        "between" | "duration" | "next_work_time" | "next_non_work_time" | "prev_work_time"
        | "prev_non_work_time" => {
            validate_minute(b, calendar.origin_serial)?;
            if case.operation == "duration" {
                a.max(b).checked_sub(a.min(b)).ok_or("Duration overflows")?;
            }
        }
        "finish_date" => check_finish_date(calendar, a, b)?,
        "start_date" if calendar.continuous => {
            a.checked_sub(b).ok_or("Start date overflows")?;
        }
        _ => {}
    }
    match case.operation.as_str() {
        "is_work" | "duration" | "finish_date" | "next_work_time" | "next_non_work_time"
        | "prev_work_time" | "prev_non_work_time" => {}
        "between" => {
            checked_work_position(calendar, b)?
                .checked_sub(checked_work_position(calendar, a)?)
                .ok_or("Work difference overflows")?;
        }
        "start_date" if calendar.continuous => {}
        operation => {
            let w = checked_work_position(calendar, a)?;
            match operation {
                "work_at" => {}
                "snap_start" => check_inverse(calendar, w, true)?,
                "snap_finish" => check_inverse(calendar, w, false)?,
                "add" => {
                    b.checked_neg().ok_or("Duration cannot be negated")?;
                    let target = w.checked_add(b).ok_or("Work coordinate overflows")?;
                    check_inverse(calendar, if b < 0 { target.max(0) } else { target }, b <= 0)?;
                }
                "sub" => {
                    b.checked_neg().ok_or("Duration cannot be negated")?;
                    let target = w.checked_sub(b).ok_or("Work coordinate overflows")?;
                    check_inverse(calendar, if b > 0 { target.max(0) } else { target }, b > 0)?;
                }
                "add_start" => check_inverse(
                    calendar,
                    w.checked_add(b).ok_or("Work coordinate overflows")?,
                    true,
                )?,
                "start_date" => check_inverse(
                    calendar,
                    w.checked_sub(b).ok_or("Work coordinate overflows")?.max(0),
                    true,
                )?,
                _ => return Err(format!("Unknown operation {operation}")),
            }
        }
    }
    Ok(())
}

pub fn run(cals: &[CalendarAdapter], cases: &[Case]) -> Result<Report, String> {
    if cases.is_empty() {
        return Err("No calendar cases".into());
    }
    for calendar in cals {
        validate_calendar(&calendar.cal)?;
    }
    let mut out = Report::default();
    for case in cases {
        let c = cals
            .get(case.calendar)
            .ok_or_else(|| format!("line {}: unknown calendar index", case.line))?;
        validate_case(&c.cal, case).map_err(|e| format!("line {}: {e}", case.line))?;
        let a = case.args[0];
        let b = case.args.get(1).copied().unwrap_or(0);
        let actual = match case.operation.as_str() {
            "is_work" => i64::from(c.is_work(a)),
            "work_at" => c.work_at(a),
            "snap_start" => c.snap_start(a),
            "snap_finish" => c.snap_finish(a),
            "add" => c.add(a, b),
            "sub" => c.sub(a, b),
            "add_start" => c.add_start(a, b),
            "between" => c.between(a, b),
            "duration" => c.cal.duration(a, b),
            "finish_date" => c.cal.get_finish_date(a, b),
            "start_date" => c.cal.get_start_date(a, b),
            "start_at" => c.start_at(a),
            "finish_at" => c.finish_at(a),
            "next_work_time" => c.cal.next_work_time(a, b),
            "next_non_work_time" => c.cal.next_non_work_time(a, b),
            "prev_work_time" => c.cal.prev_work_time(a, b),
            "prev_non_work_time" => c.cal.prev_non_work_time(a, b),
            x => return Err(format!("Unknown operation {x}")),
        };
        out.compare(
            format!(
                "line {} {} {} {:?}",
                case.line, c.id, case.operation, case.args
            ),
            Some(case.expected),
            Some(actual),
        );
    }
    Ok(out)
}
pub fn run_file(path: impl AsRef<Path>) -> Result<Report, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (c, ops) = parse(&text)?;
    run(&c, &ops)
}
#[derive(Clone, Debug)]
pub struct Expectation {
    pub key: String,
    pub fields: Vec<(String, Option<i64>)>,
}
#[derive(Clone, Debug)]
pub struct ModelDump {
    pub name: String,
    pub model: Model,
    pub forward: Vec<Expectation>,
    pub expected: Vec<Expectation>,
    pub ties: Vec<Expectation>,
}
fn expect(r: &Record) -> Result<Expectation, String> {
    if r.fields.len() < 3 {
        return Err(format!("line {}: expectation has no values", r.line));
    }
    let mut keys = HashSet::new();
    let mut fields = vec![];
    for f in r.fields.iter().skip(2) {
        let (k, v) = f.split_once('=').ok_or("Invalid expectation")?;
        if !keys.insert(k) {
            return Err("Duplicate expectation field".into());
        }
        fields.push((
            k.into(),
            if v == "-" {
                None
            } else {
                Some(v.parse().map_err(|_| "Invalid expected value")?)
            },
        ));
    }
    Ok(Expectation {
        key: decode(r.at(1)?)?,
        fields,
    })
}
pub fn parse_link(s: &str) -> Result<LinkType, String> {
    Ok(match s {
        "PR_FS" => LinkType::FS,
        "PR_SS" => LinkType::SS,
        "PR_FF" => LinkType::FF,
        "PR_SF" => LinkType::SF,
        "PR_None" => LinkType::None,
        _ => return Err(format!("Unknown link type {s}")),
    })
}
pub fn parse_type(s: &str) -> Result<TaskType, String> {
    Ok(match s {
        "TT_Task" => TaskType::Task,
        "TT_Mile" => TaskType::Mile,
        "TT_FinMile" => TaskType::FinMile,
        "TT_LOE" => TaskType::Loe,
        "TT_WBS" => TaskType::Wbs,
        "TT_Hamwk" => TaskType::Hamwk,
        "TT_Rsrc" => TaskType::Resource,
        _ => return Err(format!("Unknown activity type {s}")),
    })
}
fn option(opt: &mut ScheduleOptions, k: &str, v: &str) -> Result<(), String> {
    let b = || match v {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!("Invalid boolean {k}={v}")),
    };
    match k {
        "retained_logic" => opt.retained_logic = b()?,
        "progress_override" => opt.progress_override = b()?,
        "lag_from_early_start" => opt.lag_from_early_start = b()?,
        "use_expected_finish" => opt.use_expected_finish = b()?,
        "do_level" => opt.do_level = b()?,
        "use_remaining_dates" => opt.use_remaining_dates = b()?,
        "open_end_uses_early" => opt.open_end_uses_early = b()?,
        "use_must_finish" => opt.use_must_finish = b()?,
        "finish_on_valid_time" => opt.finish_on_valid_time = b()?,
        "lag_calendar" => {
            opt.lag_calendar = match v {
                "PredCal" => LagCalendar::PredCal,
                "SuccCal" => LagCalendar::SuccCal,
                "24Hour" => LagCalendar::Hour24,
                "ProjCal" => LagCalendar::ProjCal,
                _ => return Err("Invalid lag calendar".into()),
            }
        }
        "float_type" => {
            opt.float_type = match v {
                "FT_SS" => FloatType::Start,
                "FT_FF" => FloatType::Finish,
                "FT_Min" => FloatType::Min,
                _ => return Err("Invalid float type".into()),
            }
        }
        "out_of_sequence" => {
            opt.out_of_sequence = match v {
                "SD_None" => OutOfSequence::None,
                "SD_RetainedLogic" => OutOfSequence::RetainedLogic,
                "SD_ProgressOverride" => OutOfSequence::ProgressOverride,
                "SD_Both" => OutOfSequence::Both,
                _ => return Err("Invalid external dependency mode".into()),
            }
        }
        "multiple_float_paths" => opt.multiple_float_paths = b()?,
        "float_paths_use_total" => opt.float_paths_use_total = b()?,
        "max_float_paths" => opt.max_float_paths = v.parse().map_err(|_| "Invalid path count")?,
        "float_paths_end_activity" => {
            opt.float_paths_end_activity = if v == "-" {
                None
            } else {
                Some(v.parse().map_err(|_| "Invalid path endpoint")?)
            }
        }
        _ => return Err(format!("Unknown option {k}")),
    }
    Ok(())
}
pub fn parse_model(text: &str) -> Result<ModelDump, String> {
    let rows = records(text);
    if rows.is_empty() {
        return Err("Empty model dump".into());
    }
    let cals = calendars(&rows)?;
    let ids: HashMap<_, _> = cals
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();
    let mut a_ids = HashMap::new();
    let mut dump = ModelDump {
        name: String::new(),
        model: Model {
            origin_serial: 0,
            project_calendar: 0,
            calendars: cals,
            activities: vec![],
            links: vec![],
            project: ProjectSettings::default(),
            options: ScheduleOptions::default(),
        },
        forward: vec![],
        expected: vec![],
        ties: vec![],
    };
    let mut required = [false; 5];
    for r in &rows {
        if let Some(index) = ["model", "origin", "projcal", "proj", "opt"]
            .iter()
            .position(|&kind| kind == r.at(0).unwrap_or(""))
        {
            if required[index] {
                return Err(format!("line {}: duplicate {} record", r.line, r.at(0)?));
            }
        }
        let m = &mut dump.model;
        match r.at(0)? {
            "cal" | "wk" | "exc" => {}
            "model" => {
                r.len(3)?;
                dump.name = decode(r.at(1)?)?;
                m.project.name = dump.name.clone();
                m.project.proj_id = r.int(2)?;
                required[0] = true;
            }
            "origin" => {
                r.len(2)?;
                m.origin_serial = r.int(1)?;
                required[1] = true;
            }
            "projcal" => {
                r.len(2)?;
                m.project_calendar = *ids.get(r.at(1)?).ok_or("Unknown project calendar")?;
                required[2] = true;
            }
            "proj" => {
                r.len(6)?;
                m.project.data_date = r.date(1)?;
                m.project.scd_end = r.date(2)?;
                m.project.plan_start = r.date(3)?;
                m.project.plan_end = r.date(4)?;
                m.project.last_schedule = r.date(5)?;
                required[3] = true;
            }
            "opt" => {
                let mut keys = HashSet::new();
                for f in r.fields.iter().skip(1) {
                    let (k, v) = f.split_once('=').ok_or("Invalid option")?;
                    if !keys.insert(k) {
                        return Err("Duplicate schedule option".into());
                    }
                    option(&mut m.options, k, v)?;
                }
                required[4] = true;
            }
            "act" => {
                r.len(19)?;
                let id = r.int(1)?;
                if a_ids.insert(id, m.activities.len()).is_some() {
                    return Err("Duplicate activity id".into());
                }
                let cstr = |i| {
                    CstrType::from_code(r.int(i)?).ok_or_else(|| "Invalid constraint".to_string())
                };
                m.activities.push(Activity {
                    task_id: id,
                    task_code: decode(r.at(2)?)?,
                    task_type: parse_type(r.at(3)?)?,
                    status: match r.int(4)? {
                        0 => StatusType::NotStarted,
                        1 => StatusType::InProgress,
                        2 => StatusType::Completed,
                        _ => return Err("Invalid activity status".into()),
                    },
                    act_start: r.date(5)?,
                    act_finish: r.date(6)?,
                    resume: r.date(7)?,
                    expect_end: r.date(8)?,
                    cstr: cstr(9)?,
                    cstr_date: r.date(10)?,
                    cstr2: cstr(11)?,
                    cstr_date2: r.date(12)?,
                    rem: r.int(13)?,
                    cal: *ids.get(r.at(14)?).ok_or("Unknown activity calendar")?,
                    ext_early_start: r.date(15)?,
                    ext_late_end: r.date(16)?,
                    rem_restart: r.date(17)?,
                    rem_reend: r.date(18)?,
                    ..Activity::default()
                });
            }
            "forward" => dump.forward.push(expect(r)?),
            "expect" => dump.expected.push(expect(r)?),
            "expect_tie" => dump.ties.push(expect(r)?),
            "link" | "assign" => {}
            x => return Err(format!("line {}: unknown record {x}", r.line)),
        }
    }
    if required.contains(&false) {
        return Err("Missing model header, origin, project calendar, project or options".into());
    }
    for r in &rows {
        let m = &mut dump.model;
        match r.at(0)? {
            "link" => {
                r.len(5)?;
                m.links.push(Link {
                    pred: *a_ids.get(&r.int(1)?).ok_or("Unknown predecessor")?,
                    succ: *a_ids.get(&r.int(2)?).ok_or("Unknown successor")?,
                    link_type: parse_link(r.at(3)?)?,
                    lag: r.int(4)?,
                    driving_path: false,
                });
            }
            "assign" => {
                if r.fields.len() != 5 && r.fields.len() != 8 && r.fields.len() != 9 {
                    return Err("Expected 5, 8 or 9 assignment fields".into());
                }
                let i = *a_ids.get(&r.int(1)?).ok_or("Unknown assigned activity")?;
                // A ninth field names a Resource Dependent assignment's calendar.
                let cal = if r.fields.len() == 9 {
                    let id = r.at(8)?;
                    Some(
                        m.calendars
                            .iter()
                            .position(|c| c.id == id)
                            .ok_or("Unknown assignment calendar")?,
                    )
                } else {
                    None
                };
                m.activities[i].assignments.push(Assignment {
                    rsrc_id: r.int(2)?,
                    rollup: r.boolean(3)?,
                    rem: r.int(4)?,
                    remain_lag: if r.fields.len() >= 8 { r.int(5)? } else { 0 },
                    relag: if r.fields.len() >= 8 { r.int(6)? } else { 0 },
                    rollup_present: if r.fields.len() >= 8 {
                        r.boolean(7)?
                    } else {
                        true
                    },
                    cal,
                    ..Assignment::default()
                });
            }
            _ => {}
        }
    }
    dump.model.validate()?;
    dump.model.rebuild_links()?;
    Ok(dump)
}
fn date(v: Option<i64>) -> String {
    v.map_or_else(|| "-".into(), |x| x.to_string())
}
fn shift_text(sh: &[(i64, i64)]) -> String {
    if sh.is_empty() {
        "-".into()
    } else {
        sh.iter()
            .map(|(s, e)| format!("{s}:{e}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}
pub fn write_model(m: &Model, name: &str) -> String {
    let mut out = format!(
        "model {} {}\norigin {}\nprojcal {}\nproj {} {} {} {} {}\n",
        encode(name),
        m.project.proj_id,
        m.origin_serial,
        m.calendars[m.project_calendar].id,
        date(m.project.data_date),
        date(m.project.scd_end),
        date(m.project.plan_start),
        date(m.project.plan_end),
        date(m.project.last_schedule)
    );
    let o = &m.options;
    writeln!(out,"opt retained_logic={} progress_override={} lag_from_early_start={} use_expected_finish={} do_level={} use_remaining_dates={} lag_calendar={} float_type={} out_of_sequence={} open_end_uses_early={} use_must_finish={} multiple_float_paths={} float_paths_use_total={} max_float_paths={} float_paths_end_activity={} finish_on_valid_time={}",u8::from(o.retained_logic),u8::from(o.progress_override),u8::from(o.lag_from_early_start),u8::from(o.use_expected_finish),u8::from(o.do_level),u8::from(o.use_remaining_dates),o.lag_calendar.as_str(),o.float_type.as_str(),o.out_of_sequence.as_str(),u8::from(o.open_end_uses_early),u8::from(o.use_must_finish),u8::from(o.multiple_float_paths),u8::from(o.float_paths_use_total),o.max_float_paths,date(o.float_paths_end_activity),u8::from(o.finish_on_valid_time)).unwrap();
    for c in &m.calendars {
        writeln!(
            out,
            "cal {} {} {}",
            c.id,
            u8::from(c.cal.continuous),
            c.cal.origin_serial
        )
        .unwrap();
        for (i, s) in c.cal.template.iter().enumerate() {
            writeln!(out, "wk {} {i} {}", c.id, shift_text(s)).unwrap();
        }
        for (d, s) in &c.cal.exc {
            writeln!(out, "exc {} {d} {}", c.id, shift_text(s)).unwrap();
        }
    }
    for t in &m.activities {
        writeln!(
            out,
            "act {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
            t.task_id,
            encode(&t.task_code),
            t.task_type.as_str(),
            t.status as u8,
            date(t.act_start),
            date(t.act_finish),
            date(t.resume),
            date(t.expect_end),
            t.cstr as u8,
            date(t.cstr_date),
            t.cstr2 as u8,
            date(t.cstr_date2),
            t.rem,
            m.calendars[t.cal].id,
            date(t.ext_early_start),
            date(t.ext_late_end),
            date(t.rem_restart),
            date(t.rem_reend)
        )
        .unwrap();
    }
    for l in &m.links {
        writeln!(
            out,
            "link {} {} {} {}",
            m.activities[l.pred].task_id,
            m.activities[l.succ].task_id,
            l.link_type.as_str(),
            l.lag
        )
        .unwrap();
    }
    for t in &m.activities {
        for a in &t.assignments {
            write!(
                out,
                "assign {} {} {} {} {} {} {}",
                t.task_id,
                a.rsrc_id,
                u8::from(a.rollup),
                a.rem,
                a.remain_lag,
                a.relag,
                u8::from(a.rollup_present)
            )
            .unwrap();
            if let Some(c) = a.cal {
                write!(out, " {}", m.calendars[c].id).unwrap();
            }
            writeln!(out).unwrap();
        }
    }
    out
}
impl ModelDump {
    pub fn serialize(&self) -> String {
        let mut out = write_model(&self.model, &self.name);
        for (kind, rows) in [
            ("forward", &self.forward),
            ("expect", &self.expected),
            ("expect_tie", &self.ties),
        ] {
            for r in rows {
                let key = if kind == "expect_tie" {
                    r.key.split('|').map(encode).collect::<Vec<_>>().join("|")
                } else {
                    encode(&r.key)
                };
                write!(out, "{kind} {key}").unwrap();
                for (k, v) in &r.fields {
                    write!(out, " {k}={}", date(*v)).unwrap();
                }
                out.push('\n');
            }
        }
        out
    }
    pub fn run(&self, forward_only: bool) -> Result<Report, String> {
        self.model.validate()?;
        let mut model = self.model.clone();
        model.rebuild_links()?;
        let mut s = Scheduler::new(&mut model);
        if forward_only {
            s.load_task_lists();
            s.schedule_forward(&s.tasks_to_schedule.clone());
        } else {
            s.schedule_now()?;
        }
        let mut report = Report::default();
        let rows = if forward_only {
            &self.forward
        } else {
            &self.expected
        };
        if rows.is_empty() {
            return Err("No expected activity rows".into());
        }
        for row in rows {
            let a = s
                .model
                .activities
                .iter()
                .find(|a| a.task_code == row.key)
                .ok_or("Unknown expected activity")?;
            for (k, expected) in &row.fields {
                let actual = match k.as_str() {
                    "es" => a.early_start,
                    "ef" => a.early_finish,
                    "ls" => a.late_start,
                    "lf" => a.late_finish,
                    "tf" => a.total_float,
                    "ff" => a.free_float,
                    "drive" => Some(i64::from(a.driving_path)),
                    "rs" => a.restart,
                    "rf" => a.reend,
                    "rls" => a.rem_late_start,
                    "rlf" => a.rem_late_finish,
                    "rem" => Some(a.rem),
                    "path" => Some(a.float_path),
                    "order" => Some(a.float_path_order),
                    _ => return Err(format!("Unknown output field {k}")),
                };
                report.compare(format!("{} {} {k}", self.name, row.key), *expected, actual);
            }
        }
        if !forward_only {
            let mut used = vec![false; s.model.links.len()];
            for row in &self.ties {
                let (p, t) = row.key.split_once('|').ok_or("Invalid tie key")?;
                let i = s
                    .model
                    .links
                    .iter()
                    .enumerate()
                    .position(|(i, l)| {
                        !used[i]
                            && s.model.activities[l.pred].task_code == p
                            && s.model.activities[l.succ].task_code == t
                    })
                    .ok_or("Unknown expected tie")?;
                used[i] = true;
                for (k, expected) in &row.fields {
                    let actual = match k.as_str() {
                        "ff" => s.relationship_free_float(i),
                        "drive" => s.relationship_driving(i).map(i64::from),
                        _ => return Err("Unknown tie output".into()),
                    };
                    report.compare(format!("{} {} {k}", self.name, row.key), *expected, actual);
                }
            }
        }
        Ok(report)
    }
}
pub fn run_models(dir: impl AsRef<Path>, mode: &str) -> Result<Report, String> {
    let mut paths = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    paths.retain(|p| p.extension().is_some_and(|s| s == "txt"));
    paths.sort();
    if paths.is_empty() {
        return Err("No model dumps".into());
    }
    let mut total = Report::default();
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let dump = parse_model(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let r = if mode == "roundtrip" {
            let mut r = Report {
                total: 1,
                ..Report::default()
            };
            if dump.serialize() == text {
                r.exact = 1;
            } else {
                r.mismatches
                    .push(format!("{}: serialization differs", path.display()));
            }
            r
        } else {
            dump.run(mode == "forward")?
        };
        println!(
            "{}: {}/{} exact",
            path.file_name().unwrap_or_default().to_string_lossy(),
            r.exact,
            r.total
        );
        total.merge(r);
    }
    Ok(total)
}
#[cfg(test)]
mod tests {
    /// A test that needs a missing fixture is told to skip (`None`) and a
    /// `SKIPPED <test>: <reason>` line is printed.
    #[test]
    fn a_missing_fixture_is_skipped_with_a_reason() {
        assert!(super::fixture_path("no-such-folder/no-such-fixture.txt").is_none());
    }
}

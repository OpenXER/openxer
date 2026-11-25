//! Activity steps (`TASKPROC`) and step templates (`PROCGROUP` with their
//! `PROCITEM` rows), shared by the Steps tab of Activity Details, the
//! Activity Step Templates dialog and the command line.
//!
//! Steps are kept in `seq_num` order (renumbered 1, 2, ... after every add,
//! delete or move). A percent of 100 sets a step's complete flag; setting
//! the flag sets 100 and clearing it 0. When the project's steps drive
//! activity percent complete (`activity::steps_drive_percent`), a step
//! change sets the activity's physical percent complete to the weighted
//! step percent.
use super::records::{self, float, format_number, num};
use crate::{Document, ProjectData};

type R<T> = Result<T, String>;

pub const GROUP: &str = "PROCGROUP";
pub const ITEM: &str = "PROCITEM";

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub steps: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateStep {
    pub id: i64,
    pub name: String,
    pub weight: f64,
    pub description: String,
}

fn sorted(doc: &Document, table: &str, id: &str, keep: impl Fn(&[String]) -> bool) -> Vec<usize> {
    let Some(t) = doc.table(table) else {
        return vec![];
    };
    let mut rows: Vec<usize> = (0..t.rows.len()).filter(|&i| keep(&t.rows[i])).collect();
    rows.sort_by_key(|&i| {
        (
            num(t.value(&t.rows[i], "seq_num")),
            num(t.value(&t.rows[i], id)),
        )
    });
    rows
}

fn row_of(doc: &Document, table: &str, key: &str, id: i64) -> Option<usize> {
    let t = doc.table(table)?;
    t.rows.iter().position(|r| num(t.value(r, key)) == id)
}

/// The templates in their order, with their step counts.
pub fn templates(doc: &Document) -> Vec<Template> {
    let Some(t) = doc.table(GROUP) else {
        return vec![];
    };
    sorted(doc, GROUP, "proc_group_id", |_| true)
        .into_iter()
        .map(|i| {
            let id = num(t.value(&t.rows[i], "proc_group_id"));
            Template {
                id,
                name: t.value(&t.rows[i], "proc_group_name").to_string(),
                steps: template_steps(doc, id).len(),
            }
        })
        .collect()
}

/// The steps of template `group` in order.
pub fn template_steps(doc: &Document, group: i64) -> Vec<TemplateStep> {
    let Some(t) = doc.table(ITEM) else {
        return vec![];
    };
    let field = t.field("proc_group_id");
    sorted(doc, ITEM, "proc_item_id", |r| {
        field
            .and_then(|f| r.get(f))
            .is_some_and(|v| num(v) == group)
    })
    .into_iter()
    .map(|i| {
        let r = &t.rows[i];
        TemplateStep {
            id: num(t.value(r, "proc_item_id")),
            name: t.value(r, "proc_name").to_string(),
            weight: float(t.value(r, "proc_wt")),
            description: t.value(r, "proc_descr").to_string(),
        }
    })
    .collect()
}

fn name_taken(doc: &Document, except: Option<i64>, name: &str) -> bool {
    templates(doc)
        .iter()
        .any(|t| Some(t.id) != except && t.name.trim().eq_ignore_ascii_case(name.trim()))
}

fn check_name(doc: &Document, except: Option<i64>, name: &str) -> R<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err("The template name cannot be blank.".into());
    }
    if name.chars().count() > 60 {
        return Err("The name can be at most 60 characters long.".into());
    }
    if name_taken(doc, except, name) {
        return Err(format!("A template named {name} already exists."));
    }
    Ok(())
}

/// A free template name: `base`, then `base 2`, `base 3`...
pub fn free_name(doc: &Document, base: &str) -> String {
    std::iter::once(base.to_string())
        .chain((2..).map(|n| format!("{base} {n}")))
        .find(|n| !name_taken(doc, None, n))
        .expect("a free name")
}

/// Add a template at the end of the list; returns its id.
pub fn add_template(doc: &mut Document, name: Option<&str>) -> R<i64> {
    let name = match name {
        Some(n) => n.trim().to_string(),
        None => free_name(doc, "New Template"),
    };
    check_name(doc, None, &name)?;
    let id = records::next_id(doc, GROUP, "proc_group_id");
    let seq = templates(doc).len() as i64 + 1;
    records::add_row(
        doc,
        GROUP,
        &[
            ("proc_group_id", id.to_string()),
            ("seq_num", seq.to_string()),
            ("proc_group_name", records::clean(&name)),
        ],
    );
    Ok(id)
}

pub fn rename_template(doc: &mut Document, id: i64, name: &str) -> R<()> {
    check_name(doc, Some(id), name)?;
    let row = row_of(doc, GROUP, "proc_group_id", id).ok_or("The template no longer exists.")?;
    records::set(
        doc,
        GROUP,
        row,
        "proc_group_name",
        records::clean(name.trim()),
    );
    Ok(())
}

/// Delete a template with its steps.
pub fn delete_template(doc: &mut Document, id: i64) {
    for step in template_steps(doc, id) {
        delete_template_step(doc, step.id);
    }
    if let Some(row) = row_of(doc, GROUP, "proc_group_id", id) {
        records::remove_row(doc, GROUP, row);
    }
}

/// Add a step at the end of template `group`; returns its id.
pub fn add_template_step(
    doc: &mut Document,
    group: i64,
    name: &str,
    weight: f64,
    description: &str,
) -> i64 {
    let id = records::next_id(doc, ITEM, "proc_item_id");
    let seq = template_steps(doc, group).len() as i64 + 1;
    records::add_row(
        doc,
        ITEM,
        &[
            ("proc_item_id", id.to_string()),
            ("proc_group_id", group.to_string()),
            ("seq_num", seq.to_string()),
            ("proc_name", records::clean(name)),
            ("proc_wt", records::format_number(weight)),
            ("proc_descr", description.to_string()),
        ],
    );
    id
}

/// Which template step column an edit changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepField {
    Name,
    Weight,
}

pub fn set_template_step(doc: &mut Document, id: i64, field: StepField, text: &str) -> R<()> {
    let row = row_of(doc, ITEM, "proc_item_id", id).ok_or("The step no longer exists.")?;
    match field {
        StepField::Name => {
            let name = records::clean(text.trim());
            if name.is_empty() {
                return Err("The step name cannot be blank.".into());
            }
            records::set(doc, ITEM, row, "proc_name", name);
        }
        StepField::Weight => {
            let w: f64 = text
                .trim()
                .parse()
                .ok()
                .filter(|w: &f64| w.is_finite() && *w >= 0.)
                .ok_or("The step weight is a number of 0 or more.")?;
            records::set(doc, ITEM, row, "proc_wt", records::format_number(w));
        }
    }
    Ok(())
}

pub fn delete_template_step(doc: &mut Document, id: i64) {
    if let Some(row) = row_of(doc, ITEM, "proc_item_id", id) {
        records::remove_row(doc, ITEM, row);
    }
}

/// Add template `group`'s steps after the activity's own steps (not
/// complete). Returns the number added.
pub fn apply_template(doc: &mut Document, group: i64, task_id: i64, pid: i64) -> usize {
    let steps = template_steps(doc, group);
    let mut seq = records::rows_for(doc, "TASKPROC", task_id, pid)
        .iter()
        .map(|&r| num(&records::get(doc, "TASKPROC", r, "seq_num")))
        .max()
        .unwrap_or(0);
    for s in &steps {
        seq += 1;
        let id = records::next_id(doc, "TASKPROC", "proc_id");
        records::add_row(
            doc,
            "TASKPROC",
            &[
                ("proc_id", id.to_string()),
                ("task_id", task_id.to_string()),
                ("proj_id", pid.to_string()),
                ("seq_num", seq.to_string()),
                ("proc_name", s.name.clone()),
                ("complete_flag", "N".into()),
                ("proc_wt", records::format_number(s.weight)),
                ("complete_pct", "0".into()),
                ("proc_descr", s.description.clone()),
            ],
        );
    }
    steps.len()
}

/// Add template `group`'s steps (not complete) to the activity right after
/// its step `after` (a `proc_id`), or after its last step when `after` is
/// `None` or no longer one of its steps, then number the activity's steps
/// 1, 2, ... again. Returns the number added.
pub fn apply_template_after(
    doc: &mut Document,
    group: i64,
    task_id: i64,
    pid: i64,
    after: Option<i64>,
) -> usize {
    let steps = template_steps(doc, group);
    let mut order = sorted_steps(doc, task_id, pid);
    let at = after
        .and_then(|id| order.iter().position(|&r| step_id(doc, r) == id))
        .map_or(order.len(), |k| k + 1);
    for (k, s) in steps.iter().enumerate() {
        let copy = StepCopy {
            name: s.name.clone(),
            description: s.description.clone(),
            weight: format_number(s.weight),
            percent: "0".into(),
            complete: "N".into(),
        };
        let row = new_step_row(doc, task_id, pid, &copy);
        order.insert(at + k, row);
    }
    renumber_steps(doc, &order);
    steps.len()
}

/// The `proc_id` of step row `r`.
pub fn step_id(doc: &Document, r: usize) -> i64 {
    num(&records::get(doc, "TASKPROC", r, "proc_id"))
}

/// A new template holding the activity's steps; returns its id.
pub fn template_from_activity(doc: &mut Document, name: &str, task_id: i64, pid: i64) -> R<i64> {
    let mut rows = records::rows_for(doc, "TASKPROC", task_id, pid);
    if rows.is_empty() {
        return Err("The activity has no steps to make a template from.".into());
    }
    rows.sort_by_key(|&r| {
        (
            num(&records::get(doc, "TASKPROC", r, "seq_num")),
            num(&records::get(doc, "TASKPROC", r, "proc_id")),
        )
    });
    let steps: Vec<(String, f64, String)> = rows
        .iter()
        .map(|&r| {
            (
                records::get(doc, "TASKPROC", r, "proc_name"),
                float(&records::get(doc, "TASKPROC", r, "proc_wt")),
                records::get(doc, "TASKPROC", r, "proc_descr"),
            )
        })
        .collect();
    let id = add_template(doc, Some(&free_name(doc, name)))?;
    for (name, weight, description) in steps {
        add_template_step(doc, id, &name, weight, &description);
    }
    Ok(id)
}

// ------------------------------------------------------- activity steps

/// The activity's step rows in step order.
pub fn sorted_steps(doc: &Document, task_id: i64, pid: i64) -> Vec<usize> {
    let mut rows = records::rows_for(doc, "TASKPROC", task_id, pid);
    rows.sort_by_key(|&r| {
        (
            num(&records::get(doc, "TASKPROC", r, "seq_num")),
            num(&records::get(doc, "TASKPROC", r, "proc_id")),
        )
    });
    rows
}

/// Weighted step completion, or None when there are no steps.
pub fn weighted_step_percent(doc: &Document, task_id: i64, pid: i64) -> Option<f64> {
    let rows = sorted_steps(doc, task_id, pid);
    if rows.is_empty() {
        return None;
    }
    let mut weight = 0.;
    let mut done = 0.;
    for r in &rows {
        let w = float(&records::get(doc, "TASKPROC", *r, "proc_wt"));
        let pct = if records::get(doc, "TASKPROC", *r, "complete_flag") == "Y" {
            100.
        } else {
            float(&records::get(doc, "TASKPROC", *r, "complete_pct"))
        };
        weight += w;
        done += w * pct;
    }
    Some(if weight > 0. { done / weight } else { 0. })
}

/// Number the rows 1, 2, ... in the given order.
pub fn renumber_steps(doc: &mut Document, order: &[usize]) {
    for (k, r) in order.iter().enumerate() {
        records::set(doc, "TASKPROC", *r, "seq_num", (k + 1).to_string());
    }
}

/// A step's values when it is added or pasted.
#[derive(Clone, Debug, PartialEq)]
pub struct StepCopy {
    pub name: String,
    pub description: String,
    pub weight: String,
    pub percent: String,
    pub complete: String,
}

impl Default for StepCopy {
    fn default() -> Self {
        Self {
            name: "New Step".into(),
            description: String::new(),
            weight: "1".into(),
            percent: "0".into(),
            complete: "N".into(),
        }
    }
}

/// Add a step row (sequence 0 until the caller renumbers); returns its row.
pub fn new_step_row(doc: &mut Document, task_id: i64, pid: i64, s: &StepCopy) -> usize {
    let id = records::next_id(doc, "TASKPROC", "proc_id");
    records::add_row(
        doc,
        "TASKPROC",
        &[
            ("proc_id", id.to_string()),
            ("task_id", task_id.to_string()),
            ("proj_id", pid.to_string()),
            ("seq_num", "0".into()),
            ("proc_name", s.name.clone()),
            ("complete_flag", s.complete.clone()),
            ("proc_wt", s.weight.clone()),
            ("complete_pct", s.percent.clone()),
            ("proc_descr", s.description.clone()),
        ],
    )
}

/// One edit of a step row.
#[derive(Clone, Debug, PartialEq)]
pub enum StepEdit {
    Name(String),
    Complete(bool),
    Percent(f64),
    Weight(f64),
    Description(String),
}

/// Write one edit to step row `r`.
pub fn edit_step(doc: &mut Document, r: usize, edit: StepEdit) {
    let flag = |b: bool| if b { "Y" } else { "N" }.to_string();
    match edit {
        StepEdit::Name(v) => records::set(doc, "TASKPROC", r, "proc_name", v),
        StepEdit::Complete(done) => {
            records::set(doc, "TASKPROC", r, "complete_flag", flag(done));
            let pct = if done { "100" } else { "0" };
            records::set(doc, "TASKPROC", r, "complete_pct", pct.into());
        }
        StepEdit::Percent(v) => {
            records::set(doc, "TASKPROC", r, "complete_pct", format_number(v));
            records::set(doc, "TASKPROC", r, "complete_flag", flag(v >= 100.));
        }
        StepEdit::Weight(v) => records::set(doc, "TASKPROC", r, "proc_wt", format_number(v)),
        StepEdit::Description(html) => records::set(doc, "TASKPROC", r, "proc_descr", html),
    }
}

/// After a step change: when steps drive percent complete, the activity's
/// physical percent becomes the weighted step percent.
pub fn after_steps_change(p: &mut ProjectData, task_id: i64) {
    let pid = p.model.project.proj_id;
    if super::activity::steps_drive_percent(p, task_id, pid) {
        if let Some(pct) = weighted_step_percent(&p.document, task_id, pid) {
            p.details.entry(task_id).or_default().percent = (pct * 100.).round() / 100.;
        }
    }
}

/// The activity's step named by its number (1 is the first) or its name.
pub fn find_step(p: &ProjectData, task_id: i64, step: &str) -> R<usize> {
    let pid = p.model.project.proj_id;
    let rows = sorted_steps(&p.document, task_id, pid);
    let wanted = step.trim();
    if let Ok(n) = wanted.parse::<usize>() {
        if n >= 1 && n <= rows.len() {
            return Ok(rows[n - 1]);
        }
    }
    let found: Vec<usize> = rows
        .iter()
        .copied()
        .filter(|&r| {
            records::get(&p.document, "TASKPROC", r, "proc_name")
                .trim()
                .eq_ignore_ascii_case(wanted)
        })
        .collect();
    match found.as_slice() {
        [r] => Ok(*r),
        [] => Err(format!("No step {wanted:?} on the activity.")),
        _ => Err(format!("Step {wanted:?} is not unique; name it by number.")),
    }
}

/// Add a step named `name` at position `at` (1 is first; the end when
/// `None`); returns the new step's number.
pub fn add_step(p: &mut ProjectData, task_id: i64, name: &str, at: Option<usize>) -> usize {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let mut order = sorted_steps(doc, task_id, pid);
    let copy = StepCopy {
        name: records::clean(name),
        ..Default::default()
    };
    let row = new_step_row(doc, task_id, pid, &copy);
    let at = at.map_or(order.len(), |n| n.saturating_sub(1).min(order.len()));
    order.insert(at, row);
    renumber_steps(doc, &order);
    after_steps_change(p, task_id);
    at + 1
}

/// Delete step row `r` of the activity and renumber the rest.
pub fn delete_step(p: &mut ProjectData, task_id: i64, r: usize) {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    records::remove_row(doc, "TASKPROC", r);
    let order = sorted_steps(doc, task_id, pid);
    renumber_steps(doc, &order);
    after_steps_change(p, task_id);
}

/// Move step row `r` to position `to` (1 is first).
pub fn move_step(p: &mut ProjectData, task_id: i64, r: usize, to: usize) {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let mut order = sorted_steps(doc, task_id, pid);
    order.retain(|&x| x != r);
    let at = to.saturating_sub(1).min(order.len());
    order.insert(at, r);
    renumber_steps(doc, &order);
}

/// Set one named field of step row `r`: `name`, `complete` (true or
/// false), `percent_complete`, `weight` or `description`.
pub fn set_step(p: &mut ProjectData, task_id: i64, r: usize, field: &str, value: &str) -> R<()> {
    let number = |what: &str| -> R<f64> {
        value
            .trim()
            .trim_end_matches('%')
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.)
            .ok_or_else(|| format!("Step {what} must be a number of 0 or more."))
    };
    let edit = match field.trim().to_ascii_lowercase().as_str() {
        "name" => StepEdit::Name(records::clean(value)),
        "complete" => StepEdit::Complete(match value.trim().to_ascii_lowercase().as_str() {
            "true" | "y" | "yes" | "1" => true,
            "false" | "n" | "no" | "0" | "" => false,
            _ => return Err("Step complete is true or false.".into()),
        }),
        "percent_complete" | "percent" => {
            let v = number("percent complete")?;
            if v > 100. {
                return Err("Step percent complete is 0 to 100.".into());
            }
            StepEdit::Percent(v)
        }
        "weight" => StepEdit::Weight(number("weight")?),
        "description" => StepEdit::Description(if records::looks_like_html(value) {
            value.to_string()
        } else {
            records::text_to_memo(value)
        }),
        other => {
            return Err(format!(
                "Unknown step field {other:?}; use name, complete, percent_complete, weight or description."
            ))
        }
    };
    edit_step(&mut p.document, r, edit);
    after_steps_change(p, task_id);
    Ok(())
}

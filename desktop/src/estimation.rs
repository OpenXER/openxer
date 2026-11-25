//! Tools > Spread Budget Hours (Top-Down)...: spread a labor-unit total over the
//! project or one WBS element by estimation weights
//! (`openxer_xer::estimate`). The dialog shows the WBS under the chosen
//! element with its weights (editable here, saved with the estimate) and the
//! units each element would receive. Apply writes the weights and the units
//! as one undo step; Cancel and Escape leave the project alone.
use crate::dialog_kit as kit;
use crate::views::{record_view, wbs};
use crate::App;
use eframe::egui::{self, Align, Vec2};
use openxer_xer::ProjectData;
use std::collections::HashMap;

const W: f32 = 660.;
const H: f32 = 470.;

pub struct TopDownDialog {
    /// 0 is the whole project, k the k-th WBS choice.
    pub root: usize,
    pub total: String,
    pub not_started_only: bool,
    /// Draft WBS weights by id, saved on Apply.
    pub weights: HashMap<i64, f64>,
    pub selected: Option<usize>,
    pub weight_text: String,
    pub error: Option<String>,
}

/// The WBS rows the dialog lists for `root` (`None`: every element) with
/// their depth below it.
fn rows(p: &ProjectData, root: Option<i64>) -> Vec<(i64, usize)> {
    let tree = wbs::Tree::build(p);
    let project = wbs::project_node(p);
    match root {
        None => tree
            .preorder()
            .into_iter()
            .filter(|(id, _)| Some(*id) != project)
            .map(|(id, d)| {
                (
                    id,
                    if project.is_some() {
                        d.saturating_sub(1)
                    } else {
                        d
                    },
                )
            })
            .collect(),
        Some(r) => {
            let all = tree.preorder();
            let Some(at) = all.iter().position(|(id, _)| *id == r) else {
                return vec![];
            };
            let base = all[at].1;
            std::iter::once(all[at])
                .chain(all[at + 1..].iter().copied().take_while(|(_, d)| *d > base))
                .map(|(id, d)| (id, d - base))
                .collect()
        }
    }
}

/// Units each WBS element receives (its subtree's activities).
fn per_wbs(p: &ProjectData, lines: &[(i64, f64)]) -> HashMap<i64, f64> {
    let parents: HashMap<i64, Option<i64>> = p.wbs.iter().map(|w| (w.id, w.parent)).collect();
    let mut out: HashMap<i64, f64> = HashMap::new();
    for (task, units) in lines {
        let mut at = p.details.get(task).map(|d| d.wbs);
        let mut steps = 0;
        while let Some(id) = at.filter(|id| parents.contains_key(id)) {
            *out.entry(id).or_default() += units;
            at = parents[&id];
            steps += 1;
            if steps > parents.len() {
                break;
            }
        }
    }
    out
}

fn units_text(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl TopDownDialog {
    fn total(&self) -> Result<f64, String> {
        let text = self.total.trim().replace(',', "");
        match text.parse::<f64>() {
            Ok(v) if v.is_finite() && v > 0. => Ok(v),
            _ => Err("Enter the total labor units to distribute (a number above zero).".into()),
        }
    }
}

impl App {
    pub(crate) fn open_top_down_estimation(&mut self) {
        // The WBS view's selected element starts as the target.
        let choices = record_view::wbs_choices(&self.project);
        let root = self
            .views
            .wbs
            .selected
            .and_then(|id| choices.iter().position(|(w, _, _)| *w == id))
            .map_or(0, |k| k + 1);
        self.estimation = Some(TopDownDialog {
            root,
            total: String::new(),
            not_started_only: true,
            weights: HashMap::new(),
            selected: None,
            weight_text: String::new(),
            error: None,
        });
    }

    fn weight_of(&self, d: &TopDownDialog, id: i64) -> f64 {
        d.weights
            .get(&id)
            .copied()
            .unwrap_or_else(|| self.project.wbs_estimation_weight(id))
    }

    /// Writes the draft weights and the estimate as one undo step.
    pub(crate) fn apply_top_down(&mut self, d: &TopDownDialog, time: f64) -> Result<usize, String> {
        let total = d.total()?;
        let choices = record_view::wbs_choices(&self.project);
        let root = d
            .root
            .checked_sub(1)
            .and_then(|k| choices.get(k))
            .map(|c| c.0);
        let lines = self
            .project
            .top_down_distribution(total, root, d.not_started_only, &d.weights);
        if lines.is_empty() {
            return Err("No activities can take labor units here: only not started Task Dependent and Resource Dependent activities receive them.".into());
        }
        let before = self.project.clone();
        let before_revision = self.records_revision;
        for (&id, &w) in &d.weights {
            self.project.set_wbs_estimation_weight(id, w);
        }
        self.project.apply_labor_estimate(&lines);
        self.edit_step(before, before_revision, time);
        self.drop_frame_cache();
        self.status = format!(
            "Top down estimate: {} labor units spread over {} activities.",
            units_text(total),
            lines.len()
        );
        Ok(lines.len())
    }

    pub(crate) fn top_down_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.estimation.take() else {
            return;
        };
        let choices = record_view::wbs_choices(&self.project);
        d.root = d.root.min(choices.len());
        let root = d.root.checked_sub(1).map(|k| choices[k].0);
        let list = rows(&self.project, root);
        let lines = d.total().map_or(vec![], |t| {
            self.project
                .top_down_distribution(t, root, d.not_started_only, &d.weights)
        });
        let received = per_wbs(&self.project, &lines);
        let names: HashMap<i64, (String, String)> = choices
            .iter()
            .map(|(id, code, name)| (*id, (code.clone(), name.clone())))
            .collect();
        let weights: Vec<f64> = list.iter().map(|(id, _)| self.weight_of(&d, *id)).collect();
        let targets: Vec<String> =
            std::iter::once(format!("Project {}", self.project.model.project.name))
                .chain(
                    choices
                        .iter()
                        .map(|(_, code, name)| format!("{code}  {name}")),
                )
                .collect();
        #[derive(Default)]
        struct Clicks {
            apply: bool,
            cancel: bool,
            help: bool,
            clicked: Option<usize>,
            weight: bool,
            changed: bool,
        }
        let (k, closed, actions) = kit::window(
            ctx,
            "top_down",
            "Spread Budget Hours (Top-Down)",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                kit::text(c, 10., 12., "Estimate labor units for", true);
                let items: Vec<(&str, bool)> = targets.iter().map(|s| (s.as_str(), true)).collect();
                k.changed |= c.combo(
                    "top_down_root",
                    170.,
                    8.,
                    350.,
                    21.,
                    &items,
                    &mut d.root,
                    true,
                );
                kit::text(c, 10., 42., "Total labor units", true);
                k.changed |= c
                    .edit(
                        "top_down_total",
                        170.,
                        38.,
                        120.,
                        21.,
                        &mut d.total,
                        true,
                        Align::Max,
                    )
                    .changed();
                k.changed |= c
                    .checkbox(
                        "top_down_not_started",
                        170.,
                        66.,
                        260.,
                        18.,
                        "Not started activities only",
                        &mut d.not_started_only,
                        true,
                    )
                    .clicked();
                let bottom = c.origin.y + 94. + 300. - 1.;
                let out = kit::list(
                    c,
                    "top_down_rows",
                    (6., 94., W - 124., 300.),
                    None,
                    &[
                        kit::Col {
                            title: "WBS Code",
                            width: 150.,
                        },
                        kit::Col {
                            title: "WBS Name",
                            width: 180.,
                        },
                        kit::Col {
                            title: "Estimation Weight",
                            width: 105.,
                        },
                        kit::Col {
                            title: "Labor Units",
                            width: 95.,
                        },
                    ],
                    list.len(),
                    d.selected,
                    None,
                    &mut |c, r, col, rect, s| {
                        let (id, level) = list[r];
                        let (code, name) = names.get(&id).cloned().unwrap_or_default();
                        let text = match col {
                            0 => code,
                            1 => name,
                            2 => units_text(weights[r]),
                            _ => units_text(received.get(&id).copied().unwrap_or(0.)),
                        };
                        let pad = if col == 0 {
                            3. + 14. * level as f32
                        } else {
                            3.
                        };
                        crate::reports::cell_text(c, rect, &text, s, pad, bottom);
                    },
                );
                k.clicked = out.clicked;
                let selected = d.selected.filter(|&r| r < list.len());
                kit::text(c, 10., 406., "Estimation Weight", true);
                k.weight = c
                    .edit(
                        "top_down_weight",
                        170.,
                        402.,
                        80.,
                        21.,
                        &mut d.weight_text,
                        selected.is_some(),
                        Align::Max,
                    )
                    .changed();
                let summary = match (&d.error, d.total()) {
                    (Some(e), _) => e.clone(),
                    (None, Err(_)) => "Enter the total labor units.".into(),
                    (None, Ok(t)) => format!(
                        "{} labor units over {} activities.",
                        units_text(t),
                        lines.len()
                    ),
                };
                kit::text(c, 10., 440., &summary, true);
                kit::strip(c, W, H);
                let column = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::Button("top_down_apply", "Apply", !lines.is_empty(), false),
                        kit::Entry::cancel("top_down_cancel"),
                        kit::Entry::help("top_down_help"),
                    ],
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                k.apply = clicked("top_down_apply");
                k.cancel = clicked("top_down_cancel");
                k.help = clicked("top_down_help");
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.cancel || closed {
            return;
        }
        if k.changed {
            d.error = None;
        }
        if let Some(r) = k.clicked {
            d.selected = Some(r);
            d.weight_text = list
                .get(r)
                .map_or(String::new(), |(id, _)| units_text(self.weight_of(&d, *id)));
        }
        if k.weight {
            if let Some((id, _)) = d.selected.and_then(|r| list.get(r)) {
                match d.weight_text.trim().parse::<f64>() {
                    Ok(v) if v.is_finite() && v >= 0. => {
                        d.weights.insert(*id, v);
                        d.error = None;
                    }
                    _ if d.weight_text.trim().is_empty() => {}
                    _ => d.error = Some("A weight is a number of zero or more.".into()),
                }
            }
        }
        if k.help {
            self.open_help_for("top_down");
        }
        if k.apply {
            let time = ctx.input(|i| i.time);
            match self.apply_top_down(&d, time) {
                Ok(_) => return,
                Err(e) => d.error = Some(e),
            }
        }
        self.estimation = Some(d);
    }
}

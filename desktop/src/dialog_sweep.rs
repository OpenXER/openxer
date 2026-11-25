//! Dialog formatting sweep.
//!
//! A [`Probe`] rides along with the QA [`Driver`]: after every frame it reads
//! what egui laid out in each window and message-box layer (text with its
//! clip rectangle, interactive controls, filled rectangles) and the dialog
//! kit's own notes of which dialogs it drew. [`check`] turns one such
//! snapshot into findings: text cut off or outside its dialog, overlapping or
//! cut-off controls, dialogs off a small screen, windows that do not come
//! from the dialog kit, unwrapped long lines, empty or placeholder text and
//! scroll bars with nothing to scroll. The kit rules are checked too: every
//! dialog is framed by the kit's window shell, standard buttons (OK, Cancel,
//! Close, Apply, Help) are 108 x 25 and keep the standard order. Dialogs not
//! yet moved onto the kit are named in [`NOT_YET_CONVERTED`].
//!
//! `dialog_formatting_has_no_findings` runs the whole sweep (every menu
//! command, toolbar button, tab, Help window and message box) as an ordinary
//! test. `render_dialog_formatting_sweep` is the same sweep with a PNG per
//! dialog under the ignored `artifacts/dialogfmt/` and a findings table; run
//! it explicitly:
//!
//! `cargo.exe test -p openxer-desktop render_dialog_formatting_sweep -- --ignored`
use super::*;
use crate::qa_sweep::Driver;
use egui::epaint::{ClippedShape, Shape};
use egui::{LayerId, Order};
use std::collections::{BTreeMap, BTreeSet};

/// The screen the sweep runs on: the smallest the application supports.
pub(crate) const SMALL_SCREEN: Vec2 = Vec2::new(1366., 768.);

#[derive(Clone, Debug)]
pub(crate) struct TextBox {
    pub text: String,
    /// Where the text is laid out, before clipping.
    pub rect: Rect,
    /// What the painter let through.
    pub clip: Rect,
    pub rows: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Control {
    pub rect: Rect,
    pub visible: Rect,
    pub label: String,
    pub typ: String,
    pub click: bool,
}

/// A scroll bar track found in a layer.
#[derive(Clone, Debug)]
pub(crate) struct Bar {
    pub track: Rect,
    /// The handle covers the whole track: there is nothing to scroll.
    pub fits: bool,
}

/// One window or message-box layer after a frame.
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub name: String,
    pub order: Order,
    pub area: Rect,
    /// Dialog kit clients noted inside this layer.
    pub kit: Vec<String>,
    /// Every client the kit noted this pass (for explaining a miss).
    pub noted: Vec<(String, Rect)>,
    pub texts: Vec<TextBox>,
    pub controls: Vec<Control>,
    pub rounded: Vec<Rect>,
    pub bars: Vec<Bar>,
    /// Push buttons noted on the layer (caption, rectangle), drawing order.
    pub buttons: Vec<(String, Rect)>,
    /// The kit's window shell painted this layer's frame.
    pub framed: bool,
    /// Button columns (false) and rows (true) drawn in a declared observed
    /// order (captions).
    pub declared: Vec<(bool, Vec<String>)>,
    /// The layer's shapes, kept only when rendering.
    pub shapes: Vec<ClippedShape>,
}

#[derive(Default)]
struct Acc {
    texts: Vec<TextBox>,
    rounded: Vec<Rect>,
    rects: Vec<Rect>,
}

fn walk(shape: &Shape, clip: Rect, acc: &mut Acc) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, clip, acc)),
        Shape::Text(text) => {
            let rect = text.visual_bounding_rect();
            if rect.is_positive() && !text.galley.text().trim().is_empty() {
                acc.texts.push(TextBox {
                    text: text.galley.text().to_string(),
                    rect,
                    clip,
                    rows: text.galley.rows.len(),
                });
            } else if text.galley.text().is_empty() {
                acc.texts.push(TextBox {
                    text: String::new(),
                    rect: text.galley.rect.translate(text.pos.to_vec2()),
                    clip,
                    rows: 0,
                });
            }
        }
        Shape::Rect(rect) => {
            let r = rect.rect;
            if rect.fill.a() > 0 || rect.stroke.width > 0. {
                if rect.corner_radius != egui::CornerRadius::ZERO && r.width() > 40. {
                    acc.rounded.push(r);
                }
                acc.rects.push(r);
            }
        }
        _ => {}
    }
}

/// Reads the visible window and foreground layers. Call it inside the egui
/// pass, after the application drew.
pub(crate) fn capture(
    ctx: &egui::Context,
    keep_shapes: bool,
    actions: &[(String, Rect)],
) -> Vec<Snapshot> {
    let layers: Vec<LayerId> = ctx.memory(|m| {
        m.layer_ids()
            .filter(|l| matches!(l.order, Order::Middle | Order::Foreground))
            .filter(|l| m.areas().is_visible(l))
            .collect()
    });
    let clients = crate::dialog_kit::drawn_clients(ctx);
    let framed = crate::dialog_kit::framed_layers(ctx);
    let mut out = vec![];
    for layer in layers {
        let Some(area) = ctx.memory(|m| m.area_rect(layer.id)) else {
            continue;
        };
        if !area.is_positive() {
            continue;
        }
        let entries: Vec<ClippedShape> = ctx.graphics(|g| {
            g.get(layer)
                .map(|list| list.all_entries().cloned().collect())
                .unwrap_or_default()
        });
        let mut acc = Acc::default();
        for entry in &entries {
            walk(&entry.shape, entry.clip_rect, &mut acc);
        }
        let controls: Vec<Control> = ctx.viewport(|v| {
            let widgets = &v.this_pass.widgets;
            widgets
                .get_layer(layer)
                .filter(|w| w.sense.senses_click() || w.sense.senses_drag())
                .filter(|w| w.rect.is_positive())
                .filter_map(|w| {
                    let info = widgets.info(w.id);
                    let typ = info.map(|i| format!("{:?}", i.typ)).unwrap_or_default();
                    // Named by the application's own action table when it
                    // recorded this rectangle.
                    let named = actions.iter().find(|(_, r)| {
                        (r.min - w.rect.min).abs().max_elem() < 1.
                            && (r.max - w.rect.max).abs().max_elem() < 1.
                    });
                    let real = [
                        "Button",
                        "Checkbox",
                        "RadioButton",
                        "SelectableLabel",
                        "ComboBox",
                        "TextEdit",
                        "Slider",
                        "DragValue",
                        "ColorButton",
                    ]
                    .contains(&typ.as_str());
                    if !real && named.is_none() {
                        return None;
                    }
                    Some(Control {
                        rect: w.rect,
                        visible: w.interact_rect,
                        label: named
                            .map(|(n, _)| n.clone())
                            .or_else(|| info.and_then(|i| i.label.clone()))
                            .unwrap_or_default(),
                        typ,
                        click: w.sense.senses_click(),
                    })
                })
                .collect()
        });
        // Rectangles of draggable widgets: a real egui scroll bar is one.
        let draggable: Vec<Rect> = ctx.viewport(|v| {
            v.this_pass
                .widgets
                .get_layer(layer)
                .filter(|w| w.sense.senses_drag())
                .map(|w| w.rect)
                .collect()
        });
        let kit: Vec<String> = clients
            .iter()
            .filter(|(_, r)| {
                // The client sits inside its window: one pixel in from the
                // sides and bottom, a title bar below the top.
                (-1. ..=6.).contains(&(r.left() - area.left()))
                    && (0. ..=40.).contains(&(r.top() - area.top()))
                    && (-1. ..=6.).contains(&(area.right() - r.right()))
                    && (-1. ..=6.).contains(&(area.bottom() - r.bottom()))
            })
            .map(|(n, _)| n.clone())
            .collect();
        // Scroll bars: a 16 px wide (or tall) track followed by a handle
        // inside it.
        let mut bars = vec![];
        for pair in acc.rects.windows(2) {
            let (track, handle) = (pair[0], pair[1]);
            let vertical = (track.width() - 16.).abs() < 0.01 && track.height() >= 17.;
            let horizontal = (track.height() - 16.).abs() < 0.01 && track.width() >= 17.;
            let widget = draggable.iter().any(|d| {
                (d.min - track.min).abs().max_elem() < 1.
                    && (d.max - track.max).abs().max_elem() < 1.
            });
            if (vertical || horizontal) && widget && track.expand(0.5).contains_rect(handle) {
                let fits = if vertical {
                    handle.height() >= track.height() - 1.
                } else {
                    handle.width() >= track.width() - 1.
                };
                bars.push(Bar { track, fits });
            }
        }
        let name = kit.first().cloned().unwrap_or_else(|| {
            acc.texts
                .iter()
                .filter(|t| !t.text.is_empty())
                .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
                .map(|t| t.text.clone())
                .unwrap_or_else(|| format!("{:?}", layer.id))
        });
        out.push(Snapshot {
            name,
            order: layer.order,
            area,
            kit,
            noted: clients.clone(),
            texts: acc.texts,
            controls,
            rounded: acc.rounded,
            bars,
            buttons: crate::dialog_keys::layer_buttons(ctx, layer),
            framed: framed.contains(&layer),
            declared: crate::dialog_kit::declared_orders(ctx, layer),
            shapes: if keep_shapes { entries } else { vec![] },
        });
    }
    out
}

/// One row of the findings table.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Finding {
    pub dialog: String,
    pub issue: &'static str,
    pub measure: String,
}

/// Text the user typed or imported (names, codes, file paths), which may
/// run past a fixed column without being a layout fault.
fn is_user_data(text: &str, data: &BTreeSet<String>) -> bool {
    let t = text.trim().trim_end_matches("...");
    t.contains(":\\")
        || data
            .iter()
            .any(|d| t.contains(d.as_str()) || (t.len() > 3 && d.contains(t)))
}

/// A line this long and this wide should have wrapped.
const LONG_LINE: usize = 60;
const LONG_WIDTH: f32 = 480.;

fn short(text: &str) -> String {
    let t: String = text.chars().take(32).collect();
    if text.chars().count() > 32 {
        format!("{t}...")
    } else {
        t
    }
}

/// Menus and drop-down lists are foreground layers that are not dialogs; a
/// dialog in the foreground is always drawn through the dialog kit.
fn is_popup(s: &Snapshot) -> bool {
    s.order == Order::Foreground && s.kit.is_empty()
}

/// A noted layer that covers the whole screen (the print preview) is a
/// view, not a dialog: it has its own toolbar and status bar instead of a
/// title bar, so it is not framed by the dialog shell. Its text and
/// controls are checked like a dialog's.
fn is_view(s: &Snapshot, screen: Vec2) -> bool {
    s.area.width() >= screen.x - 1. && s.area.height() >= screen.y - 1.
}

/// True for layers that are dialogs, windows or message boxes.
pub(crate) fn is_dialog(s: &Snapshot) -> bool {
    !is_popup(s)
}

/// Problems with one snapshot on a screen of `screen` size, each with the
/// rectangle to outline in the render.
pub(crate) fn check(
    s: &Snapshot,
    screen: Vec2,
    dialog: &str,
    data: &BTreeSet<String>,
) -> Vec<(Finding, Option<Rect>)> {
    let mut out = vec![];
    let mut add = |issue: &'static str, measure: String, at: Option<Rect>| {
        out.push((
            Finding {
                dialog: dialog.to_owned(),
                issue,
                measure,
            },
            at,
        ));
    };
    let screen_rect = Rect::from_min_size(Pos2::ZERO, screen);
    if s.kit.is_empty() {
        add(
            "default egui window, not the dialog kit",
            format!(
                "{:.0} x {:.0}; area {:?}; noted {:?}",
                s.area.width(),
                s.area.height(),
                s.area,
                s.noted
            ),
            None,
        );
    }
    if !s.rounded.is_empty() {
        add(
            "rounded corners",
            format!("{} rounded rectangles", s.rounded.len()),
            s.rounded.first().copied(),
        );
    }
    if s.area.width() > SMALL_SCREEN.x + 1. || s.area.height() > SMALL_SCREEN.y + 1. {
        add(
            "larger than a 1366 x 768 screen",
            format!("{:.0} x {:.0}", s.area.width(), s.area.height()),
            None,
        );
    } else if !screen_rect.expand(0.5).contains_rect(s.area) {
        add(
            "extends off the screen",
            format!(
                "{:.0},{:.0} to {:.0},{:.0} on {:.0} x {:.0}",
                s.area.left(),
                s.area.top(),
                s.area.right(),
                s.area.bottom(),
                screen.x,
                screen.y
            ),
            None,
        );
    }
    let body = s.area.expand(1.);
    for t in &s.texts {
        if t.text.is_empty() {
            continue;
        }
        let visible = t.rect.intersect(t.clip);
        // Text clipped at the side by its own control rectangle. Vertical
        // clipping is a list row scrolled partly out of view.
        let right = t.rect.right() - t.clip.right();
        let left = t.clip.left() - t.rect.left();
        let in_edit = s
            .controls
            .iter()
            .any(|c| c.typ == "TextEdit" && c.rect.expand(2.).contains_rect(t.clip));
        if (right > 1.5 || left > 1.5)
            && visible.is_positive()
            && t.clip.right() <= s.area.right() + 1.
            && !in_edit
            && !is_user_data(&t.text, data)
        {
            add(
                "text cut off by its control",
                format!("{:?} clipped {:.0} px", short(&t.text), right.max(left)),
                Some(t.rect),
            );
        }
        if visible.is_positive() && !body.contains_rect(visible) {
            add(
                "text outside the dialog",
                format!(
                    "{:?} reaches {:.0} px past the edge",
                    short(&t.text),
                    (visible.right() - body.right())
                        .max(body.left() - visible.left())
                        .max(visible.bottom() - body.bottom())
                        .max(body.top() - visible.top())
                ),
                Some(t.rect),
            );
        }
        // A caption that runs past the control it sits in (a tab, a button):
        // the smallest small control that holds the start of the text.
        let holder = s
            .controls
            .iter()
            .filter(|c| c.typ != "TextEdit" && c.rect.width() < 400. && c.rect.height() < 60.)
            .filter(|c| c.rect.contains(t.rect.left_center()))
            .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()));
        if let Some(c) = holder {
            let past = t.rect.right() - c.rect.right();
            if past > 1.5 && visible.is_positive() && !is_user_data(&t.text, data) {
                add(
                    "text runs past its control",
                    format!(
                        "{:?} reaches {past:.0} px past {:?}",
                        short(&t.text),
                        short(&c.label)
                    ),
                    Some(t.rect),
                );
            }
        }
        if t.rows == 1 && t.text.chars().count() > LONG_LINE && t.rect.width() > LONG_WIDTH {
            add(
                "long line does not wrap",
                format!(
                    "{} characters in {:.0} px",
                    t.text.chars().count(),
                    t.rect.width()
                ),
                Some(t.rect),
            );
        }
        let lower = t.text.to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric() && c != '(')
            .collect();
        for marker in [
            "todo",
            "placeholder",
            "lorem",
            "not built",
            "unbuilt",
            "{}",
            "{:?}",
        ] {
            if lower.contains(marker) {
                add(
                    "placeholder or debug text",
                    format!("{:?} contains {marker:?}", short(&t.text)),
                    Some(t.rect),
                );
            }
        }
        for word in ["nan", "some("] {
            if words.contains(&word) && t.text.chars().count() < 12 {
                add(
                    "placeholder or debug text",
                    format!("{:?} reads like debug output", short(&t.text)),
                    Some(t.rect),
                );
            }
        }
    }
    let real_texts = s.texts.iter().filter(|t| !t.text.is_empty()).count();
    if real_texts < 2 {
        add("empty dialog", format!("{real_texts} text items"), None);
    }
    for c in &s.controls {
        if c.typ == "Button" && c.label.trim().is_empty() {
            add(
                "button without a caption",
                format!("{:.0} x {:.0}", c.rect.width(), c.rect.height()),
                Some(c.rect),
            );
        }
        // A row scrolled out of a list has no visible part and is not a
        // control outside the dialog.
        if c.visible.is_positive() && !body.contains_rect(c.visible) && c.rect.width() < 400. {
            add(
                "control outside the dialog",
                format!("{:?} at {:?}", short(&c.label), c.rect),
                Some(c.rect),
            );
        }
        if c.typ == "Button"
            && c.rect.width() < 400.
            && (c.visible.width() < c.rect.width() - 1.5
                || c.visible.height() < c.rect.height() - 1.5)
        {
            add(
                "button cut off",
                format!(
                    "{:?} visible {:.0} x {:.0} of {:.0} x {:.0}",
                    short(&c.label),
                    c.visible.width(),
                    c.visible.height(),
                    c.rect.width(),
                    c.rect.height()
                ),
                Some(c.rect),
            );
        }
    }
    let click: Vec<&Control> = s
        .controls
        .iter()
        .filter(|c| c.click && c.rect.width() < 400. && c.rect.height() < 200.)
        .collect();
    for (i, a) in click.iter().enumerate() {
        for b in &click[i + 1..] {
            let both = a.visible.intersect(b.visible);
            if both.is_positive()
                && both.area() > 6.
                && !a.visible.expand(1.).contains_rect(b.visible)
                && !b.visible.expand(1.).contains_rect(a.visible)
            {
                add(
                    "controls overlap",
                    format!(
                        "{:?} and {:?} share {:.0} x {:.0}",
                        short(&a.label),
                        short(&b.label),
                        both.width(),
                        both.height()
                    ),
                    Some(both),
                );
            }
        }
    }
    for bar in &s.bars {
        if bar.fits {
            add(
                "scroll bar where the content fits",
                format!("track {:.0} x {:.0}", bar.track.width(), bar.track.height()),
                Some(bar.track),
            );
        }
    }
    if !s.kit.is_empty() {
        for (issue, measure, at) in kit_rules(s) {
            if issue == NOT_FRAMED && is_view(s, screen) {
                continue;
            }
            let waived = NOT_YET_CONVERTED
                .iter()
                .any(|(name, i)| *name == s.name && *i == issue);
            if !waived {
                add(issue, format!("{}: {measure}", s.name), at);
            }
        }
    }
    out
}

/// A dialog that does not go through the kit's window shell.
pub(crate) const NOT_FRAMED: &str = "not framed by the kit's window shell";
/// A standard button (OK, Cancel, Close, Apply, Help) of another size.
pub(crate) const BUTTON_SIZE: &str = "standard button not 108 x 25";
/// Standard buttons out of the order OK, Cancel, Apply, others, Help (in a
/// row Help comes first).
pub(crate) const BUTTON_ORDER: &str = "buttons out of the standard order";

/// Dialogs not yet moved onto the dialog kit, by kit name, with the rule
/// each still breaks. Each later batch removes the entries of the dialogs
/// it converts; the list ends empty. Never add an entry for a new dialog.
pub(crate) const NOT_YET_CONVERTED: &[(&str, &str)] = &[];

/// The kit rules one dialog layer breaks: its frame, the size of its
/// standard buttons and their order in each column and row.
fn kit_rules(s: &Snapshot) -> Vec<(&'static str, String, Option<Rect>)> {
    let mut out = vec![];
    if !s.framed {
        out.push((NOT_FRAMED, format!("{:?}", s.area), None));
    }
    let std = crate::dialog_kit::BUTTON;
    for (caption, rect) in &s.buttons {
        if crate::dialog_kit::is_standard_caption(caption)
            && (rect.size() - std).abs().max_elem() > 0.5
        {
            out.push((
                BUTTON_SIZE,
                format!("{caption} is {:.0} x {:.0}", rect.width(), rect.height()),
                Some(*rect),
            ));
        }
    }
    // Columns share a left edge, rows a top edge; each is read in order.
    let mut groups: Vec<(bool, Vec<(String, Rect)>)> = vec![];
    for column in [true, false] {
        let key = |r: &Rect| if column { r.left() } else { r.top() };
        let mut seen: Vec<f32> = vec![];
        for (_, rect) in &s.buttons {
            let k = key(rect);
            if seen.iter().any(|v| (v - k).abs() < 0.5) {
                continue;
            }
            seen.push(k);
            let mut group: Vec<(String, Rect)> = s
                .buttons
                .iter()
                .filter(|(_, r)| (key(r) - k).abs() < 0.5)
                .cloned()
                .collect();
            group.sort_by(|a, b| {
                let (a, b) = if column {
                    (a.1.top(), b.1.top())
                } else {
                    (a.1.left(), b.1.left())
                };
                a.total_cmp(&b)
            });
            group.dedup_by(|a, b| a.1 == b.1);
            if column {
                if group.len() > 1 {
                    groups.push((false, group));
                }
                continue;
            }
            // A row is a run of buttons at most a gap and a group gap
            // apart; a button further off at the same height is not in it.
            let reach = crate::dialog_kit::ROW_GAP + crate::dialog_kit::GROUP_GAP + 1.;
            let mut run: Vec<(String, Rect)> = vec![];
            for button in group {
                if run
                    .last()
                    .is_some_and(|(_, r)| button.1.left() - r.right() > reach)
                {
                    if run.len() > 1 {
                        groups.push((true, std::mem::take(&mut run)));
                    }
                    run.clear();
                }
                run.push(button);
            }
            if run.len() > 1 {
                groups.push((true, run));
            }
        }
    }
    for (row, group) in groups {
        let captions: Vec<&str> = group.iter().map(|(c, _)| c.as_str()).collect();
        // A column or row drawn in its dialog's declared observed order is
        // held to that order (it is drawn from it); every other group to
        // the standard one.
        let plain: Vec<String> = captions
            .iter()
            .map(|c| crate::chrome::mnemonic(c).0)
            .collect();
        if s.declared.contains(&(row, plain)) {
            continue;
        }
        if let Some(problem) = crate::dialog_kit::order_problem(&captions, row) {
            let at = group.iter().fold(Rect::NOTHING, |a, (_, r)| a.union(*r));
            out.push((
                BUTTON_ORDER,
                format!("{problem} ({})", captions.join(", ")),
                Some(at),
            ));
        }
    }
    out
}

/// Collects what the sweep sees.
#[derive(Default)]
pub(crate) struct Probe {
    pub enabled: bool,
    /// Write a PNG per dialog state under this folder.
    pub render: Option<PathBuf>,
    pub snaps: Vec<Snapshot>,
    painter: crate::headless_render::SoftwarePainter,
    pub findings: BTreeSet<Finding>,
    /// Dialog states seen: name to the file written (or empty).
    pub seen: BTreeMap<String, String>,
    /// Each distinct dialog (by kit name or title) with the commands that
    /// showed it.
    pub dialogs: BTreeMap<String, BTreeSet<String>>,
    /// The window rectangle each dialog was last seen at.
    pub rects: BTreeMap<String, Rect>,
    /// Commands already tried by the survey.
    pub tried: BTreeSet<String>,
    /// Pictures written so far, for file numbering.
    pub pictures: usize,
}

impl Probe {
    pub(crate) fn on(render: Option<PathBuf>) -> Self {
        Self {
            enabled: true,
            render,
            ..Default::default()
        }
    }

    pub(crate) fn frame_done(&mut self, snaps: Vec<Snapshot>, output: &egui::FullOutput) {
        self.snaps = snaps;
        if self.render.is_some() {
            self.painter.ingest(output);
        }
    }
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if c != '-' || !out.ends_with('-') {
            out.push(c.to_ascii_lowercase());
        }
    }
    out.trim_matches('-').chars().take(90).collect()
}

fn outline(image: &mut image::RgbaImage, rect: Rect, color: [u8; 4]) {
    let (w, h) = (image.width() as i32, image.height() as i32);
    let (l, t, r, b) = (
        rect.left().round() as i32,
        rect.top().round() as i32,
        rect.right().round() as i32,
        rect.bottom().round() as i32,
    );
    let mut put = |x: i32, y: i32| {
        if x >= 0 && y >= 0 && x < w && y < h {
            image.put_pixel(x as u32, y as u32, image::Rgba(color));
        }
    };
    for x in l..=r {
        put(x, t);
        put(x, b);
    }
    for y in t..=b {
        put(l, y);
        put(r, y);
    }
}

impl Driver {
    /// A PNG of the whole screen as the next frame draws it (a driver made
    /// with [`Driver::probed`] and a render folder).
    pub(crate) fn screenshot(&mut self, path: &std::path::Path) {
        assert!(
            self.probe.render.is_some(),
            "screenshots need a probed driver"
        );
        let output = self.frame(vec![]);
        let image = self
            .probe
            .painter
            .rasterize(
                &self.ctx,
                output.shapes,
                output.pixels_per_point,
                self.screen,
            )
            .0;
        image.save(path).unwrap();
    }

    /// Records every dialog on screen now under `label`: checks it and, when
    /// rendering, writes a PNG of it.
    pub(crate) fn record_dialogs(&mut self, label: &str) {
        self.frames(2);
        let _ = self.frame(vec![]);
        let snaps = self.probe.snaps.clone();
        let data = user_data(&self.app);
        let dialogs: Vec<&Snapshot> = snaps.iter().filter(|s| is_dialog(s)).collect();
        let multiple = dialogs.len() > 1;
        for s in dialogs {
            let key = if multiple {
                format!("{label} / {}", s.name)
            } else {
                label.to_owned()
            };
            self.probe
                .dialogs
                .entry(s.name.clone())
                .or_default()
                .insert(label.to_owned());
            self.probe.rects.insert(s.name.clone(), s.area);
            let found = check(s, self.screen, &key, &data);
            let mut file = String::new();
            if let Some(dir) = self.probe.render.clone() {
                let crop = s.area.expand(6.);
                let delta = -crop.min.to_vec2();
                let shapes = s
                    .shapes
                    .iter()
                    .map(|c| ClippedShape {
                        clip_rect: c.clip_rect.translate(delta),
                        shape: {
                            let mut shape = c.shape.clone();
                            shape.translate(delta);
                            shape
                        },
                    })
                    .collect();
                let (mut shot, _) =
                    self.probe
                        .painter
                        .rasterize(&self.ctx, shapes, 1., crop.size());
                for (_, at) in &found {
                    if let Some(at) = at {
                        outline(&mut shot, at.translate(delta), [255, 0, 0, 255]);
                    }
                }
                self.probe.pictures += 1;
                file = format!("{:03}-{}.png", self.probe.pictures, slug(&key));
                std::fs::create_dir_all(&dir).unwrap();
                shot.save(dir.join(&file)).unwrap();
            }
            self.probe.seen.entry(key).or_insert(file);
            self.probe
                .findings
                .extend(found.into_iter().map(|(f, _)| f));
        }
    }

    /// Called by `try_command` with a command's dialog open: records it,
    /// then each tab and each Help window reachable from it.
    pub(crate) fn explore(&mut self, action: &str, baseline: &BTreeSet<String>) {
        self.explore_with(action, baseline, true);
    }

    /// `explore`; `escape` also presses Escape and expects every dialog the
    /// command opened to close (not for a child window over its parent).
    fn explore_with(&mut self, action: &str, baseline: &BTreeSet<String>, escape: bool) {
        self.record_dialogs(action);
        let keys: Vec<String> = self
            .keys()
            .into_iter()
            .filter(|k| !baseline.contains(k))
            .collect();
        for tab in keys.iter().filter(|k| is_tab(k)) {
            if self.has(tab) {
                self.click(tab);
                self.record_dialogs(&format!("{action} tab {tab}"));
            }
        }
        // Cell Tips keys end in `_help` too; they are no Help button.
        let helps = keys
            .iter()
            .filter(|k| k.ends_with("_help") && !k.contains("hint_help"));
        for help in helps {
            if self.has(help) {
                self.click(help);
                self.record_dialogs(&format!("{action} help"));
                let by_button = self.help_topic();
                self.close_help();
                // F1 on the same dialog opens the same topic.
                self.key(Key::F1);
                self.frames(2);
                let by_f1 = self.help_topic();
                self.close_help();
                let issue = match (&by_button, &by_f1) {
                    // A parent's Help under a modal child cannot be
                    // clicked; its own scene checks it.
                    (None, _) if !escape => None,
                    (None, _) => Some("Help opens no topic"),
                    (Some(t), _) if t == crate::help::OVERVIEW || t.starts_with("view-") => {
                        Some("Help opens no topic of its own")
                    }
                    (b, f) if b != f => Some("F1 and Help open different topics"),
                    _ => None,
                };
                if let Some(issue) = issue {
                    self.probe.findings.insert(Finding {
                        dialog: action.to_owned(),
                        issue,
                        measure: format!("{help}: Help {by_button:?}, F1 {by_f1:?}"),
                    });
                }
            }
        }
        if escape && !keys.is_empty() {
            self.escape_closes(action);
        }
    }

    /// Escape must close what a command opened. A dialog that stays is a
    /// finding; the caller closes it by its own buttons afterwards.
    fn escape_closes(&mut self, action: &str) {
        self.frames(2);
        let shown: BTreeSet<String> = self
            .probe
            .snaps
            .iter()
            .filter(|s| is_dialog(s))
            .map(|s| s.name.clone())
            .collect();
        if shown.is_empty() {
            return;
        }
        self.key(Key::Escape);
        self.frames(3);
        let left: Vec<String> = self
            .probe
            .snaps
            .iter()
            .filter(|s| is_dialog(s) && shown.contains(&s.name))
            .map(|s| s.name.clone())
            .collect();
        if !left.is_empty() {
            self.probe.findings.insert(Finding {
                dialog: action.to_owned(),
                issue: "does not close on Escape",
                measure: left.join(", "),
            });
        }
    }

    /// The topic the help window shows, if it is open.
    fn help_topic(&mut self) -> Option<String> {
        self.frames(1);
        self.app.help.open.then(|| self.app.help.topic.clone())
    }

    /// Closes the help window if it is showing.
    fn close_help(&mut self) {
        self.frames(2);
        if self.has("help_close") {
            self.click("help_close");
        }
        self.frames(2);
        assert!(
            !self.app.help.open,
            "the help window did not close with its Close button"
        );
    }
}

/// Every string in the project that dialogs show as data.
fn user_data(app: &App) -> BTreeSet<String> {
    let project = &app.project;
    let mut out = BTreeSet::new();
    // Bar names are typed into the Bars dialog.
    out.extend(app.gantt.bars.iter().map(|b| b.name.clone()));
    out.extend(app.gantt.bars.iter().map(|b| b.filter.label()));
    for table in &project.document.tables {
        for row in &table.rows {
            for cell in row {
                if cell.len() >= 4 {
                    out.insert(cell.clone());
                }
            }
        }
    }
    for i in 0..project.model.activities.len() {
        out.insert(project.name(i).to_string());
        out.insert(project.model.activities[i].task_code.clone());
    }
    out
}

/// A tab of a tabbed dialog (the Activity Details strip is not one).
fn is_tab(key: &str) -> bool {
    // The tab strips of the main views are not dialog tabs.
    const VIEWS: [&str; 6] = [
        "detail_",
        "dock_",
        "wbs_",
        "projects_",
        "resources_",
        "assignments_",
    ];
    key.contains("_tab_") && !VIEWS.iter().any(|v| key.starts_with(v))
}

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// A project the survey can open every dialog on.
fn survey_driver(render: Option<PathBuf>) -> Driver {
    let mut d = Driver::probed("dialogfmt", starter(), SMALL_SCREEN, render);
    d.enter_check = true;
    d.setup = Some(survey_setup);
    survey_setup(&mut d);
    d
}

fn survey_setup(d: &mut Driver) {
    // Paths a user would have, not this machine's long temporary folder.
    // Off Windows "C:\..." is a relative file name and was written into the
    // crate folder; a folder that is not there fails as C:\Reports does.
    if cfg!(windows) {
        d.app.sched.check_report_path = "C:\\Reports\\Check Schedule.html".into();
        d.app.sched.log_path = "C:\\Reports\\SCHEDLOG.TXT".into();
    } else {
        d.app.sched.check_report_path = "/Reports/Check Schedule.html".into();
        d.app.sched.log_path = "/Reports/SCHEDLOG.TXT".into();
    }
    d.frames(3);
    d.home();
    d.select(0);
}

/// Runs the whole sweep on the starter project.
fn run_survey(render: Option<PathBuf>) -> Driver {
    let mut d = survey_driver(render);
    d.sweep_menus();
    d.sweep_toolbars();
    extra_scenes(&mut d, "");
    d
}

/// Opens one dialog by a means the menus do not cover, records it with its
/// tabs and Help, and closes it again.
fn scene(d: &mut Driver, only: &str, name: &str, open: impl FnOnce(&mut Driver)) {
    if !only.is_empty() && !only.split(',').any(|o| o == name) {
        return;
    }
    d.home();
    let baseline = d.keys();
    open(d);
    d.frames(3);
    // A child window closes before its parent, so Escape closes only one.
    let child = [
        "bar filter",
        "calendar used by",
        "calendar editor",
        "calendar delete",
        "timescale date picker",
        "timescale date picker specific date",
        "modify global change",
        "edit column",
        "sort",
        "group band editor",
        "filter editor",
        "calendar weekly hours",
        "calendar time periods",
        "calendar copy from",
        "add code value",
    ];
    d.explore_with(name, &baseline, !child.contains(&name));
    if std::env::var_os("OPENXER_SURVEY_KEYS").is_some() {
        let keys: Vec<String> = d
            .keys()
            .into_iter()
            .filter(|k| !baseline.contains(k))
            .collect();
        println!("SCENE {name} keys: {keys:?}");
    }
    // Escape closes the top dialog first, so a child goes before its parent.
    for _ in 0..4 {
        if d.keys().iter().all(|k| baseline.contains(k)) {
            break;
        }
        d.key(Key::Escape);
    }
    d.close_dialogs(&baseline, name);
    d.app.error = None;
    d.home();
}

/// Message boxes, confirmations and child windows that no menu command or
/// toolbar button opens by itself.
fn extra_scenes(d: &mut Driver, only: &str) {
    scene(d, only, "error message", |d| {
        d.app.error = Some(
            "The schedule could not be calculated because activity RB-1020 has a relationship to itself and the calendar for RB-1030 has no work time."
                .into(),
        );
    });
    scene(d, only, "delete several activities", |d| {
        let ids: Vec<i64> = d
            .app
            .project
            .model
            .activities
            .iter()
            .take(8)
            .map(|a| a.task_id)
            .collect();
        d.app.delete_confirm = Some(ids);
    });
    scene(d, only, "import message", |d| {
        d.app.interchange.message = Some(crate::interchange::Message {
            title: "Import".into(),
            text: "The file was read with 3 warnings.".into(),
            lines: (1..=12)
                .map(|n| format!("Row {n}: a value was outside its range and was ignored."))
                .collect(),
        });
    });
    scene(d, only, "import wizard", |d| d.app.open_import_wizard());
    // Every later page of the import wizard, on a file of two projects.
    let pages = [
        ("import file name", 1),
        ("import project options", 2),
        ("import update options", 3),
        ("import summary", 4),
    ];
    for (name, step) in pages {
        scene(d, only, name, |d| d.app.open_import_wizard_at(step));
    }
    scene(d, only, "export wizard", |d| d.app.open_export_wizard());
    scene(d, only, "new activity wizard", |d| {
        d.app.open_activity_wizard()
    });
    scene(d, only, "new resource wizard", |d| {
        d.app.open_resource_wizard()
    });
    scene(d, only, "edit relationship", |d| {
        d.app.edit_link(0);
    });
    scene(d, only, "calendars", |d| d.app.open_calendars());
    scene(d, only, "schedule log", |d| d.app.show_log = true);
    scene(d, only, "check schedule results", |d| {
        d.app.open_check_schedule();
        d.frames(3);
        d.click("check_run");
    });
    scene(d, only, "customize resource columns", |d| {
        d.app.selected = Some(0);
        d.click("detail_tab_resources");
        d.app
            .open_column_customize(&crate::details::grid_columns::RESOURCES);
    });
    scene(d, only, "calendar used by", |d| {
        d.app.open_calendars();
        d.click("calendar_list_row_0");
        d.click("calendars_used_by");
    });
    scene(d, only, "calendar editor", |d| {
        d.app.open_calendars();
        d.click("calendar_list_row_0");
        d.click("calendars_modify");
    });
    scene(d, only, "calendar delete", |d| {
        d.app.open_calendars();
        d.click("calendar_list_row_0");
        d.click("calendars_delete");
    });
    scene(d, only, "bar filter", |d| {
        d.click("tool_bars");
        d.click("bars_filter_0");
    });
    scene(d, only, "run report", |d| {
        let id = d.app.new_blank_report(None).unwrap();
        d.app
            .open_run_report(crate::reports::rpt_entries::Which::Stored(id), "New Report");
    });
    scene(d, only, "run batch", |d| d.app.open_run_batch());
    scene(d, only, "select report group", |d| {
        let entry = d.app.report_entries().remove(0);
        d.app.reports.pick_group = Some(crate::reports::reports_view::GroupPick {
            selected: entry.group,
            name: entry.name,
            which: entry.which,
        });
    });
    scene(d, only, "report editor", |d| {
        let id = d.app.new_blank_report(None).unwrap();
        d.app.open_report_editor(id);
    });
    scene(d, only, "report properties", |d| {
        let id = d.app.new_blank_report(None).unwrap();
        d.app.open_report_editor(id);
        d.frames(3);
        d.click("report_editor_properties");
    });
    scene(d, only, "print preview", |d| {
        d.click("tool_print_preview");
    });
    scene(d, only, "timescale date picker", |d| {
        d.click("menu_bar_file");
        d.click("menu_page_setup");
        d.frames(3);
        d.click("page_setup_tab_4");
        d.click("page_setup_ts_start_pick");
    });
    scene(d, only, "timescale date picker specific date", |d| {
        d.click("menu_bar_file");
        d.click("menu_page_setup");
        d.frames(3);
        d.click("page_setup_tab_4");
        d.click("page_setup_ts_finish_pick");
        d.click("ts_picker_day_28");
    });
    scene(d, only, "report print preview", |d| {
        let page = crate::print::ops::Page {
            size: Vec2::new(816., 1056.),
            ops: vec![],
        };
        d.app.reports.page_preview = Some(crate::reports::rpt_run::PagePreview::from_pages(
            "Report".into(),
            vec![page.clone(), page],
            vec![],
            (String::new(), String::new()),
            d.app.viewer_start(),
        ));
    });
    scene(d, only, "wbs delete", |d| {
        d.click("directory_wbs");
        d.frames(2);
        d.click("wbs_row_1");
        d.click("dock_delete");
    });
    scene(d, only, "wbs delete all", |d| {
        d.click("directory_wbs");
        d.frames(2);
        d.click("wbs_row_1");
        d.click("dock_delete");
        if d.has("wbs_confirm_delete_all") {
            d.click("wbs_confirm_delete_all");
        }
    });
    scene(d, only, "delete project", |d| {
        d.click("directory_projects");
        d.frames(2);
        d.click("projects_row_0");
        d.click("dock_delete");
    });
    scene(d, only, "copy project options", |d| {
        d.click("directory_projects");
        d.frames(2);
        d.click("projects_row_0");
        d.click("dock_copy");
        d.click("dock_paste");
    });
    scene(d, only, "new project from projects add", |d| {
        d.click("directory_projects");
        d.frames(2);
        d.click("projects_row_0");
        d.click("add");
    });
    scene(d, only, "copy activity options", |d| {
        d.app.editing.dialogs.copy_options = Some(Default::default());
    });
    scene(d, only, "edit summary", |d| {
        d.app.editing.dialogs.summary = Some(crate::editing::dialogs::Summary {
            title: "Paste".into(),
            applied: 4,
            rejected: (0..3)
                .map(|n| crate::editing::Rejected {
                    activity: format!("RB-10{n}0"),
                    column: "Original Duration".into(),
                    value: "x".into(),
                    reason: "Enter a duration.".into(),
                })
                .collect(),
        });
    });
    scene(d, only, "renumber duplicates", |d| {
        d.app.editing.dialogs.duplicates = Some(crate::editing::dialogs::DuplicatesDraft {
            targets: vec![(0, "RB-1010".into()), (1, "RB-1020".into())],
            shown: vec![0, 1],
            paste: None,
        });
    });
    scene(d, only, "renumber on paste", |d| {
        d.app.editing.options.renumber.on_paste = true;
        let clip = crate::editing::ops::copy(&d.app.project, &[0]);
        d.app.editing.clipboard = Some(clip);
        d.app.paste_after_options(0.);
    });
    scene(d, only, "add code value", |d| {
        d.app.selected = Some(0);
        d.app.detail.picker = Some(crate::details::Picker::Code {
            type_id: None,
            selected: None,
        });
        d.frames(2);
        d.app.detail.new_code = Some(crate::details::assign_kit::NewCode::default());
    });
    scene(d, only, "modify global change", |d| {
        d.app.open_global_change();
        d.frames(3);
        d.click("gc_new");
    });
    scene(d, only, "global change report", |d| {
        let change = |n: usize, error: Option<&str>| crate::editing::gc::Change {
            task_id: 0,
            project: "RB".into(),
            wbs: "RB.1".into(),
            activity: format!("RB-10{n}0"),
            name: "Pour footings".into(),
            column: "original_duration".into(),
            title: "Original Duration".into(),
            old: "5d".into(),
            new_text: "6d".into(),
            new: "6d".into(),
            error: error.map(String::from),
            apply: error.is_none(),
        };
        d.app.editing.dialogs.gc_report = Some(crate::editing::dialogs::GcReport::new(
            "Field work",
            vec![change(1, None), change(2, Some("Not editable"))],
        ));
    });
    scene(d, only, "edit column", |d| {
        d.app.open_columns_dialog();
        d.frames(3);
        d.click("columns_edit");
    });
    scene(d, only, "sort", |d| {
        d.app.open_group_sort_dialog();
        d.frames(3);
        d.click("group_sort");
    });
    scene(d, only, "group band editor", |d| {
        d.app.open_group_sort_dialog();
        d.frames(3);
        d.app.grid.dialogs.group_sort.as_mut().unwrap().band_edit = Some(0);
    });
    scene(d, only, "filter editor", |d| {
        d.app.open_filters_dialog();
        d.frames(3);
        d.click("filters_new");
    });
    scene(d, only, "calendar weekly hours", |d| {
        d.app.open_calendars();
        d.click("calendar_list_row_0");
        d.click("calendars_modify");
        d.frames(3);
        d.click("calendar_workweek");
    });
    scene(d, only, "calendar time periods", |d| {
        d.app.open_calendars();
        d.click("calendar_list_row_0");
        d.click("calendars_modify");
        d.frames(3);
        d.click("calendar_time_periods");
    });
    scene(d, only, "calendar copy from", |d| {
        d.app.open_calendars();
        d.frames(3);
        d.click("calendars_add");
    });
    scene(d, only, "projects customize columns", |d| {
        d.click("directory_projects");
        d.frames(2);
        d.click("projects_layout");
        d.click("projects_layout_customize");
    });
}

/// Opens only the named commands (comma separated in `OPENXER_SURVEY_ONLY`),
/// for quick reruns while fixing one dialog.
fn run_only(names: &str, render: Option<PathBuf>) -> Driver {
    let mut d = survey_driver(render);
    d.home();
    let baseline = d.keys();
    if let Some(scenes) = names.strip_prefix("scene:") {
        extra_scenes(&mut d, scenes);
        return d;
    }
    // Entries are `action` or `menu:action` (the menu bar name, lower case).
    for entry in names.split(',').filter(|n| !n.is_empty()) {
        d.home();
        let (menu, name) = match entry.split_once(':') {
            Some((menu, name)) => (Some(menu.to_string()), name),
            None => (
                crate::menu::menu_holding(entry).map(|m| m.to_lowercase()),
                entry,
            ),
        };
        if let Some(menu) = menu {
            d.click(&format!("menu_bar_{menu}"));
            d.frames(2);
        }
        d.try_command(name, &baseline, "only");
    }
    d
}

fn report(d: &Driver) -> String {
    let mut text = String::from("| dialog | issue | measure |\n|---|---|---|\n");
    for f in &d.probe.findings {
        text.push_str(&format!("| {} | {} | {} |\n", f.dialog, f.issue, f.measure));
    }
    text.push_str("\nDialogs seen (window size and position, then the commands that opened it):\n");
    for (name, via) in &d.probe.dialogs {
        let r = d.probe.rects[name];
        text.push_str(&format!(
            "- {name}: {:.0} x {:.0} at {:.0},{:.0}; {}\n",
            r.width(),
            r.height(),
            r.left(),
            r.top(),
            via.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    text
}

/// Known exceptions: a finding is listed here only when the application is
/// right and the check cannot tell. Each entry is `(dialog, issue)`.
const ACCEPTED: &[(&str, &str)] = &[
    // The preview shows printed pages: their text is clipped at the page
    // edge, as it prints.
    ("menu_print_preview", "text cut off by its control"),
    ("tool_print_preview", "text cut off by its control"),
    ("print preview", "text cut off by its control"),
];

fn unexpected(d: &Driver) -> Vec<&Finding> {
    d.probe
        .findings
        .iter()
        .filter(|f| {
            !ACCEPTED
                .iter()
                .any(|(dialog, issue)| f.dialog.starts_with(dialog) && f.issue == *issue)
        })
        .collect()
}

/// Fails with the findings table when the survey found a problem.
fn assert_clean(d: &Driver, part: &str, at_least: usize) {
    assert!(
        d.probe.dialogs.len() >= at_least,
        "{part}: the sweep reached only {} dialogs",
        d.probe.dialogs.len()
    );
    let found = unexpected(d);
    if !found.is_empty() {
        // The whole table, for a run whose log keeps only the first lines.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("artifacts/dialogfmt");
        let table: Vec<String> = found
            .iter()
            .map(|f| format!("{} | {} | {}", f.dialog, f.issue, f.measure))
            .collect();
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(
                dir.join(format!("findings-{}.txt", slug(part))),
                table.join(
                    "
",
                ),
            );
        }
    }
    assert!(
        found.is_empty(),
        "{part}: dialog formatting findings:\n{}",
        found
            .iter()
            .map(|f| format!("{} | {} | {}", f.dialog, f.issue, f.measure))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The survey on a 1366 x 768 screen: every menu command, toolbar button,
/// tab, Help window and message box. There is no text outside its control
/// or dialog, no overlapping or cut-off controls, nothing off screen, no
/// window outside the dialog kit, no empty dialog and no scroll bar with
/// nothing to scroll. One test per menu, so they run side by side.
macro_rules! menu_survey {
    ($($name:ident: $menu:literal,)*) => {$(
        #[test]
        fn $name() {
            let mut d = survey_driver(None);
            let baseline = d.keys();
            let (menu, subs) = crate::qa_sweep::MENUS
                .iter()
                .find(|(m, _)| *m == $menu)
                .copied()
                .unwrap();
            d.sweep_menu(menu, subs, &baseline);
            assert_clean(&d, concat!($menu, " menu"), 1);
        }
    )*};
}

menu_survey! {
    dialog_formatting_file_menu: "file",
    dialog_formatting_edit_menu: "edit",
    dialog_formatting_view_menu: "view",
    dialog_formatting_project_menu: "project",
    dialog_formatting_enterprise_menu: "enterprise",
    dialog_formatting_tools_menu: "tools",
    dialog_formatting_ai_agents_menu: "ai_agents",
    dialog_formatting_admin_menu: "admin",
    dialog_formatting_help_menu: "help",
}

#[test]
fn dialog_formatting_toolbars() {
    let mut d = survey_driver(None);
    d.sweep_toolbars();
    assert_clean(&d, "toolbars", 10);
}

#[test]
fn dialog_formatting_message_boxes_and_child_windows() {
    let mut d = survey_driver(None);
    extra_scenes(&mut d, "");
    assert_clean(&d, "scenes", 10);
}

#[test]
#[ignore = "writes artifacts/dialogfmt; run explicitly"]
fn render_dialog_formatting_sweep() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/dialogfmt");
    let only = std::env::var("OPENXER_SURVEY_ONLY").unwrap_or_default();
    let d = if only.is_empty() {
        let _ = std::fs::remove_dir_all(&out);
        run_survey(Some(out.clone()))
    } else {
        run_only(&only, Some(out.clone()))
    };
    std::fs::create_dir_all(&out).unwrap();
    let name = if only.is_empty() {
        "report.md"
    } else {
        "report-only.md"
    };
    std::fs::write(out.join(name), report(&d)).unwrap();
    println!("{}", report(&d));
}

/// The close box in a kit window's title bar closes it: a window with an
/// open flag clears the flag, one without answers like Escape.
#[test]
fn the_title_bar_close_box_closes_kit_windows() {
    let mut d = Driver::probed("close-box", starter(), SMALL_SCREEN, None);
    d.frames(3);
    d.home();
    let close_box = |d: &Driver, name: &str| {
        let area = d
            .probe
            .snaps
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{name} is drawn"))
            .area;
        Pos2::new(area.right() - 24., area.top() + 16.)
    };
    // About has no open flag.
    d.click("menu_bar_help");
    d.click("menu_about");
    d.frames(3);
    assert!(d.app.show_about);
    let at = close_box(&d, "About OpenXER");
    d.click_at(at);
    d.frames(3);
    assert!(!d.app.show_about, "the close box closed About");
    // Schedule Log has one.
    d.app.show_log = true;
    d.frames(3);
    let at = close_box(&d, "Schedule Log");
    d.click_at(at);
    d.frames(3);
    assert!(!d.app.show_log, "the close box closed the Schedule Log");
}

/// No source file builds a window or modal of its own: every dialog goes
/// through the dialog kit (`dialog_kit::shell` and the flow window in
/// `kit_window`), which is what gives them one look. A dialog added with a
/// bare `egui::Window`, `egui::Modal` or `egui::Area`, or a private
/// `fn window` wrapper around the kit, fails here even when no sweep
/// reaches it.
/// Files whose `egui::Area` is not a dialog: drop-down lists, floating
/// toolbars, the print preview page and in-place popups.
const AREAS_NOT_DIALOGS: &[&str] = &[
    "details/form.rs",
    "dockbars_ui.rs",
    "gantt/mod.rs",
    "grid/cell_combo.rs",
    "grid/table.rs",
    "panes/network.rs",
    "print/preview.rs",
];

/// A source path relative to `src`, with forward slashes.
fn rel(path: &std::path::Path) -> String {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    path.strip_prefix(&src)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn no_dialog_is_a_bare_egui_window() {
    fn visit(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, hits);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let allowed = ["kit_window.rs", "dialog_kit.rs", "dialog_sweep.rs"];
            if !name.ends_with(".rs") || allowed.contains(&name.as_str()) {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                let wrapper =
                    code.trim_start().starts_with("fn window(") && !name.ends_with("_tests.rs");
                let area = code.contains("Area::new(")
                    && !AREAS_NOT_DIALOGS.contains(&rel(&path).as_str())
                    && !name.ends_with("_tests.rs");
                if ["Window::new(", "Modal::new("]
                    .iter()
                    .any(|p| code.contains(p))
                    || wrapper
                    || area
                {
                    hits.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
    }
    let mut hits = vec![];
    visit(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(
        hits.is_empty(),
        "dialogs must use chrome::dialog_window or chrome::plain_window:\n{}",
        hits.join("\n")
    );
}

/// The kit's button column, move pad, tab lists and message box are written
/// once, in the kit. A second copy declared anywhere else fails here.
#[test]
fn kit_controls_are_written_once() {
    fn visit(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, hits);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let kit = ["dialog_kit.rs", "kit_window.rs", "dialog_sweep.rs"];
            if !name.ends_with(".rs") || kit.contains(&name.as_str()) {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                let copy = [
                    "fn button_column(",
                    "fn move_pad(",
                    "fn move_button(",
                    "fn message_box(",
                    "fn vertical_tabs",
                    "fn link(c: &mut Canvas",
                ]
                .iter()
                .any(|p| code.contains(p));
                // A tab strip on a Canvas (a method listing tabs is fine).
                let tabs = code.contains("fn tabs(")
                    && lines[n..(n + 3).min(lines.len())]
                        .iter()
                        .any(|l| l.contains("Canvas"));
                if copy || tabs {
                    hits.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
    }
    let mut hits = vec![];
    visit(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(
        hits.is_empty(),
        "use the dialog kit's control instead of a copy:\n{}",
        hits.join("\n")
    );
}

/// Dialogs whose own reference shot shows a button column or bottom row
/// order other than the standard one, and which draw it through
/// `dialog_kit::observed_button_column` or `observed_button_row`. Any
/// other caller fails here: the standard order stays the rule for every
/// other dialog.
const OBSERVED_ORDER_DIALOGS: &[&str] = &[
    "calendars/editor.rs",
    "calendars/mod.rs",
    "schedule_dialogs.rs",
];

#[test]
fn only_named_dialogs_declare_an_observed_button_order() {
    fn visit(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, hits);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs")
                || ["dialog_kit.rs", "dialog_sweep.rs"].contains(&name.as_str())
            {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            if (text.contains("observed_button_column(") || text.contains("observed_button_row("))
                && !OBSERVED_ORDER_DIALOGS.contains(&rel(&path).as_str())
            {
                hits.push(rel(&path));
            }
        }
    }
    let mut hits = vec![];
    visit(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(
        hits.is_empty(),
        "an observed button order is declared only by the dialogs in OBSERVED_ORDER_DIALOGS: {hits:?}"
    );
}

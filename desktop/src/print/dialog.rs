//! The Page Setup dialog: Page, Margins, Header, Footer and Options tabs in
//! a notebook, with OK, Cancel, Apply, Default and Help in the standard
//! button column. Control rectangles are fixed client pixels.
use super::compose::Summary;
use super::setup::{self, Content, IncludeOn, Orientation, PageSetup, Scaling, TextAlign};
use super::ts_picker::{Field, Request, TsPicker};
use crate::dialog_kit::{self as kit, Entry, FACE, SHADOW};
use crate::schedule_dialogs::{modal, Canvas};
use crate::*;

/// Warnings about the printout, on the dialog face.
const WARNING: Color32 = crate::theme::color(crate::theme::WARNING_TEXT);
pub const CLIENT: Vec2 = Vec2::new(744., 413.);
pub const TABS: [&str; 5] = ["Page", "Margins", "Header", "Footer", "Options"];

fn number(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Dialog state: the draft and the text of every number field.
pub struct SetupDialog {
    pub draft: PageSetup,
    pub saved: PageSetup,
    pub tab: usize,
    /// Selected section of the header and the footer.
    pub section: [usize; 2],
    percent: String,
    wide: String,
    tall: String,
    timescale_wide: String,
    margins: [String; 4],
    divide: [String; 2],
    height: [String; 2],
    scaling_choice: usize,
    /// What the setup comes to (the result line), kept for the setup it
    /// was worked out for.
    pub summary: Option<(PageSetup, Result<Summary, String>)>,
    /// The timescale date picker, while it is open.
    pub(super) picker: Option<TsPicker>,
    /// A `...` button was clicked: open the picker for that box.
    pub(super) picker_request: Option<Request>,
}

impl SetupDialog {
    pub fn new(setup: PageSetup) -> Self {
        let mut d = Self {
            saved: setup.clone(),
            draft: setup,
            tab: 0,
            section: [0, 0],
            percent: String::new(),
            wide: String::new(),
            tall: String::new(),
            timescale_wide: String::new(),
            margins: Default::default(),
            divide: Default::default(),
            height: Default::default(),
            scaling_choice: 0,
            summary: None,
            picker: None,
            picker_request: None,
        };
        d.load_texts();
        d
    }

    /// Page width and height in inches as drawn: Automatic shows the
    /// orientation the content picked.
    pub fn page_inches(&self) -> (f32, f32) {
        let mut orientation = self.draft.orientation;
        if orientation == Orientation::Automatic {
            if let Some((_, Ok(summary))) = &self.summary {
                orientation = summary.orientation;
            }
        }
        let p = setup::paper(&self.draft.paper);
        match orientation {
            Orientation::Portrait => (p.width, p.height),
            _ => (p.height, p.width),
        }
    }

    /// The setup as it would be saved now; `None` while a number field does
    /// not read.
    pub fn effective(&self) -> Option<PageSetup> {
        let mut draft = self.draft.clone();
        self.read_numbers(&mut draft).ok()?;
        draft.sanitize();
        Some(draft)
    }

    fn load_texts(&mut self) {
        let s = &self.draft;
        let (percent, wide, tall, ts, choice) = match s.scaling {
            Scaling::Adjust { percent } => (percent, 1, 0, 1, 0),
            Scaling::Fit { wide, tall } => (100, wide, tall, 1, 1),
            Scaling::FitTimescale { wide } => (100, 1, 0, wide, 2),
        };
        self.percent = percent.to_string();
        self.wide = wide.to_string();
        self.tall = tall.to_string();
        self.timescale_wide = ts.to_string();
        self.scaling_choice = choice;
        let m = s.margins;
        self.margins = [m.top, m.left, m.right, m.bottom].map(number);
        self.divide = [s.header.count(), s.footer.count()].map(|n| n.to_string());
        self.height = [s.header.height, s.footer.height].map(number);
    }

    /// Does the dialog differ from the saved setup?
    pub fn changed(&self) -> bool {
        if self.draft != self.saved {
            return true;
        }
        let saved = SetupDialog::new(self.saved.clone());
        (
            &self.percent,
            &self.wide,
            &self.tall,
            &self.timescale_wide,
            &self.margins,
            &self.divide,
            &self.height,
            self.scaling_choice,
        ) != (
            &saved.percent,
            &saved.wide,
            &saved.tall,
            &saved.timescale_wide,
            &saved.margins,
            &saved.divide,
            &saved.height,
            saved.scaling_choice,
        )
    }

    /// Reads the number fields into the draft; an error names the field.
    pub fn commit(&mut self) -> Result<(), String> {
        let mut draft = self.draft.clone();
        self.read_numbers(&mut draft)?;
        self.draft = draft;
        self.draft.sanitize();
        self.section = [
            self.section[0].min(self.draft.header.count() - 1),
            self.section[1].min(self.draft.footer.count() - 1),
        ];
        Ok(())
    }

    fn read_numbers(&self, draft: &mut PageSetup) -> Result<(), String> {
        let int = |t: &str, name: &str| -> Result<u32, String> {
            t.trim()
                .parse::<u32>()
                .map_err(|_| format!("{name} must be a whole number"))
        };
        let real = |t: &str, name: &str| -> Result<f32, String> {
            t.trim()
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.)
                .ok_or_else(|| format!("{name} must be a number of inches"))
        };
        // Pages wide stay at 1 unless pages may sit side by side.
        let tiling = draft.allow_tiling;
        let wide = |t: &str, name: &str| -> Result<u32, String> {
            if tiling {
                int(t, name)
            } else {
                Ok(1)
            }
        };
        draft.scaling = match self.scaling_choice {
            0 => Scaling::Adjust {
                percent: int(&self.percent, "Adjust to")?,
            },
            1 => Scaling::Fit {
                wide: wide(&self.wide, "Pages wide")?,
                tall: int(&self.tall, "Pages tall")?,
            },
            _ => Scaling::FitTimescale {
                wide: wide(&self.timescale_wide, "Timescale pages wide")?,
            },
        };
        let [t, l, r, b] = &self.margins;
        draft.margins = setup::Margins {
            top: real(t, "Top")?,
            left: real(l, "Left")?,
            right: real(r, "Right")?,
            bottom: real(b, "Bottom")?,
        };
        for (k, name) in ["Header", "Footer"].into_iter().enumerate() {
            let n = int(&self.divide[k], "Divide Into")? as usize;
            let h = real(&self.height[k], &format!("{name} height"))?;
            let band = if k == 0 {
                &mut draft.header
            } else {
                &mut draft.footer
            };
            if n.clamp(1, setup::MAX_SECTIONS) != band.count() {
                band.divide(n);
            }
            band.height = h;
        }
        Ok(())
    }
}

/// Clicks of the button strip.
#[derive(Default)]
struct Clicks {
    ok: bool,
    cancel: bool,
    apply: bool,
    default: bool,
    help: bool,
}

/// A portrait or landscape page glyph with a letter, drawn for this app.
fn page_glyph(c: &Canvas, x: f32, y: f32, w: f32, h: f32) {
    c.fill(x, y, w, h, Color32::WHITE);
    c.hline(x, y, w, INK);
    c.hline(x, y + h - 1., w, INK);
    c.vline(x, y, h, INK);
    c.vline(x + w - 1., y, h, INK);
    let font = FontId::new(20., egui::FontFamily::Name("Arial Bold".into()));
    c.ui.painter().text(
        c.r(x, y, w, h).center(),
        Align2::CENTER_CENTER,
        "A",
        font,
        INK,
    );
}

impl App {
    pub(crate) fn page_setup_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.print.dialog.take() else {
            return;
        };
        let activities = self.views.main.is_activities();
        let grouped = activities && !self.grid.layout.group_sort.levels.is_empty();
        let network = activities && self.panes.top == panes::TopView::Network;
        // Which of Profile, Spreadsheet and Trace Logic the bottom pane shows.
        let bottom = self.printable_bottom().map(|(kind, _)| kind);
        let legend: Vec<String> = self
            .gantt
            .bars
            .iter()
            .filter(|b| b.display)
            .map(|b| b.name.clone())
            .collect();
        // The result line follows the setup as it would be saved now.
        if let Some(eff) = d.effective() {
            if d.summary.as_ref().is_none_or(|(kept, _)| *kept != eff) {
                d.summary = self.plan_summary(&eff).map(|summary| (eff, summary));
            }
        }
        let summary = d.summary.as_ref().map(|(_, s)| s.clone());
        // The dates Same as screen shows in the timescale boxes.
        let screen_range = match &summary {
            Some(Ok(s)) => s.timescale.filter(|_| self.screen_timescale().is_some()),
            _ => self.screen_timescale(),
        }
        .map(|(a, b)| (self.day_text(a), self.day_text(b)));
        let error = std::cell::RefCell::new(None::<String>);
        let (clicks, closed, actions) = modal(ctx, "page_setup", "Page Setup", CLIENT, |c| {
            let mut k = Clicks::default();
            // Notebook: the tab strip over its page.
            let tabs = TABS.map(|label| kit::VTab {
                label,
                key: "",
                unavailable: None,
            });
            let keys = ["0", "1", "2", "3", "4"];
            let tabs: Vec<kit::VTab> = tabs
                .into_iter()
                .zip(keys)
                .map(|(t, key)| kit::VTab { key, ..t })
                .collect();
            if let Some(i) =
                kit::tabs_spread(c, "page_setup_tab_", (5., 27., 624., 381.), &tabs, d.tab)
            {
                if i != d.tab {
                    if let Err(e) = d.commit() {
                        *error.borrow_mut() = Some(e);
                    } else {
                        d.tab = i;
                        d.load_texts();
                    }
                }
            }
            match d.tab {
                0 => page_tab(c, &mut d, summary.as_ref()),
                1 => margins_tab(c, &mut d),
                2 | 3 => {
                    let which = d.tab - 2;
                    band_tab(c, &mut d, which, network, &legend)
                }
                _ => options_tab(c, &mut d, grouped, bottom, screen_range.as_ref()),
            }
            let changed = d.changed();
            kit::strip(c, CLIENT.x, CLIENT.y);
            let column = kit::button_column(
                c,
                CLIENT.x,
                &[
                    Entry::ok("page_setup_ok"),
                    Entry::cancel("page_setup_cancel"),
                    Entry::apply("page_setup_apply", changed),
                    Entry::Button("page_setup_default", "Default", true, true),
                    Entry::help("page_setup_help"),
                ],
            );
            let clicked = |key: &str| column.clicked.as_deref() == Some(key);
            k.ok = clicked("page_setup_ok");
            k.cancel = clicked("page_setup_cancel");
            k.apply = clicked("page_setup_apply");
            k.default = clicked("page_setup_default");
            k.help = clicked("page_setup_help");
            k
        });
        self.record_dialog_actions(actions);
        self.ts_picker_dialog(ctx, &mut d);
        if let Some(e) = error.into_inner() {
            self.error(e);
        }
        if closed || clicks.cancel {
            return;
        }
        if clicks.default {
            let tab = d.tab;
            d.draft = self.starting_page_setup();
            d.load_texts();
            d.tab = tab;
        }
        if clicks.help {
            self.open_help_for("page_setup");
        }
        if clicks.ok || clicks.apply {
            match d.commit() {
                Ok(()) => match self.save_page_setup(&d.draft) {
                    Ok(()) => {
                        d.saved = d.draft.clone();
                        d.load_texts();
                        self.status = format!("Page setup saved for {}", self.grid.layout.name);
                        if let Some(mut p) = self.print.preview.take() {
                            // Refresh an open preview with the new setup.
                            let setup = self.page_setup();
                            if self.lay_out_preview(&mut p, setup) {
                                self.print.preview = Some(p);
                            }
                        }
                        if clicks.ok {
                            return;
                        }
                    }
                    Err(e) => self.error(e),
                },
                Err(e) => self.error(e),
            }
        }
        self.print.dialog = Some(d);
    }
}

/// Text in the dialog font and a color: the result line and its warnings.
fn note(c: &Canvas, x: f32, y: f32, text: &str, color: Color32) {
    c.ui.painter().text(
        c.r(x, y, 0., 0.).min,
        Align2::LEFT_TOP,
        text,
        kit::body_font(),
        color,
    );
}

/// The next larger paper to offer when the text prints small.
fn larger_paper(name: &str) -> Option<&'static str> {
    Some(match name {
        "Executive" => "Letter",
        "Letter" | "Legal" => "Tabloid",
        "A5" => "A4",
        "A4" | "B4 (JIS)" => "A3",
        "Tabloid" | "A3" => "ANSI C",
        "ANSI C" => "ANSI D",
        "ANSI D" => "ANSI E",
        _ => return None,
    })
}

/// What the setup comes to, in the free lower part of the Scaling group,
/// with a button for each easy way to make small text larger.
fn result_line(c: &mut Canvas, d: &mut SetupDialog, summary: Option<&Result<Summary, String>>) {
    match summary {
        None => {}
        Some(Err(message)) => note(c, 21., 243., message, WARNING),
        Some(Ok(s)) => {
            let color = if s.small() || s.tiled() { WARNING } else { INK };
            note(c, 21., 241., &s.line(), color);
            if let Some(line) = s.tiled_line() {
                note(c, 21., 254., &line, WARNING);
            }
            if s.small() {
                let mut x = 610.;
                if let Some(next) = larger_paper(&d.draft.paper) {
                    x -= 104.;
                    if kit::button(
                        c,
                        "page_setup_larger_paper",
                        x,
                        243.,
                        104.,
                        21.,
                        &format!("Use {next}"),
                        true,
                    ) {
                        d.draft.paper = next.into();
                    }
                    x -= 6.;
                }
                if d.draft.orientation == Orientation::Portrait {
                    x -= 104.;
                    if kit::button(
                        c,
                        "page_setup_use_landscape",
                        x,
                        243.,
                        104.,
                        21.,
                        "Use Landscape",
                        true,
                    ) {
                        d.draft.orientation = Orientation::Landscape;
                    }
                }
            }
        }
    }
}

fn page_tab(c: &mut Canvas, d: &mut SetupDialog, summary: Option<&Result<Summary, String>>) {
    kit::section(c, 15., 37., 601., 85., "Orientation");
    page_glyph(c, 32., 64., 26., 33.);
    if c.radio(
        "page_setup_portrait",
        71.,
        74.,
        94.,
        17.,
        "Portrait",
        d.draft.orientation == Orientation::Portrait,
        true,
    ) {
        d.draft.orientation = Orientation::Portrait;
    }
    page_glyph(c, 176., 69., 34., 25.);
    if c.radio(
        "page_setup_landscape",
        217.,
        74.,
        113.,
        17.,
        "Landscape",
        d.draft.orientation == Orientation::Landscape,
        true,
    ) {
        d.draft.orientation = Orientation::Landscape;
    }
    if c.radio(
        "page_setup_automatic",
        345.,
        74.,
        113.,
        17.,
        "Automatic",
        d.draft.orientation == Orientation::Automatic,
        true,
    ) {
        d.draft.orientation = Orientation::Automatic;
    }
    kit::section(c, 15., 130., 601., 138., "Scaling");
    for (i, (y, text)) in [
        (163., "Adjust to:"),
        (190., "Fit to:"),
        (217., "Fit timescale to:"),
    ]
    .into_iter()
    .enumerate()
    {
        if c.radio(
            &format!("page_setup_scaling_{i}"),
            21.,
            y,
            173.,
            17.,
            text,
            d.scaling_choice == i,
            true,
        ) {
            d.scaling_choice = i;
        }
    }
    let on = |i: usize| d.scaling_choice == i;
    let (a, f, t) = (on(0), on(1), on(2));
    // Pages wide stay at 1 unless pages may sit side by side.
    let tiling = d.draft.allow_tiling;
    let before = tiling;
    let mut allow = tiling;
    c.checkbox(
        "page_setup_tiling",
        462.,
        163.,
        148.,
        17.,
        "Allow pages side by side",
        &mut allow,
        true,
    );
    if allow != before {
        d.draft.allow_tiling = allow;
        if !allow {
            d.wide = "1".into();
            d.timescale_wide = "1".into();
        }
    }
    let (f_wide, t_wide) = (f && allow, t && allow);
    c.spin(
        "page_setup_percent",
        193.,
        160.,
        50.,
        23.,
        &mut d.percent,
        10.,
        10.0..=400.,
        a,
    );
    c.label(249., 160., 23., "% normal size", a);
    c.spin(
        "page_setup_wide",
        193.,
        187.,
        50.,
        23.,
        &mut d.wide,
        1.,
        1.0..=50.,
        f_wide,
    );
    c.label(249., 187., 23., "page(s) wide by", f_wide);
    c.spin(
        "page_setup_tall",
        354.,
        187.,
        50.,
        23.,
        &mut d.tall,
        1.,
        0.0..=200.,
        f,
    );
    c.label(410., 187., 23., "tall", f);
    c.spin(
        "page_setup_timescale_wide",
        193.,
        214.,
        50.,
        23.,
        &mut d.timescale_wide,
        1.,
        1.0..=50.,
        t_wide,
    );
    c.label(249., 214., 23., "page(s) wide", t_wide);
    result_line(c, d, summary);
    kit::section(c, 14., 274., 602., 103., "Paper Size");
    c.label(20., 305., 22., "Paper size:", true);
    let names: Vec<(&str, bool)> = setup::PAPERS.iter().map(|p| (p.name, true)).collect();
    let mut index = setup::PAPERS
        .iter()
        .position(|p| p.name == d.draft.paper)
        .unwrap_or(0);
    if c.combo(
        "page_setup_paper",
        126.,
        305.,
        242.,
        22.,
        &names,
        &mut index,
        true,
    ) {
        d.draft.paper = setup::PAPERS[index].name.into();
    }
    let (w, h) = d.page_inches();
    c.label(20., 333., 23., "Width:", false);
    let mut wt = number(w);
    c.spin(
        "page_setup_width",
        126.,
        333.,
        54.,
        23.,
        &mut wt,
        0.,
        0.0..=100.,
        false,
    );
    c.label(216., 333., 23., "Height:", false);
    let mut ht = number(h);
    c.spin(
        "page_setup_height",
        254.,
        333.,
        54.,
        23.,
        &mut ht,
        0.,
        0.0..=100.,
        false,
    );
}

fn margins_tab(c: &mut Canvas, d: &mut SetupDialog) {
    let fields = [
        ("Top:", 171., 48., 0),
        ("Left:", 18., 144., 1),
        ("Right:", 314., 144., 2),
        ("Bottom:", 171., 257., 3),
    ];
    for (label, x, y, i) in fields {
        let lx = if i == 0 { x + 14. } else { x + 1. };
        c.label(lx, y - 14., 13., label, true);
        c.spin(
            &format!("page_setup_margin_{i}"),
            x,
            y,
            98.,
            23.,
            &mut d.margins[i],
            0.1,
            0.0..=4.,
            true,
        );
    }
    // Page preview: the page in its orientation, margins as double lines.
    let (w, h) = d.page_inches();
    let panel = c.r(127., 78., 180., 160.);
    let s = (170. / w).min(150. / h);
    let page = Rect::from_center_size(panel.center(), Vec2::new(w * s, h * s));
    let p = c.ui.painter();
    p.rect_filled(page.translate(Vec2::splat(2.)), 0., SHADOW);
    p.rect_filled(page, 0., Color32::WHITE);
    p.rect_stroke(page, 0., Stroke::new(1_f32, INK), egui::StrokeKind::Inside);
    let value = |i: usize| d.margins[i].trim().parse::<f32>().unwrap_or(0.) * s;
    let guide = Stroke::new(1_f32, SHADOW);
    let (top, left, right, bottom) = (value(0), value(1), value(2), value(3));
    for dy in [0., 3.] {
        p.hline(page.x_range(), (page.top() + top + dy).round() + 0.5, guide);
        p.hline(
            page.x_range(),
            (page.bottom() - bottom - dy).round() - 0.5,
            guide,
        );
    }
    for dx in [0., 3.] {
        p.vline(
            (page.left() + left + dx).round() + 0.5,
            page.y_range(),
            guide,
        );
        p.vline(
            (page.right() - right - dx).round() - 0.5,
            page.y_range(),
            guide,
        );
    }
}

/// Width of the Divide Into spin on the Header and Footer tabs.
pub(super) const DIVIDE_SPIN_WIDTH: f32 = 54.;

fn band_tab(c: &mut Canvas, d: &mut SetupDialog, which: usize, network: bool, legend: &[String]) {
    let name = if which == 0 { "header" } else { "footer" };
    let key = |k: &str| format!("page_setup_{name}_{k}");
    c.label(33., 42., 23., "Divide Into:", true);
    // Wide enough that the number sits inside its edit, clear of the
    // arrows (the edit is the spin's width less 18).
    c.spin(
        &key("divide"),
        93.,
        42.,
        DIVIDE_SPIN_WIDTH,
        23.,
        &mut d.divide[which],
        1.,
        1.0..=5.,
        true,
    );
    c.label(93. + DIVIDE_SPIN_WIDTH + 8., 42., 23., "Sections", true);
    c.label(228., 42., 22., "Include on:", true);
    let band = if which == 0 {
        &mut d.draft.header
    } else {
        &mut d.draft.footer
    };
    let items: Vec<(&str, bool)> = IncludeOn::ALL.iter().map(|i| (i.label(), true)).collect();
    let mut include = IncludeOn::ALL
        .iter()
        .position(|i| *i == band.include_on)
        .unwrap_or(2);
    if c.combo(
        &key("include"),
        287.,
        42.,
        135.,
        22.,
        &items,
        &mut include,
        true,
    ) {
        band.include_on = IncludeOn::ALL[include];
    }
    c.label(475., 42., 23., "Height:", true);
    c.spin(
        &key("height"),
        514.,
        42.,
        98.,
        23.,
        &mut d.height[which],
        0.05,
        0.1..=4.,
        true,
    );
    // Divide Into applies as soon as the number reads.
    if let Ok(n) = d.divide[which].trim().parse::<usize>() {
        let n = n.clamp(1, setup::MAX_SECTIONS);
        if n != band.count() {
            band.divide(n);
        }
    }
    let selected = d.section[which].min(band.count() - 1);
    kit::section(
        c,
        15.,
        71.,
        604.,
        328.,
        if which == 0 {
            "Define header"
        } else {
            "Define footer"
        },
    );
    let mut dividers = band.dividers;
    c.checkbox(
        &key("dividers"),
        24.,
        99.,
        344.,
        17.,
        "Show Section Divider Lines",
        &mut dividers,
        true,
    );
    band.dividers = dividers;
    // Ruler with a handle on every divider.
    let ruler = c.r(24., 122., 586., 22.);
    c.fill(24., 122., 586., 22., Color32::WHITE);
    let edges = band.edges();
    for (k, edge) in edges.iter().enumerate().take(edges.len() - 1).skip(1) {
        let x = ruler.left() + edge * ruler.width();
        let handle = Rect::from_center_size(Pos2::new(x, ruler.center().y), Vec2::new(7., 15.));
        let id = c.id(&format!("{name}_ruler_{k}"));
        let response = c.ui.interact(handle.expand(2.), id, Sense::drag());
        c.act(&key(&format!("ruler_{k}")), handle);
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                band.move_divider(k - 1, (pos.x - ruler.left()) / ruler.width());
            }
        }
        let p = c.ui.painter();
        p.rect_filled(handle, 0., FACE);
        p.rect_stroke(
            handle,
            0.,
            Stroke::new(1_f32, INK),
            egui::StrokeKind::Inside,
        );
    }
    // Section grid: captions, then the content row.
    let grid_top = 144.;
    let edges = band.edges();
    let mut pick = None;
    for k in 0..band.count() {
        let x0 = (24. + edges[k] * 586.).round();
        let x1 = (24. + edges[k + 1] * 586.).round();
        c.fill(x0, grid_top, x1 - x0, 19., FACE);
        c.vline(x1 - 1., grid_top, 40., INK);
        let caption = format!("Section {} ({}%)", k + 1, band.widths[k].round());
        let font = kit::body_font();
        c.ui.painter().text(
            c.r(x0, grid_top, x1 - x0, 19.).center(),
            Align2::CENTER_CENTER,
            caption,
            font.clone(),
            INK,
        );
        let cell = c.r(x0, grid_top + 20., x1 - x0 - 1., 19.);
        let content = band.sections[k].content;
        let label = content.label(network);
        if k == selected {
            let items: Vec<(&str, bool)> = Content::ALL
                .iter()
                .map(|c| (c.label(network), true))
                .collect();
            let mut i = Content::ALL.iter().position(|x| *x == content).unwrap_or(0);
            if c.combo(
                &key("content"),
                x0,
                grid_top + 20.,
                x1 - x0 - 1.,
                19.,
                &items,
                &mut i,
                true,
            ) {
                band.sections[k].content = Content::ALL[i];
            }
            c.ui.painter().rect_stroke(
                cell,
                0.,
                Stroke::new(2_f32, kit::FOCUS),
                egui::StrokeKind::Inside,
            );
        } else {
            c.ui.painter().rect_filled(cell, 0., kit::SELECTED);
            c.ui.painter().text(
                cell.left_center() + Vec2::new(4., 0.),
                Align2::LEFT_CENTER,
                label,
                font,
                Color32::WHITE,
            );
            if c.hit(&key(&format!("section_{k}")), cell, true).clicked() {
                pick = Some(k);
            }
        }
    }
    c.hline(24., grid_top + 19., 586., INK);
    c.hline(24., grid_top + 39., 586., INK);
    c.vline(24., grid_top, 40., INK);
    c.hline(24., grid_top, 586., INK);
    if let Some(k) = pick {
        d.section[which] = k;
    }
    let section = &mut band.sections[selected];
    // Detail panel for the selected section.
    let title = match section.content {
        Content::Text => "Add Text",
        Content::Legend if network => "Activity Network Legend",
        Content::Legend => "Gantt Chart Legend",
        Content::Revisions => "Revisions",
        Content::None => "(None)",
    };
    c.fill(24., 191., 586., 198., FACE);
    kit::heading(c, 24., 191., 586., title);
    match section.content {
        Content::Text => {
            let response = kit::text_area(
                c,
                &key("text"),
                (24., 212., 586., 144.),
                &mut section.text,
                kit::body_font(),
                INK,
            );
            c.fill(25., 356., 584., 32., FACE);
            c.hline(25., 356., 584., SHADOW);
            match crate::views::sheet::notes_buttons(
                c,
                &format!("page_setup_{name}"),
                28.,
                361.,
                true,
                &section.text,
            ) {
                Some(0) => response.request_focus(),
                Some(2) => c.ui.ctx().copy_text(section.text.clone()),
                _ => {}
            }
            // Insert a field token at the end of the text.
            let mut fields: Vec<(&str, bool)> = vec![("Insert field...", true)];
            fields.extend(setup::FIELDS.iter().map(|(_, caption)| (*caption, true)));
            let mut chosen = 0;
            if c.combo(
                &key("field"),
                302.,
                361.,
                150.,
                21.,
                &fields,
                &mut chosen,
                true,
            ) && chosen > 0
            {
                section
                    .text
                    .push_str(&format!("[{}]", setup::FIELDS[chosen - 1].0));
            }
            let aligns = [("Left", true), ("Center", true), ("Right", true)];
            let mut a = match section.align {
                TextAlign::Left => 0,
                TextAlign::Center => 1,
                TextAlign::Right => 2,
            };
            if c.combo(&key("align"), 462., 361., 100., 21., &aligns, &mut a, true) {
                section.align = [TextAlign::Left, TextAlign::Center, TextAlign::Right][a];
            }
        }
        Content::Legend => {
            let font = kit::body_font();
            let names: Vec<String> = if network {
                vec!["Critical activity".into(), "Non-critical activity".into()]
            } else {
                legend.to_vec()
            };
            for (k, n) in names.iter().enumerate() {
                let (col, row) = (k / 10, k % 10);
                c.ui.painter().text(
                    c.r(34. + col as f32 * 190., 218. + row as f32 * 16., 1., 1.)
                        .min,
                    Align2::LEFT_TOP,
                    n,
                    font.clone(),
                    INK,
                );
            }
        }
        Content::Revisions => {
            c.label(30., 218., 21., "Revision Box Title:", true);
            c.edit(
                &key("revision_title"),
                140.,
                218.,
                250.,
                21.,
                &mut section.revision_title,
                true,
                egui::Align::Min,
            );
            let cols = [
                ("Date", 0.),
                ("Revision", 120.),
                ("Checked", 330.),
                ("Approved", 450.),
            ];
            for (caption, x) in cols {
                c.label(34. + x, 246., 16., caption, true);
            }
            let mut remove = None;
            for (r, rev) in section.revisions.iter_mut().enumerate().take(5) {
                let y = 264. + r as f32 * 22.;
                let fields = [
                    (&mut rev.date, 0., 115.),
                    (&mut rev.revision, 120., 205.),
                    (&mut rev.checked, 330., 115.),
                    (&mut rev.approved, 450., 115.),
                ];
                for (i, (text, x, w)) in fields.into_iter().enumerate() {
                    c.edit(
                        &key(&format!("revision_{r}_{i}")),
                        30. + x,
                        y,
                        w,
                        21.,
                        text,
                        true,
                        egui::Align::Min,
                    );
                }
                if kit::button(
                    c,
                    &key(&format!("revision_delete_{r}")),
                    570.,
                    y,
                    34.,
                    21.,
                    "X",
                    true,
                ) {
                    remove = Some(r);
                }
            }
            if let Some(r) = remove {
                section.revisions.remove(r);
            }
            let room = section.revisions.len() < 5;
            if kit::button(c, &key("revision_add"), 30., 375., 85., 21., "Add", room) {
                section.revisions.push(setup::Revision {
                    date: chrono::Local::now().format("%d-%b-%y").to_string(),
                    revision: String::new(),
                    checked: String::new(),
                    approved: String::new(),
                });
            }
        }
        Content::None => {}
    }
}

/// One timescale box with its `...` button. The box is always typeable:
/// with Same as screen ticked it shows the screen's date, and typing in it
/// or choosing a date with the picker unticks Same as screen.
fn timescale_box(
    c: &mut Canvas,
    d: &mut SetupDialog,
    screen: Option<&(String, String)>,
    field: Field,
    (x, y): (f32, f32),
) {
    let (key, pick_key) = match field {
        Field::Start => ("page_setup_ts_start", "page_setup_ts_start_pick"),
        Field::Finish => ("page_setup_ts_finish", "page_setup_ts_finish_pick"),
    };
    let o = &d.draft.options;
    let stored = match field {
        Field::Start => &o.timescale_start,
        Field::Finish => &o.timescale_finish,
    };
    let shown = match (screen, o.same_as_screen) {
        (Some((start, finish)), true) => match field {
            Field::Start => start,
            Field::Finish => finish,
        },
        _ => stored,
    };
    let before = shown.clone();
    let mut text = before.clone();
    let frame = c.r(x, y, 131., 21.);
    c.edit(key, x, y, 112., 21., &mut text, true, egui::Align::Min);
    let picked = c
        .ellipsis(pick_key, frame, true)
        .on_hover_text(
            "Choose what the date counts from and an offset, or a date. Typed: PS, PF, DD, CD, \
             SW or SM (project start, project finish, data date, current date, start of this \
             week or month) with +/- N D, W, M or Y, or a date",
        )
        .clicked();
    if text != before {
        d.use_expression(field, text, screen.cloned());
    }
    if picked {
        d.picker_request = Some(Request {
            field,
            at: Pos2::new(frame.left(), frame.bottom() + 2.),
            text: before,
        });
    }
}

fn options_tab(
    c: &mut Canvas,
    d: &mut SetupDialog,
    grouped: bool,
    bottom: Option<usize>,
    screen: Option<&(String, String)>,
) {
    c.label(27., 38., 21., "Timescale Start:", true);
    timescale_box(c, d, screen, Field::Start, (206., 38.));
    c.label(27., 70., 21., "Timescale Finish:", true);
    timescale_box(c, d, screen, Field::Finish, (205., 70.));
    let o = &mut d.draft.options;
    c.checkbox(
        "page_setup_same_as_screen",
        350.,
        40.,
        230.,
        17.,
        "Same as screen",
        &mut o.same_as_screen,
        screen.is_some(),
    );
    kit::section(c, 24., 109., 313., 201., "Print");
    let boxes = [
        ("Activity Table", &mut o.activity_table, true),
        ("All Columns", &mut o.all_columns, true),
        ("Grid Lines", &mut o.grid_lines, true),
        ("Gantt Chart", &mut o.gantt_chart, true),
    ];
    for (i, (text, value, enabled)) in boxes.into_iter().enumerate() {
        c.checkbox(
            &format!("page_setup_print_{i}"),
            38.,
            141. + 24. * i as f32,
            283.,
            17.,
            text,
            value,
            enabled,
        );
    }
    // Enabled for the view the bottom pane shows, as in the reference.
    for (i, (text, value)) in [
        ("Profile", &mut o.profile),
        ("Spreadsheet", &mut o.spreadsheet),
        ("Trace Logic", &mut o.trace_logic),
    ]
    .into_iter()
    .enumerate()
    {
        c.checkbox(
            &format!("page_setup_print_{}", i + 4),
            38.,
            237. + 24. * i as f32,
            283.,
            17.,
            text,
            value,
            bottom == Some(i),
        );
    }
    kit::section(c, 24., 325., 313., 65., "Page Settings");
    let mut brk = o.break_every_group && grouped;
    c.checkbox(
        "page_setup_break",
        39.,
        363.,
        282.,
        17.,
        "Break Page Every Group",
        &mut brk,
        grouped,
    );
    if grouped {
        o.break_every_group = brk;
    }
    kit::section(c, 350., 109., 260., 65., "Gantt Chart");
    let gantt = o.gantt_chart;
    c.checkbox(
        "page_setup_relationships",
        364.,
        141.,
        230.,
        17.,
        "Relationship Lines",
        &mut o.relationships,
        gantt,
    );
    kit::section(c, 350., 186., 260., 65., "Bands");
    let mut repeat = o.repeat_band && grouped;
    c.checkbox(
        "page_setup_repeat_band",
        364.,
        218.,
        230.,
        17.,
        "Repeat band header on each page",
        &mut repeat,
        grouped,
    );
    if grouped {
        o.repeat_band = repeat;
    }
}

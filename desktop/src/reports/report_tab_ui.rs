//! The content of a report tab: the header line, the shared document viewer
//! with the Export, Print and Refresh buttons added to its toolbar, and a
//! status line with the page, the zoom and the file last written.
use super::report_tab::{Export, ReportTab, ReportTabId};
use super::rpt::writers::{TextOptions, DELIMITERS, QUALIFIERS};
use crate::chrome::{self, Item};
use crate::dialog_kit as kit;
use crate::print::viewer;
use crate::print::viewer_ui::{self, text_button};
use crate::views::{sheet, MainView};
use crate::App;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2};

/// Height of the header line above the viewer.
const INFO: f32 = 22.;
/// Height of the status line under it.
const STATUS: f32 = 22.;
const LINK: Color32 = Color32::from_rgb(0x00, 0x4E, 0xC8);

#[derive(Default)]
struct Clicks {
    close: bool,
    print: bool,
    refresh: bool,
    /// Options... (a built-in report's tab): its dialog again.
    options: bool,
    copy: bool,
    export: Option<Export>,
    open: Option<bool>,
    save_prefs: bool,
}

/// One of the Export menu's items, as the action name a test clicks.
pub fn export_action(format: Export) -> String {
    format!("report_tab_export_{}", format.key())
}

impl App {
    /// A report tab's view: header line, viewer and status line.
    pub(crate) fn report_tab_view(&mut self, host: &mut egui::Ui, id: ReportTabId) {
        let Some(mut t) = self.reports.tabs.take(id) else {
            return;
        };
        let ctx = host.ctx().clone();
        let view = MainView::Report(id);
        let focused = self.views.groups.current() == view;
        let modal = self.modal_open(&ctx);
        let tabular = t.tabular;
        let progress = matches!(t.source, super::report_tab::Source::Progress(_));
        let mut k = Clicks::default();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(sheet::MARGIN))
            .show_inside(host, |ui| {
                let all = ui.max_rect();
                let p = ui.painter().clone();
                p.rect_filled(all, 0., crate::print::preview::BACKDROP);
                let info = Rect::from_min_size(all.min, Vec2::new(all.width(), INFO));
                let status = Rect::from_min_max(
                    Pos2::new(all.left(), (all.bottom() - STATUS).max(info.bottom())),
                    all.max,
                );
                let area =
                    Rect::from_min_max(info.left_bottom(), Pos2::new(all.right(), status.top()));
                self.paint_tab_info(&p, info, &t);
                // The wheel and the keys go to the viewer that has the
                // pointer or the focus.
                let hovered = ctx
                    .input(|i| i.pointer.hover_pos())
                    .is_some_and(|pos| area.contains(pos));
                let active = !modal && (focused || hovered);
                if active && hovered {
                    viewer::cover(&ctx, area);
                }
                if active
                    && focused
                    && !ctx.wants_keyboard_input()
                    && ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)))
                {
                    k.copy = true;
                }
                if active
                    && focused
                    && t.view.find.open
                    && ctx.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    t.view.find.open = false;
                }
                let page_size = t.page_size();
                let count = t.pages.len();
                let has_pages = count > 0;
                let ReportTab {
                    pages,
                    page,
                    zoom,
                    nav,
                    view: vs,
                    ..
                } = &mut t;
                let mut host_buttons = |bar: &mut egui::Ui, acts: &mut Vec<(String, Rect)>| {
                    let export = bar.add_enabled(
                        has_pages,
                        egui::Button::new(RichText::new("Export  ").font(chrome::ui_font())),
                    );
                    acts.push(("report_tab_export".into(), export.rect));
                    paint_drop_arrow(bar.painter(), export.rect, has_pages);
                    egui::Popup::menu(&export)
                        .style(chrome::popup_menu_style)
                        .show(|ui| {
                            for format in Export::ALL {
                                let enabled = format != Export::Csv || tabular;
                                let item = Item::new(format.caption()).enabled(enabled);
                                if self.item(ui, &export_action(format), item) {
                                    k.export = Some(format);
                                }
                            }
                        });
                    k.print |=
                        text_button(bar, acts, "report_tab_print", "Print", has_pages, false);
                    k.refresh |=
                        text_button(bar, acts, "report_tab_refresh", "Refresh", true, false);
                    if progress {
                        k.options |=
                            text_button(bar, acts, "report_tab_options", "Options...", true, false);
                    }
                };
                let frame = viewer_ui::begin(
                    ui,
                    viewer_ui::Chrome {
                        names: viewer_ui::Names {
                            salt: id.0,
                            ..viewer_ui::TAB
                        },
                        area,
                        page_size,
                        count,
                        across: 1,
                        page: &mut *page,
                        zoom: &mut *zoom,
                        nav: &mut *nav,
                        view: &mut *vs,
                        active,
                    },
                    &mut host_buttons,
                );
                k.close = frame.close;
                k.save_prefs = frame.save_prefs;
                for (name, rect) in &frame.actions {
                    self.action_rects.insert(
                        name.clone(),
                        [rect.left(), rect.top(), rect.right(), rect.bottom()],
                    );
                }
                for &(i, origin) in &frame.cells {
                    crate::print::preview::paint_page(
                        &frame.painter,
                        &pages[i],
                        origin,
                        frame.zoom,
                    );
                    viewer_ui::paint_matches(&frame.painter, &vs.find, i, origin, frame.zoom);
                }
                for &(i, rect) in &frame.thumbs {
                    viewer_ui::paint_thumb(&frame.thumb_painter, rect, pages.get(i));
                }
                // Every page is built, so the search is done at once.
                while let Some(i) = vs.find.next_scan(count) {
                    vs.find.scan(i, &pages[i]);
                }
                let zoom_now = frame.zoom;
                self.paint_tab_status(ui, status, &t, zoom_now, &mut k);
            });
        if k.save_prefs {
            self.save_viewer_prefs(t.zoom, &t.view);
            t.view.take_dirty();
        }
        self.reports.tabs.put(t);
        if k.close {
            self.request_close(view);
        }
        if k.refresh {
            self.refresh_report_tab(id);
        }
        if k.options {
            self.open_progress_options(id);
        }
        if k.print {
            self.print_report_tab(id);
        }
        if k.copy {
            self.copy_report_tab(&ctx, id);
        }
        if let Some(format) = k.export {
            self.export_report_tab(id, format);
        }
        if let Some(folder) = k.open {
            self.open_saved_report(id, folder);
        }
    }

    /// The header line: the report, its project, the data date, when it ran
    /// and how many pages it has.
    fn paint_tab_info(&self, p: &egui::Painter, rect: Rect, t: &ReportTab) {
        p.rect_filled(rect, 0., chrome::pal::FACE);
        chrome::hline(
            p,
            rect.left(),
            rect.right(),
            rect.bottom() - 1.,
            chrome::pal::SHADOW,
        );
        let p = p.with_clip_rect(rect);
        let mut x = rect.left() + 8.;
        let y = rect.center().y;
        let name = p.layout_no_wrap(t.name.clone(), chrome::bold_font(), chrome::INK);
        let w = name.size().x;
        p.galley(Pos2::new(x, y - name.size().y / 2.), name, chrome::INK);
        x += w + 16.;
        let pages = t.pages.len();
        for text in [
            format!("Project: {}", t.project),
            format!("Data date: {}", t.data_date),
            format!("Run: {}", t.ran),
            format!("{pages} page{}", if pages == 1 { "" } else { "s" }),
        ] {
            let galley = p.layout_no_wrap(text, chrome::ui_font(), chrome::INK);
            let w = galley.size().x;
            p.galley(Pos2::new(x, y - galley.size().y / 2.), galley, chrome::INK);
            x += w + 16.;
        }
    }

    /// The status line: page and zoom on the left, the file last written
    /// (with Open and Open Folder) or the last note on the right.
    fn paint_tab_status(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        t: &ReportTab,
        zoom: f32,
        k: &mut Clicks,
    ) {
        let p = ui.painter().clone();
        p.rect_filled(rect, 0., chrome::pal::FACE);
        chrome::hline(
            &p,
            rect.left(),
            rect.right(),
            rect.top(),
            chrome::pal::SHADOW,
        );
        let mut left = format!(
            "Page {} of {}    Zoom {:.0}%",
            t.page + 1,
            t.pages.len().max(1),
            zoom * 100.
        );
        if !t.warnings.is_empty() {
            left.push_str(&format!(
                "    {} warning(s): {}",
                t.warnings.len(),
                t.warnings.join("; ")
            ));
        }
        let y = rect.center().y;
        let galley = p.layout_no_wrap(left, chrome::ui_font(), chrome::INK);
        let left_end = rect.left() + 8. + galley.size().x;
        p.with_clip_rect(rect).galley(
            Pos2::new(rect.left() + 8., y - galley.size().y / 2.),
            galley,
            chrome::INK,
        );
        let mut right = rect.right() - 8.;
        let id = t.id.0;
        if let Some(path) = &t.saved {
            for (label, name, folder) in [
                ("Open Folder", "report_tab_open_folder", true),
                ("Open", "report_tab_open", false),
            ] {
                let galley = p.layout_no_wrap(label.into(), chrome::ui_font(), LINK);
                let size = galley.size();
                let r = Rect::from_min_size(Pos2::new(right - size.x, y - size.y / 2.), size);
                let response = ui.interact(r.expand(2.), egui::Id::new((name, id)), Sense::click());
                self.action(name, &response);
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                p.galley(r.min, galley, LINK);
                p.hline(r.x_range(), r.bottom() - 1., Stroke::new(1_f32, LINK));
                if response.clicked() {
                    k.open = Some(folder);
                }
                right = r.left() - 14.;
            }
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into(),
            );
            self.paint_note(&p, rect, y, left_end, right, &format!("Saved {name}"));
        } else if let Some(note) = &t.note {
            self.paint_note(&p, rect, y, left_end, right, note);
        }
    }

    /// `text`, right aligned to `right`, if it fits after `left`.
    fn paint_note(&self, p: &egui::Painter, rect: Rect, y: f32, left: f32, right: f32, text: &str) {
        let galley = p.layout_no_wrap(text.into(), chrome::ui_font(), chrome::INK);
        let w = galley.size().x;
        if right - w < left + 24. {
            return;
        }
        p.with_clip_rect(rect).galley(
            Pos2::new(right - w, y - galley.size().y / 2.),
            galley,
            chrome::INK,
        );
    }

    /// The Text export's delimiter and qualifier, asked before Save As.
    pub(crate) fn report_text_export_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.text_export.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        const W: f32 = 390.;
        const H: f32 = 124.;
        let delimiters: Vec<(&str, bool)> = DELIMITERS.iter().map(|(c, _)| (*c, true)).collect();
        let qualifiers: Vec<(&str, bool)> = QUALIFIERS.iter().map(|(c, _)| (*c, true)).collect();
        let ((ok, cancel), closed, actions) = kit::window(
            ctx,
            "report_text_export",
            "Export Text",
            Vec2::new(W, H),
            |c| {
                c.fill(0., 0., W, H, kit::FACE);
                kit::text(c, 14., 14., "Field Delimiter", true);
                c.combo(
                    "report_text_delimiter",
                    14.,
                    30.,
                    150.,
                    21.,
                    &delimiters,
                    &mut d.delimiter,
                    true,
                );
                kit::text(c, 14., 62., "Text Qualifier", true);
                c.combo(
                    "report_text_qualifier",
                    14.,
                    78.,
                    150.,
                    21.,
                    &qualifiers,
                    &mut d.qualifier,
                    true,
                );
                kit::strip(c, W, H);
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::ok("report_text_ok"),
                        kit::Entry::cancel("report_text_cancel"),
                    ],
                );
                let clicked = col.clicked.as_deref();
                (
                    clicked == Some("report_text_ok"),
                    clicked == Some("report_text_cancel"),
                )
            },
        );
        self.record_dialog_actions(actions);
        if cancel || closed {
            return;
        }
        if ok {
            let delimiter = d.delimiter.min(DELIMITERS.len() - 1);
            let qualifier = d.qualifier.min(QUALIFIERS.len() - 1);
            self.reports.text_choice = (delimiter, qualifier);
            let text = TextOptions {
                delimiter: DELIMITERS[delimiter].1.to_string(),
                qualifier: QUALIFIERS[qualifier].1.to_string(),
            };
            self.save_report_tab(d.id, Export::Text, &text);
            return;
        }
        self.reports.text_export = Some(d);
    }
}

/// The small arrow at the right of the Export button.
fn paint_drop_arrow(p: &egui::Painter, rect: Rect, enabled: bool) {
    let ink = if enabled {
        chrome::INK
    } else {
        Color32::from_gray(150)
    };
    let c = Pos2::new(rect.right() - 9., rect.center().y + 0.5);
    p.add(Shape::convex_polygon(
        vec![
            c + Vec2::new(-3.5, -2.),
            c + Vec2::new(3.5, -2.),
            c + Vec2::new(0., 2.),
        ],
        ink,
        Stroke::NONE,
    ));
}

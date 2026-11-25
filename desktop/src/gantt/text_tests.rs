//! Text attachments: font and color from the shared font picker, anchors on
//! activity and grouping band rows, and click to place. Real frames and
//! clicks on the rendered controls through the QA driver.
use super::settings::{Rgb, TextFont};
use super::*;
use crate::chrome::Face;
use crate::qa_sweep::Driver;

fn scheduled() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn driver(name: &str) -> Driver {
    let mut d = Driver::new(&format!("l1-{name}"), scheduled());
    d.home();
    d
}

/// A point on the chart half of the row anchored at `anchor`.
fn chart_point(d: &Driver, anchor: &RowAnchor) -> Pos2 {
    let row = d
        .app
        .row_geometry
        .iter()
        .find(|(a, _)| a == anchor)
        .unwrap_or_else(|| panic!("row {anchor:?} is not drawn"))
        .1;
    Pos2::new(d.app.gantt_left + 8., row.center().y)
}

fn first_band(d: &Driver) -> (String, Rect) {
    d.app
        .row_geometry
        .iter()
        .find_map(|(a, r)| match a {
            RowAnchor::Band(key) => Some((key.clone(), *r)),
            RowAnchor::Activity(_) => None,
        })
        .expect("the starter project is grouped by WBS")
}

fn drag(d: &mut Driver, from: Pos2, to: Pos2) {
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    d.frame(vec![egui::Event::PointerMoved(from)]);
    d.frame(vec![button(from, true)]);
    for step in 1..=8 {
        let pos = from + (to - from) * (step as f32 / 8.);
        d.frame(vec![egui::Event::PointerMoved(pos)]);
    }
    d.frame(vec![button(to, false)]);
    d.frames(2);
}

#[test]
fn text_dialog_picks_font_and_color_and_the_layout_keeps_them() {
    let mut d = driver("font");
    d.app.begin_text_placement();
    d.frames(2);
    let pos = chart_point(&d, &RowAnchor::Activity(1020));
    d.click_at(pos);
    let draft = d
        .app
        .gantt_ui
        .text
        .as_ref()
        .expect("a click places the text");
    assert_eq!((draft.task_id, draft.band.clone()), (1020, None));
    assert_eq!(draft.font, TextFont::default());
    d.click("text_attachment_text");
    d.type_text("Mobilize crane");
    // Font...: the shared font picker, with the dialog's sample as its start.
    d.click("text_attachment_font");
    assert!(d.has("font_picker_ok"), "the font picker opens");
    d.click("font_picker_face_row_1");
    d.click("font_picker_style_row_3");
    d.click("font_picker_size_row_6");
    d.click("font_picker_ok");
    assert!(!d.has("font_picker_ok"), "OK closes the picker");
    let font = d.app.gantt_ui.text.as_ref().unwrap().font;
    assert_eq!(font.face, Face::SegoeUi);
    assert!(font.bold && font.italic);
    assert_eq!(font.points, 12.);
    // Color: the swatch drops down the basic colors; the first is pale red.
    let swatch = d.rect("text_attachment_color");
    d.click("text_attachment_color");
    d.click_at(Pos2::new(swatch.left() + 12., swatch.bottom() + 12.));
    let color = d.app.gantt_ui.text.as_ref().unwrap().font.color;
    assert_ne!(color, Rgb::BLACK, "a basic color was picked");
    d.click("text_attachment_ok");
    assert!(d.app.gantt_ui.text.is_none());
    assert_eq!(d.app.gantt.texts.len(), 1);
    let kept = d.app.gantt.texts[0].clone();
    assert_eq!(kept.text, "Mobilize crane");
    assert_eq!(kept.font, TextFont { color, ..font });
    // Drawn in that font and color on the chart.
    let output = d.frame(vec![]);
    let drawn: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "Mobilize crane" => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(drawn.len(), 1, "drawn once");
    let section = &drawn[0].galley.job.sections[0].format;
    assert!(section.italics);
    assert_eq!(section.color, color.color());
    assert_eq!(section.font_id, font.face.font(12., true));
    // Saved with a layout, cleared, reopened.
    d.app.save_layout_as("L1 text font").unwrap();
    let mut cleared = d.app.gantt.clone();
    cleared.texts.clear();
    d.app.apply_gantt(cleared).unwrap();
    d.app.open_layout("L1 text font", true).unwrap();
    assert_eq!(d.app.gantt.texts, vec![kept.clone()]);
    // Printed.
    let printed = d
        .app
        .print_with(&crate::print::PageSetup::default())
        .unwrap();
    assert!(printed.pages[0].texts().contains(&"Mobilize crane"));
    // Escape closes the picker first, then the dialog; nothing changes.
    d.app.edit_text_attachment(0);
    d.click("text_attachment_font");
    assert!(d.has("font_picker_ok"));
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.has("font_picker_ok"), "Escape closes the picker");
    assert!(d.app.gantt_ui.text.is_some(), "the text dialog stays");
    d.key(Key::Escape);
    assert!(d.app.gantt_ui.text.is_none());
    assert_eq!(d.app.gantt.texts, vec![kept]);
    // The picker's Cancel keeps the dialog's font.
    d.app.edit_text_attachment(0);
    d.click("text_attachment_font");
    d.click("font_picker_size_row_1");
    d.click("font_picker_cancel");
    assert_eq!(d.app.gantt_ui.text.as_ref().unwrap().font.points, 12.);
    d.click("text_attachment_cancel");
    assert_eq!(d.app.gantt.texts.len(), 1);
}

#[test]
fn text_attachments_anchor_to_band_rows_and_follow_a_drag_between_row_kinds() {
    let mut d = driver("band");
    let (key, band_row) = first_band(&d);
    d.app.begin_text_placement();
    d.frames(2);
    d.click_at(Pos2::new(d.app.gantt_left + 8., band_row.center().y));
    let draft = d
        .app
        .gantt_ui
        .text
        .as_ref()
        .expect("a click places the text");
    assert_eq!(draft.band.as_deref(), Some(key.as_str()));
    d.click("text_attachment_text");
    d.type_text("Phase note");
    d.click("text_attachment_ok");
    assert_eq!(d.app.gantt.texts[0].band.as_deref(), Some(key.as_str()));
    d.frames(2);
    let drawn = d.rect("text_attachment_0");
    assert_eq!(drawn.top(), band_row.top() + 1., "hangs from the band row");
    // Kept in the layout and a new session.
    d.app.save_layout_as("L1 band text").unwrap();
    let saved = d.app.gantt.texts.clone();
    let mut cleared = d.app.gantt.clone();
    cleared.texts.clear();
    d.app.apply_gantt(cleared).unwrap();
    d.app.open_layout("L1 band text", true).unwrap();
    assert_eq!(d.app.gantt.texts, saved);
    // Printed on the band's row.
    let printed = d
        .app
        .print_with(&crate::print::PageSetup::default())
        .unwrap();
    assert!(printed.pages[0].texts().contains(&"Phase note"));
    // Dragging it onto an activity row re-anchors it there.
    d.frames(2);
    let target = chart_point(&d, &RowAnchor::Activity(1030));
    let from = d.rect("text_attachment_0").center();
    drag(&mut d, from, Pos2::new(from.x, target.y));
    let t = &d.app.gantt.texts[0];
    assert_eq!((t.task_id, t.band.clone()), (1030, None));
    // And back onto the band.
    d.frames(2);
    let from = d.rect("text_attachment_0").center();
    drag(&mut d, from, Pos2::new(from.x, band_row.center().y));
    assert_eq!(d.app.gantt.texts[0].band.as_deref(), Some(key.as_str()));
    // A band the layout no longer has draws nothing and keeps the text.
    d.app.gantt.texts[0].band = Some("gone".into());
    let output = d.frame(vec![]);
    assert!(!output.shapes.iter().any(|s| matches!(
        &s.shape,
        egui::Shape::Text(t) if t.galley.text() == "Phase note"
    )));
    assert_eq!(d.app.gantt.texts.len(), 1);
}

#[test]
fn click_to_place_waits_for_the_chart_and_escape_cancels() {
    let mut d = driver("place");
    // The View menu's Attachments > Text starts the placing.
    d.click("menu_bar_view");
    assert!(d.hover_text("Attachments"));
    d.click("view_attachments_text");
    assert!(d.app.gantt_ui.placing_text.is_some());
    assert!(d.app.gantt_ui.text.is_none(), "no dialog until the click");
    assert!(d.app.status.contains("Click the chart"));
    // A click on the table places nothing.
    d.click_at(Pos2::new(d.app.gantt_left - 60., 400.));
    assert!(d.app.gantt_ui.text.is_none());
    assert!(d.app.gantt_ui.placing_text.is_some());
    // Escape cancels.
    d.key(Key::Escape);
    assert!(d.app.gantt_ui.placing_text.is_none() && d.app.gantt_ui.text.is_none());
    // Again from the layout menu, then the chart.
    d.click("layout_button");
    assert!(d.hover_text("Attachments"));
    d.click("layout_attachments_text");
    assert!(d.app.gantt_ui.placing_text.is_some());
    let pos = chart_point(&d, &RowAnchor::Activity(1010));
    d.click_at(pos);
    let draft = d.app.gantt_ui.text.as_ref().unwrap();
    assert_eq!(draft.task_id, 1010);
    assert_eq!(draft.date % 60, 0);
    d.click("text_attachment_cancel");
    assert!(d.app.gantt.texts.is_empty());
}

#[test]
fn a_click_on_the_bottom_gantt_places_text_on_its_own_timescale() {
    let mut d = driver("bottom-place");
    d.app.set_bottom(Some(crate::panes::BottomView::Gantt));
    d.frames(3);
    // Give the bottom chart its own scroll position, a week to the right.
    let top_origin = d.app.timeline_origin;
    let shifted = d.app.panes.grid.scale.timeline_origin.unwrap() + 7 * DAY;
    d.app.panes.grid.scale.timeline_origin = Some(shifted);
    d.frames(2);
    let top_rows = d.app.row_geometry.len();
    assert!(top_rows > 0);
    // The bottom chart's left edge: where its header's chart part starts.
    let bottom_left = d.app.panes.grid.gantt_left;
    let bottom_y = d.app.panes.grid.header.0 + d.app.panes.grid.header.1 + 40.;
    d.app.begin_text_placement();
    d.frames(2);
    let pos = Pos2::new(bottom_left + 30., bottom_y);
    d.click_at(pos);
    let draft = d.app.gantt_ui.text.as_ref().expect("the click places it");
    // 30 px into the bottom chart is a date on the bottom timescale, a
    // week after the same x on the top chart.
    let ppd = d.app.panes.grid.scale.pixels_per_day;
    let expected = shifted as f64 + 30. / ppd as f64 * DAY as f64;
    assert!(
        (draft.date as f64 - expected).abs() < DAY as f64 / 2.,
        "{} vs {expected}",
        draft.date
    );
    assert_eq!(d.app.timeline_origin, top_origin, "the top is unchanged");
    d.click("text_attachment_cancel");
}

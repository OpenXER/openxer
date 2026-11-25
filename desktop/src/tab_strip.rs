//! The flat tab strip over a framed page, shared by every details pane: the
//! Activities page details and the bottom details of Projects, WBS,
//! Resources, Assignments and the record views, and the horizontal tabs of
//! the dialogs (`dialog_kit::tabs`). The Activities page is the reference
//! look; every other view and dialog paints through the same function.
//!
//! The strip sits directly above `page`: a `#646464` top line and a
//! full-height divider on both sides of every tab, the open tab with a white
//! top/left edge, a shadow right edge and no bottom line, hover fill on the
//! others, left-aligned captions sized to their text, and scroll arrows at
//! the right when the tabs do not fit. Dialogs whose tabs share the panel's
//! width (`show_spread`, `dialog_kit::tabs_spread`) pad them to equal widths
//! across the page with centred captions. The page frame is the outer `#646464`
//! line with white top/left and shadow bottom/right inside it.
use crate::chrome::{self, INK};
use crate::details::form;
use eframe::egui::{self, Pos2, Rect, Sense, Vec2};
use std::hash::Hash;

/// What the strip did this frame.
pub struct Strip {
    /// The whole strip above the page (for a context menu on its background).
    pub strip: egui::Response,
    /// Each tab in view: its index into the captions and its response.
    pub tabs: Vec<(usize, egui::Response)>,
    /// The tab the user picked this frame, by a click or by scrolling the
    /// open tab out of view.
    pub clicked: Option<usize>,
}

/// The strip rectangle over `page`.
pub fn strip_rect(page: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(page.left(), page.top() - form::TAB_H),
        Pos2::new(page.right(), page.top()),
    )
}

/// Paints the tabs over `page` and the page frame, and hit-tests them.
/// `first` is the first tab in view when the tabs overflow; it is kept
/// so the selected tab stays in view and moved by the scroll arrows.
pub fn show(
    ui: &mut egui::Ui,
    salt: impl Hash + Copy,
    page: Rect,
    captions: &[&str],
    selected: usize,
    first: &mut usize,
) -> Strip {
    show_spread(ui, salt, page, captions, selected, first, false)
}

/// Tab widths that share the page's width: equal when every caption fits
/// an equal share, else each caption's width plus an equal part of the
/// room left. `None` when the captions do not fit (the strip scrolls).
pub fn spread_widths(natural: &[f32], page_width: f32) -> Option<Vec<f32>> {
    // The last tab's right divider sits on the page frame's right line.
    let room = page_width - 1.;
    let total: f32 = natural.iter().sum();
    if natural.is_empty() || total > room {
        return None;
    }
    let n = natural.len() as f32;
    let share = (room / n).floor();
    let mut widths: Vec<f32> = if natural.iter().all(|w| *w <= share) {
        vec![share; natural.len()]
    } else {
        let extra = ((room - total) / n).floor();
        natural.iter().map(|w| w + extra).collect()
    };
    // The rounding remainder goes to the last tab.
    let used: f32 = widths.iter().sum();
    if let Some(last) = widths.last_mut() {
        *last += room - used;
    }
    Some(widths)
}

/// [`show`], with `spread` set for a dialog whose tabs share the panel's
/// width with their captions centred (Bars, Bar Chart Options, Page
/// Setup). Tabs that do not fit fall back to their own widths and scroll.
pub fn show_spread(
    ui: &mut egui::Ui,
    salt: impl Hash + Copy,
    page: Rect,
    captions: &[&str],
    selected: usize,
    first: &mut usize,
    spread: bool,
) -> Strip {
    let natural: Vec<f32> = captions.iter().map(|c| form::tab_width(ui, c)).collect();
    let spread_to = spread
        .then(|| spread_widths(&natural, page.width()))
        .flatten();
    let centred = spread_to.is_some();
    let widths = spread_to.unwrap_or_else(|| natural.clone());
    let strip = strip_rect(page);
    let top = strip.top();
    let strip_response = ui.interact(strip, ui.id().with((salt, "strip")), Sense::click());
    let total: f32 = widths.iter().sum();
    let overflow = total + 1. > strip.width() && !captions.is_empty();
    let arrows = if overflow { 34. } else { 0. };
    if overflow {
        *first = (*first).min(captions.len() - 1);
        // Keep the selected tab in view.
        if selected < *first {
            *first = selected;
        }
        while *first < selected
            && widths[*first..=selected.min(captions.len() - 1)]
                .iter()
                .sum::<f32>()
                > strip.width() - arrows - 1.
        {
            *first += 1;
        }
    } else {
        *first = 0;
    }
    let p = ui
        .painter()
        .with_clip_rect(Rect::from_min_max(strip.min, page.max).intersect(ui.clip_rect()));
    // The page.
    form::fill(&p, page, form::FACE);
    let (l, t, r, b) = (page.left(), page.top(), page.right(), page.bottom());
    chrome::hline(&p, l, r, t, form::FRAME);
    chrome::vline(&p, l, t, b, form::FRAME);
    chrome::vline(&p, r - 1., t, b, form::FRAME);
    chrome::hline(&p, l, r, b - 1., form::FRAME);
    chrome::hline(&p, l + 1., r - 1., t + 1., form::WHITE);
    chrome::vline(&p, l + 1., t + 1., b - 1., form::WHITE);
    chrome::vline(&p, r - 2., t + 1., b - 1., form::SHADOW);
    chrome::hline(&p, l + 1., r - 1., b - 2., form::SHADOW);
    // The tabs, left to right from the page's left edge.
    let mut x = l;
    let mut clicked = None;
    let mut tabs = Vec::new();
    for (index, caption) in captions.iter().enumerate() {
        if index < *first {
            continue;
        }
        let width = widths[index];
        if x + width > strip.right() - arrows - 1. && index != *first {
            break;
        }
        let hit = form::rect(x, top, width + 1., form::TAB_H);
        let response = ui.interact(hit, ui.id().with((salt, "tab", index)), Sense::click());
        let (d0, d1) = (x, x + width);
        form::fill(
            &p,
            form::rect(d0 + 1., top + 1., width - 1., form::TAB_H - 1.),
            form::FACE,
        );
        chrome::hline(&p, d0, d1 + 1., top, form::FRAME);
        chrome::vline(&p, d0, top, t, form::FRAME);
        chrome::vline(&p, d1, top, t, form::FRAME);
        if index == selected {
            // Open the page under the tab.
            form::fill(&p, form::rect(d0 + 1., t, width - 1., 2.), form::FACE);
            chrome::vline(&p, d0 + 1., top + 1., t + 2., form::WHITE);
            chrome::hline(&p, d0 + 1., d1 - 1., top + 1., form::WHITE);
            chrome::vline(&p, d1 - 1., top + 1., t + 2., form::SHADOW);
        } else if response.hovered() {
            form::fill(
                &p,
                form::rect(d0 + 1., top + 1., width - 1., form::TAB_H - 1.),
                chrome::pal::FOCUS_INNER,
            );
        }
        // A spread tab centres its caption; the caption's own tab width
        // keeps it 5 px in from the left divider.
        let inset = if centred {
            5. + ((width - natural[index]) / 2.).floor()
        } else {
            5.
        };
        form::text(&p, d0 + inset, top + 2., form::TAB_H, caption, INK);
        if response.clicked() {
            clicked = Some(index);
        }
        tabs.push((index, response));
        x = d1;
    }
    if overflow {
        let left = form::rect(strip.right() - arrows, top + 2., 17., form::TAB_H - 4.);
        let right = left.translate(Vec2::new(17., 0.));
        for (rect, key, delta) in [(left, "left", -1isize), (right, "right", 1)] {
            let response = ui.interact(
                rect,
                ui.id().with((salt, "tab_scroll", key)),
                Sense::click(),
            );
            form::fill(&p, rect, form::FACE);
            form::raised(&p, rect);
            chrome::chevron(
                &p,
                Pos2::new(rect.center().x - 1., rect.center().y - 2.),
                false,
                INK,
            );
            if delta > 0 {
                // Mirror the left chevron.
                form::fill(&p, rect.shrink(2.), form::FACE);
                for step in 0..3 {
                    let s = step as f32;
                    chrome::vline(
                        &p,
                        rect.center().x - 1. + s,
                        rect.center().y - 2. + s,
                        rect.center().y + 3. - s,
                        INK,
                    );
                }
            }
            if response.clicked() {
                *first = (*first as isize + delta).clamp(0, captions.len() as isize - 1) as usize;
                // Scrolling moves the strip only; select the first tab in
                // view like a tab control.
                if delta > 0 && selected < *first {
                    clicked = Some(*first);
                }
            }
        }
    }
    Strip {
        strip: strip_response,
        tabs,
        clicked,
    }
}

#[cfg(test)]
mod tests {
    use super::spread_widths;

    #[test]
    fn spread_tabs_share_the_page_width() {
        // Page Setup: five short captions on a 624 px page, about 124 each.
        let w = spread_widths(&[40., 50., 48., 46., 52.], 624.).unwrap();
        assert_eq!(w.iter().sum::<f32>(), 623.);
        assert!(w[..4].iter().all(|x| *x == 124.), "{w:?}");
        // Bars: three tabs across 643 px.
        let w = spread_widths(&[62., 70., 66.], 643.).unwrap();
        assert_eq!(w, [214., 214., 214.]);
        // A caption wider than an equal share keeps its width plus the
        // same part of the room left as the others.
        let w = spread_widths(&[200., 40.], 301.).unwrap();
        assert_eq!(w, [230., 70.]);
        // Captions that do not fit are not spread (the strip scrolls).
        assert_eq!(spread_widths(&[300., 300.], 500.), None);
        assert_eq!(spread_widths(&[], 500.), None);
    }
}

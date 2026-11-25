//! The preview builds its pages lazily and reuses the painted canvas; these
//! tests hold the result to the one-pass construction the first version
//! used, byte for byte, for several setups, after edits and after undo.
use super::ops::Page;
use super::setup::{Content, IncludeOn, Orientation, PageSetup, Scaling};
use super::tests::{app, resource_app};
use super::*;
use crate::qa_sweep::Driver;
use panes::BottomView as B;

/// Setups that exercise the page grid, the bands and the options.
fn setups() -> Vec<(&'static str, PageSetup)> {
    let mut texts = PageSetup::default();
    texts.header.sections[0].text = "Plan [project_name] on [print_date] at [print_time]".into();
    texts.footer.sections[2].text = "[page_number] of [total_pages] [layout_name]".into();
    let mut margins = PageSetup::default();
    margins.margins.left = 1.25;
    margins.margins.top = 0.75;
    margins.header.height = 0.6;
    let mut portrait = PageSetup {
        orientation: Orientation::Portrait,
        paper: "A4".into(),
        ..PageSetup::default()
    };
    portrait.scaling = Scaling::Adjust { percent: 130 };
    portrait.allow_tiling = true;
    let fit = PageSetup {
        scaling: Scaling::Fit { wide: 2, tall: 2 },
        allow_tiling: true,
        ..PageSetup::default()
    };
    let timescale = PageSetup {
        scaling: Scaling::FitTimescale { wide: 3 },
        allow_tiling: true,
        ..PageSetup::default()
    };
    let mut options = PageSetup::default();
    options.options.grid_lines = false;
    options.options.relationships = false;
    options.options.all_columns = false;
    let mut bands = PageSetup::default();
    bands.header.include_on = IncludeOn::FirstPage;
    bands.footer.include_on = IncludeOn::LastPage;
    bands.footer.sections[0].content = Content::Legend;
    bands.scaling = Scaling::Adjust { percent: 200 };
    bands.allow_tiling = true;
    vec![
        ("default", PageSetup::default()),
        ("header and footer text", texts),
        ("margins", margins),
        ("portrait 130%", portrait),
        ("fit 2 by 2", fit),
        ("fit timescale", timescale),
        ("options off", options),
        ("bands on some pages", bands),
    ]
}

/// The job's pages built one at a time, last first, against the one-pass
/// reference.
fn assert_lazy_equals_reference(app: &mut App, s: &PageSetup, context: &str) {
    let ctx = app.print_context();
    let job = app
        .compose_job(&ctx, s, true)
        .unwrap()
        .expect("painted job");
    let reference = app.reference_pages(&ctx, &job);
    assert_eq!(reference.len(), job.count(), "{context}: page count");
    for k in (0..job.count()).rev() {
        assert_eq!(
            app.job_page(&ctx, &job, k),
            reference[k],
            "{context}: page {k}"
        );
    }
    let eager = app.print_with(s).unwrap();
    assert_eq!(eager.pages, reference, "{context}: print_with");
}

#[test]
fn lazily_built_pages_equal_the_one_pass_pages() {
    let (path, _ctx, mut app) = resource_app("lazy-pages");
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    app.selected = Some(i);
    app.panes.usage.resource = Some(7);
    for (view, name) in [
        (B::Details, "details"),
        (B::ActivityUsageProfile, "bottom profile"),
        (B::TraceLogic, "Trace Logic"),
    ] {
        app.set_bottom(Some(view));
        for (label, s) in setups() {
            assert_lazy_equals_reference(&mut app, &s, &format!("{name}, {label}"));
        }
    }
    assert!(app.error.is_none(), "{:?}", app.error);
    let _ = std::fs::remove_file(path);
}

#[test]
fn dashed_lines_kept_whole_print_the_same_ops_as_one_mesh_per_dash() {
    let (path, _ctx, mut app) = resource_app("dashes");
    app.gantt.options.data_date.style = gantt::settings::LineStyle::DashDot;
    app.gantt.options.relationship_colors.non_driving_dotted = true;
    let s = PageSetup::default();
    let kept = app.print_with(&s).unwrap();
    gantt::paint::PLAIN_DASHES.set(true);
    let plain = app.print_with(&s).unwrap();
    gantt::paint::PLAIN_DASHES.set(false);
    assert_eq!(kept.pages, plain.pages);
    let dashes = kept
        .pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter(|c| matches!(c.op, ops::Op::Dash { .. }))
        .count();
    assert!(dashes > 0, "the printout has dashed lines");
    let _ = std::fs::remove_file(path);
}

#[test]
fn merging_while_converting_equals_merging_the_finished_list() {
    let ctx = egui::Context::default();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        let painter = ctx.layer_painter(egui::LayerId::background());
        for style in [
            gantt::settings::LineStyle::Dashed,
            gantt::settings::LineStyle::Dotted,
            gantt::settings::LineStyle::DashDot,
            gantt::settings::LineStyle::Solid,
        ] {
            for k in 0..4 {
                let y = 10. + k as f32 * 3.;
                gantt::paint::hline(&painter, 5., 205.5, y, 1., Color32::RED, style, 5.);
                gantt::paint::vline(&painter, y, 20., 120.25, 2., Color32::BLUE, style, 20.);
            }
        }
    });
    let finished = ops::coalesce(ops::from_shapes(&output.shapes));
    assert_eq!(ops::from_shapes_coalesced(&output.shapes), finished);
}

/// Open the preview from the toolbar and let it finish.
fn open_preview(d: &mut Driver) {
    d.home();
    d.click("tool_print_preview");
    settle(d);
}

fn settle(d: &mut Driver) {
    for _ in 0..20 {
        if d.app.print_preview_ready() {
            return;
        }
        d.frames(1);
    }
    panic!("the preview never finished");
}

/// The open preview's pages.
fn preview_pages(d: &mut Driver) -> Vec<std::rc::Rc<Page>> {
    let ctx = d.app.print_context();
    let mut p = d.app.print.preview.take().expect("a preview is open");
    let pages = p.all_pages(&d.app, &ctx);
    d.app.print.preview = Some(p);
    pages
}

/// Apply in the open Page Setup after `change`.
fn apply(d: &mut Driver, change: impl FnOnce(&mut PageSetup)) {
    if d.app.print.dialog.is_none() {
        d.click("preview_page_setup");
    }
    d.frames(2);
    change(&mut d.app.print.dialog.as_mut().unwrap().draft);
    d.frames(2);
    d.click("page_setup_apply");
}

#[test]
fn apply_reuses_the_painted_canvas_when_only_the_bands_or_page_change() {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("apply-cache", p);
    open_preview(&mut d);
    assert_eq!(d.app.print.paints, 1, "opening paints the canvas once");
    // Header and footer text: the page being shown is ready at once.
    apply(&mut d, |s| {
        s.header.sections[0].text = "Tower [project_id] rev 7".into();
    });
    assert!(d.app.print_preview_ready(), "no waiting for a text change");
    assert_eq!(d.app.print.paints, 1);
    let texts: Vec<String> = preview_pages(&mut d)
        .iter()
        .map(|p| p.texts().join("|"))
        .collect();
    assert!(texts[0].contains("Tower"), "{}", texts[0]);
    // Margins, orientation and paper change the page grid only.
    apply(&mut d, |s| {
        s.margins.left = 1.5;
        s.orientation = Orientation::Portrait;
        s.paper = "A4".into();
    });
    assert!(d.app.print_preview_ready());
    assert_eq!(d.app.print.paints, 1, "page changes keep the canvas");
    // A print option changes the canvas: the old pages stay up, the new
    // ones follow a couple of frames later.
    d.click("preview_page_setup");
    d.frames(2);
    d.app
        .print
        .dialog
        .as_mut()
        .unwrap()
        .draft
        .options
        .grid_lines = false;
    d.frames(2);
    let apply_at = d.rect("page_setup_apply").center();
    d.click_at(apply_at);
    assert!(!d.app.print_preview_ready(), "the canvas is painted later");
    assert_eq!(d.app.print.paints, 1, "the click paints nothing");
    settle(&mut d);
    assert_eq!(d.app.print.paints, 2);
    // An edit and its undo each change the project snapshot.
    let first = d.visible_activities()[0];
    d.key(Key::Escape);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.print.preview.is_none());
    assert!(d
        .app
        .apply_cell(first, "activity_name", "Renamed for print"));
    d.frames(3);
    d.click("tool_print_preview");
    settle(&mut d);
    assert_eq!(d.app.print.paints, 3, "a new snapshot paints again");
    let edited = preview_pages(&mut d);
    assert!(edited
        .iter()
        .any(|p| p.texts().iter().any(|t| t.contains("Renamed for print"))));
    let ctx = d.app.print_context();
    d.app.history_move(&ctx, false);
    d.frames(3);
    apply(&mut d, |s| {
        s.header.sections[1].text = "after undo".into();
        // No clock fields, so the two builds can be compared exactly.
        for section in s.header.sections.iter_mut().chain(&mut s.footer.sections) {
            if section.text.contains("print_") {
                section.text = "fixed".into();
            }
        }
    });
    settle(&mut d);
    assert_eq!(
        d.app.print.paints, 4,
        "the undone snapshot is not the kept one"
    );
    let undone = preview_pages(&mut d);
    assert!(undone
        .iter()
        .all(|p| !p.texts().iter().any(|t| t.contains("Renamed for print"))));
    // What the preview shows is what a fresh print gives.
    let setup = d.app.page_setup();
    d.app.print.canvas = None;
    let fresh = d.app.print_with(&setup).unwrap();
    assert_eq!(undone.len(), fresh.pages.len());
    for (k, page) in fresh.pages.iter().enumerate() {
        assert_eq!(*page, *undone[k], "page {k}");
    }
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn apply_without_a_preview_paints_nothing_and_closing_forgets_the_canvas() {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("apply-bare", p);
    d.home();
    d.app.open_page_setup();
    d.frames(3);
    d.app.print.dialog.as_mut().unwrap().draft.header.sections[0].text = "No preview".into();
    d.frames(2);
    d.click("page_setup_apply");
    d.frames(3);
    assert_eq!(d.app.print.paints, 0, "Apply composes nothing");
    assert_eq!(d.app.page_setup().header.sections[0].text, "No preview");
    d.key(Key::Escape);
    d.frames(2);
    open_preview(&mut d);
    assert_eq!(d.app.print.paints, 1);
    d.key(Key::Escape);
    d.frames(3);
    assert!(d.app.print.preview.is_none());
    assert!(d.app.print.canvas.is_none(), "the canvas is not kept");
    // A print with no preview paints again.
    let s = d.app.page_setup();
    d.app.print_with(&s).unwrap();
    assert_eq!(d.app.print.paints, 2);
    assert!(
        d.app.print.canvas.is_none(),
        "no canvas is kept without a preview"
    );
}

#[test]
fn page_setup_is_saved_per_layout_without_rewriting_the_others() {
    let (path, _ctx, mut app) = app("slots");
    let first = app.grid.layout.name.clone();
    let a = PageSetup {
        paper: "A3".into(),
        ..PageSetup::default()
    };
    app.save_page_setup(&a).unwrap();
    app.grid.layout.name = "Second".into();
    let b = PageSetup {
        paper: "Legal".into(),
        ..PageSetup::default()
    };
    app.save_page_setup(&b).unwrap();
    assert_eq!(app.page_setup().paper, "Legal");
    app.grid.layout.name = first.clone();
    assert_eq!(app.page_setup().paper, "A3");
    assert!(app
        .store
        .preference(&setup::slot_key(setup::PREF_KEY, "Second"))
        .unwrap()
        .is_some());
    assert!(
        app.store.preference(setup::PREF_KEY).unwrap().is_none(),
        "the older map is not written"
    );
    // Setups saved in the older map still load, until a layout saves its own.
    let mut old = std::collections::BTreeMap::new();
    old.insert("Legacy".to_string(), a.clone());
    old.insert(first.clone(), b.clone());
    app.store
        .set_preference(setup::PREF_KEY, &serde_json::to_string(&old).unwrap())
        .unwrap();
    app.grid.layout.name = "Legacy".into();
    assert_eq!(app.page_setup().paper, "A3");
    app.grid.layout.name = first;
    assert_eq!(app.page_setup().paper, "A3", "the layout's own entry wins");
    let _ = std::fs::remove_file(path);
}

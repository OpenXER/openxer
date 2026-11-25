//! Enterprise structures: tree rules, each dictionary's edits and checks,
//! XER round trips of every table, the workspace EPS and portfolios, and the
//! dialogs driven through the real UI without a window.
use super::ops::{self, Interval};
use super::tree::{self, TreeSpec};
use super::workspace::{self, Workspace};
use super::*;
use crate::qa_sweep::Driver;
use chrono::{NaiveDate, Weekday};
use openxer_xer::{Document, ProjectData};

fn doc() -> Document {
    openxer_xer::starter().document
}

fn names(doc: &Document, spec: &TreeSpec, owner: Option<i64>) -> Vec<(String, usize)> {
    tree::nodes(doc, spec, owner)
        .iter()
        .map(|n| (tree::value(doc, spec, n.id, spec.name), n.depth))
        .collect()
}

fn reread(doc: &Document) -> Document {
    Document::parse(&doc.text()).unwrap()
}

/// The project exported to XER text and read back.
fn round_trip(p: &ProjectData) -> Document {
    Document::parse(&p.updated_document().text()).unwrap()
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

// ------------------------------------------------------------- tree rules

#[test]
fn tree_add_rename_move_and_delete() {
    let mut doc = doc();
    let spec = ops::OBS;
    let a = tree::add(&mut doc, &spec, None, None, &[]).unwrap();
    let b = tree::add(&mut doc, &spec, None, Some(a), &[]).unwrap();
    let c = tree::add(&mut doc, &spec, None, Some(a), &[]).unwrap();
    assert_eq!(
        names(&doc, &spec, None),
        [
            ("New OBS Element".to_string(), 0),
            ("New OBS Element".to_string(), 1),
            ("New OBS Element 1".to_string(), 1)
        ]
    );
    assert!(tree::rename(&mut doc, &spec, None, b, "obs_name", "Piping").unwrap());
    assert!(!tree::rename(&mut doc, &spec, None, b, "obs_name", " Piping ").unwrap());
    tree::rename(&mut doc, &spec, None, c, "obs_name", "Electrical").unwrap();
    // Siblings keep unique names, case-insensitively; other levels may repeat.
    let e = tree::rename(&mut doc, &spec, None, c, "obs_name", "piping").unwrap_err();
    assert!(e.contains("already named"), "{e}");
    tree::rename(&mut doc, &spec, None, a, "obs_name", "Piping").unwrap();
    for bad in ["", "   ", "Tab\there", "Line\nbreak", &"x".repeat(101)] {
        assert!(
            tree::rename(&mut doc, &spec, None, c, "obs_name", bad).is_err(),
            "{bad:?}"
        );
    }
    // Up / down among siblings.
    assert!(tree::shift(&mut doc, &spec, None, c, -1));
    assert!(!tree::shift(&mut doc, &spec, None, c, -1));
    assert_eq!(names(&doc, &spec, None)[1].0, "Electrical");
    // Indent under the previous sibling, outdent back after the parent.
    assert!(tree::indent(&mut doc, &spec, None, b).unwrap());
    assert_eq!(names(&doc, &spec, None)[2], ("Piping".to_string(), 2));
    assert!(tree::outdent(&mut doc, &spec, None, b).unwrap());
    assert!(tree::outdent(&mut doc, &spec, None, c).unwrap());
    // Two top-level siblings named Piping would clash.
    let e = tree::outdent(&mut doc, &spec, None, b).unwrap_err();
    assert!(e.contains("already named"), "{e}");
    assert_eq!(
        names(&doc, &spec, None),
        [
            ("Piping".to_string(), 0),
            ("Piping".to_string(), 1),
            ("Electrical".to_string(), 0)
        ]
    );
    let gone = tree::delete(&mut doc, &spec, None, a);
    assert_eq!(gone, vec![a, b]);
    assert_eq!(names(&doc, &spec, None), [("Electrical".to_string(), 0)]);
    // The table keeps its exported columns.
    assert_eq!(
        doc.table("OBS").unwrap().fields,
        openxer_xer::standard_fields("OBS").unwrap()
    );
}

#[test]
fn tree_depth_is_bounded_and_parent_loops_do_not_hang() {
    let mut doc = doc();
    let spec = TreeSpec {
        max_depth: 3,
        ..ops::FUNDING
    };
    let a = tree::add(&mut doc, &spec, None, None, &[]).unwrap();
    let b = tree::add(&mut doc, &spec, None, Some(a), &[]).unwrap();
    let c = tree::add(&mut doc, &spec, None, Some(b), &[]).unwrap();
    let e = tree::add(&mut doc, &spec, None, Some(c), &[]).unwrap_err();
    assert!(e.contains("3 levels"), "{e}");
    // Indenting a two-level branch under a top-level sibling would make 4.
    let x = tree::add(&mut doc, &spec, None, None, &[]).unwrap();
    tree::add(&mut doc, &spec, None, Some(x), &[]).unwrap();
    let y = tree::add(&mut doc, &spec, None, None, &[]).unwrap();
    tree::shift(&mut doc, &spec, None, y, -1);
    tree::add(&mut doc, &spec, None, Some(y), &[]).unwrap();
    // x is after y now: indent x under y keeps 3 levels (y, x, x's child).
    assert!(tree::indent(&mut doc, &spec, None, x).unwrap());
    let z = tree::add(&mut doc, &spec, None, Some(x), &[]).unwrap();
    assert!(tree::nodes(&doc, &spec, None)
        .iter()
        .any(|n| n.id == z && n.depth == 2));
    // A loop in the file (a is b's parent and b is a's) still lists each row once.
    let t = doc.table_mut("FUNDSRC");
    let (id, parent) = (
        t.field("fund_id").unwrap(),
        t.field("parent_fund_id").unwrap(),
    );
    for r in &mut t.rows {
        if r[id] == a.to_string() {
            r[parent] = b.to_string();
        }
    }
    let nodes = tree::nodes(&doc, &spec, None);
    let mut ids: Vec<i64> = nodes.iter().map(|n| n.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), nodes.len());
    assert_eq!(nodes.len(), doc.table("FUNDSRC").unwrap().rows.len());
}

#[test]
fn tree_cut_moves_and_copy_duplicates_with_unique_names() {
    let mut doc = doc();
    let spec = ops::ROLES;
    let crafts = tree::add(&mut doc, &spec, None, None, &[]).unwrap();
    tree::rename(&mut doc, &spec, None, crafts, "role_name", "Crafts").unwrap();
    let fitter = tree::add(&mut doc, &spec, None, Some(crafts), &[]).unwrap();
    tree::rename(&mut doc, &spec, None, fitter, "role_name", "Pipefitter").unwrap();
    tree::rename(&mut doc, &spec, None, fitter, "role_short_name", "PF").unwrap();
    let staff = tree::add(&mut doc, &spec, None, None, &[]).unwrap();
    // Role IDs are unique in the whole tree.
    let e = tree::rename(&mut doc, &spec, None, staff, "role_short_name", "pf").unwrap_err();
    assert!(e.contains("already in use"), "{e}");
    let clip = tree::copy(&doc, &spec, None, crafts, false).unwrap();
    let copy = tree::paste(&mut doc, &spec, None, None, &clip).unwrap();
    assert_ne!(copy, crafts);
    assert_eq!(tree::value(&doc, &spec, copy, "role_name"), "Crafts 1");
    let copied: Vec<String> = tree::subtree(&doc, &spec, None, copy)
        .iter()
        .map(|id| tree::value(&doc, &spec, *id, "role_short_name"))
        .collect();
    assert_eq!(copied[1], "PF 1", "copied IDs stay unique");
    // Cut keeps the IDs (assignments still point at them) and moves the branch.
    let clip = tree::copy(&doc, &spec, None, crafts, true).unwrap();
    let moved = tree::paste(&mut doc, &spec, None, Some(staff), &clip).unwrap();
    assert_eq!(moved, crafts);
    let node = tree::nodes(&doc, &spec, None)
        .into_iter()
        .find(|n| n.id == fitter)
        .unwrap();
    assert_eq!(node.depth, 2);
    // A branch cannot go under itself.
    let clip = tree::copy(&doc, &spec, None, staff, true).unwrap();
    assert!(tree::paste(&mut doc, &spec, None, Some(fitter), &clip).is_err());
}

// ------------------------------------------------------------ dictionaries

#[test]
fn obs_delete_hands_responsibility_to_the_parent() {
    let mut p = openxer_xer::starter();
    let doc = &mut p.document;
    let top = tree::add(doc, &ops::OBS, None, None, &[]).unwrap();
    let pm = tree::add(doc, &ops::OBS, None, Some(top), &[]).unwrap();
    let wbs = doc.table_mut("PROJWBS");
    for i in 0..wbs.rows.len() {
        wbs.set(i, "obs_id", pm.to_string());
    }
    let rows = doc.table("PROJWBS").unwrap().rows.len();
    assert_eq!(ops::obs_uses(doc, pm), rows);
    assert_eq!(ops::obs_heir(doc, pm), Some(top));
    ops::delete_obs(doc, pm).unwrap();
    assert_eq!(ops::obs_uses(doc, top), rows);
    let e = ops::delete_obs(doc, top).unwrap_err();
    assert!(e.contains("at least one"), "{e}");
    let back = round_trip(&p);
    assert_eq!(
        names(&back, &ops::OBS, None),
        [("New OBS Element".to_string(), 0)]
    );
}

#[test]
fn role_prices_codes_and_delete() {
    let mut doc = doc();
    let role = tree::add(&mut doc, &ops::ROLES, None, None, &[]).unwrap();
    assert!(ops::set_role_price(&mut doc, role, 0, "85.5").unwrap());
    assert!(ops::set_role_price(&mut doc, role, 2, "$1,200").unwrap());
    assert!(!ops::set_role_price(&mut doc, role, 0, "85.50").unwrap());
    assert!(ops::set_role_price(&mut doc, role, 1, "-1").is_err());
    assert!(ops::set_role_price(&mut doc, role, 1, "abc").is_err());
    assert_eq!(ops::role_prices(&doc, role), [85.5, 0., 1200., 0., 0.]);
    // Role codes: a code, a value tree and the role's value.
    let code = ops::add_role_code_type(&mut doc);
    assert!(ops::list_rename(&mut doc, &ops::ROLE_CODE_TYPES, code, "Craft").unwrap());
    let spec = ops::role_value_spec(&doc, code);
    let v = tree::add(&mut doc, &spec, Some(code), None, &[]).unwrap();
    tree::rename(&mut doc, &spec, Some(code), v, spec.name, "PIPE").unwrap();
    // Max Length 10: longer values are refused, and so is shortening below them.
    assert!(tree::rename(&mut doc, &spec, Some(code), v, spec.name, "ABCDEFGHIJK").is_err());
    assert!(ops::set_role_code_length(&mut doc, code, "3").is_err());
    assert!(ops::set_role_code_length(&mut doc, code, "4").unwrap());
    assert!(ops::set_role_code(&mut doc, role, code, Some(v)));
    assert_eq!(ops::role_code(&doc, role, code), Some(v));
    // Assignments and resources that use the role lose it on delete.
    records::add_row(
        &mut doc,
        "TASKRSRC",
        &[("taskrsrc_id", "1".into()), ("role_id", role.to_string())],
    );
    assert_eq!(ops::role_uses(&doc, role), 1);
    let back = reread(&doc);
    assert_eq!(ops::role_prices(&back, role)[2], 1200.);
    assert_eq!(ops::role_code(&back, role, code), Some(v));
    ops::delete_role(&mut doc, role);
    assert_eq!(ops::role_uses(&doc, role), 0);
    assert!(doc.table("ROLERATE").unwrap().rows.is_empty());
    assert!(doc.table("ROLERCAT").unwrap().rows.is_empty());
    ops::delete_role_code_type(&mut doc, code);
    assert!(doc.table("ROLECATVAL").unwrap().rows.is_empty());
}

#[test]
fn curves_normalize_and_protect_default_curves() {
    let mut doc = doc();
    let even = ops::add_curve(&mut doc, None);
    assert_eq!(ops::curve_points(&doc, even).iter().sum::<f64>(), 100.);
    let mut texts: Vec<String> = vec!["0".into(); 21];
    texts[10] = "2".into();
    texts[20] = "6".into();
    let points = ops::normalize_points(&texts).unwrap();
    assert_eq!((points[10], points[20]), (25., 75.));
    assert!(ops::normalize_points(&vec!["0".to_string(); 21]).is_err());
    assert!(ops::normalize_points(&vec!["101".to_string(); 21]).is_err());
    assert!(ops::set_curve(&mut doc, even, "Late", &points).unwrap());
    let back = reread(&doc);
    assert_eq!(ops::curve_points(&back, even)[20], 75.);
    assert_eq!(
        ops::list_value(&back, &ops::CURVES, even, "curv_name"),
        "Late"
    );
    // A curve marked default in the file cannot change or go.
    let fixed = ops::add_curve(&mut doc, Some(even));
    let i = doc.table("RSRCCURVDATA").unwrap().rows.len() - 1;
    doc.table_mut("RSRCCURVDATA")
        .set(i, "default_flag", "Y".into());
    assert!(ops::set_curve(&mut doc, fixed, "X", &points).is_err());
    assert!(ops::delete_curve(&mut doc, fixed).is_err());
    records::add_row(
        &mut doc,
        "TASKRSRC",
        &[("taskrsrc_id", "1".into()), ("curv_id", even.to_string())],
    );
    assert_eq!(ops::curve_uses(&doc, even), 1);
    ops::delete_curve(&mut doc, even).unwrap();
    assert_eq!(ops::curve_uses(&doc, even), 0);
    assert_eq!(ops::curve_names(&doc).len(), 1);
}

#[test]
fn preset_curves_are_listed_locked_and_copied_with_add() {
    use openxer_xer::edit::curves;
    let mut doc = doc();
    let rows = ops::curve_rows(&doc);
    assert_eq!(rows.len(), curves::presets().len());
    // Presets not in the file have keys below zero; nothing is stored.
    assert!(rows.iter().all(|(k, c)| *k < 0 && c.id.is_none()));
    assert!(doc.table("RSRCCURVDATA").is_none_or(|t| t.rows.is_empty()));
    let (bell, _) = rows
        .iter()
        .find(|(_, c)| c.name == "Bell, Centered")
        .unwrap();
    let bell = *bell;
    assert!(ops::is_locked_curve(&doc, bell) && ops::is_preset_curve(&doc, bell));
    assert_eq!(ops::curve_name(&doc, bell), "Bell, Centered");
    assert_eq!(
        ops::curve_points(&doc, bell),
        curves::preset("Bell, Centered").unwrap().points
    );
    assert!(ops::rename_curve(&mut doc, bell, "Mine").is_err());
    assert!(ops::set_curve(&mut doc, bell, "Bell, Centered", &[5.; 21]).is_err());
    assert!(ops::delete_curve(&mut doc, bell).is_err());
    // Add copies the preset's shape into an editable curve of the file.
    let copy = ops::add_curve(&mut doc, Some(bell));
    assert!(copy > 0 && !ops::is_locked_curve(&doc, copy));
    assert_eq!(ops::curve_points(&doc, copy), ops::curve_points(&doc, bell));
    // A file curve cannot take a preset's name, but keeps its own.
    assert!(ops::rename_curve(&mut doc, copy, "two peaks").is_err());
    assert!(ops::rename_curve(&mut doc, copy, "My Bell").unwrap());
    // The presets stay listed once beside the file's curve.
    let names: Vec<String> = ops::curve_rows(&doc)
        .into_iter()
        .map(|(_, c)| c.name)
        .collect();
    assert_eq!(names.len(), curves::presets().len() + 1);
    assert_eq!(names[0], "My Bell");
    // Picking a preset for an assignment stores it once, still locked.
    records::add_row(&mut doc, "TASKRSRC", &[("taskrsrc_id", "1".into())]);
    let row = doc.table("TASKRSRC").unwrap().rows.len() - 1;
    curves::set_assignment_curve(&mut doc, row, Some("Ramp Up")).unwrap();
    let back = reread(&doc);
    let stored = curves::of_assignment(&back, row).unwrap();
    assert!(ops::is_preset_curve(&back, stored) && ops::is_locked_curve(&back, stored));
    let ramps = ops::curve_rows(&back)
        .into_iter()
        .filter(|(_, c)| c.name == "Ramp Up")
        .count();
    assert_eq!(ramps, 1);
}

#[test]
fn the_curves_dialog_shows_presets_and_add_copies_one() {
    let mut d = driver("curve-presets");
    open_from_menu(&mut d, Dialog::Curves);
    let rows = ops::curve_rows(&d.app.project.document);
    let (two, _) = rows.iter().find(|(_, c)| c.name == "Two Peaks").unwrap();
    d.app.enterprise.ui.selected = Some(*two);
    d.frames(2);
    d.click("curves_add");
    d.frames(2);
    let doc = &d.app.project.document;
    let added = ops::list_ids(doc, &ops::CURVES);
    assert_eq!(added.len(), 1, "only the copy is stored");
    assert_eq!(ops::curve_name(doc, added[0]), "New Curve");
    assert_eq!(
        ops::curve_points(doc, added[0]),
        openxer_xer::edit::curves::preset("Two Peaks")
            .unwrap()
            .points
    );
    // Modify on a preset opens the viewer, read only.
    let rows = ops::curve_rows(&d.app.project.document);
    let (two, _) = rows.iter().find(|(_, c)| c.name == "Two Peaks").unwrap();
    d.app.enterprise.ui.selected = Some(*two);
    d.frames(2);
    d.click("curves_modify");
    d.frames(2);
    assert!(d.app.enterprise.ui.curve.as_ref().unwrap().read_only);
    d.click("curve_editor_cancel");
    d.frames(2);
    d.click("curves_close");
    d.assert_clean("curve presets");
}

/// Renders of the Resource Curves list with a preset selected and of the
/// viewer on a preset, written to the ignored `artifacts/curve-presets/`.
#[test]
fn render_curve_presets() {
    use crate::headless_render::SoftwarePainter;
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/curve-presets");
    std::fs::create_dir_all(&out).unwrap();
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    let mine = ops::add_curve(&mut p.document, None);
    ops::rename_curve(&mut p.document, mine, "Site Crew Pattern").unwrap();
    let path = std::env::temp_dir().join(format!(
        "openxer-curve-presets-render-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, crate::store::Store::open(path.clone()).unwrap(), p);
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1400., 900.));
    let mut time = 0.;
    let mut shot = |app: &mut App, name: &str| {
        for _ in 0..4 {
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        image.save(out.join(name)).unwrap();
    };
    app.open_enterprise(Dialog::Curves);
    for name in ["Bell, Early Peak", "Two Peaks"] {
        let key = ops::curve_rows(&app.project.document)
            .into_iter()
            .find(|(_, c)| c.name == name)
            .unwrap()
            .0;
        app.enterprise.ui.selected = Some(key);
        let file = format!("list-{}.png", name.replace([',', ' '], "-").to_lowercase());
        shot(&mut app, &file);
    }
    let key = ops::curve_rows(&app.project.document)
        .into_iter()
        .find(|(_, c)| c.name == "Steady with Ramps")
        .unwrap()
        .0;
    let doc = &app.project.document;
    app.enterprise.ui.curve = Some(CurveEditor {
        id: key,
        name: ops::curve_name(doc, key),
        points: ops::curve_points(doc, key)
            .iter()
            .map(|v| format!("{v}"))
            .collect(),
        read_only: true,
    });
    shot(&mut app, "viewer-steady-with-ramps.png");
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn shifts_keep_unique_starts_and_one_period() {
    let mut doc = doc();
    let shift = ops::add_shift(&mut doc);
    let first = ops::shift_periods(&doc, shift)[0].0;
    let second = ops::add_shift_period(&mut doc, shift).unwrap();
    assert!(ops::set_period_start(&mut doc, shift, second, "08:00").unwrap());
    assert!(ops::set_period_start(&mut doc, shift, second, "0").is_err());
    assert!(ops::set_period_start(&mut doc, shift, second, "24").is_err());
    let third = ops::add_shift_period(&mut doc, shift).unwrap();
    ops::set_period_start(&mut doc, shift, third, "18").unwrap();
    let periods = ops::shift_periods(&doc, shift);
    assert_eq!(ops::period_hours(&periods), [8, 10, 6]);
    ops::delete_shift_period(&mut doc, shift, second).unwrap();
    ops::delete_shift_period(&mut doc, shift, third).unwrap();
    assert!(ops::delete_shift_period(&mut doc, shift, first).is_err());
    let back = reread(&doc);
    assert_eq!(ops::shift_periods(&back, shift), [(first, 0)]);
    ops::delete_shift(&mut doc, shift);
    assert!(doc.table("SHIFTPER").unwrap().rows.is_empty());
}

#[test]
fn currencies_keep_a_fixed_base_and_check_fields() {
    let mut doc = doc();
    let usd = ops::add_currency(&mut doc);
    ops::set_currency(&mut doc, usd, "curr_short_name", "USD").unwrap();
    ops::set_currency(&mut doc, usd, "curr_type", "Dollar").unwrap();
    // The starter header names USD as the base currency.
    assert_eq!(ops::base_currency(&doc), Some(usd));
    assert!(ops::set_currency(&mut doc, usd, "base_exch_rate", "2").is_err());
    assert!(ops::delete_currency(&mut doc, usd).is_err());
    let eur = ops::add_currency(&mut doc);
    assert!(ops::set_currency(&mut doc, eur, "curr_short_name", "usd").is_err());
    ops::set_currency(&mut doc, eur, "curr_short_name", "EUR").unwrap();
    assert!(ops::set_currency(&mut doc, eur, "curr_type", "dollar").is_err());
    ops::set_currency(&mut doc, eur, "curr_type", "Euro").unwrap();
    ops::set_currency(&mut doc, eur, "base_exch_rate", "0.92").unwrap();
    assert!(ops::set_currency(&mut doc, eur, "base_exch_rate", "0").is_err());
    ops::set_currency(&mut doc, eur, "decimal_symbol", ",").unwrap();
    ops::set_currency(&mut doc, eur, "digit_group_symbol", ".").unwrap();
    assert!(ops::set_currency(&mut doc, eur, "decimal_symbol", "12").is_err());
    assert!(ops::set_currency(&mut doc, eur, "decimal_digit_cnt", "9").is_err());
    ops::set_currency(&mut doc, eur, "pos_curr_fmt_type", "1.1 #").unwrap();
    ops::set_currency(&mut doc, eur, "curr_symbol", "EUR").unwrap();
    assert_eq!(
        ops::currency_example(&doc, eur),
        "123.456,78 EUR   (EUR123.456,78)"
    );
    records::add_row(
        &mut doc,
        "RSRC",
        &[("rsrc_id", "9".into()), ("curr_id", eur.to_string())],
    );
    ops::delete_currency(&mut doc, eur).unwrap();
    assert_eq!(
        ops::currency_uses(&doc, usd),
        1,
        "resources move to the base"
    );
    // The currency list feeds User Preferences.
    let mut p = openxer_xer::starter();
    p.document = doc;
    let formats = crate::format::CurrencyFormat::from_project(&p);
    assert_eq!(formats.len(), 1);
    assert_eq!(formats[0].name, "Dollar");
}

#[test]
fn funding_sources_delete_their_project_rows() {
    let mut doc = doc();
    let a = tree::add(&mut doc, &ops::FUNDING, None, None, &[]).unwrap();
    let b = tree::add(&mut doc, &ops::FUNDING, None, Some(a), &[]).unwrap();
    assert!(ops::set_tree_text(&mut doc, &ops::FUNDING, b, "fund_descr", "Bond", 100).unwrap());
    assert!(ops::set_tree_text(&mut doc, &ops::FUNDING, b, "fund_descr", "\u{7}", 100).is_err());
    records::add_row(
        &mut doc,
        "PROJFUND",
        &[("proj_fund_id", "1".into()), ("fund_id", b.to_string())],
    );
    assert_eq!(ops::funding_uses(&doc, a), 1);
    let back = reread(&doc);
    assert_eq!(tree::value(&back, &ops::FUNDING, b, "fund_descr"), "Bond");
    ops::delete_funding(&mut doc, a);
    assert!(doc.table("FUNDSRC").unwrap().rows.is_empty());
    assert!(doc.table("PROJFUND").unwrap().rows.is_empty());
}

#[test]
fn financial_periods_batch_create_and_store_performance() {
    let months = ops::batch_periods(d(2026, 1, 1), d(2026, 3, 15), Interval::Monthly(31)).unwrap();
    assert_eq!(
        months,
        [
            (d(2026, 1, 1), d(2026, 1, 31)),
            (d(2026, 2, 1), d(2026, 2, 28)),
            (d(2026, 3, 1), d(2026, 3, 31))
        ]
    );
    let mid = ops::batch_periods(d(2026, 1, 1), d(2026, 2, 1), Interval::Monthly(25)).unwrap();
    assert_eq!(mid[0], (d(2026, 1, 1), d(2026, 1, 25)));
    assert_eq!(mid[1], (d(2026, 1, 26), d(2026, 2, 25)));
    let weeks = ops::batch_periods(
        d(2026, 11, 2),
        d(2026, 11, 20),
        Interval::Weekly(Weekday::Sat),
    )
    .unwrap();
    assert_eq!(weeks[0], (d(2026, 11, 2), d(2026, 11, 7)));
    assert_eq!(weeks.last().unwrap().1, d(2026, 11, 21));
    assert!(ops::batch_periods(d(2026, 2, 1), d(2026, 1, 1), Interval::Monthly(31)).is_err());

    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let cal = ops::add_fin_calendar(doc).unwrap();
    assert_eq!(ops::default_fin_calendar(doc), Some(cal));
    let ids = ops::batch_create(
        doc,
        cal,
        d(2026, 1, 1),
        d(2026, 3, 31),
        Interval::Monthly(31),
    )
    .unwrap();
    assert_eq!(ids.len(), 3);
    assert_eq!(ops::periods(doc, cal)[1].name, "Feb-26");
    let e = ops::batch_create(
        doc,
        cal,
        d(2026, 3, 1),
        d(2026, 4, 30),
        Interval::Monthly(31),
    )
    .unwrap_err();
    assert!(e.contains("overlap"), "{e}");
    assert!(ops::rename_period(doc, cal, ids[0], "Feb-26").is_err());
    assert!(ops::rename_period(doc, cal, ids[0], "January").unwrap());
    let second = ops::add_fin_calendar(doc).unwrap();
    assert!(ops::set_default_fin_calendar(doc, second));
    assert_eq!(ops::default_fin_calendar(doc), Some(second));
    ops::set_default_fin_calendar(doc, cal);
    assert_eq!(ops::project_fin_calendar(doc, pid), Some(cal));
    assert_eq!(
        ops::period_containing(doc, cal, d(2026, 2, 14)),
        Some(ids[1])
    );

    // Store: actuals to date, less what earlier periods hold.
    let mut current = doc.clone();
    let task = records::num(
        current
            .table("TASK")
            .unwrap()
            .value(&current.table("TASK").unwrap().rows[0], "task_id"),
    );
    let row = |current: &mut Document, qty: &str, cost: &str| {
        let t = current.table_mut("TASK");
        t.set(0, "act_work_qty", qty.into());
        let t = current.table_mut("TASKRSRC");
        if t.rows.is_empty() {
            t.add(&[
                ("taskrsrc_id", "7".into()),
                ("task_id", task.to_string()),
                ("proj_id", pid.to_string()),
                ("rsrc_type", "RT_Labor".into()),
            ]);
        }
        t.set(0, "act_reg_qty", qty.into());
        t.set(0, "act_reg_cost", cost.into());
    };
    row(&mut current, "10", "500");
    assert_eq!(
        ops::store_period(doc, &current, pid, cal, ids[0]).unwrap(),
        1
    );
    row(&mut current, "25", "1250");
    ops::store_period(doc, &current, pid, cal, ids[1]).unwrap();
    // Storing again into February replaces its rows.
    ops::store_period(doc, &current, pid, cal, ids[1]).unwrap();
    let back = reread(doc);
    let fin = back.table("TASKFIN").unwrap();
    let feb: Vec<_> = fin
        .rows
        .iter()
        .filter(|r| records::num(fin.value(r, "fin_dates_id")) == ids[1])
        .collect();
    assert_eq!(feb.len(), 1);
    assert_eq!(records::float(fin.value(feb[0], "act_work_qty")), 15.);
    assert_eq!(records::float(fin.value(feb[0], "act_work_cost")), 750.);
    let trsrc = back.table("TRSRCFIN").unwrap();
    assert_eq!(trsrc.rows.len(), 2);
    assert_eq!(
        back.table("PROJECT")
            .unwrap()
            .value(&back.table("PROJECT").unwrap().rows[0], "last_fin_dates_id"),
        ids[1].to_string()
    );
    // A calendar, or a period, holding stored performance is kept.
    assert_eq!(
        ops::delete_fin_calendar(doc, cal).unwrap_err(),
        ops::FIN_CALENDAR_KEPT
    );
    assert!(ops::delete_periods(doc, &[ids[0]]).is_err());
    assert_eq!(ops::periods(doc, cal).len(), 3);
    assert_eq!(doc.table("TASKFIN").unwrap().rows.len(), 2);
    // Without stored performance the calendar goes with its periods.
    for t in ["TASKFIN", "TRSRCFIN"] {
        doc.table_mut(t).rows.clear();
    }
    ops::delete_fin_calendar(doc, cal).unwrap();
    assert!(ops::periods(doc, cal).is_empty());
    assert_eq!(ops::default_fin_calendar(doc), Some(second));
}

/// Storing a period while a later period already holds performance must not
/// count the later period's actuals twice: the stored periods still add up
/// to the actuals to date, and the project's last stored period stays the
/// latest one.
#[test]
fn storing_an_earlier_period_after_a_later_one_does_not_double_count() {
    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let cal = ops::add_fin_calendar(doc).unwrap();
    let ids = ops::batch_create(
        doc,
        cal,
        d(2026, 1, 1),
        d(2026, 3, 31),
        Interval::Monthly(31),
    )
    .unwrap();
    let mut current = doc.clone();
    let task = records::num(
        current
            .table("TASK")
            .unwrap()
            .value(&current.table("TASK").unwrap().rows[0], "task_id"),
    );
    let set = |current: &mut Document, qty: &str, cost: &str| {
        current.table_mut("TASK").set(0, "act_work_qty", qty.into());
        let t = current.table_mut("TASKRSRC");
        if t.rows.is_empty() {
            t.add(&[
                ("taskrsrc_id", "7".into()),
                ("task_id", task.to_string()),
                ("proj_id", pid.to_string()),
                ("rsrc_type", "RT_Labor".into()),
            ]);
        }
        t.set(0, "act_reg_qty", qty.into());
        t.set(0, "act_reg_cost", cost.into());
    };
    // January holds 10, March holds 15 (25 to date), then February is
    // stored late with 30 to date: February earns the 5 not yet stored.
    set(&mut current, "10", "500");
    ops::store_period(doc, &current, pid, cal, ids[0]).unwrap();
    set(&mut current, "25", "1250");
    ops::store_period(doc, &current, pid, cal, ids[2]).unwrap();
    set(&mut current, "30", "1500");
    ops::store_period(doc, &current, pid, cal, ids[1]).unwrap();
    let back = reread(doc);
    let fin = back.table("TASKFIN").unwrap();
    let qty = |id: i64, field: &str| -> f64 {
        fin.rows
            .iter()
            .filter(|r| records::num(fin.value(r, "fin_dates_id")) == id)
            .map(|r| records::float(fin.value(r, field)))
            .sum()
    };
    assert_eq!(qty(ids[0], "act_work_qty"), 10.);
    assert_eq!(qty(ids[1], "act_work_qty"), 5.);
    assert_eq!(qty(ids[2], "act_work_qty"), 15.);
    let total: f64 = ids.iter().map(|id| qty(*id, "act_work_cost")).sum();
    assert_eq!(total, 1500.);
    let trsrc = back.table("TRSRCFIN").unwrap();
    let assigned: f64 = trsrc
        .rows
        .iter()
        .map(|r| records::float(trsrc.value(r, "act_qty")))
        .sum();
    assert_eq!(assigned, 30.);
    assert_eq!(
        back.table("PROJECT")
            .unwrap()
            .value(&back.table("PROJECT").unwrap().rows[0], "last_fin_dates_id"),
        ids[2].to_string()
    );
}

#[test]
fn every_enterprise_table_survives_save_reopen_and_export() {
    let mut p = openxer_xer::starter();
    let doc = &mut p.document;
    tree::add(doc, &ops::OBS, None, None, &[]).unwrap();
    let role = tree::add(doc, &ops::ROLES, None, None, &[]).unwrap();
    ops::set_role_price(doc, role, 0, "70").unwrap();
    let code = ops::add_role_code_type(doc);
    let spec = ops::role_value_spec(doc, code);
    let value = tree::add(doc, &spec, Some(code), None, &[]).unwrap();
    ops::set_role_code(doc, role, code, Some(value));
    ops::add_curve(doc, None);
    ops::add_shift(doc);
    ops::add_currency(doc);
    tree::add(doc, &ops::FUNDING, None, None, &[]).unwrap();
    let cal = ops::add_fin_calendar(doc).unwrap();
    ops::batch_create(
        doc,
        cal,
        d(2026, 1, 1),
        d(2026, 1, 31),
        Interval::Monthly(31),
    )
    .unwrap();
    let tables = [
        "OBS",
        "ROLES",
        "ROLERATE",
        "ROLECATTYPE",
        "ROLECATVAL",
        "ROLERCAT",
        "RSRCCURVDATA",
        "SHIFT",
        "SHIFTPER",
        "CURRTYPE",
        "FUNDSRC",
        "FINTMPL",
        "FINDATES",
    ];
    // Export: every table with its rows, in the reference's table order.
    let exported = round_trip(&p);
    for t in tables {
        assert_eq!(
            exported.table(t).map(|t| &t.rows),
            p.document.table(t).map(|t| &t.rows),
            "{t}"
        );
        assert_eq!(
            exported.table(t).unwrap().fields,
            openxer_xer::standard_fields(t).unwrap(),
            "{t}"
        );
    }
    let rank = |n: &str| {
        openxer_xer::TABLE_ORDER
            .iter()
            .position(|t| *t == n)
            .unwrap()
    };
    let order: Vec<usize> = exported.tables.iter().map(|t| rank(&t.name)).collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    // Save and reopen through the workspace database.
    let path = std::env::temp_dir().join(format!("openxer-enterprise-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    {
        let mut store = crate::store::Store::open(path.clone()).unwrap();
        store.save(&p).unwrap();
        let back = store.last().unwrap().unwrap();
        for t in tables {
            assert_eq!(
                back.document.table(t).map(|t| &t.rows),
                p.document.table(t).map(|t| &t.rows),
                "{t}"
            );
        }
    }
    let _ = std::fs::remove_file(&path);
    // Undo snapshots cover the tables.
    for t in tables {
        assert!(records::DETAIL_TABLES.contains(&t), "{t}");
    }
}

#[test]
fn corpus_enterprise_tables_import_and_export_unchanged() {
    let dir = crate::test_corpus::dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        println!("corpus not found; skipped");
        return;
    };
    let mut checked = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "xer")
            || entry.metadata().map_or(0, |m| m.len()) > 1_000_000
        {
            continue;
        }
        let doc = Document::read(&path).unwrap();
        let Some((key, _)) = doc.projects().first().cloned() else {
            continue;
        };
        // Files the scheduler refuses are covered by the XER robustness tests.
        let Ok(p) = ProjectData::from_document(doc.clone(), key, path.display().to_string()) else {
            continue;
        };
        let back = round_trip(&p);
        for t in [
            "CURRTYPE",
            "FINTMPL",
            "OBS",
            "SHIFT",
            "SHIFTPER",
            "RSRCCURVDATA",
        ] {
            assert_eq!(
                back.table(t).map(|t| (&t.fields, &t.rows)),
                doc.table(t).map(|t| (&t.fields, &t.rows)),
                "{}: {t}",
                path.display()
            );
        }
        // The dictionaries read: curves, OBS and currencies.
        if doc.table("RSRCCURVDATA").is_some() {
            assert!(!ops::curve_names(&doc).is_empty());
        }
        assert!(ops::base_currency(&doc).is_some() || doc.table("CURRTYPE").is_none());
        checked += 1;
    }
    println!("checked {checked} corpus files");
}

// --------------------------------------------------------------- workspace

#[test]
fn workspace_eps_places_projects_and_keeps_them_on_delete() {
    let mut ws = Workspace::default();
    let root = ws.eps()[0].id;
    assert_eq!(ws.eps_value(root, "eps_name"), "Workspace");
    let spec = workspace::EPS;
    let north = tree::add(&mut ws.doc, &spec, None, Some(root), &[]).unwrap();
    tree::rename(&mut ws.doc, &spec, None, north, "eps_name", "North Plant").unwrap();
    tree::rename(&mut ws.doc, &spec, None, north, "eps_short_name", "NORTH").unwrap();
    let unit = tree::add(&mut ws.doc, &spec, None, Some(north), &[]).unwrap();
    let projects = [1, 2, 3];
    assert!(ws.place(2, unit).unwrap());
    assert!(!ws.place(2, unit).unwrap());
    ws.place(3, north).unwrap();
    assert_eq!(
        ws.node_of(Some(1)),
        root,
        "unplaced projects sit under the first node"
    );
    assert_eq!(ws.projects_under(north, &projects), [2, 3]);
    let groups = ws.grouped(&projects);
    assert_eq!(
        groups,
        [(root, 0, vec![1]), (north, 1, vec![3]), (unit, 2, vec![2])]
    );
    let lines = crate::views::projects::eps_lines(&groups, &Default::default());
    use crate::views::projects::EpsLine::*;
    assert_eq!(
        lines,
        [
            Eps(root, 0),
            Eps(north, 1),
            Eps(unit, 2),
            Project(2, 3),
            Project(3, 2),
            Project(1, 1)
        ]
    );
    let collapsed = [north].into_iter().collect();
    assert_eq!(
        crate::views::projects::eps_lines(&groups, &collapsed),
        [Eps(root, 0), Eps(north, 1), Project(1, 1)]
    );
    // Text round trip.
    let again = Workspace::from_text(&ws.to_text());
    assert_eq!(again.to_text(), ws.to_text());
    // Deleting a node takes the nodes and projects under it.
    assert_eq!(ws.delete_eps(north, &projects).unwrap(), [2, 3]);
    assert_eq!(ws.eps().len(), 1);
    assert_eq!(ws.node_of(Some(1)), root);
    assert!(
        ws.delete_eps(root, &projects).is_err(),
        "the last node stays"
    );
    // Portfolios have no screen any more; rows a workspace already holds
    // are kept through the text round trip, and a deleted project's
    // memberships still go.
    crate::records::add_row(
        &mut ws.doc,
        "PFOLIO",
        &[
            ("pfolio_id", "7".into()),
            ("pfolio_name", "Turnaround".into()),
        ],
    );
    for project in ["1", "2"] {
        crate::records::add_row(
            &mut ws.doc,
            "PFOLIOPROJ",
            &[("pfolio_id", "7".into()), ("project_id", project.into())],
        );
    }
    let again = Workspace::from_text(&ws.to_text());
    assert_eq!(again.doc.table("PFOLIO").unwrap().rows.len(), 1);
    assert_eq!(again.doc.table("PFOLIOPROJ").unwrap().rows.len(), 2);
    ws.forget(2);
    assert_eq!(ws.doc.table("PFOLIOPROJ").unwrap().rows.len(), 1);
    assert_eq!(Workspace::from_text("not json").eps().len(), 1);
}

// --------------------------------------------------------------- headless

fn driver(name: &str) -> Driver {
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    Driver::new(name, p)
}

fn open_from_menu(d: &mut Driver, which: Dialog) {
    d.home();
    d.click(&format!("menu_bar_{}", which.menu().to_lowercase()));
    d.click(which.action());
    d.frames(2);
    assert_eq!(
        d.app.enterprise.ui.open,
        Some(which),
        "{which:?} did not open"
    );
}

#[test]
fn every_enterprise_menu_item_opens_and_closes_without_changes() {
    let mut d = driver("enterprise-menus");
    for which in Dialog::ALL {
        let model = d.app.project.model.clone();
        let tables = d.app.project.document.text();
        let undo = d.app.history.counts().0;
        open_from_menu(&mut d, which);
        d.key(Key::Escape);
        d.frames(2);
        assert_eq!(
            d.app.enterprise.ui.open, None,
            "{which:?}: Escape did not close it"
        );
        open_from_menu(&mut d, which);
        let closer = format!("{}_close", which.salt());
        d.click(&closer);
        assert_eq!(
            d.app.enterprise.ui.open, None,
            "{which:?}: {closer} did not close it"
        );
        assert!(d.app.project.model == model, "{which:?} changed the model");
        assert_eq!(
            d.app.project.document.text(),
            tables,
            "{which:?} changed the tables"
        );
        assert_eq!(
            d.app.history.counts().0,
            undo,
            "{which:?} added an undo step"
        );
        d.assert_clean(&format!("{which:?}"));
    }
    // The Dictionaries toolbar's Role Codes and Funding Sources buttons.
    for (tool, which) in [
        ("tool_role_codes", Dialog::RoleCodes),
        ("tool_funding_sources", Dialog::Funding),
    ] {
        d.home();
        d.click(tool);
        assert_eq!(d.app.enterprise.ui.open, Some(which));
        d.key(Key::Escape);
    }
}

/// A name typed in the OBS Name field is kept when Enter presses the
/// dialog's default button, and dropped by Escape.
#[test]
fn enter_in_the_obs_name_field_keeps_the_typed_name() {
    let mut d = driver("enterprise-obs-enter");
    open_from_menu(&mut d, Dialog::Obs);
    d.click("obs_add");
    d.frames(2);
    let nodes = tree::nodes(&d.app.project.document, &ops::OBS, None);
    let id = nodes[0].id;
    let name = |d: &Driver| tree::value(&d.app.project.document, &ops::OBS, id, "obs_name");
    let undo = d.app.history.counts().0;
    d.click("obs_name");
    for _ in 0..30 {
        d.key(Key::Backspace);
    }
    d.type_text("Field Supervision");
    d.frames(1);
    d.key(Key::Enter);
    d.frames(3);
    assert!(
        d.app.enterprise.ui.open.is_none(),
        "Enter ran the default button"
    );
    assert_eq!(name(&d), "Field Supervision", "the typed name is kept");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    // Escape drops what was typed.
    open_from_menu(&mut d, Dialog::Obs);
    d.app.enterprise.ui.selected = Some(id);
    d.frames(2);
    d.click("obs_name");
    for _ in 0..30 {
        d.key(Key::Backspace);
    }
    d.type_text("Dropped by Escape");
    d.frames(1);
    d.key(Key::Escape);
    d.frames(3);
    assert_eq!(
        name(&d),
        "Field Supervision",
        "Escape keeps the stored name"
    );
    assert_eq!(d.app.history.counts().0, undo + 1);
}

#[test]
fn obs_add_and_rename_through_the_dialog_are_undo_steps() {
    let mut d = driver("enterprise-obs");
    open_from_menu(&mut d, Dialog::Obs);
    let undo = d.app.history.counts().0;
    d.click("obs_add");
    d.frames(2);
    assert_eq!(d.app.history.counts().0, undo + 1, "Add is one undo step");
    let nodes = tree::nodes(&d.app.project.document, &ops::OBS, None);
    assert_eq!(nodes.len(), 1);
    // Click the selected row's name cell, type, Enter.
    d.click("obs_list_cell_0_0");
    d.frames(1);
    assert!(d.app.enterprise.ui.edit.is_some(), "in-place edit started");
    for _ in 0..20 {
        d.key(Key::Backspace);
    }
    d.type_text("Project Controls");
    d.key(Key::Enter);
    d.frames(2);
    assert_eq!(
        tree::value(&d.app.project.document, &ops::OBS, nodes[0].id, "obs_name"),
        "Project Controls"
    );
    assert_eq!(
        d.app.history.counts().0,
        undo + 2,
        "rename is one undo step"
    );
    // A child, then the pad's left arrow brings it to the top level.
    d.click("obs_add");
    d.click("obs_move_left");
    d.frames(2);
    assert_eq!(
        names(&d.app.project.document, &ops::OBS, None)
            .iter()
            .map(|n| n.1)
            .collect::<Vec<_>>(),
        [0, 0]
    );
    // Delete asks first; No keeps it, Yes deletes.
    d.click("obs_delete");
    d.click("enterprise_msg_no");
    assert_eq!(
        tree::nodes(&d.app.project.document, &ops::OBS, None).len(),
        2
    );
    d.click("obs_delete");
    d.click("enterprise_msg_yes");
    assert_eq!(
        tree::nodes(&d.app.project.document, &ops::OBS, None).len(),
        1
    );
    d.click("obs_close");
    // Undo walks back every step.
    let steps = d.app.history.counts().0 - undo;
    assert_eq!(steps, 5);
    for _ in 0..steps {
        d.key_with(Key::Z, egui::Modifiers::COMMAND);
    }
    assert!(tree::nodes(&d.app.project.document, &ops::OBS, None).is_empty());
}

#[test]
fn eps_edits_are_undoable_workspace_steps_and_group_the_projects_view() {
    let mut d = driver("enterprise-eps");
    open_from_menu(&mut d, Dialog::Eps);
    let undo = d.app.history.counts().0;
    d.click("eps_add");
    d.frames(2);
    assert_eq!(d.app.history.counts().0, undo + 1);
    let ws = d.app.workspace().clone();
    let nodes = ws.eps();
    assert_eq!(nodes.len(), 2);
    let child = nodes[1].id;
    assert_eq!(
        nodes[1].depth, 1,
        "Add puts the node under the selected one"
    );
    // The workspace keeps it across a reopen of the database.
    let stored = d.app.store.preference(workspace::KEY).unwrap().unwrap();
    assert_eq!(Workspace::from_text(&stored).eps().len(), 2);
    d.click("eps_close");
    // Move the open project under the new node with Cut and Paste.
    d.click("directory_projects");
    d.frames(2);
    // Rows: Workspace, the new node under it, then Workspace's project.
    d.click("projects_row_2");
    d.click("dock_cut");
    d.click("projects_row_1");
    d.click("dock_paste");
    d.frames(2);
    let store_id = d.app.workspace_ids()[0];
    assert_eq!(d.app.workspace().node_of(Some(store_id)), child);
    assert!(d.has("projects_row_2"));
    assert_eq!(d.app.history.counts().0, undo + 2);
    // Undo the move, then the node.
    d.home();
    d.key_with(Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(d.app.workspace().node_of(Some(store_id)), nodes[0].id);
    d.key_with(Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(d.app.workspace().eps().len(), 1);
    d.key_with(Key::Y, egui::Modifiers::COMMAND);
    assert_eq!(d.app.workspace().eps().len(), 2);
    // Deleting a node takes its projects (qa_workflows::eps_delete); here
    // it holds the only project, which is open, so it is refused.
    d.key_with(Key::Y, egui::Modifiers::COMMAND);
    assert_eq!(d.app.workspace().node_of(Some(store_id)), child);
    open_from_menu(&mut d, Dialog::Eps);
    d.click("eps_list_row_1");
    d.click("eps_delete");
    assert!(d.app.enterprise.ui.confirm.is_none(), "nothing to confirm");
    assert!(d
        .app
        .enterprise
        .ui
        .notice
        .as_ref()
        .unwrap()
        .contains("one project always stays open"));
    d.click("enterprise_msg_ok");
    assert_eq!(d.app.workspace().eps().len(), 2);
    assert_eq!(d.app.workspace().node_of(Some(store_id)), child);
    d.assert_clean("eps");
}

#[test]
fn portfolio_commands_are_gone_and_a_stored_portfolio_filters_nothing() {
    let mut d = driver("enterprise-portfolios");
    // A workspace saved before the 2026-11-07 cleanup can hold a portfolio
    // and an active-portfolio choice; the Projects view shows every project.
    let mut ws = d.app.workspace().clone();
    crate::records::add_row(
        &mut ws.doc,
        "PFOLIO",
        &[("pfolio_id", "7".into()), ("pfolio_name", "Old".into())],
    );
    d.app
        .store
        .set_preference(crate::enterprise::workspace::KEY, &ws.to_text())
        .unwrap();
    d.app
        .store
        .set_preference("enterprise_active_portfolio", "7")
        .unwrap();
    d.app.enterprise.cache = None;
    for (menu, gone) in [
        ("menu_bar_file", "menu_select_portfolio"),
        ("menu_bar_enterprise", "menu_portfolios"),
    ] {
        d.click(menu);
        d.frames(2);
        assert!(!d.has(gone), "{gone} is gone");
        d.key(Key::Escape);
        d.frames(2);
    }
    assert!(!d
        .app
        .status_panels()
        .iter()
        .any(|p| p.starts_with("Portfolio:") || p.starts_with("Access Mode:")));
    d.click("directory_projects");
    d.frames(2);
    assert!(d.has("projects_row_0"), "no portfolio filter applies");
    // The stored rows are left as they were.
    assert_eq!(d.app.workspace().doc.table("PFOLIO").unwrap().rows.len(), 1);
    d.assert_clean("portfolios");
}

#[test]
fn store_period_performance_stores_into_the_chosen_period() {
    let mut d = driver("enterprise-store");
    // No periods: Store Now is disabled.
    d.home();
    d.click("menu_bar_tools");
    d.click("menu_store_period");
    d.frames(2);
    let undo = d.app.history.counts().0;
    d.click("store_now");
    assert_eq!(d.app.history.counts().0, undo);
    d.click("store_cancel");
    assert!(!d.app.sched.progress.store_period);
    // Periods around the data date.
    open_from_menu(&mut d, Dialog::FinCalendars);
    d.click("fin_calendars_add");
    d.frames(2);
    let data_date = d.app.project.model.project.data_date.unwrap()
        + d.app.project.model.origin_serial * openxer_engine::calendar::DAY;
    let day = ops::day_of_minutes(data_date);
    d.app.enterprise.ui.batch.from = crate::format::day(day - chrono::Duration::days(40));
    d.app.enterprise.ui.batch.to = crate::format::day(day + chrono::Duration::days(40));
    d.click("fin_batch_create");
    d.frames(2);
    let cal = ops::default_fin_calendar(&d.app.project.document).unwrap();
    assert!(ops::periods(&d.app.project.document, cal).len() >= 3);
    d.click("fin_calendars_close");
    let undo = d.app.history.counts().0;
    d.home();
    d.click("menu_bar_tools");
    d.click("menu_store_period");
    d.frames(2);
    d.click("store_now");
    d.frames(2);
    assert!(!d.app.sched.progress.store_period);
    assert_eq!(
        d.app.history.counts().0,
        undo + 1,
        "Store Now is one undo step"
    );
    // The period offered is the last one that ends before the data date,
    // not the one holding it.
    let all = ops::periods(&d.app.project.document, cal);
    let closed = ops::period_before(&all, day).unwrap();
    assert_ne!(
        Some(closed),
        ops::period_containing(&d.app.project.document, cal, day)
    );
    let project = d.app.project.document.table("PROJECT").unwrap();
    assert_eq!(
        project.value(&project.rows[0], "last_fin_dates_id"),
        closed.to_string()
    );
    d.assert_clean("store period");
}

#[test]
fn the_dictionaries_feed_assignments_resources_and_preferences() {
    let mut d = driver("enterprise-pickers");
    let doc = &mut d.app.project.document;
    let curve = ops::add_curve(doc, None);
    let shift = ops::add_shift(doc);
    records::add_row(
        doc,
        "RSRC",
        &[("rsrc_id", "50".into()), ("rsrc_short_name", "CREW".into())],
    );
    records::set(doc, "RSRC", 0, "shift_id", shift.to_string());
    assert_eq!(ops::shift_uses(doc, shift), 1);
    assert_eq!(ops::curve_names(doc), [(curve, "New Curve".to_string())]);
    // Deleting the shift through the dialog asks, then clears the resource.
    open_from_menu(&mut d, Dialog::Shifts);
    d.click("shifts_delete");
    assert!(d
        .app
        .enterprise
        .ui
        .confirm
        .as_ref()
        .unwrap()
        .text
        .contains("1 resource"));
    d.click("enterprise_msg_yes");
    assert_eq!(ops::shift_uses(&d.app.project.document, shift), 0);
    d.click("shifts_close");
    d.assert_clean("pickers");
}

// ----------------------------------------------------------------- renders

/// Software renders of every enterprise dialog with some content, written to
/// the ignored `artifacts/enterprise/` folder for a look.
#[test]
fn render_enterprise_dialogs() {
    use crate::headless_render::SoftwarePainter;
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/enterprise");
    std::fs::create_dir_all(&out).unwrap();
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    {
        let doc = &mut p.document;
        let top = tree::add(doc, &ops::OBS, None, None, &[]).unwrap();
        tree::rename(doc, &ops::OBS, None, top, "obs_name", "Owner").unwrap();
        let pm = tree::add(doc, &ops::OBS, None, Some(top), &[]).unwrap();
        tree::rename(doc, &ops::OBS, None, pm, "obs_name", "Project Manager").unwrap();
        let crafts = tree::add(doc, &ops::ROLES, None, None, &[]).unwrap();
        tree::rename(doc, &ops::ROLES, None, crafts, "role_name", "Crafts").unwrap();
        let fitter = tree::add(doc, &ops::ROLES, None, Some(crafts), &[]).unwrap();
        tree::rename(doc, &ops::ROLES, None, fitter, "role_name", "Pipefitter").unwrap();
        ops::set_role_price(doc, fitter, 0, "85").unwrap();
        let code = ops::add_role_code_type(doc);
        ops::list_rename(doc, &ops::ROLE_CODE_TYPES, code, "Craft Type").unwrap();
        let spec = ops::role_value_spec(doc, code);
        let v = tree::add(doc, &spec, Some(code), None, &[]).unwrap();
        tree::rename(doc, &spec, Some(code), v, spec.name, "MECH").unwrap();
        ops::set_tree_text(doc, &spec, v, "role_catg_name", "Mechanical", 100).unwrap();
        ops::add_curve(doc, None);
        let shift = ops::add_shift(doc);
        let p2 = ops::add_shift_period(doc, shift).unwrap();
        ops::set_period_start(doc, shift, p2, "12").unwrap();
        let usd = ops::add_currency(doc);
        ops::set_currency(doc, usd, "curr_short_name", "USD").unwrap();
        ops::set_currency(doc, usd, "curr_type", "Dollar").unwrap();
        let f = tree::add(doc, &ops::FUNDING, None, None, &[]).unwrap();
        tree::rename(doc, &ops::FUNDING, None, f, "fund_name", "Capital Budget").unwrap();
        let cal = ops::add_fin_calendar(doc).unwrap();
        ops::batch_create(
            doc,
            cal,
            d(2026, 1, 1),
            d(2026, 12, 31),
            Interval::Monthly(31),
        )
        .unwrap();
    }
    let path = std::env::temp_dir().join(format!(
        "openxer-enterprise-render-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, crate::store::Store::open(path.clone()).unwrap(), p);
    app.save();
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1600., 1000.));
    let mut time = 0.;
    for which in Dialog::ALL {
        app.open_enterprise(which);
        if which == Dialog::Roles {
            app.enterprise.ui.tab = 1;
        }
        for _ in 0..4 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        let client = crate::dialog_kit::noted_client(&ctx, which.salt())
            .unwrap_or_else(|| panic!("{which:?} was not drawn"));
        let x = client.left().round().max(0.) as u32;
        let y = (client.top() - 32.).round().max(0.) as u32;
        let w = (client.width().round() as u32 + 2).min(image.width() - x);
        let h = (client.height().round() as u32 + 34).min(image.height() - y);
        image::imageops::crop_imm(&image, x, y, w, h)
            .to_image()
            .save(out.join(format!("{}.png", which.salt())))
            .unwrap();
    }
    // The curve editor and Store Period Performance.
    app.open_enterprise(Dialog::Curves);
    let id = ops::list_ids(&app.project.document, &ops::CURVES)[0];
    app.enterprise.ui.curve = Some(CurveEditor {
        id,
        name: "Bell".into(),
        points: (0..21_i32)
            .map(|k| format!("{}", 10 - (k - 10).abs()))
            .collect(),
        read_only: false,
    });
    app.enterprise.ui.open = Some(Dialog::Curves);
    for _ in 0..4 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("curve-editor-screen.png")).unwrap();
    app.enterprise.ui = Default::default();
    app.sched.progress.store_period = true;
    for _ in 0..4 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("store-period-screen.png")).unwrap();
    app.sched.progress.store_period = false;
    // A second EPS level holding the project.
    let id = app.workspace_ids()[0];
    let mut ws = app.workspace().clone();
    let root = ws.eps()[0].id;
    let north = tree::add(&mut ws.doc, &workspace::EPS, None, Some(root), &[]).unwrap();
    tree::rename(
        &mut ws.doc,
        &workspace::EPS,
        None,
        north,
        "eps_name",
        "North Plant",
    )
    .unwrap();
    ws.place(id, north).unwrap();
    app.store
        .set_preference(workspace::KEY, &ws.to_text())
        .unwrap();
    app.enterprise = Default::default();
    app.show_view(crate::views::MainView::Projects);
    for _ in 0..4 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("projects-view.png")).unwrap();
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn user_preferences_load_only_the_financial_periods_in_the_chosen_range() {
    let mut d = driver("enterprise-fin-range");
    // Three monthly periods in the default financial period calendar.
    let doc = &mut d.app.project.document;
    let cal = ops::add_fin_calendar(doc).unwrap();
    ops::batch_create(
        doc,
        cal,
        self::d(2026, 1, 1),
        self::d(2026, 3, 31),
        Interval::Monthly(31),
    )
    .unwrap();
    d.app.open_user_preferences();
    assert!(d.app.prefs.fin.has_calendar);
    assert_eq!((d.app.prefs.fin.total, d.app.prefs.fin.loaded), (3, 3));
    d.app.prefs.tab = crate::user_prefs::PrefsTab::Application;
    d.frames(3);
    assert!(d.has("prefs_periods_select") && d.has("prefs_periods_from"));
    let enter = |d: &mut Driver, from: &str, to: &str| {
        d.app.prefs.fin.from = from.into();
        d.app.prefs.fin.to = to.into();
        d.click("prefs_periods_select");
        d.frames(2);
    };
    // A range holding February alone.
    enter(&mut d, "01-Feb-26", "15-Feb-26");
    assert_eq!(d.app.formats.fin_from, Some(self::d(2026, 2, 1)));
    assert_eq!(d.app.formats.fin_to, Some(self::d(2026, 2, 15)));
    assert_eq!((d.app.prefs.fin.total, d.app.prefs.fin.loaded), (3, 1));
    assert!(d.app.prefs.fin.error.is_none());
    let saved = crate::user_prefs::load(&d.app.store).unwrap().unwrap();
    assert_eq!(saved, d.app.formats, "saved at once");
    // Store Period Performance lists only that period.
    let periods = ops::in_range(
        ops::periods(&d.app.project.document, cal),
        d.app.formats.fin_from,
        d.app.formats.fin_to,
    );
    assert_eq!(periods.len(), 1);
    assert_eq!(periods[0].name, "Feb-26");
    // Refused entries change nothing.
    for (from, to, why) in [
        ("01-Mar-26", "01-Feb-26", "after"),
        ("01-Feb-26", "", "both"),
        ("soon", "01-Feb-26", "valid date"),
    ] {
        enter(&mut d, from, to);
        let e = d.app.prefs.fin.error.clone().expect(why);
        assert!(e.contains(why), "{e}");
        assert_eq!(d.app.formats.fin_from, Some(self::d(2026, 2, 1)));
    }
    // Both empty loads every period again.
    enter(&mut d, "", "");
    assert_eq!((d.app.formats.fin_from, d.app.formats.fin_to), (None, None));
    assert_eq!(d.app.prefs.fin.loaded, 3);
    // The range comes back when the dialog is opened again.
    enter(&mut d, "01-Jan-26", "31-Jan-26");
    d.click("prefs_close");
    d.app.open_user_preferences();
    assert_eq!(d.app.prefs.fin.from, "01-Jan-26");
    assert_eq!(d.app.prefs.fin.to, "31-Jan-26");
    assert_eq!(d.app.prefs.fin.loaded, 1);
}

#[test]
fn the_store_period_dialog_lists_the_loaded_periods_only() {
    let mut d = driver("enterprise-fin-store");
    let doc = &mut d.app.project.document;
    let cal = ops::add_fin_calendar(doc).unwrap();
    ops::batch_create(
        doc,
        cal,
        self::d(2026, 1, 1),
        self::d(2026, 3, 31),
        Interval::Monthly(31),
    )
    .unwrap();
    d.app.formats.fin_from = Some(self::d(2026, 9, 1));
    d.app.formats.fin_to = Some(self::d(2026, 9, 30));
    d.home();
    d.click("menu_bar_tools");
    d.click("menu_store_period");
    d.frames(2);
    // Nothing is loaded for the period: the choice is empty and Store Now
    // is refused.
    assert!(d.has("store_period_none"), "no period falls in the range");
    let undo = d.app.history.counts().0;
    d.click("store_now");
    assert_eq!(d.app.history.counts().0, undo);
    d.click("store_cancel");
    d.app.formats.fin_from = None;
    d.app.formats.fin_to = None;
    d.click("menu_bar_tools");
    d.click("menu_store_period");
    d.frames(2);
    assert!(d.has("store_period_choice"));
    d.click("store_cancel");
}

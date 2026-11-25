//! Software renders of the dialogs, cropped to each dialog's window (frame
//! and title bar included), for review.
//! The PNG files go to the ignored `artifacts/review/dialogs/` folder.
use super::*;
use crate::headless_render::SoftwarePainter;

/// One scene: the crop name, the noted client name, and how to open it.
struct Scene {
    file: &'static str,
    client: &'static str,
    open: fn(&mut App),
    clicks: &'static [&'static str],
}

fn scenes() -> Vec<Scene> {
    use crate::user_prefs::PrefsTab;
    macro_rules! prefs_scene {
        ($file:literal, $tab:expr) => {
            Scene {
                file: $file,
                client: "user_preferences",
                open: |app| {
                    app.open_user_preferences();
                    app.prefs.tab = $tab;
                },
                clicks: &[],
            }
        };
    }
    let mut scenes = vec![
        prefs_scene!("user-preferences-time-units", PrefsTab::TimeUnits),
        prefs_scene!("user-preferences-dates", PrefsTab::Dates),
        prefs_scene!("user-preferences-currency", PrefsTab::Currency),
        prefs_scene!("user-preferences-assistance", PrefsTab::Assistance),
        prefs_scene!("user-preferences-application", PrefsTab::Application),
        prefs_scene!("user-preferences-calculations", PrefsTab::Calculations),
        Scene {
            file: "admin-preferences-general",
            client: "admin_preferences",
            open: |app| app.open_admin_preferences(),
            clicks: &[],
        },
        Scene {
            file: "admin-preferences-time-periods",
            client: "admin_preferences",
            open: |app| {
                app.open_admin_preferences();
                app.prefs.admin.tab = 1;
            },
            clicks: &[],
        },
        Scene {
            file: "calendars",
            client: "calendars",
            open: |app| app.open_calendars(),
            clicks: &[],
        },
        Scene {
            file: "calendar-modify",
            client: "calendar_editor",
            open: |app| app.open_calendars(),
            clicks: &["calendars_modify"],
        },
        Scene {
            file: "calendar-workweek",
            client: "calendar_workweek",
            open: |app| app.open_calendars(),
            clicks: &["calendars_modify", "calendar_workweek"],
        },
        Scene {
            file: "update-progress",
            client: "update",
            open: |app| app.open_update_progress(),
            clicks: &[],
        },
        Scene {
            file: "apply-actuals",
            client: "actuals",
            open: |app| app.open_apply_actuals(),
            clicks: &[],
        },
        Scene {
            file: "global-change",
            client: "global_change_list",
            open: |app| app.open_global_change(),
            clicks: &[],
        },
        Scene {
            file: "maintain-baselines",
            client: "baselines_maintain",
            open: |app| app.open_maintain_baselines(),
            clicks: &[],
        },
        Scene {
            file: "assign-baselines",
            client: "baselines_assign",
            open: |app| app.open_assign_baselines(),
            clicks: &[],
        },
        Scene {
            file: "renumber",
            client: "renumber_ids",
            open: |app| {
                app.selected = Some(0);
                app.open_renumber();
            },
            clicks: &[],
        },
        Scene {
            file: "import",
            client: "xer_import",
            open: |app| app.open_import_wizard(),
            clicks: &[],
        },
        Scene {
            file: "import-1-file-name",
            client: "xer_import",
            open: |app| app.open_import_wizard_at(1),
            clicks: &[],
        },
        Scene {
            file: "import-2-project-options",
            client: "xer_import",
            open: |app| app.open_import_wizard_at(2),
            clicks: &[],
        },
        Scene {
            file: "import-3-update-options",
            client: "xer_import",
            open: |app| app.open_import_wizard_at(3),
            clicks: &[],
        },
        Scene {
            file: "import-4-summary",
            client: "xer_import",
            open: |app| app.open_import_wizard_at(4),
            clicks: &[],
        },
        Scene {
            file: "export",
            client: "export",
            open: |app| app.open_export_wizard(),
            clicks: &[],
        },
        Scene {
            file: "schedule",
            client: "schedule",
            open: |app| app.open_schedule_dialog(),
            clicks: &[],
        },
        Scene {
            file: "columns",
            client: "grid_columns",
            open: |app| app.open_columns_dialog(),
            clicks: &[],
        },
        // A selected column: every arrow button enabled.
        Scene {
            file: "columns-selected",
            client: "grid_columns",
            open: |app| app.open_columns_dialog(),
            clicks: &["columns_selected_1"],
        },
        Scene {
            file: "filters",
            client: "grid_filters",
            open: |app| app.open_filters_dialog(),
            clicks: &[],
        },
        Scene {
            file: "group-sort",
            client: "grid_group_sort",
            open: |app| app.open_group_sort_dialog(),
            clicks: &[],
        },
        Scene {
            file: "edit-relationship",
            client: "edit_relationship",
            open: |app| {
                let m = &app.project.model;
                let id = |i: usize| m.activities[i].task_id;
                let index = m
                    .links
                    .iter()
                    .position(|l| id(l.pred) == 1020 && id(l.succ) == 1030)
                    .expect("starter relationship");
                app.edit_link(index);
            },
            clicks: &[],
        },
        Scene {
            file: "bars",
            client: "bars",
            open: |app| app.open_bars_dialog(),
            clicks: &[],
        },
        Scene {
            file: "timescale",
            client: "ts",
            open: |app| app.open_timescale_dialog(),
            clicks: &[],
        },
        Scene {
            file: "chart-options",
            client: "chart",
            open: |app| app.open_chart_options(),
            clicks: &[],
        },
        Scene {
            file: "level",
            client: "level",
            open: |app| app.open_level_resources(),
            clicks: &[],
        },
        Scene {
            file: "check-schedule",
            client: "check",
            open: |app| app.open_check_schedule(),
            clicks: &[],
        },
        Scene {
            file: "about",
            client: "About OpenXER",
            open: |app| app.show_about = true,
            clicks: &[],
        },
        Scene {
            file: "schedule-log",
            client: "Schedule Log",
            open: |app| app.show_log = true,
            clicks: &[],
        },
        Scene {
            file: "error-message",
            client: "OpenXER",
            open: |app| app.error = Some("The file could not be read.".into()),
            clicks: &[],
        },
        Scene {
            file: "roles",
            client: "roles",
            open: |app| app.open_enterprise(crate::enterprise::Dialog::Roles),
            clicks: &[],
        },
    ];
    scenes.extend(remaining_scenes());
    scenes
}

/// Scenes of the remaining Canvas and flow dialogs (preferences, print,
/// comparison, spelling, loops, layouts, panes and views).
fn remaining_scenes() -> Vec<Scene> {
    macro_rules! scene {
        ($file:literal, $client:literal, $open:expr) => {
            scene!($file, $client, $open, &[])
        };
        ($file:literal, $client:literal, $open:expr, $clicks:expr) => {
            Scene {
                file: $file,
                client: $client,
                open: $open,
                clicks: $clicks,
            }
        };
    }
    use crate::panes::usage_fields::FieldsView;
    vec![
        scene!(
            "admin-preferences-earned-value",
            "admin_preferences",
            |app| {
                app.open_admin_preferences();
                app.prefs.admin.tab = 2;
            }
        ),
        scene!("page-setup-page", "page_setup", |app| app.open_page_setup()),
        scene!(
            "page-setup-margins",
            "page_setup",
            |app| app.open_page_setup(),
            &["page_setup_tab_1"]
        ),
        scene!(
            "page-setup-header",
            "page_setup",
            |app| app.open_page_setup(),
            &["page_setup_tab_2"]
        ),
        scene!(
            "page-setup-options",
            "page_setup",
            |app| app.open_page_setup(),
            &["page_setup_tab_4"]
        ),
        scene!(
            "ts-picker",
            "ts_picker",
            |app| app.open_page_setup(),
            &["page_setup_tab_4", "page_setup_ts_start_pick"]
        ),
        scene!("print-preview", "print_preview", |app| app
            .open_print_preview()),
        scene!(
            "schedule-options-general",
            "options",
            |app| app.open_schedule_dialog(),
            &["schedule_options"]
        ),
        scene!(
            "schedule-options-advanced",
            "options",
            |app| {
                app.open_schedule_dialog();
                app.sched.options_advanced = true;
            },
            &["schedule_options"]
        ),
        scene!("recalc-costs", "recalc", |app| app.open_recalc_costs()),
        scene!("compare", "compare", |app| app.open_schedule_comparison()),
        scene!("spell", "spell", |app| app.open_spell_check()),
        scene!("loops", "loops", |app| app.open_loops()),
        scene!("open-project", "open_project", |app| app
            .open_project_dialog()),
        scene!("top-down", "top_down", |app| app.open_top_down_estimation()),
        scene!("table-font", "table_font", |app| app.open_table_font()),
        scene!("chart-font", "chart_font", |app| app.open_chart_font()),
        scene!("collapse-to", "collapse_to", |app| app.open_collapse_to()),
        scene!("new-layout", "new_layout", |app| app.open_new_layout()),
        scene!("network-options", "Activity Network Options", |app| app
            .open_network_options()),
        scene!("trace-options", "Trace Logic Options", |app| app
            .open_trace_options()),
        scene!("spreadsheet-fields", "usage_fields", |app| app
            .open_spreadsheet_fields(FieldsView::ResourceSheet)),
        scene!("profile-options", "usage_profile_options", |app| app
            .open_profile_options()),
        scene!("issue-navigator", "issue_navigator", |app| app
            .open_issue_navigator()),
        scene!("monitor-thresholds", "monitor_thresholds", |app| app
            .open_monitor_thresholds()),
        scene!("customize-toolbars", "Customize", |app| app
            .open_customize_toolbars()),
        scene!("external-apps", "External Applications", |app| app
            .open_external_apps()),
        scene!("new-project", "New Project", |app| app.open_new_project()),
    ]
    .into_iter()
    .chain(gantt_and_baseline_scenes())
    .collect()
}

/// The Gantt and baseline dialogs, each page of the tabbed ones.
fn gantt_and_baseline_scenes() -> Vec<Scene> {
    macro_rules! scene {
        ($file:literal, $client:literal, $open:expr, $clicks:expr) => {
            Scene {
                file: $file,
                client: $client,
                open: $open,
                clicks: $clicks,
            }
        };
    }
    vec![
        scene!(
            "baseline-add",
            "baselines_add",
            |app| app.open_maintain_baselines(),
            &["baselines_add"]
        ),
        scene!(
            "baseline-select-project",
            "baselines_select",
            |app| app.open_maintain_baselines(),
            &["baselines_add", "add_convert", "add_ok"]
        ),
        scene!(
            "baseline-update",
            "baselines_update",
            |app| app.open_maintain_baselines(),
            &[
                "baselines_add",
                "add_ok",
                "baselines_row_0",
                "baselines_update"
            ]
        ),
        scene!(
            "baseline-update-activities",
            "baselines_update",
            |app| app.open_maintain_baselines(),
            &[
                "baselines_add",
                "add_ok",
                "baselines_row_0",
                "baselines_update",
                "update_category_row_1"
            ]
        ),
        scene!(
            "baseline-update-activity",
            "baselines_update",
            |app| app.open_maintain_baselines(),
            &[
                "baselines_add",
                "add_ok",
                "baselines_row_0",
                "baselines_update",
                "update_category_row_3"
            ]
        ),
        scene!(
            "baseline-update-resource-assignment",
            "baselines_update",
            |app| app.open_maintain_baselines(),
            &[
                "baselines_add",
                "add_ok",
                "baselines_row_0",
                "baselines_update",
                "update_category_row_4"
            ]
        ),
        scene!(
            "baseline-update-preview",
            "update_preview",
            |app| app.open_maintain_baselines(),
            &[
                "baselines_add",
                "add_ok",
                "baselines_row_0",
                "baselines_update",
                "update_preview"
            ]
        ),
        scene!(
            "baseline-update-filter",
            "update_filter_select",
            |app| app.open_maintain_baselines(),
            &[
                "baselines_add",
                "add_ok",
                "baselines_row_0",
                "baselines_update",
                "update_category_row_1",
                "update_filter_activities",
                "update_filter_pick"
            ]
        ),
        scene!(
            "baseline-update-keep-options",
            "Update Baseline",
            |app| app.open_maintain_baselines(),
            &[
                "baselines_add",
                "add_ok",
                "baselines_row_0",
                "baselines_update",
                "update_opt_wbs_udfs",
                "update_close"
            ]
        ),
        scene!(
            "baseline-confirm",
            "Confirmation",
            |app| app.open_maintain_baselines(),
            &[
                "baselines_add",
                "add_ok",
                "baselines_row_0",
                "baselines_delete"
            ]
        ),
        scene!(
            "bars-settings",
            "bars",
            |app| app.open_bars_dialog(),
            &["bars_tab_settings"]
        ),
        scene!(
            "bars-labels",
            "bars",
            |app| app.open_bars_dialog(),
            &["bars_tab_labels", "bars_label_add"]
        ),
        scene!(
            "bar-filter",
            "bar_filter",
            |app| app.open_bars_dialog(),
            &["bars_filter_0"]
        ),
        scene!(
            "chart-options-collapsed",
            "chart",
            |app| app.open_chart_options(),
            &["chart_tab_collapsed"]
        ),
        scene!(
            "chart-options-data-date",
            "chart",
            |app| app.open_chart_options(),
            &["chart_tab_datadate"]
        ),
        scene!(
            "chart-options-sight-lines",
            "chart",
            |app| app.open_chart_options(),
            &["chart_tab_sightlines"]
        ),
        scene!(
            "chart-options-progress-line",
            "chart",
            |app| app.open_chart_options(),
            &["chart_tab_progress"]
        ),
        scene!(
            "text-attachment",
            "text_attachment",
            |app| {
                app.gantt
                    .texts
                    .push(crate::gantt::settings::TextAttachment {
                        text: "Crane on site".into(),
                        date: 0,
                        task_id: app.project.model.activities[0].task_id,
                        band: None,
                        font: Default::default(),
                    });
                app.edit_text_attachment(0);
            },
            &[]
        ),
        scene!("curtain", "curtain", |app| app.open_curtain(None, 0), &[]),
    ]
    .into_iter()
    .chain(new_activity_wizard_scenes())
    .chain(new_resource_wizard_scenes())
    .collect()
}

/// Every page of the New Resource wizard, with the personal calendar page
/// as well.
fn new_resource_wizard_scenes() -> Vec<Scene> {
    use crate::resource_wizard::Page;
    macro_rules! page {
        ($file:literal, $page:expr, $personal:expr) => {
            Scene {
                file: $file,
                client: crate::resource_wizard::SALT,
                open: |app| {
                    app.open_resource_wizard();
                    if let Some(w) = app.activity_wizard.resource.as_mut() {
                        w.personal = $personal;
                        w.page = $page;
                    }
                },
                clicks: &[],
            }
        };
    }
    vec![
        page!("new-resource-1-name", Page::Name, false),
        page!("new-resource-2-type", Page::Type, false),
        page!("new-resource-3-units", Page::Units, false),
        page!("new-resource-4-contact", Page::Contact, false),
        page!("new-resource-5-roles", Page::Roles, false),
        page!("new-resource-6-calendar", Page::Calendar, false),
        page!(
            "new-resource-7-existing-calendar",
            Page::CalendarName,
            false
        ),
        page!(
            "new-resource-7b-personal-calendar",
            Page::CalendarName,
            true
        ),
        page!("new-resource-8-auto-compute", Page::AutoCompute, false),
        page!("new-resource-9-done", Page::Done, false),
    ]
}

/// Every page of the New Activity wizard, opened from the first activity.
fn new_activity_wizard_scenes() -> Vec<Scene> {
    use crate::activity_wizard::Page;
    macro_rules! page {
        ($file:literal, $page:expr) => {
            Scene {
                file: $file,
                client: crate::activity_wizard::SALT,
                open: |app| {
                    app.selected = Some(0);
                    app.open_activity_wizard();
                    if let Some(w) = app.activity_wizard.open.as_mut() {
                        w.relationships = true;
                        w.more_details = true;
                        w.page = $page;
                    }
                },
                clicks: &[],
            }
        };
    }
    vec![
        page!("new-activity-1-name", Page::Name),
        page!("new-activity-2-wbs", Page::Wbs),
        page!("new-activity-3-type", Page::Type),
        page!("new-activity-4-resources", Page::Resources),
        page!("new-activity-5-duration-type", Page::DurationType),
        page!("new-activity-6-units", Page::Units),
        page!("new-activity-7-dependent", Page::Dependent),
        page!("new-activity-7a-predecessors", Page::Predecessors),
        page!("new-activity-7b-successors", Page::Successors),
        page!("new-activity-8-more-details", Page::MoreDetails),
        page!("new-activity-8a-expenses", Page::Expenses),
        page!("new-activity-8b-codes", Page::Codes),
        page!("new-activity-8c-documents", Page::Documents),
        page!("new-activity-9-done", Page::Done),
    ]
}

/// Renders every scene on a fresh starter project and writes the crops plus
/// `clients.json` (client sizes) for the comparison script.
#[test]
fn render_dialogs_for_review() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/dialogs")
        .join(std::env::var("OPENXER_DIALOG_SET").unwrap_or_else(|_| "after".into()));
    std::fs::create_dir_all(&out).unwrap();
    let mut sizes = vec![];
    let only = std::env::var("OPENXER_DIALOG_ONLY").unwrap_or_default();
    let mut missed = vec![];
    'scenes: for scene in scenes() {
        if !scene.file.starts_with(&only) {
            continue;
        }
        let path = std::env::temp_dir().join(format!(
            "openxer-dialog-cal-{}-{}.db",
            std::process::id(),
            scene.file
        ));
        let _ = std::fs::remove_file(&path);
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let mut app = App::new(&ctx, Store::open(path.clone()).unwrap(), project);
        let mut painter = SoftwarePainter::default();
        painter.screen = Some(Vec2::new(1600., 1000.));
        let mut time = 0.;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        (scene.open)(&mut app);
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        for click in scene.clicks {
            if !app.action_rects.contains_key(*click) {
                missed.push(format!("{}: no action {click}", scene.file));
                continue 'scenes;
            }
            painter.click_action(&mut app, &ctx, &mut time, click);
            for _ in 0..2 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        let Some(client) = crate::dialog_kit::noted_client(&ctx, scene.client) else {
            missed.push(format!(
                "{}: dialog {} was not drawn",
                scene.file, scene.client
            ));
            continue;
        };
        // The whole window: 1 px border at the sides and bottom, the title
        // bar above the client, and the drop shadow.
        let window = Rect::from_min_max(
            client.min - Vec2::new(1., 32.),
            client.max + Vec2::new(4., 4.),
        );
        let x = window.left().round().max(0.) as u32;
        let y = window.top().round().max(0.) as u32;
        let w = (window.width().round() as u32).min(image.width() - x);
        let h = (window.height().round() as u32).min(image.height() - y);
        image::imageops::crop_imm(&image, x, y, w, h)
            .to_image()
            .save(out.join(format!("{}.png", scene.file)))
            .unwrap();
        sizes.push(format!("  \"{}\": [{w}, {h}]", scene.file));
        drop(app);
        let _ = std::fs::remove_file(&path);
    }
    std::fs::write(
        out.join("clients.json"),
        format!("{{\n{}\n}}\n", sizes.join(",\n")),
    )
    .unwrap();
    assert!(
        missed.is_empty(),
        "scenes not rendered:\n{}",
        missed.join("\n")
    );
}

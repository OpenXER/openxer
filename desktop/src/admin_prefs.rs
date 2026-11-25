//! Admin > Admin Preferences: a vertical-tab dialog (609 x 560 px inside the
//! frame) with the three tabs that have something to show. XER files carry no
//! administrative settings, so the dialog is mostly read-only: General shows
//! the open project's code separator and Time Periods the hours per period
//! used to convert durations; Earned Value has one live setting, the baseline
//! dates earned value and the BL date columns read, kept in the workspace.
use super::*;
use crate::dialog_kit::{self as kit, Entry, VTab};
use crate::schedule_dialogs::{modal, Canvas};

const CLIENT: Vec2 = Vec2::new(609., 560.);
const READ_ONLY: &str = "Read only: XER files carry no administrative settings.";

const TABS: [(&str, &str); 3] = [
    ("&General", "general"),
    ("T&ime Periods", "time_periods"),
    ("Earned &Value", "earned_value"),
];

#[derive(Default)]
pub(crate) struct AdminPrefs {
    pub open: bool,
    pub tab: usize,
}

impl App {
    pub(crate) fn open_admin_preferences(&mut self) {
        self.prefs.admin.open = true;
    }

    pub(crate) fn admin_preferences_dialog(&mut self, ctx: &egui::Context) {
        if !self.prefs.admin.open {
            return;
        }
        // The separator in use: the project's own, or "." when it has none.
        let separator = openxer_xer::edit::wbs::separator(&self.project);
        let tabs: Vec<VTab> = TABS
            .iter()
            .map(|&(label, key)| VTab {
                label,
                key,
                unavailable: None,
            })
            .collect();
        let tab = self.prefs.admin.tab.min(TABS.len() - 1);
        let mut basis = self.date_basis();
        let ((close, help), closed, actions) =
            modal(ctx, "admin_preferences", "Admin Preferences", CLIENT, |c| {
                c.fill(0., 0., CLIENT.x, CLIENT.y, kit::STRIP);
                kit::page(c, 102., 5., 502., 514.);
                if let Some(i) = kit::vertical_tabs(c, "admin_tab_", 5., 5., 98., &tabs, tab) {
                    self.prefs.admin.tab = i;
                }
                match TABS[tab].1 {
                    "time_periods" => time_periods(c),
                    "earned_value" => earned_value(c, &mut basis),
                    _ => general(c, &separator),
                }
                // Settings here apply at once, so the dialog closes with
                // Close (no OK and Cancel).
                let row = kit::button_row(
                    c,
                    CLIENT.x - 5.,
                    530.,
                    &[Entry::close("admin_close"), Entry::help("admin_help")],
                );
                let clicked = |key: &str| row.clicked.as_deref() == Some(key);
                (clicked("admin_close"), clicked("admin_help"))
            });
        self.record_dialog_actions(actions);
        self.set_date_basis(basis);
        if help {
            self.open_help_for("admin_preferences");
        }
        if close || closed {
            self.prefs.admin.open = false;
        }
    }
}

fn general(c: &mut Canvas, separator: &str) {
    kit::section(c, 109., 12., 488., 106., "Code Separator");
    kit::text(
        c,
        129.,
        45.,
        "The character between concatenated codes and WBS code levels.",
        true,
    );
    kit::text(c, 387., 90., "Code Separator", true);
    kit::field(c, 478., 88., 36., 21., separator, false);
    kit::section(c, 109., 123., 488., 98., "Starting Day of Week");
    kit::text(
        c,
        129.,
        156.,
        "The first day of the week shown in calendars.",
        false,
    );
    kit::text(c, 259., 188., "First day of week", false);
    let mut sunday = 0;
    c.combo(
        "admin_week_start",
        358.,
        185.,
        159.,
        22.,
        &[("Sunday", true)],
        &mut sunday,
        false,
    );
    kit::section(c, 109., 225., 488., 85., "Date Filters");
    kit::text(
        c,
        129.,
        255.,
        "How date filters measure the current day, week or month.",
        false,
    );
    let mut off = false;
    c.checkbox(
        "admin_date_filters",
        119.,
        285.,
        455.,
        24.,
        "Finish date filters use start of Current Date, Week, and Month",
        &mut off,
        false,
    );
    kit::section(c, 109., 314., 488., 85., "Activity Duration");
    kit::text(
        c,
        129.,
        344.,
        "The original duration given to new activities.",
        false,
    );
    kit::text(c, 319., 372., "Default Duration", false);
    kit::field(c, 407., 369., 108., 21., "", false);
    kit::text(c, 497., 372., "5d", false);
    kit::section(c, 109., 399., 488., 113., "Password Policy");
    kit::text(
        c,
        129.,
        429.,
        "The workspace file has no user logins.",
        false,
    );
    c.checkbox(
        "admin_password_policy",
        119.,
        465.,
        455.,
        24.,
        "Enable password policy",
        &mut off,
        false,
    );
    kit::tooltip(c, "admin_general_tip", 109., 12., 488., 500., READ_ONLY);
}

fn time_periods(c: &mut Canvas) {
    let p = format::Periods::DEFAULT;
    kit::section(c, 109., 12., 488., 153., "Hours per Time Period");
    kit::text(
        c,
        129.,
        45.,
        "Work hours in each time period, used to convert durations.",
        true,
    );
    for (label, minutes, x) in [
        ("Hours/Day", p.day, 127.),
        ("Hours/Week", p.week, 249.),
        ("Hours/Month", p.month, 372.),
        ("Hours/Year", p.year, 495.),
    ] {
        kit::text(c, x + 2., 86., label, true);
        let value = format!("{:.1}", minutes as f64 / 60.);
        kit::number_field(c, x, 101., 63., 21., &value, true);
    }
    let mut on = true;
    c.checkbox(
        "admin_calendar_hours",
        124.,
        127.,
        449.,
        32.,
        "Use assigned calendar to specify the number of work hours for each time period",
        &mut on,
        false,
    );
    kit::section(c, 109., 170., 488., 342., "Time Period Abbreviations");
    kit::text(
        c,
        129.,
        203.,
        "Letters shown after durations and units.",
        true,
    );
    for (label, value, x, y) in [
        ("Minutes", "n", 189., 259.),
        ("Hours", "h", 282., 259.),
        ("Days", "d", 375., 259.),
        ("Weeks", "w", 189., 314.),
        ("Months", "m", 282., 314.),
        ("Years", "y", 375., 314.),
    ] {
        kit::text(c, x, y - 15., label, true);
        kit::field(c, x, y, 63., 21., value, true);
    }
    kit::tooltip(c, "admin_periods_tip", 109., 12., 488., 500., READ_ONLY);
}

/// Earned Value: the two techniques are set per WBS element and drawn here
/// as read-only defaults; the baseline dates choice is live.
fn earned_value(c: &mut Canvas, basis: &mut crate::baselines::data::DateBasis) {
    use crate::baselines::data::DateBasis;
    kit::section(
        c,
        109.,
        12.,
        488.,
        89.,
        "Technique for computing performance percent complete",
    );
    for (key, label, x, y, on) in [
        ("admin_ev_activity", "Activity % Complete", 122., 35., true),
        ("admin_ev_wbs", "Use WBS Milestones", 122., 54., false),
        ("admin_ev_zero", "0/100 % Complete", 122., 73., false),
        ("admin_ev_fifty", "50/50 % Complete", 312., 35., false),
        ("admin_ev_custom", "Custom % Complete", 312., 54., false),
    ] {
        kit::radio(c, key, x, y, 190., 19., label, on, false);
    }
    kit::section(
        c,
        109.,
        108.,
        488.,
        280.,
        "Technique for computing Estimate to Complete (ETC)",
    );
    kit::radio(
        c,
        "admin_ev_etc_remaining",
        122.,
        143.,
        312.,
        17.,
        "ETC = remaining cost for activity",
        true,
        false,
    );
    kit::text(c, 218., 172., "or", false);
    kit::text(
        c,
        122.,
        194.,
        "ETC = PF * (Budget at Completion - Earned Value), where:",
        false,
    );
    for (key, label, y) in [
        ("admin_ev_pf_one", "PF = 1", 216.),
        ("admin_ev_pf_cpi", "PF = 1/CPI", 237.),
        ("admin_ev_pf_cpi_spi", "PF = 1/(CPI * SPI)", 258.),
        ("admin_ev_pf_user", "PF =", 279.),
    ] {
        kit::radio(c, key, 122., y, 373., 21., label, false, false);
    }
    kit::number_field(c, 172., 279., 57., 21., "1.00", false);
    kit::tooltip(c, "admin_ev_tip", 109., 12., 488., 376., READ_ONLY);
    kit::section(c, 109., 396., 488., 116., "Earned value calculation");
    kit::text(
        c,
        119.,
        430.,
        "When calculating earned value from a baseline use",
        true,
    );
    let items = [
        (DateBasis::Planned.label(), true),
        (DateBasis::Current.label(), true),
    ];
    let mut index = usize::from(*basis == DateBasis::Current);
    c.combo(
        "admin_ev_dates",
        128.,
        461.,
        250.,
        22.,
        &items,
        &mut index,
        true,
    );
    *basis = if index == 1 {
        DateBasis::Current
    } else {
        DateBasis::Planned
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless_render::SoftwarePainter;

    #[test]
    fn admin_preferences_open_switch_tabs_and_close() {
        let path =
            std::env::temp_dir().join(format!("openxer-admin-prefs-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(1.);
            let mut app = App::new(
                &ctx,
                Store::open(path.clone()).unwrap(),
                openxer_xer::starter(),
            );
            let mut painter = SoftwarePainter::default();
            let mut time = 0.;
            app.open_admin_preferences();
            for _ in 0..3 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
            for (_, key) in TABS {
                assert!(app.action_rects.contains_key(&format!("admin_tab_{key}")));
            }
            // The seven "Not available" tabs were removed (2026-11-07).
            for gone in [
                "data_limits",
                "id_lengths",
                "reports",
                "options",
                "rate_types",
                "industry",
                "exception_sites",
            ] {
                assert!(!app.action_rects.contains_key(&format!("admin_tab_{gone}")));
            }
            // Three tabs on a 25 px pitch; Close is a standard button.
            let general = app.action_rects["admin_tab_general"];
            let last = app.action_rects["admin_tab_earned_value"];
            assert_eq!(last[1] - general[1], 50.);
            let close = app.action_rects["admin_close"];
            assert_eq!((close[2] - close[0], close[3] - close[1]), (108., 25.));
            painter.click_action(&mut app, &ctx, &mut time, "admin_tab_time_periods");
            assert_eq!(app.prefs.admin.tab, 1);
            painter.click_action(&mut app, &ctx, &mut time, "admin_tab_earned_value");
            assert_eq!(app.prefs.admin.tab, 2);
            painter.click_action(&mut app, &ctx, &mut time, "admin_close");
            assert!(!app.prefs.admin.open);
        }
        let _ = std::fs::remove_file(path);
    }
}

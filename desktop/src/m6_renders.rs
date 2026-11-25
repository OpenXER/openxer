//! Renders of resource and assignment windows and tabs for a look: `artifacts/m6/*.png`
//! (ignored by git). Software painter only; no native window.
use super::*;
use crate::headless_render::SoftwarePainter;

struct Shot {
    app: App,
    ctx: egui::Context,
    painter: SoftwarePainter,
    time: f64,
    dir: PathBuf,
}

impl Shot {
    fn new(name: &str, project: ProjectData) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "openxer-m6-{name}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let app = App::new(&ctx, Store::open(dir.clone()).unwrap(), project);
        let mut s = Self {
            app,
            ctx,
            painter: Default::default(),
            time: 0.,
            dir,
        };
        s.frames(4);
        s
    }
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let _ = self
                .painter
                .frame(&mut self.app, &self.ctx, &mut self.time, vec![], false);
        }
    }
    fn save(&mut self, out: &Path, name: &str) {
        self.frames(3);
        let (image, _) = self
            .painter
            .frame(&mut self.app, &self.ctx, &mut self.time, vec![], true)
            .unwrap();
        image.save(out.join(format!("{name}.png"))).unwrap();
    }
}

impl Drop for Shot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.dir);
    }
}

#[test]
fn render_the_m6_windows() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/m6");
    std::fs::create_dir_all(&out).unwrap();
    let scheduled = || {
        let mut p = openxer_xer::starter();
        schedule_model(&mut p.model, &p.resources).unwrap();
        p
    };
    // New Project dialog, with a refused entry shown.
    let mut s = Shot::new("wizard", scheduled());
    s.app.open_new_project();
    let w = s.app.new_project.as_mut().unwrap();
    w.error = Some("Must Finish By is before the Planned Start.".into());
    w.error_field = Some("new_project_finish_by");
    s.save(&out, "new-project-wizard");
    // External Applications.
    let mut s = Shot::new("external", scheduled());
    s.app.external.apps = vec![
        external_apps::ExternalApp {
            name: "Viewer".into(),
            command: "notepad.exe".into(),
            arguments: "%xer%".into(),
        },
        external_apps::ExternalApp {
            name: "Calculator".into(),
            command: "calc.exe".into(),
            arguments: String::new(),
        },
    ];
    s.app.open_external_apps();
    s.app.external.dialog.as_mut().unwrap().error =
        Some("Enter the command line of the application.".into());
    s.save(&out, "external-applications");
    // User Preferences > Application with a range.
    let mut s = Shot::new("prefs", scheduled());
    let doc = &mut s.app.project.document;
    let cal = enterprise::ops::add_fin_calendar(doc).unwrap();
    let d = |m: u32, day: u32| chrono::NaiveDate::from_ymd_opt(2026, m, day).unwrap();
    enterprise::ops::batch_create(
        doc,
        cal,
        d(1, 1),
        d(12, 31),
        enterprise::ops::Interval::Monthly(31),
    )
    .unwrap();
    s.app.formats.fin_from = Some(d(3, 1));
    s.app.formats.fin_to = Some(d(6, 30));
    s.app.open_user_preferences();
    s.app.prefs.tab = user_prefs::PrefsTab::Application;
    s.save(&out, "user-preferences-application");
    // Budget tabs and milestones.
    let mut p = views::wbs::tests::with_project_node();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let node = views::wbs::project_node(&p).unwrap();
    let mut s = Shot::new("budget", p);
    {
        use views::budget::*;
        let doc = &mut s.app.project.document;
        for (day, amount, status, reason) in [
            (2, "25000", "Approved", "Added pump skid"),
            (9, "-4000", "Approved", "Scope removed"),
            (16, "12000", "Pending", "Extra shutdown crew"),
        ] {
            let base = (chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
                - chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap())
            .num_days();
            let row = add_change(doc, 1, node, (base + day) * 1440);
            set_change(doc, row, LogField::Amount, amount).unwrap();
            set_change(doc, row, LogField::Status, status).unwrap();
            set_change(doc, row, LogField::Reason, reason).unwrap();
            set_change(doc, row, LogField::Responsible, "P. Manager").unwrap();
        }
        for (m, spend, benefit) in [(11, 12000., 0.), (12, 18000., 2500.), (1, 9000., 4000.)] {
            let year = if m == 1 { 2027 } else { 2026 };
            set_month(
                doc,
                1,
                node,
                chrono::NaiveDate::from_ymd_opt(year, m, 1).unwrap(),
                PlanField::Spend,
                spend,
            )
            .unwrap();
            set_month(
                doc,
                1,
                node,
                chrono::NaiveDate::from_ymd_opt(year, m, 1).unwrap(),
                PlanField::Benefit,
                benefit,
            )
            .unwrap();
        }
        let a = add_milestone(doc, 1, 10);
        set_milestone(doc, a, StepField::Name, "Design issued").unwrap();
        set_milestone(doc, a, StepField::Done, "Yes").unwrap();
        let b = add_milestone(doc, 1, 10);
        set_milestone(doc, b, StepField::Name, "Long-lead equipment ordered").unwrap();
        set_milestone(doc, b, StepField::Weight, "3").unwrap();
    }
    s.app.touch_records();
    s.app.show_view(views::MainView::Projects);
    s.frames(3);
    let p = &s.app.project;
    s.app.views.projects.selected = Some((p.source_name.clone(), p.model.project.proj_id));
    s.app.views.projects.show_details = true;
    for (tab, name) in [
        ("Budget Log", "budget-log"),
        ("Spending Plan", "spending-plan"),
        ("Budget Summary", "budget-summary"),
    ] {
        s.app.views.projects.tab = views::projects::TABS
            .iter()
            .position(|t| *t == tab)
            .unwrap();
        s.save(&out, name);
    }
    s.app.show_view(views::MainView::Wbs);
    s.app.views.wbs.selected = Some(10);
    s.app.views.wbs.show_details = true;
    s.app.views.wbs.tab = views::wbs::TABS
        .iter()
        .position(|t| *t == "WBS Milestones")
        .unwrap();
    s.save(&out, "wbs-milestones");
}

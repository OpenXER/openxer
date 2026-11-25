//! Workflow: Schedule Options closes with Close, as the reference does.
//! Its buttons are Close (the default), Cancel, Default and Help. Close,
//! Cancel and the window close box ask whether to save only when a value
//! differs from what the dialog opened with: Yes saves (one undo step), No
//! drops the change, Cancel and Escape on the question go back to the
//! dialog, and Enter answers Yes, the observed default. Escape on the
//! dialog itself does nothing. Default first asks (Enter answers Yes), then
//! puts in the values observed in the reference and leaves the two
//! workspace check boxes as they are; nothing is saved until the dialog is
//! closed with Yes.
//!
//! [`close_schedule_options`] is the one way tests close the dialog.
use super::*;

/// How a test presses a rendered control, and the app it drives: the
/// workflow [`Driver`], or an app driven frame by frame ([`Frames`]).
pub(crate) trait Press {
    fn press(&mut self, action: &str);
    fn settle(&mut self);
    fn app(&self) -> &App;
}

impl Press for Driver {
    fn press(&mut self, action: &str) {
        self.click(action);
    }
    fn settle(&mut self) {
        self.frames(2);
    }
    fn app(&self) -> &App {
        &self.app
    }
}

/// An app driven frame by frame, as the `headless_ui` tests do.
pub(crate) struct Frames<'a>(pub &'a mut App, pub &'a egui::Context, pub &'a mut f64);

impl Press for Frames<'_> {
    fn press(&mut self, action: &str) {
        self.settle();
        crate::headless_ui::click(self.0, self.1, action, self.2);
    }
    fn settle(&mut self) {
        for _ in 0..2 {
            let _ = crate::headless_ui::frame(self.0, self.1, vec![], self.2);
        }
    }
    fn app(&self) -> &App {
        self.0
    }
}

/// The save question's button for `answer` ("Yes", "No" or "Cancel").
fn save_button(answer: &str) -> String {
    format!("options_save_{}", answer.to_lowercase())
}

/// Whether the save question is up.
pub(crate) fn save_question_up(p: &mut impl Press) -> bool {
    p.settle();
    p.app().action_rects.contains_key(&save_button("Yes"))
}

/// Answer the save question Close (or Escape) raised: `Some(answer)`
/// requires the question and presses that answer, `None` requires that no
/// question was asked.
#[track_caller]
pub(crate) fn answer_options_save(p: &mut impl Press, answer: Option<&str>) {
    let asked = save_question_up(p);
    match answer {
        Some(answer) => {
            assert!(asked, "Close after a change asks whether to save it");
            p.press(&save_button(answer));
        }
        None => assert!(!asked, "Close with nothing changed asks nothing"),
    }
}

/// Close on Schedule Options, then answer the save question (see
/// [`answer_options_save`]).
#[track_caller]
pub(crate) fn close_schedule_options(p: &mut impl Press, answer: Option<&str>) {
    assert!(
        p.app().action_rects.contains_key("options_close"),
        "Schedule Options closes with a Close button"
    );
    p.press("options_close");
    answer_options_save(p, answer);
}

fn starter(name: &str) -> Driver {
    Driver::new(name, crate::reports::test_support::scheduled_starter())
}

/// F9, then Options... in the Schedule dialog.
fn open_options(d: &mut Driver) {
    if !d.app.sched.schedule_open {
        d.home();
        d.key(Key::F9);
        assert!(d.app.sched.schedule_open, "F9 opens the Schedule dialog");
    }
    d.click("schedule_options");
    assert!(d.app.show_schedule, "Options... opens Schedule Options");
    d.click("options_tab_0");
}

fn expected_finish(d: &Driver) -> bool {
    d.app.project.model.options.use_expected_finish
}

fn shown_expected_finish(d: &Driver) -> bool {
    d.app
        .draft_options
        .as_ref()
        .expect("Schedule Options has a draft")
        .use_expected_finish
}

fn undo_steps(d: &Driver) -> usize {
    d.app.history.counts().0
}

#[test]
fn workflow_schedule_options_buttons_follow_the_reference() {
    let mut d = starter("wf-options-close-buttons");
    open_options(&mut d);
    for key in [
        "options_close",
        "options_cancel",
        "options_default",
        "options_help",
    ] {
        assert!(d.has(key), "Schedule Options has {key}");
    }
    assert!(!d.has("options_ok"), "no OK button");
    let (close, cancel, default, help) = (
        d.rect("options_close"),
        d.rect("options_cancel"),
        d.rect("options_default"),
        d.rect("options_help"),
    );
    // One column at the right: Close, Cancel, a gap, Default, Help.
    for r in [cancel, default, help] {
        assert!((r.left() - close.left()).abs() < 0.5, "one column");
    }
    assert!((cancel.top() - close.top() - 30.).abs() < 0.5);
    assert!((default.top() - cancel.top() - 40.).abs() < 0.5);
    assert!((help.top() - default.top() - 30.).abs() < 0.5);
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Close"),
        "Close is the default button"
    );
}

#[test]
fn workflow_schedule_options_close_asks_to_save_a_change() {
    let mut d = starter("wf-options-close");
    let before = expected_finish(&d);
    let steps = undo_steps(&d);

    // Close with nothing changed: no question, nothing saved.
    open_options(&mut d);
    close_schedule_options(&mut d, None);
    assert!(!d.app.show_schedule, "Close closes Schedule Options");
    assert!(d.app.sched.schedule_open, "the Schedule dialog stays up");
    assert_eq!(undo_steps(&d), steps, "nothing to undo");

    // A change, Close, No: the change is dropped.
    open_options(&mut d);
    d.click("options_expected_finish");
    assert_eq!(shown_expected_finish(&d), !before);
    d.click("options_close");
    assert!(save_question_up(&mut d), "the change is noticed");
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Yes"),
        "Yes is the default, as observed"
    );
    assert!(d.has("options_save_cancel"), "the question has Cancel");
    d.click("options_save_no");
    assert!(!save_question_up(&mut d));
    assert!(!d.app.show_schedule, "No closes Schedule Options");
    assert_eq!(expected_finish(&d), before, "No saves nothing");
    assert_eq!(undo_steps(&d), steps);
    open_options(&mut d);
    assert_eq!(shown_expected_finish(&d), before, "reopened: the old value");

    // Cancel on the question, then Escape on it: back to the dialog with
    // the change still on screen.
    d.click("options_expected_finish");
    d.click("options_close");
    assert!(save_question_up(&mut d));
    d.click("options_save_cancel");
    assert!(!save_question_up(&mut d), "Cancel closes the question");
    assert!(d.app.show_schedule, "Cancel goes back to the dialog");
    assert_eq!(shown_expected_finish(&d), !before, "the change is kept");
    d.click("options_close");
    assert!(save_question_up(&mut d));
    d.key(Key::Escape);
    assert!(!save_question_up(&mut d), "Escape closes the question");
    assert!(d.app.show_schedule, "Escape goes back to the dialog");
    assert_eq!(shown_expected_finish(&d), !before);

    // Escape on the dialog does nothing: no question, the change stays.
    d.key(Key::Escape);
    assert!(!save_question_up(&mut d), "Escape asks nothing");
    assert!(d.app.show_schedule, "Escape leaves Schedule Options open");
    assert_eq!(shown_expected_finish(&d), !before, "the change stays");
    // Close then Yes saves, one undo step.
    close_schedule_options(&mut d, Some("Yes"));
    assert!(!d.app.show_schedule, "Yes closes Schedule Options");
    assert!(d.app.sched.schedule_open, "the Schedule dialog stays up");
    assert_eq!(expected_finish(&d), !before, "Yes saves the change");
    open_options(&mut d);
    assert_eq!(
        shown_expected_finish(&d),
        !before,
        "reopened: the new value"
    );
    close_schedule_options(&mut d, None);
    d.key(Key::Escape);
    assert!(!d.app.sched.schedule_open);
    assert_eq!(undo_steps(&d), steps + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(expected_finish(&d), before, "one Undo brings the old value");

    // Enter on the dialog is Close; Enter on the question is Yes.
    open_options(&mut d);
    d.click("options_expected_finish");
    d.key(Key::Enter);
    assert!(save_question_up(&mut d), "Enter on the dialog asks");
    d.key(Key::Enter);
    assert!(!save_question_up(&mut d));
    assert!(!d.app.show_schedule);
    assert_eq!(expected_finish(&d), !before, "Enter answers Yes");
    d.assert_clean("schedule options close");
}

/// Cancel and the window close box behave as Close: they ask only when a
/// value differs from what the dialog opened with.
#[test]
fn workflow_schedule_options_cancel_and_close_box_ask_only_after_a_change() {
    let mut d = starter("wf-options-cancel");
    let before = expected_finish(&d);
    let steps = undo_steps(&d);

    // Cancel with nothing changed: closes, no question.
    open_options(&mut d);
    d.click("options_cancel");
    assert!(
        !save_question_up(&mut d),
        "Cancel with no change asks nothing"
    );
    assert!(!d.app.show_schedule, "Cancel closes Schedule Options");

    // A change, Cancel: the save question; No drops the change.
    open_options(&mut d);
    d.click("options_expected_finish");
    d.click("options_cancel");
    assert!(
        save_question_up(&mut d),
        "Cancel after a change asks to save"
    );
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Yes"),
        "Yes is the default, as observed"
    );
    d.click("options_save_no");
    assert!(!d.app.show_schedule, "No closes Schedule Options");
    assert_eq!(expected_finish(&d), before, "No saves nothing");

    // A change undone again, then the close box: closes at once.
    open_options(&mut d);
    d.click("options_open_critical");
    d.click("options_open_critical");
    d.click("options_title_close");
    assert!(!save_question_up(&mut d), "no change left: no question");
    assert!(
        !d.app.show_schedule,
        "the close box closes Schedule Options"
    );

    // A change, the close box: the same question; Yes saves.
    open_options(&mut d);
    d.click("options_expected_finish");
    d.click("options_title_close");
    answer_options_save(&mut d, Some("Yes"));
    assert!(!d.app.show_schedule);
    assert_eq!(expected_finish(&d), !before, "Yes saves the change");
    assert_eq!(undo_steps(&d), steps + 1, "one undo step");
    d.key(Key::Escape);
    assert!(
        !d.app.sched.schedule_open,
        "Escape closes the Schedule dialog"
    );
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(expected_finish(&d), before, "one Undo brings the old value");
}

/// The Default question's button for `answer` ("Yes" or "No").
fn reset_button(answer: &str) -> String {
    format!("options_reset_{}", answer.to_lowercase())
}

#[test]
fn workflow_schedule_options_default_then_close_saves_the_defaults() {
    let mut d = starter("wf-options-default");
    // Options a new project does not have.
    {
        let o = &mut d.app.project.model.options;
        o.retained_logic = false;
        o.progress_override = true;
        o.lag_from_early_start = false;
        o.use_expected_finish = false;
        o.open_end_uses_early = true;
        o.out_of_sequence = OutOfSequence::None;
        o.float_type = FloatType::Start;
        o.lag_calendar = LagCalendar::Hour24;
        o.multiple_float_paths = true;
        o.max_float_paths = 3;
    }
    open_options(&mut d);
    d.click("options_critical_longest");
    d.click("options_auto_schedule");
    close_schedule_options(&mut d, Some("Yes"));
    assert_eq!(d.app.critical_definition(), (true, 0.));
    assert!(
        d.app.sched.auto_schedule,
        "Schedule automatically is ticked"
    );
    let recalc = d.app.recalc_costs_after_scheduling();

    open_options(&mut d);
    let shown = d.app.draft_options.clone();
    // Default asks first; No leaves every value as it was.
    d.click("options_default");
    assert!(
        d.has(&reset_button("Yes")),
        "Default asks before resetting the options"
    );
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Yes"),
        "Yes is the default, as observed"
    );
    d.click(&reset_button("No"));
    assert!(!d.has(&reset_button("Yes")), "No closes the question");
    assert_eq!(d.app.draft_options, shown, "No changes nothing");
    assert!(d.app.sched.critical_longest && d.app.sched.auto_draft);
    // Default again, Enter: Yes puts the defaults in the dialog.
    d.click("options_default");
    d.key(Key::Enter);
    d.frames(2);
    assert!(!d.has(&reset_button("Yes")), "Enter answers the question");
    assert!(d.app.show_schedule, "Default keeps the dialog up");
    assert!(
        shown_expected_finish(&d),
        "the dialog shows the defaults (Use Expected Finish Dates on)"
    );
    assert!(
        !d.app.project.model.options.use_expected_finish,
        "Default saves nothing by itself"
    );
    assert!(
        d.app.sched.auto_draft,
        "Default leaves Schedule automatically ticked in the dialog"
    );
    assert_eq!(
        d.app.sched.recalc_costs_draft, recalc,
        "Default leaves Recalculate assignment costs as it was"
    );
    close_schedule_options(&mut d, Some("Yes"));
    let o = &d.app.project.model.options;
    assert!(o.retained_logic && !o.progress_override);
    assert!(o.lag_from_early_start && o.use_expected_finish);
    assert!(!o.open_end_uses_early && !o.do_level);
    assert_ne!(o.out_of_sequence, OutOfSequence::None);
    assert_eq!(o.float_type, FloatType::Finish);
    assert_eq!(o.lag_calendar, LagCalendar::PredCal);
    assert!(!o.multiple_float_paths && o.float_paths_use_total);
    assert_eq!(o.max_float_paths, 10);
    assert_eq!(o.float_paths_end_activity, None);
    assert_eq!(d.app.critical_definition(), (false, 0.));
    assert!(
        d.app.sched.auto_schedule,
        "Default then Yes leaves Schedule automatically ticked"
    );
    assert_eq!(
        d.app.recalc_costs_after_scheduling(),
        recalc,
        "Default then Yes leaves Recalculate assignment costs as it was"
    );
}

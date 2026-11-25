//! Enterprise > Calendars: the Global / Resource / Project calendar lists with Add,
//! Delete, Modify, Used By, default and conversions, plus the calendar editor
//! (month view, workweek, exceptions, detailed hours and time periods).
//! Edits change the CALENDAR table and rebuild the scheduling calendars, so F9,
//! save, export and undo all see them.
use super::*;
use crate::dialog_kit::{self as kit, Col, Entry};

mod editor;
pub mod ops;
#[cfg(test)]
mod tests;

pub use editor::Editor;
use ops::{Convert, Kind};

#[derive(Default)]
pub struct CalendarsState {
    pub kind: Option<Kind>,
    pub selected: Option<String>,
    /// Calendar being renamed in place and its draft name.
    pub rename: Option<(String, String)>,
    rename_focus: bool,
    pub editor: Option<Editor>,
    /// "Calendar Used By": calendar id and whether Resources is chosen.
    pub used_by: Option<(String, bool)>,
    /// "Select Calendar To Copy From" with the picked calendar.
    pub copy_from: Option<Option<String>>,
    pub confirm_delete: Option<String>,
    /// "Calendars in Use": the calendar Delete was asked for while something
    /// uses it, and whether the default global calendar is chosen to take
    /// its place (else a replacement is picked next).
    pub in_use: Option<(String, bool)>,
    /// "Select Replacement Calendar": the calendar being deleted and the
    /// global calendar picked to take its place.
    pub replace: Option<(String, Option<String>)>,
    pub notice: Option<String>,
    /// Select mode: the picker that opened the dialog and the resource it
    /// sets. Select puts the chosen calendar there and closes the dialog.
    pub select: Option<(crate::views::resources::Pick, i64)>,
    /// The calendar Select chose.
    pub chosen: Option<String>,
}
impl CalendarsState {
    pub fn kind(&self) -> Kind {
        self.kind.unwrap_or(Kind::Global)
    }
    fn child_open(&self) -> bool {
        self.editor.is_some()
            || self.used_by.is_some()
            || self.copy_from.is_some()
            || self.confirm_delete.is_some()
            || self.in_use.is_some()
            || self.replace.is_some()
            || self.notice.is_some()
    }
}

enum Cmd {
    Add(Option<String>),
    Delete(String),
    /// Delete a calendar in use, moving its users to the second one.
    DeleteReplacing(String, String),
    Rename(String, String),
    SetDefault(String),
    Convert(String, Convert),
    Commit(
        String,
        Box<openxer_xer::calendar_data::CalendarData>,
        [f64; 4],
    ),
}

impl App {
    pub(crate) fn open_calendars(&mut self) {
        self.calendars = Some(CalendarsState::default());
    }

    /// The Calendars dialog in select mode for a resource's calendar: the
    /// list that holds `current` is shown with it selected.
    pub(crate) fn open_calendars_to_select(
        &mut self,
        pick: crate::views::resources::Pick,
        rsrc: i64,
        current: &str,
    ) {
        let pid = self.project.model.project.proj_id;
        let doc = &self.project.document;
        let kind = [Kind::Resource, Kind::Global, Kind::Project]
            .into_iter()
            .find(|k| ops::list(doc, *k, pid).iter().any(|e| e.id == current))
            .unwrap_or(Kind::Resource);
        self.calendars = Some(CalendarsState {
            kind: Some(kind),
            selected: Some(current.to_string()).filter(|c| !c.is_empty()),
            select: Some((pick, rsrc)),
            ..Default::default()
        });
    }

    pub(crate) fn calendars_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.calendars.take() else {
            return;
        };
        let mut escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let mut cmds = vec![];
        if st.editor.is_some() {
            self.calendar_editor(ctx, &mut st, &mut cmds, &mut escape);
        }
        self.calendar_small_dialogs(ctx, &mut st, &mut cmds, &mut escape);
        let mut open = !std::mem::take(&mut escape) || st.rename.is_some();
        if st.rename.is_some() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            st.rename = None;
        }
        self.calendar_list(ctx, &mut st, &mut cmds, &mut open);
        let time = ctx.input(|i| i.time);
        for cmd in cmds {
            self.run_calendar_cmd(&mut st, cmd, time);
        }
        let picked = st.chosen.take().zip(st.select);
        if open && picked.is_none() {
            self.calendars = Some(st);
        }
        if let Some((id, (pick, rsrc))) = picked {
            self.apply_resource_pick(pick, rsrc, id);
        }
    }

    /// The Calendars list (448 x 399 px inside the frame): the
    /// Global / Resource / Project radios in a strip, the list under a
    /// `Display: Calendars` header, and Close, Add, Delete, Modify..., Used
    /// By..., To Global, To Shared, To Personal and Help down the right.
    fn calendar_list(
        &mut self,
        ctx: &egui::Context,
        st: &mut CalendarsState,
        cmds: &mut Vec<Cmd>,
        open: &mut bool,
    ) {
        let pid = self.project.model.project.proj_id;
        let busy = st.child_open();
        let kind = st.kind();
        let entries = ops::list(&self.project.document, kind, pid);
        if !entries.iter().any(|e| Some(&e.id) == st.selected.as_ref()) {
            st.selected = entries.first().map(|e| e.id.clone());
        }
        let selected = entries
            .iter()
            .position(|e| Some(&e.id) == st.selected.as_ref());
        let flag_title = match kind {
            Kind::Global => "Default",
            Kind::Resource => "Personal",
            Kind::Project => "",
        };
        let mut default_click = None;
        let mut rename_done = None;
        let mut close = false;
        let mut help = false;
        // Select mode adds a Select button under Close; the list grows with it.
        let dy = if st.select.is_some() { 30. } else { 0. };
        let ((), closed, actions) = kit::window(
            ctx,
            "calendars",
            "Calendars",
            Vec2::new(448., 399. + dy),
            |c| {
                c.fill(0., 0., 448., 399. + dy, kit::STRIP);
                c.fill(5., 5., 326., 27., kit::FACE);
                c.lowered(5., 5., 326., 27.);
                for (k, x, w) in [
                    (Kind::Global, 15., 95.),
                    (Kind::Resource, 120., 95.),
                    (Kind::Project, 225., 95.),
                ] {
                    let name = format!("calendars_{}", k.caption().to_lowercase());
                    if c.radio(&name, x, 12., w, 17., k.caption(), kind == k, !busy) && kind != k {
                        st.kind = Some(k);
                        st.selected = None;
                        st.rename = None;
                    }
                }
                let mut cols = vec![Col {
                    title: "Calendar Name",
                    width: 199.,
                }];
                if !flag_title.is_empty() {
                    cols.push(Col {
                        title: flag_title,
                        width: 80.,
                    });
                }
                let rename = &mut st.rename;
                let rename_focus = &mut st.rename_focus;
                let out = kit::list(
                    c,
                    "calendar_list",
                    (5., 37., 326., 357. + dy),
                    Some("Display: Calendars"),
                    &cols,
                    entries.len(),
                    selected,
                    Some(0),
                    &mut |c, i, col, rect, sel| {
                        let e = &entries[i];
                        if col == 0 {
                            chrome::paint_icon_at(
                                c.ui.painter(),
                                chrome::Icon::Calendars,
                                Pos2::new(rect.left() + 3., rect.top() + 1.),
                                true,
                            );
                            match rename {
                                Some((id, text)) if *id == e.id => {
                                    let edit = Rect::from_min_max(
                                        Pos2::new(rect.left() + 21., rect.top() + 2.),
                                        rect.max - Vec2::new(2., 2.),
                                    );
                                    c.ui.painter().rect_filled(edit, 0., Color32::WHITE);
                                    let r = c.ui.put(
                                        edit,
                                        egui::TextEdit::singleline(text)
                                            .frame(false)
                                            .font(crate::schedule_dialogs::dialog_font())
                                            .text_color(INK)
                                            .margin(egui::Margin::ZERO)
                                            .desired_width(edit.width()),
                                    );
                                    if std::mem::take(rename_focus) {
                                        r.request_focus();
                                    }
                                    if r.lost_focus() {
                                        rename_done = Some((id.clone(), text.clone()));
                                    }
                                }
                                _ => kit::cell_text(c, rect, &e.name, sel, 22.),
                            }
                        } else {
                            let flag = if kind == Kind::Global {
                                e.default
                            } else {
                                e.personal
                            };
                            let x = (rect.center().x - 6.5).floor();
                            let local = Pos2::new(x, rect.top() + 2.) - c.origin.to_vec2();
                            kit::check_glyph(c, local.x, local.y, flag, true);
                            if kind == Kind::Global && !busy {
                                let hit = c.ui.interact(
                                    Rect::from_min_size(
                                        Pos2::new(x, rect.top() + 2.),
                                        Vec2::splat(13.),
                                    ),
                                    c.id("calendar_default").with(i),
                                    Sense::click(),
                                );
                                if hit.clicked() && !flag {
                                    default_click = Some(e.id.clone());
                                }
                            }
                        }
                    },
                );
                if !busy {
                    if let Some(i) = out.clicked {
                        let id = entries[i].id.clone();
                        if selected == Some(i) && out.double.is_none() && st.rename.is_none() {
                            st.rename = Some((id.clone(), entries[i].name.clone()));
                            st.rename_focus = true;
                        } else if selected != Some(i) {
                            st.rename = None;
                        }
                        st.selected = Some(id);
                    }
                    if let Some(i) = out.double {
                        st.rename = None;
                        st.selected = Some(entries[i].id.clone());
                        self.modify_calendar_in(st, &entries[i].id);
                    }
                }
                let has = st.selected.clone().filter(|_| !busy);
                let personal = selected.is_some_and(|i| entries[i].personal);
                let to_global = kind == Kind::Project && has.is_some();
                let shared = kind == Kind::Resource && has.is_some() && personal;
                let to_personal = kind == Kind::Resource && has.is_some() && !personal;
                let mut column = vec![Entry::Button("calendars_close", "Close", !busy, false)];
                if st.select.is_some() {
                    column.push(Entry::Button(
                        "calendars_select",
                        "Select",
                        has.is_some(),
                        false,
                    ));
                }
                column.extend([
                    Entry::Button("calendars_add", "Add", !busy, true),
                    Entry::Button("calendars_delete", "Delete", has.is_some(), false),
                    Entry::Button("calendars_modify", "Modify...", has.is_some(), false),
                    Entry::Button("calendars_used_by", "Used By...", has.is_some(), false),
                    Entry::Button("calendars_to_global", "To Global", to_global, true),
                    Entry::Button("calendars_to_shared", "To Shared", shared, false),
                    Entry::Button("calendars_to_personal", "To Personal", to_personal, false),
                    Entry::Button("calendars_help", "Help", !busy, true),
                ]);
                let out = kit::button_column(c, 448., &column);
                match out.clicked.as_deref() {
                    Some("calendars_close") => close = true,
                    Some("calendars_select") => st.chosen = has.clone(),
                    Some("calendars_add") => {
                        st.rename = None;
                        st.copy_from = Some(self.default_copy_source(kind));
                    }
                    Some("calendars_delete") => {
                        if let Some(id) = has.clone() {
                            // Not in use: the plain question. In use: Calendars
                            // in Use, with no question first. The default
                            // global calendar is refused.
                            match ops::delete_check(&self.project, &id) {
                                Ok(false) => st.confirm_delete = Some(id),
                                Ok(true) => st.in_use = Some((id, false)),
                                Err(e) => st.notice = Some(e),
                            }
                        }
                    }
                    Some("calendars_modify") => {
                        if let Some(id) = &has {
                            self.modify_calendar_in(st, id);
                        }
                    }
                    Some("calendars_used_by") => st.used_by = has.clone().map(|id| (id, false)),
                    Some("calendars_to_global") => {
                        if let Some(id) = &has {
                            cmds.push(Cmd::Convert(id.clone(), Convert::ToGlobal));
                        }
                    }
                    Some("calendars_to_shared") => {
                        if let Some(id) = &has {
                            cmds.push(Cmd::Convert(id.clone(), Convert::Shared));
                        }
                    }
                    Some("calendars_to_personal") => {
                        if let Some(id) = &has {
                            cmds.push(Cmd::Convert(id.clone(), Convert::Personal));
                        }
                    }
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        if let Some(id) = default_click {
            cmds.push(Cmd::SetDefault(id));
        }
        if let Some((id, text)) = rename_done {
            st.rename = None;
            if !ctx.input(|i| i.key_pressed(Key::Escape))
                && text != ops::name(&self.project.document, &id)
            {
                cmds.push(Cmd::Rename(id, text));
            }
        }
        if !busy && st.rename.is_none() && ctx.input(|i| i.key_pressed(Key::F2)) {
            if let Some(i) = selected {
                st.rename = Some((entries[i].id.clone(), entries[i].name.clone()));
                st.rename_focus = true;
            }
        }
        if help {
            self.open_help_for("calendars");
        }
        if close || closed {
            *open = false;
        }
    }

    fn modify_calendar_in(&mut self, st: &mut CalendarsState, id: &str) {
        match Editor::open(&self.project, id) {
            Ok(editor) => st.editor = Some(editor),
            Err(e) => st.notice = Some(e),
        }
    }

    /// The default global calendar, else the first global calendar.
    fn default_copy_source(&self, _kind: Kind) -> Option<String> {
        let list = ops::list(
            &self.project.document,
            Kind::Global,
            self.project.model.project.proj_id,
        );
        list.iter()
            .find(|e| e.default)
            .or(list.first())
            .map(|e| e.id.clone())
    }

    fn calendar_small_dialogs(
        &mut self,
        ctx: &egui::Context,
        st: &mut CalendarsState,
        cmds: &mut Vec<Cmd>,
        escape: &mut bool,
    ) {
        let pid = self.project.model.project.proj_id;
        if let Some(text) = st.notice.clone() {
            let mut close = std::mem::take(escape);
            let mut rects = vec![];
            let answer = crate::kit_window::message_box(
                ctx,
                "calendars_notice",
                "Calendars",
                crate::kit_window::MessageIcon::Info,
                &text,
                &["OK"],
                Some(0),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            close |= answer.is_some();
            if close {
                st.notice = None;
            }
        }
        if let Some(id) = st.confirm_delete.clone() {
            let mut close = std::mem::take(escape);
            let mut rects = vec![];
            let answer = crate::kit_window::message_box(
                ctx,
                "calendars_delete",
                "Delete Calendar",
                crate::kit_window::MessageIcon::Question,
                &format!(
                    "Delete the calendar \"{}\"?",
                    ops::name(&self.project.document, &id)
                ),
                &["Yes", "No"],
                crate::kit_window::Question::DeleteCalendar.default_in(&["Yes", "No"]),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            if answer == Some(0) {
                cmds.push(Cmd::Delete(id.clone()));
            }
            close |= answer.is_some();
            if close {
                st.confirm_delete = None;
            }
        }
        if let Some((id, use_default)) = st.in_use.clone() {
            self.calendars_in_use(ctx, st, cmds, escape, id, use_default);
        }
        if let Some((id, pick)) = st.replace.clone() {
            self.select_replacement_calendar(ctx, st, cmds, escape, id, pick);
        }
        if let Some((id, resources)) = st.used_by.clone() {
            let mut close = std::mem::take(escape);
            let mut resources = resources;
            let usage = ops::usage(&self.project, &id);
            let name = ops::name(&self.project.document, &id);
            // 420 x 270 px inside the frame: the calendar's name, Projects or
            // Resources, the list of users and Close in the button column.
            const W: f32 = 420.;
            const H: f32 = 270.;
            let ((), closed, actions) = kit::window(
                ctx,
                "calendars_used_by",
                "Calendar Used By",
                Vec2::new(W, H),
                |c| {
                    kit::strip(c, W, H);
                    kit::text(c, 8., 8., &name, true);
                    for (key, x, caption, value) in [
                        ("calendars_used_by_projects", 8., "Projects", false),
                        ("calendars_used_by_resources", 100., "Resources", true),
                    ] {
                        if c.radio(key, x, 26., 86., 17., caption, resources == value, true) {
                            resources = value;
                        }
                    }
                    let rows = if resources {
                        &usage.resources
                    } else {
                        &usage.projects
                    };
                    let header = if resources {
                        "Resources Using Calendar"
                    } else {
                        "Projects Using Calendar"
                    };
                    let list_w = W - kit::STRIP_WIDTH - 12.;
                    kit::list(
                        c,
                        "calendar_used_by",
                        (6., 50., list_w, 190.),
                        None,
                        &[Col {
                            title: header,
                            width: list_w - 2. - kit::SCROLLBAR,
                        }],
                        rows.len(),
                        None,
                        None,
                        &mut |c, i, _, rect, sel| kit::cell_text(c, rect, &rows[i], sel, 3.),
                    );
                    if !resources && usage.activities > 0 {
                        kit::text(
                            c,
                            8.,
                            248.,
                            &format!("Activities using this calendar: {}", usage.activities),
                            true,
                        );
                    }
                    close |= kit::button_column(c, W, &[Entry::close("calendars_used_by_close")])
                        .clicked
                        .is_some();
                },
            );
            self.record_dialog_actions(actions);
            close |= closed;
            st.used_by = (!close).then_some((id, resources));
        }
        if let Some(pick) = st.copy_from.clone() {
            let mut close = std::mem::take(escape);
            let mut pick = pick;
            let kind = st.kind();
            let mut sources: Vec<ops::Entry> = ops::list(&self.project.document, Kind::Global, pid);
            if kind != Kind::Global {
                sources.extend(ops::list(&self.project.document, kind, pid));
            }
            // 420 x 260 px inside the frame: the calendars to copy, Select and
            // Cancel in the button column.
            const W: f32 = 420.;
            const H: f32 = 260.;
            let ((), closed, actions) = kit::window(
                ctx,
                "calendars_copy_from",
                "Select Calendar to Copy From",
                Vec2::new(W, H),
                |c| {
                    kit::strip(c, W, H);
                    let selected = sources.iter().position(|e| Some(&e.id) == pick.as_ref());
                    let list_w = W - kit::STRIP_WIDTH - 12.;
                    let out = kit::list(
                        c,
                        "calendar_copy_from",
                        (6., 6., list_w, H - 12.),
                        None,
                        &[Col {
                            title: "Calendar Name",
                            width: list_w - 2. - kit::SCROLLBAR,
                        }],
                        sources.len(),
                        selected,
                        Some(0),
                        &mut |c, i, _, rect, sel| {
                            kit::cell_text(c, rect, &sources[i].name, sel, 3.)
                        },
                    );
                    if let Some(i) = out.clicked.or(out.double) {
                        pick = Some(sources[i].id.clone());
                    }
                    let column = kit::button_column(
                        c,
                        W,
                        &[
                            Entry::Button("calendars_copy_select", "Select", true, false),
                            Entry::cancel("calendars_copy_cancel"),
                        ],
                    );
                    match column.clicked.as_deref() {
                        Some("calendars_copy_select") => {
                            cmds.push(Cmd::Add(pick.clone()));
                            close = true;
                        }
                        Some(_) => close = true,
                        None if out.double.is_some() => {
                            cmds.push(Cmd::Add(pick.clone()));
                            close = true;
                        }
                        None => {}
                    }
                },
            );
            self.record_dialog_actions(actions);
            close |= closed;
            st.copy_from = (!close).then_some(pick);
        }
    }

    /// "Calendars in Use" (400 x 180 px inside the frame), asked instead of
    /// the delete question when something uses the calendar: the note in a
    /// lowered panel, the two choices, and Help, Cancel and OK in the bottom
    /// row (the observed order). OK with Select Replacement Calendar opens the picker; with the
    /// default global calendar it deletes at once.
    fn calendars_in_use(
        &mut self,
        ctx: &egui::Context,
        st: &mut CalendarsState,
        cmds: &mut Vec<Cmd>,
        escape: &mut bool,
        id: String,
        use_default: bool,
    ) {
        const W: f32 = 400.;
        const H: f32 = 180.;
        let mut close = std::mem::take(escape);
        let mut use_default = use_default;
        let mut ok = false;
        let mut help = false;
        let ((), closed, actions) = kit::window(
            ctx,
            "calendars_in_use",
            "Calendars in Use",
            Vec2::new(W, H),
            |c| {
                c.fill(0., 0., W, H, kit::FACE);
                c.lowered(5., 5., W - 10., 80.);
                kit::lines(
                    c,
                    12.,
                    12.,
                    15.,
                    &[
                        "Activities, projects or resources use the selected calendar.",
                        "Choose the calendar that takes its place before it is deleted.",
                    ],
                    true,
                );
                for (key, y, caption, value) in [
                    (
                        "calendars_in_use_select",
                        95.,
                        "Select Replacement Calendar",
                        false,
                    ),
                    (
                        "calendars_in_use_default",
                        117.,
                        "Use Default Global Calendar",
                        true,
                    ),
                ] {
                    if c.radio(key, 12., y, 260., 17., caption, use_default == value, true) {
                        use_default = value;
                    }
                }
                // The reference's own order: Help, Cancel, OK.
                let out = kit::observed_button_row(
                    c,
                    W - 5.,
                    H - 31.,
                    &[
                        Entry::help("calendars_in_use_help"),
                        Entry::cancel("calendars_in_use_cancel"),
                        Entry::ok("calendars_in_use_ok"),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("calendars_in_use_ok") => ok = true,
                    Some("calendars_in_use_cancel") => close = true,
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("calendars_in_use");
        }
        close |= closed;
        if ok {
            st.in_use = None;
            if use_default {
                match ops::default_global(&self.project.document) {
                    Some(to) if to != id => cmds.push(Cmd::DeleteReplacing(id, to)),
                    _ => {
                        st.notice = Some(
                            "There is no default global calendar to take its place. Select a \
                             replacement calendar instead."
                                .into(),
                        )
                    }
                }
            } else {
                let pick = ops::default_global(&self.project.document)
                    .filter(|d| *d != id)
                    .or_else(|| {
                        ops::replacements(&self.project.document, &id)
                            .first()
                            .map(|e| e.id.clone())
                    });
                st.replace = Some((id, pick));
            }
        } else {
            st.in_use = (!close).then_some((id, use_default));
        }
    }

    /// "Select Replacement Calendar" (420 x 260 px inside the frame): the
    /// global calendars under a `Display: Global Calendars` header with
    /// their Default column, Select and Cancel in the button column. Select
    /// deletes the calendar and moves its users to the one picked.
    fn select_replacement_calendar(
        &mut self,
        ctx: &egui::Context,
        st: &mut CalendarsState,
        cmds: &mut Vec<Cmd>,
        escape: &mut bool,
        id: String,
        pick: Option<String>,
    ) {
        const W: f32 = 420.;
        const H: f32 = 260.;
        let mut close = std::mem::take(escape);
        let mut pick = pick;
        let mut chosen = None;
        let sources = ops::replacements(&self.project.document, &id);
        let ((), closed, actions) = kit::window(
            ctx,
            "calendars_replacement",
            "Select Replacement Calendar",
            Vec2::new(W, H),
            |c| {
                kit::strip(c, W, H);
                let selected = sources.iter().position(|e| Some(&e.id) == pick.as_ref());
                let list_w = W - kit::STRIP_WIDTH - 12.;
                let out = kit::list(
                    c,
                    "calendar_replacement",
                    (6., 6., list_w, H - 12.),
                    Some("Display: Global Calendars"),
                    &[
                        Col {
                            title: "Calendar Name",
                            width: list_w - 82. - kit::SCROLLBAR,
                        },
                        Col {
                            title: "Default",
                            width: 80.,
                        },
                    ],
                    sources.len(),
                    selected,
                    Some(0),
                    &mut |c, i, col, rect, sel| {
                        let e = &sources[i];
                        if col == 0 {
                            kit::cell_text(c, rect, &e.name, sel, 3.)
                        } else {
                            let x = (rect.center().x - 6.5).floor();
                            let local = Pos2::new(x, rect.top() + 2.) - c.origin.to_vec2();
                            kit::check_glyph(c, local.x, local.y, e.default, true);
                        }
                    },
                );
                if let Some(i) = out.clicked.or(out.double) {
                    pick = Some(sources[i].id.clone());
                }
                let column = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button(
                            "calendars_replacement_select",
                            "Select",
                            pick.is_some(),
                            false,
                        ),
                        Entry::cancel("calendars_replacement_cancel"),
                    ],
                );
                match column.clicked.as_deref() {
                    Some("calendars_replacement_select") => chosen = pick.clone(),
                    Some(_) => close = true,
                    None if out.double.is_some() => chosen = pick.clone(),
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        close |= closed;
        if let Some(to) = chosen {
            cmds.push(Cmd::DeleteReplacing(id, to));
            st.replace = None;
        } else {
            st.replace = (!close).then_some((id, pick));
        }
    }

    fn run_calendar_cmd(&mut self, st: &mut CalendarsState, cmd: Cmd, time: f64) {
        let stamp = ops::now_stamp();
        let result = match cmd {
            Cmd::Add(source) => ops::add(&mut self.project, st.kind(), source.as_deref(), &stamp)
                .map(|id| {
                    st.selected = Some(id.clone());
                    st.rename = Some((id, "New Calendar".into()));
                    st.rename_focus = true;
                    "Calendar added".to_string()
                }),
            Cmd::Delete(id) => ops::delete(&mut self.project, &id).map(|_| {
                st.selected = None;
                "Calendar deleted".to_string()
            }),
            Cmd::DeleteReplacing(id, to) => {
                // Activities move too, so the whole project is one undo step.
                let before = self.project.clone();
                let before_revision = self.records_revision;
                let name = ops::name(&self.project.document, &to);
                match ops::delete_replacing(&mut self.project, &id, &to) {
                    Ok(moved) => {
                        self.edit_step(before, before_revision, time);
                        st.selected = None;
                        self.status = format!(
                            "Calendar deleted; {moved} activit{} now use {name}",
                            if moved == 1 { "y" } else { "ies" }
                        );
                    }
                    Err(e) => st.notice = Some(e),
                }
                return;
            }
            Cmd::Rename(id, text) => {
                ops::rename(&mut self.project, &id, &text).map(|_| "Calendar renamed".into())
            }
            Cmd::SetDefault(id) => ops::set_default(&mut self.project, &id)
                .map(|_| "Default global calendar changed".into()),
            Cmd::Convert(id, to) => ops::convert(&mut self.project, &id, to, &stamp).map(|_| {
                if to == Convert::ToGlobal {
                    st.kind = Some(Kind::Global);
                }
                "Calendar converted".into()
            }),
            Cmd::Commit(id, data, hours) => {
                match ops::commit(&mut self.project, &id, &data, hours, &stamp) {
                    Ok(true) => Ok("Calendar updated. Press F9 to reschedule.".into()),
                    Ok(false) => Ok("Calendar unchanged".into()),
                    Err(e) => Err(e),
                }
            }
        };
        match result {
            Ok(message) => {
                if message != "Calendar unchanged" {
                    self.touch_records();
                }
                self.status = message;
            }
            Err(e) => st.notice = Some(e),
        }
    }
}

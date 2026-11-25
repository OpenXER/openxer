# DESIGN.md

How OpenXER's screens are built. Read this before any UI work. The code is the
source of truth: where this file and the code disagree, fix this file in the
same change.

## The one rule

A control that already exists is reused. If the shared one lacks something,
extend it so every caller gains it. Never paint a control yourself and never
write a second copy. Every visual drift found so far came from a second copy.

Where two screens differ and nothing below settles it, the Activities page is
the reference look.

## Find the control first

| You need | Call | Lives in |
|---|---|---|
| Dialog window | `kit::shell`, or the ready-made `kit::window`, `kit::modal`, `kit::modal_at`; flow layout: `chrome::dialog_window`, `chrome::plain_window` | `desktop/src/dialog_kit.rs`, `chrome.rs` |
| Window that rolls up to its title bar, resizes from its corner grip, or stays open beside the work (a tool window: it holds the keyboard only while active); its place kept between sessions | `kit::shell_with` with `kit::Extras { roll_up, min_client, tool }`; `kit::geometry`, `kit::set_geometry`; the grip: `kit::paint_size_grip` | `dialog_kit.rs` |
| Push button | `kit::button`, `kit::default_button` | `dialog_kit.rs` |
| Button column or row | `kit::button_column` with `kit::strip`, `kit::button_row`, entries from `Entry::ok/cancel/close/apply/help`; a default other than OK: `kit::mark_default`; a column in a declared observed order: `kit::observed_button_column`, a row: `kit::observed_button_row` | `dialog_kit.rs` |
| Text link (a web page, a setting) | `kit::link` | `dialog_kit.rs` |
| App setup row (name, state mark, detail, its action) | `ai_agents::row::paint` | `desktop/src/ai_agents/row.rs` |
| Message or confirm box | `kit::message_box`, `kit::message_box_with_lines` | `dialog_kit.rs` |
| Check box, radio | `Canvas::checkbox`, `kit::check_right`, `kit::radio`, `Canvas::radio` | `dialog_kit.rs` |
| Edit, read-only field | `Canvas::edit` (`Canvas::edit_masked` for a key, never drawn), `kit::commit_edit`, `kit::field`; several lines: `kit::text_area`, with a hint, a disabled state, scrolling or Enter left to the caller: `kit::text_area_with` | `dialog_kit.rs` |
| Combo, spin | `Canvas::combo`, `Canvas::spin`; drawn items: `kit::drawn_combo`; a number value: `kit::number_spin` | `dialog_kit.rs` |
| Color picker | `kit::swatch` (flow layout: `kit::color_button`) | `dialog_kit.rs` |
| Date field with calendar (the `...` inside the edit frame) | `views::sheet::date_field` (commits on Enter); over a dialog's draft text: `views::sheet::date_text_field` | `views/sheet.rs` |
| Section box, heading | `kit::section`, `kit::heading` | `dialog_kit.rs` |
| Wizard page (panel and title) and its bottom row (Cancel alone at the left, Prev / Next / Finish at the right) | `kit::wizard_page`, `kit::wizard_buttons`; a title alone: `kit::wizard_title` | `dialog_kit.rs` |
| List with scroll bar | `kit::list`, `kit::rows_in`, `kit::cell_text` | `dialog_kit.rs` |
| Tree list with icons, check cells and in-place editing | `enterprise::ui::grid` (built on `kit::list`) | `enterprise/ui.rs` |
| Kit controls inside a flow window | `kit::flow_canvas` (a Canvas over a block of the flow body) | `dialog_kit.rs` |
| Tabs inside a dialog | `kit::tabs` (horizontal, painted by `tab_strip::show`); tabs that share the panel's width with centred captions: `kit::tabs_spread`; `kit::vertical_tabs`, `kit::vertical_tabs_with_icons`, `kit::page` | `dialog_kit.rs` |
| Move arrows | four-arrow pad: `kit::move_pad`; a two-list picker's 33 x 33 arrow buttons (single or double): `kit::move_button` | `dialog_kit.rs` |
| Detail tabs under a view | `tab_strip::show` | `desktop/src/tab_strip.rs` |
| Table in a view or pane | `grid::Table` (`show`, or `show_with` and `Extras` for line numbers, a sort arrow, a kept scroll offset) | `views/grid.rs` |
| Grouping band look (level fill and text, caption font, totals in the row font) | `grid::band` (`fill`, `ink`, `caption`, `total_font`, `level_style`); view tables pick it by level through `views::grid::band_style` | `grid/band.rs` |
| The activity table and Gantt | the `grid/` and `gantt/` modules; do not build another | `grid/`, `gantt/` |
| Splitter | `activities::paint_splitter` | `activities.rs` |
| Histogram with cumulative curves and its legend (usage profiles, S-Curve report) | `histogram::paint_plot`, `histogram::paint_legend`; the timescale under it `usage::paint_tier` | `panes/histogram.rs` |
| Assign dialog (Assign Resources, Roles, Predecessors, Successors, Activity Codes, ...): window, button column, list with Ctrl and Shift multi-select | `assign_window`, `assign_list`, `assign_picked` | `details/assign_kit.rs` |
| Cell commands of a view table (focus cell, Fill Down, Find and Replace) | `note_view_cells`, `set_view_cell` | `views/table_cells.rs` |
| Controls inside a details pane | `details::form` (`command_button`, `combo`, `section`, `text`) | `details/form.rs` |
| Notebook tab (topic list, Add, Delete, notes pane) of Activity, WBS and Project Details; a captioned notes pane with its formatted text, Modify and the formatting toolbar | `notebook_pane`, `memo_section` | `details/notes_pane.rs` |
| Add button of a tab; Modify / Print / Copy under a notes pane; a number box (right-aligned) | `details::form::add_button`; `form::notes_buttons` (Canvas: `views::sheet::notes_buttons`); `views::sheet::number_field`, read-only `kit::number_field` | `details/form.rs`, `views/sheet.rs`, `dialog_kit.rs` |
| Icon | `chrome::Icon` with `chrome::paint_icon`; new icons are drawn by `tools/icons/` | `chrome.rs` |
| Picture as the mouse pointer | `chrome::paint_pointer_icon` | `chrome.rs` |
| Right-click or drop-down menu | `context_menu::Menu` with `chrome::popup_menu_style` | `context_menu.rs` |
| Colors | `chrome::pal`; chosen defaults in `theme.rs` | `chrome.rs`, `theme.rs` |
| Text in the light Markdown subset (help topics, agent replies): paragraphs, headings, lists, notes, tables, bold, italic, code, links; text still arriving | `markdown::blocks` with a `Dialect`, laid out by `markdown::layout`, links by `markdown::link_at` and `markdown::link_rects`; tables (columns fitted to the width, text wrapped in its cell) by `markdown::table_ui` | `desktop/src/markdown.rs` |
| Conversation (message list): user messages, agent replies with Activity ID links, tool rows, proposed changes, status lines, the empty state | `chat::list::show` over a `chat::model::Conversation`; IDs as links: `chat::links::link_ids` | `desktop/src/chat/` |

The comment at the top of `dialog_kit.rs` is the detailed rulebook for dialogs.

## Standard measurements

- Dialog button: 108 x 25. Button strip at the right: 112 wide, first button
  6 down, 30 from one button to the next, 10 extra before a new group.
- Button order in a column: the committing button (OK, or the dialog's own
  verb such as Schedule), Cancel, Apply, the dialog's other commands, Help
  last. In a bottom row Help comes first, at the left, then the same order
  (`kit::button_row` places it; callers list Help last as for a column). A
  dialog that changes nothing has Close, not OK and Cancel. A dialog whose
  own reference shot shows a different column or bottom row order may
  declare that observed order (`kit::observed_button_column`,
  `kit::observed_button_row`, entries listed in the order drawn); the sweep
  then accepts exactly that order on that dialog. Today: the Calendar editor
  (OK, Cancel, Help, then Work and the rest), Schedule (Cancel, Schedule,
  View Log, Help, Options...) and the Calendars in Use row (Help, Cancel,
  OK).
- One text size in dialogs and panes: `kit::body_font` (Arial 8 pt). Section
  and heading bar captions are bold (`kit::heading_font`, and
  `details::form::section` in the details panes).
- Dialog face `#F0F0F0`. Read-only fields `#F8F8F8`. Disabled text `#6D6D6D`,
  plain.
- Table rows are 18 high. Row lines and the selection end at the last column.
- Buttons highlight on hover. The button Enter would press shows the default
  frame.

## Behaviour every screen keeps

- Escape closes a dialog. Cancel or Close leaves the project unchanged.
- Enter presses the default button, and a value typed in an edit is
  committed first (Escape drops it). A confirmation names its default
  button (what Enter presses, framed as the default) in one table,
  `kit_window::Question`. The default button follows the observed
  reference behaviour, even when that is Yes on a destructive action;
  where not observed, No.
- A command that changes project data is one undo step.
- A dialog is dragged by its title bar and stays on screen.
- Controls that cannot act are shown disabled or left out. No dead buttons,
  with one exception: the "add" buttons of the details and record tabs
  (Add, Assign, Add from Template, Add Resource, Add Role) stay enabled
  when there is nothing to pick, as in the reference, and open their picker
  empty (`details::form::add_button`).
- Words are plain and short. Captions that open another dialog end in "...".

## Guards

Drift fails a test, so run the sweeps after any UI change
(`cargo test -p openxer-desktop -- full_ui_sweep dialog_formatting_sweep`):

- `dialog_sweep.rs` opens every dialog and checks that the shared shell drew
  its frame, that standard buttons are 108 x 25, and the button order (a
  column or row drawn in a declared observed order is checked against that order). Dialogs
  still being moved onto the kit are named in `NOT_YET_CONVERTED`; that list
  only ever shrinks.
- The same file's source check rejects a bare `egui::Area` or a private window
  wrapper outside the kit, and `kit_controls_are_written_once` rejects a
  second button column, move pad, tab list or message box.
- `panes::histogram::tests::one_histogram_painter` fails when a second
  cumulative-curve painter appears outside `panes/histogram.rs`.
- `chat::tests::chat_controls_are_written_once` fails on a second roll-up or
  size grip painter, Markdown reader or text builder, conversation model,
  Activity ID linker, pinned message list or Markdown table layout or painter.
- `details::notes_pane::tests::the_notebook_tabs_share_one_pane` fails when
  a Notebook tab draws its own topic pane.
- `views::headless::projects_details_tabs_show_dividers_and_switch` and
  `usage_list_tests::row_lines_stop_at_the_last_column_like_the_activity_table`
  pin the tab strip and the table against the Activities page.

When you add a shared control, add its row to the table above and a guard
that would catch a second copy.

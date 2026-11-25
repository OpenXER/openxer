//! The Available Options header menu shared by the Columns dialog and the
//! Customize ... Columns dialogs of the Activity Details grids. Clicking the
//! header opens a small popup: Group and Sort By > List / Category. List is
//! one flat alphabetical list of every available column; Category is the
//! grouped tree (in the Details grids, whose columns have no groups, the
//! grid's own order). The choice is a workspace preference.
//!
//! The popup's layout is built in the native popup style from the table
//! context menu.
use super::*;
use chrome::Item;

/// Preference row holding the choice.
const KEY: &str = "available_options_view_v1";

/// How the Available Options list is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AvailableView {
    /// The tree of group rows (the default).
    #[default]
    Category,
    /// One flat list sorted by title.
    List,
}

impl AvailableView {
    fn name(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::List => "list",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "category" => Some(Self::Category),
            "list" => Some(Self::List),
            _ => None,
        }
    }

    pub(crate) fn is_list(self) -> bool {
        self == Self::List
    }
}

/// Sort `items` by title, ignoring case; equal titles keep their order.
pub(crate) fn alphabetical<T>(items: &mut [T], title: impl Fn(&T) -> &str) {
    items.sort_by_cached_key(|item| title(item).to_lowercase());
}

fn state_id() -> egui::Id {
    egui::Id::new("available_options_menu")
}

/// Whether the header menu is open. Dialogs read this before drawing, so
/// that Escape closes the menu first and the dialog next.
pub(crate) fn menu_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<egui::Id>(state_id()))
        .is_some_and(|id| egui::Popup::is_id_open(ctx, id))
}

/// Closes the header menu (a dialog that closes takes its menu along).
pub(crate) fn close_menu(ctx: &egui::Context) {
    if let Some(id) = ctx.data(|d| d.get_temp::<egui::Id>(state_id())) {
        egui::Popup::close_id(ctx, id);
    }
}

/// The popup under a header: a click on `header` opens it at the header's
/// bottom-left corner. Returns the view picked this frame. The menu's
/// control rectangles are added to `actions` as
/// `available_view_group_sort`, `available_view_list` and
/// `available_view_category`.
pub(crate) fn header_menu(
    header: &egui::Response,
    current: AvailableView,
    actions: &mut Vec<(String, Rect)>,
) -> Option<AvailableView> {
    let ctx = header.ctx.clone();
    let id = egui::Popup::default_response_id(header);
    ctx.data_mut(|d| d.insert_temp(state_id(), id));
    let mut chosen = None;
    egui::Popup::menu(header)
        .style(chrome::popup_menu_style)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(140.);
            let (group, _) = chrome::submenu(ui, Item::new("Group and Sort By"), |ui| {
                for (view, key, label) in [
                    (AvailableView::List, "available_view_list", "List"),
                    (
                        AvailableView::Category,
                        "available_view_category",
                        "Category",
                    ),
                ] {
                    let item = chrome::menu_item(ui, Item::new(label).checked(current == view));
                    actions.push((key.into(), item.rect));
                    if item.clicked() {
                        chosen = Some(view);
                    }
                }
            });
            actions.push(("available_view_group_sort".into(), group.rect));
        });
    if chosen.is_some() {
        egui::Popup::close_id(&ctx, id);
    }
    chosen
}

impl App {
    /// Read the choice from the workspace preferences.
    pub(crate) fn restore_available_view(&mut self) {
        if let Ok(Some(text)) = self.store.preference(KEY) {
            self.available_view = AvailableView::parse(&text).unwrap_or_default();
        }
    }

    /// Use `view` from now on and keep it in the workspace.
    pub(crate) fn set_available_view(&mut self, view: AvailableView) {
        if view == self.available_view {
            return;
        }
        match self.store.set_preference(KEY, view.name()) {
            Ok(()) => self.available_view = view,
            Err(e) => self.error(format!("Cannot save the Available Options view: {e}")),
        }
    }
}

#[cfg(test)]
mod tests;

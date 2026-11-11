//! What a screen reader reads in the main window. Each pane is a group named by its folder whose cursor row is the
//! focused item (the window keeps keyboard focus on its root, so the row is reported as its active descendant). The
//! group is not a list: only the cursor row is a node, so a list would be read as holding one item.
//! The rows themselves are painted by the list, grid and columns elements; these nodes draw nothing and take no
//! room, so every view reports the same way.
use crate::pane::states::Kind;
use crate::state::Shell;
use flea::i18n::I18n;
use gpui::{div, prelude::*, AnyElement, ElementId, Role, SharedString, Stateful, Div};

/// The name a folder is read by: its last part ("Downloads"), the whole label for the root or a named place.
pub(crate) fn folder_name(label: &str) -> String {
    let trimmed = label.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some((_, last)) if !last.is_empty() => last.to_owned(),
        _ if label.is_empty() => String::new(),
        _ if trimmed.is_empty() => "/".to_owned(),
        _ => trimmed.to_owned(),
    }
}

/// The row's type as the list's Kind column says it, else Folder or File.
fn row_kind(row: &flea::protocol::RowFacts, catalog: &I18n) -> String {
    if !row.kind.is_empty() {
        row.kind.clone()
    } else if row.dir {
        catalog.tr("dialogs.file.properties.kind_folder")
    } else {
        catalog.tr("dialogs.file.properties.kind_file")
    }
}

/// A list's held rows (the ones near the view; a long folder holds only some) as a screen reader reads them: each
/// by name and type, selected ones saying so, and the cursor row as the focused item. Every row is a node so the
/// cursor moves between rows that stay, and a screen reader reads just the new row. `shown` is a filter's rows
/// (None while every row shows); `selected` answers for a listing index.
pub(crate) fn list_rows(
    key: &str,
    held: &crate::list::pane::Held,
    cursor: Option<usize>,
    shown: Option<&[usize]>,
    selected: impl Fn(usize) -> bool,
    catalog: &I18n,
) -> Vec<Stateful<Div>> {
    let end = held.start + held.rows.len();
    let place = |position: usize, index: usize, count: usize| {
        let row = held.row(index)?;
        // Keyed by folder and name: a refresh that keeps a file keeps its node.
        let id = ElementId::Name(format!("a11y-row-{key}/{}", row.name).into());
        Some(
            div()
                .id(id)
                .role(Role::ListBoxOption)
                .aria_label(row.name.clone())
                .aria_description(row_kind(row, catalog))
                // only a selected row says so: "not selected" on every row is noise
                .when(selected(index), |option| option.aria_selected(true))
                .aria_position_in_set(position + 1)
                .aria_size_of_set(count)
                .when(cursor == Some(index), |option| option.aria_active_descendant()),
        )
    };
    match shown {
        Some(shown) => shown
            .iter()
            .enumerate()
            .filter(|(_, &index)| (held.start..end).contains(&index))
            .filter_map(|(position, &index)| place(position, index, shown.len()))
            .collect(),
        None => (held.start..end).filter_map(|index| place(index, index, held.total)).collect(),
    }
}

impl Shell {
    /// One group per shown pane holding its held rows; the focused pane's cursor row is the focused item.
    pub(crate) fn accessible_panes(&self, window: &gpui::Window, cx: &gpui::App) -> Vec<AnyElement> {
        let catalog = &self.locale.catalog;
        // the rows are built only while a screen reader listens; the rest costs nothing
        let active = window.is_a11y_active();
        // Quick Look keeps the keyboard on the window: while its card is open, the card is the focused item.
        if let Some(look) = self.quick_look.as_ref().filter(|look| look.view.read(cx).is_open()) {
            let name = self.panes[look.side]
                .as_ref()
                .zip(look.row)
                .and_then(|(host, row)| host.pane.held.row(row))
                .map(|row| row.name.clone())
                .unwrap_or_default();
            return vec![div()
                .id("a11y-quick-look")
                .role(Role::Dialog)
                .aria_label(format!("{}, {name}", catalog.tr("keys.action.preview")))
                .absolute()
                .size_0()
                .aria_active_descendant()
                .into_any_element()];
        }
        // Renaming edits in the row while the window keeps the keyboard: the field is the focused item.
        if let Some(edit) = self.rename.edit.as_ref().filter(|edit| edit.entry.editing) {
            return vec![text_field(div().id("a11y-rename"), catalog.tr("keys.action.rename"), &edit.entry)
                .absolute()
                .size_0()
                .aria_active_descendant()
                .into_any_element()];
        }
        if let Some(menu) = self.accessible_menu() {
            return vec![menu.into_any_element()];
        }
        let dual = self.view == super::startup::View::Dual;
        let mut out = Vec::new();
        for (side, host) in self.panes.iter().enumerate() {
            let Some(host) = host else { continue };
            if side == 1 && !dual {
                continue;
            }
            let focused = side == self.focus_side;
            let ready = host.pane.state.kind == Kind::Ready;
            let rows = if ready && active {
                list_rows(
                    &format!("{side}-{}", host.path),
                    &host.pane.held,
                    focused.then_some(host.pane.cursor),
                    host.filter.shown.as_deref(),
                    |index| host.selected.binary_search(&index).is_ok(),
                    catalog,
                )
            } else {
                Vec::new()
            };
            let empty = focused && (!ready || host.pane.held.row(host.pane.cursor).is_none());
            // Loading, empty or unreadable: a line saying so is the focused item.
            let state = empty.then(|| {
                let message = match host.state_text.message.as_str() {
                    "" => catalog.tr("window.loading"),
                    message => message.to_owned(),
                };
                div()
                    .id("a11y-state")
                    .role(Role::Label)
                    // a label's text is its value
                    .aria_value(message)
                    .aria_active_descendant()
            });
            out.push(
                div()
                    .id(("a11y-pane", side))
                    .role(Role::Group)
                    .aria_label(folder_name(&self.path_labels[side]))
                    .absolute()
                    .size_0()
                    .children(state)
                    .children(rows)
                    .into_any_element(),
            );
        }
        out
    }
}

impl Shell {
    /// An open menu keeps the keyboard on the window too: its card (the open flyout's while one is open) is a menu
    /// whose cursor row is the focused item.
    fn accessible_menu(&self) -> Option<Stateful<Div>> {
        use gpui::accesskit::Toggled;
        let menu = self.menu.menu.borrow();
        let card = menu.flyout.as_ref().map(|flyout| &flyout.card).or(menu.card.as_ref())?;
        let catalog = &self.locale.catalog;
        let items = card.entries.iter().enumerate().filter(|(_, entry)| !entry.separator);
        let count = items.clone().count();
        let items = items.enumerate().map(|(position, (index, entry))| {
            let disabled = entry.disabled;
            div()
                .id(("a11y-menu-item", index))
                .role(match entry.check {
                    Some(_) => Role::MenuItemCheckBox,
                    None => Role::MenuItem,
                })
                .aria_label(entry.label.as_ref().map(|label| label.resolve(catalog)).unwrap_or_default())
                .when_some(entry.hint.as_ref(), |item, hint| item.aria_keyshortcuts(hint.resolve(catalog)))
                .when_some(entry.check, |item, on| {
                    item.aria_toggled(if on { Toggled::True } else { Toggled::False })
                })
                .when(disabled, |item| {
                    item.a11y_synthetic_children(|builder| builder.parent_node().set_disabled())
                })
                .aria_position_in_set(position + 1)
                .aria_size_of_set(count)
                .when(card.cursor == Some(index), |item| item.aria_active_descendant())
        });
        Some(
            div()
                .id(if menu.flyout.is_some() { "a11y-flyout" } else { "a11y-menu" })
                .role(Role::Menu)
                .absolute()
                .size_0()
                .children(items.collect::<Vec<_>>()),
        )
    }
}

/// The footer's announcement: the folder and its count after a folder loads, else the count or selection alone.
pub(crate) fn announcement(said_path: &str, path: &str, counts: &str) -> String {
    if counts.is_empty() {
        String::new()
    } else if said_path != path {
        format!("{}, {counts}", folder_name(path))
    } else {
        counts.to_owned()
    }
}

/// A live node: a screen reader speaks its label each time the label changes, and nothing while it stays.
pub(crate) fn live(id: &'static str, label: SharedString) -> Stateful<Div> {
    div().id(id).role(Role::Group).absolute().size_0().a11y_synthetic_children(move |builder| {
        let mut node = gpui::accesskit::Node::new(Role::Status);
        node.set_live(gpui::accesskit::Live::Polite);
        if !label.is_empty() {
            node.set_label(label.to_string());
        }
        let id = builder.synthetic_node_id(0);
        builder.push_child(id, node);
    })
}

/// A one-line text field as a screen reader reads it: its name, its text (bullets for a password) and the caret
/// or selection, so typing, moving and selecting are spoken.
pub(crate) fn text_field(field: Stateful<Div>, name: String, entry: &crate::path::entry::Entry) -> Stateful<Div> {
    let text: String = if entry.kind == crate::path::entry::Kind::Private {
        "\u{2022}".repeat(entry.text.chars().count())
    } else {
        entry.text.clone()
    };
    let chars = |byte: usize| entry.text.get(..byte).map_or(0, |head| head.chars().count());
    let (anchor, focus) = if entry.reversed {
        (chars(entry.selected.end), chars(entry.selected.start))
    } else {
        (chars(entry.selected.start), chars(entry.selected.end))
    };
    field.role(Role::TextInput).aria_label(name).a11y_synthetic_children(move |builder| {
        use gpui::accesskit::{Node, TextPosition, TextSelection};
        let mut run = Node::new(Role::TextRun);
        run.set_character_lengths(text.chars().map(|c| c.len_utf8() as u8).collect::<Vec<_>>());
        run.set_value(text.clone());
        let id = builder.synthetic_node_id("text");
        builder.push_child(id, run);
        let at = |character_index| TextPosition { node: id, character_index };
        let parent = builder.parent_node();
        parent.set_value(text);
        parent.set_text_selection(TextSelection { anchor: at(anchor), focus: at(focus) });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names() {
        assert_eq!(folder_name("/home/alex/Downloads"), "Downloads");
        assert_eq!(folder_name("/home/alex/Downloads/"), "Downloads");
        assert_eq!(folder_name("/"), "/");
        assert_eq!(folder_name("Recent"), "Recent");
        assert_eq!(folder_name(""), "");
    }

    #[test]
    fn announcements() {
        assert_eq!(announcement("", "/home/alex/Downloads", "42 items"), "Downloads, 42 items");
        assert_eq!(announcement("/home/alex/Downloads", "/home/alex/Downloads", "3 of 42 selected"), "3 of 42 selected");
        assert_eq!(announcement("/home/alex", "/home/alex/Downloads", ""), "");
    }
}

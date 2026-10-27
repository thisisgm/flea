//! Remembered display roots follow persisted network labels without changing a navigation path.
use super::*;
use crate::path::crumbs::{relabel_aliases, remember_alias};
use flea::ui_logic_pl2::mounts;

impl Shell {
    pub(super) fn network_remember_alias(&mut self, path: &str, uri: &str, label: &str) {
        let key = mounts::normalize(uri);
        let label = self
            .rail
            .as_ref()
            .and_then(|join| join.network.as_ref())
            .and_then(|network| network.saved_labels.get(&key))
            .map_or(label, String::as_str);
        self.chrome.aliases = remember_alias(&self.chrome.aliases, path, label, Some(&key));
    }

    pub(super) fn network_alias_labels(&mut self, bookmarks: &str) {
        let labels = flea::ui_logic_pl1::mounts::non_file_bookmarks(bookmarks)
            .into_iter()
            .map(|place| (mounts::normalize(&place.uri), place.label))
            .collect();
        self.chrome.aliases = relabel_aliases(&self.chrome.aliases, &labels);
        if let Some(network) = self.network() {
            network.saved_labels = labels;
        }
    }

    pub(super) fn network_alias_saved(&mut self, edit: &bookmarks::Edit) {
        let Some(network) = self.network() else {
            return;
        };
        // A read started before this save cannot publish the old label after the write landed.
        network.rows_generation += 1;
        match edit {
            bookmarks::Edit::Rename { uri, label }
            | bookmarks::Edit::Replace { uri, label, .. } => {
                network
                    .saved_labels
                    .insert(mounts::normalize(uri), label.clone());
            }
            bookmarks::Edit::Remove { uri } => {
                network.saved_labels.remove(&mounts::normalize(uri));
            }
        }
        let labels = network.saved_labels.clone();
        self.chrome.aliases = relabel_aliases(&self.chrome.aliases, &labels);
    }
}

//! Network mount aliases follow persisted bookmark and favourite labels, never live provider labels.
use super::crumbs::{relabel_aliases, remember_alias};
use crate::state::Shell;
use flea::{jsondoc::Json, ui_logic_pl1::mounts};
use std::collections::HashMap;

pub(crate) fn saved_labels(bookmarks: &str) -> HashMap<String, String> {
    mounts::non_file_bookmarks(bookmarks)
        .into_iter()
        .map(|place| (mounts::normalize(&place.uri), place.label))
        .collect()
}

impl Shell {
    fn alias_labels(&self, state: &Json) -> HashMap<String, String> {
        let mut labels = self.chrome.saved_labels.clone();
        if let Some(favourites) = state
            .get("places")
            .and_then(|places| places.get("favourites"))
            .and_then(Json::as_array)
        {
            for favourite in favourites {
                if let (Some(path), Some(label)) = (
                    favourite.get("path").and_then(Json::as_str),
                    favourite.get("label").and_then(Json::as_str),
                ) {
                    if !path.is_empty() && !label.is_empty() {
                        labels.insert(mounts::normalize(path), label.into());
                    }
                }
            }
        }
        labels
    }

    pub(crate) fn relabel_path_aliases(&mut self, state: &Json) {
        self.chrome.aliases = relabel_aliases(&self.chrome.aliases, &self.alias_labels(state));
    }

    pub(crate) fn path_saved_labels(&mut self, labels: HashMap<String, String>) -> bool {
        self.chrome.saved_labels = labels;
        let next = relabel_aliases(
            &self.chrome.aliases,
            &self.alias_labels(&self.launch.settings.data),
        );
        if next == self.chrome.aliases {
            return false;
        }
        self.chrome.aliases = next;
        true
    }

    pub(crate) fn remember_path_alias(&mut self, path: &str, label: &str, uri: &str) {
        let key = mounts::normalize(uri);
        let labels = self.alias_labels(&self.launch.settings.data);
        let label = labels.get(&key).map_or(label, String::as_str);
        self.chrome.aliases = remember_alias(&self.chrome.aliases, path, label, Some(&key));
    }
}

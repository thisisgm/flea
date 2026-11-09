//! The backend's captured filenames let MIME rules cover selected rows outside the held viewport.
use crate::state::Shell;
use flea::{jsondoc::Json, ui_logic_context_menus::build, ui_logic_custom_actions::plugins::Item};

pub(super) struct Selection {
    side: usize,
    id: u64,
    names: Vec<String>,
    modes: Vec<u32>,
}

impl Shell {
    /// True when a current capture's selected names were stored, so the open card must rebuild its rows.
    // Sample input: {"t":"menuaction","op":"snapshot","id":7,"ok":true,"names":["notes.txt","Photos"],"kinds":[33188,16877]}
    pub(crate) fn plugin_selection_reply(&mut self, side: usize, reply: &Json) -> bool {
        if reply.get("t").and_then(Json::as_str) != Some("menuaction")
            || reply.get("op").and_then(Json::as_str) != Some("snapshot")
            || reply.get("ok").and_then(Json::as_bool) != Some(true)
        {
            return false;
        }
        let Some(id) = reply.get("id").and_then(|v| match v {
            Json::Num(n) => n.parse::<u64>().ok(),
            _ => None,
        }) else {
            return false;
        };
        let Some(capture) = self.menu.captured.as_ref().filter(|c| {
            c.id == id
                && c.side == side
                && !c.background
                && c.rows.len() <= crate::g2::plugins::limits::SELECTION
        }) else {
            return false;
        };
        let Some(names) = reply
            .get("names")
            .and_then(Json::as_array)
            .filter(|names| names.len() == capture.rows.len())
        else {
            return false;
        };
        let Some(modes) = reply
            .get("kinds")
            .and_then(Json::as_array)
            .filter(|modes| modes.len() == names.len())
        else {
            return false;
        };
        let Some(names) = names
            .iter()
            .map(|name| {
                name.as_str()
                    .filter(|name| {
                        !name.is_empty()
                            && name.len() <= crate::g2::plugins::limits::TEXT
                            && !name.contains(['/', '\0'])
                            && !matches!(*name, "." | "..")
                    })
                    .map(str::to_owned)
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let Some(modes) = modes
            .iter()
            .map(|mode| match mode {
                Json::Num(n) => n.parse::<u32>().ok(),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        self.menu.plugin_runtime.selection = Some(Selection {
            side,
            id,
            names,
            modes,
        });
        true
    }

    pub(super) fn captured_plugin_items(&self, side: usize, id: u64) -> Option<Vec<Item>> {
        let selection = self
            .menu
            .plugin_runtime
            .selection
            .as_ref()
            .filter(|s| s.side == side && s.id == id)?;
        Some(
            selection
                .names
                .iter()
                .zip(&selection.modes)
                .map(|(name, mode)| {
                    let folder = mode & build::S_IFMT == build::S_IFDIR;
                    Item {
                        folder,
                        mime: if folder {
                            "inode/directory".into()
                        } else {
                            self.menu
                                .plugin_runtime
                                .mimes
                                .as_ref()
                                .and_then(|db| db.lookup(name))
                                .unwrap_or("application/octet-stream")
                                .into()
                        },
                    }
                })
                .collect(),
        )
    }
}

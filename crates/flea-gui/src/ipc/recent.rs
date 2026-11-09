//! The active pane's Recent readers, mounted even while the history is closed.
use super::Adapter;
use flea::{ipc::wire::Reply, ui_logic_rail_places::recentmode::RecentMode};

pub(crate) struct RecentAdapter<'a>(pub &'a RecentMode);

impl Adapter for RecentAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        Some(Reply::value(match function {
            "recentMode" => {
                if self.0.on {
                    "results"
                } else {
                    ""
                }
            }
            "recentFrom" => self.0.from.as_str(),
            _ => return None,
        }))
    }
}

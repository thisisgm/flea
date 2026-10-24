//! The status strip's IPC readers over a host snapshot: ui/Ipc.qml names, producer and theme adapters not served here.
use super::Adapter;
use flea::ipc::wire::Reply;

/// One frame's host-composed status words, translated before they arrive here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub path: String,
    pub total: usize,
    pub selected_count: usize,
    pub listing_state: String,
    pub filesystem: String,
    pub counts: String,
    pub primary: String,
    pub secondary: String,
    pub is_error: bool,
    pub detail: String,
    pub busy: bool,
    pub activity_state: String,
}

pub struct StripsAdapter<'a> {
    pub snapshot: &'a Snapshot,
}

impl StripsAdapter<'_> {
    fn footer(&self) -> String {
        let snapshot = self.snapshot;
        let mut out = String::from("{");
        out.push_str(&format!("\"path\":{},", quoted(&snapshot.path)));
        out.push_str(&format!("\"total\":{},", snapshot.total));
        out.push_str(&format!("\"selected\":{},", snapshot.selected_count));
        out.push_str(&format!(
            "\"listingState\":{},",
            quoted(&snapshot.listing_state)
        ));
        out.push_str(&format!("\"filesystem\":{},", quoted(&snapshot.filesystem)));
        out.push_str(&format!("\"counts\":{}", quoted(&snapshot.counts)));
        out.push('}');
        out
    }
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", flea::json::escape(text))
}

impl Adapter for StripsAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let snapshot = self.snapshot;
        let value = match function {
            "statusPrimary" => snapshot.primary.clone(),
            "statusSecondary" => snapshot.secondary.clone(),
            "statusError" => snapshot.is_error.to_string(),
            "statusDetail" => snapshot.detail.clone(),
            "statusBusy" => snapshot.busy.to_string(),
            "statusActivityState" => snapshot.activity_state.clone(),
            "statusFooterState" => self.footer(),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

//! The shebang probe's state as `menuState` reports it: null or false until the producer that asks fills it.
use flea::json::escape;

/// What the probe's reply carried, as ui/IpcMenuState.qml's `shebangReply` holds it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShebangReply {
    pub pane: String,
    pub path: String,
    pub id: u64,
    pub has_shebang: bool,
}

/// The one seam the Make executable producer (U23) fills; every field stays null or false until it does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shebang {
    // The pane's name as ui.sh compares it with the reply's pane, such as pane-B.
    pub pane: Option<String>,
    // The path the last two-byte read asked about.
    pub asked: Option<String>,
    // The id of that ask, which rises with every probe.
    pub id: Option<u64>,
    // The reply that landed for the ask in flight.
    pub reply: Option<ShebangReply>,
    // Whether the cursor row has a shebang.
    pub has: bool,
}

fn text(value: &Option<String>) -> String {
    value
        .as_ref()
        .map_or("null".into(), |text| format!("\"{}\"", escape(text)))
}

impl Shebang {
    /// The `pane` and `shebang*` members of `menuState`, without the braces.
    pub fn members(&self) -> String {
        let reply = self.reply.as_ref().map_or("null".into(), |reply| {
            format!(
                "{{\"pane\":\"{}\",\"path\":\"{}\",\"id\":{},\"hasShebang\":{}}}",
                escape(&reply.pane),
                escape(&reply.path),
                reply.id,
                reply.has_shebang
            )
        });
        format!(
            "\"pane\":{},\"shebangAsked\":{},\"shebangId\":{},\"shebangReply\":{reply},\"shebangHas\":{}",
            text(&self.pane),
            text(&self.asked),
            self.id.map_or("null".into(), |id| id.to_string()),
            self.has
        )
    }
}

//! Join seams: each contract another unit owns, as the narrowest call the Trash pane makes into it.
use flea::ui_logic_trash_browser::host::Item;

/// The delete card for op1's dialog shell, every word resolved; the destructive button answers with the token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    pub body: String,
    pub confirm: String,
    pub cancel: String,
    pub token: u64,
    pub empty: bool,
}

/// Where a menu opens, in pane coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct At {
    pub x: f32,
    pub y: f32,
}

/// Everything outside the Trash pane it talks to; each method names the unit that owns the real contract.
pub trait Outside {
    fn focus(&mut self) {}
    /// G1 backend session: one raw trashbrowse, trashthumb or trashthumbcancel line.
    fn send(&mut self, line: String);
    /// op1 ConfirmedOperation: open the one delete card, or close it with None; a close from here is not an answer, so op1 never echoes it as Cancel.
    fn card(&mut self, card: Option<Card>);
    /// mn1 menu primitive: these rows at this point; the pick comes back through TrashPane::menu_action.
    fn menu(&mut self, items: Vec<Item>, at: At);
    /// G1.13 status slot: an arm prompt, a refusal or a finished operation.
    fn status(&mut self, text: String, error: bool);
    fn arm(&mut self, text: String) {
        self.status(text, false);
    }
    fn result(&mut self, headline: String, detail: String, error: bool);
    /// TrashMonitor onFailed: the watch or its count failed; the window decides where the sentence shows.
    fn monitor_failed(&mut self, text: String) {
        self.status(text, true);
    }
    /// G1.13 status slot: Escape is offered here first; true when it dismissed a showing message, so Trash stays open.
    fn escape_taken(&mut self) -> bool;
    /// G1.11 rail host: the Trash row's count and the Tab hop into the rail.
    fn count(&mut self, total: usize);
    fn focus_rail(&mut self);
    /// G1.10 window: back to the listing Trash covers, and window actions Trash forwards.
    fn back(&mut self);
    fn action(&mut self, name: String);
    /// Persist only a completed sweep's local day through the window settings writer.
    fn swept(&mut self, _day: i64) {}
}

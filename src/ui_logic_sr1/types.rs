// The session's vocabulary: its modes and scopes, what it reads from a pane and what it asks the pane to do.
use super::wire::Request;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Typing,
    Results,
}

// The three scopes Tab cycles; Home is the default and walks home only from inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    Folder,
    #[default]
    Home,
    Everywhere,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Names,
    Contents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListingState {
    Ready,
    Loading,
    Empty,
}

// A key on the query line, already read from the platform event by the front end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key<'a> {
    Enter,
    Escape,
    Backspace,
    Tab,
    Contents,
    Text(&'a str),
    // The clipboard's text at the paste chord, None when it holds none.
    Paste(Option<&'a str>),
    Other,
}

// What the session reads from its pane; the pane keeps the cursor, the rows and Recent.
#[derive(Clone, Copy, Debug)]
pub struct Pane<'a> {
    pub path: &'a str,
    pub home: &'a str,
    pub hidden: bool,
    pub count: usize,
    pub sizes: usize,
    pub cursor_name: Option<&'a str>,
    // Some while the pane shows Recent, naming the folder Recent was opened from.
    pub recent: Option<&'a str>,
    pub now: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Send(Request),
    // The pane leaves Recent, restores its sort and stands in this folder.
    LeaveRecent { path: String },
    // A new session restarts the landed generation, so its first answer parks on its best match.
    ResetLanded,
    // Open the folder without history, holding the walk's drawn state until it lands, with the cursor file armed.
    Relist { path: String, select: String },
    Reveal { folder: String, select: String },
    RestartSettle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamps {
    pub gen: u64,
    pub key_at: u64,
    pub first_rows_at: u64,
    pub settled_at: u64,
}

//! Join seams: the plain data and the one trait this unit needs from surfaces other units own.

/// The backend pipe for the rclone and jobs requests (int4 rclone producer, through G1.02's backend line).
pub trait Wire {
    /// Queues one request line; false when the backend is gone.
    fn send(&self, line: String) -> bool;
}

/// Send with rclone's row in the file menu, for the menu primitive (mn1) to place before Move to Dropbox (sd1's sharing rows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuRow {
    /// Settings > Menus storage id in `menu.hidden`.
    pub id: &'static str,
    pub label: String,
    /// Monochrome atlas mark name.
    pub mark: &'static str,
    pub chevron: bool,
    /// Lit in grey while its flyout is open, so the path through the menu stays lit.
    pub lit: bool,
}

/// One remote in the flyout, by rclone's own name, under the server mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlyoutRow {
    pub label: String,
    pub mark: &'static str,
}

/// The send card's contents for the dialog shell (op1): Copy To's one-field card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    /// DialogField draws its label in capitals above the field.
    pub field_label: String,
    pub value: String,
    /// The remembered folder is selected so typing replaces it.
    pub selected_all: bool,
    /// The muted or error line under the field, when there is one.
    pub error: Option<String>,
    pub cancel: String,
    pub primary: String,
    /// Send takes a press only with a folder and while no request is out.
    pub can_submit: bool,
}

//! The GUI's plugin bounds, shared by the host, the menus, the places and the routes.

/// Requests the app keeps in flight to the plugin owner, which runs as many lines at once.
pub(crate) const PENDING: usize = 16;
/// Rows or paths one menu or place operation may name.
pub(crate) const SELECTION: usize = 512;
/// Bytes of one selected name, place path or plugin notice that Flea keeps.
pub(crate) const TEXT: usize = 4096;
/// Characters of a send target's id, label or state.
pub(crate) const TARGET_TEXT: usize = 512;
/// Bytes of one plugin entry name, the longest filename Linux allows.
pub(crate) const NAME_BYTES: usize = 255;
/// Rows one plugin place window answers.
pub(crate) const WINDOW_ROWS: usize = 1024;
/// Menu snapshots one place keeps before it forgets them all.
pub(crate) const SNAPSHOTS: usize = 32;
/// Pane commands queued for a plugin place's owner.
pub(crate) const ROUTE_QUEUE: usize = 64;
/// A send target's refresh interval when it names none, and the range it is held to, in seconds.
pub(crate) const REFRESH_SECONDS: f64 = 10.;
pub(crate) const REFRESH_RANGE: (f64, f64) = (1., 3600.);
/// How soon a refused rail or target request is asked again.
pub(crate) const RETRY: std::time::Duration = std::time::Duration::from_secs(1);

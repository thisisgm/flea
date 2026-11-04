//! The ShareBrowser cursor, the ShareLink handshake and Taildrop words, from ui/ShareBrowser.qml and ui/ShareLink.qml.
use crate::i18n::I18n;

// The overlay over the listing slot: open flips instantly so the IPC read never races the fade.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShareBrowser {
    pub active: bool,
    pub base_uri: String,
    pub base_label: String,
    pub shares: Vec<String>,
    pub cursor: usize,
}

impl ShareBrowser {
    pub fn open(&mut self, uri: &str, label: &str, names: &[String]) {
        self.active = true;
        self.base_uri = uri.into();
        self.base_label = label.into();
        self.shares = names.to_vec();
        self.cursor = 0;
    }

    // Idempotent, so wiring it to every navigation costs nothing once already shut.
    pub fn close(&mut self) -> bool {
        if !self.active {
            return false;
        }
        self.active = false;
        true
    }

    pub fn move_cursor(&mut self, delta: i64) {
        if self.shares.is_empty() {
            return;
        }
        let next = self.cursor as i64 + delta;
        self.cursor = next.clamp(0, self.shares.len() as i64 - 1) as usize;
    }

    // The resolved share uri and name, the shape openShare() already takes for a bookmarked share.
    pub fn activate(&self) -> Option<(String, String)> {
        let name = self.shares.get(self.cursor)?;
        Some((join_uri(&self.base_uri, name), name.clone()))
    }

    // A row click makes that row the cursor and its share the pick; a gap or shut overlay picks nothing.
    pub fn pick(&mut self, row: Option<usize>) -> Option<(String, String)> {
        let row = row.filter(|row| self.active && *row < self.shares.len())?;
        self.cursor = row;
        self.activate()
    }

    // One share name per line, in cursor order; ui/Ipc.qml answers the list kept after a close too.
    pub fn entries_text(&self) -> String {
        self.shares.join("\n")
    }

    // The cursor kept as the overlay left it, as ui/Ipc.qml answers shareBrowserCursor.
    pub fn cursor_ipc(&self) -> i64 {
        self.cursor as i64
    }
}

fn join_uri(base: &str, name: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{name}/")
    } else {
        format!("{base}/{name}/")
    }
}

// The ShareLink copy handshake: one sharelink run, then one clipboard run, never overlapping.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShareLink {
    pub pending: Option<String>,
    pub clipboard_url: Option<String>,
}

impl ShareLink {
    // A second copy while one runs is refused; a non-absolute path never starts a run.
    pub fn copy(&mut self, path: &str) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("busy".into());
        }
        if !valid_path(path) {
            return Err("no-file".into());
        }
        self.pending = Some(path.into());
        self.clipboard_url = None;
        Ok(())
    }

    pub fn busy_reason(i18n: &I18n) -> String {
        i18n.tr("send.providers.share_busy")
    }

    pub fn no_file_reason(i18n: &I18n) -> String {
        i18n.tr("send.providers.share_no_file")
    }

    // The sharelink run ended: never started is "asked"; a non-zero exit or non-URL output fails make.
    pub fn share_done(&mut self, started: bool, exit_code: i32, output: &str) -> ShareStep {
        if self.pending.is_none() {
            return ShareStep::Idle;
        }
        if !started {
            self.pending = None;
            return ShareStep::Fail("ask".into());
        }
        let url = output.trim().to_string();
        if exit_code != 0 || !is_url(&url) {
            self.pending = None;
            return ShareStep::Fail("make".into());
        }
        self.clipboard_url = Some(url.clone());
        ShareStep::CopyClipboard(url)
    }

    pub fn clipboard_done(&mut self, ok: bool) -> ShareStep {
        if self.pending.is_none() {
            return ShareStep::Idle;
        }
        let url = self.clipboard_url.clone().unwrap_or_default();
        self.pending = None;
        self.clipboard_url = None;
        if ok && !url.is_empty() {
            ShareStep::Copied(url)
        } else {
            ShareStep::Fail("clipboard".into())
        }
    }

    pub fn fail_reason(i18n: &I18n, step: &str) -> String {
        let key = match step {
            "ask" => "send.providers.share_ask_failed",
            "make" => "send.providers.share_make_failed",
            _ => "send.providers.share_clipboard_failed",
        };
        i18n.tr(key)
    }
}

// What one finished ShareLink step asks of the host, which owns the processes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShareStep {
    Idle,
    CopyClipboard(String),
    Copied(String),
    Fail(String),
}

fn valid_path(path: &str) -> bool {
    path.starts_with('/') && !path.contains('\u{0}')
}

// Sample output: https://www.dropbox.com/s/example/file.txt?dl=0; exit-zero error text is no URL.
pub fn is_url(text: &str) -> bool {
    let rest = match text.strip_prefix("https://").or_else(|| text.strip_prefix("http://")) {
        Some(rest) => rest,
        None => return false,
    };
    if rest.is_empty() || rest.contains(char::is_whitespace) {
        return false;
    }
    !matches!(rest.find(['/', '?', '#']), Some(0))
}

// The dispatch sentence; the OEM script owns the delivered notification.
pub fn taildrop_sending(i18n: &I18n, peer_label: &str, path: &str) -> String {
    i18n.tr_args("send.providers.taildrop_sending", &[leaf(path), peer_label])
}

// A send that did not start or exited non-zero names its file and peer.
pub fn taildrop_failed(i18n: &I18n, peer_label: &str, path: &str) -> String {
    i18n.tr_args("send.providers.taildrop_failed", &[leaf(path), peer_label])
}

fn leaf(path: &str) -> &str {
    path.rfind('/').map_or(path, |cut| &path[cut + 1..])
}

// A refusal reads its stable code, never the English reason; the CLI diagnostic rides the argument slot.
pub fn taildrop_refusal(i18n: &I18n, code: &str, reason: &str) -> String {
    match code {
        "signedOut" => i18n.tr("send.providers.taildrop_signed_out"),
        "noPeers" => i18n.tr("send.providers.taildrop_no_peers"),
        "disabled" => i18n.tr("send.providers.taildrop_disabled"),
        "invalid" => i18n.tr("send.providers.taildrop_invalid"),
        "statusFailed" if reason.is_empty() => {
            let unchecked = i18n.tr("send.providers.taildrop_unchecked");
            i18n.tr_args("send.providers.taildrop_reason", &[&unchecked])
        }
        "statusFailed" => i18n.tr_args("send.providers.taildrop_reason", &[reason]),
        "stopped" if !reason.is_empty() => i18n.tr_args("send.providers.taildrop_reason", &[reason]),
        "" if reason == "checking" => i18n.tr("send.providers.taildrop_checking"),
        _ => i18n.tr("send.providers.taildrop_gone"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Paths;
    use std::path::PathBuf;

    fn english() -> I18n {
        let none = PathBuf::from("/nonexistent/flea-sd1");
        I18n::load(Paths { omarchy: none.clone(), language_file: none, catalogs: Vec::new() })
    }

    #[test]
    fn the_browser_opens_shut_moves_and_activates() {
        let mut browser = ShareBrowser::default();
        assert!(!browser.close());
        browser.open("smb://nas", "nas", &["media".to_string(), "docs".to_string()]);
        assert!(browser.active);
        assert_eq!(browser.entries_text(), "media\ndocs");
        assert_eq!(browser.cursor_ipc(), 0);
        browser.move_cursor(5);
        assert_eq!(browser.cursor_ipc(), 1);
        browser.move_cursor(-5);
        assert_eq!(browser.cursor_ipc(), 0);
        assert_eq!(browser.activate(), Some(("smb://nas/media/".to_string(), "media".to_string())));
        assert!(browser.close());
        assert!(!browser.active);
        assert_eq!(browser.entries_text(), "media\ndocs", "ui/Ipc.qml keeps the list after a close");
        assert_eq!(browser.cursor_ipc(), 0, "and the cursor it was left on");
    }

    #[test]
    fn a_trailing_slash_base_joins_once_and_an_empty_list_activates_nothing() {
        let mut browser = ShareBrowser::default();
        browser.open("smb://nas/", "nas", &[]);
        assert_eq!(browser.activate(), None);
        browser.open("smb://nas/", "nas", &["media".to_string()]);
        assert_eq!(browser.activate(), Some(("smb://nas/media/".to_string(), "media".to_string())));
    }

    // ui/ShareBrowser.qml:103-108: a clicked row takes the cursor and activates; a gap never does.
    #[test]
    fn a_click_picks_its_row_and_a_gap_picks_nothing() {
        let mut browser = ShareBrowser::default();
        assert_eq!(browser.pick(Some(0)), None, "a shut overlay picks nothing");
        browser.open("smb://nas", "nas", &["media".to_string(), "docs".to_string()]);
        assert_eq!(browser.pick(None), None, "a gap picks nothing");
        assert_eq!(browser.pick(Some(2)), None, "past the last share picks nothing");
        assert_eq!(browser.cursor, 0);
        assert_eq!(browser.pick(Some(1)), Some(("smb://nas/docs/".to_string(), "docs".to_string())));
        assert_eq!(browser.cursor, 1);
    }

    #[test]
    fn a_second_copy_waits_and_a_relative_path_never_starts() {
        let i18n = english();
        let mut link = ShareLink::default();
        assert!(link.copy("/home/gm/file.txt").is_ok());
        assert_eq!(link.copy("/home/gm/other.txt").unwrap_err(), "busy");
        assert_eq!(ShareLink::busy_reason(&i18n), "A share link is still being copied; try again when it finishes.");
        let mut fresh = ShareLink::default();
        assert_eq!(fresh.copy("relative.txt").unwrap_err(), "no-file");
        assert_eq!(ShareLink::no_file_reason(&i18n), "Flea could not find that file's location, so no share link was made.");
    }

    #[test]
    fn only_an_http_url_reaches_the_clipboard_step() {
        let mut link = ShareLink::default();
        assert!(link.copy("/home/gm/file.txt").is_ok());
        assert!(is_url("https://www.dropbox.com/s/example/file.txt?dl=0"));
        assert!(!is_url("Dropbox isn't running!"));
        assert!(!is_url(""));
        let url = "https://www.dropbox.com/s/example/file.txt?dl=0";
        assert_eq!(link.share_done(true, 0, url), ShareStep::CopyClipboard(url.into()));
        assert_eq!(link.clipboard_done(true), ShareStep::Copied(url.into()));
        let mut failed = ShareLink::default();
        assert!(failed.copy("/home/gm/file.txt").is_ok());
        assert_eq!(failed.share_done(false, 0, ""), ShareStep::Fail("ask".into()));
        let mut text = ShareLink::default();
        assert!(text.copy("/home/gm/file.txt").is_ok());
        assert_eq!(text.share_done(true, 0, "Dropbox isn't running!"), ShareStep::Fail("make".into()));
        let mut clip = ShareLink::default();
        assert!(clip.copy("/home/gm/file.txt").is_ok());
        assert_eq!(clip.share_done(true, 0, url), ShareStep::CopyClipboard(url.into()));
        assert_eq!(clip.clipboard_done(false), ShareStep::Fail("clipboard".into()));
        let i18n = english();
        assert_eq!(ShareLink::fail_reason(&i18n, "ask"), "Dropbox could not be asked for a share link.");
        assert_eq!(ShareLink::fail_reason(&i18n, "make"), "Dropbox could not make a share link for that file.");
        assert_eq!(ShareLink::fail_reason(&i18n, "clipboard"), "The share link could not be copied to the clipboard.");
    }

    // ui/ShareLink.qml:48-50 words a run that never started as "ask"; any non-zero exit is "make", even with a URL.
    #[test]
    fn a_nonzero_exit_makes_a_failure_even_with_a_url() {
        let url = "https://www.dropbox.com/s/example/file.txt?dl=0";
        let mut exited = ShareLink::default();
        assert!(exited.copy("/home/gm/file.txt").is_ok());
        assert_eq!(exited.share_done(true, 1, url), ShareStep::Fail("make".into()));
        let mut never = ShareLink::default();
        assert!(never.copy("/home/gm/file.txt").is_ok());
        assert_eq!(never.share_done(false, 1, url), ShareStep::Fail("ask".into()));
    }

    #[test]
    fn taildrop_says_the_dispatch_once_and_names_a_stale_peer() {
        let i18n = english();
        assert_eq!(taildrop_sending(&i18n, "laptop", "/home/gm/a.txt"), "Sending a.txt to laptop.");
        assert_eq!(taildrop_failed(&i18n, "laptop", "/home/gm/a.txt"), "Taildrop could not send a.txt to laptop.");
        assert_eq!(taildrop_refusal(&i18n, "signedOut", "signed out"), "Taildrop \u{b7} signed out");
        assert_eq!(taildrop_refusal(&i18n, "noPeers", "no peers"), "Taildrop \u{b7} no peers");
        assert_eq!(taildrop_refusal(&i18n, "statusFailed", "Permission denied"), "Taildrop \u{b7} Permission denied");
        assert_eq!(
            taildrop_refusal(&i18n, "statusFailed", ""),
            "Taildrop \u{b7} Tailscale could not be checked"
        );
        assert_eq!(taildrop_refusal(&i18n, "", "checking"), "Taildrop \u{b7} checking");
        assert_eq!(taildrop_refusal(&i18n, "disabled", "x"), "Taildrop \u{b7} disabled for this account");
        assert_eq!(taildrop_refusal(&i18n, "invalid", "x"), "Taildrop \u{b7} invalid status");
        assert_eq!(taildrop_refusal(&i18n, "stopped", "Tailscale is stopped"), "Taildrop \u{b7} Tailscale is stopped");
        assert_eq!(taildrop_refusal(&i18n, "stopped", ""), "That Taildrop peer is no longer available.");
        assert_eq!(taildrop_refusal(&i18n, "", ""), "That Taildrop peer is no longer available.");
    }
}

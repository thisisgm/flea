//! Dropbox account, membership and daemon status, ported from ui/js/Dropbox.js.
//! Sample account input: {"personal":{"path":"/home/gm/Dropbox"}}; personal precedes business.
use crate::jsondoc::{self, Json};

// What account() names for logic; the reason stays diagnostic data, never a translated word.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub path: String,
    pub reason: String,
    pub code: String,
}

// What status() names for logic; same diagnostic-data contract as Account.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub reason: String,
    pub code: String,
}

pub fn account(raw: &str, error: &str) -> Account {
    if !error.is_empty() {
        return Account { path: String::new(), reason: error.into(), code: "readFailed".into() };
    }
    if raw.is_empty() {
        return Account { path: String::new(), reason: "Dropbox is signed out".into(), code: "signedOut".into() };
    }
    let data = match jsondoc::parse(raw) {
        Ok(data) => data,
        Err(_) => return invalid_account(),
    };
    if !matches!(data, Json::Obj(_)) {
        return invalid_account();
    }
    for key in ["personal", "business"] {
        let Some(value) = data.get(key) else { continue };
        if !matches!(value, Json::Obj(_)) {
            continue;
        }
        match value.get("path").and_then(Json::as_str) {
            Some(path) if valid_root(path.trim_end_matches('/')) => {
                let path = path.trim_end_matches('/').to_string();
                return Account { path, reason: String::new(), code: String::new() };
            }
            _ => return invalid_folder(),
        }
    }
    Account { path: String::new(), reason: "Dropbox is signed out".into(), code: "signedOut".into() }
}

fn invalid_account() -> Account {
    Account { path: String::new(), reason: "Dropbox account details could not be read".into(), code: "invalid".into() }
}

fn invalid_folder() -> Account {
    Account { path: String::new(), reason: "The Dropbox folder could not be found".into(), code: "invalid".into() }
}

// An account root is absolute and never the filesystem root; anything else refuses the account.
fn valid_root(path: &str) -> bool {
    path.starts_with('/') && path != "/" && !path.contains('\u{0}')
}

pub fn contains(root: &str, path: &str) -> bool {
    !root.is_empty()
        && root.len() > 1
        && root.starts_with('/')
        && !path.is_empty()
        && (path == root || path.starts_with(&format!("{root}/")))
}

// Sample daemon output: dropbox-cli status prints "Up to date" or "Dropbox isn't running!",
// either with exit status zero; only the words tell readiness apart.
pub fn status(output: &str, exit_code: i32, error: &str) -> Status {
    let text = output.trim();
    let detail = error.trim();
    if exit_code != 0 {
        let reason = if !detail.is_empty() { detail } else if !text.is_empty() { text } else { "Dropbox could not be checked" };
        return Status { reason: reason.into(), code: "statusFailed".into() };
    }
    if text.is_empty() {
        let reason = if !detail.is_empty() { detail } else { "Dropbox did not report its status" };
        return Status { reason: reason.into(), code: "statusFailed".into() };
    }
    if !detail.is_empty() {
        return Status { reason: detail.into(), code: "statusFailed".into() };
    }
    let lower = text.to_lowercase();
    if lower.contains("not running")
        || lower.contains("isn't running")
        || lower == "stopped"
        || text.starts_with("Couldn't get status:")
        || text == "Dropbox isn't responding!"
        || text == "Dropbox daemon stopped."
    {
        return Status { reason: text.into(), code: "notRunning".into() };
    }
    Status { reason: String::new(), code: String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_unreadable_accounts_keep_their_codes() {
        assert_eq!(account("", "").reason, "Dropbox is signed out");
        assert_eq!(account("", "").code, "signedOut");
        assert_eq!(account("", "Permission denied").reason, "Permission denied");
        assert_eq!(account("", "Permission denied").code, "readFailed");
        assert_eq!(account("{", "").reason, "Dropbox account details could not be read");
        assert_eq!(account("{", "").code, "invalid");
        assert_eq!(account("[]", "").code, "invalid");
        assert_eq!(account("{}", "").code, "signedOut");
    }

    #[test]
    fn personal_precedes_business_and_bad_paths_refuse() {
        let both = r#"{"personal":{"path":"/custom/personal/"},"business":{"path":"/custom/business"}}"#;
        assert_eq!(account(both, "").path, "/custom/personal");
        assert_eq!(account(both, "").code, "");
        let business = r#"{"business":{"path":"/custom/business"}}"#;
        assert_eq!(account(business, "").path, "/custom/business");
        assert_eq!(account(r#"{"personal":{"path":"relative"}}"#, "").path, "");
        assert_eq!(account(r#"{"personal":{"path":"relative"}}"#, "").code, "invalid");
        assert_eq!(account(r#"{"personal":{},"business":{"path":"/other"}}"#, "").path, "");
        assert_eq!(account(r#"{"personal":{"path":"//"}}"#, "").code, "invalid", "a slash-only root is the filesystem root");
    }

    #[test]
    fn membership_is_a_root_or_a_child_and_never_a_sibling() {
        assert!(contains("/custom/Dropbox", "/custom/Dropbox"));
        assert!(contains("/custom/Dropbox", "/custom/Dropbox/file.txt"));
        assert!(contains("/custom/Dropbox", "/custom/Dropbox/nested/file.txt"));
        assert!(!contains("/custom/Dropbox", "/custom/Dropbox-old/file.txt"));
        assert!(!contains("/custom/Dropbox", "/custom"));
        assert!(!contains("", "/custom/file.txt"));
        assert!(!contains("custom/Dropbox", "custom/Dropbox/file.txt"));
        assert!(!contains("/custom/Dropbox", ""));
    }

    #[test]
    fn daemon_words_tell_readiness_apart_at_exit_zero() {
        // tests/js/taildrop.js:68-73 assert the reason of each of these inputs, so the reason is checked too.
        assert_eq!(status("Up to date", 0, "").reason, "");
        assert_eq!(status("Up to date", 0, "").code, "");
        assert_eq!(status("Idle", 0, "").reason, "");
        assert_eq!(status("Idle", 0, "").code, "");
        assert_eq!(status("Dropbox isn't running!", 0, "").reason, "Dropbox isn't running!");
        assert_eq!(status("Dropbox isn't running!", 0, "").code, "notRunning");
        assert_eq!(status("Dropbox isn't responding!", 0, "").reason, "Dropbox isn't responding!");
        assert_eq!(status("Dropbox isn't responding!", 0, "").code, "notRunning");
        assert_eq!(status("Dropbox daemon stopped.", 0, "").reason, "Dropbox daemon stopped.");
        assert_eq!(status("Dropbox daemon stopped.", 0, "").code, "notRunning");
        assert_eq!(
            status("Couldn't get status: daemon isn't responding", 0, "").reason,
            "Couldn't get status: daemon isn't responding"
        );
        assert_eq!(status("Couldn't get status: daemon isn't responding", 0, "").code, "notRunning");
        assert_eq!(status("", 0, "").reason, "Dropbox did not report its status");
        assert_eq!(status("", 0, "").code, "statusFailed");
        assert_eq!(status("", 1, "Permission denied").reason, "Permission denied");
        assert_eq!(status("", 1, "Permission denied").code, "statusFailed");
        assert_eq!(status("Up to date", 0, "warning").code, "statusFailed");
    }
}

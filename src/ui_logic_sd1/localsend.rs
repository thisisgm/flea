//! LocalSend wording and discovery policy, from ui/js/LocalSend.js; the CLI's refusal stays an untranslated parameter.
use crate::i18n::I18n;

// Discovery costs over a second of CLI, so a list stays warm this long between menu opens.
pub const WARM_MS: u64 = 15000;

// One file names the file and the device; several are counted, grouped past three figures.
pub fn sending(i18n: &I18n, peer: &str, paths: &[String]) -> String {
    let what = match paths {
        [only] => leaf(only),
        _ => i18n.trn("items.count", paths.len() as i64),
    };
    i18n.tr_args("send.providers.sending", &[&what, peer])
}

// The CLI ends its own run when the transfer does, so its verdict is the only result ever known.
pub fn verdict(i18n: &I18n, ok: bool, reason: &str) -> String {
    if ok {
        return i18n.tr("send.providers.finished");
    }
    if reason.is_empty() {
        return i18n.tr("send.providers.unfinished");
    }
    i18n.tr_args("send.providers.reason", &[reason])
}

pub fn missing(i18n: &I18n, reason: Option<&str>) -> String {
    match reason {
        Some(reason) if !reason.is_empty() => i18n.tr_args("send.providers.reason", &[reason]),
        _ => i18n.tr("send.providers.no_cli"),
    }
}

// The one call a menu makes: nothing to send, or no CLI to send it with, each its own sentence.
pub fn dispatch_empty(i18n: &I18n) -> String {
    i18n.tr("send.providers.empty")
}

// The LocalSend.qml refresh policy as pure inputs: installed, a list in flight, last answer.
pub fn should_refresh(installed: bool, checking: bool, asked_at_ms: Option<u64>, now_ms: u64) -> bool {
    if !installed || checking {
        return false;
    }
    match asked_at_ms {
        Some(asked) => now_ms.saturating_sub(asked) >= WARM_MS,
        None => true,
    }
}

// The backend names devices the way their owners named them; the menu keys rows by that name.
pub fn row_id(name: &str) -> &str {
    name
}

fn leaf(path: &str) -> String {
    match path.rfind('/') {
        Some(cut) => path[cut + 1..].into(),
        None => path.into(),
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
    fn one_file_names_the_file_and_several_are_counted_grouped() {
        let i18n = english();
        let peer = "Clean Lemon";
        assert_eq!(
            sending(&i18n, peer, &["/home/gm/a file.txt".to_string()]),
            "Sending a file.txt to Clean Lemon with LocalSend"
        );
        assert_eq!(
            sending(&i18n, peer, &["/home/gm/a.txt".to_string(), "/home/gm/b.txt".to_string()]),
            "Sending 2 items to Clean Lemon with LocalSend"
        );
        let crowd: Vec<String> = (0..1204).map(|i| format!("/home/gm/f{i}.txt")).collect();
        assert_eq!(sending(&i18n, peer, &crowd), "Sending 1,204 items to Clean Lemon with LocalSend");
    }

    #[test]
    fn finished_refused_and_empty_refusals_each_say_their_sentence() {
        let i18n = english();
        assert_eq!(verdict(&i18n, true, ""), "LocalSend finished the transfer.");
        assert_eq!(
            verdict(&i18n, false, "Clean Lemon did not accept the transfer."),
            "LocalSend \u{b7} Clean Lemon did not accept the transfer."
        );
        assert_eq!(verdict(&i18n, false, ""), "LocalSend could not finish the transfer.");
        assert_eq!(dispatch_empty(&i18n), "There is nothing to send.");
    }

    #[test]
    fn a_cli_that_left_says_why_and_a_missing_reason_names_the_cli() {
        let i18n = english();
        assert_eq!(
            missing(&i18n, Some("localsend-cli is not installed.")),
            "LocalSend \u{b7} localsend-cli is not installed."
        );
        assert_eq!(missing(&i18n, None), "localsend-cli is no longer installed.");
        assert_eq!(missing(&i18n, Some("")), "localsend-cli is no longer installed.");
    }

    #[test]
    fn discovery_runs_once_then_rests_for_fifteen_seconds() {
        assert!(should_refresh(true, false, None, 0));
        assert!(!should_refresh(false, false, None, 0));
        assert!(!should_refresh(true, true, None, 0));
        assert!(!should_refresh(true, false, Some(1000), 1000 + WARM_MS - 1));
        assert!(should_refresh(true, false, Some(1000), 1000 + WARM_MS));
    }
}

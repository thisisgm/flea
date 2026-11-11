use super::*;
use crate::ui_logic_settings_pages::test_support::say;

#[test]
fn english_words_match_the_oracle_table() {
    let pairs = [
        ("settings.words.taildrop_folder", "files only"),
        ("settings.words.taildrop_signed_out", "signed out"),
        ("settings.words.taildrop_no_peers", "no devices"),
        ("settings.words.taildrop_down", "offline"),
        ("settings.words.localsend_folder", "files only"),
        ("settings.words.localsend_no_peers", "no devices"),
        ("settings.words.dropbox_signed_out", "signed out"),
        ("settings.words.dropbox_down", "not running"),
        ("settings.words.permissions_link", "symlink"),
        ("settings.words.permissions_special", "special file"),
        ("settings.words.permissions_not_yours", "not yours"),
    ];
    for (key, english) in pairs {
        assert_eq!(say(&Text::Key(key)), english, "{key}");
    }
}


#[test]
fn taildrop_folder_names_its_limit_whatever_the_code() {
    assert_eq!(taildrop(true, "signedOut"), Text::Key("settings.words.taildrop_folder"));
    assert_eq!(say(&taildrop(true, "")), "files only");
    assert_eq!(say(&taildrop(true, "noPeers")), "files only");
}

#[test]
fn taildrop_file_reads_its_status_code() {
    assert_eq!(taildrop(false, "signedOut"), Text::Key("settings.words.taildrop_signed_out"));
    assert_eq!(say(&taildrop(false, "signedOut")), "signed out");
    assert_eq!(say(&taildrop(false, "noPeers")), "no devices");
    assert_eq!(say(&taildrop(false, "down")), "offline");
    assert_eq!(say(&taildrop(false, "")), "offline");
    assert_eq!(say(&taildrop(false, "unknownCode")), "offline");
}

#[test]
fn localsend_folder_or_no_devices() {
    assert_eq!(say(&localsend(true)), "files only");
    assert_eq!(localsend(false), Text::Key("settings.words.localsend_no_peers"));
    assert_eq!(say(&localsend(false)), "no devices");
}

#[test]
fn dropbox_signed_out_or_not_running() {
    assert_eq!(dropbox("signedOut"), Text::Key("settings.words.dropbox_signed_out"));
    assert_eq!(say(&dropbox("signedOut")), "signed out");
    assert_eq!(say(&dropbox("running")), "not running");
    assert_eq!(say(&dropbox("")), "not running");
}

#[test]
fn permissions_without_a_file_type_name_nothing() {
    assert_eq!(permissions(0), Text::Empty);
    assert_eq!(permissions(0o755), Text::Empty);
    assert!(permissions(0o644).is_empty());
}

#[test]
fn permissions_link_names_itself_as_symlink() {
    let link = 0o120000 | 0o777;
    assert_eq!(permissions(link), Text::Key("settings.words.permissions_link"));
    assert_eq!(say(&permissions(link)), "symlink");
}

#[test]
fn permissions_other_kinds_read_special_file() {
    // Oracle behaviour: any kind that is not zero and not a link reads as special, regular files included.
    let socket = 0o140000 | 0o644;
    let fifo = 0o010000 | 0o600;
    let folder = 0o040000 | 0o755;
    let regular = 0o100000 | 0o644;
    for mode in [socket, fifo, folder, regular] {
        assert_eq!(permissions(mode), Text::Key("settings.words.permissions_special"), "mode {mode:o}");
    }
    assert_eq!(say(&permissions(socket)), "special file");
}

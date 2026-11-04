// The state word an errored menu row says in its hint slot: a fixed key, never a provider's reason.
use super::Text;

// The file type lives in the top four bits of st_mode.
const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;

// Taildrop's hint: a folder names its limit, otherwise the status code picks the word.
pub fn taildrop(folder: bool, code: &str) -> Text {
    if folder {
        Text::Key("settings.words.taildrop_folder")
    } else if code == "signedOut" {
        Text::Key("settings.words.taildrop_signed_out")
    } else if code == "noPeers" {
        Text::Key("settings.words.taildrop_no_peers")
    } else {
        Text::Key("settings.words.taildrop_down")
    }
}

// LocalSend finds nobody on the network for any cause other than a folder.
pub fn localsend(folder: bool) -> Text {
    if folder {
        Text::Key("settings.words.localsend_folder")
    } else {
        Text::Key("settings.words.localsend_no_peers")
    }
}

// Dropbox names a signed-out account, and any other state reads as not running.
pub fn dropbox(code: &str) -> Text {
    if code == "signedOut" {
        Text::Key("settings.words.dropbox_signed_out")
    } else {
        Text::Key("settings.words.dropbox_down")
    }
}

// A link names itself; any other kind is special. A mode with no file type names nothing.
pub fn permissions(mode: u32) -> Text {
    let kind = mode & S_IFMT;
    if kind == 0 {
        Text::Empty
    } else if kind == S_IFLNK {
        Text::Key("settings.words.permissions_link")
    } else {
        Text::Key("settings.words.permissions_special")
    }
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;

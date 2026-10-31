use std::os::unix::ffi::OsStrExt;
use std::path::{Component, PathBuf};

// An empty FLEA_UI would resolve the entry against the working directory, so it is no candidate.
fn env_ui_dir() -> Option<PathBuf> {
    ui_dir_from(std::env::var("FLEA_UI").ok())
}

// Split so a test names the value without writing this process's environment.
fn ui_dir_from(value: Option<String>) -> Option<PathBuf> {
    let value = value?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

// The Quickshell entry, in its own directory so ui/qmldir's singletons stay off the startup path.
pub const ENTRY: &str = "boot/shell.qml";

// The chooser's own entry, beside it; see AGENTS.md "The first window".
pub const PICKER_ENTRY: &str = "boot/picker.qml";

// The ShellId pragmas in ui/boot/shell.qml and ui/boot/picker.qml; qsregistry prunes only these.
pub const SHELL_ID: &str = "flea";
pub const PICKER_SHELL_ID: &str = "fleapicker";

// Issue 216: names the entry file sought, since a package missing ui/boot reads as a missing dir.
pub fn missing_ui_message() -> String {
    format!("flea: Flea's interface files are missing (/usr/share/flea/ui/{}). Reinstall flea.", ENTRY)
}

// The UI ships as data, so it is found the same way FLEA_BIN finds the binary.
pub fn ui_dir() -> Option<PathBuf> {
    if let Some(p) = env_ui_dir() {
        if p.join(ENTRY).is_file() {
            return Some(p);
        }
    }
    // The dev tree keeps ui/ beside target/, so walk up from the binary.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dev) = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).map(|p| p.join("ui")) {
            if dev.join(ENTRY).is_file() {
                return Some(dev);
            }
        }
    }
    let packaged = PathBuf::from("/usr/share/flea/ui");
    if packaged.join(ENTRY).is_file() {
        return Some(packaged);
    }
    None
}

pub fn has_display() -> bool {
    ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

// Decodes %XX triplets in a file:// URI path; a malformed triplet passes through as literal text.
pub fn percent_decode(s: &str) -> String {
    String::from_utf8_lossy(&percent_decode_bytes(s)).into_owned()
}

// The raw bytes behind %XX triplets, for a name that need not be UTF-8.
pub fn percent_decode_bytes(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// A relative argument made absolute and folded lexically, never above /, as g_file_new_for_commandline_arg reads one; anything else passes through.
pub fn commandline_path(arg: &str) -> String {
    if arg.starts_with('/') || arg.contains("://") {
        return arg.to_string();
    }
    let Ok(full) = crate::shelf::absolute(arg) else {
        return arg.to_string();
    };
    let trailing_slash = full.as_os_str().as_bytes().ends_with(b"/");
    let mut out = PathBuf::from("/");
    for component in full.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(name) => out.push(name),
            _ => {}
        }
    }
    if trailing_slash && out.as_os_str().len() > 1 {
        out.push("");
    }
    out.into_os_string().into_string().unwrap_or_else(|_| arg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_missing_ui_message_names_the_entry_file_it_looked_for() {
        assert_eq!(
            missing_ui_message(),
            "flea: Flea's interface files are missing (/usr/share/flea/ui/boot/shell.qml). Reinstall flea."
        );
    }

    #[test]
    fn an_empty_flea_ui_is_not_a_candidate() {
        assert_eq!(ui_dir_from(Some(String::new())), None);
        assert_eq!(ui_dir_from(None), None);
        assert_eq!(ui_dir_from(Some("/flea-test/ui".into())), Some(PathBuf::from("/flea-test/ui")));
    }

    #[test]
    fn percent_decode_reads_the_normal_case() {
        assert_eq!(percent_decode("My%20Files"), "My Files");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("caf%C3%A9"), "café");
    }

    #[test]
    fn a_literal_plus_is_not_decoded_to_a_space() {
        // RFC 3986 path decoding, not the application/x-www-form-urlencoded query rule: a file
        // literally named "a+b.txt" must round-trip, so only %XX triplets ever decode here.
        assert_eq!(percent_decode("a+b.txt"), "a+b.txt");
        assert_eq!(percent_decode("My%20Files+Archive"), "My Files+Archive");
    }

    #[test]
    fn a_malformed_triplet_passes_through_literally() {
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("100%2"), "100%2");
        assert_eq!(percent_decode("a%zzb"), "a%zzb");
    }

    // Sample file line: `//@ pragma ShellId flea` in ui/boot/shell.qml.
    fn shell_id_of(text: &str) -> Option<&str> {
        text.lines().find_map(|line| line.trim_start().strip_prefix("//@ pragma ShellId "))
    }

    #[test]
    fn the_shell_ids_match_the_boot_pragmas() {
        assert_eq!(shell_id_of(include_str!("../ui/boot/shell.qml")), Some(SHELL_ID));
        assert_eq!(shell_id_of(include_str!("../ui/boot/picker.qml")), Some(PICKER_SHELL_ID));
    }

    // Relative cases read the cwd cargo gave the test, because changing it would race the other tests in the process.
    fn here() -> String {
        std::env::current_dir().unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn a_dot_start_is_the_launch_directory() {
        assert_eq!(commandline_path("."), here());
    }

    #[test]
    fn a_relative_name_joins_the_launch_directory() {
        assert_eq!(commandline_path("./a"), format!("{}/a", here()));
        assert_eq!(commandline_path("a/b"), format!("{}/a/b", here()));
    }

    #[test]
    fn a_dot_dot_inside_a_relative_path_folds_lexically() {
        assert_eq!(commandline_path("a/../b"), format!("{}/b", here()));
    }

    #[test]
    fn a_leading_dot_dot_names_the_parent_of_the_launch_directory() {
        let cwd = std::env::current_dir().unwrap();
        let parent = cwd.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "/".to_string());
        assert_eq!(commandline_path(".."), parent);
        assert_eq!(commandline_path("../x"), format!("{}/x", parent.trim_end_matches('/')));
    }

    #[test]
    fn dot_dot_never_climbs_above_the_root() {
        assert_eq!(commandline_path(&"../".repeat(64)), "/");
        assert_eq!(commandline_path(&format!("{}x", "../".repeat(64))), "/x");
    }

    #[test]
    fn an_absolute_path_passes_through_byte_for_byte() {
        for kept in ["/", "/etc/hostname", "/etc/", "/a/./b/../c", "//host/share", "/a//b/"] {
            assert_eq!(commandline_path(kept), kept);
        }
    }

    #[test]
    fn a_uri_passes_through() {
        for kept in ["smb://host/share", "file://./a.png", "trash:///"] {
            assert_eq!(commandline_path(kept), kept);
        }
    }

    #[test]
    fn an_empty_argument_stays_empty() {
        assert_eq!(commandline_path(""), "");
    }

    #[test]
    fn a_relative_trailing_slash_is_kept_once() {
        assert_eq!(commandline_path("a/"), format!("{}/a/", here()));
        assert_eq!(commandline_path("a//"), format!("{}/a/", here()));
    }
}

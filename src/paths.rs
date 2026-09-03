use std::path::PathBuf;

// An empty FLEA_UI would resolve shell.qml against the working directory, so it is no candidate.
fn env_ui_dir() -> Option<PathBuf> {
    let value = std::env::var("FLEA_UI").ok()?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

// The UI ships as data, so it is found the same way FLEA_BIN finds the binary.
pub fn ui_dir() -> Option<PathBuf> {
    if let Some(p) = env_ui_dir() {
        if p.join("shell.qml").is_file() {
            return Some(p);
        }
    }
    let packaged = PathBuf::from("/usr/share/flea/ui");
    if packaged.join("shell.qml").is_file() {
        return Some(packaged);
    }
    // The dev tree keeps ui/ beside target/, so walk up from the binary.
    let exe = std::env::current_exe().ok()?;
    let dev = exe.parent()?.parent()?.parent()?.join("ui");
    if dev.join("shell.qml").is_file() {
        return Some(dev);
    }
    None
}

pub fn has_display() -> bool {
    ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .any(|name| std::env::var_os(name).map_or(false, |value| !value.is_empty()))
}

// Decodes %XX triplets in a file:// URI path; a malformed triplet passes through as literal text.
pub fn percent_decode(s: &str) -> String {
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
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both cases sit in one test because FLEA_UI is process wide and cargo runs tests in threads.
    #[test]
    fn an_empty_flea_ui_is_not_a_candidate() {
        std::env::set_var("FLEA_UI", "");
        assert_eq!(env_ui_dir(), None);
        std::env::set_var("FLEA_UI", "/usr/share/flea/ui");
        assert_eq!(env_ui_dir(), Some(PathBuf::from("/usr/share/flea/ui")));
        std::env::remove_var("FLEA_UI");
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
}

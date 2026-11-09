//! src/backend/fsname.rs's per-drive name refusals read back into catalog keys, so each one reads whole in the user's language.
use super::Sentence;

// src/backend/fsname.rs names only these drives, and refuses only these characters.
const NAMED_DRIVES: [&str; 3] = ["vfat", "exfat", "ntfs3"];
const REFUSED_CHARACTERS: &str = "\"*:<>?\\|";

// Sample input: "':' is not allowed in a name on this vfat drive" reads back as feedback.drive.char with ":" and "vfat".
/// One of fsname's per-drive name refusals read back whole into its key, then the refused part and the drive.
pub fn drive_refusal(message: &str) -> Option<Sentence> {
    let text = message.trim().trim_end_matches('.');
    let (said, drive) = text.strip_suffix(" drive")?.rsplit_once(" on this ")?;
    if !NAMED_DRIVES.contains(&drive) {
        return None;
    }
    let quoted = |tail: &str| said.strip_prefix('\'')?.strip_suffix(tail);
    if said == "a name cannot end in a dot" {
        Some(Sentence::args("feedback.drive.end_dot", &[drive]))
    } else if said == "a name cannot end in a space" {
        Some(Sentence::args("feedback.drive.end_space", &[drive]))
    } else if let Some(code) = said.strip_prefix("a name cannot hold control character ") {
        let digits = code.strip_prefix("U+")?;
        (digits.len() >= 4
            && digits
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)))
        .then(|| Sentence::args("feedback.drive.control", &[code, drive]))
    } else if let Some(bad) = quoted("' is not allowed in a name") {
        let mut chars = bad.chars();
        matches!((chars.next(), chars.next()), (Some(one), None) if REFUSED_CHARACTERS.contains(one))
            .then(|| Sentence::args("feedback.drive.char", &[bad, drive]))
    } else {
        let stem = quoted("' is reserved")?;
        reserved(stem).then(|| Sentence::args("feedback.drive.reserved", &[stem, drive]))
    }
}

// The DOS device names fsname refuses: CON, PRN, AUX, NUL, COM1 to COM9 and LPT1 to LPT9.
fn reserved(stem: &str) -> bool {
    matches!(stem, "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

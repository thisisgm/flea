//! Whether a menu row's archive can be extracted with the tools the backend found, from ui/js/Archive.js canExtract.
use crate::ui_logic_pv1::kinds::is_archive;

// Tails 7-Zip opens that bsdtar does not, and the disk images that only list.
const SEVEN_TAILS: &[&str] = &[".appimage", ".cpio", ".cab", ".jar", ".apk", ".rpm", ".deb", ".iso"];
const LIST_ONLY: &[&str] = &[".iso"];

/// The tools a backend reported in its formats reply's extract object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extraction {
    pub archive: bool,
    pub seven_zip: bool,
    pub zip: bool,
}

pub fn row_is_archive(name: &str, dir: bool) -> bool {
    !dir && is_archive(name)
}

pub fn can_extract(name: &str, caps: Extraction) -> bool {
    let lower = name.to_lowercase();
    let tail = |suffix: &&str| lower.ends_with(suffix);
    if LIST_ONLY.iter().any(tail) {
        return false;
    }
    if lower.ends_with(".7z") {
        return caps.seven_zip;
    }
    if lower.ends_with(".zip") || lower.ends_with(".rar") {
        return caps.zip;
    }
    if SEVEN_TAILS.iter().any(tail) {
        return caps.seven_zip;
    }
    caps.archive
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_tail_reads_its_own_tool_and_a_disk_image_never_extracts() {
        let only = |archive, seven_zip, zip| Extraction { archive, seven_zip, zip };
        assert!(can_extract("a.tar.gz", only(true, false, false)));
        assert!(!can_extract("a.tar.gz", only(false, true, true)));
        assert!(can_extract("A.ZIP", only(false, false, true)));
        assert!(can_extract("a.rar", only(false, false, true)));
        assert!(can_extract("a.7z", only(false, true, false)));
        assert!(can_extract("a.deb", only(false, true, false)));
        assert!(!can_extract("a.deb", only(true, false, true)));
        assert!(!can_extract("a.iso", only(true, true, true)));
        assert!(row_is_archive("a.zip", false) && !row_is_archive("a.zip", true));
    }
}

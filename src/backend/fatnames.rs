// Names a FAT-family filesystem cannot hold, caught before anything is written: the Drops board's callout 3.
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

// Sample input: "exfat" -> true, "ext4" -> false.
pub fn is_fat_fstype(fstype: &str) -> bool {
    // durable::fat_name_is_durable names four of these for write confirmation; the card also takes the fat and msdos spellings.
    matches!(fstype.to_lowercase().as_str(), "vfat" | "exfat" | "fat" | "fat12" | "fat16" | "fat32" | "msdos" | "ntfs" | "ntfs3")
}

// The filesystem type of the mount owning dest, from the same table renamecompat reads.
pub fn fs_type_of(dest: &Path) -> Option<String> {
    let body = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    super::mountinfo::mount_type_in(dest, &body)
}

// Whether dest sits on a mount whose name rules the card enforces; one that cannot be read is not one.
pub fn on_fat(dest: &Path) -> bool {
    #[cfg(test)]
    if STOOD_FAT.lock().unwrap().iter().any(|root| dest.starts_with(root)) {
        return true;
    }
    fs_type_of(dest).is_some_and(|fs| is_fat_fstype(&fs))
}

// Roots read as FAT-family mounts on every thread, because the question runs on a thread of its own; a sandbox root keeps one test from reaching another.
#[cfg(test)]
static STOOD_FAT: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

// Sample input: test_stand_fat(root, true) makes every destination under root read as a FAT-family mount; false ends it.
#[cfg(test)]
pub fn test_stand_fat(root: &Path, on: bool) {
    let mut roots = STOOD_FAT.lock().unwrap();
    roots.retain(|stood| stood != root);
    if on {
        roots.push(root.to_path_buf());
    }
}

// Sample input: "clip: final.mp4" -> "clip_ final.mp4".
pub fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        out.push(match ch {
            ':' | '?' | '*' | '"' | '<' | '>' | '|' | '\\' => '_',
            // Linux vfat_bad_char and exfat_bad_char, and the exFAT spec's Table 35, refuse below 0x20 only; 0x7f (DEL) is legal.
            other if (other as u32) < 0x20 => '_',
            other => other,
        });
    }
    let trimmed = out.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() { "_".to_string() } else { trimmed.to_string() }
}

// The name a file or folder below the top lands under: Rename sanitizes a text name, any other answer or a name that is not text keeps it.
pub fn inside_leaf(name: &std::ffi::OsStr, rename: bool) -> std::ffi::OsString {
    match name.to_str() {
        Some(text) if rename && needs_rename(text) => sanitize(text).into(),
        _ => name.to_os_string(),
    }
}

// Whether the card's Rename answer covers a destination on a FAT-family mount, and with it every name below the top.
pub fn renames(fat: Option<Choice>, on_fat: bool) -> bool {
    fat == Some(Choice::Rename) && on_fat
}

// Whether the card's answer applies to dest: an answer sent to any other filesystem changes nothing.
pub fn answered_on_fat(fat: Option<Choice>, dest: &Path) -> bool {
    fat.is_some() && on_fat(dest)
}

// Whether the name cannot sit on the target as it is.
pub fn needs_rename(name: &str) -> bool {
    sanitize(name) != name
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choice {
    Rename,
    Skip,
}

impl Choice {
    // Sample input: {"c":"transfer","op":"copy","paths":["/a/b.png"],"dest":"/mnt/128GB","fat":"rename","collideId":7}
    pub fn parse(line: &str) -> Option<Choice> {
        match crate::json::field_str(line, "fat").as_deref() {
            Some("rename") => Some(Choice::Rename),
            Some("skip") => Some(Choice::Skip),
            _ => None,
        }
    }
}

// A copy or move made under the card's Rename keeps that answer, so a redo writes the same names.
pub fn mark_step(step: &mut super::undo::Step) {
    if let super::undo::Step::Copied { rename_fat, .. } | super::undo::Step::Moved { rename_fat, .. } = step {
        *rename_fat = true;
    }
}

// A copy or move made under Rename journals `"fat":1` beside its fields; an older journal has no key and reads as a copy made without it.
pub(crate) fn mark_renamed(mut pairs: Vec<(&'static str, crate::jsondoc::Json)>, renamed: bool) -> Vec<(&'static str, crate::jsondoc::Json)> {
    if renamed {
        pairs.push(("fat", super::undocodec::n(1)));
    }
    pairs
}

pub(crate) fn decode_renamed(pairs: &[(String, crate::jsondoc::Json)]) -> bool {
    super::undocodec::get(pairs, "fat").and_then(super::undocodec::parse_u32) == Some(1)
}

// Where dst lands under this answer on a FAT destination, decided before anything is written.
pub enum Mapped {
    Same,
    Renamed(PathBuf),
    Skipped,
}

pub fn map_dst(choice: Choice, fs_fat: bool, dst: &Path) -> Mapped {
    if !fs_fat {
        return Mapped::Same;
    }
    // A name that is not text keeps it, as inside_leaf keeps the names below it.
    let leaf = match dst.file_name().map(|n| n.to_str()) {
        Some(Some(text)) => text,
        Some(None) => return Mapped::Same,
        None => "",
    };
    if !needs_rename(leaf) {
        return Mapped::Same;
    }
    match choice {
        Choice::Rename => Mapped::Renamed(dst.parent().unwrap_or(Path::new("/")).join(sanitize(leaf))),
        Choice::Skip => Mapped::Skipped,
    }
}

// Where one item lands under the card's answer: the leaf remapped, or nothing for a skip.
pub fn apply_dst(fat: Option<Choice>, fs_fat: bool, dst: PathBuf, here: bool) -> Option<(PathBuf, bool)> {
    let Some(choice) = fat else { return Some((dst, here)) };
    match map_dst(choice, fs_fat, &dst) {
        Mapped::Same => Some((dst, here)),
        Mapped::Renamed(to) => Some((to, false)),
        Mapped::Skipped => None,
    }
}

// One question leaf under the same answer: renamed leaves ask under their new name, skips ask nothing.
pub fn ask_leaf(fat: Option<Choice>, fs_fat: bool, dest: &Path, name: &str) -> Option<(PathBuf, String)> {
    let first = dest.join(flea::bytename::view(name));
    let Some(choice) = fat else { return Some((first, name.to_string())) };
    match map_dst(choice, fs_fat, &first) {
        Mapped::Same => Some((first, name.to_string())),
        Mapped::Renamed(to) => Some((to.clone(), to.file_name().and_then(|n| n.to_str()).unwrap_or(name).to_string())),
        Mapped::Skipped => None,
    }
}

// The card's rows, one per name that cannot stand: the mark, what was asked for and what Rename and copy writes.
fn rename_rows(paths: &[String], body: &str, mime: &super::mime::Db, icons: &super::icons::Names) -> Vec<String> {
    let mut shown = Vec::new();
    for raw in paths {
        let src = flea::bytename::view(raw);
        let Some(leaf) = src.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !needs_rename(leaf) {
            continue;
        }
        let (dir, mode, dev) = std::fs::symlink_metadata(&src)
            .map(|m| (m.is_dir(), m.mode(), m.dev()))
            .unwrap_or((false, 0, 0));
        // The mode is the source's own stat, so its execute bits count only where the source's mount keeps modes.
        let real_modes = super::gvfsroot::gvfs_modes_are_real(&src)
            .or_else(|| super::mountinfo::modes_are_real_for_dev(dev, body))
            .unwrap_or_else(|| super::mountinfo::modes_are_real(&src, body));
        let mode = if real_modes { mode } else { mode & !super::icons::ANY_EXECUTE_BIT };
        let new = sanitize(leaf);
        shown.push(format!(r#"{{"old":"{}","new":"{}","d":{},"i":"{}"}}"#,
            crate::json::escape(leaf), crate::json::escape(&new), dir,
            crate::json::escape(icons.icon_for(mime.lookup(&new), dir, mode))));
    }
    shown
}

// Sample output: {"t":"fatnames","id":7,"fstype":"exfat","volume":"128GB","renames":[{"old":"clip: x.mp4","new":"clip_ x.mp4","d":false,"i":"video-x-generic"}]}
pub fn answer(id: usize, paths: &[String], dest: &str, mime: &super::mime::Db, icons: &super::icons::Names) -> Option<String> {
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    answer_in(&body, id, paths, dest, mime, icons)
}

// The answer against a given mount table, so a suite can stand a FAT mount where it cannot make one.
fn answer_in(body: &str, id: usize, paths: &[String], dest: &str, mime: &super::mime::Db, icons: &super::icons::Names) -> Option<String> {
    let entry = super::mountinfo::mount_entry_in(&flea::bytename::view(dest), body).filter(|entry| is_fat_fstype(&entry.fstype))?;
    let shown = rename_rows(paths, body, mime, icons);
    let volume = entry.mount.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    (!shown.is_empty()).then(|| format!(r#"{{"t":"fatnames","id":{},"fstype":"{}","volume":"{}","renames":[{}]}}"#,
        id, crate::json::escape(&entry.fstype), crate::json::escape(volume), shown.join(",")))
}

#[path = "casefold.rs"]
pub mod casefold;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn clean_names_stand_untouched() {
        assert_eq!(sanitize("final.mp4"), "final.mp4");
        assert_eq!(sanitize("notes.md"), "notes.md");
        assert!(!needs_rename("final.mp4"));
    }

    #[test]
    fn each_forbidden_character_becomes_an_underscore() {
        assert_eq!(sanitize("clip: final.mp4"), "clip_ final.mp4");
        assert_eq!(sanitize("notes?.md"), "notes_.md");
        assert_eq!(sanitize("a*b\"c<d>e|f"), "a_b_c_d_e_f");
        assert_eq!(sanitize("a\\b.txt"), "a_b.txt");
        assert_eq!(sanitize("a\tb"), "a_b");
        assert_eq!(sanitize("a\u{1}b\u{1f}c"), "a_b_c");
        assert_eq!(sanitize("a\u{7f}b"), "a\u{7f}b", "DEL is a legal FAT and exFAT name character");
        assert_eq!(sanitize("x\u{7f}\n:y.mp4"), "x\u{7f}__y.mp4");
        assert!(!needs_rename("a\u{7f}b") && needs_rename("a\u{1}b"));
        assert_eq!(sanitize("a\u{80}b"), "a\u{80}b", "a C1 character is a legal FAT name character");
        assert!(needs_rename("clip: final.mp4"));
    }

    #[test]
    fn trailing_dots_and_spaces_are_trimmed() {
        assert_eq!(sanitize("draft."), "draft");
        assert_eq!(sanitize("draft  "), "draft");
        assert_eq!(sanitize("draft. . "), "draft");
        assert!(needs_rename("draft."));
    }

    #[test]
    fn a_name_of_only_dots_still_names_a_file() {
        assert!(!sanitize("...").is_empty());
        assert_eq!(sanitize("..."), "_");
    }

    #[test]
    fn the_fat_family_matches_and_nothing_else_does() {
        for fs in ["vfat", "exfat", "FAT32", "msdos", "fat16", "ntfs", "ntfs3"] {
            assert!(is_fat_fstype(fs), "{} is a FAT filesystem", fs);
        }
        for fs in ["ext4", "btrfs", "xfs", "fuse.rclone", "nfs", "tmpfs", ""] {
            assert!(!is_fat_fstype(fs), "{} is not one", fs);
        }
    }

    #[test]
    fn the_card_answer_parses_and_nothing_else_does() {
        let line = r#"{"c":"transfer","op":"copy","dest":"/mnt/128GB","fat":"rename","collideId":7}"#;
        assert_eq!(Choice::parse(line), Some(Choice::Rename));
        assert_eq!(Choice::parse(r#"{"c":"transfer","fat":"skip"}"#), Some(Choice::Skip));
        assert_eq!(Choice::parse(r#"{"c":"transfer"}"#), None);
        assert_eq!(Choice::parse(r#"{"c":"transfer","fat":"keep"}"#), None);
    }

    #[test]
    fn mapping_holds_clean_names_and_spends_skips() {
        let dst = Path::new("/mnt/128GB/final.mp4");
        assert!(matches!(map_dst(Choice::Rename, true, dst), Mapped::Same));
        assert!(matches!(map_dst(Choice::Rename, false, Path::new("/mnt/128GB/clip: x.mp4")), Mapped::Same));
        let bad = Path::new("/mnt/128GB/clip: x.mp4");
        match map_dst(Choice::Rename, true, bad) {
            Mapped::Renamed(to) => assert_eq!(to, Path::new("/mnt/128GB/clip_ x.mp4")),
            _ => panic!("rename remaps the leaf"),
        }
        assert!(matches!(map_dst(Choice::Skip, true, bad), Mapped::Skipped));
    }

    #[test]
    fn the_card_lists_only_the_names_that_cannot_stand_with_their_new_names() {
        let d = TestDir::new("fatnames-rows");
        let dir = d.dir("a: folder");
        let names: Vec<String> = ["clip: final.mp4", "notes?.md", "draft.", "final.mp4"].iter().map(|n| d.join(n).to_string_lossy().into()).collect();
        let rows = rename_rows(&[names.clone(), vec![dir.to_string_lossy().into()]].concat(), "", &crate::backend::mime::Db::load(), &crate::backend::icons::Names::load());
        let joined = rows.join("|");
        assert_eq!(rows.len(), 4, "the clean name never reaches the card: {}", joined);
        assert!(joined.contains(r#""old":"clip: final.mp4","new":"clip_ final.mp4","d":false"#), "{}", joined);
        assert!(joined.contains(r#""old":"notes?.md","new":"notes_.md""#) && joined.contains(r#""old":"draft.","new":"draft""#), "{}", joined);
        assert!(joined.contains(r#""old":"a: folder","new":"a_ folder","d":true"#), "a folder shows as one: {}", joined);
    }

    #[test]
    fn an_exfat_mount_answers_its_volume_and_its_bad_names_and_nothing_else_does() {
        let d = TestDir::new("fatnames-answer");
        let dest = d.dir("128GB");
        let table = format!("40 1 8:17 / {} rw - exfat /dev/sdb1 rw\n", dest.display());
        let bad = d.join("clip: x.mp4").to_string_lossy().to_string();
        let (db, icons) = (crate::backend::mime::Db::load(), crate::backend::icons::Names::load());
        let line = answer_in(&table, 7, &[bad.clone(), d.join("ok.mp4").to_string_lossy().into()], &dest.to_string_lossy(), &db, &icons).expect("asked");
        assert!(line.starts_with(r#"{"t":"fatnames","id":7,"fstype":"exfat","volume":"128GB","renames":[{"old":"clip: x.mp4","new":"clip_ x.mp4""#), "{}", line);
        assert_eq!(line.matches(r#""old":"#).count(), 1, "the clean name never reaches the card: {}", line);
        assert!(answer_in(&table, 7, &[d.join("ok.mp4").to_string_lossy().into()], &dest.to_string_lossy(), &db, &icons).is_none(), "clean names ask nothing");
        let ext4 = table.replace("exfat", "ext4");
        assert!(answer_in(&ext4, 7, &[bad], &dest.to_string_lossy(), &db, &icons).is_none(), "an ext4 destination asks nothing");
    }

    // A FAT volume stores 0755 on every file, so a bad-named program read from one keeps the document mark.
    #[test]
    fn a_program_from_a_fat_source_draws_the_document_mark_because_its_mode_is_the_mounts_default() {
        use std::os::unix::fs::PermissionsExt;
        let d = TestDir::new("fatnames-fatsrc");
        let dest = d.dir("128GB");
        let usb = d.dir("usb");
        let bad = usb.join("clip: launcher");
        std::fs::write(&bad, "x").unwrap();
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o755)).unwrap();
        let dev = super::super::mountinfo::dev_majmin(std::fs::metadata(&bad).unwrap().dev());
        let table = format!("1 0 0:0 / / rw - ext4 /dev/a rw\n40 1 0:0 / {} rw - exfat /dev/sdb1 rw\n41 1 {dev} / {} rw - vfat /dev/sdc1 rw\n", dest.display(), usb.display());
        let (db, icons) = (crate::backend::mime::Db::load(), crate::backend::icons::Names::load());
        let line = answer_in(&table, 7, &[bad.to_string_lossy().into()], &dest.to_string_lossy(), &db, &icons).expect("asked");
        assert!(line.contains(r#""i":"text-x-generic""#), "{}", line);
    }

    // The execute bits are the source's own, so a program on a mode-keeping source keeps its mark on a FAT destination.
    #[test]
    fn a_program_from_a_mode_keeping_source_keeps_its_mark_on_a_fat_destination() {
        use std::os::unix::fs::PermissionsExt;
        let d = TestDir::new("fatnames-extsrc");
        let dest = d.dir("128GB");
        let bad = d.join("clip: launcher");
        std::fs::write(&bad, "x").unwrap();
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o755)).unwrap();
        let dev = super::super::mountinfo::dev_majmin(std::fs::metadata(&bad).unwrap().dev());
        let table = format!("1 0 {dev} / / rw - ext4 /dev/a rw\n40 1 0:0 / {} rw - exfat /dev/sdb1 rw\n", dest.display());
        let (db, icons) = (crate::backend::mime::Db::load(), crate::backend::icons::Names::load());
        let line = answer_in(&table, 7, &[bad.to_string_lossy().into()], &dest.to_string_lossy(), &db, &icons).expect("asked");
        assert!(line.contains(r#""i":"application-x-executable""#), "{}", line);
    }

    #[test]
    fn a_symlinked_source_parent_uses_its_target_mounts_modes() {
        use std::os::unix::fs::PermissionsExt;
        let d = TestDir::new("fatnames-linked-source");
        let dest = d.dir("128GB");
        let usb = d.dir("usb");
        let bad = usb.join("clip: launcher");
        std::fs::write(&bad, "x").unwrap();
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o755)).unwrap();
        let dev = super::super::mountinfo::dev_majmin(std::fs::metadata(&bad).unwrap().dev());
        let link = d.join("linked-usb");
        std::os::unix::fs::symlink(&usb, &link).unwrap();
        let source = link.join("clip: launcher");
        let device_mount = d.join("device-only");
        assert!(!source.starts_with(&device_mount) && !bad.starts_with(&device_mount));
        let (db, icons) = (crate::backend::mime::Db::load(), crate::backend::icons::Names::load());
        for fstype in ["vfat", "exfat", "ext4", "fuse.gvfsd-fuse"] {
            let lexical = if fstype == "ext4" { "exfat" } else { "ext4" };
            let table = format!("1 0 0:0 / / rw - {lexical} /dev/a rw\n40 1 0:0 / {} rw - exfat /dev/b rw\n41 1 {dev} / {} rw - {fstype} /dev/c rw\n", dest.display(), device_mount.display());
            let real_modes = fstype == "ext4";
            assert_eq!(super::super::mountinfo::modes_are_real(Path::new(&source), &table), !real_modes);
            assert_eq!(super::super::mountinfo::modes_are_real(&bad, &table), !real_modes);
            let line = answer_in(&table, 7, &[source.to_string_lossy().into()], &dest.to_string_lossy(), &db, &icons).expect("asked");
            let icon = if fstype == "ext4" { "application-x-executable" } else { "text-x-generic" };
            assert!(line.contains(&format!(r#""i":"{icon}""#)), "{fstype}: {line}");
        }
    }

    #[test]
    fn the_question_leaf_follows_the_answer_and_a_skip_asks_nothing() {
        let dest = Path::new("/mnt/128GB");
        assert_eq!(ask_leaf(None, true, dest, "clip: x"), Some((dest.join("clip: x"), "clip: x".to_string())));
        assert_eq!(ask_leaf(Some(Choice::Rename), true, dest, "clip: x"), Some((dest.join("clip_ x"), "clip_ x".to_string())));
        assert_eq!(ask_leaf(Some(Choice::Rename), false, dest, "clip: x"), Some((dest.join("clip: x"), "clip: x".to_string())));
        assert_eq!(ask_leaf(Some(Choice::Skip), true, dest, "clip: x"), None);
        assert_eq!(ask_leaf(Some(Choice::Skip), true, dest, "fine"), Some((dest.join("fine"), "fine".to_string())));
    }

    #[test]
    fn a_raw_leaf_keeps_its_own_bytes_and_asks_under_them() {
        use flea::bytename::ExactText as _;
        use std::os::unix::ffi::OsStrExt;
        let raw = Path::new(std::ffi::OsStr::from_bytes(b"/mnt/128GB/clip: \xff"));
        assert!(
            matches!(map_dst(Choice::Rename, true, raw), Mapped::Same),
            "a raw leaf is never renamed to an underscore"
        );
        let name = raw.file_name().unwrap().exact_text().into_owned();
        assert_eq!(
            ask_leaf(Some(Choice::Rename), true, Path::new("/mnt/128GB"), &name),
            Some((raw.to_path_buf(), name.clone()))
        );
    }
}

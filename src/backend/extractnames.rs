// Names an extract cannot write as they are on the destination's drive, judged from the archive's index before anything is written.
use crate::backend::fatnames::{self, casefold};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::path::Path;

// The drive's rules for the folder an extract lands in, read once per job.
pub(crate) struct Rules {
    pub fstype: Option<String>,
    pub windows_names: bool,
    pub folds: bool,
}

impl Rules {
    pub(crate) fn of(parent: &Path) -> Rules {
        let fat = fatnames::on_fat(parent);
        Rules {
            fstype: fat.then(|| fatnames::fs_type_of(parent)).flatten(),
            windows_names: fat && crate::backend::fsname::has_windows_names(parent),
            folds: casefold::folds(parent),
        }
    }
}

// Sample: "set/clip: final.txt" on vfat answers "'set/clip: final.txt' cannot be extracted onto this drive: ':' is not allowed in a name on this vfat drive".
pub(crate) fn refusal<'a>(
    members: impl IntoIterator<Item = &'a String>,
    rules: &Rules,
) -> Option<String> {
    let mut names: Vec<&str> = members.into_iter().map(String::as_str).collect();
    // The index is a map, so the first name named is the first in order and the sentence never changes between runs.
    names.sort_unstable();
    if let Some(fstype) = &rules.fstype {
        for name in &names {
            if let Some(why) = name.split('/').find_map(|part| {
                crate::backend::fsname::refuse_name(part, fstype, rules.windows_names)
            }) {
                return Some(format!(
                    "'{name}' cannot be extracted onto this drive: {why}"
                ));
            }
        }
    }
    if !rules.folds {
        return None;
    }
    let (one, other) = twins(&names)?;
    Some(format!(
        "'{one}' and '{other}' would take one name on this drive, so nothing was extracted"
    ))
}

// Two spellings that fold to one name where one is not a folder: the drive keeps only one file, or cannot make the folder.
fn twins<'a>(names: &[&'a str]) -> Option<(&'a str, &'a str)> {
    let (mut folders, mut first) = (HashSet::new(), HashMap::new());
    for &name in names {
        let mut end = 0;
        while let Some(at) = name[end..].find('/') {
            end += at;
            folders.insert(&name[..end]);
            first
                .entry(casefold::key(&name[..end], true))
                .or_insert(&name[..end]);
            end += 1;
        }
    }
    // corner: the index has no kinds, so an empty folder reads as a file and one beside a folder of another case is refused.
    for &name in names.iter().filter(|name| !folders.contains(**name)) {
        match first.entry(casefold::key(name, true)) {
            Entry::Occupied(earlier) if *earlier.get() != name => {
                return Some((*earlier.get(), name))
            }
            Entry::Occupied(_) => {}
            Entry::Vacant(slot) => {
                slot.insert(name);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(fstype: Option<&str>, folds: bool) -> Rules {
        Rules {
            fstype: fstype.map(str::to_string),
            windows_names: false,
            folds,
        }
    }

    fn members(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_name_fat_refuses_is_named_before_anything_is_written() {
        let names = members(&["set/fine.txt", "set/clip: final.txt"]);
        assert_eq!(refusal(&names, &rules(Some("vfat"), true)).unwrap(),
            "'set/clip: final.txt' cannot be extracted onto this drive: ':' is not allowed in a name on this vfat drive");
        let folder = members(&["bad?/inside.txt"]);
        assert_eq!(refusal(&folder, &rules(Some("exfat"), true)).unwrap(),
            "'bad?/inside.txt' cannot be extracted onto this drive: '?' is not allowed in a name on this exfat drive");
        assert_eq!(
            refusal(&names, &rules(None, false)),
            None,
            "ext4 holds every one of them"
        );
        assert_eq!(
            refusal(&names, &rules(Some("ntfs3"), false)),
            None,
            "ntfs3 without windows_names holds them too"
        );
    }

    #[test]
    fn case_twins_are_refused_only_where_the_drive_folds_case() {
        let names = members(&["pair/a.txt", "pair/A.txt"]);
        assert_eq!(refusal(&names, &rules(None, true)).unwrap(),
            "'pair/A.txt' and 'pair/a.txt' would take one name on this drive, so nothing was extracted");
        assert_eq!(refusal(&names, &rules(None, false)), None);
        let folders = members(&["Docs/a.txt", "docs/b.txt", "docs"]);
        assert_eq!(
            refusal(&folders, &rules(None, true)),
            None,
            "folders that differ only in case merge, and their files do not collide"
        );
        let inside = members(&["Docs/a.txt", "docs/A.TXT"]);
        assert!(
            refusal(&inside, &rules(None, true)).is_some(),
            "the files inside those folders still fold together"
        );
        let file_on_folder = members(&["Readme", "readme/notes.txt"]);
        assert_eq!(
            refusal(&file_on_folder, &rules(None, true)).unwrap(),
            "'readme' and 'Readme' would take one name on this drive, so nothing was extracted"
        );
        let same = members(&["a.txt"]);
        assert_eq!(refusal(&same, &rules(Some("vfat"), true)), None);
    }

    // The job reads the index first, so a case-twin archive bound for a folding folder writes nothing at all.
    #[test]
    fn an_extract_of_case_twins_into_a_folding_folder_publishes_nothing() {
        use crate::backend::archive::Formats;
        use crate::backend::testdir::TestDir;
        use std::sync::atomic::AtomicBool;
        if crate::backend::sandboxprobe::skipped() {
            return;
        }
        let d = TestDir::new("extract-case-twins");
        let pair = d.dir("src/pair");
        std::fs::write(pair.join("a.txt"), "lower").unwrap();
        std::fs::write(pair.join("A.txt"), "UPPER").unwrap();
        let archive = d.join("twins.tar");
        let built = std::process::Command::new("bsdtar")
            .env("LC_ALL", "C.UTF-8")
            .args([
                "-c",
                "-f",
                &archive.to_string_lossy(),
                "-C",
                &d.join("src").to_string_lossy(),
                "pair",
            ])
            .status();
        let Ok(built) = built else {
            eprintln!(
                "SKIP extract case twins: no bsdtar on this box, so the fixture cannot be built"
            );
            return;
        };
        assert!(built.success());
        let out = d.dir("out");
        let formats = Formats::from_tools(true, true);
        casefold::test_stand_folding(&out, true);
        let refused = super::super::extract(
            &formats,
            &archive,
            &out.join("twins"),
            &AtomicBool::new(false),
            None,
        );
        casefold::test_stand_folding(&out, false);
        assert_eq!(refused.unwrap_err().msg, "'pair/A.txt' and 'pair/a.txt' would take one name on this drive, so nothing was extracted");
        assert_eq!(
            std::fs::read_dir(&out).unwrap().count(),
            0,
            "no destination and no staging folder were left"
        );
        super::super::extract(
            &formats,
            &archive,
            &out.join("twins"),
            &AtomicBool::new(false),
            None,
        )
        .expect("a case-sensitive folder takes both");
        assert_eq!(
            std::fs::read_to_string(out.join("twins/pair/A.txt")).unwrap(),
            "UPPER"
        );
    }

    #[test]
    fn both_refusals_read_back_into_their_catalog_keys() {
        let read = |names: &[&str], rules: &Rules| {
            let sentence = refusal(&members(names), rules).unwrap();
            let refusal = flea::ui_logic_status_feedback::errors::extract_refusal(&sentence)
                .unwrap_or_else(|| panic!("{sentence} reads back"));
            (refusal.key, refusal.args)
        };
        assert_eq!(
            read(&["pair/a.txt", "pair/A.txt"], &rules(None, true)),
            ("archive.done.twins", members(&["pair/A.txt", "pair/a.txt"]))
        );
        assert_eq!(
            read(&["set/clip: final.txt"], &rules(Some("vfat"), true)),
            (
                "archive.done.name_refused",
                members(&[
                    "set/clip: final.txt",
                    "':' is not allowed in a name on this vfat drive"
                ])
            )
        );
    }
}

// tests/js/properties.js and propertyfacts.js's walk lines, with the board's Kind, grouped bytes and several-item card.
use super::english;
use super::properties::*;
use crate::jsondoc::parse;

fn drawn(line: &Line) -> String {
    format!("{}{}", line.value, line.tail)
}

fn walk(bytes: u64, items: u64, partial: bool, done: bool) -> Walk {
    Walk { bytes, items, partial, done }
}

fn facts(tags: &[&str]) -> Facts {
    Facts {
        path: "/fixture/note.txt".into(), kind: "File".into(), bytes: 13, modified: 1_758_835_200, mode: "0644".into(),
        owner: "gm".into(), uid: 1000, gid: 1000, tags: tags.iter().map(|t| t.to_string()).collect(), ..Facts::default()
    }
}

fn names(pairs: &[Pair]) -> String {
    let i18n = english();
    pairs.iter().map(|p| match &p.name { Name::Own(label) => i18n.tr(label.key()), Name::Extra(name) => name.clone() }).collect::<Vec<_>>().join(",")
}

#[test]
fn bfx_172_1_folder_size_waits_for_the_recursive_reply() {
    let i18n = english();
    let folder = Facts { directory: true, bytes: 4096, ..facts(&[]) };
    let first = begin(&folder);
    assert_eq!(size(&i18n, folder.bytes, Some(first)).value, "Unknown");
    assert_eq!(first.bytes, 0, "the directory entry is not counted content");
    assert_eq!(size(&i18n, folder.bytes, Some(first)).tail, " \u{b7} counting\u{2026}");
    let counted = next(first, &parse(r#"{"ok":true,"bytes":13,"items":1,"partial":false,"done":true}"#).unwrap());
    assert_eq!(drawn(&size(&i18n, folder.bytes, Some(counted))), "13 B (13 bytes)");
}

#[test]
fn bfx_172_2_zero_unknown_is_distinct_from_exact_zero() {
    let i18n = english();
    for (partial, done) in [(false, false), (true, false), (true, true)] {
        let line = size(&i18n, 4096, Some(walk(0, 0, partial, done)));
        assert_eq!(line.value, "Unknown", "partial={partial}, done={done}");
        assert_eq!(line.tail.is_empty(), done);
    }
    assert_eq!(drawn(&size(&i18n, 4096, Some(walk(0, 0, false, true)))), "0 B (0 bytes)");
    assert_eq!(drawn(&size(&i18n, 0, None)), "0 B (0 bytes)");
    assert_eq!(drawn(&size(&i18n, 4096, Some(walk(13, 1, true, true)))), "> 13 B (13 bytes)");
}

#[test]
fn bfx_172_2_refused_or_cancelled_uncounted_walk_stays_unknown() {
    let i18n = english();
    let folder = Facts { directory: true, bytes: 4096, ..facts(&[]) };
    for reply in [r#"{"ok":false}"#, r#"{"ok":true,"cancelled":true}"#] {
        let stopped = next(begin(&folder), &parse(reply).unwrap());
        assert_eq!(drawn(&size(&i18n, folder.bytes, Some(stopped))), "Unknown");
    }
}

#[test]
fn bfx_172_2_incomplete_zero_combined_size_is_unknown() {
    let i18n = english();
    for (partial, done) in [(false, false), (true, false), (true, true), (false, true)] {
        let several = Several { bytes: 0, partial, done, ..Several::default() };
        let combined = several_pairs(&i18n, &several)[1].line.clone();
        assert_eq!(combined.value, if done && !partial { "0 B" } else { "Unknown" });
        assert_eq!(combined.tail.is_empty(), done);
    }
}

#[test]
fn a_folder_walk_reads_as_a_floor_until_it_ends_whole() {
    let i18n = english();
    let counting = walk(2_104_332_800, 812, false, false);
    assert_eq!(drawn(&size(&i18n, 0, Some(counting))), "> 2.1 GB (2,104,332,800 bytes) \u{b7} counting\u{2026}");
    assert_eq!(drawn(&contains(&i18n, Some(counting)).unwrap()), "> 812 items \u{b7} counting\u{2026}");
    assert_eq!(size(&i18n, 0, Some(counting)).tail, " \u{b7} counting\u{2026}", "the tail is its own run so it can wear the muted ink");
    let done = walk(7_404_118_016, 1204, false, true);
    assert_eq!(drawn(&size(&i18n, 0, Some(done))), "7.4 GB (7,404,118,016 bytes)");
    assert_eq!(drawn(&contains(&i18n, Some(done)).unwrap()), "1,204 items");
    let first = begin(&Facts { bytes: 4096, ..Facts::default() });
    assert_eq!(drawn(&size(&i18n, 4096, Some(first))), "Unknown \u{b7} counting\u{2026}");
    assert_eq!(drawn(&contains(&i18n, Some(first)).unwrap()), "> 0 items \u{b7} counting\u{2026}");
    assert_eq!(drawn(&contains(&i18n, Some(walk(4096, 1, false, true))).unwrap()), "1 item");
    assert_eq!(drawn(&contains(&i18n, Some(walk(4096, 1, false, false))).unwrap()), "> 1 item \u{b7} counting\u{2026}");
    assert_eq!(drawn(&contains(&i18n, Some(walk(4096, 0, false, true))).unwrap()), "0 items");
    let cut = walk(5000, 3, true, true);
    assert_eq!(drawn(&size(&i18n, 0, Some(cut))), "> 5.0 kB (5,000 bytes)", "a partial done Size stays a floor");
    assert_eq!(drawn(&contains(&i18n, Some(cut)).unwrap()), "> 3 items");
    assert_eq!(drawn(&size(&i18n, 13, None)), "13 B (13 bytes)", "a file has no walk");
    assert_eq!(contains(&i18n, None), None, "a file has no Contains");
    assert_eq!(drawn(&size(&i18n, 2_560_562, Some(walk(2_560_562, 11, false, true)))), "2.6 MB (2,560,562 bytes)", "the folder board's grouped bytes");
}

#[test]
fn replies_fold_into_the_walk_and_never_count_forever() {
    let first = walk(4096, 0, false, false);
    let progress = next(first, &parse(r#"{"ok":true,"bytes":9000,"items":5,"partial":false,"done":false}"#).unwrap());
    assert_eq!(progress, walk(9000, 5, false, false));
    let last = next(progress, &parse(r#"{"ok":true,"bytes":9500,"items":6,"partial":false,"done":true}"#).unwrap());
    assert!(last.done);
    let cancel = parse(r#"{"ok":true,"cancelled":true,"bytes":1,"items":1,"partial":false,"done":false}"#).unwrap();
    assert_eq!(next(progress, &cancel), walk(9000, 5, true, true), "a cancelled walk settles as the floor already counted");
    assert_eq!(next(last, &cancel), last, "a cancelled reply after done changes nothing");
    let refused = parse(r#"{"ok":false,"error":"Selected items changed; reopen the menu."}"#).unwrap();
    assert_eq!(next(progress, &refused), walk(9000, 5, true, true), "a refused walk ends as a floor");
}

#[test]
fn pairs_follow_the_boards_order() {
    let i18n = english();
    assert_eq!(names(&pairs(&i18n, &facts(&[]), None, &[], "")), "Path,Kind,Size,Modified,Permissions,Owner,Group", "an untagged item shows no Tags pair");
    let tagged = pairs(&i18n, &facts(&["Red", "Blue"]), None, &[], "Plain text document");
    assert_eq!(names(&tagged), "Path,Kind,Size,Tags,Modified,Permissions,Owner,Group");
    assert_eq!(tagged[3].line.value, "Red, Blue", "the Tags pair holds both tags");
    assert_eq!(tagged[1].line.value, "Plain text document", "Kind names the MIME database's word");
    let reply = parse(r#"{"ok":true,"path":"/fixture/note.txt","kind":"File","bytes":13,"modified":1758835200,"mode":"0644","owner":"gm","uid":1000,"gid":1000}"#).unwrap();
    assert!(Facts::from_reply(&reply).tags.is_empty(), "a reply with no tags key reads as untagged");
    let link = |tags: &[&str]| Facts { symlink: true, target: "/fixture/real.txt".into(), ..facts(tags) };
    let untagged_link = pairs(&i18n, &link(&[]), None, &[], "");
    assert_eq!(untagged_link.last().unwrap().line.value, "/fixture/real.txt", "an untagged link ends with its Link target");
    assert_eq!(names(&untagged_link[untagged_link.len() - 1..]), "Link target");
    let tagged_link = pairs(&i18n, &link(&["Red"]), None, &[], "");
    assert_eq!(format!("{}|{}", names(&tagged_link[tagged_link.len() - 2..]), names(&tagged_link[2..4])), "Group,Link target|Size,Tags");
    let folder = Facts { directory: true, kind: "Directory".into(), ..facts(&["Red"]) };
    let folder_pairs = pairs(&i18n, &folder, Some(begin(&folder)), &[], "");
    assert_eq!(names(&folder_pairs), "Path,Kind,Size,Contains,Tags,Modified,Permissions,Owner,Group");
    assert_eq!(folder_pairs[1].line.value, "Folder", "Directory reads Folder");
    let video = vec![("Codecs".to_string(), "H.264 \u{b7} AAC".to_string()), ("Bit rate".to_string(), "8.1 Mb/s".to_string())];
    assert_eq!(names(&pairs(&i18n, &facts(&[]), None, &video, "")), "Path,Kind,Size,Codecs,Bit rate,Modified,Permissions,Owner,Group");
    let photo: Vec<(String, String)> = ["Camera", "Lens", "Exposure", "Taken"].iter().map(|n| (n.to_string(), "x".to_string())).collect();
    assert_eq!(names(&pairs(&i18n, &facts(&[]), None, &photo, "")), "Path,Kind,Size,Camera,Lens,Exposure,Taken,Modified,Permissions,Owner,Group");
    assert_eq!(names(&pairs(&i18n, &facts(&["Red"]), None, &video, "")), "Path,Kind,Size,Codecs,Bit rate,Tags,Modified,Permissions,Owner,Group");
}

#[test]
fn owner_group_and_kind_words_read_as_the_board_draws_them() {
    let i18n = english();
    let shown = pairs(&i18n, &facts(&[]), None, &[], "");
    assert_eq!(shown[5].line.value, "gm (uid 1000)");
    assert_eq!(shown[6].line.value, "gid 1000");
    let anonymous = pairs(&i18n, &Facts { owner: String::new(), ..facts(&[]) }, None, &[], "");
    assert_eq!(anonymous[5].line.value, "Unknown (uid 1000)");
    assert_eq!(kind_word(&i18n, &Facts { kind: "Symbolic link".into(), ..Facts::default() }, "Plain text document"), "Symbolic link", "a link is named as a link");
    assert_eq!(kind_word(&i18n, &Facts { kind: "File".into(), ..Facts::default() }, ""), "File", "a file with no MIME word falls back");
    assert_eq!(i18n.tr("dialogs.file.properties.title"), "Properties");
    assert_eq!(i18n.tr("dialogs.file.properties.close"), "Close");
}

#[test]
fn several_items_show_kinds_combined_newest_and_oldest_only() {
    let i18n = english();
    let reply = |text: &str| Several::from_reply(&parse(text).unwrap());
    let done = reply(r#"{"t":"menuaction","id":7,"op":"properties","ok":true,"kinds":"2 images, 1 video","bytes":52300000,"newest":"2026-09-23 10:14","oldest":"2026-08-21 12:02","partial":false,"done":true}"#);
    let shown = several_pairs(&i18n, &done);
    assert_eq!(names(&shown), "Kinds,Combined,Newest,Oldest");
    assert_eq!(shown.iter().map(|p| drawn(&p.line)).collect::<Vec<_>>().join("|"), "2 images, 1 video|52.3 MB|2026-09-23 10:14|2026-08-21 12:02");
    let counting = reply(r#"{"ok":true,"kinds":"","bytes":52300000,"newest":"","oldest":"","partial":false,"done":false}"#);
    assert_eq!(drawn(&several_pairs(&i18n, &counting)[1].line), "> 52.3 MB \u{b7} counting\u{2026}");
    let partial = reply(r#"{"ok":true,"kinds":"1 folder","bytes":52300000,"newest":"x","oldest":"y","partial":true,"done":true}"#);
    assert_eq!(drawn(&several_pairs(&i18n, &partial)[1].line), "> 52.3 MB", "a partial final keeps the floor and drops the tail");
    let cancelled = reply(r#"{"t":"menuaction","id":7,"op":"properties","ok":true,"cancelled":true,"partial":true,"done":false}"#);
    assert!(cancelled.cancelled && !cancelled.done);
}

#[test]
fn the_backends_fixed_refusals_map_to_the_cards_own_words() {
    let i18n = english();
    for sentence in [
        "The selected items stopped answering.", "Selected items changed; reopen the menu.", "Selected item moved or disappeared; reopen the menu.",
        "Selected item changed; reopen the menu.", "Could not format the selected items' modification dates.", "The selected items are too large to total.",
        "A menu request is still running; try again when it finishes.", "Menu request cancelled.", "The menu stopped answering; reopen this window.",
    ] {
        assert_eq!(refusal(&i18n, sentence), sentence, "the English key says the backend's sentence");
    }
    assert_eq!(refusal(&i18n, "Could not read the link: gone."), "Could not read the link: gone.", "an unknown sentence stays the backend's");
    assert_eq!(refusal(&i18n, ""), "That action could not be completed.");
    assert_eq!(snapshot_request(7, 3, &[0, 2, 4], 2), r#"{"c":"menuaction","op":"snapshot","id":7,"listing":3,"rows":[0,2,4],"cursor":2}"#);
    assert_eq!(properties_request(7), r#"{"c":"menuaction","op":"properties","id":7}"#);
}

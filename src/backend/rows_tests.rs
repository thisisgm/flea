use super::*;

#[test]
fn rename_source_identity_uses_the_displayed_symlink_inode_without_following_it() {
    use std::os::unix::fs::MetadataExt;
    let root = super::super::testdir::TestDir::new("row-rename-source");
    let target = root.file("target.txt", "original bytes");
    let link = root.join("link.txt");
    root.assert_contains(&link);
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut listing = Listing::new();
    listing.push("link.txt", false);
    let (metas, ms) = super::super::meta::stat_range(root.path(), &listing, 0, 1);
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let line = rows_line(root.path(), &listing, &metas, crate::backend::rows::Window { start: 0, ms }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    let line = super::super::rowguard::stamped(line, 1);
    let flea::protocol::Reply::Rows { rows, .. } = flea::protocol::parse_reply(&line).unwrap() else { panic!("rows receipt") };
    let meta = link.symlink_metadata().unwrap();
    assert_eq!(rows[0].source, Some(format!("{}:{}:{}", meta.dev(), meta.ino(), meta.mode() & 0o170000)));
    assert_ne!(meta.ino(), target.symlink_metadata().unwrap().ino());
}

// Literal databases, so a serialiser test never depends on the box's /usr/share/mime.
pub(super) fn dbs() -> (Db, Names, Aliases, Thumbnailers, Kinds) {
    // The glob names the alias and the thumbnailer the canonical type, so a dropped aliases argument reddens.
    let aliases = Aliases::from_str("image/pjpeg image/jpeg\n");
    let spec = "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=image/jpeg;\n";
    let thumbs =
        Thumbnailers::from_entries(&[("t.thumbnailer".to_string(), spec.to_string())], &aliases);
    // Seeded under the canonical name, the same one kind_for reaches through aliases.canonical first.
    let kinds = Kinds::from_pairs(&[
        ("text/plain", Some("Plain Text Document")),
        ("image/jpeg", Some("JPEG Image")),
    ]);
    (
        Db::from_str("50:text/plain:*.txt\n50:image/pjpeg:*.jpg\n"),
        Names::from_str("text/plain:text-x-generic\n"),
        aliases,
        thumbs,
        kinds,
    )
}

#[test]
fn emits_a_rows_line_with_escaped_names() {
    let mut l = Listing::new();
    l.push("say \"hi\".txt", false);
    l.push("sub", true);
    let metas = vec![
        Meta { ino: None, birth: None, size: 12, mtime: 1787790423, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
        Meta { ino: None, birth: None, size: 4096, mtime: 1787790424, mode: 16877, target_is_dir: false, target: String::new(), dev: 42 },
    ];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 1.25 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(s.starts_with(r#"{"t":"rows","start":0,"rows":["#));
    assert!(s.contains(
        r#""n":"say \"hi\".txt","d":false,"s":12,"m":1787790423,"b":null,"p":33188,"i":"text-x-generic","t":false,"k":0"#
    ));
    assert!(s.contains(r#""n":"sub","d":true,"s":4096,"m":1787790424,"b":null,"p":16877,"i":"folder","t":false,"k":1,"v":42"#));
    assert!(s.ends_with(r#"],"kinds":["Plain Text Document","Folder"],"ms":1.250}"#));
}

#[test]
fn a_symlink_to_a_directory_keeps_d_false_and_gains_the_folder_icon() {
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let mut l = Listing::new();
    l.push("linkdir", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o120777, target_is_dir: true, target: String::new(), dev: 0 }];
    let line = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(line.contains(r#""d":false"#), "d describes the link: {}", line);
    assert!(line.contains(r#""i":"folder""#), "the icon describes the target: {}", line);
}

#[test]
fn only_a_symlink_carries_its_target_and_a_plain_row_carries_no_l_at_all() {
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let mut l = Listing::new();
    l.push("shell", false);
    l.push("notes.txt", false);
    let metas = vec![
        Meta { ino: None, birth: None, size: 18, mtime: 2, mode: 0o120777, target_is_dir: true,
               target: "/usr/share/omarchy".to_string(), dev: 0 },
        Meta { ino: None, birth: None, size: 4, mtime: 5, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
    ];
    let line = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(line.contains(r#""n":"shell","d":false"#), "d still describes the link: {}", line);
    assert!(line.contains(r#""l":"/usr/share/omarchy""#), "the link carries its target: {}", line);
    assert_eq!(line.matches(r#""l":"#).count(), 1, "the plain row carries no l: {}", line);
}

#[test]
fn a_target_with_a_quote_in_it_is_escaped_like_every_other_field() {
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let mut l = Listing::new();
    l.push("odd", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o120777, target_is_dir: false,
                            target: "say \"hi\"/two\nlines".to_string(), dev: 0 }];
    let line = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert_eq!(line.lines().count(), 1, "a newline in a target stays on one line: {}", line);
    assert!(line.contains(r#""l":"say \"hi\"/two\nlines""#), "{}", line);
}

#[test]
fn only_a_row_with_a_declared_thumbnailer_can_be_thumbnailed() {
    let mut l = Listing::new();
    l.push("photo.jpg", false);
    l.push("notes.txt", false);
    // Real st_mode values, because the flag now reads the file type out of the mode.
    let metas = vec![
        Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
        Meta { ino: None, birth: None, size: 4, mtime: 5, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
    ];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(s.contains(r#""n":"photo.jpg","d":false,"s":1,"m":2,"b":null,"p":33188,"i":"image-x-generic","t":true"#));
    assert!(s.contains(r#""n":"notes.txt","d":false,"s":4,"m":5,"b":null,"p":33188,"i":"text-x-generic","t":false"#));
}

#[test]
fn only_a_thumbnailable_appimage_row_carries_the_a_flag() {
    let mut l = Listing::new();
    for name in ["app.AppImage", "setup.exe", "photo.jpg", "sub"] {
        l.push(name, name == "sub");
    }
    let aliases = Aliases::from_str("");
    // The exe has a third-party thumbnailer and the same generic icon, which is the row Images mode must keep as a mark.
    let exe = "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=application/x-ms-dos-executable;\n";
    let thumbs = Thumbnailers::from_entries(&[("e.thumbnailer".to_string(), exe.to_string())], &aliases).with_appimage_route(&aliases, true);
    let mime = Db::from_str("50:application/vnd.appimage:*.AppImage\n50:application/x-ms-dos-executable:*.exe\n50:image/jpeg:*.jpg\n");
    let icons = Names::from_str("application/vnd.appimage:application-x-executable\napplication/x-ms-dos-executable:application-x-executable\n");
    let mut kinds = Kinds::from_pairs(&[]);
    let metas: Vec<Meta> = [33188, 33188, 33188, 16877].iter().map(|&mode| Meta { ino: None, birth: None, size: 1, mtime: 2, mode, target_is_dir: false, target: String::new(), dev: 0 }).collect();
    let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    let rows: Vec<&str> = s.split("},{").collect();
    assert!(rows[0].contains(r#""i":"application-x-executable","t":true"#) && rows[0].contains(r#""a":true"#), "{}", rows[0]);
    assert!(rows[1].contains(r#""i":"application-x-executable","t":true"#), "the exe lost its thumbnailer: {}", rows[1]);
    assert!(!rows[1].contains(r#""a""#), "an exe carried the AppImage flag: {}", rows[1]);
    assert!(!rows[2].contains(r#""a""#) && !rows[3].contains(r#""a""#));
    // No route and no thumbnailer for the type, so the row is not thumbnailable and carries no flag.
    let bare = Thumbnailers::from_entries(&[], &aliases);
    let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &bare, kinds: &mut kinds });
    assert!(!s.contains(r#""a":true"#), "{}", s);
}

#[test]
fn a_newline_in_a_name_stays_on_one_line() {
    let mut l = Listing::new();
    l.push("two\nlines.txt", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 3, target_is_dir: false, target: String::new(), dev: 0 }];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert_eq!(s.lines().count(), 1);
    assert!(s.contains(r#""n":"two\nlines.txt""#));
}

#[test]
fn a_window_at_a_nonzero_start_names_the_right_rows() {
    let mut l = Listing::new();
    l.push("zero.txt", false);
    l.push("one.txt", false);
    l.push("two-dir", true);
    let metas = vec![Meta { ino: None, birth: None, size: 7, mtime: 8, mode: 9, target_is_dir: false, target: String::new(), dev: 77 }];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let mounts = "1 0 0:77 / / rw - ext4 /dev/fixture rw\n";
    assert_eq!(
        rows_line_in(std::path::Path::new("/nonexistent"), mounts, &l, &metas, crate::backend::rows::Window { start: 2, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds }),
        r#"{"t":"rows","start":2,"rows":[{"n":"two-dir","d":true,"s":7,"m":8,"b":null,"p":9,"i":"folder","t":false,"k":0,"v":77}],"kinds":["Folder"],"ms":0.000}"#
    );
}

#[test]
fn identical_kinds_share_one_dictionary_entry() {
    let mut l = Listing::new();
    l.push("a.txt", false);
    l.push("b.txt", false);
    let metas = vec![
        Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 3, target_is_dir: false, target: String::new(), dev: 0 },
        Meta { ino: None, birth: None, size: 4, mtime: 5, mode: 6, target_is_dir: false, target: String::new(), dev: 0 },
    ];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(s.contains(r#""kinds":["Plain Text Document"]"#), "two rows of one type share one entry: {}", s);
    assert_eq!(s.matches(r#""k":0"#).count(), 2, "both rows point at that one index: {}", s);
}

#[test]
fn a_type_nothing_can_describe_reads_as_data_and_never_as_an_icon_name() {
    let mut l = Listing::new();
    l.push("thing.zzzznotreal", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100644, target_is_dir: false, target: String::new(), dev: 0 }];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    // No glob matches, so there is no type to describe. The icon ladder still falls to
    // text-x-generic, but the Kind must not claim the row is text, and must not leak an
    // internal icon name into a human column; see the preview column's Unsupported state.
    assert!(s.contains(r#""kinds":["Data"]"#), "an unresolved type reads as Data: {}", s);
    assert!(!s.contains(r#""kinds":["text-x-generic"]"#));
    assert!(s.contains(r#""i":"text-x-generic""#), "the icon itself is unchanged");
}

#[test]
fn a_control_character_in_a_name_is_escaped() {
    let mut l = Listing::new();
    l.push("bell\u{7}tab\ttwo\nlines", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 3, target_is_dir: false, target: String::new(), dev: 0 }];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert_eq!(s.lines().count(), 1);
    assert!(s.contains(r#""n":"bell\u0007tab\ttwo\nlines""#));
}

#[test]
fn a_start_past_the_end_emits_no_rows() {
    let mut l = Listing::new();
    l.push("only.txt", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 3, target_is_dir: false, target: String::new(), dev: 0 }];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    // run.rs clamps start to l.len() before calling rows_line, see docs/protocol.md.
    assert_eq!(
        rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 1, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds }),
        r#"{"t":"rows","start":1,"rows":[],"kinds":[],"ms":0.000}"#
    );
}

#[test]
fn a_special_file_is_never_offered_as_thumbnailable() {
    let mut l = Listing::new();
    l.push("photo.jpg", false);
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    // A fifo, a socket, a character device and a vanished row, all carrying a name a thumbnailer declares.
    for mode in [0o010644, 0o140644, 0o020644, 0] {
        let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode, target_is_dir: false, target: String::new(), dev: 0 }];
        let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
        assert!(s.contains(r#""i":"image-x-generic","t":false"#), "mode {:o} was offered", mode);
    }
    // The symlink is true, because the request path stats the target before it queues anything.
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o120777, target_is_dir: false, target: String::new(), dev: 0 }];
    let s = rows_line(std::path::Path::new("/nonexistent"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(s.contains(r#""i":"image-x-generic","t":true"#));
}

#[test]
fn a_tagged_row_carries_its_names_while_an_untagged_row_carries_no_field() {
    use crate::backend::testdir::TestDir;
    let d = TestDir::new("rowstags");
    let tagged = d.file("tagged.txt", "body");
    let _plain = d.file("plain.txt", "body");
    crate::backend::tags::write_names(&tagged, &["Blue".to_string(), "Red".to_string()]).unwrap();
    let mut l = Listing::new();
    l.push("tagged.txt", false);
    l.push("plain.txt", false);
    let metas = vec![
        Meta { ino: None, birth: None, size: 4, mtime: 2, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
        Meta { ino: None, birth: None, size: 4, mtime: 2, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
    ];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let s = rows_line(d.path(), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(s.contains(r#""tg":["Blue","Red"]"#), "{}", s);
    assert_eq!(s.matches(r#""tg""#).count(), 1, "only the tagged row carries the field: {}", s);
}

#[test]
fn a_network_base_reads_no_attributes_even_where_a_file_carries_some() {
    use crate::backend::testdir::TestDir;
    let d = TestDir::new("rowstagsremote");
    let tagged = d.file("tagged.txt", "body");
    crate::backend::tags::write_names(&tagged, &["Red".to_string()]).unwrap();
    // An absolute name reaches the tagged file whatever the base claims, so only the class gate can refuse it.
    let absolute = tagged.to_string_lossy().into_owned();
    let mut l = Listing::new();
    l.push(&absolute, false);
    let metas = vec![Meta { ino: None, birth: None, size: 4, mtime: 2, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 }];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let s = rows_line(std::path::Path::new("/run/user/1000/gvfs/smb-share:server=nas,share=media"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(!s.contains(r#""tg""#), "{}", s);
}

#[test]
fn model3d_rows_advertise_only_embedded_thumbnail_formats() {
    for (name,expected) in [("part.stl",false),("part.OBJ",false),("part.ply",false),("part.amf",false),("part.3MF",true),("part.GCO",true),("part.bgcode",true)] {
        let mut listing = Listing::new();
        listing.push(name,false);
        let meta = [Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100644, target_is_dir: false, target: String::new(), dev: 0 }];
        let (mime,icons,aliases,thumbs,mut kinds) = dbs();
        let line = rows_line_in(Path::new("/nonexistent"),"",&listing,&meta,crate::backend::rows::Window { start: 0, ms: 0. },crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
        let line = crate::jsondoc::parse(&line).unwrap();
        let rows = line.get("rows").unwrap().as_array().unwrap();
        assert_eq!(rows[0].get("t").and_then(crate::jsondoc::Json::as_bool),Some(expected),"{name}");
        assert!(crate::backend::model3d::supported(name).is_some());
    }
}

#[test]
fn creation_time_is_serialized_from_the_window_metadata_without_a_filesystem_read() {
    let mut listing=Listing::new(); listing.push("absent",false);
    let meta=[Meta { ino: None, birth: Some(123), size: 1, mtime: 999, mode: 0o100644, target_is_dir: false, target: String::new(), dev: 0 }];
    let (mime,icons,aliases,thumbs,mut kinds)=dbs();
    let line=rows_line(std::path::Path::new("/nonexistent"),&listing,&meta,crate::backend::rows::Window { start: 0, ms: 0.0 },crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    let document=crate::jsondoc::parse(&line).unwrap();
    let row=&document.get("rows").unwrap().as_array().unwrap()[0];
    assert_eq!(row.get("b").unwrap().as_f64(),Some(123.0));
    assert_eq!(row.get("m").unwrap().as_f64(),Some(999.0));
}

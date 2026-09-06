use crate::backend::aliases::Aliases;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::listing::Listing;
use crate::backend::meta::{thumbnailable, Meta};
use crate::backend::mime::Db;
use crate::backend::thumbspec::Thumbnailers;
use crate::json::escape;
use std::collections::HashMap;

// A row is a short name, two flags, three numbers and an icon name, so this rarely reallocates.
const ROW_BYTES_ESTIMATE: usize = 128;

pub fn rows_line(
    l: &Listing,
    metas: &[Meta],
    start: usize,
    ms: f64,
    mime: &Db,
    icons: &Names,
    aliases: &Aliases,
    thumbs: &Thumbnailers,
    kinds: &mut Kinds,
) -> String {
    // Field names stay one character: a window serialises on every scroll.
    let mut s = String::with_capacity(metas.len() * ROW_BYTES_ESTIMATE);
    s.push_str(&format!(r#"{{"t":"rows","start":{},"rows":["#, start));
    // Clamped like stat_range, so a stale start cannot panic the backend.
    let n = metas.len().min(l.len().saturating_sub(start));
    // Distinct Kind strings this one response uses, so the per-row cost is an integer, not a repeated string.
    let mut kind_index: HashMap<String, usize> = HashMap::new();
    let mut kind_list: Vec<String> = Vec::new();
    for (i, m) in metas[..n].iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let name = l.name(start + i);
        let is_dir = l.is_dir(start + i);
        // Looked up once and shared with the icon and the Kind text, because it is the only per-row allocation here.
        let mime_name = mime.lookup(name);
        // A directory or a special file is never thumbnailed, so the spec lookup is skipped for one; see AGENTS.md "Thumbnail requests".
        let can_thumb = thumbnailable(m.mode) && mime_name.is_some_and(|m| thumbs.for_mime(m, aliases).is_some());
        // A symlink to a directory carries d:false by contract, so only the ICON follows the target.
        let folder_icon = is_dir || m.target_is_dir;
        let icon = icons.icon_for(mime_name, folder_icon, m.mode);
        let kind = kind_for(folder_icon, mime_name, icon, aliases, kinds);
        let k = match kind_index.get(&kind) {
            Some(&idx) => idx,
            None => {
                let idx = kind_list.len();
                kind_index.insert(kind.clone(), idx);
                kind_list.push(kind);
                idx
            }
        };
        // Only a directory carries its filesystem id, because only a directory can be a drop
        // destination: canDrop requires d true, so a file row's device would never be read. On the
        // 100k scale fixture, which holds no directories at all, this adds nothing whatsoever.
        let dev = if is_dir { format!(r#","v":{}"#, m.dev) } else { String::new() };
        // Only a symlink carries a target, the same conditional shape v uses, and never both: a symlink's d is false.
        let link = if m.target.is_empty() { String::new() } else { format!(r#","l":"{}""#, escape(&m.target)) };
        // The icon is escaped like every other field: the escape is the contract, not an optimisation.
        s.push_str(&format!(
            r#"{{"n":"{}","d":{},"s":{},"m":{},"p":{},"i":"{}","t":{},"k":{}{}{}}}"#,
            escape(name),
            is_dir,
            m.size,
            m.mtime,
            m.mode,
            escape(icon),
            can_thumb,
            k,
            link,
            dev
        ));
    }
    let kinds_json: String = kind_list.iter().map(|k| format!(r#""{}""#, escape(k))).collect::<Vec<_>>().join(",");
    s.push_str(&format!(r#"],"kinds":[{}],"ms":{:.3}}}"#, kinds_json, ms));
    s
}

// A directory has no MIME type in this model, so its Kind is one hardcoded string, see docs/protocol.md "rows".
// A name no glob matched, and a type with no comment of its own, are both "a kind nothing here can
// describe". They read as Data rather than as an icon name, because an icon name is an internal
// string and the Kind column is a human description. Data is the canvas's own word for it.
pub const UNKNOWN_KIND: &str = "Data";

fn kind_for(folder_icon: bool, mime_name: Option<&str>, _icon: &str, aliases: &Aliases, kinds: &mut Kinds) -> String {
    if folder_icon {
        return "Folder".to_string();
    }
    match mime_name {
        Some(m) => kinds.comment(aliases.canonical(m)).unwrap_or_else(|| UNKNOWN_KIND.to_string()),
        None => UNKNOWN_KIND.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Literal databases, so a serialiser test never depends on the box's /usr/share/mime.
    fn dbs() -> (Db, Names, Aliases, Thumbnailers, Kinds) {
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
            Meta { size: 12, mtime: 1787790423, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
            Meta { size: 4096, mtime: 1787790424, mode: 16877, target_is_dir: false, target: String::new(), dev: 42 },
        ];
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        let s = rows_line(&l, &metas, 0, 1.25, &mime, &icons, &aliases, &thumbs, &mut kinds);
        assert!(s.starts_with(r#"{"t":"rows","start":0,"rows":["#));
        assert!(s.contains(
            r#""n":"say \"hi\".txt","d":false,"s":12,"m":1787790423,"p":33188,"i":"text-x-generic","t":false,"k":0"#
        ));
        assert!(s.contains(r#""n":"sub","d":true,"s":4096,"m":1787790424,"p":16877,"i":"folder","t":false,"k":1,"v":42"#));
        assert!(s.ends_with(r#"],"kinds":["Plain Text Document","Folder"],"ms":1.250}"#));
    }

    #[test]
    fn a_symlink_to_a_directory_keeps_d_false_and_gains_the_folder_icon() {
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        let mut l = Listing::new();
        l.push("linkdir", false);
        let metas = vec![Meta { size: 1, mtime: 2, mode: 0o120777, target_is_dir: true, target: String::new(), dev: 0 }];
        let line = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
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
            Meta { size: 18, mtime: 2, mode: 0o120777, target_is_dir: true,
                   target: "/usr/share/omarchy".to_string(), dev: 0 },
            Meta { size: 4, mtime: 5, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
        ];
        let line = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
        assert!(line.contains(r#""n":"shell","d":false"#), "d still describes the link: {}", line);
        assert!(line.contains(r#""l":"/usr/share/omarchy""#), "the link carries its target: {}", line);
        assert_eq!(line.matches(r#""l":"#).count(), 1, "the plain row carries no l: {}", line);
    }

    #[test]
    fn a_target_with_a_quote_in_it_is_escaped_like_every_other_field() {
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        let mut l = Listing::new();
        l.push("odd", false);
        let metas = vec![Meta { size: 1, mtime: 2, mode: 0o120777, target_is_dir: false,
                                target: "say \"hi\"/two\nlines".to_string(), dev: 0 }];
        let line = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
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
            Meta { size: 1, mtime: 2, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
            Meta { size: 4, mtime: 5, mode: 33188, target_is_dir: false, target: String::new(), dev: 0 },
        ];
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        let s = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
        assert!(s.contains(r#""n":"photo.jpg","d":false,"s":1,"m":2,"p":33188,"i":"image-x-generic","t":true"#));
        assert!(s.contains(r#""n":"notes.txt","d":false,"s":4,"m":5,"p":33188,"i":"text-x-generic","t":false"#));
    }

    #[test]
    fn a_newline_in_a_name_stays_on_one_line() {
        let mut l = Listing::new();
        l.push("two\nlines.txt", false);
        let metas = vec![Meta { size: 1, mtime: 2, mode: 3, target_is_dir: false, target: String::new(), dev: 0 }];
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        let s = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
        assert_eq!(s.lines().count(), 1);
        assert!(s.contains(r#""n":"two\nlines.txt""#));
    }

    #[test]
    fn a_window_at_a_nonzero_start_names_the_right_rows() {
        let mut l = Listing::new();
        l.push("zero.txt", false);
        l.push("one.txt", false);
        l.push("two-dir", true);
        let metas = vec![Meta { size: 7, mtime: 8, mode: 9, target_is_dir: false, target: String::new(), dev: 77 }];
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        assert_eq!(
            rows_line(&l, &metas, 2, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds),
            r#"{"t":"rows","start":2,"rows":[{"n":"two-dir","d":true,"s":7,"m":8,"p":9,"i":"folder","t":false,"k":0,"v":77}],"kinds":["Folder"],"ms":0.000}"#
        );
    }

    #[test]
    fn identical_kinds_share_one_dictionary_entry() {
        let mut l = Listing::new();
        l.push("a.txt", false);
        l.push("b.txt", false);
        let metas = vec![
            Meta { size: 1, mtime: 2, mode: 3, target_is_dir: false, target: String::new(), dev: 0 },
            Meta { size: 4, mtime: 5, mode: 6, target_is_dir: false, target: String::new(), dev: 0 },
        ];
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        let s = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
        assert!(s.contains(r#""kinds":["Plain Text Document"]"#), "two rows of one type share one entry: {}", s);
        assert_eq!(s.matches(r#""k":0"#).count(), 2, "both rows point at that one index: {}", s);
    }

    #[test]
    fn a_type_nothing_can_describe_reads_as_data_and_never_as_an_icon_name() {
        let mut l = Listing::new();
        l.push("thing.zzzznotreal", false);
        let metas = vec![Meta { size: 1, mtime: 2, mode: 3, target_is_dir: false, target: String::new(), dev: 0 }];
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        let s = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
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
        let metas = vec![Meta { size: 1, mtime: 2, mode: 3, target_is_dir: false, target: String::new(), dev: 0 }];
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        let s = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
        assert_eq!(s.lines().count(), 1);
        assert!(s.contains(r#""n":"bell\u0007tab\ttwo\nlines""#));
    }

    #[test]
    fn a_start_past_the_end_emits_no_rows() {
        let mut l = Listing::new();
        l.push("only.txt", false);
        let metas = vec![Meta { size: 1, mtime: 2, mode: 3, target_is_dir: false, target: String::new(), dev: 0 }];
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        // run.rs clamps start to l.len() before calling rows_line, see docs/protocol.md.
        assert_eq!(
            rows_line(&l, &metas, 1, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds),
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
            let metas = vec![Meta { size: 1, mtime: 2, mode, target_is_dir: false, target: String::new(), dev: 0 }];
            let s = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
            assert!(s.contains(r#""i":"image-x-generic","t":false"#), "mode {:o} was offered", mode);
        }
        // The symlink is true, because the request path stats the target before it queues anything.
        let metas = vec![Meta { size: 1, mtime: 2, mode: 0o120777, target_is_dir: false, target: String::new(), dev: 0 }];
        let s = rows_line(&l, &metas, 0, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds);
        assert!(s.contains(r#""i":"image-x-generic","t":true"#));
    }
}

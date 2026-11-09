// A read-only look at a directory that is not the one the pane is on. The columns view needs the
// rows of every ancestor at once, and `list` can only ever hold one listing, so this answers
// without touching it.
use crate::backend::icons::Names;
use crate::backend::mime::Db;
use crate::backend::fsinfo::dev_of;
use crate::backend::ops::dir_writable;
use crate::backend::scan::{mode_of, scan};
use crate::backend::sort::sort_by_name;
use crate::json::escape;

// One peek, answered on a worker when the mount is slow; the columns view's own also arms a watch BEFORE the scan, so no change is lost to its gap.
pub(crate) fn answer(path: &str, (first, details): (usize, bool), hidden: bool, order: Order, focus: String, watch: Option<(std::ffi::c_int, crate::backend::events::Sender<crate::backend::events::Event>)>, tb: &crate::backend::state::Tables) -> String {
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let base = flea::bytename::exact(path);
    let (mime, icons) = (std::sync::Arc::clone(&tb.mime), std::sync::Arc::clone(&tb.icons));
    let owned = path.to_string();
    match super::iomount::call(&base, &body, "peek", move || {
        if let Some((fd, tx)) = watch {
            let wd = super::watch::Watch::add_raw(fd, &flea::bytename::view(&owned));
            let _ = tx.send(crate::backend::events::Event::PeekArmed(wd, flea::bytename::exact(&owned)));
        }
        if details { peek_line_details(&owned, first, true, hidden, order, &focus, (&mime, &icons)) }
        else { peek_line(&owned, first, hidden, order, &focus, &mime, &icons) }
    }) {
        Ok(line) => line,
        Err(_) => format!("{{\"t\":\"peeked\",\"path\":\"{}\",\"hidden\":{},\"hiddenLast\":{},\"first\":{}{},\"n\":0,\"failed\":true,\"rows\":[]}}", escape(path), hidden, order.hidden_last, first, order.echo()),
    }
}

// Where a peek puts dotfiles, and the folder's own remembered order when the column sent one; none is name order.
#[derive(Clone, Copy)]
pub struct Order { pub hidden_last: bool, pub by: Option<(crate::backend::sort::SortBy, bool)> }

impl Order {
    // The wire's by and desc: absent is name order, and a key the wire never defined is refused, as a sort refuses it.
    pub fn wire(hidden_last: bool, by: Option<&str>, desc: bool) -> Result<Order, &'static str> {
        Ok(Order { hidden_last, by: by.map(crate::backend::sort::parse_sort_by).transpose()?.map(|b| (b, desc)) })
    }

    // The request's by and desc, echoed only when it carried them, so a reader that never sent them reads the line it always did.
    fn echo(&self) -> String {
        match self.by {
            Some((by, desc)) => format!(r#","by":"{}","desc":{}"#, crate::backend::sort::sort_by_key(by), desc),
            None => String::new(),
        }
    }
}

// A column draws what fits on screen, so a peek at a huge directory is capped rather than streamed.
pub const PEEK_CAP: usize = 512;

// corner: a symlink draws its own mark rather than its target's, unlike a listing row.
pub fn peek_line(path: &str, first: usize, hidden: bool, order: Order, focus: &str, mime: &Db, icons: &Names) -> String {
    peek_line_details(path, first, false, hidden, order, focus, (mime, icons))
}
fn peek_line_details(path: &str, first: usize, details: bool, hidden: bool, order: Order, focus: &str, tables: (&Db, &Names)) -> String {
    let (mime, icons) = tables;
    let hidden_last = order.hidden_last;
    let (mut listing, _read_ms) = match scan(path, hidden) {
        Ok(v) => v,
        // An unreadable ancestor is still not an error for the pane it belongs to, but it is not an
        // empty directory either, and the column drew those two the same way until this line.
        Err(_) => return failed_peek(path, first, hidden, order),
    };
    // A row is stat'd only for a remembered size or date order, so a name-order peek stays cheap enough to run per ancestor per move.
    match order.by {
        Some((by, desc)) => {
            crate::backend::sort::sort_listing(
                &mut listing,
                &flea::bytename::view(path),
                by,
                desc,
                hidden_last,
            );
        }
        None => {
            sort_by_name(&mut listing, false, hidden_last);
        }
    }
    // The directory's own device and writability, once per peek, so a drag from a column takes the listing's verb.
    let (dev, writable) = (
        dev_of(&flea::bytename::view(path)),
        dir_writable(&flea::bytename::view(path)),
    );
    let total = listing.len();
    let count = first.min(PEEK_CAP).min(total);
    let start = if focus.is_empty() {
        0
    } else {
        (0..total)
            .find(|&i| listing.name(i) == focus)
            .map(|i| i.saturating_sub(count / 2).min(total - count))
            .unwrap_or(0)
    };
    let mut out = String::with_capacity(count * 64);
    // Echoed so peek clients tell replies apart: NetworkMounts asks with 1 and 512 while the columns view asks with the pane's window size.
    out.push_str(&format!(
        r#"{{"t":"peeked","path":"{}","hidden":{},"hiddenLast":{},"first":{}{},"n":{},"v":{},"w":{},"rows":["#,
        escape(path), hidden, hidden_last, first, order.echo(), total, dev, writable
    ));
    for i in start..start + count {
        if i > start {
            out.push(',');
        }
        let name = listing.name(i);
        let dir = listing.is_dir(i);
        // Peek icons use filename facts; optional size metadata does not change their classification.
        let icon = icons.icon_for(mime.lookup(name), dir, 0);
        out.push_str(&format!(
            r#"{{"n":"{}","d":{},"i":"{}"{}}}"#,
            escape(name), dir, escape(icon), super::peekdetail::fields(path, name, details)
        ));
    }
    out.push_str("]}");
    out
}

// A scan that failed answers zero rows, which is the exact count an empty directory answers, so the
// line says which of the two it is. mode carries the directory's own permissions when the stat
// outlived the refused read, and is left out when it did not, the same rule the error line follows.
fn failed_peek(path: &str, first: usize, hidden: bool, order: Order) -> String {
    let mode = mode_of(path);
    let mode_field = if mode == 0 { String::new() } else { format!(r#","mode":{}"#, mode) };
    format!(
        r#"{{"t":"peeked","path":"{}","hidden":{},"hiddenLast":{},"first":{}{},"n":0,"failed":true{},"rows":[]}}"#,
        escape(path), hidden, order.hidden_last, first, order.echo(), mode_field
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;

    // Name order, which every column asks for unless its folder remembers another.
    fn names(hidden_last: bool) -> Order { Order { hidden_last, by: None } }

    fn tables() -> (Db, Names) {
        (Db::load(), Names::load())
    }

    #[test]
    fn a_peek_answers_names_marks_and_the_directory_bit_with_directories_first() {
        let d = TestDir::new("peek");
        d.dir("sub");
        d.file("a.txt", "body");
        d.file("b.png", "");
        let (mime, icons) = tables();
        let line = peek_line(&d.path().to_string_lossy(), 10, false, names(false), "", &mime, &icons);
        assert!(line.starts_with(r#"{"t":"peeked""#));
        // The flag the request carried, so an asker can tell its own reply from the other's.
        assert!(line.contains(r#""hidden":false"#), "the reply says what it was asked for: {}", line);
        // The marker file the sandbox carries is a dotfile, so hidden false drops it.
        assert!(line.contains(r#""n":4"#) || line.contains(r#""n":3"#), "got {}", line);
        let sub = line.find(r#""n":"sub""#).expect("the directory row");
        let txt = line.find(r#""n":"a.txt""#).expect("the file row");
        assert!(sub < txt, "directories sort first, the same as a listing");
        assert!(line.contains(r#""n":"sub","d":true,"i":"folder""#));
        assert!(line.contains(r#""n":"a.txt","d":false"#));
    }

    #[test]
    fn a_peek_names_its_directory_device_and_writability() {
        let d = TestDir::new("peekdev");
        d.file("a.txt", "body");
        let (mime, icons) = tables();
        let path = d.path().to_string_lossy().to_string();
        let line = peek_line(&path, 10, false, names(false), "", &mime, &icons);
        assert!(line.contains(&format!(r#""v":{},"w":true,"#, dev_of(d.path()))), "got {}", line);
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let locked = peek_line(&path, 10, false, names(false), "", &mime, &icons);
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        extern "C" {
            fn geteuid() -> u32;
        }
        // Root passes access(2) on a 0555 directory, so only a non-root run can see it read-only.
        if unsafe { geteuid() } != 0 {
            assert!(locked.contains(r#""w":false,"#), "a read-only directory says so: {}", locked);
        }
    }

    #[test]
    fn hidden_follows_the_same_rule_a_listing_uses() {
        let d = TestDir::new("peekhidden");
        d.file("visible.txt", "");
        d.file(".secret", "");
        let (mime, icons) = tables();
        let shown = peek_line(&d.path().to_string_lossy(), 10, false, names(false), "", &mime, &icons);
        assert!(!shown.contains(".secret"));
        assert!(shown.contains(r#""hidden":false"#));
        let all = peek_line(&d.path().to_string_lossy(), 10, true, names(false), "", &mime, &icons);
        assert!(all.contains(".secret"));
        assert!(all.contains(r#""hidden":true"#), "the two replies differ in the field that tells them apart");
    }

    #[test]
    fn hidden_last_orders_a_peek_like_the_listing_it_sits_beside() {
        let d = TestDir::new("peekhiddenlast");
        d.file("notes.md", "");
        d.file(".cache", "");
        let (mime, icons) = tables();
        let off = peek_line(&d.path().to_string_lossy(), 10, true, names(false), "", &mime, &icons);
        let cache = off.find(r#""n":".cache""#).expect("the dotfile row");
        let notes = off.find(r#""n":"notes.md""#).expect("the visible row");
        assert!(cache < notes, "off keeps today's dotfiles-first order: {}", off);
        let on = peek_line(&d.path().to_string_lossy(), 10, true, names(true), "", &mime, &icons);
        let cache = on.find(r#""n":".cache""#).expect("the dotfile row");
        let notes = on.find(r#""n":"notes.md""#).expect("the visible row");
        assert!(notes < cache, "on keeps every dotfile last, like the listing: {}", on);
    }

    #[test]
    fn a_directory_that_cannot_be_read_is_never_an_error_but_says_it_failed() {
        let d = TestDir::new("peekmissing");
        let (mime, icons) = tables();
        let line = peek_line(&d.join("never-existed").to_string_lossy(), 10, false, names(false), "", &mime, &icons);
        assert!(line.starts_with(r#"{"t":"peeked""#), "still an answer and not an error: {}", line);
        // A refusal is still a reply to one of the two askers, so it carries the flag too.
        assert!(line.contains(r#""hidden":false"#), "a refusal is still a reply to a request: {}", line);
        assert!(line.contains(r#""n":0"#));
        assert!(line.contains(r#""failed":true"#), "a column that could not look says so: {}", line);
        assert!(!line.contains(r#""mode":"#), "nothing to stat, so no mode is claimed: {}", line);
        assert!(line.ends_with(r#""rows":[]}"#));
    }

    #[test]
    fn a_count_of_zero_tells_a_folder_a_file_and_nothing_apart_without_rows() {
        let d = TestDir::new("peekkind");
        let file = d.file("a.txt", "x");
        d.file("b.txt", "y");
        let (mime, icons) = tables();
        let ask = |path: &std::path::Path| peek_line(&path.to_string_lossy(), 0, true, names(false), "", &mime, &icons);
        let folder = ask(d.path());
        assert!(folder.contains(r#""n":3"#) && folder.ends_with(r#""rows":[]}"#) && !folder.contains(r#""failed""#), "a folder counts and lists nothing: {}", folder);
        // A scan of a file fails, and the stat that outlived it carries the regular-file type bits.
        let named = ask(&file);
        let mode: u32 = named.split(r#""mode":"#).nth(1).and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next()).and_then(|digits| digits.parse().ok()).expect("a file's mode");
        assert!(named.contains(r#""failed":true"#) && mode & 0o170000 == 0o100000, "a file is a failed peek of a regular mode: {}", named);
        assert!(!ask(&d.join("never-existed")).contains(r#""mode":"#), "nothing there claims no mode");
    }

    #[test]
    fn an_empty_directory_and_an_unreadable_one_are_two_different_answers() {
        let empty = TestDir::new("peekempty");
        let locked = TestDir::new("peeklocked");
        let inner = locked.dir("noread");
        // Write and enter, never read: opendir is refused while stat still answers.
        std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o300)).unwrap();
        let (mime, icons) = tables();

        // The sandbox marker is a dotfile, so hidden false makes this directory look empty.
        let empty_line = peek_line(&empty.path().to_string_lossy(), 10, false, names(false), "", &mime, &icons);
        let locked_line = peek_line(&inner.to_string_lossy(), 10, false, names(false), "", &mime, &icons);

        assert!(empty_line.contains(r#""n":0"#), "got {}", empty_line);
        assert!(locked_line.contains(r#""n":0"#), "got {}", locked_line);
        assert!(!empty_line.contains(r#""failed""#), "an empty directory is not a failure: {}", empty_line);
        assert!(locked_line.contains(r#""failed":true"#), "got {}", locked_line);
        // 0o40300: the file-type bits plus write and execute, which is what the stat still answers.
        assert!(locked_line.contains(r#""mode":16576"#), "the mode the pane draws: {}", locked_line);

        std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn the_row_count_is_capped_while_the_total_still_tells_the_truth() {
        let d = TestDir::new("peekcap");
        for i in 0..12 {
            d.file(&format!("f{:03}.txt", i), "");
        }
        let (mime, icons) = tables();
        let line = peek_line(&d.path().to_string_lossy(), 5, false, names(false), "", &mime, &icons);
        assert!(line.contains(r#""n":12"#), "the total is the whole directory: {}", line);
        assert_eq!(line.matches(r#""d":"#).count(), 5, "only five rows were asked for");
    }

    #[test]
    fn focused_peek_keeps_a_late_directory_visible_without_expanding_the_window() {
        let d = TestDir::new("peekfocus");
        for i in 0..20 { d.dir(&format!("d{:02}", i)); }
        let (mime, icons) = tables();
        for focus in ["d00", "d10", "d19"] {
            let line = peek_line(&d.path().to_string_lossy(), 5, false, names(false), focus, &mime, &icons);
            assert!(line.contains(&format!(r#""n":"{}""#, focus)), "{}", line);
            assert_eq!(line.matches(r#""d":"#).count(), 5);
        }
        let initial = peek_line(&d.path().to_string_lossy(), 5, false, names(false), "", &mime, &icons);
        let missing = peek_line(&d.path().to_string_lossy(), 5, false, names(false), "gone", &mime, &icons);
        assert_eq!(missing, initial);
        let zero = peek_line(&d.path().to_string_lossy(), 0, false, names(false), "d19", &mime, &icons);
        assert!(zero.ends_with(r#""rows":[]}"#));
    }

    #[test]
    fn a_peeked_line_echoes_hidden_last_the_way_it_echoes_hidden() {
        let d = TestDir::new("peekecho");
        d.file("a.txt", "");
        let (mime, icons) = tables();
        let off = peek_line(&d.path().to_string_lossy(), 10, false, names(false), "", &mime, &icons);
        assert!(off.contains(r#""hiddenLast":false"#), "the reply says which order it was asked for: {}", off);
        let on = peek_line(&d.path().to_string_lossy(), 10, true, names(true), "", &mime, &icons);
        assert!(on.contains(r#""hiddenLast":true"#), "a stale dotfiles-first column must not survive the toggle: {}", on);
        let mixed = peek_line(&d.path().to_string_lossy(), 10, false, names(true), "", &mime, &icons);
        assert!(mixed.contains(r#""hidden":false"#) && mixed.contains(r#""hiddenLast":true"#), "the mixed pair echoes each flag in its own place: {}", mixed);
        let swapped = peek_line(&d.path().to_string_lossy(), 10, true, names(false), "", &mime, &icons);
        assert!(swapped.contains(r#""hidden":true"#) && swapped.contains(r#""hiddenLast":false"#), "the reverse pair does too: {}", swapped);
        let missing = peek_line(&d.join("never-existed").to_string_lossy(), 10, true, names(true), "", &mime, &icons);
        assert!(missing.contains(r#""hiddenLast":true"#), "a refusal is still a reply to a request: {}", missing);
    }

    #[test]
    fn a_name_needing_escapes_survives_the_hand_built_line() {
        let d = TestDir::new("peekescape");
        d.file(r#"say "hi".txt"#, "");
        let (mime, icons) = tables();
        let line = peek_line(&d.path().to_string_lossy(), 10, false, names(false), "", &mime, &icons);
        assert!(line.contains(r#"say \"hi\".txt"#), "got {}", line);
    }

    #[test]
    fn a_peeked_line_echoes_first_so_a_repair_reply_cannot_land_in_a_column() {
        let d = TestDir::new("peekfirst");
        d.file("a.txt", "");
        let (mime, icons) = tables();
        let one = peek_line(&d.path().to_string_lossy(), 1, false, names(false), "", &mime, &icons);
        assert!(one.contains(r#""first":1"#), "the column tells its own reply from a repair peek: {}", one);
        let wide = peek_line(&d.path().to_string_lossy(), 512, false, names(false), "", &mime, &icons);
        assert!(wide.contains(r#""first":512"#), "got {}", wide);
        let missing = peek_line(&d.join("never-existed").to_string_lossy(), 1, true, names(true), "", &mime, &icons);
        assert!(missing.contains(r#""first":1"#), "a refusal is still a reply to a request: {}", missing);
    }

    #[test]
    fn a_remembered_size_order_reaches_the_column_and_name_order_stays_the_default() {
        let d = TestDir::new("peekorder");
        // Name order is small, big, mid, so no order below can pass through the name branch by accident.
        d.file("a-small", &"x".repeat(10));
        d.file("b-big", &"x".repeat(300));
        d.file("c-mid", &"x".repeat(100));
        let (mime, icons) = tables();
        let path = d.path().to_string_lossy().to_string();
        let at = |line: &str, name: &str| line.find(&format!(r#""n":"{}""#, name)).unwrap_or_else(|| panic!("no {} row: {}", name, line));
        let by_size = peek_line(&path, 10, false, Order { hidden_last: false, by: Some((crate::backend::sort::SortBy::Size, true)) }, "", &mime, &icons);
        assert!(at(&by_size, "b-big") < at(&by_size, "c-mid") && at(&by_size, "c-mid") < at(&by_size, "a-small"), "size descending, largest first: {}", by_size);
        let reversed = peek_line(&path, 10, false, Order { hidden_last: false, by: Some((crate::backend::sort::SortBy::Name, true)) }, "", &mime, &icons);
        assert!(at(&reversed, "c-mid") < at(&reversed, "b-big") && at(&reversed, "b-big") < at(&reversed, "a-small"), "name order reversed: {}", reversed);
        let none = peek_line(&path, 10, false, names(false), "", &mime, &icons);
        assert!(at(&none, "a-small") < at(&none, "b-big") && at(&none, "b-big") < at(&none, "c-mid"), "no order asked is name order: {}", none);
    }

    #[test]
    fn a_peeked_line_echoes_by_and_desc_only_when_the_request_carried_them() {
        use crate::backend::sort::SortBy;
        let d = TestDir::new("peekbyecho");
        d.file("a.txt", "");
        let (mime, icons) = tables();
        let path = d.path().to_string_lossy().to_string();
        let sized = peek_line(&path, 10, false, Order { hidden_last: false, by: Some((SortBy::Size, true)) }, "", &mime, &icons);
        assert!(sized.contains(r#""first":10,"by":"size","desc":true,"#), "the reply says which folder order sorted it: {}", sized);
        let dated = peek_line(&path, 10, false, Order { hidden_last: false, by: Some((SortBy::Mtime, false)) }, "", &mime, &icons);
        assert!(dated.contains(r#""by":"mtime","desc":false"#), "the echo is the wire spelling the request used: {}", dated);
        let plain = peek_line(&path, 10, false, names(false), "", &mime, &icons);
        assert!(!plain.contains(r#""by":"#) && !plain.contains(r#""desc":"#), "a request without an order reads the line it always did: {}", plain);
        let missing = peek_line(&d.join("never-existed").to_string_lossy(), 10, false, Order { hidden_last: false, by: Some((SortBy::Name, true)) }, "", &mime, &icons);
        assert!(missing.contains(r#""by":"name","desc":true"#), "a refusal is still a reply to a request: {}", missing);
    }

    #[test]
    fn the_wire_order_maps_by_and_desc_and_refuses_a_key_it_never_defined() {
        use crate::backend::sort::SortBy;
        assert!(matches!(Order::wire(true, Some("size"), true), Ok(Order { hidden_last: true, by: Some((SortBy::Size, true)) })));
        assert!(matches!(Order::wire(false, Some("mtime"), false), Ok(Order { hidden_last: false, by: Some((SortBy::Mtime, false)) })));
        assert!(matches!(Order::wire(false, None, true), Ok(Order { hidden_last: false, by: None })));
        assert_eq!(Order::wire(false, Some("kind"), false).err(), Some("no such sort key; send name, size or mtime"));
    }
}

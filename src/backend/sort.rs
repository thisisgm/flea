use crate::backend::dirsize::DirSize;
use crate::backend::listing::Listing;
use crate::backend::metasort::sort_by_stat;
use std::cmp::Ordering;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub enum SortBy {
    Name,
    Size,
    Mtime,
}

// The key is the wire's own spelling. A key this wire never defined is refused with the one sentence
// on the right, never answered as name order: a mark reading Kind over a name-ordered listing would
// be a lie, see docs/protocol.md "sort".
pub fn parse_sort_by(s: &str) -> Result<SortBy, &'static str> {
    match s {
        "name" => Ok(SortBy::Name),
        "size" => Ok(SortBy::Size),
        "mtime" => Ok(SortBy::Mtime),
        _ => Err("no such sort key; send name, size or mtime"),
    }
}

// The wire spelling parse_sort_by reads, so a reply can echo the key its request carried.
pub fn sort_by_key(by: SortBy) -> &'static str {
    match by {
        SortBy::Name => "name",
        SortBy::Size => "size",
        SortBy::Mtime => "mtime",
    }
}

// The loop's one entry: name reads phase 1 alone, size and date pay the metadata pass, and hidden_last keeps dotfiles out of the way without a second pass.
pub fn sort_listing(l: &mut Listing, base: &Path, by: SortBy, desc: bool, hidden_last: bool) -> (f64, f64, Vec<Option<DirSize>>) {
    match by {
        SortBy::Name => (0.0, sort_by_name(l, desc, hidden_last), Vec::new()),
        SortBy::Size | SortBy::Mtime => sort_by_stat(l, base, by, desc, hidden_last),
    }
}

// The order the operator sees, and it is macOS Finder's rather than the byte order a raw cmp gives.
// Three properties, applied per segment as the two names are walked together:
//   1. a run of digits compares by value, so file_2 comes before file_10
//   2. letters compare case-insensitively, so Apple sits beside apple instead of in a block above it
//   3. leading zeros do not change a value, so file_01 and file_1 are equal here
// corner: ASCII only. Finder also collates a diacritic next to its base letter, and that needs a
// collation table; this tree ships zero dependencies with Cargo.lock byte-identical as a gate, so a
// byte above 0x7f falls back to byte order, which for UTF-8 is code point order.
fn finder_cmp(a: &[u8], b: &[u8]) -> Ordering {
    let mut i = 0;
    let mut j = 0;
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (a_end, a_sig) = digit_run(a, i);
            let (b_end, b_sig) = digit_run(b, j);
            match cmp_digit_runs(&a[a_sig..a_end], &b[b_sig..b_end]) {
                Ordering::Equal => {}
                other => return other,
            }
            i = a_end;
            j = b_end;
            continue;
        }
        let ca = a[i].to_ascii_lowercase();
        let cb = b[j].to_ascii_lowercase();
        if ca != cb {
            return ca.cmp(&cb);
        }
        i += 1;
        j += 1;
    }
    // Whichever ran out first is the shorter name; both running out is a match so far.
    (a.len() - i).cmp(&(b.len() - j))
}

// The run of digits starting at `at`, as (one past its last digit, its first significant digit).
fn digit_run(s: &[u8], at: usize) -> (usize, usize) {
    let mut end = at;
    while end < s.len() && s[end].is_ascii_digit() {
        end += 1;
    }
    // At least one digit always survives, so a run of nothing but zeros compares as a single zero.
    let mut sig = at;
    while sig + 1 < end && s[sig] == b'0' {
        sig += 1;
    }
    (end, sig)
}

// Two runs with their leading zeros already skipped. A 40 digit run overflows every integer type,
// so parsing into u64 is the obvious implementation and it is wrong: more digits is a larger number,
// and the same count of digits is settled by comparing them as bytes.
fn cmp_digit_runs(a: &[u8], b: &[u8]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => a.cmp(b),
        other => other,
    }
}

// Case-insensitivity makes README and readme equal, and a stable sort would then keep readdir order,
// which is not reproducible across runs. The raw bytes break the tie, so a listing of one directory
// is the same listing every time.
pub fn name_order(a: &[u8], b: &[u8]) -> Ordering {
    match finder_cmp(a, b) {
        Ordering::Equal => a.cmp(b),
        other => other,
    }
}

// Hidden is a leading dot, the same rule scan.rs filters on.
pub fn is_hidden(name: &str) -> bool {
    name.as_bytes().first() == Some(&b'.')
}

// Kind tie-break from name bytes alone: text after the last dot, None for no dot, a lone leading dot or a trailing dot.
pub fn extension(name: &str) -> Option<&str> {
    let base = name.rsplit('/').next().unwrap_or(name);
    let dot = base.rfind('.')?;
    if dot == 0 {
        return None;
    }
    let ext = &base[dot + 1..];
    if ext.is_empty() { None } else { Some(ext) }
}

// None sorts before any extension; two extensions use the Finder comparison without the byte tie-break.
pub fn cmp_extension(a: &str, b: &str) -> Ordering {
    match (extension(a), extension(b)) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(ea), Some(eb)) => finder_cmp(ea.as_bytes(), eb.as_bytes()),
    }
}

pub fn sort_by_name(l: &mut Listing, desc: bool, hidden_last: bool) -> f64 {
    let t = Instant::now();
    // Take the buffer out so the comparator can borrow it while spans are moved.
    let names = std::mem::take(&mut l.names);
    l.spans.sort_by(|a, b| {
        let an = &names[a.off as usize..(a.off + a.len) as usize];
        let bn = &names[b.off as usize..(b.off + b.len) as usize];
        // Hidden entries follow every visible one in both directions; only names inside each block reverse.
        if hidden_last {
            match (is_hidden(an), is_hidden(bn)) {
                (true, false) => return Ordering::Greater,
                (false, true) => return Ordering::Less,
                _ => {}
            }
        }
        match (a.is_dir, b.is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => {
                // desc is the exact reverse of asc, tie-break included. as_bytes is a cast, not a
                // conversion: the arena is a String, so the spans slice to &str and the comparator
                // wants bytes.
                if desc {
                    name_order(bn.as_bytes(), an.as_bytes())
                } else {
                    name_order(an.as_bytes(), bn.as_bytes())
                }
            }
        }
    });
    l.names = names;
    t.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::listing::Listing;

    fn sample() -> Listing {
        let mut l = Listing::new();
        l.push("zebra.txt", false);
        l.push("alpha.txt", false);
        l.push("zzz-dir", true);
        l.push("aaa-dir", true);
        l
    }

    #[test]
    fn directories_come_first_then_name_order() {
        let mut l = sample();
        sort_by_name(&mut l, false, false);
        assert_eq!(l.name(0), "aaa-dir");
        assert_eq!(l.name(1), "zzz-dir");
        assert_eq!(l.name(2), "alpha.txt");
        assert_eq!(l.name(3), "zebra.txt");
    }

    #[test]
    fn descending_reverses_names_but_keeps_directories_first() {
        let mut l = sample();
        sort_by_name(&mut l, true, false);
        assert_eq!(l.name(0), "zzz-dir");
        assert_eq!(l.name(1), "aaa-dir");
        assert_eq!(l.name(2), "zebra.txt");
        assert_eq!(l.name(3), "alpha.txt");
    }

    #[test]
    fn sorting_an_empty_listing_does_not_panic() {
        let mut l = Listing::new();
        sort_by_name(&mut l, false, false);
        assert_eq!(l.len(), 0);
    }

    // A mark reading Kind over a name-ordered listing would be a lie, so an unknown key is Err, never Name.
    #[test]
    fn the_three_orders_parse_and_every_other_key_is_refused_by_one_sentence() {
        assert!(matches!(parse_sort_by("name"), Ok(SortBy::Name)));
        assert!(matches!(parse_sort_by("size"), Ok(SortBy::Size)));
        assert!(matches!(parse_sort_by("mtime"), Ok(SortBy::Mtime)));
        // The two header columns that are not orders, the empty key a missing by parses to, and the
        // wrong case, because the key is the wire's own spelling.
        for key in ["kind", "mode", "", "Name"] {
            assert_eq!(parse_sort_by(key).err(), Some("no such sort key; send name, size or mtime"), "key {:?}", key);
        }
    }

    #[test]
    fn sort_listing_routes_name_to_the_phase_one_order_and_never_stats() {
        let mut l = sample();
        // A base that does not exist: name order never looks at it, so nothing here can fail.
        let (pass, _, _) = sort_listing(&mut l, Path::new("/definitely/not/here"), SortBy::Name, true, false);
        assert_eq!(pass, 0.0, "name pays no metadata pass");
        assert_eq!(l.name(0), "zzz-dir");
        assert_eq!(l.name(3), "alpha.txt");
    }

    // Every edge case section 9 enumerated, exercised rather than remembered. The helper sorts a
    // flat list of names so a case reads as the order it asserts.
    fn ordered(names: &[&str]) -> Vec<String> {
        let mut l = Listing::new();
        for n in names {
            l.push(n, false);
        }
        sort_by_name(&mut l, false, false);
        (0..l.len()).map(|i| l.name(i).to_string()).collect()
    }

    #[test]
    fn a_digit_run_compares_by_value_and_not_by_byte() {
        assert_eq!(ordered(&["file_10", "file_2", "file_11", "file_1"]),
                   vec!["file_1", "file_2", "file_10", "file_11"]);
        // Digits leading the name, and several runs in one name.
        assert_eq!(ordered(&["10file", "2file", "1file"]), vec!["1file", "2file", "10file"]);
        assert_eq!(ordered(&["v1.2.10", "v1.2.9", "v1.10.0"]),
                   vec!["v1.2.9", "v1.2.10", "v1.10.0"]);
    }

    #[test]
    fn letters_compare_case_insensitively_and_do_not_form_an_upper_case_block() {
        assert_eq!(ordered(&["b", "A", "a", "B"]), vec!["A", "a", "B", "b"]);
        // The byte compare this replaces put every capital first, so README led the listing.
        assert_eq!(ordered(&["apple.txt", "Banana.txt", "README"]),
                   vec!["apple.txt", "Banana.txt", "README"]);
    }

    #[test]
    fn leading_zeros_do_not_change_a_value_and_the_tie_is_still_total() {
        // Numerically equal, so the raw bytes decide, and they decide the same way every run.
        let once = ordered(&["file_1", "file_01", "file_001"]);
        let again = ordered(&["file_001", "file_1", "file_01"]);
        assert_eq!(once, again, "the same names must produce the same listing whatever readdir said");
        assert_eq!(once, vec!["file_001", "file_01", "file_1"]);
        assert_eq!(ordered(&["README", "readme"]), vec!["README", "readme"]);
        assert_eq!(ordered(&["readme", "README"]), vec!["README", "readme"]);
    }

    #[test]
    fn a_digit_run_longer_than_any_integer_still_compares() {
        // 40 digits, which overflows u64 and u128 both, so a parsing comparator answers wrongly.
        let big = format!("f{}", "9".repeat(40));
        let bigger = format!("f{}", "9".repeat(41));
        let padded = format!("f{}{}", "0".repeat(30), "9".repeat(40));
        assert_eq!(ordered(&[&bigger, &big]), vec![big.clone(), bigger.clone()],
                   "more significant digits is the larger number");
        // Thirty leading zeros are worth nothing, so this is the same value as big.
        let pair = ordered(&[&padded, &big]);
        assert_eq!(pair.len(), 2);
        assert!(pair.contains(&big) && pair.contains(&padded));
    }

    #[test]
    fn names_with_no_digits_only_digits_and_a_leading_dot_all_order() {
        assert_eq!(ordered(&["zebra", "apple"]), vec!["apple", "zebra"]);
        assert_eq!(ordered(&["10", "9", "1"]), vec!["1", "9", "10"]);
        // A dot is 0x2e, below every digit and letter, so dotfiles lead exactly as they did.
        assert_eq!(ordered(&[".b", "a", ".a"]), vec![".a", ".b", "a"]);
    }

    // Hidden files last: a dotfile follows every visible entry in both directions with folders first kept.
    #[test]
    fn hidden_last_keeps_dotfiles_after_visible_with_folders_first_on_and_off() {
        fn placed(names: &[(&str, bool)], desc: bool, hidden_last: bool) -> Vec<String> {
            let mut l = Listing::new();
            for (n, dir) in names {
                l.push(n, *dir);
            }
            sort_by_name(&mut l, desc, hidden_last);
            (0..l.len()).map(|i| l.name(i).to_string()).collect()
        }
        // Pushed in an order that is none of the answers.
        let names = [("notes.md", false), (".bashrc", false), ("Work", true),
                     (".cache", true), ("apple.txt", false)];
        assert_eq!(placed(&names, false, false),
                   vec![".cache", "Work", ".bashrc", "apple.txt", "notes.md"]);
        assert_eq!(placed(&names, false, true),
                   vec!["Work", "apple.txt", "notes.md", ".cache", ".bashrc"]);
        // Descending reverses inside each block while folders still lead it.
        assert_eq!(placed(&names, true, true),
                   vec!["Work", "notes.md", "apple.txt", ".cache", ".bashrc"]);
        // A dot alone is still hidden, and an empty listing stays empty either way.
        let mut l = Listing::new();
        l.push(".", false);
        l.push("a", false);
        sort_by_name(&mut l, false, true);
        assert_eq!((l.name(0), l.name(1)), ("a", "."));
        let mut empty = Listing::new();
        sort_by_name(&mut empty, false, true);
        assert_eq!(empty.len(), 0);
    }

    // A name that is not UTF-8 reaches this comparator as its exact text, see src/bytename.rs. What
    // this pins is a multi-byte UTF-8 name: above ASCII there is no case folding and no collation,
    // which is exactly what the comparator's corner tag promises.
    #[test]
    fn a_name_above_ascii_orders_by_code_point_and_sorts_after_every_ascii_letter() {
        assert_eq!(ordered(&["\u{e9}clair.txt", "apple.txt", "Zebra.txt"]),
                   vec!["apple.txt", "Zebra.txt", "\u{e9}clair.txt"]);
        // Same base letter, different case, and no collation joins them: e is ASCII and is folded,
        // the accented one is two bytes starting 0xc3 and is not.
        assert_eq!(ordered(&["\u{e9}", "E", "e"]), vec!["E", "e", "\u{e9}"]);
    }

    #[test]
    fn descending_is_the_exact_reverse_including_the_tie_break() {
        let names = ["file_2", "file_10", "README", "readme", "apple"];
        let mut asc = Listing::new();
        let mut desc = Listing::new();
        for n in names {
            asc.push(n, false);
            desc.push(n, false);
        }
        sort_by_name(&mut asc, false, false);
        sort_by_name(&mut desc, true, false);
        let up: Vec<String> = (0..asc.len()).map(|i| asc.name(i).to_string()).collect();
        let mut down: Vec<String> = (0..desc.len()).map(|i| desc.name(i).to_string()).collect();
        down.reverse();
        assert_eq!(up, down);
    }

    #[test]
    fn extension_reads_the_last_dot_of_the_base_name() {
        assert_eq!(extension("clip.mp4"), Some("mp4"));
        assert_eq!(extension("a.tar.gz"), Some("gz"));
        assert_eq!(extension("Makefile"), None);
        assert_eq!(extension(".bashrc"), None);
        assert_eq!(extension("foo."), None);
        assert_eq!(extension("v1.2/notes.txt"), Some("txt"));
        assert_eq!(extension("v1.2/notes"), None);
        assert_eq!(cmp_extension("a.lrf", "b.mp4"), std::cmp::Ordering::Less);
        assert_eq!(cmp_extension("README", "a.lrf"), std::cmp::Ordering::Less);
    }

    #[test]
    fn kind_breaks_ties_by_extension_then_name() {
        use crate::backend::mime::Db;
        use crate::backend::ordering::ordered;
        let d = crate::backend::testdir::TestDir::new("kind-ext");
        let db = Db::from_str("50:video/mp4:*.mp4\n50:video/mp4:*.lrf\n50:video/mp4:README\n50:video/mp4:.bashrc\n");
        let pushed = || {
            let mut l = Listing::new();
            for n in ["b.MP4", "a.lrf", "c.LRF", "README", ".bashrc", "z.mp4"] { l.push(n, false); }
            l
        };
        let names = |l: &Listing| (0..l.len()).map(|i| l.name(i).to_string()).collect::<Vec<_>>();
        let mut asc = pushed();
        ordered(&mut asc, d.path(), &db, "kind", false, true, false, false).unwrap();
        assert_eq!(names(&asc), [".bashrc", "README", "a.lrf", "c.LRF", "b.MP4", "z.mp4"]);
        let mut desc = pushed();
        ordered(&mut desc, d.path(), &db, "kind", true, true, false, false).unwrap();
        let mut rev = names(&asc); rev.reverse();
        assert_eq!(names(&desc), rev);
        let mut f = pushed(); f.push("zebra", true);
        ordered(&mut f, d.path(), &db, "kind", false, true, false, false).unwrap();
        assert_eq!(f.name(0), "zebra");
    }
}

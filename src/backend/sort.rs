use crate::backend::listing::Listing;
use crate::backend::metasort::sort_by_stat;
use std::cmp::Ordering;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Copy)]
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

// The one entry the loop calls: name works on phase-1 data alone, size and date pay the metadata
// pass first. Answers (pass ms, sort ms), which the listed line carries as read and sort.
pub fn sort_listing(l: &mut Listing, base: &Path, by: SortBy, desc: bool) -> (f64, f64) {
    match by {
        SortBy::Name => (0.0, sort_by_name(l, desc)),
        SortBy::Size | SortBy::Mtime => sort_by_stat(l, base, by, desc),
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

pub fn sort_by_name(l: &mut Listing, desc: bool) -> f64 {
    let t = Instant::now();
    // Take the buffer out so the comparator can borrow it while spans are moved.
    let names = std::mem::take(&mut l.names);
    l.spans.sort_by(|a, b| {
        let an = &names[a.off as usize..(a.off + a.len) as usize];
        let bn = &names[b.off as usize..(b.off + b.len) as usize];
        match (an.starts_with('.'), bn.starts_with('.')) {
            // Hidden entries form the tail of the listing in either direction; within the regular
            // and hidden halves, directories still lead files and names reverse independently.
            (false, true) => Ordering::Less,
            (true, false) => Ordering::Greater,
            _ => match (a.is_dir, b.is_dir) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => {
                    // as_bytes is a cast, not a conversion: the arena is a String, so the spans
                    // slice to &str and the comparator wants bytes.
                    if desc {
                        name_order(bn.as_bytes(), an.as_bytes())
                    } else {
                        name_order(an.as_bytes(), bn.as_bytes())
                    }
                }
            },
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
        sort_by_name(&mut l, false);
        assert_eq!(l.name(0), "aaa-dir");
        assert_eq!(l.name(1), "zzz-dir");
        assert_eq!(l.name(2), "alpha.txt");
        assert_eq!(l.name(3), "zebra.txt");
    }

    #[test]
    fn hidden_entries_follow_all_regular_entries_and_keep_directories_first() {
        let mut l = Listing::new();
        for (name, dir) in [(".z-file", false), ("z-file", false), (".a-dir", true),
                            ("a-file", false), ("z-dir", true), (".a-file", false),
                            ("a-dir", true), (".z-dir", true)] {
            l.push(name, dir);
        }
        sort_by_name(&mut l, false);
        assert_eq!((0..l.len()).map(|i| l.name(i)).collect::<Vec<_>>(),
                   ["a-dir", "z-dir", "a-file", "z-file", ".a-dir", ".z-dir", ".a-file", ".z-file"]);
        sort_by_name(&mut l, true);
        assert_eq!((0..l.len()).map(|i| l.name(i)).collect::<Vec<_>>(),
                   ["z-dir", "a-dir", "z-file", "a-file", ".z-dir", ".a-dir", ".z-file", ".a-file"]);
    }

    #[test]
    fn descending_reverses_names_but_keeps_directories_first() {
        let mut l = sample();
        sort_by_name(&mut l, true);
        assert_eq!(l.name(0), "zzz-dir");
        assert_eq!(l.name(1), "aaa-dir");
        assert_eq!(l.name(2), "zebra.txt");
        assert_eq!(l.name(3), "alpha.txt");
    }

    #[test]
    fn sorting_an_empty_listing_does_not_panic() {
        let mut l = Listing::new();
        sort_by_name(&mut l, false);
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
        let (pass, _) = sort_listing(&mut l, Path::new("/definitely/not/here"), SortBy::Name, true);
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
        sort_by_name(&mut l, false);
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
        // The dot still orders names inside the hidden tail; it no longer lifts that tail above a.
        assert_eq!(ordered(&[".b", "a", ".a"]), vec!["a", ".a", ".b"]);
    }

    // A name that is not UTF-8 never reaches this comparator: the arena is a String and scan.rs:16
    // already converts lossily at the readdir, with its own corner tag there. What does reach it is
    // a multi-byte UTF-8 name, and above ASCII there is no case folding and no collation, which is
    // exactly what the comparator's corner tag promises.
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
        sort_by_name(&mut asc, false);
        sort_by_name(&mut desc, true);
        let up: Vec<String> = (0..asc.len()).map(|i| asc.name(i).to_string()).collect();
        let mut down: Vec<String> = (0..desc.len()).map(|i| desc.name(i).to_string()).collect();
        down.reverse();
        assert_eq!(up, down);
    }
}

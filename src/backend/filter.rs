use super::listing::Listing;
use crate::json::escape;
use flea::ui_logic_listing_filter::narrow;
use std::fmt::Write;
use std::time::Instant;

// Filtering reads the name arena only; original indices keep paths and operations in the same numbering.
// Filter040: exact rows in listing order; with none, the close rows ride a `close` member in rank order.
pub fn filtered_line(listing: &Listing, query: &str, id: usize) -> String {
    let started = Instant::now();
    let mut line = format!(r#"{{"t":"filtered","id":{id},"query":"{}","n":{},"rows":"#, escape(query), listing.len());
    match narrow(query, (0..listing.len()).map(|row| (row, listing.name(row)))) {
        None => line.push_str("null"),
        Some(narrowed) => {
            push_rows(&mut line, &narrowed.exact);
            if narrowed.exact.is_empty() && !narrowed.close.is_empty() {
                line.push_str(",\"close\":");
                push_rows(&mut line, &narrowed.close);
            }
        }
    }
    write!(line, ",\"ms\":{:.3}}}", started.elapsed().as_secs_f64() * 1000.0).unwrap();
    line
}

fn push_rows(line: &mut String, rows: &[usize]) {
    line.push('[');
    for (at, row) in rows.iter().enumerate() {
        if at > 0 { line.push(','); }
        write!(line, "{row}").unwrap();
    }
    line.push(']');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::{field_str, field_usize_array};

    #[test]
    fn filtering_keeps_order_identity_and_case_insensitive_substrings_without_a_filesystem() {
        let mut listing = Listing::new();
        for name in ["Zeta", "CAFÉ.txt", "alpha.txt", "sub/Alpha.png", "quote\"\n.txt"] { listing.push(name, false); }
        assert_eq!(field_usize_array(&filtered_line(&listing, "ALP", 1), "rows"), vec![2, 3]);
        assert_eq!(field_usize_array(&filtered_line(&listing, "café", 2), "rows"), vec![1]);
        assert_eq!(field_usize_array(&filtered_line(&listing, "ZETA", 3), "rows"), vec![0]);
        assert_eq!(field_usize_array(&filtered_line(&listing, "missing", 4), "rows"), Vec::<usize>::new());
        let quoted = filtered_line(&listing, "\"\n", 5);
        assert_eq!(field_str(&quoted, "query").as_deref(), Some("\"\n"));
        assert_eq!(field_usize_array(&quoted, "rows"), vec![4]);
        assert!(filtered_line(&listing, "", 6).contains("\"rows\":null"));
        assert_eq!(listing.name(2), "alpha.txt");
    }

    #[test]
    fn empty_listing_long_queries_and_wildcards_follow_filter040() {
        assert!(filtered_line(&Listing::new(), "a", 0).contains("\"rows\":[]"));
        let mut listing = Listing::new();
        listing.push("a b.txt", false);
        for needle in ["a b.txt extra", " a", "b ", "a*z", "a?z"] {
            let line = filtered_line(&listing, needle, 0);
            assert!(line.contains("\"rows\":[]") && !line.contains("\"close\""), "{needle}: {line}");
        }
        // * and ? are wildcards behind an implied leading star; without them the query is a substring.
        for needle in ["a b", "a*b", "a?b", "*.txt", "?.txt"] {
            assert_eq!(field_usize_array(&filtered_line(&listing, needle, 0), "rows"), vec![0], "{needle}");
        }
    }

    #[test]
    fn close_rows_ride_only_beside_an_empty_exact_list_in_rank_order() {
        let mut listing = Listing::new();
        for name in ["zeta.md", "notes.txt", "nuts.txt", "notes-old.txt"] { listing.push(name, false); }
        let typo = filtered_line(&listing, "ntoes", 1);
        assert_eq!(field_usize_array(&typo, "rows"), Vec::<usize>::new());
        let close = field_usize_array(&typo, "close");
        assert!(!close.is_empty() && close.contains(&1) && !close.contains(&0), "{typo}");
        let exact = filtered_line(&listing, "notes", 2);
        assert_eq!(field_usize_array(&exact, "rows"), vec![1, 3]);
        assert!(!exact.contains("\"close\""), "an exact match hides every close one: {exact}");
        assert!(!filtered_line(&listing, "qqqqqq", 3).contains("\"close\""));
    }

    #[test]
    fn the_answer_bytes_match_the_window_consumer_fixture() {
        // crates/flea-gui/src/window/filter_tests.rs replays these bytes, stamped, through the window's transport.
        let mut listing = Listing::new();
        for name in ["docs", "notes.txt", "nuts.txt", "zeta.md"] { listing.push(name, false); }
        let typo = filtered_line(&listing, "ntoes", 2);
        assert_eq!(typo.split(",\"ms\":").next(), Some(r#"{"t":"filtered","id":2,"query":"ntoes","n":4,"rows":[],"close":[1]"#));
        let exact = filtered_line(&listing, "t*s", 3);
        assert_eq!(exact.split(",\"ms\":").next(), Some(r#"{"t":"filtered","id":3,"query":"t*s","n":4,"rows":[1,2]"#));
    }
}

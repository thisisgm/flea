use super::listing::Listing;
use crate::json::escape;
use std::fmt::Write;
use std::time::Instant;

// Filtering reads the name arena only; original indices keep paths and operations in the same numbering.
pub fn filtered_line(listing: &Listing, query: &str, id: usize) -> String {
    let started = Instant::now();
    let mut line = format!(r#"{{"t":"filtered","id":{id},"query":"{}","n":{},"rows":"#, escape(query), listing.len());
    if query.is_empty() {
        line.push_str("null");
    } else {
        let needle = query.to_lowercase();
        line.push('[');
        let mut first = true;
        for row in 0..listing.len() {
            if !matches(listing.name(row), &needle) { continue; }
            if !first { line.push(','); }
            first = false;
            write!(line, "{row}").unwrap();
        }
        line.push(']');
    }
    write!(line, ",\"ms\":{:.3}}}", started.elapsed().as_secs_f64() * 1000.0).unwrap();
    line
}

fn matches(name: &str, needle: &str) -> bool {
    if name.is_ascii() && needle.is_ascii() {
        name.as_bytes().windows(needle.len()).any(|part| part.eq_ignore_ascii_case(needle.as_bytes()))
    } else {
        name.to_lowercase().contains(needle)
    }
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
    fn empty_listing_and_long_or_literal_queries_do_not_match() {
        assert!(filtered_line(&Listing::new(), "a", 0).contains("\"rows\":[]"));
        let mut listing = Listing::new();
        listing.push("a b.txt", false);
        for needle in ["a b.txt extra", "a*b", "a?b", " a", "b "] {
            assert!(filtered_line(&listing, needle, 0).contains("\"rows\":[]"), "{needle}");
        }
        assert_eq!(field_usize_array(&filtered_line(&listing, "a b", 0), "rows"), vec![0]);
    }
}

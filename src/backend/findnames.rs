//! Yazi's smart-case regular-expression traversal, without filtering or statting entries.
use super::{findnormalizer::Normalizer, listing::Listing};
use crate::jsondoc::Json;

pub fn line(listing: &Listing, query: &str, id: usize) -> String {
    let compiled = Normalizer::normalize(query)
        .map_err(|e| e.to_string())
        .and_then(|pattern| {
            regex::bytes::RegexBuilder::new(&pattern)
                .case_insensitive(!query.chars().any(char::is_uppercase))
                .build()
                .map_err(|e| e.to_string())
        });
    let (rows, error) = match compiled {
        Ok(regex) => (
            (0..listing.len())
                .filter(|&i| regex.is_match(listing.name(i).as_bytes()))
                .collect::<Vec<_>>(),
            String::new(),
        ),
        Err(error) => (Vec::new(), error.to_string()),
    };
    crate::jsondoc::render(&Json::Obj(vec![
        ("t".into(), Json::Str("found".into())),
        ("query".into(), Json::Str(query.into())),
        ("id".into(), Json::Num(id.to_string())),
        (
            "rows".into(),
            Json::Arr(rows.iter().map(|i| Json::Num(i.to_string())).collect()),
        ),
        ("error".into(), Json::Str(error)),
    ]))
    .replace('\n', "")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smart_regex_keeps_the_listing_indices_and_reports_invalid_patterns() {
        let mut l = Listing::new();
        for name in ["Alpha.txt", "alpha.rs", "ß.txt", "other"] {
            l.push(name, false);
        }
        let rows = |query| {
            crate::jsondoc::parse(&line(&l, query, 7))
                .unwrap()
                .get("rows")
                .cloned()
                .unwrap()
        };
        assert_eq!(rows("^alpha"), crate::jsondoc::parse("[0,1]").unwrap());
        assert_eq!(rows("^Alpha"), crate::jsondoc::parse("[0]").unwrap());
        assert_eq!(rows("\\.txt$"), crate::jsondoc::parse("[0,2]").unwrap());
        let reply = line(&l, "^alpha", 7);
        assert_eq!(
            reply.lines().count(),
            1,
            "one JSON object per protocol line"
        );
        let stamped = super::super::rowguard::stamped(reply, 9);
        assert_eq!(
            crate::jsondoc::parse(&stamped)
                .unwrap()
                .get("listing")
                .unwrap()
                .as_f64(),
            Some(9.0)
        );
        assert!(!crate::jsondoc::parse(&line(&l, "[", 7))
            .unwrap()
            .get("error")
            .unwrap()
            .as_str()
            .unwrap()
            .is_empty());
        assert_eq!(l.name(0), "Alpha.txt");
    }
}

pub fn requested(listing: &Listing, query: &str, id: usize, request: &str) -> String {
    if crate::json::field_bool(request, "traverse") {
        line(listing, query, id)
    } else {
        super::filter::filtered_line(listing, query, id)
    }
}

#[cfg(test)]
mod normalization_tests {
    use super::*;
    #[test]
    fn yazi_normalization_matches_composed_and_decomposed_names() {
        let mut l = Listing::new();
        for name in ["mäc", "ma\u{308}c", "MAC", "猫"] {
            l.push(name, false);
        }
        let rows = |query| {
            crate::jsondoc::parse(&line(&l, query, 1))
                .unwrap()
                .get("rows")
                .cloned()
                .unwrap()
        };
        assert_eq!(rows("mäc"), crate::jsondoc::parse("[0,1]").unwrap());
        assert_eq!(rows("^m.c$"), crate::jsondoc::parse("[0,1,2]").unwrap());
        let invalid = crate::jsondoc::parse(&line(&l, "猫[", 7)).unwrap();
        assert!(!invalid.get("error").unwrap().as_str().unwrap().is_empty());
        assert!(invalid.get("rows").unwrap().as_array().unwrap().is_empty());
    }
}

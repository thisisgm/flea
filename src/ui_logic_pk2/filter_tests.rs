use super::filter::{filter_request, list_fields};
use super::request::FilterDef;

fn chip(globs: &[&str], mimes: &[&str]) -> FilterDef {
    FilterDef {
        label: "Pictures".to_owned(),
        globs: globs.iter().map(|g| g.to_string()).collect(),
        mimes: mimes.iter().map(|m| m.to_string()).collect(),
    }
}

#[test]
fn the_chip_globs_reach_the_backend_verbatim_for_fnmatch_to_decide_case() {
    let png = chip(&["*.[pP][nN][gG]"], &[]);
    assert_eq!(filter_request(Some(&png)), (vec!["*.[pP][nN][gG]".to_owned()], Vec::new()));
    assert!(list_fields(Some(&png), false).contains(r#""pickerGlobs":["*.[pP][nN][gG]"]"#));
}

#[test]
fn a_chip_names_its_mimes_in_the_same_request() {
    let text = chip(&["*.zip"], &["image/*", "text/plain"]);
    let fields = list_fields(Some(&text), false);
    assert!(
        fields.contains(r#""pickerMimes":["image/*","text/plain"]"#),
        "mimes ride on the list request: {fields}"
    );
}

#[test]
fn no_chip_sends_empty_arrays_so_every_file_stays() {
    assert_eq!(list_fields(None, false), r#""pickerGlobs":[],"pickerMimes":[],"hidden":false"#);
}

#[test]
fn the_hidden_toggle_is_the_same_field_the_backend_reads() {
    assert!(list_fields(None, true).ends_with(r#""hidden":true"#));
    assert!(list_fields(None, false).ends_with(r#""hidden":false"#));
}

#[test]
fn a_quote_in_a_glob_is_escaped_so_the_request_stays_one_json_document() {
    let odd = chip(&["a\"b*"], &[]);
    let fields = list_fields(Some(&odd), false);
    assert!(fields.contains(r#""pickerGlobs":["a\"b*"]"#), "the quote is escaped: {fields}");
    assert!(crate::jsondoc::parse(&format!("{{{fields}}}")).is_ok(), "the fields parse as members");
}

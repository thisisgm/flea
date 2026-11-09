//! Port of tests/js/picker.js request/label/chip assertions plus tab order.
use super::request::*;
use super::testkit::english;

fn req(text: &str) -> Request {
    parse_request(text)
}

#[test]
fn a_non_json_request_still_opens() {
    let r = req("not json");
    assert_eq!(r.mode, "open");
    assert!(r.filters.is_empty());
    assert!(!r.multiple);
}

#[test]
fn an_absent_mode_is_never_save() {
    assert_eq!(req("{\"mode\":\"nonsense\"}").mode, "open");
    assert_eq!(req("{}").mode, "open");
}

#[test]
fn title_fallbacks_name_the_window_purpose() {
    let i18n = english();
    assert_eq!(title_of(&i18n, &req("{}")), "Choose file");
    assert_eq!(title_of(&i18n, &req("{\"directory\":true}")), "Choose folder");
    assert_eq!(title_of(&i18n, &req("{\"mode\":\"save\"}")), "Save File");
    assert_eq!(title_of(&i18n, &req("{\"title\":\"Send to unraid\"}")), "Send to unraid");
}

#[test]
fn no_app_id_draws_no_requester_line() {
    let i18n = english();
    assert_eq!(subtitle_app(&i18n, &req("{}")), None);
    assert_eq!(
        subtitle_app(&i18n, &req("{\"app\":\"org.gnome.gedit\"}")),
        Some("Requested by org.gnome.gedit".to_owned())
    );
    assert_eq!(subtitle_app(&i18n, &req("{\"app\":\"a%1b\"}")), Some("Requested by a%1b".to_owned()));
}

#[test]
fn the_collision_stem_ends_at_the_last_dot() {
    assert_eq!(super::save::stem_end("Downloading the Source.html"), 22);
    assert_eq!(super::save::stem_end("a.tar.gz"), 5);
    assert_eq!(super::save::stem_end("README"), 6);
    assert_eq!(super::save::stem_end(".bashrc"), 7);
    assert_eq!(super::save::stem_end(""), 0);
}

#[test]
fn tab_order_walks_chrome_rail_view_save_answers() {
    fn stop(name: &str) -> TabStop {
        TabStop {
            name: name.to_owned(),
            visible: true,
            enabled: true,
            tab: true,
        }
    }
    let chrome = [stop("Back"), stop("Up")];
    let rail = stop("Rail");
    let view = stop("List");
    let save = [stop("Filename")];
    let answers = [stop("Cancel"), stop("Answer")];
    assert_eq!(
        tab_order(&chrome, &rail, &view, &save, &answers).join(","),
        "Back,Up,Rail,List,Filename,Cancel,Answer"
    );
    let off = TabStop {
        name: "Back".to_owned(),
        visible: true,
        enabled: false,
        tab: true,
    };
    assert_eq!(
        tab_order(&[off, stop("Up")], &rail, &view, &[], &answers).join(","),
        "Up,Rail,List,Cancel,Answer"
    );
    let untabbed = TabStop {
        name: "Up".to_owned(),
        visible: true,
        enabled: true,
        tab: false,
    };
    assert_eq!(tab_order(&[untabbed], &rail, &view, &[], &[]).join(","), "Rail,List");
}

#[test]
fn chips_list_every_filter_then_all_files() {
    let i18n = english();
    let r = req("{\"filters\":[{\"label\":\"Images\",\"globs\":[\"*.png\"],\"mimes\":[]},{\"label\":\"Any text\",\"globs\":[],\"mimes\":[\"text/plain\"]}],\"current\":\"Images\"}");
    let rows = chips(&i18n, &r);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].label, "Images");
    assert_eq!(rows[2].label, "All files");
    assert_eq!(rows[2].index, -1);
    assert!(chips(&i18n, &req("{}")).is_empty());
    assert_eq!(current_chip(&r), 0);
    assert_eq!(current_chip(&req("{\"filters\":[{\"label\":\"A\"}],\"current\":\"Z\"}")), 0);
    assert_eq!(current_chip(&req("{}")), -1);
}

#[test]
fn accept_status_and_hints_match_the_oracle() {
    let i18n = english();
    let open = req("{\"accept\":\"Send\",\"multiple\":true}");
    assert_eq!(accept_label(&i18n, &open, 3), "Send 3");
    assert_eq!(accept_label(&i18n, &open, 1), "Send");
    assert_eq!(accept_label(&i18n, &req("{}"), 0), "Open");
    assert_eq!(accept_label(&i18n, &req("{\"directory\":true}"), 0), "Choose folder");
    assert_eq!(status_line(&i18n, 0, 0), "0 selected");
    assert_eq!(status_line(&i18n, 3, 2_100_000), "3 selected \u{b7} 2.1 MB");
    assert_eq!(status_line(&i18n, 1, 99), "1 selected \u{b7} 99 B");
    assert_eq!(hints(&i18n, &open), "space select \u{b7} enter open/send \u{b7} esc cancel");
    assert_eq!(hints(&i18n, &req("{}")), "enter open \u{b7} esc cancel");
    assert_eq!(hints(&i18n, &req("{\"mode\":\"save\"}")), "enter save \u{b7} esc cancel");
    assert_eq!(
        hints(&i18n, &req("{\"multiple\":true,\"directory\":true}")),
        "enter open \u{b7} space mark folder \u{b7} esc cancel"
    );
    assert_eq!(hints_filtered(&i18n), "enter open \u{b7} esc clears");
}

#[test]
fn every_label_comes_from_the_reserved_catalog_prefix() {
    // Every picker.browser key resolves and is read here.
    let i18n = english();
    assert_eq!(recent_label(&i18n), "Recent");
    let catalog = crate::jsondoc::parse(include_str!("../../i18n/en/flea.json")).expect("the English catalog parses");
    let pairs = catalog.as_object().expect("the catalog is an object");
    let picker: Vec<&(String, crate::jsondoc::Json)> = pairs.iter().filter(|(key, _)| key.starts_with("picker.browser.")).collect();
    assert_eq!(picker.len(), 38, "the reserved prefix holds exactly the chooser keys");
    for (key, value) in picker {
        match value.as_str() {
            Some(text) => assert_eq!(i18n.tr(key), text, "{key} resolves to its catalog text"),
            None => assert_ne!(i18n.trn(key, 2), *key, "{key} is a plural object that resolves"),
        }
    }
}

#[test]
fn a_portal_request_keeps_its_non_ascii_text_through_the_json_escapes() {
    // tools/flea-portal writes json.dumps, which escapes every non-ASCII character as a u escape.
    let line = r#"{"mode": "save", "title": "Datei \u00f6ffnen", "folder": "/home/u/T\u00e9l\u00e9chargements", "name": "R\u00e9sum\u00e9.pdf", "files": [], "filters": []}"#;
    let r = req(line);
    assert_eq!(r.title, "Datei \u{f6}ffnen");
    assert_eq!(r.folder, "/home/u/T\u{e9}l\u{e9}chargements");
    assert_eq!(r.name, "R\u{e9}sum\u{e9}.pdf");
}

#[test]
fn a_caller_title_never_redirects_another_field() {
    let line = r#"{"mode": "open", "title": "folder", "app": "org.example.App", "folder": "/home/u/Docs", "file": "", "name": ""}"#;
    let r = req(line);
    assert_eq!(r.folder, "/home/u/Docs");
    assert_eq!(r.app, "org.example.App");
}

#[test]
fn savefiles_keeps_its_files() {
    let line = r#"{"mode": "savefiles", "title": "", "folder": "/home/u", "files": ["/tmp/a.txt", "/tmp/b.txt"]}"#;
    let r = req(line);
    assert_eq!(r.mode, "savefiles");
    assert_eq!(r.files, vec!["/tmp/a.txt".to_owned(), "/tmp/b.txt".to_owned()]);
}

#[test]
fn a_filter_missing_its_globs_keeps_its_own_label_and_no_other_filters_globs() {
    let line = r#"{"mode": "open", "filters": [{"label": "All"}, {"label": "Text", "globs": ["*.txt"], "mimes": ["text/*"]}], "currentIndex": 1}"#;
    let r = req(line);
    assert_eq!(r.filters.len(), 2);
    assert_eq!(r.filters[0].label, "All");
    assert!(r.filters[0].globs.is_empty(), "the first filter keeps no glob of the second");
    assert_eq!(r.filters[1].globs, vec!["*.txt".to_owned()]);
    assert_eq!(r.filters[1].mimes, vec!["text/*".to_owned()]);
    assert_eq!(r.current_index, 1);
}

#[test]
fn an_absent_current_index_is_minus_one_and_a_broken_request_still_opens() {
    assert_eq!(req(r#"{"mode": "open"}"#).current_index, -1);
    let broken = req(r#"{"mode": "save", "title": "x""#);
    assert_eq!(broken.mode, "open", "a request that does not parse opens as a plain open");
    assert_eq!(broken.title, "");
}

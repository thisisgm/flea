//! Listing tests: total, window, filtering, cursor, stale, death.
use super::listing::{Cause, Escape, Listing, SubstringMatcher};
use flea::i18n::{I18n, Paths};
use flea::protocol::RowFacts;
use flea::ui_logic_pk2::FilterDef;

fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-pk2-listing");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

fn row(name: &str, dir: bool) -> RowFacts {
    RowFacts {
        depth: 0,
        open: false,
        name: name.to_owned(),
        dir,
        size: 0,
        mtime: 0,
        mode: 0o644,
        icon: String::new(),
        link: None,
        real_modes: true,
        thumbless: false,
        kind: String::new(),
        inline: None,
        tags: None,
    }
}

fn png() -> FilterDef {
    FilterDef {
        label: "PNG".to_owned(),
        globs: vec!["*.[pP][nN][gG]".to_owned()],
        mimes: vec![],
    }
}

// A listing of `total` rows whose first window holds the given rows.
fn landed(total: usize, window: Vec<RowFacts>) -> Listing {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(list.landed(gen, total));
    assert!(list.held_rows(gen, 0, window));
    list
}

#[test]
fn the_count_is_the_backend_total_and_only_the_window_holds_rows() {
    let list = landed(100_000, vec![row("Docs", true), row("a.txt", false)]);
    let facts = list.facts();
    assert_eq!((facts.total, facts.first, facts.held), (100_000, 0, 2));
    assert_eq!(facts.names, vec!["Docs".to_owned(), "a.txt".to_owned()]);
    assert_eq!(
        list.window_request(2, 60).as_deref(),
        Some(r#"{"c":"window","start":2,"count":60}"#)
    );
}

#[test]
fn a_window_past_the_total_is_refused_not_held() {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(list.landed(gen, 2));
    assert!(
        !list.held_rows(gen, 1, vec![row("a", false), row("b", false)]),
        "two rows from index 1 overrun a total of 2"
    );
    assert_eq!(list.facts().held, 0);
}

#[test]
fn the_caller_chip_and_hidden_rule_ride_on_the_request_for_the_backend() {
    let mut list = Listing::default();
    list.open("/home/u/Pics");
    list.set_filter(Some(png()), 1);
    let line = list.request_line(60);
    assert!(
        line.contains(r#""pickerGlobs":["*.[pP][nN][gG]"]"#),
        "the glob reaches the backend verbatim: {line}"
    );
    assert!(
        line.contains(r#""pickerMimes":[]"#) && line.contains(r#""hidden":false"#),
        "no mime, dotfiles stay out: {line}"
    );
    list.set_hidden(true);
    assert!(
        list.request_line(60).contains(r#""hidden":true"#),
        "the hidden toggle is part of the same request"
    );
}

#[test]
fn a_row_the_backend_kept_is_never_dropped_again_by_the_gui() {
    let mut list = Listing::default();
    list.open("/d");
    list.set_filter(Some(png()), 1);
    let gen = list.generation();
    assert!(list.landed(gen, 2));
    assert!(list.held_rows(
        gen,
        0,
        vec![row("photo.png", false), row("notes.txt", false)]
    ));
    assert!(
        list.facts().names.contains(&"photo.png".to_owned()),
        "the GUI keeps every row the backend answered"
    );
}

#[test]
fn a_stale_reply_never_lands_on_a_newer_listing() {
    let mut list = Listing::default();
    list.open("/first");
    let old = list.generation();
    list.open("/second");
    assert!(!list.landed(old, 5), "the first folder's count is refused");
    assert_eq!(list.facts().total, 0);
    assert!(list.is_pending());
}

#[test]
fn a_cancelled_listing_refuses_its_late_answer() {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    list.cancel();
    assert!(!list.landed(gen, 5));
    assert!(
        !list.fail(gen, Cause::Gone),
        "a cancelled worker's death is refused too"
    );
    assert!(!list.facts().failed);
}

#[test]
fn backend_death_after_the_listing_landed_is_reported_and_keeps_its_rows() {
    let mut list = landed(2, vec![row("Docs", true), row("photo.png", false)]);
    let gen = list.generation();
    assert!(list.fail(gen, Cause::Gone));
    let facts = list.facts();
    assert!(facts.failed);
    assert_eq!(facts.state, "dead");
    assert_eq!(
        facts.names.len(),
        2,
        "held rows stay, as PickerWindow.qml keeps them"
    );
    assert_eq!(
        list.failure_line(&english()),
        "Flea stopped responding, so this folder could not be listed."
    );
}

#[test]
fn a_reply_that_does_not_parse_fails_the_listing_and_is_never_an_empty_folder() {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(
        list.receive(gen, "{\"t\":"),
        "a broken line is an answer that fails the listing"
    );
    let facts = list.facts();
    assert!(facts.failed);
    assert_ne!(facts.state, "empty");
    assert_eq!(
        list.failure_line(&english()),
        "This folder could not be listed."
    );
}

#[test]
fn a_malformed_reply_after_landing_keeps_the_rows_it_could_not_replace() {
    let mut list = landed(2, vec![row("Docs", true), row("a.txt", false)]);
    let gen = list.generation();
    assert!(list.receive(gen, "not json at all"));
    assert_eq!(list.facts().names.len(), 2);
    assert!(list.facts().failed);
}

#[test]
fn a_listing_that_never_started_says_restart() {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(list.fail(gen, Cause::Unstarted));
    assert_eq!(
        list.failure_line(&english()),
        "This folder could not be listed. Restart Flea."
    );
}

#[test]
fn a_deep_clear_drops_the_held_rows_and_the_window_is_asked_again() {
    let mut list = landed(3, vec![row("a", false), row("b", false), row("c", false)]);
    list.deep_clear();
    assert!(
        list.facts().names.is_empty(),
        "a deep clear leaves no drawn name behind"
    );
    assert_eq!(list.facts().total, 3, "the count survives a deep clear");
    assert!(list.window_request(0, 60).is_some());
}

#[test]
fn hunt_moves_the_cursor_to_the_first_drawn_match_only() {
    let mut list = landed(
        3,
        vec![
            row("Docs", true),
            row("photo.png", false),
            row("notes.txt", false),
        ],
    );
    assert_eq!(list.hunt(&mut SubstringMatcher::of("NOTES")), Some(2));
    assert_eq!(list.facts().cursor, 2);
    assert_eq!(list.hunt(&mut SubstringMatcher::of("zzz")), None);
    assert_eq!(
        list.facts().cursor,
        2,
        "a miss leaves the cursor where it was"
    );
}

#[test]
fn facts_name_the_cursor_file_only_for_files() {
    let mut list = landed(2, vec![row("Docs", true), row("a.txt", false)]);
    assert_eq!(
        list.facts().cursor_file,
        None,
        "the cursor starts on a folder"
    );
    list.move_cursor(1);
    assert_eq!(list.facts().cursor_file.as_deref(), Some("a.txt"));
}

#[test]
fn an_empty_landed_listing_says_empty_not_ready() {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(list.landed(gen, 0));
    assert_eq!(list.facts().state, "empty");
    assert!(!list.facts().failed);
}

#[test]
fn a_huge_row_start_is_refused_without_overflow() {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(list.landed(gen, 2));
    let line = format!(
        r#"{{"t":"rows","start":{},"listing":{gen},"rows":[{{"n":"a","d":false,"s":0,"m":0,"p":420,"i":""}}]}}"#,
        usize::MAX
    );
    assert!(!list.receive(gen, &line));
    assert_eq!(list.facts().held, 0);
}

// PickerWindow.qml:503 says the backend msg for scan failure.
#[test]
fn a_scan_failure_keeps_the_backend_reason() {
    let mut list = Listing::default();
    list.open("/mnt/nas/d");
    let gen = list.generation();
    assert!(list.receive(
        gen,
        r#"{"t":"error","where":"scan","path":"/mnt/nas/d","msg":"/mnt/nas is not responding."}"#
    ));
    assert!(list.facts().failed);
    let line = list.failure_line(&english());
    assert!(
        line.contains("/mnt/nas is not responding."),
        "the reason is gone: {line}"
    );
}

#[test]
fn a_sort_failure_keeps_the_backend_reason() {
    let mut list = landed(2, vec![row("Docs", true), row("a.txt", false)]);
    let gen = list.generation();
    assert!(list.receive(
        gen,
        r#"{"t":"error","where":"sort","path":"size","msg":"bad sort key"}"#
    ));
    assert!(list.facts().failed);
    assert!(list.failure_line(&english()).contains("bad sort key"));
}

// PickerWindow.qml:496-503: only scan/sort fail; the rest says sentences.
#[test]
fn a_thumbnail_or_window_error_does_not_fail_the_listing() {
    for line in [
        r#"{"t":"error","where":"thumb","path":"/d/a.png","msg":"/mnt/nas is not responding."}"#,
        r#"{"t":"error","where":"window","path":"/d","msg":"/mnt/nas is not responding."}"#,
    ] {
        let mut list = landed(100, vec![row("Docs", true), row("a.png", false)]);
        let gen = list.generation();
        assert!(
            list.receive(gen, line),
            "{line} is an answer the footer says"
        );
        assert!(!list.facts().failed, "{line} failed the whole listing");
        assert_eq!(list.facts().notice, "/mnt/nas is not responding.");
        assert!(
            list.window_request(2, 60).is_some(),
            "{line} stopped every later window"
        );
    }
}

// Board filter via the backend request; matches outside rows found.
#[test]
fn the_first_match_outside_the_held_window_is_found_through_the_backend_filter() {
    // A 100,000-row folder; notes.txt is row 50,000.
    let mut list = landed(100_000, vec![row("Docs", true), row("a.txt", false)]);
    let line = list.filter_line("notes");
    assert!(
        line.contains(r#""c":"filter""#),
        "no backend filter is asked for: {line}"
    );
    let gen = list.generation();
    assert!(
        list.receive(gen, r#"{"t":"filtered","id":1,"query":"notes","n":100000,"rows":[50000],"ms":0.5,"listing":1}"#),
        "the filtered answer never lands"
    );
    let facts = list.facts();
    assert!(!facts.failed);
    let filtered = facts
        .filtered
        .as_ref()
        .expect("the shown count draws from the filtered answer");
    assert_eq!((filtered.total, filtered.rows.len()), (100_000, 1));
    assert_eq!(
        facts.cursor, 50_000,
        "the cursor moves to the first match of the folder, not of the held window"
    );
}

// rev-g2-pk2-r3 proof: an empty query is no filter.
#[test]
fn rev_r3_an_empty_query_is_no_filter_and_esc_cancels() {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(list.landed(gen, 9));
    assert!(list.held_rows(gen, 0, vec![row("Docs", true), row("notes.txt", false)]));
    list.filter_line("");
    let answer = format!(
        r#"{{"t":"filtered","id":1,"query":"","n":9,"rows":null,"ms":0.010,"listing":{gen}}}"#
    );
    assert!(list.receive(gen, &answer), "the empty query's answer lands");
    assert_eq!(
        list.facts().filtered.as_ref().map(|f| f.rows.len()),
        None,
        "an empty query draws 0 of 9 shown"
    );
    assert_eq!(
        list.escape(),
        Escape::Cancel,
        "the first esc clears a filter that is not there"
    );
}

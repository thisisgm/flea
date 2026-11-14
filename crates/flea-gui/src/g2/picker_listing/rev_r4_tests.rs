//! Review proofs for rev-g2-pk2-r4; each test asserts the oracle behaviour, so a failure is the finding.
use super::listing::{Escape, Listing};
use super::save::{ProbeKey, SaveField};
use flea::protocol::RowFacts;
use flea::ui_logic_picker_listing::sort::{picker_next_order, PickerOrder};
use flea::ui_logic_picker_listing::SaveCheck;

fn row(name: &str, dir: bool) -> RowFacts {
    RowFacts {
        listing_index: None,
        source: None,
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

// PickerWindow.qml:765 seeds saveName from a valid req.name, so Return on the untouched field probes report.txt.
#[test]
fn rev_r4_a_valid_request_name_seeds_the_field_and_return_probes_it() {
    let mut field = SaveField::new("report.txt");
    field.set_folder("/d");
    assert_eq!(
        field.asked_name(),
        "report.txt",
        "the field draws the request name"
    );
    assert_eq!(
        field.accept(),
        SaveCheck::Probe,
        "the field draws report.txt yet Return asks for a name"
    );
}

// PickerWindow.qml:257 probes [path, saveName]; the seeded name is what the window probes and then saves.
#[test]
fn rev_r4_the_seeded_name_is_probed_and_saved() {
    let mut field = SaveField::new("report.txt");
    field.set_folder("/d");
    assert_eq!(
        field.probe_key(),
        ProbeKey {
            folder: "/d".into(),
            name: "report.txt".into()
        },
        "the window probes no name"
    );
    let key = field.probe_key();
    field.set_exists(&key, false);
    assert_eq!(
        field.accept(),
        SaveCheck::Ready,
        "an absent seeded name is never written"
    );
}

// r1-11 stands with the seed: emptying the seeded field asks for a name.
#[test]
fn rev_r4_emptying_the_seeded_field_asks_for_a_name() {
    let mut field = SaveField::new("report.txt");
    field.edit("");
    assert_eq!(field.name(), "");
    assert_eq!(field.accept(), SaveCheck::NeedName);
}

// Filter.js shown(): an empty query is no filter at once; CHANGES picker-filter says esc cancels once the filter is cleared.
#[test]
fn rev_r4_a_cleared_query_is_no_filter_before_its_answer_lands() {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(list.landed(gen, 9));
    assert!(list.held_rows(gen, 0, vec![row("Docs", true), row("notes.txt", false)]));
    list.filter_line("tes");
    assert!(list.receive(
        gen,
        r#"{"t":"filtered","id":1,"query":"tes","n":9,"rows":[7],"ms":0.010}"#
    ));
    assert!(list.facts().filtered.is_some(), "the tes answer lands");
    list.filter_line("");
    assert_eq!(
        list.escape(),
        Escape::Cancel,
        "the cleared query still makes the first esc clear a filter"
    );
}

// tests/js/picker.js:169 "s from an inherited kind order starts over at name" (sanity, expected to pass).
#[test]
fn rev_r4_s_from_an_inherited_kind_order_starts_over_at_name() {
    let kind = PickerOrder {
        key: "kind".into(),
        desc: true,
    };
    assert_eq!(
        picker_next_order(&kind),
        PickerOrder {
            key: "name".into(),
            desc: false
        }
    );
}

use super::*;

#[test]
fn builtin_ids_come_from_the_key_table_without_arms_or_repeats() {
    let ids = builtin_ids();
    for id in [
        "copy",
        "paste",
        "trash",
        "viewGrid",
        "windowNew",
        "tab1",
        "newFolder",
    ] {
        assert!(ids.contains(&id), "{id} is a key action");
    }
    assert!(!SEQUENCES.is_empty());
    for pair in SEQUENCES {
        assert!(
            !ids.contains(&pair.arm),
            "{} is a pair arm, never an id",
            pair.arm
        );
        assert_eq!(parse(pair.arm), None);
    }
    assert!(!ids.contains(&""));
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted, ids, "sorted and distinct");
    assert!(
        ids.iter().all(|id| !id.contains(':')),
        "a colon marks plugin ids only"
    );
}

#[test]
fn ids_resolve_to_their_kind_and_unknown_words_do_not() {
    assert_eq!(parse("viewGrid"), Some(Id::Builtin("viewGrid")));
    assert_eq!(
        parse("example.checksums:sha256"),
        Some(Id::Plugin {
            plugin: "example.checksums",
            action: "sha256"
        })
    );
    for unknown in [
        "",
        "noSuchAction",
        "copyArm",
        ":sha256",
        "example.checksums:",
        "a:b:c",
        "a b:c",
        "a/b:c",
    ] {
        assert_eq!(parse(unknown), None, "{unknown:?}");
    }
}

#[test]
fn availability_follows_the_key_context() {
    assert!(available_in("viewGrid", Context::Listing));
    assert!(
        !available_in("viewGrid", Context::Menu),
        "a listing key does nothing while a menu holds the keys"
    );
    assert!(
        available_in("copy", Context::Listing),
        "a paired action runs where its key does"
    );
    assert!(available_in("tab1", Context::Listing));
    assert!(!available_in("tab1", Context::Preview));
    assert!(available_in("newFolder", Context::Listing));
    assert!(!available_in("noSuchAction", Context::Listing));
}

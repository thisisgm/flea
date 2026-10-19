// tests/js/permissions.js and permissions-skips.js, behaviour for behaviour.
use super::english;
use super::permissions::*;

fn skip(path: &str, why: &str) -> Skip {
    Skip {
        path: path.into(),
        why: why.into(),
    }
}

fn modes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn shown(summary: &Summary) -> String {
    summary
        .bits
        .iter()
        .map(|b| {
            if b.on {
                "on"
            } else if b.mixed {
                "some"
            } else {
                "off"
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

const SETUID: &str = "Read-only: setuid bit is present.";

#[test]
fn modes_parse_toggle_and_refuse_as_the_card_reads_them() {
    assert_eq!(parse("644"), Some(0o644), "ordinary mode");
    assert_eq!(parse("0644"), Some(0o644), "leading zero");
    assert_eq!(parse("0688"), None, "invalid remains rejected");
    assert_eq!(parse("4755"), None, "special bits unavailable");
    assert_eq!(parse(""), None, "empty is not mode zero");
    assert_eq!(parse("644 "), None, "whitespace not accepted");
    assert_eq!(toggle("0644", 64), "0744", "owner execute toggle");
    assert_eq!(toggle("0744", 64), "0644", "octal and grid one value");
    assert_eq!(toggle("0688", 64), "0688", "invalid text preserved");
    assert_eq!(shown_bits("4644"), Some(0o644));
    assert_eq!(shown_bits(""), None);
}

#[test]
fn several_modes_summarize_into_one_grid() {
    assert!(
        !summarize(&modes(&["0644", "0644"]), &[]).mixed,
        "identical modes show no mixed bit"
    );
    assert!(
        summarize(&modes(&["0644", "0755"]), &[]).bits[2].mixed,
        "a differing owner-execute bit shows mixed"
    );
    assert!(
        summarize(&modes(&["0644", "0755"]), &[]).bits[0].on,
        "a bit set everywhere reads on"
    );
    assert!(
        !summarize(&modes(&["0644", "0644"]), &[]).bits[2].on,
        "a bit set nowhere reads off"
    );
    assert_eq!(
        english().tr("dialogs.file.permissions.mixed"),
        "Mixed boxes keep each item's setting unless you change them."
    );
}

#[test]
fn special_bits_are_named_in_the_backends_own_words() {
    assert_eq!(special_reason("2775"), "Read-only: setgid bit is present.");
    assert_eq!(
        format!("{}|{}", special_reason("4755"), special_reason("1755")),
        "Read-only: setuid bit is present.|Read-only: sticky bit is present."
    );
    assert_eq!(special_reason("0644"), "", "an ordinary mode names nothing");
    assert_eq!(
        special_reason("0688"),
        "",
        "and neither does an unparseable one"
    );
}

#[test]
fn a_skipped_file_neither_sets_nor_mixes_a_bit() {
    let skipping = summarize(&modes(&["0644", "4644"]), &modes(&["", SETUID]));
    assert_eq!(shown(&skipping), "on,on,off,on,off,off,on,off,off");
    assert!(!skipping.mixed);
    assert_eq!(
        shown(&summarize(&modes(&["0644", "4644"]), &[])),
        "on,on,off,on,off,off,on,off,off",
        "a setuid mode alone counts without its reason too"
    );
    assert!(
        !summarize(&modes(&["0644", "2755", "1755"]), &[]).mixed,
        "setgid and sticky are skipped the same way"
    );
    assert!(
        summarize(&modes(&["0644", ""]), &[]).bits[0].on,
        "a refused inspect is skipped too"
    );
    let owner = modes(&["", NOT_OWNER]);
    assert!(
        !summarize(&modes(&["0644", "0755"]), &owner).mixed,
        "a reasoned row neither sets nor mixes a bit"
    );
    assert!(
        !summarize(&modes(&["0644", "0755"]), &owner).bits[2].on,
        "a reasoned row leaves the other file's bits"
    );
    let all = summarize(&modes(&["4644", "4644"]), &modes(&[SETUID, SETUID]));
    assert_eq!(
        format!("{}|{}", shown(&all), all.changeable),
        "on,on,off,on,off,off,on,off,off|false"
    );
    let differ = summarize(&modes(&["4644", "2755"]), &[]);
    assert_eq!(
        format!("{}|{}|{}", shown(&differ), differ.mixed, differ.changeable),
        "on,on,some,on,off,some,on,off,some|true|false"
    );
    let refused = summarize(&modes(&["", ""]), &[]);
    assert!(
        refused.bits.iter().all(|b| !b.on && !b.mixed) && !refused.changeable,
        "a refused inspect shows no bit and changes nothing"
    );
    let one = summarize(&modes(&["0644", "4755"]), &[]);
    assert!(
        one.changeable && !one.bits[2].on,
        "one changeable file keeps the card editable and shows only its bits"
    );
}

#[test]
fn skip_notes_say_one_cause_in_one_sentence() {
    let i18n = english();
    assert_eq!(
        skip_note(&i18n, &[skip("/d/special.txt", SETUID)]),
        "special.txt keeps its mode because its setuid bit is set."
    );
    assert_eq!(
        format!("{}|{}", skip_note(&i18n, &[skip("/d/a", "Read-only: setgid bit is present.")]), skip_note(&i18n, &[skip("/d/b", "Read-only: sticky bit is present.")])),
        "a keeps its mode because its setgid bit is set.|b keeps its mode because its sticky bit is set."
    );
    assert_eq!(
        skip_note(&i18n, &[skip("/d/a.txt", NOT_OWNER)]),
        "a.txt keeps its mode because you do not own it."
    );
    assert_eq!(
        skip_note(&i18n, &[skip("/d/a.txt", "Gone.")]),
        "a.txt keeps its mode: Gone."
    );
    assert_eq!(
        skip_note(&i18n, &[skip("/d/a.txt", "Could not read /mnt/c:d.")]),
        "a.txt keeps its mode: Could not read /mnt/c:d."
    );
    assert_eq!(
        skip_note(
            &i18n,
            &[
                skip("/d/a", SETUID),
                skip("/d/b", "Read-only: sticky bit is present.")
            ]
        ),
        "2 items keep their modes because a special bit is set: a, b."
    );
    assert_eq!(
        skip_note(&i18n, &[skip("/d/a", NOT_OWNER), skip("/d/b", NOT_OWNER)]),
        "2 items keep their modes because you do not own them: a, b."
    );
    assert_eq!(
        skip_note(&i18n, &[skip("/d/a", SETUID), skip("/d/b", "Gone.")]),
        "2 items keep their modes because they cannot be changed: a, b."
    );
    let four: Vec<Skip> = ["a", "b", "c", "d"]
        .iter()
        .map(|n| skip(&format!("/d/{n}.txt"), SETUID))
        .collect();
    assert_eq!(
        skip_note(&i18n, &four),
        "4 items keep their modes because a special bit is set: a.txt, b.txt, c.txt and 1 more."
    );
    let lines = [
        skip_note(&i18n, &[skip("/d/a", SETUID)]),
        skip_note(&i18n, &[skip("/d/a", NOT_OWNER), skip("/d/b", SETUID)]),
        skip_note(&i18n, &[skip("/d/a", "Gone.")]),
        skip_note(&i18n, &[skip("/d/a", "Gone")]),
        multi_result(&i18n, &[skip("/d/special.txt", SETUID)]),
        multi_result(&i18n, &four),
    ];
    for line in lines {
        assert!(
            line.matches(':').count() <= 1
                && !line.contains(". ")
                && line.ends_with('.')
                && !line.ends_with(".."),
            "{line}"
        );
    }
}

#[test]
fn the_line_after_apply_names_what_was_kept_and_never_why() {
    let i18n = english();
    assert_eq!(multi_result(&i18n, &[]), "Permissions changed.");
    assert_eq!(
        multi_result(
            &i18n,
            &[
                skip("/d/secret.txt", "Read-only: setgid bit is present."),
                skip("/d/gone.txt", FAILED)
            ]
        ),
        "2 items kept their modes."
    );
    assert_eq!(
        multi_result(
            &i18n,
            &[skip("/d/secret.txt", "Read-only: setgid bit is present.")]
        ),
        "secret.txt kept its mode."
    );
    let four: Vec<Skip> = (1..=4)
        .map(|n| skip(&format!("/d/{n}.txt"), &format!("r{n}")))
        .collect();
    assert_eq!(multi_result(&i18n, &four), "4 items kept their modes.");
    assert_eq!(
        multi_result(&i18n, &[skip("/d/a.txt", NOT_OWNER)]),
        "a.txt kept its mode."
    );
    assert_eq!(
        multi_result(&i18n, &[skip("/d/a.txt", "Could not read /mnt/c:d.")]),
        "a.txt kept its mode."
    );
    assert_eq!(
        multi_result(
            &i18n,
            &[
                skip("/d/a", SETUID),
                skip("/d/b", SETUID),
                skip("/d/c", SETUID)
            ]
        ),
        "3 items kept their modes."
    );
}

#[test]
fn the_newest_shebang_id_on_the_asked_path_alone_lands() {
    assert_eq!(MAKE_EXEC_ID, 1_000_000, "the offset is named once");
    assert!(!lands_shebang("/d/a.sh", 1, "/d/a.sh", 2));
    assert!(lands_shebang("/d/a.sh", 2, "/d/a.sh", 2));
    assert!(!lands_shebang("/d/b.sh", 2, "/d/a.sh", 2));
}

#[test]
fn a_batch_applies_changed_boxes_and_counts_every_skip_once() {
    let paths = modes(&["/d/a", "/d/b", "/d/c", "/d/d"]);
    let store = Store {
        modes: modes(&["0644", "0755", "4755", ""]),
        reasons: modes(&["", "", "", "Gone."]),
        skipped: Vec::new(),
        pending: 0,
    };
    let out = batch(&store, &paths, 0o100, 0o004);
    assert_eq!(out.paths, modes(&["/d/a", "/d/b"]));
    assert_eq!(
        out.modes,
        modes(&["0740", "0751"]),
        "an untouched box keeps each file's own bit"
    );
    assert_eq!(
        out.skipped,
        vec![skip("/d/c", SETUID), skip("/d/d", "Gone.")]
    );
    let line = batch_request(7, &out);
    assert_eq!(
        line,
        r#"{"c":"permissionsBatch","paths":["/d/a","/d/b"],"modes":["0740","0751"],"id":7}"#
    );
}

#[test]
fn a_box_over_several_files_walks_its_three_states() {
    let summary = summarize(&modes(&["0644", "0755"]), &[]);
    let (mut set, mut clear) = (0, 0);
    let exec = 0o100;
    assert_eq!(multi_value(&summary, set, clear, exec), BoxValue::Some);
    multi_toggle(&summary, &mut set, &mut clear, exec);
    assert_eq!(
        multi_value(&summary, set, clear, exec),
        BoxValue::On,
        "a mixed box sets first"
    );
    multi_toggle(&summary, &mut set, &mut clear, exec);
    assert_eq!(
        multi_value(&summary, set, clear, exec),
        BoxValue::Off,
        "a set mixed box clears next"
    );
    multi_toggle(&summary, &mut set, &mut clear, exec);
    assert_eq!(
        multi_value(&summary, set, clear, exec),
        BoxValue::Some,
        "and lets go back to mixed"
    );
    let uniform = summarize(&modes(&["0644", "0644"]), &[]);
    let (mut set, mut clear) = (0, 0);
    let walk: Vec<BoxValue> = (0..4)
        .map(|_| {
            multi_toggle(&uniform, &mut set, &mut clear, exec);
            multi_value(&uniform, set, clear, exec)
        })
        .collect();
    assert_eq!(
        walk,
        vec![BoxValue::On, BoxValue::Off, BoxValue::On, BoxValue::Off],
        "a uniform-off box reads off, on, off, on"
    );
}

#[test]
fn requests_escape_the_path_they_carry() {
    assert_eq!(
        inspect_request(3, "/d/a\"b"),
        r#"{"c":"permissions","op":"inspect","id":3,"path":"/d/a\"b"}"#
    );
    assert_eq!(
        apply_request(3, "0644"),
        r#"{"c":"permissions","op":"apply","id":3,"mode":"0644"}"#
    );
    assert_eq!(
        close_request(3),
        r#"{"c":"permissions","op":"close","id":3}"#
    );
}

#[test]
fn the_busy_refusal_comes_from_the_catalogue_and_other_text_stays_the_backends() {
    let i18n = english();
    assert_eq!(
        refusal(&i18n, "an operation is already running"),
        "An operation is already running."
    );
    assert_eq!(
        refusal(&i18n, "An operation is already running."),
        "An operation is already running."
    );
    assert_eq!(refusal(&i18n, ""), FAILED);
    assert_eq!(refusal(&i18n, "Permission denied"), "Permission denied.");
}

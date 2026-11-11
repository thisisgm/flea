//! Review proofs for g2-op3 round 1: each test asserts v0.3.12's behaviour, so a failure shows the finding.
use super::keys::Key;
use super::permissions::Permissions;
use super::tests::{buttons, english, notes, open_with, permissions, sent};
use super::Effect;
use flea::jsondoc::parse;

#[test]
fn review_r1_1_escape_while_a_single_apply_runs_keeps_the_card() {
    let mut card = permissions();
    assert_eq!(sent(&card.key(Key::Enter)).len(), 1, "Apply sent");
    assert_eq!(
        card.key(Key::Escape),
        Vec::<Effect>::new(),
        "v0.3.12 close() returns while applyLocked"
    );
}

#[test]
fn review_r1_1_cancel_is_unavailable_while_apply_runs() {
    let i18n = english();
    let mut card = permissions();
    card.key(Key::Enter);
    assert_eq!(
        buttons(&card.card(&i18n)),
        "Cancel~|Apply*@~",
        "v0.3.12 Cancel available: !root.applyLocked"
    );
}

fn several() -> Permissions {
    let paths: Vec<String> = (0..2).map(|n| format!("/fx/p/{n}.jpg")).collect();
    let (mut card, _) = Permissions::open(3, paths, "");
    for at in 0..2 {
        card.reply(
            &english(),
            &parse(&format!(
                r#"{{"op":"inspect","id":{},"ok":true,"mode":"0644","reason":""}}"#,
                3000 + at
            ))
            .unwrap(),
        );
    }
    card
}

#[test]
fn review_r1_1_cancel_and_escape_dismiss_nothing_while_a_batch_runs() {
    let mut card = several();
    assert!(
        sent(&card.key(Key::Enter))[0].starts_with(r#"{"c":"permissionsBatch""#),
        "batch sent"
    );
    card.key(Key::Backtab);
    assert_eq!(
        (card.key(Key::Enter), card.key(Key::Escape)),
        (Vec::new(), Vec::new()),
        "v0.3.12 close() returns while applyingMany"
    );
}

#[test]
fn review_r1_1_a_partly_applied_batch_rereads_the_pane_and_the_grid() {
    let i18n = english();
    let mut card = several();
    card.key(Key::Enter);
    let failed = r#"{"t":"permissions","id":3,"op":"applyMany","ok":false,"mode":"0644","error":"Permission denied. 1 of 2 items were changed; undo restores them."}"#;
    let effects = card.reply(&i18n, &parse(failed).unwrap());
    assert!(
        effects.contains(&Effect::Refresh),
        "v0.3.12 receiveMany failure calls refreshNeeded(): {effects:?}"
    );
    assert_eq!(
        sent(&effects).len(),
        2,
        "v0.3.12 receiveMany failure calls inspectAll(): {effects:?}"
    );
}

#[test]
fn review_r1_2_another_ids_open_with_reply_does_not_close_this_card() {
    let i18n = english();
    let mut card = open_with();
    card.key(Key::Enter);
    let stale = parse(r#"{"t":"menuaction","id":9,"op":"openWith","ok":true}"#).unwrap();
    assert_eq!(
        card.reply(&i18n, &stale),
        Vec::<Effect>::new(),
        "v0.3.12 receive() drops message.id !== requestId"
    );
}

#[test]
fn review_r1_2_another_ids_catalogue_does_not_replace_this_cards_list() {
    let i18n = english();
    let mut card = open_with();
    let other = r#"{"t":"menuaction","id":9,"op":"applications","ok":true,"path":"/fx/v/work/a.png","kind":"PNG image","mime":"image/png","applications":[{"id":"imv.desktop","label":"imv","icon":"imv","default":true}],"installed":[]}"#;
    card.reply(&i18n, &parse(other).unwrap());
    card.key(Key::Tab);
    card.key(Key::Tab);
    card.key(Key::Space);
    card.key(Key::Tab);
    card.key(Key::Tab);
    assert_eq!(
        sent(&card.key(Key::Enter)),
        vec![
            r#"{"c":"menuaction","op":"openWith","id":4,"application":"nvim.desktop","always":true}"#
        ]
    );
}

#[test]
fn review_r1_2_a_cancelled_open_is_not_an_error() {
    let i18n = english();
    let mut card = open_with();
    card.key(Key::Enter);
    card.reply(&i18n, &parse(r#"{"t":"menuaction","id":4,"op":"openWith","ok":false,"error":"Menu request cancelled.","cancelled":true}"#).unwrap());
    let shown = notes(&card.card(&i18n));
    assert!(
        !shown
            .iter()
            .any(|(t, _)| t == "The application could not be opened."),
        "v0.3.12 receive() skips the error when message.cancelled: {shown:?}"
    );
}

#[test]
fn review_r1_3_the_permissions_busy_sentence_comes_from_the_catalogue() {
    let i18n = english();
    let mut card = several();
    card.key(Key::Enter);
    card.reply(&i18n, &parse(r#"{"t":"permissions","id":3,"op":"applyMany","ok":false,"mode":"","error":"an operation is already running"}"#).unwrap());
    let catalogue = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n/en/flea.json"),
    )
    .unwrap();
    let catalogue = parse(&catalogue).unwrap();
    let english: Vec<&str> = catalogue
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(_, v)| v.as_str())
        .collect();
    let (shown, _) = notes(&card.card(&i18n)).into_iter().last().unwrap();
    assert!(
        english.contains(&shown.as_str()),
        "the plan's Strings line names this sentence; shown raw: {shown:?}"
    );
}

#[test]
fn review_fix_props_2_a_refused_snapshot_asks_no_catalogue_and_says_so() {
    let i18n = english();
    let mut card = super::openwith::OpenWith::open(4, "notes.txt");
    let refused = parse(r#"{"t":"menuaction","id":4,"op":"snapshot","ok":false,"error":"The folder changed; reopen the menu."}"#).unwrap();
    assert_eq!(
        card.reply(&i18n, &refused),
        Vec::<Effect>::new(),
        "no catalogue on a snapshot the backend refused"
    );
    let shown = notes(&card.card(&i18n));
    assert!(
        shown
            .iter()
            .any(|(t, _)| t == "The list of applications could not be read."),
        "the card says why its list stays empty: {shown:?}"
    );
}

// S2-8, OpenWithDialog.qml: Space ticks Always from the list, the box or an empty search, types after a query, and presses a focused button.
#[test]
fn space_answers_every_open_with_stop_as_v0312_did() {
    let mut card = open_with();
    assert!(card.key(Key::Space).is_empty());
    assert!(card.always, "an empty search line ticks Always");
    assert_eq!(card.query, "");
    card.key(Key::Text("n".into()));
    card.key(Key::Space);
    assert_eq!(card.query, "n ", "after a query Space types");
    assert!(card.always);
    card.key(Key::Tab);
    card.key(Key::Space);
    assert!(!card.always, "the list toggles Always");
    card.key(Key::Tab);
    card.key(Key::Space);
    assert!(card.always, "the box toggles Always");
    card.key(Key::Tab);
    assert_eq!(
        sent(&card.key(Key::Space)),
        vec![r#"{"c":"menuaction","op":"close","id":4}"#],
        "Space on Cancel closes"
    );
    let mut card = open_with();
    for _ in 0..4 {
        card.key(Key::Tab);
    }
    assert_eq!(
        sent(&card.key(Key::Space)),
        vec![
            r#"{"c":"menuaction","op":"openWith","id":4,"application":"nvim.desktop","always":false}"#
        ],
        "Space on Open launches the cursor application"
    );
}

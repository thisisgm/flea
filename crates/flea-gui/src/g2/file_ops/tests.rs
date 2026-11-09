// The host drives the shared decisions end to end, and the overlay draws every card without a window of its own.
use super::host::Overlay;
use super::model::{Dialog, Operations};
use super::palette::CardPalette;
use flea::i18n::I18n;
use flea::ui_logic_file_ops::{collide, confirm, trash};
use gpui::TestAppContext;
use std::sync::Arc;

pub(super) fn catalog() -> I18n {
    I18n::load(flea::i18n::Paths {
        omarchy: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/i18n/fixtures/omarchy"),
        language_file: "/nonexistent/flea-op1-test-language".into(),
        catalogs: Vec::new(),
    })
}

pub(super) fn trash_source() -> confirm::Source {
    confirm::Source::Trash(
        collide::Request::new()
            .with("all", "false")
            .with("emptyTrash", "false")
            .with("uris", &collide::texts(&["trash:///a"])),
    )
}

pub(super) fn request() -> collide::Request {
    collide::Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("copy"))
        .with("paths", &collide::texts(&["/a/notes.txt"]))
        .with("dest", &collide::text("/home/u/work"))
}

#[test]
fn a_paste_asks_then_keep_both_answers_on_enter() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    assert!(ops.ask(&cat, &request(), &[], false));
    assert_eq!(
        ops.outbox,
        vec![r#"{"c":"collisions","id":1,"dest":"/home/u/work","paths":["/a/notes.txt"]}"#]
    );
    assert!(ops.line(&cat, r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":"text-x-generic"}],"mergeable":false}"#, 0));
    let Some(Dialog::Collide {
        title,
        focus,
        merge,
        ..
    }) = &ops.dialog
    else {
        panic!("no card")
    };
    assert_eq!(title, "notes.txt already exists in work.");
    assert_eq!((*focus, *merge), (collide::Choice::Keep, false));
    assert!(ops.key(&cat, "enter", false));
    assert!(ops.dialog.is_none());
    assert_eq!(
        ops.outbox[1],
        r#"{"c":"transfer","op":"copy","paths":["/a/notes.txt"],"dest":"/home/u/work","collide":"keep","collideId":1}"#
    );
}

#[test]
fn a_second_ask_is_refused_out_loud_and_sends_nothing() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    ops.ask(&cat, &request(), &[], false);
    assert!(!ops.ask(&cat, &request(), &[], false));
    assert_eq!(ops.outbox.len(), 1);
    assert_eq!(
        ops.said.last().map(|s| s.text.as_str()),
        Some("A transfer is already waiting on its question; try again once it is answered.")
    );
}

#[test]
fn escape_cancels_and_a_stale_answer_opens_nothing() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    ops.ask(&cat, &request(), &[], false);
    ops.line(&cat, r#"{"t":"collisions","id":9,"total":1,"names":[{"n":"x","d":false,"i":""}],"mergeable":false}"#, 0);
    assert!(ops.dialog.is_none());
    ops.line(&cat, r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"x","d":false,"i":""}],"mergeable":true}"#, 0);
    assert!(ops.key(&cat, "escape", false));
    assert!(ops.dialog.is_none());
    assert_eq!(ops.outbox.len(), 1);
    assert!(!ops.key(&cat, "j", false));
}

#[test]
fn the_delete_card_opens_on_cancel_and_tab_keeps_the_accent_there() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    ops.confirm_delete(
        trash::Review {
            count: Some(1),
            token: "7".into(),
            ..Default::default()
        },
        trash::Scope::TrashItems,
        trash_source(),
        0,
    );
    ops.key(&cat, "tab", false);
    let Some(Dialog::Delete { focus, .. }) = &ops.dialog else {
        panic!("no card")
    };
    assert_eq!(*focus, trash::Button::Delete);
    assert_eq!(trash::PRIMARY, trash::Button::Cancel);
    ops.key(&cat, "enter", false);
    assert_eq!(
        ops.outbox,
        vec![r#"{"c":"trashbrowse","op":"delete","id":1,"token":7}"#]
    );
    assert!(ops.dialog.is_none());
    ops.line(
        &cat,
        r#"{"t":"trashdeletestarted","id":40,"requestId":1}"#,
        0,
    );
    assert!(ops.emptying.running);
}

#[test]
fn an_unfilled_review_never_enables_delete() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    ops.confirm_delete(
        trash::Review::default(),
        trash::Scope::TrashItems,
        trash_source(),
        0,
    );
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    assert!(ops.outbox.is_empty());
    assert!(ops.dialog.is_some());
}

#[test]
fn jobs_lines_fold_into_blocks_and_the_strip() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    ops.line(&cat, r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying 10 of 18 to NAS","file":"DSC_4944.jpg","done":9,"total":18,"bytes":5300000,"totalBytes":9700000,"itemBytes":458300,"rate":1100000,"left":4,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":1}"#, 0);
    ops.line(&cat, r#"{"t":"job","job":{"id":13,"kind":"copy","head":"Copy 8 items to NAS","file":"","done":0,"total":8,"bytes":0,"totalBytes":0,"itemBytes":0,"rate":0,"left":null,"state":"waiting","reason":"waiting for the copy to NAS","output":[],"undo":false,"cancelled":false},"unfinished":2}"#, 0);
    let ids: Vec<u64> = ops.jobs.blocks().map(|r| r.id).collect();
    assert_eq!(ids, vec![13, 12]);
    let (head, count) = flea::ui_logic_file_ops::jobs::strip(&cat, &ops.jobs).unwrap();
    assert_eq!(
        (head.as_str(), count.as_str()),
        ("Copying 10 of 18 to NAS", " · 2 jobs")
    );
    ops.jobs.toggle_fold();
    assert_eq!(
        flea::ui_logic_file_ops::jobs::strip(&cat, &ops.jobs).unwrap(),
        (head, count)
    );
    ops.job_action(13, flea::ui_logic_file_ops::jobs::Action::Cancel);
    assert_eq!(ops.outbox, vec![r#"{"c":"jobs","op":"cancel","id":13}"#]);
}

#[test]
fn the_ipc_reader_reports_the_open_card() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    assert_eq!(ops.collide_state(), r#"{"opened":false,"state":"idle"}"#);
    ops.ask(&cat, &request(), &[], false);
    ops.line(&cat, r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":""}],"mergeable":true}"#, 0);
    let adapter = super::ipc::OperationsAdapter { model: &ops };
    let reply = crate::ipc::Adapter::call(&adapter, "collideState", &[]).unwrap();
    assert_eq!(reply.stdout, "{\"opened\":true,\"state\":\"open\",\"title\":\"notes.txt already exists in work.\",\"names\":[\"notes.txt\"],\"more\":\"\",\"focus\":\"keep\",\"buttons\":[\"cancel\",\"skip\",\"merge\",\"replace\",\"keep\"]}\n");
    assert!(crate::ipc::Adapter::call(&adapter, "dialogOpen", &[]).is_none());
}

#[gpui::test]
fn the_overlay_draws_every_card_and_the_jobs_block(cx: &mut TestAppContext) {
    let handle = cx.add_window(|window, cx| {
        window.set_rem_size(gpui::px(14.));
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = CardPalette::from_list(
            &crate::list::Palette::inherited(&window.text_style(), theme),
            0.,
        );
        Overlay::new(
            Arc::new(crate::test_catalogs::english()),
            palette,
            "default",
        )
    });
    for open in 0..4 {
        handle
            .update(cx, |o, window, cx| {
                let cat = o.catalog.clone();
                match open {
                    0 => o.model.confirm_delete(trash::Review { count: Some(2), token: "1".into(), ..Default::default() }, trash::Scope::Items, confirm::Source::Menu(3), 0),
                    1 => o.model.copy_to(3, false, "/home/u"),
                    2 => {
                        o.model.dialog = None;
                        o.model.ask(&cat, &request(), &[], false);
                        o.model.line(&cat, r#"{"t":"collisions","id":1,"total":3,"names":[{"n":"a","d":true,"i":""}],"mergeable":true}"#, 0);
                    }
                    _ => {
                        o.model.line(&cat, r#"{"t":"job","job":{"id":4,"kind":"send","head":"Sending 2 of 3 to gdrive","file":"","done":1,"total":3,"bytes":null,"totalBytes":null,"itemBytes":0,"rate":0,"left":null,"state":"failed","reason":"DSC_4930.jpg did not match on b2-photos.","output":[],"undo":false,"cancelled":false},"unfinished":0}"#, 0);
                    }
                }
                cx.notify();
                window.refresh();
            })
            .unwrap();
        cx.run_until_parked();
    }
    handle
        .update(cx, |o, _, _| {
            assert!(matches!(o.model.dialog, Some(Dialog::Collide { .. })));
            assert!(o.model.jobs.visible());
        })
        .unwrap();
}

// FIELDS-01: card error text and marks draw the on-surface role; the bare error role appears only as a frame.
#[test]
fn card_error_ink_is_only_ever_a_frame() {
    // oracle tests/js/cardink.js:30
    // oracle tests/js/cardink.js:31
    // oracle tests/js/cardink.js:32
    // oracle tests/js/cardink.js:33
    // oracle tests/js/cardink.js:34
    // oracle tests/js/cardink.js:35
    // oracle tests/js/cardink.js:40
    for (name, source) in [
        ("view.rs", include_str!("view.rs")),
        ("host.rs", include_str!("host.rs")),
    ] {
        for (at, line) in source.lines().enumerate() {
            let mut rest = line;
            while let Some(found) = rest.find("palette.error") {
                let after = &rest[found + "palette.error".len()..];
                if !after.starts_with("_on_surface") {
                    assert!(
                        rest[..found].contains("border_color("),
                        "{name}:{} draws the bare error role outside a frame",
                        at + 1
                    );
                }
                rest = after;
            }
        }
    }
    let look = flea::ui_logic_file_ops::card::look(flea::ui_logic_file_ops::card::ButtonState {
        destructive: true,
        ..Default::default()
    });
    assert_eq!(
        look.label,
        flea::ui_logic_file_ops::card::Ink::ErrorOnSurface
    );
}

// A repeated job line changes nothing drawn, so the host skips its redraw; a new figure redraws.
#[test]
fn an_unchanged_job_line_asks_for_no_redraw() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    let line = r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying 10 of 18 to NAS","file":"a","done":9,"total":18,"bytes":5,"totalBytes":9,"itemBytes":1,"rate":1,"left":4,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":1}"#;
    assert!(ops.line(&cat, line, 0) && ops.changed);
    assert!(ops.line(&cat, line, 0));
    assert!(!ops.changed);
    assert!(ops.line(&cat, &line.replace(r#""done":9"#, r#""done":10"#), 0) && ops.changed);
}

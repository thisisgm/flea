// Every string the op1 boards draw, built by the functions the cards call, against the board HTML exactly.
use crate::ui_logic_op1::testcat::Catalog;
use crate::ui_logic_op1::{collide, jobs, joined, trash, Collision, JobRow};

fn job(id: u64, kind: &str, head: &str, state: &str) -> JobRow {
    JobRow {
        id,
        kind: kind.into(),
        head: head.into(),
        state: state.into(),
        ..JobRow::default()
    }
}

#[test]
fn the_dialog_boards_strings() {
    let cat = Catalog::english();
    let c = &cat.i18n;
    let notes = [Collision {
        name: "notes.txt".into(),
        dir: false,
        icon: "text-x-generic".into(),
    }];
    // board collide, collide-replace, collide-skip
    assert_eq!(
        collide::title(c, &notes, 1, "/fx/v/work"),
        "notes.txt already exists in work."
    );
    // board collide-merge, collide-merge-inner
    let docs = [Collision {
        name: "Docs".into(),
        dir: true,
        icon: String::new(),
    }];
    assert_eq!(
        collide::title(c, &docs, 1, "/fx/v/Empty"),
        "Docs already exists in Empty."
    );
    assert_eq!(
        collide::title(c, &notes, 1, "/fx/v/Empty/Docs"),
        "notes.txt already exists in Docs."
    );
    assert_eq!(
        collide::explain(c),
        "Replaced items go to Trash, and Undo restores them."
    );
    let labels: Vec<String> = collide::shown(true)
        .iter()
        .map(|b| c.tr(b.label_key()))
        .collect();
    assert_eq!(labels, ["Cancel", "Skip", "Merge", "Replace", "Keep Both"]);
    // Negative: Merge is drawn only on a card that can merge.
    let labels: Vec<String> = collide::shown(false)
        .iter()
        .map(|b| c.tr(b.label_key()))
        .collect();
    assert_eq!(labels, ["Cancel", "Skip", "Replace", "Keep Both"]);
    assert_eq!(c.tr("jobs.esc"), "esc");
    // board delete, delete-focus
    let review = trash::Review {
        count: Some(1),
        token: "1".into(),
        ..Default::default()
    };
    assert_eq!(
        trash::confirm_title(c, &review),
        "Delete 1 item permanently?"
    );
    assert_eq!(
        trash::confirm_body(c, &review, trash::Scope::Items),
        "This item is deleted from disk. This cannot be undone."
    );
    let labels: Vec<String> = trash::BUTTONS
        .iter()
        .map(|b| c.tr(b.label_key(false)))
        .collect();
    assert_eq!(labels, ["Cancel", "Delete"]);
    // board copyto, copyto-empty: the field name is drawn in capitals from this source string.
    assert_eq!(c.tr("jobs.copyto.title"), "Copy To");
    assert_eq!(c.tr("jobs.copyto.field"), "Destination folder");
    assert_eq!(c.tr("jobs.button.copy"), "Copy");
    // board trash-arm
    assert_eq!(
        trash::trash_prompt(c, "default", 1),
        "Press d again to move it to Trash, or press Delete on its own."
    );
}

#[test]
fn the_jobs_boards_strings() {
    let cat = Catalog::english();
    let c = &cat.i18n;
    let mut copy = job(12, "copy", "Copying 10 of 18 to NAS", "running");
    copy.file = "DSC_4944.jpg".into();
    (copy.done, copy.total, copy.item_bytes, copy.rate, copy.left) =
        (9, 18, 458_300, 1_100_000, Some(4));
    (copy.bytes, copy.total_bytes) = (Some(5_300_000), Some(9_700_000));
    let b = jobs::block(c, &copy);
    assert_eq!(
        (b.head.as_str(), b.file.as_str()),
        ("Copying 10 of 18 to NAS", "DSC_4944.jpg · 458.3 kB")
    );
    assert_eq!(joined(&b.line), "5.3 MB of 9.7 MB · 1.1 MB/s · 0:04 left");
    assert_eq!(b.buttons, [jobs::Action::Cancel]);
    let mut waiting = job(13, "copy", "Copy 8 items to NAS", "waiting");
    waiting.reason = "waiting for the copy to NAS".into();
    let b = jobs::block(c, &waiting);
    assert_eq!(
        (b.file.as_str(), b.bar, b.mark_dim),
        ("waiting for the copy to NAS", Some(0.0), true)
    );
    let mut action = job(15, "action", "Resize to 1600 px · 4 of 9", "running");
    (action.file, action.done, action.total) = ("DSC_4901.jpg".into(), 3, 9);
    let b = jobs::block(c, &action);
    assert_eq!((b.file.as_str(), b.line.len()), ("DSC_4901.jpg", 0));
    let mut failed = job(14, "send", "Sending 2 of 3 to gdrive", "failed");
    failed.reason = "DSC_4930.jpg did not match on b2-photos.".into();
    let b = jobs::block(c, &failed);
    assert_eq!(
        (b.head.as_str(), b.file.as_str(), b.bar),
        (
            "Send failed",
            "DSC_4930.jpg did not match on b2-photos.",
            None
        )
    );
    let labels: Vec<String> = b.buttons.iter().map(|a| c.tr(a.label_key())).collect();
    assert_eq!(labels, ["Retry", "Dismiss"]);
    let mut reg = jobs::Jobs::default();
    for row in [copy, action, waiting, failed] {
        reg.update(row);
    }
    assert_eq!(
        jobs::strip(c, &reg),
        Some(("Copying 10 of 18 to NAS".into(), " · 3 jobs".into()))
    );
    // board trash-emptying
    let mut empty = job(16, "emptyTrash", "", "running");
    (
        empty.file,
        empty.item_bytes,
        empty.done,
        empty.total,
        empty.left,
    ) = (
        "node_modules".into(),
        412_600_000,
        41_212,
        104_881,
        Some(27),
    );
    let b = jobs::block(c, &empty);
    assert_eq!(
        (b.head.as_str(), b.file.as_str()),
        ("Emptying Trash", "node_modules · 412.6 MB")
    );
    assert_eq!(joined(&b.line), "41,212 of 104,881 deleted · 0:27 left");
    // Negative: the emptying bar counts files, never bytes.
    assert!((b.bar.unwrap() - 41_212.0 / 104_881.0).abs() < 1e-9);
}

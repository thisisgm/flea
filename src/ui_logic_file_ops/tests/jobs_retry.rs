// A failed block's buttons: Retry only where trying the same request again could pass.
use crate::ui_logic_file_ops::jobs::{self, Action};
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::JobRow;

/// S1B-12: a refusal the same request always meets offers Dismiss alone; a failure that can pass later keeps Retry.
#[test]
fn s1b_12_a_settled_refusal_offers_no_retry() {
    let cat = Catalog::english();
    let failed = |reason: &str| {
        let r = JobRow {
            id: 16,
            kind: "copy".into(),
            state: "failed".into(),
            head: "Copying 1 of 1".into(),
            reason: reason.into(),
            ..JobRow::default()
        };
        jobs::block(&cat.i18n, &r).buttons
    };
    for settled in [
        "cannot move or copy a folder into itself",
        "tree: cannot move or copy a folder into itself",
        "already in that folder",
        "a file of 4 GiB or more does not fit on a FAT32 drive",
        "That name is too long",
        "operation is not supported",
        "':' is not allowed in a name on this vfat drive",
        "an entry in this archive would land outside its folder, so nothing was extracted",
        "'a.txt' and 'A.txt' would take one name on this drive, so nothing was extracted",
        "'set/clip: final.txt' cannot be extracted onto this drive: ':' is not allowed in a name on this vfat drive",
    ] {
        assert_eq!(failed(settled), vec![Action::Dismiss], "{settled}");
    }
    for passing in [
        "operation timed out",
        "the drive is unavailable",
        "permission denied",
        "Some items could not be transferred.",
        "",
    ] {
        assert_eq!(
            failed(passing),
            vec![Action::Retry, Action::Dismiss],
            "{passing}"
        );
    }
}

/// S1B-12: every settled phrase is still a sentence the backend writes, so a reworded refusal goes red here instead of regaining Retry.
#[test]
fn s1b_12_every_settled_phrase_is_one_the_backend_still_writes() {
    let sources = [
        include_str!("../../backend/opsreq.rs"),
        include_str!("../../backend/copyfile.rs"),
        include_str!("../../error.rs"),
        include_str!("../../backend/archiveops.rs"),
    ];
    for phrase in jobs::SETTLED {
        let written = format!("\"{phrase}\"");
        assert!(
            sources.iter().any(|source| source.contains(&written)),
            "{phrase}"
        );
    }
}

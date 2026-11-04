//! Port of the card sentences, the sticky lines and the commit validation.
use super::{cards::*, english};

#[test]
fn leaf_readers_cut_at_the_last_slash() {
    assert_eq!(leaf_of("/d/backup.7z"), "backup.7z");
    assert_eq!(leaf_of("backup.7z"), "backup.7z");
    assert_eq!(path_leaves(&["/d/a", "/d/b"]), vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn card_titles_are_whole_templates() {
    let i18n = english();
    assert_eq!(compress_title(&i18n, 1), "Compress 1 Item");
    assert_eq!(compress_title(&i18n, 3), "Compress 3 Items");
    assert_eq!(encrypted_title(&i18n, "backup.7z"), "backup.7z is encrypted.");
    assert_eq!(wrong_password_sentence(&i18n, "backup.7z"), "That password did not open backup.7z.");
}

#[test]
fn sticky_line_counts_once_per_selection() {
    let i18n = english();
    assert_eq!(compress_sticky(&i18n, 3, "7z"), "Compressing 3 items to .7z");
    assert_eq!(compress_sticky(&i18n, 1, "zip"), "Compressing 1 item to .zip");
}

#[test]
fn empty_and_mismatched_pairs_never_reach_the_wire() {
    assert_eq!(compress_commit(true, true, false), Commit::EmptyPassword);
    assert_eq!(compress_commit(false, true, false), Commit::Mismatch);
    assert_eq!(compress_commit(false, false, false), Commit::Mismatch);
    assert_eq!(compress_commit(true, false, true), Commit::EmptyName);
    assert_eq!(compress_commit(true, false, false), Commit::Accept);
}

#[test]
fn refusals_name_themselves() {
    let i18n = english();
    assert_eq!(compress_refusal(&i18n, &Commit::Mismatch), "Passwords do not match.");
    assert_eq!(compress_refusal(&i18n, &Commit::EmptyPassword), "Type a password first.");
    assert_eq!(compress_refusal(&i18n, &Commit::EmptyName), "");
    assert_eq!(compress_refusal(&i18n, &Commit::Accept), "");
}

#[test]
fn extract_asks_for_its_password() {
    assert_eq!(extract_commit(true), Commit::EmptyPassword);
    assert_eq!(extract_commit(false), Commit::Accept);
}

#[test]
fn extracting_heads_and_files_follow_the_copy_grammar() {
    let i18n = english();
    assert_eq!(extracting_head(&i18n, 7, 18), "Extracting 7 of 18");
    assert_eq!(extracting_file(&i18n, "DSC_4907.jpg", 586600), "DSC_4907.jpg · 586.6 kB");
}

#[test]
fn extracting_block_counts_from_the_index() {
    let i18n = english();
    let block = extracting_block(
        &i18n,
        &super::wire::ExtractSample { id: 9, done: 6, total: 18, file: "DSC_4907.jpg".into(), item_bytes: 586600, bytes: 3700000, total_bytes: 9700000, rate: 2400000, left: Some(2) },
    );
    assert_eq!(block.head, "Extracting 7 of 18");
    assert_eq!(block.file, "DSC_4907.jpg · 586.6 kB");
}

#[test]
fn extracting_block_leads_with_the_unpacked_bytes() {
    let i18n = english();
    let block = extracting_block(
        &i18n,
        &super::wire::ExtractSample { id: 9, done: 6, total: 18, file: "DSC_4907.jpg".into(), item_bytes: 586600, bytes: 3700000, total_bytes: 9700000, rate: 2400000, left: Some(2) },
    );
    let text: String = block.byte.iter().map(|part| part.text.clone()).collect();
    assert_eq!(text, "3.7 MB of 9.7 MB · 2.4 MB/s · 0:02 left");
    assert!(block.byte.first().is_some_and(|part| part.figure), "the unpacked bytes lead as a figure");
    assert!((block.share - 3700000.0 / 9700000.0).abs() < 1e-9);
}

fn sample(done: usize, total: usize, bytes: u64, total_bytes: u64, rate: u64, left: Option<u64>) -> super::wire::ExtractSample {
    super::wire::ExtractSample {
        id: 9,
        done,
        total,
        file: "DSC_4907.jpg".into(),
        item_bytes: 586600,
        bytes,
        total_bytes,
        rate,
        left,
    }
}

#[test]
fn extracting_head_names_no_count_until_the_index_is_known() {
    let i18n = english();
    assert_eq!(extracting_head(&i18n, 1, 0), "Extracting", "an unknown index names no count");
    assert_eq!(extracting_head(&i18n, 12, 0), "Extracting 11 items", "the backend's own unknown-total head");
    assert_eq!(extracting_block(&i18n, &sample(11, 0, 0, 0, 0, None)).head, "Extracting 11 items");
}

#[test]
fn extracting_unknown_index_head_counts_one_item_in_the_singular() {
    let i18n = english();
    assert_eq!(extracting_head(&i18n, 2, 0), "Extracting 1 item", "one landed entry reads in the singular");
    assert_eq!(extracting_head(&i18n, 12, 0), "Extracting 11 items", "eleven landed entries read in the plural");
}

#[test]
fn extracting_head_clamps_the_final_sample_to_the_total() {
    let i18n = english();
    assert_eq!(extracting_head(&i18n, 19, 18), "Extracting 18 of 18", "the final JobEntries sets done to the index total");
    assert_eq!(extracting_block(&i18n, &sample(18, 18, 0, 0, 0, None)).head, "Extracting 18 of 18");
}

#[test]
fn extracting_head_survives_a_huge_backend_count() {
    let i18n = english();
    assert_eq!(extracting_block(&i18n, &sample(usize::MAX, 18, 0, 0, 0, None)).head, "Extracting 18 of 18");
    assert!(extracting_block(&i18n, &sample(usize::MAX, usize::MAX, 0, 0, 0, None)).head.starts_with("Extracting"));
}

#[test]
fn extracting_byte_line_names_no_total_while_the_index_is_unknown() {
    let i18n = english();
    let landed: String = extracting_block(&i18n, &sample(6, 0, 3700000, 0, 2400000, None))
        .byte
        .iter()
        .map(|part| part.text.clone())
        .collect();
    assert_eq!(landed, "3.7 MB · 2.4 MB/s", "no zero total is named once bytes land");
    let empty: String = extracting_block(&i18n, &sample(0, 0, 0, 0, 0, None))
        .byte
        .iter()
        .map(|part| part.text.clone())
        .collect();
    assert_eq!(empty, "0 B · 0 B/s", "an empty unknown index names no total");
}

#[test]
fn extracting_cancel_stays_live_while_entries_land() {
    let i18n = english();
    let block = extracting_block(&i18n, &sample(0, 0, 0, 0, 0, None));
    assert!(block.cancel, "Cancel stays live while entries land");
    let text: String = block.byte.iter().map(|part| part.text.clone()).collect();
    assert_eq!(text, "0 B · 0 B/s", "no estimate without a total");
}

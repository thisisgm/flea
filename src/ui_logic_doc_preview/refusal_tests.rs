use super::*;

// The fixed projected sentences, extracted from the frozen v0.3.12 files named on each class.
#[test]
fn refusal_classes_project_fixed_sentences() {
    let i18n = crate::i18n::test_catalogs::english();
    let status = |r: Refusal| r.status_key().map(|key| i18n.tr(key));
    let frame = |r: Refusal| r.frame_key().map(|key| i18n.tr(key));
    assert_eq!(
        status(Refusal::TooLarge).as_deref(),
        Some("This file is too large to preview."),
        "CodeRefusal.qml too large"
    );
    assert_eq!(
        frame(Refusal::TooLarge).as_deref(),
        Some("too large to preview"),
        "the too-large frame's caption"
    );
    assert_eq!(
        status(Refusal::Binary),
        None,
        "a binary file reports ready, its frame is the whole answer"
    );
    assert_eq!(
        frame(Refusal::Binary).as_deref(),
        Some("binary file"),
        "the binary frame's caption"
    );
    assert_eq!(
        status(Refusal::Unreadable).as_deref(),
        Some("This file could not be read."),
        "a failed read"
    );
    assert_eq!(
        frame(Refusal::Unreadable),
        None,
        "a failed read draws its sentence alone"
    );
    assert_eq!(
        status(Refusal::FontUnsupported).as_deref(),
        Some("This file cannot be previewed."),
        "FontPane.qml refusal"
    );
    assert_eq!(
        status(Refusal::ArchiveUnreadable).as_deref(),
        Some("This archive could not be read."),
        "Preview.qml archive"
    );
    assert_eq!(
        Refusal::Unreadable.sentence_key(),
        Some("document.unreadable"),
        "the sentence drawn alone"
    );
    assert_eq!(
        Refusal::TooLarge.sentence_key(),
        None,
        "a framed class draws no lone sentence"
    );
}

#[test]
fn refusal_reads_the_reply_state_never_its_text() {
    assert_eq!(from_code_state("binary"), Some(Refusal::Binary));
    assert_eq!(from_code_state("large"), Some(Refusal::TooLarge));
    assert_eq!(from_code_state("failed"), Some(Refusal::Unreadable));
    assert_eq!(
        from_code_state("styled"),
        None,
        "a coloured answer is no refusal"
    );
    assert_eq!(
        from_code_state("plain"),
        None,
        "a plain answer is no refusal"
    );
    assert_eq!(
        from_code_state(""),
        None,
        "an answer still out is no refusal"
    );
}

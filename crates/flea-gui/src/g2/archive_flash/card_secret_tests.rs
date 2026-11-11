//! Compress secret visibility, destination and focus checks.
use super::*;

#[test]
fn holding_the_eye_reveals_each_compress_secret_field() {
    let i18n = english();
    let mut card = compress();
    typed(&mut card, &i18n, "s3cret");
    card.eye_press(CompressStop::Password);
    let drawn = card.card(&i18n);
    let Block::Field(password) = &drawn.body[2] else {
        panic!("password field")
    };
    assert!(password.revealed, "the password eye is held");
    assert_eq!(
        password.drawn(),
        "s3cret",
        "holding the eye shows the password"
    );
    let Block::Field(confirm) = &drawn.body[3] else {
        panic!("confirm field")
    };
    assert!(!confirm.revealed, "the other eye is not held with it");
    assert_eq!(
        confirm.drawn(),
        "",
        "the confirm field stays empty and masked"
    );
    card.eye_release(CompressStop::Password);
    let drawn = card.card(&i18n);
    let Block::Field(password) = &drawn.body[2] else {
        panic!("password field")
    };
    assert_eq!(
        password.drawn(),
        "••••••",
        "releasing the eye masks it again"
    );
    card.eye_press(CompressStop::Confirm);
    let drawn = card.card(&i18n);
    let Block::Field(confirm) = &drawn.body[3] else {
        panic!("confirm field")
    };
    assert!(confirm.revealed, "the confirm field has its own held eye");
}

#[test]
fn revealed_secret_draws_its_value() {
    use super::super::cards::Field;
    let field = Field {
        input: None,
        label: None,
        value: "s3cret".into(),
        placeholder: String::new(),
        secret: true,
        held: 0,
        revealed: true,
        eye: true,
        error: false,
        focused: true,
    };
    assert_eq!(field.drawn(), "s3cret", "holding the eye shows the text");
}

#[test]
fn compress_dest_joins_the_pane_base() {
    let i18n = english();
    let mut card = Compress::open(vec!["/base/sub/a.txt".into()], "/base", "work", 0, &["7z"]).0;
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "s3cret");
    let effects = card.press(&i18n, CompressStop::Compress);
    let lines = sent(&effects);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains(r#""dest":"/base/a.7z""#),
        "the archive lands in the pane base: {}",
        lines[0]
    );
}

#[test]
fn compress_dest_at_the_root_never_doubles_the_slash() {
    let i18n = english();
    let mut card = Compress::open(vec!["/a.txt".into()], "/", "", 0, &["7z"]).0;
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "s3cret");
    let effects = card.press(&i18n, CompressStop::Compress);
    let lines = sent(&effects);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains(r#""dest":"/a.7z""#),
        "the root joins as the oracle does: {}",
        lines[0]
    );
}

#[test]
fn secret_field_debug_never_prints_the_value() {
    use super::super::cards::Field;
    let field = Field {
        input: None,
        label: None,
        value: "s3cret".into(),
        placeholder: String::new(),
        secret: true,
        held: 0,
        revealed: true,
        eye: true,
        error: false,
        focused: true,
    };
    let printed = format!("{field:?}");
    assert!(
        !printed.contains("s3cret") && printed.contains("<6 chars>"),
        "a secret prints its length: {printed}"
    );
    let plain = Field {
        secret: false,
        ..field
    };
    assert!(
        format!("{plain:?}").contains("s3cret"),
        "a plain field prints its value"
    );
}

#[test]
fn focused_chip_frames_in_the_accent() {
    use super::super::draw_controls::chip_frame;
    let theme = crate::palette::Theme::from_text(
        "background='#1a1b26'\nforeground='#a9b1d6'\naccent='#7aa2f7'",
    );
    let t = super::super::cards::Tokens {
        surface: theme.surface,
        ground: theme.surface,
        fg: theme.foreground,
        muted: theme.muted,
        muted_frame: theme.muted_frame,
        accent: theme.accent,
        accent_frame: theme.accent_frame,
        error: theme.error,
        rule: theme.metadata_frame,
    };
    assert_ne!(
        t.accent_frame, t.fg,
        "focused and resting chips must be distinguishable"
    );
    assert_ne!(
        t.accent_frame, t.muted_frame,
        "chosen and unchosen chips must be distinguishable"
    );
    assert_eq!(
        chip_frame(&t, true, true),
        t.accent_frame,
        "the chosen chip frames in accent while focused"
    );
    assert_eq!(
        chip_frame(&t, true, false),
        t.fg,
        "the chosen chip frames in foreground at rest"
    );
    assert_eq!(
        chip_frame(&t, false, true),
        t.muted_frame,
        "an unchosen chip stays muted while focused"
    );
}

#[test]
fn compress_zip_tab_from_confirm_skips_the_hidden_switch() {
    let i18n = english();
    let mut card = compress();
    card.key(&i18n, Key::Backtab);
    card.key(&i18n, Key::Backtab);
    card.key(&i18n, Key::Right);
    for _ in 0..3 {
        card.key(&i18n, Key::Tab);
    }
    assert!(
        fields(&card.card(&i18n))[2].2,
        "the walk reaches Confirm on zip"
    );
    card.key(&i18n, Key::Tab);
    let drawn = card.card(&i18n);
    assert!(
        !drawn
            .body
            .iter()
            .any(|block| matches!(block, Block::CheckLine(_))),
        "zip draws no Encrypt switch"
    );
    assert!(
        buttons(&drawn).starts_with("Cancel@"),
        "Tab from Confirm lands on Cancel: {}",
        buttons(&drawn)
    );
}

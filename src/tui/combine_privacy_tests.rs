//! Decoder staging stays private and locale changes invalidate native preview work.
use super::*;
use std::os::unix::fs::PermissionsExt;

pub(super) fn run(root: &Path) {
    fixture(root);
    safety::install_slow_sandbox(root);
    let (mut m, mut w) = open(root);
    let omarchy = root.join("locale");
    std::fs::create_dir_all(omarchy.join("default/i18n")).unwrap();
    let roster = omarchy.join("default/i18n/locales.json");
    std::fs::write(&roster, roster_text("0")).unwrap();
    let language = root.join("language");
    std::fs::write(&language, "en\n").unwrap();
    m.i18n = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy,
        language_file: language.clone(),
        catalogs: vec![root.join("i18n")],
    });
    m.capabilities.graphics = Protocol::Kitty;
    let mut graphics = Graphics::new();
    graphics.protocol = Protocol::Kitty;
    let mut shown = super::super::stage::Shown::default();
    let mut frame = String::new();
    std::fs::write(root.join("slow"), b"owned locale epoch hold").unwrap();
    stage(&mut m, &mut graphics, &mut shown, &mut w, &mut frame);
    let pid = safety::marker(root);
    let directories: Vec<_> = std::fs::read_dir(root.join("tmp"))
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".flea-work-tui-combine-preview-")
        })
        .collect();
    assert_eq!(directories.len(), 1);
    assert_eq!(
        directories[0].metadata().unwrap().permissions().mode() & 0o777,
        0o700,
        "selected image bytes must not inherit umask022 permissions in a shared temporary folder"
    );
    std::fs::write(&language, "ar\n").unwrap();
    assert!(m.i18n.poll_changed());
    assert_eq!(m.i18n.language(), "ar");
    stage(&mut m, &mut graphics, &mut shown, &mut w, &mut frame);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::rename(root.join("slow"), root.join("locale-slow-used")).unwrap();
    let arabic = native(&mut m, &mut graphics, &mut shown, &mut w, &mut frame);
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "locale epoch must reap previous decoder"
    );
    let arabic = number_pixels(root, &m, "arabic.png", arabic);
    std::fs::write(&language, "en\n").unwrap();
    assert!(m.i18n.poll_changed());
    stage(&mut m, &mut graphics, &mut shown, &mut w, &mut frame);
    let latin = native(&mut m, &mut graphics, &mut shown, &mut w, &mut frame);
    let latin = number_pixels(root, &m, "latin.png", latin);
    assert_ne!(
        arabic, latin,
        "native numeral pixels must follow selected locale"
    );
    std::fs::write(&roster, roster_text("٠")).unwrap();
    assert!(m.i18n.poll_changed());
    assert_eq!(
        m.i18n.language(),
        "en",
        "this control changes locale data without changing its code"
    );
    stage(&mut m, &mut graphics, &mut shown, &mut w, &mut frame);
    let revised = native(&mut m, &mut graphics, &mut shown, &mut w, &mut frame);
    let revised = number_pixels(root, &m, "revised.png", revised);
    assert_eq!(
        arabic, revised,
        "same-language locale revision must redraw actual numeral pixels"
    );
    press(
        &mut m,
        &mut w,
        super::super::input::Key::named("Escape", ""),
    );
    assert!(m.combining.card.is_none());
}
fn roster_text(zero: &str) -> String {
    format!(
        r#"{{"en":{{"name":"English","formatLocale":"en-US","locale":"C","zeroDigit":"{zero}"}},"ar":{{"name":"Arabic","formatLocale":"ar","locale":"C","zeroDigit":"٠","direction":"rtl"}}}}"#
    )
}
fn stage(
    m: &mut Model,
    graphics: &mut Graphics,
    shown: &mut super::super::stage::Shown,
    w: &mut Wire,
    frame: &mut String,
) {
    super::super::stage::step(m, graphics, shown, w, (130, 31), true, frame).unwrap();
}
fn native(
    m: &mut Model,
    graphics: &mut Graphics,
    shown: &mut super::super::stage::Shown,
    w: &mut Wire,
    frame: &mut String,
) -> Vec<u8> {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        stage(m, graphics, shown, w, frame);
        assert!(m.combining.card.as_ref().unwrap().error.is_empty());
        if !graphics.transmit.is_empty() {
            return graphics_tests::kitty_png(&std::mem::take(&mut graphics.transmit));
        }
        assert!(
            Instant::now() < end,
            "live locale preview did not reach native graphics"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn number_pixels(root: &Path, m: &Model, name: &str, png: Vec<u8>) -> Vec<u8> {
    let output = root.join(name);
    std::fs::write(&output, png).unwrap();
    let g = combine::geometry(m);
    let tile = m
        .combining
        .card
        .as_ref()
        .unwrap()
        .composition()
        .unwrap()
        .fitted(
            ((g.preview.width - 2) * 8) as f64,
            ((g.preview.height - 2) * 16) as f64,
            0.,
        )
        .tiles[0];
    tool(&[
        "magick",
        output.to_str().unwrap(),
        "-crop",
        &format!("13x15+{}+{}", tile.x as usize, tile.y as usize),
        "+repage",
        "-depth",
        "8",
        "rgb:-",
    ])
}

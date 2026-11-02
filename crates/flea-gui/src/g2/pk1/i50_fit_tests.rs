//! I50-25: the Japanese save chooser keeps New Folder…, Cancel and Save whole on one row, at real shaped metrics.
use super::tests::fixture::fixture;
use super::view::{BUTTON_HEIGHT, BUTTON_SIDE};
use crate::window::locale::Snapshot;
use flea::i18n::{I18n, Paths};
use gpui::{px, size, Bounds, Pixels, SharedString, TestAppContext, TestDispatcher};
use std::{borrow::Cow, path::Path, sync::Arc};

fn japanese() -> Snapshot {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut catalog = I18n::load(Paths {
        omarchy: repo.join("src/ui_logic_op3/fixtures"),
        language_file: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/g2/pk1/i50_fit_fixtures/language-ja"),
        catalogs: vec![repo.join("i18n")],
    });
    assert_eq!(catalog.language(), "ja");
    catalog.mark_paragraphs();
    Snapshot {
        revision: 2,
        catalog: Arc::new(catalog),
    }
}

/// picker-save in Japanese: every answer-row button draws its whole shaped label on the one 30 px row.
#[test]
fn i50_fit_cjk_picker() {
    let text = gpui_platform::current_platform(true).text_system();
    let mut app = TestAppContext::build_with_text_system(TestDispatcher::new(0), None, text);
    app.update(|cx| {
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
            )])
            .unwrap();
    });
    let mut f = fixture(
        &mut app,
        r#"{"mode":"save","folder":"/d","name":"報告書.txt"}"#,
        None,
    );
    f.cx.simulate_resize(size(px(1100.), px(700.)));
    f.update(|picker, cx| picker.set_locale(japanese(), cx));
    f.cx.run_until_parked();
    f.cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let (cancel, answer) = f.read(|p| (p.labels.cancel.clone(), p.answer_label()));
    let folder: SharedString = f.read(|p| p.locale.catalog.tr("picker.browser.new_folder").into());
    assert_eq!(folder.as_ref(), "新しいフォルダー…");
    let band: Bounds<Pixels> =
        f.cx.debug_bounds("picker-answers-band")
            .expect("the answer row is drawn");
    let mut top = None;
    for (selector, label) in [
        ("picker-new-folder", folder),
        ("picker-cancel", cancel),
        ("picker-answer", answer),
    ] {
        let button =
            f.cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} is drawn"));
        let whole = f.cx.update(|window, cx| {
            let face = crate::list::Typeface::for_window(window, cx);
            crate::g2::fb1::caption::measure(label.clone(), face.font, face.body, px(0.), window)
        });
        assert_eq!(
            f32::from(button.size.height),
            BUTTON_HEIGHT,
            "{selector} {label:?} wraps"
        );
        assert!(
            f32::from(button.size.width) + 0.5 >= whole + 2. * (BUTTON_SIDE + 1.),
            "{selector} {label:?} is cut: {button:?} for a {whole} px label"
        );
        assert!(
            button.left() >= band.left() && button.right() <= band.right(),
            "{selector} {label:?} leaves the row {band:?}: {button:?}"
        );
        assert_eq!(
            *top.get_or_insert(button.top()),
            button.top(),
            "{selector} {label:?} leaves the row"
        );
    }
}

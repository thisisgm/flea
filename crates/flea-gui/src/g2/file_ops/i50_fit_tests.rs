//! I50-25: a translated card keeps its buttons on one row, grows no wider than the window less 2 x 48, and keeps tall scripts whole, at real shaped metrics.
use super::{
    fix10_tests::{init, real_app, redraw},
    geometry::Metrics,
    host::Overlay,
    palette::CardPalette,
    tests::request,
};
use crate::{g2::status_feedback::caption::measure, list::Typeface, palette::Theme};
use flea::i18n::{I18n, Paths};
use flea::ui_logic_file_ops::card::{self, Kind};
use gpui::{px, size, Bounds, Pixels, TestAppContext, VisualTestContext};
use std::{path::Path, sync::Arc};

// LANGUAGES-SPEC Length: a grown card stops at the window less 2 x 48.
const SPEC_MARGIN: f32 = 48.;

// A drawn button: its label, its painted bounds, and the label's whole shaped width.
type Button = (String, Bounds<Pixels>, f32);

// The repository's own catalog for a language, with Omarchy's German Abbrechen from the op3 fixture's shell catalog.
fn catalog(code: &str) -> Arc<I18n> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut catalog = I18n::load(Paths {
        omarchy: repo.join("src/ui_logic_open_with_properties/fixtures"),
        language_file: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/g2/file_ops/i50_fit_fixtures")
            .join(format!("language-{code}")),
        catalogs: vec![repo.join("i18n")],
    });
    assert_eq!(catalog.language(), code, "the fixture names {code}");
    catalog.mark_paragraphs();
    Arc::new(catalog)
}

fn open<'a>(
    app: &'a mut TestAppContext,
    code: &str,
    width: f32,
    mirrored: bool,
    show: impl FnOnce(&mut Overlay, &mut gpui::Context<Overlay>),
) -> (gpui::Entity<Overlay>, &'a mut VisualTestContext) {
    init(app);
    let catalog = catalog(code);
    let (view, cx) = app.add_window_view(|window, _| {
        window.set_rem_size(px(14.));
        window.set_layout_mirrored(mirrored);
        let palette = CardPalette::from_theme(&window.text_style(), Theme::default(), 8.);
        Overlay::new(catalog, palette, "default")
    });
    cx.simulate_resize(size(px(width), px(700.)));
    view.update(cx, show);
    cx.run_until_parked();
    redraw(cx);
    (view, cx)
}

fn collide(o: &mut Overlay, _: &mut gpui::Context<Overlay>) {
    let cat = o.catalog.clone();
    o.model.ask(&cat, &request(), &[], false);
    o.model.line(&cat, r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":"text-x-generic"}],"mergeable":false}"#, 0);
}

fn bounds(cx: &mut VisualTestContext, selector: String) -> Option<Bounds<Pixels>> {
    let selector: &'static str = Box::leak(selector.into_boxed_str());
    cx.debug_bounds(selector)
}

// The card, its buttons in row order, and each button's label shaped whole in the card's face at its body size.
fn drawn(
    view: &gpui::Entity<Overlay>,
    cx: &mut VisualTestContext,
) -> (Bounds<Pixels>, Vec<Button>) {
    let card = bounds(cx, "op1-card".into()).expect("the card is drawn");
    let labels: Vec<String> = view.read_with(cx, |o, _| {
        let Some(super::model::Dialog::Collide { merge, .. }) = &o.model.dialog else {
            panic!("the collision card is open")
        };
        flea::ui_logic_file_ops::collide::shown(*merge)
            .into_iter()
            .map(|b| o.catalog.tr(b.label_key()))
            .collect()
    });
    let buttons = labels
        .into_iter()
        .enumerate()
        .map(|(at, label)| {
            let button = bounds(cx, format!("op1-button-{at}")).expect("every button is drawn");
            let whole = cx.update(|window, cx| {
                let face = cx.global::<Typeface>().clone();
                measure(label.clone().into(), face.font, face.body, px(0.), window)
            });
            (label, button, whole)
        })
        .collect();
    (card, buttons)
}

fn metrics(cx: &mut VisualTestContext) -> Metrics {
    cx.update(|_, cx| Metrics::new(cx.global::<Typeface>()))
}

// One row, every label whole inside its button, every button inside the card's padding.
fn assert_one_row(at: &str, m: &Metrics, card: Bounds<Pixels>, buttons: &[Button]) {
    let inset = card::BORDER + m.padding;
    for (label, button, whole) in buttons {
        assert_eq!(
            button.top(),
            buttons[0].1.top(),
            "{at}: {label:?} leaves the row"
        );
        assert_eq!(
            f32::from(button.size.height),
            m.button,
            "{at}: {label:?} wraps"
        );
        assert!(
            f32::from(button.size.width) + 0.5 >= whole + 2. * (m.button_pad + card::BORDER),
            "{at}: {label:?} is cut: {button:?} for a {whole} px label"
        );
        assert!(
            f32::from(button.left() - card.left()) + 0.5 >= inset
                && f32::from(card.right() - button.right()) + 0.5 >= inset,
            "{at}: {label:?} runs past the card: {button:?} in {card:?}"
        );
    }
}

/// lang-de-dialog: Abbrechen, Überspringen, Ersetzen and Beide behalten stay on one row; the card grows from 448 to 503.
#[test]
fn i50_fit_dialog_buttons_one_row() {
    let mut app = real_app();
    let (view, cx) = open(&mut app, "de", 1100., false, collide);
    let (card, buttons) = drawn(&view, cx);
    let words: Vec<&str> = buttons.iter().map(|(l, ..)| l.as_str()).collect();
    assert_eq!(
        words,
        ["Abbrechen", "Überspringen", "Ersetzen", "Beide behalten"]
    );
    let m = metrics(cx);
    assert_one_row("de at 1100", &m, card, &buttons);
    let row: f32 = buttons
        .iter()
        .map(|(_, b, _)| f32::from(b.size.width))
        .sum::<f32>()
        + 3. * m.button_gap;
    let width = f32::from(card.size.width);
    assert!(
        (width - (row + 2. * (m.padding + card::BORDER))).abs() <= 1.,
        "the card is exactly as wide as its row needs: {width} for a {row} px row"
    );
    assert!(
        (width - 503.).abs() <= 1.,
        "lang-de-dialog draws the card 503 wide, not {width}"
    );
}

/// LANGUAGES-SPEC Length: a card grows only as far as the window less 2 x 48, never below its board width, and an English card keeps 448.
#[test]
fn i50_fit_card_window_margin() {
    let mut app = real_app();
    let (view, cx) = open(&mut app, "de", 1100., false, |_, _| {});
    view.update(cx, |o, cx| {
        o.catalog = Arc::new(super::tests::catalog());
        collide(o, cx);
    });
    redraw(cx);
    let english = bounds(cx, "op1-card".into()).expect("the English card is drawn");
    assert_eq!(
        f32::from(english.size.width),
        448.,
        "English fits the board width"
    );
    for window in [1100., 700., 600., 560., 400.] {
        let mut app = real_app();
        let (view, cx) = open(&mut app, "de", window, false, collide);
        let (card, buttons) = drawn(&view, cx);
        let m = metrics(cx);
        let board = m.width(Kind::Collide, window);
        let widest = (window - 2. * SPEC_MARGIN).max(board);
        let width = f32::from(card.size.width);
        assert!(
            width + 0.5 >= board && width <= widest + 0.5,
            "at {window}: {width} leaves {board}..{widest}"
        );
        if width > board + 0.5 {
            assert!(
                f32::from(card.left()) + 0.5 >= SPEC_MARGIN
                    && window - f32::from(card.right()) + 0.5 >= SPEC_MARGIN,
                "at {window}: a grown card keeps 48 px of window on each side: {card:?}"
            );
        }
        let need = buttons
            .iter()
            .map(|(_, _, whole)| whole + 2. * (m.button_pad + card::BORDER))
            .sum::<f32>()
            + 3. * m.button_gap
            + 2. * (m.padding + card::BORDER);
        if need <= widest {
            assert_one_row(&format!("de at {window}"), &m, card, &buttons);
        } else {
            // Past the approved limit nothing is shrunk, wrapped or elided; the overflow is a controller question, not a new rule.
            assert_eq!(width, widest, "at {window}: the card stops at its limit");
            for (label, button, whole) in &buttons {
                assert!(
                    f32::from(button.size.width) + 0.5
                        >= whole + 2. * (m.button_pad + card::BORDER),
                    "at {window}: {label:?} keeps its whole label"
                );
            }
        }
    }
}

/// lang-ar-list's mirrored window: Arabic collision buttons stay on one row inside the card.
#[test]
fn i50_fit_rtl_card_bounds() {
    let mut app = real_app();
    let (view, cx) = open(&mut app, "ar", 1100., true, collide);
    assert!(view.read_with(cx, |o, _| o.catalog.locale().right_to_left));
    assert!(cx.update(|window, _| window.layout_mirrored()));
    let (card, buttons) = drawn(&view, cx);
    let m = metrics(cx);
    assert_one_row("ar mirrored", &m, card, &buttons);
    let window = 1100.;
    assert!(
        f32::from(card.left()) >= 0. && f32::from(card.right()) <= window,
        "the card stays inside the window: {card:?}"
    );
}

/// Thai and Tamil keep whole ascenders and descenders: each button label's shaped line, and the Copy To field's text, inside their boxes.
#[test]
fn i50_fit_tall_script_field() {
    for (code, path) in [
        ("th", "/home/u/เก็บทั้งคู่ฎฐ"),
        ("ta", "/home/u/இரண்டையும் வைத்திரு"),
    ] {
        let mut app = real_app();
        let (view, cx) = open(&mut app, code, 1100., false, collide);
        let samples = redraw(cx);
        let (_, buttons) = drawn(&view, cx);
        for (label, button, _) in &buttons {
            let sample = samples
                .iter()
                .find(|s| &s.text == label)
                .unwrap_or_else(|| panic!("{code}: {label:?} paints"));
            assert!(
                sample.baseline - sample.ascent >= button.top()
                    && sample.baseline + sample.descent <= button.bottom(),
                "{code}: {label:?} clips: ascent {:?} descent {:?} at {:?} in {button:?}",
                sample.ascent,
                sample.descent,
                sample.baseline
            );
        }
        let mut app = real_app();
        let (view, cx) = open(&mut app, code, 1100., false, |o, cx| {
            o.copy_to(71, false, path, cx)
        });
        let field = bounds(cx, "op1-field".into()).expect("the Copy To field is drawn");
        view.read_with(cx, |o, _| {
            let drawn = o.editor.drawn.borrow();
            let paint = drawn.paint.clone().expect("the field paints its text");
            let line = drawn.line.run.as_ref().expect("the text is shaped").line();
            assert_eq!(paint.text, path, "{code}: the field keeps every byte");
            assert!(
                paint.mask.size.height + px(0.5) >= line.ascent + line.descent,
                "{code}: the field's text box {:?} cuts a {:?} + {:?} line",
                paint.mask,
                line.ascent,
                line.descent
            );
            assert!(
                paint.mask.top() >= field.top() && paint.mask.bottom() <= field.bottom(),
                "{code}: the text box {:?} stays inside the field {field:?}",
                paint.mask
            );
        });
    }
}

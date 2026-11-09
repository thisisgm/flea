//! The FLASH card's draw tests: cards, flash states and the step frame's roles.
use super::cards::{Card, DialogShell};
use super::compress::Compress;
use super::draw::StandIn;
use super::extract::Extract;
use super::flash::{Flash, View};
use super::tests::english;
use crate::list::{Palette, Typeface};
use crate::palette::Theme;
use flea::i18n::I18n;
use flea::ui_logic_archive_flash::wire;
use gpui::{prelude::*, px, size, Context, Render, TestAppContext, Window};

struct CardProbe {
    card: Card,
    typeface: Typeface,
    radius: f32,
}

impl Render for CardProbe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        StandIn.present(
            &self.card,
            &palette,
            &self.typeface,
            f32::from(window.viewport_size().width),
            self.radius,
        )
    }
}

struct FlashProbe {
    view: View,
    i18n: I18n,
    radius: f32,
}

impl Render for FlashProbe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let typeface = Typeface::omarchy_default();
        let now = cx.background_executor().now();
        let reduced = cx.reduce_motion();
        StandIn.present_flash(super::draw_flash::FlashFrame {
            view: &self.view,
            i18n: &self.i18n,
            palette: &palette,
            chrome: theme.chrome(),
            typeface: &typeface,
            radius: self.radius,
            taps: &super::draw_flash::FlashTaps::default(),
            press: &super::flash::PressAnim::settled(false),
            now,
            reduced,
            window,
        })
    }
}

fn typeface() -> Typeface {
    Typeface {
        body: px(14.),
        ..Typeface::omarchy_default()
    }
}

fn draw_card(cx: &mut TestAppContext, card: Card, radius: f32) -> bool {
    let (_, cx) = cx.add_window_view(|_, _| CardProbe {
        card,
        typeface: typeface(),
        radius,
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    cx.debug_bounds("op2-card").is_some()
}

fn targets() -> Vec<wire::TargetRow> {
    vec![wire::TargetRow {
        dev: "sda".into(),
        seq: 2,
        serial: "2003007157171692".into(),
        label: "128GB".into(),
        model: "General USB Flash Disk".into(),
        bytes: 124623257600,
        refused: String::new(),
    }]
}

fn opened() -> Flash {
    let mut view = Flash::closed();
    view.open_with_image("/scope/iso/debian-13.iso");
    view.receive_image(
        "/scope/iso/debian-13.iso",
        &wire::FlashImage {
            kind: "iso".into(),
            bytes: 4294967296,
            unpacked: 4294967296,
            known: true,
            member: String::new(),
        },
    );
    view.receive_targets(targets());
    view
}

fn compress_card(i18n: &I18n) -> Card {
    Compress::open(
        vec!["/fx/v/work/notes.txt".into()],
        "/fx/v/work",
        "work",
        0,
        &["7z", "zip"],
    )
    .0
    .card(i18n)
}

fn extract_card(i18n: &I18n) -> (Extract, Card) {
    let mut card = Extract::closed();
    card.open("/fx/v/work/locked.7z", "/fx/v/work/locked", "", i18n);
    let drawn = card.card(i18n);
    (card, drawn)
}

#[gpui::test]
fn cards_draw_frame_buttons_and_format_label_at_both_radii(cx: &mut TestAppContext) {
    let i18n = english();
    for radius in [0., 8.] {
        assert!(
            draw_card(cx, compress_card(&i18n), radius),
            "compress draws at radius {radius}"
        );
        let (_, cx) = cx.add_window_view(|_, _| CardProbe {
            card: compress_card(&i18n),
            typeface: typeface(),
            radius,
        });
        cx.simulate_resize(size(px(1100.), px(700.)));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("op2-compress-format-label").is_some(),
            "FORMAT caption over the chips"
        );
        assert!(
            cx.debug_bounds("op2-button-0").is_some() && cx.debug_bounds("op2-button-1").is_some()
        );
        let live = if radius == 0. {
            "op2-radius-0"
        } else {
            "op2-radius-8"
        };
        let other = if radius == 0. {
            "op2-radius-8"
        } else {
            "op2-radius-0"
        };
        assert!(
            cx.debug_bounds(live).is_some(),
            "the card frame carries the resolved radius {radius}"
        );
        assert!(
            cx.debug_bounds(other).is_none(),
            "the card frame never carries the other radius"
        );
        let (_, cx) = cx.add_window_view(|_, _| CardProbe {
            card: extract_card(&i18n).1,
            typeface: typeface(),
            radius,
        });
        cx.simulate_resize(size(px(1100.), px(700.)));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("op2-card").is_some(),
            "extract draws at radius {radius}"
        );
    }
}

#[gpui::test]
fn flash_rest_draws_flash_action_and_no_run_states(cx: &mut TestAppContext) {
    let i18n = english();
    let view = opened().view(&i18n);
    let (_, cx) = cx.add_window_view(|_, _| FlashProbe {
        view,
        i18n: i18n.clone(),
        radius: 8.,
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("op2-flash").is_some());
    assert!(
        cx.debug_bounds("op2-flash-action").is_some(),
        "rest draws the Flash action"
    );
    assert!(cx.debug_bounds("op2-flash-cancel").is_none());
    assert!(cx.debug_bounds("op2-flash-bar").is_none());
    assert!(cx.debug_bounds("op2-flash-byte").is_none());
    assert!(cx.debug_bounds("op2-flash-end").is_none());
    assert!(
        cx.debug_bounds("op2-flash-cursor-edge").is_some(),
        "the cursor row takes its edge"
    );
    assert!(
        cx.debug_bounds("op2-flash-cursor-edge-live").is_some(),
        "the cursor edge paints, not just lays out"
    );
    assert!(
        cx.debug_bounds("op2-radius-8").is_some(),
        "the flash column carries the resolved radius"
    );
    assert!(
        cx.debug_bounds("op2-radius-0").is_none(),
        "the flash column never carries the other radius"
    );
}

#[gpui::test]
fn flash_running_draws_cancel_bar_and_byte_line(cx: &mut TestAppContext) {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 1, 4294967296, true, 0);
    view.progress_transfer(
        &i18n,
        &wire::FlashProgress {
            id: 1,
            phase: "write".into(),
            bytes: 1048576,
            total: 4294967296,
            permille: 0,
        },
        3000,
    );
    let drawn = view.view(&i18n);
    let (_, cx) = cx.add_window_view(|_, _| FlashProbe {
        view: drawn,
        i18n: i18n.clone(),
        radius: 8.,
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("op2-flash-cancel").is_some(),
        "running draws Cancel, not Flash"
    );
    assert!(
        cx.debug_bounds("op2-flash-bar").is_some(),
        "running draws the bar"
    );
    assert!(
        cx.debug_bounds("op2-flash-byte").is_some(),
        "running draws the byte line"
    );
    assert!(cx.debug_bounds("op2-flash-action").is_none());
    assert!(cx.debug_bounds("op2-flash-end").is_none());
}

#[gpui::test]
fn flash_ending_draws_its_end_button(cx: &mut TestAppContext) {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 1, 1000, true, 0);
    let done = flea::jsondoc::parse(
        r#"{"t":"flashdone","id":1,"ok":false,"phase":"write","at":0,"err":"busy","blank":false}"#,
    )
    .unwrap();
    view.finish_transfer(&i18n, &wire::parse_flashdone(&done));
    let drawn = view.view(&i18n);
    let (_, cx) = cx.add_window_view(|_, _| FlashProbe {
        view: drawn,
        i18n: i18n.clone(),
        radius: 8.,
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("op2-flash-end").is_some(),
        "a failure draws its end button"
    );
    assert!(cx.debug_bounds("op2-flash-action").is_none());
    assert!(cx.debug_bounds("op2-flash-cancel").is_none());
}

#[gpui::test]
fn flash_refused_image_and_empty_drives_draw_their_marks(cx: &mut TestAppContext) {
    let i18n = english();
    let mut view = Flash::closed();
    view.open_with_image("/scope/iso/notes.txt");
    view.receive_image(
        "/scope/iso/notes.txt",
        &wire::FlashImage {
            kind: String::new(),
            bytes: 10,
            unpacked: 0,
            known: false,
            member: String::new(),
        },
    );
    let drawn = view.view(&i18n);
    let (_, cx) = cx.add_window_view(|_, _| FlashProbe {
        view: drawn,
        i18n: i18n.clone(),
        radius: 8.,
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("op2-flash-image-hint").is_some(),
        "a refused image draws its hint"
    );
    let mut bare = Flash::closed();
    bare.open();
    bare.receive_targets(Vec::new());
    let picker = bare.view(&i18n);
    let (_, cx) = cx.add_window_view(|_, _| FlashProbe {
        view: picker,
        i18n: i18n.clone(),
        radius: 0.,
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("op2-flash-empty-mark").is_some(),
        "the empty row takes the drive mark"
    );
    assert!(
        cx.debug_bounds("op2-radius-0").is_some(),
        "the flash column carries the resolved radius 0"
    );
    assert!(
        cx.debug_bounds("op2-radius-8").is_none(),
        "the flash column never carries the other radius"
    );
}

#[gpui::test]
fn ejected_flash_draws_no_action(cx: &mut TestAppContext) {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 1, 1000, true, 0);
    let done = flea::jsondoc::parse(
        r#"{"t":"flashdone","id":1,"ok":true,"phase":"verify","at":0,"err":"","blank":false}"#,
    )
    .unwrap();
    view.finish_transfer(&i18n, &wire::parse_flashdone(&done));
    let eject = flea::jsondoc::parse(r#"{"t":"flasheject","ok":true,"err":""}"#).unwrap();
    view.finish_eject(&i18n, &wire::parse_flasheject(&eject));
    let drawn = view.view(&i18n);
    let (_, cx) = cx.add_window_view(|_, _| FlashProbe {
        view: drawn,
        i18n: i18n.clone(),
        radius: 8.,
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("op2-flash-action").is_none());
    assert!(cx.debug_bounds("op2-flash-end").is_none());
    assert!(
        cx.debug_bounds("op2-flash-cancel").is_none(),
        "after Eject no button is drawn"
    );
}

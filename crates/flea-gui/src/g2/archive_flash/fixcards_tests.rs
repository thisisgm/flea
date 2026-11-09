//! EY-483: the Compress card's unselected Format chip reads in Theme.color.muted, lighter than the chosen one on Latte.
use super::cards::Tokens;
use super::draw_controls::chip_ink;
use crate::list::Palette;
use crate::palette::Theme;
use gpui::{Hsla, Rgba};
use std::collections::BTreeMap;

/// The golden table's stock palette count, the floor golden_tests.rs also holds.
const STOCK_PALETTES: usize = 23;

// Sample lines: "catppuccin-latte\t14\tinput.colors.000007.foreground\t#4c4f69" and "catppuccin-latte\t14\tcolor.muted\t#ff64666e".
fn stock() -> BTreeMap<String, (String, String)> {
    let mut out: BTreeMap<String, (String, String)> = BTreeMap::new();
    for line in include_str!("../../../../../tests/fixtures/theme-golden.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if line.starts_with('#') || fields.len() != 4 || fields[1] != "14" {
            continue;
        }
        let entry = out.entry(fields[0].to_owned()).or_default();
        if let Some(key) = fields[2].strip_prefix("input.colors.") {
            let key = key.split_once('.').map_or(key, |(_, rest)| rest);
            entry.0.push_str(&format!("{key} = \"{}\"\n", fields[3]));
        } else if fields[2] == "color.muted" {
            entry.1 = fields[3].to_owned();
        }
    }
    out
}

fn argb(color: Hsla) -> String {
    let c: Rgba = color.into();
    let q = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", q(c.a), q(c.r), q(c.g), q(c.b))
}

fn tokens(colors: &str) -> Tokens {
    Tokens::of(&Palette::inherited(
        &gpui::TextStyle::default(),
        Theme::from_text(colors),
    ))
}

#[test]
fn fixcards_ey483_unselected_chip_is_lighter_than_the_chosen_one_on_latte() {
    let stock = stock();
    let (colors, _) = &stock["catppuccin-latte"];
    let t = tokens(colors);
    let (on, off) = (chip_ink(&t, true), chip_ink(&t, false));
    assert!(
        off.l > on.l,
        "Latte: the unselected chip {} must read lighter than the chosen {}",
        argb(off),
        argb(on)
    );
}

#[test]
fn fixcards_ey483_unselected_chip_takes_the_muted_role_in_every_stock_palette() {
    let stock = stock();
    assert!(
        stock.len() >= STOCK_PALETTES,
        "stock palette coverage shrank"
    );
    for (name, (colors, muted)) in &stock {
        let t = tokens(colors);
        assert_eq!(&argb(chip_ink(&t, false)), muted, "{name}");
        assert_eq!(chip_ink(&t, true), t.fg, "{name}");
    }
}

// EY-495: a press held on the Extract card's painted eye reveals the password, and any release masks it again.
#[gpui::test]
fn fixcards_ey495_holding_the_extract_eye_reveals_until_release(cx: &mut gpui::TestAppContext) {
    use crate::{backend::Backend, state::tests::shell_with, window::root::Root};
    use gpui::{AppContext, Modifiers, MouseButton};
    let (backend, _lines) = Backend::test_operation_channel();
    let (root, cx) = cx.add_window_view(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.update(|_, cx| {
        shell.update(cx, |s, cx| {
            let catalog = s.locale.catalog.clone();
            s.operations
                .archive
                .extract
                .open("/files/a.7z", "/files", "", &catalog);
            cx.notify();
        })
    });
    cx.run_until_parked();
    let held = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, _| s.operations.archive.extract.eye_held())
    };
    let eye = shell
        .read_with(cx, |s, _| s.operations.archive.eye.get())
        .expect("the Extract card paints its eye");
    let card = cx
        .debug_bounds("op2-card")
        .expect("the Extract card is drawn");
    let beside = gpui::point(card.left() + gpui::px(4.), card.top() + gpui::px(4.));
    cx.simulate_mouse_down(beside, MouseButton::Left, Modifiers::none());
    assert!(!held(cx), "a press off the eye reveals nothing");
    cx.simulate_mouse_up(beside, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_down(eye.center(), MouseButton::Left, Modifiers::none());
    assert!(held(cx), "a press held on the eye reveals the password");
    cx.simulate_mouse_up(card.center(), MouseButton::Left, Modifiers::none());
    assert!(!held(cx), "a release on the card masks it again");
    cx.simulate_mouse_down(eye.center(), MouseButton::Left, Modifiers::none());
    assert!(held(cx), "a second press on the eye reveals it again");
    let outside = gpui::point(gpui::px(-20.), gpui::px(-20.));
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    assert!(!held(cx), "a release dragged off the window masks it too");
}

// EY-557: a press held on a Compress field's painted eye reveals that field alone, and any release masks it.
#[gpui::test]
fn fixcards_ey557_holding_a_compress_eye_reveals_that_field_until_release(
    cx: &mut gpui::TestAppContext,
) {
    use crate::{backend::Backend, state::tests::shell_with, window::root::Root};
    use gpui::{AppContext, Modifiers, MouseButton};
    let (backend, _lines) = Backend::test_operation_channel();
    let (root, cx) = cx.add_window_view(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.update(|_, cx| {
        shell.update(cx, |s, cx| {
            let (card, _) = super::compress::Compress::open(
                vec!["/files/notes.txt".into()],
                "/files",
                "files",
                0,
                &["7z", "zip"],
            );
            s.operations.archive.compress = Some(card);
            cx.notify();
        })
    });
    cx.run_until_parked();
    // The Password and Confirm fields' revealed flags in body order, then whether the closed Extract card's eye is held.
    let revealed = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, _| {
            let card = s
                .operations
                .archive
                .compress
                .as_ref()
                .expect("Compress is open")
                .card(&s.locale.catalog);
            card.body
                .iter()
                .filter_map(|block| match block {
                    super::cards::Block::Field(field) if field.secret => Some(field.revealed),
                    _ => None,
                })
                .chain([s.operations.archive.extract.eye_held()])
                .collect::<Vec<_>>()
        })
    };
    let password = cx
        .debug_bounds("op2-eye-0")
        .expect("the Password eye is drawn");
    let confirm = cx
        .debug_bounds("op2-eye-1")
        .expect("the Confirm eye is drawn");
    let card = cx
        .debug_bounds("op2-card")
        .expect("the Compress card is drawn");
    assert_eq!(
        revealed(cx),
        [false, false, false],
        "both fields open masked"
    );
    cx.simulate_mouse_down(password.center(), MouseButton::Left, Modifiers::none());
    assert_eq!(
        revealed(cx),
        [true, false, false],
        "the Password eye reveals Password alone"
    );
    cx.simulate_mouse_up(card.center(), MouseButton::Left, Modifiers::none());
    assert_eq!(
        revealed(cx),
        [false, false, false],
        "a release on the card masks it again"
    );
    cx.simulate_mouse_down(confirm.center(), MouseButton::Left, Modifiers::none());
    assert_eq!(
        revealed(cx),
        [false, true, false],
        "the Confirm eye reveals Confirm alone"
    );
    let outside = gpui::point(gpui::px(-20.), gpui::px(-20.));
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    assert_eq!(
        revealed(cx),
        [false, false, false],
        "a release dragged off the window masks it too"
    );
}

// EY-531: after a wrong password the field still holds the typed count as bullets, so Extract stays available.
#[test]
fn fixcards_ey531_extract_stays_available_while_the_field_holds_text_after_a_wrong_password() {
    use super::extract::{Extract, Stop};
    let i18n = super::tests::english();
    let extract = |card: &Extract| {
        card.card(&i18n)
            .buttons
            .iter()
            .find(|button| button.primary)
            .expect("the Extract button")
            .available
    };
    let mut card = Extract::closed();
    card.open("/Sec/enc1.zip", "/Sec/enc1", "", &i18n);
    assert!(
        !extract(&card),
        "an empty first ask keeps Extract unavailable"
    );
    card.take_secret_for_test("wrong-pass");
    assert!(extract(&card), "typed text makes Extract available");
    card.press(&i18n, Stop::Extract);
    card.open("/Sec/enc1.zip", "/Sec/enc1", "wrong-password", &i18n);
    assert_eq!(
        card.held_marks(),
        "wrong-pass".len(),
        "the field keeps the typed count"
    );
    let secret = card.card(&i18n).body.iter().find_map(|block| match block {
        super::cards::Block::Field(field) if field.secret => Some(field.value.clone()),
        _ => None,
    });
    assert_eq!(
        secret.as_deref(),
        Some(""),
        "the refused secret is gone, so only the bullets can keep Extract available"
    );
    assert!(
        extract(&card),
        "the held bullets alone keep Extract available"
    );
}

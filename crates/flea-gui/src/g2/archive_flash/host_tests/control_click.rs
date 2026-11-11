//! A click on a card's chip, field or checkbox answers as that control's key does, through the production pointer path.
use super::cancel_click::{card_open, click, drawn, extract_card};
use super::*;
use crate::g2::archive_flash::cards::{Block, Card};
use gpui::{Modifiers, MouseButton};

const CHIPS: [&str; 6] = [
    "op2-chip-0",
    "op2-chip-1",
    "op2-chip-2",
    "op2-chip-3",
    "op2-chip-4",
    "op2-chip-5",
];

/// The Compress card over a backend that reports three formats, so the chips have somewhere to go.
fn compress_card(cx: &mut TestAppContext) -> Handle {
    let (handle, _requests, _other) = start(cx);
    receive(
        cx,
        handle,
        0,
        r#"{"t":"formats","archive":["zip","tar","7z"],"convert":false,"extract":{"archive":true,"sevenZip":true,"zip":true}}"#,
    );
    choose(
        cx,
        handle,
        72,
        "compress",
        "compressPassword",
        &["/files/notes.txt"],
    );
    assert!(
        card_open(cx, handle),
        "With Password opens the Compress card"
    );
    handle
}

/// The open card as the shell draws it, with no live input mounted.
fn card_of(cx: &mut TestAppContext, handle: Handle) -> Card {
    edit(cx, handle, |shell, _, _| {
        let catalog = shell.locale.catalog.clone();
        let archive = &shell.operations.archive;
        match archive.compress.as_ref().filter(|card| card.opened()) {
            Some(card) => card.card(&catalog),
            None => archive.extract.card(&catalog),
        }
    })
}

/// The Format chips' count and the chosen one.
fn chips(card: &Card) -> (usize, usize) {
    card.body
        .iter()
        .find_map(|block| match block {
            Block::Chips(chips) => Some((chips.options.len(), chips.current)),
            _ => None,
        })
        .expect("the Compress card draws its Format chips")
}

fn encrypt(card: &Card) -> (bool, bool) {
    card.body
        .iter()
        .find_map(|block| match block {
            Block::CheckLine(check) => Some((check.checked, check.focused)),
            _ => None,
        })
        .expect("a 7z card draws the Encrypt checkbox")
}

/// The Compress card after these keys, or after a click on this selector followed by the typed keys.
fn compress_after(by_click: Option<&'static str>, keyed: &str, typed: &str) -> Card {
    let mut cx = crate::list::bench::real_cx();
    let handle = compress_card(&mut cx);
    match by_click {
        Some(selector) => {
            let at = drawn(&mut cx, handle, selector);
            click(&mut cx, handle, at.center());
        }
        None => keys(&mut cx, handle, keyed),
    }
    if !typed.is_empty() {
        keys(&mut cx, handle, typed);
    }
    assert!(card_open(&mut cx, handle), "the card stays open");
    card_of(&mut cx, handle)
}

#[test]
fn b26_a_click_on_each_format_chip_picks_it_as_the_arrow_keys_do() {
    let mut cx = crate::list::bench::real_cx();
    let handle = compress_card(&mut cx);
    let (count, first) = chips(&card_of(&mut cx, handle));
    assert!(
        (2..=CHIPS.len()).contains(&count),
        "the card offers more than one format, each with a locator"
    );
    for (at, chip) in CHIPS.iter().enumerate().take(count) {
        let walk = if at < first { "left " } else { "right " }.repeat(at.abs_diff(first));
        let keyed = compress_after(None, &format!("shift-tab shift-tab {walk}"), "");
        let clicked = compress_after(Some(chip), "", "");
        assert_eq!(chips(&clicked).1, at, "the clicked chip is chosen");
        assert_eq!(
            clicked, keyed,
            "a click on chip {at} answers as the arrows to it do"
        );
    }
}

#[test]
fn b26_a_click_on_the_encrypt_checkbox_toggles_it_as_space_does() {
    let opened = compress_after(None, "", "");
    let clicked = compress_after(Some("op2-encrypt"), "", "");
    assert_ne!(encrypt(&clicked).0, encrypt(&opened).0, "the click toggles");
    assert_eq!(encrypt(&clicked), (!encrypt(&opened).0, true));
    assert_eq!(clicked, compress_after(None, "tab tab space", ""));
    let twice = {
        let mut cx = crate::list::bench::real_cx();
        let handle = compress_card(&mut cx);
        for _ in 0..2 {
            let at = drawn(&mut cx, handle, "op2-encrypt");
            click(&mut cx, handle, at.center());
        }
        card_of(&mut cx, handle)
    };
    assert_eq!(twice, compress_after(None, "tab tab space space", ""));
}

#[test]
fn b26_a_click_on_an_unfocused_field_takes_the_keys_as_tab_to_it_does() {
    for (selector, walk) in [("op2-field-0", "shift-tab"), ("op2-field-2", "tab")] {
        let clicked = compress_after(Some(selector), "", "x->x");
        assert_eq!(
            clicked,
            compress_after(None, walk, "x->x"),
            "a click on {selector} focuses it and the next key types there"
        );
    }
}

#[test]
fn b26_a_click_on_compress_focuses_it_before_its_refusal_as_enter_does() {
    let clicked = compress_after(Some("op2-button-1"), "", "");
    assert_eq!(clicked, compress_after(None, "tab tab tab tab enter", ""));
    assert!(
        clicked.buttons[1].focused,
        "the refused button keeps the focus"
    );
}

#[test]
fn b26_a_held_eye_on_an_unfocused_field_never_moves_the_focus() {
    let mut cx = crate::list::bench::real_cx();
    let handle = compress_card(&mut cx);
    let opened = card_of(&mut cx, handle);
    let eye = edit(&mut cx, handle, |shell, _, _| {
        shell.operations.archive.confirm_eye.get()
    })
    .expect("the Confirm eye painted");
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_move(eye.center(), None, Modifiers::none());
    visual.simulate_mouse_down(eye.center(), MouseButton::Left, Modifiers::none());
    frame(&mut cx, handle);
    let revealed: Vec<bool> = card_of(&mut cx, handle)
        .body
        .iter()
        .filter_map(|block| match block {
            Block::Field(field) if field.secret => Some(field.revealed),
            _ => None,
        })
        .collect();
    assert_eq!(
        revealed,
        [false, true],
        "the held eye reveals Confirm alone"
    );
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_up(eye.center(), MouseButton::Left, Modifiers::none());
    frame(&mut cx, handle);
    assert_eq!(
        card_of(&mut cx, handle),
        opened,
        "the release hides it again; Password keeps the keys"
    );
}

#[test]
fn b26_a_click_on_the_extract_password_takes_the_keys_back_from_a_button() {
    let card = |by_click: bool| {
        let mut cx = crate::list::bench::real_cx();
        let (handle, _requests, _other) = extract_card(&mut cx);
        keys(&mut cx, handle, "tab");
        assert!(
            card_of(&mut cx, handle).buttons[0].focused,
            "Cancel holds the keys"
        );
        if by_click {
            let field = drawn(&mut cx, handle, "op2-field-0");
            click(&mut cx, handle, field.center());
        } else {
            keys(&mut cx, handle, "shift-tab");
        }
        keys(&mut cx, handle, "x->x");
        card_of(&mut cx, handle)
    };
    let clicked = card(true);
    assert!(
        clicked.focused_button().is_none(),
        "the field holds the keys"
    );
    assert_eq!(clicked, card(false));
}

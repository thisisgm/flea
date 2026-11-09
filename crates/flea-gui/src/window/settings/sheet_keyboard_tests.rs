//! The sheet's keyboard built over the US fixture's samples with the live dispatcher: what a press would run, nothing more.
use super::Samples;
use crate::backend::Backend;
use crate::window::user_keys::test_fixture::samples;
use flea::keys::{CONTROL, SHIFT};
use flea::ui_logic_st1::sheet_keyboard::SheetKeyboard;
use gpui::{AppContext, Capslock, Keystroke, Modifiers, TestAppContext};

fn built(
    cx: &mut TestAppContext,
    preset: &'static str,
    sampler: fn(Modifiers, Capslock) -> Samples,
) -> SheetKeyboard {
    let (backend, requests) = Backend::test_channel();
    std::mem::forget(requests);
    let shell = cx.new(|cx| crate::state::tests::shell_with(backend, cx));
    shell
        .read_with(cx, |shell, _| {
            shell.sheet_keyboard_from(preset, false, sampler)
        })
        .expect("the fixture samples a keymap")
}

fn action(keyboard: &SheetKeyboard, id: &str, mask: u8) -> Option<String> {
    let cap = keyboard.faces.iter().position(|face| face.cap.id == id)?;
    keyboard
        .bound
        .iter()
        .find(|bound| bound.cap == cap && bound.mask == mask)
        .map(|bound| bound.action.clone())
}

/// The fixture with `[` turned into a dead key, which types nothing alone, as ¨ and ´ are on Swedish.
fn dead_bracket(held: Modifiers, caps: Capslock) -> Option<Vec<(Keystroke, Modifiers)>> {
    let mut samples = samples(held, caps)?;
    for (keystroke, _) in &mut samples {
        if keystroke.us_key.as_deref() == Some("[") || keystroke.us_key.as_deref() == Some("{") {
            keystroke.key_char = None;
        }
    }
    Some(samples)
}

#[gpui::test]
fn the_keyboard_reads_the_dispatchers_answer_for_each_chord(cx: &mut TestAppContext) {
    let keyboard = built(cx, "default", samples);
    assert_eq!(action(&keyboard, "AB03", CONTROL).as_deref(), Some("copy"));
    assert_eq!(
        action(&keyboard, "AB03", CONTROL | SHIFT).as_deref(),
        Some("copyPath")
    );
    assert_eq!(action(&keyboard, "FK02", 0).as_deref(), Some("rename"));
    assert_eq!(
        action(&keyboard, "AB10", SHIFT).as_deref(),
        Some("keymapSheet")
    );
    assert_eq!(action(&keyboard, "AC07", 0).as_deref(), Some("cursorDown"));
    let copy = keyboard
        .bound
        .iter()
        .find(|bound| bound.action == "copy")
        .unwrap();
    assert_eq!(copy.label, "Copy");
}

#[gpui::test]
fn a_us_group_draws_an_ansi_keyboard(cx: &mut TestAppContext) {
    let keyboard = built(cx, "default", samples);
    assert!(!keyboard.faces.iter().any(|face| face.cap.id == "LSGT"));
    assert_eq!(
        keyboard
            .faces
            .iter()
            .filter(|face| face.cap.id == "RTRN")
            .count(),
        1
    );
    let q = keyboard
        .faces
        .iter()
        .find(|face| face.cap.id == "AD01")
        .unwrap();
    assert_eq!(q.legend, "Q");
    let one = keyboard
        .faces
        .iter()
        .find(|face| face.cap.id == "AE01")
        .unwrap();
    assert_eq!((one.legend.as_str(), one.shifted.as_str()), ("1", "!"));
}

#[gpui::test]
fn a_vim_preset_marks_its_pair_arms(cx: &mut TestAppContext) {
    let keyboard = built(cx, "vim", samples);
    let pairs: Vec<_> = keyboard
        .bound
        .iter()
        .filter(|bound| bound.pair() && bound.mask == 0)
        .map(|bound| keyboard.faces[bound.cap].cap.id)
        .collect();
    assert!(pairs.contains(&"AC03"), "d d cuts: {pairs:?}");
}

#[gpui::test]
fn a_dead_key_binds_nothing_alone_or_with_shift(cx: &mut TestAppContext) {
    let live = built(cx, "default", samples);
    assert!(
        action(&live, "AD11", 0).is_some(),
        "the fixture's [ is bound"
    );
    let dead = built(cx, "default", dead_bracket);
    assert_eq!(action(&dead, "AD11", 0), None);
    assert_eq!(action(&dead, "AD11", SHIFT), None);
    let face = dead
        .faces
        .iter()
        .find(|face| face.cap.id == "AD11")
        .unwrap();
    assert!(face.legend.is_empty());
}

/// Every default listing action the window answers shows on the keyboard under at least one of its keys: none is lost
/// between table and board.
#[gpui::test]
fn every_default_listing_action_reaches_the_keyboard(cx: &mut TestAppContext) {
    let keyboard = built(cx, "default", with_navigation);
    let shown = |action: &str| {
        keyboard.bound.iter().any(|bound| {
            bound.action == action || flea::keys::action_group(&bound.action) == action
        })
    };
    let missing: Vec<&str> = flea::keys::PRESET_KEYS
        .iter()
        .chain(flea::keys::SHARED_KEYS)
        .filter(|binding| matches!(binding.preset, "all" | "default"))
        .filter(|binding| {
            binding
                .context
                .split(',')
                .any(|context| context == "listing")
        })
        .filter(|binding| binding.frontend != "tui")
        .filter(|binding| !shown(binding.action))
        .map(|binding| binding.keys)
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
}

/// The fixture with the navigation keys a real keymap samples too.
fn with_navigation(held: Modifiers, caps: Capslock) -> Option<Vec<(Keystroke, Modifiers)>> {
    let mut samples = samples(held, caps)?;
    for key in [
        "left",
        "right",
        "up",
        "down",
        "delete",
        "home",
        "end",
        "pageup",
        "pagedown",
        "insert",
        "backspace",
    ] {
        samples.push((
            Keystroke {
                key: key.into(),
                modifiers: held,
                ..Default::default()
            },
            held,
        ));
    }
    Some(samples)
}

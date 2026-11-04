// The Plugins pane as a real GPUI entity in a test window: drawn frames, keys, clicks, emitted effects and marks read off the UI thread.
use super::page::Card;
use super::page_tests::{builtins, checksums, english, listing, Recorder};
use super::seam::{Effect, PluginsWire, Tokens};
use super::view::PluginsPane;
use gpui::{point, px, size, Entity, Modifiers, TestAppContext, VisualTestContext};
use std::cell::RefCell;
use std::rc::Rc;

fn tokens() -> Tokens {
    Tokens::interim(&crate::list::Palette::inherited(
        &gpui::TextStyle::default(),
        crate::palette::Theme::default(),
    ))
}

type Effects = Rc<RefCell<Vec<Effect>>>;

// The Settings pane's own box on the boards: 630 wide card less its 168px rail, 684 tall less the 28px title.
fn pane(
    cx: &mut TestAppContext,
) -> (
    Entity<PluginsPane>,
    Rc<Recorder>,
    Effects,
    &mut VisualTestContext,
) {
    let wire = Rc::new(Recorder::default());
    let effects = Rc::new(RefCell::new(Vec::new()));
    let (sent, seen) = (wire.clone(), effects.clone());
    let (entity, cx) = cx.add_window_view(move |_, cx| {
        PluginsPane::new(english(), sent as Rc<dyn PluginsWire>, tokens(), 8., cx)
    });
    cx.simulate_resize(size(px(462.), px(656.)));
    cx.update(|_, cx| {
        cx.subscribe(&entity, move |_, effect: &Effect, _| {
            seen.borrow_mut().push(effect.clone())
        })
        .detach()
    });
    cx.update(|window, cx| entity.read(cx).focus_handle().clone().focus(window, cx));
    (entity, wire, effects, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn title(entity: &Entity<PluginsPane>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| entity.read(cx).page().title())
}

#[gpui::test]
fn st2_gui_view_draws_the_list_and_its_keys_reach_the_wire(cx: &mut TestAppContext) {
    let (entity, wire, effects, cx) = pane(cx);
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"list","id":1}"#,
        "the pane asks for the list when it opens"
    );
    draw(cx);
    cx.update(|_, cx| entity.update(cx, |pane, cx| pane.receive(&listing(1, builtins("{}")), cx)));
    draw(cx);
    cx.simulate_keystrokes("down space");
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"enable","id":2,"plugin":"flea.shelf"}"#
    );
    cx.simulate_keystrokes("down enter");
    assert_eq!(title(&entity, cx), "LocalSend");
    draw(cx);
    cx.simulate_keystrokes("escape");
    assert_eq!(title(&entity, cx), "Plugins");
    assert!(effects.borrow().is_empty());
    cx.simulate_keystrokes("escape");
    assert_eq!(
        *effects.borrow(),
        [Effect::Close],
        "esc on the list goes to the Settings host"
    );
}

#[gpui::test]
fn st2_gui_view_the_add_card_takes_typing_and_draws_over_the_list(cx: &mut TestAppContext) {
    // The Settings host draws the card and mounts its field, so typing needs the real parent.
    let (entity, wire, cx) = super::ime_tests::mounted(cx);
    cx.simulate_keystrokes("a b space c backspace");
    let text = cx.update(|_, cx| match entity.read(cx).page().card() {
        Card::Add(card) => card.text.clone(),
        other => panic!("{other:?}"),
    });
    assert_eq!(
        text, "ab ",
        "space types a space while the card holds the keyboard"
    );
    draw(cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"add","id":2,"source":"ab"}"#
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(
        cx.update(|_, cx| entity.read(cx).page().card().clone()),
        Card::None
    );
}

#[gpui::test]
fn st2_gui_view_a_click_opens_a_row_and_a_click_on_its_box_only_toggles(cx: &mut TestAppContext) {
    let (entity, wire, _, cx) = pane(cx);
    cx.update(|_, cx| entity.update(cx, |pane, cx| pane.receive(&listing(1, builtins("{}")), cx)));
    draw(cx);
    // LocalSend is the second 37px row under the INSTALLED heading, whose height is 16 + a caption line + 7.8.
    let y = px(95.);
    // The box sits before the 19px chevron, its 9px gap and the 14px pad: its centre is 462 - 14 - 19 - 9 - 9.
    cx.simulate_click(point(px(411.), y), Modifiers::none());
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"disable","id":2,"plugin":"flea.localsend"}"#
    );
    assert_eq!(
        title(&entity, cx),
        "Plugins",
        "the box toggles without opening the page"
    );
    cx.simulate_click(point(px(100.), y), Modifiers::none());
    assert_eq!(
        title(&entity, cx),
        "LocalSend",
        "the rest of the row opens its page"
    );
}

#[gpui::test]
fn st2_gui_view_a_plugin_mark_is_read_off_the_ui_thread(cx: &mut TestAppContext) {
    let (entity, _, _, cx) = pane(cx);
    let mut gitted = checksums(true, None);
    gitted.mark_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packaging/com.thisisgm.flea.svg"
    )
    .into();
    let mut missing = checksums(false, None);
    missing.manifest.id = "mtorvik.gone".into();
    missing.mark_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packaging/no-such-mark.svg"
    )
    .into();
    let mut plugins = builtins("{}");
    plugins.extend([gitted, missing]);
    cx.update(|_, cx| entity.update(cx, |pane, cx| pane.receive(&listing(1, plugins), cx)));
    assert_eq!(
        cx.update(|_, cx| entity.read(cx).loaded_marks()),
        0,
        "nothing is read inside receive"
    );
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| entity.read(cx).loaded_marks()),
        1,
        "the mark that exists loads, the missing one draws the glyph"
    );
    draw(cx);
}

#[test]
fn vf4_plugins_failed_row_keeps_its_hash_mark() {
    let (_, svg) = super::glyphs::glyph("hash").expect("Checksums keeps its mark when failed");
    assert!(std::str::from_utf8(svg)
        .unwrap()
        .contains("M4 9h16 M4 15h16"));
}

#[gpui::test]
fn vf4_plugins_heading_uses_the_board_line_box(cx: &mut TestAppContext) {
    let (entity, _, _, cx) = pane(cx);
    cx.update(|_, cx| entity.update(cx, |pane, cx| pane.receive(&listing(1, builtins("{}")), cx)));
    draw(cx);
    let parts = entity.read_with(cx, |pane, _| pane.probe.rect("row:plugins.installed"));
    let rect: Vec<f32> = parts
        .split_whitespace()
        .map(|p| p.parse().unwrap())
        .collect();
    assert_eq!(rect.len(), 4, "{parts}");
    assert!(
        (rect[3] - 38.2).abs() < 1.,
        "CSS heading is 16 + 14.4 + 7.8: {rect:?}"
    );
}

#[gpui::test]
fn visual_geometry_plugins_heading_moves_without_moving_rows(cx: &mut TestAppContext) {
    let (entity, _, _, cx) = pane(cx);
    cx.update(|_, cx| entity.update(cx, |pane, cx| pane.receive(&listing(1, builtins("{}")), cx)));
    draw(cx);
    // Probe rectangle sample: "14 17 56 14.4" (x y width height).
    let rect = |id| {
        entity
            .read_with(cx, |pane, _| pane.probe.rect(id))
            .split_whitespace()
            .map(|p| p.parse::<f32>().unwrap())
            .collect::<Vec<_>>()
    };
    let heading = rect("heading:plugins.installed");
    assert_eq!(heading.len(), 4, "heading's painted text is measured");
    assert_eq!(
        heading[1], 17.,
        "16 px top padding plus 1 px text displacement"
    );
    let row = rect("row:flea.shelf");
    assert!(
        (row[1] - 38.2).abs() < 1.,
        "Shelf stays at its approved row: {row:?}"
    );
}

#[test]
fn vf4_r1_localsend_mark_fills_its_disc_inside_the_dashed_ring() {
    let (_, svg) = super::glyphs::glyph("localsend").expect("LocalSend keeps its mark");
    let image = gpui::SvgRenderer::new(std::sync::Arc::new(()))
        .render_single_frame(svg, 1.)
        .unwrap();
    let (width, height) = (
        image.size(0).width.0 as usize,
        image.size(0).height.0 as usize,
    );
    let bytes = image.as_bytes(0).unwrap();
    // Alpha along the centre row, at a radius in the board's 24-unit box (plugin-failed: a filled r 6.62 disc).
    let alpha = |radius: f32| {
        let x = (width as f32 * (12. + radius) / 24.) as usize;
        bytes[((height / 2) * width + x) * 4 + 3]
    };
    assert!(alpha(0.) > 200 && alpha(4.) > 200, "the disc is filled");
    assert_eq!(alpha(7.3), 0, "the disc carries no stroke past r 6.62");
    assert!(alpha(11.2) > 100, "the dashed ring at r 11.18 stays");
}

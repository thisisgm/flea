//! Fact rows laid out by the real renderer at the Settings pane's board width.
use super::mark::metrics as m;
use super::page_tests::{english, Recorder};
use super::seam::Tokens;
use super::view::PluginsPane;
use flea::ui_logic_st2::pages::Row;
use gpui::{px, size, Div, TestAppContext};
use std::rc::Rc;

pub(super) fn measure(row: Div, index: usize, probe: crate::g2::st1::probe::Probe) -> Div {
    row.on_children_prepainted(move |children, _, _| {
        for (part, bounds) in children.into_iter().enumerate() {
            probe.record(&format!("fact-test:{index}:{part}"), bounds);
        }
    })
}

#[gpui::test]
fn plugin_source_row_long_origin_keeps_label_and_bounds_value(cx: &mut TestAppContext) {
    let origin = format!("/home/gm/{}/hello", "long-plugin-parent/".repeat(18));
    let (view, cx) = cx.add_window_view(|_, cx| {
        let palette = crate::list::Palette::inherited(
            &gpui::TextStyle::default(),
            crate::palette::Theme::default(),
        );
        let mut pane = PluginsPane::new(
            english(),
            Rc::new(Recorder::default()),
            Tokens::interim(&palette),
            8.,
            cx,
        );
        pane.page.rows = vec![
            Row::Fact {
                text: "Installed from".into(),
                value: origin.clone(),
            },
            Row::Fact {
                text: "Reads the files you choose".into(),
                value: String::new(),
            },
            Row::Action {
                id: "remove".into(),
                label: "Remove plugin…".into(),
                glyph: String::new(),
                value: String::new(),
                chevron: false,
            },
        ];
        pane
    });
    cx.simulate_resize(size(px(462.), px(300.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| {
        let probe = &view.read(cx).probe;
        let label = probe.bounds("fact-test:0:2").unwrap();
        let value = probe.bounds("fact-test:0:3").unwrap();
        let action_label = probe.bounds("fact-test:2:2").unwrap();
        let style = window.text_style();
        let width = |text: &str| {
            window
                .text_system()
                .shape_line(
                    text.to_owned().into(),
                    px(m::BODY),
                    &[style.to_run(text.len())],
                    None,
                )
                .width
        };
        assert_eq!(
            label.origin.x, action_label.origin.x,
            "facts use the action label column"
        );
        assert!(
            label.size.width >= width("Installed from"),
            "long origin must not squeeze out Installed from: {label:?}"
        );
        assert!(
            value.size.width < width(&origin),
            "long origin must take a bounded elision box"
        );
        assert!(
            value.right() <= px(462. - m::PAD_X),
            "origin must stay inside row padding: {value:?}"
        );
        assert_eq!(
            value.origin.x,
            label.right() + px(m::GAP),
            "origin follows its label without a spacer"
        );
        assert_eq!(
            probe.bounds("fact-test:1:2").unwrap().origin.x,
            label.origin.x,
            "ACCESS keeps the same label column"
        );
    });
}

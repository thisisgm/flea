//! Open and Save mount their actual rails, filename field, filter and answer controls.
use super::tests::fixture::fixture;
use crate::presentation::frame_gate;
use gpui::{px, size, TestAppContext};

fn check(cx: &mut TestAppContext, saving: bool) {
    let mut fixture = fixture(
        cx,
        if saving {
            r#"{"mode":"save","name":"notes.txt","filters":[{"label":"Text","globs":["*.txt"]}]}"#
        } else {
            r#"{"filters":[{"label":"Text","globs":["*.txt"]}]}"#
        },
        None,
    );
    fixture.update(|picker, _| picker.drawn.arm());
    let theme = frame_gate::theme();
    fixture.cx.simulate_resize(size(px(1100.), px(700.)));
    for radius in [0., 8., 13.] {
        fixture.cx.update(|_, cx| {
            cx.set_global(theme);
            cx.set_global(crate::compositor::Corner(radius));
        });
        frame_gate::capture(fixture.cx);
        let focus = fixture.read(|picker| picker.focus.clone());
        fixture.cx.update(|window, cx| window.focus(&focus, cx));
        frame_gate::capture(fixture.cx);
        let controls = fixture.read(|picker| picker.drawn.controls_json());
        let controls = flea::jsondoc::parse(&controls).unwrap();
        let bounds = controls.as_array().unwrap()[0]
            .get("bounds")
            .unwrap()
            .as_array()
            .unwrap();
        let values: Vec<f32> = bounds.iter().map(|v| v.as_f64().unwrap() as f32).collect();
        let bounds = gpui::Bounds::new(
            gpui::point(px(values[0]), px(values[1])),
            size(px(values[2]), px(values[3])),
        );
        frame_gate::assert_frame(
            if saving { "picker-save" } else { "picker-open" },
            fixture.cx,
            bounds,
            radius,
            theme,
        );
        let named: &[&str] = if saving {
            &["picker-new-folder", "pk1-filename", "picker-filter"]
        } else {
            &["picker-filter"]
        };
        for name in named {
            let bounds = fixture.cx.debug_bounds(name).expect(name);
            frame_gate::assert_frame(name, fixture.cx, bounds, radius, theme);
        }
    }
}

#[gpui::test]
fn vf_frames_picker_open(cx: &mut TestAppContext) {
    check(cx, false);
}
#[gpui::test]
fn vf_frames_picker_save(cx: &mut TestAppContext) {
    check(cx, true);
}

//! Mounted regression for the solid tag squares decoded from accepted rows.
use crate::{backend::Backend, palette::Theme, state::tests::shell_with};
use flea::protocol::{parse_reply, Reply as BackendReply};
use gpui::{AppContext, TestAppContext};
use std::rc::Rc;

#[gpui::test]
fn pb_tags_list_and_grid_paint_solid_squares_from_wire_rows(cx: &mut TestAppContext) {
    let (backend, _read) = Backend::test_channel();
    let handle = cx.add_window(|window, cx| super::root::Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            let BackendReply::Rows { rows, .. } = parse_reply(
                r#"{"t":"rows","start":0,"listing":2,"rows":[{"n":"notes.txt","d":false,"s":1,"m":0,"p":33188,"i":"text-plain","tg":["Red","Blue"]}]}"#
            ).unwrap() else { panic!("rows") };
            let host = shell.panes[0].as_mut().unwrap();
            let held = Rc::get_mut(&mut host.pane.held).unwrap();
            held.rows[0] = rows.into_iter().next().unwrap();
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    for colors in [
        "",
        "background='#ffffff'\nforeground='#111111'",
        "red='invalid'\nblue='invalid'\nbackground='#171717'",
    ] {
        let theme = Theme::from_text(colors);
        cx.set_global(theme);
        for view in [super::startup::View::List, super::startup::View::Grid] {
            handle
                .update(cx, |root, window, cx| {
                    root.shell
                        .update(cx, |shell, cx| shell.set_view(view, window, cx));
                })
                .unwrap();
            cx.update_window(handle.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
                let scale = window.scale_factor();
                let quads = window.painted_quads();
                let squares: Vec<_> = quads
                    .iter()
                    .filter(|quad| {
                        (quad.bounds.size.width.as_f32() - 14. * scale).abs() < 0.1
                            && (quad.bounds.size.height.as_f32() - 14. * scale).abs() < 0.1
                    })
                    .collect();
                assert_eq!(
                    squares.len(),
                    2,
                    "{view:?} must paint two 14px tag squares on {colors:?}"
                );
                for (quad, ink) in squares
                    .iter()
                    .zip([theme.tag_listing_inks[0], theme.tag_listing_inks[4]])
                {
                    assert_eq!(quad.background.as_solid(), Some(ink));
                    assert_eq!(ink.a, 1., "tag ink must stay opaque");
                }
            })
            .unwrap();
        }
    }
}

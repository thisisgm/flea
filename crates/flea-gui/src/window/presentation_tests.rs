use crate::{backend::Backend, palette::Theme, state::tests::shell_with};
use gpui::{div, AppContext, Context, IntoElement, Render, TestAppContext, TextStyle, Window};
use std::{cell::RefCell, rc::Rc};

struct StyleProbe(Rc<RefCell<Option<TextStyle>>>);

impl Render for StyleProbe {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        *self.0.borrow_mut() = Some(window.text_style());
        div()
    }
}

#[gpui::test]
fn u01_presentation_text_stops_reach_painted_rows_and_bounded_requests(cx: &mut TestAppContext) {
    for body in [12, 14, 16] {
        let (backend, _) = Backend::test_channel();
        let handle = cx.add_window(|_, cx| super::root::Root {
            shell: cx.new(|cx| {
                let mut shell = shell_with(backend, cx);
                shell.launch.settings.data = flea::jsondoc::parse(&format!(
                    "{{\"display\":{{\"textSize\":{{\"mode\":{body}}}}}}}"
                ))
                .unwrap();
                shell
            }),
        });
        cx.update_window(handle.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
        })
        .unwrap();
        handle
            .update(cx, |root, window, cx| {
                let shell = root.shell.read(cx);
                let host = shell.panes[0].as_ref().unwrap();
                let list = host.list.borrow();
                assert_eq!(f32::from(list.typeface.body), body as f32);
                assert_eq!(
                    list.drawn.metrics.row,
                    crate::list::Metrics::at(gpui::px(body as f32)).row
                );
                let height = shell
                    .geometry(window)
                    .rows_height(false, shell.slots.strip_height);
                assert_eq!(
                    host.pane.visible_rows(),
                    (height / list.drawn.metrics.row).ceil() as usize
                );
            })
            .unwrap();
    }
}

#[gpui::test]
fn u01_presentation_cursor_paints_the_loaded_accent(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let theme = Theme::from_text("background='#101315'\nforeground='#cacccc'\naccent='#e5a55b'");
    let accent = Theme::from_text("foreground='#e5a55b'").foreground;
    cx.update(|cx| cx.set_global(theme));
    let handle = cx.add_window(|_, cx| super::root::Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let painted = list.drawn.palette.expect("the list painted");
            assert_eq!(painted.accent, accent);
            assert_ne!(painted.accent, painted.foreground);
        })
        .unwrap();
}

#[gpui::test]
fn u01_presentation_root_gives_unstyled_children_the_live_face_and_ink(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let theme = Theme::from_text("foreground='#cacccc'");
    cx.update(|cx| cx.set_global(theme));
    let style = Rc::new(RefCell::new(None));
    let handle = cx.add_window(|_, cx| super::root::Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.slots.rail = Some(cx.new(|_| StyleProbe(style.clone())).into());
            shell
        }),
    });
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let style = style.borrow().clone().expect("the child rendered");
    handle
        .update(cx, |root, window, cx| {
            let shell = root.shell.read(cx);
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            assert_eq!(style.font_family, list.typeface.font.family);
            assert_eq!(
                style.font_size.to_pixels(window.rem_size()),
                list.typeface.body
            );
            assert_eq!(style.color, theme.foreground);
        })
        .unwrap();
}

#[gpui::test]
fn u01_presentation_live_snapshot_changes_face_ink_and_scroll_in_one_draw(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let style = Rc::new(RefCell::new(None));
    let handle = cx.add_window(|_, cx| super::root::Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.slots.rail = Some(cx.new(|_| StyleProbe(style.clone())).into());
            shell
        }),
    });
    for (body, foreground, accent) in [(12, "#cacccc", "#e5a55b"), (16, "#123456", "#654321")] {
        handle
            .update(cx, |root, window, cx| {
                root.shell.update(cx, |shell, cx| {
                    let (theme, parsed) = Theme::from_text_observed(&format!(
                        "foreground='{foreground}'\naccent='{accent}'"
                    ));
                    let fonts = crate::presentation::Fonts::from_sources(
                        &format!("[font]\nbase-size={body}"),
                        "",
                        shell.active_typeface().font.family.to_string(),
                    );
                    shell.apply_presentation(
                        crate::presentation::Snapshot {
                            theme,
                            parsed,
                            fonts,
                        },
                        window,
                        cx,
                    );
                });
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        handle
            .update(cx, |root, window, cx| {
                root.shell.update(cx, |shell, _| {
                    let host = shell.panes[0].as_mut().unwrap();
                    let list = host.list.borrow();
                    let style = style.borrow().clone().unwrap();
                    assert_eq!(f32::from(list.typeface.body), body as f32);
                    assert_eq!(style.font_family, list.typeface.font.family);
                    assert_eq!(
                        style.font_size.to_pixels(window.rem_size()),
                        list.typeface.body
                    );
                    assert_eq!(
                        style.color,
                        Theme::from_text(&format!("foreground='{foreground}'")).foreground
                    );
                    assert_eq!(list.drawn.palette.unwrap().foreground, style.color);
                    assert_eq!(
                        list.drawn.palette.unwrap().accent,
                        Theme::from_text(&format!("foreground='{accent}'")).foreground
                    );
                    if body == 16 {
                        assert_eq!(list.drawn.scroll_y, 100. * list.drawn.metrics.row);
                        assert_eq!(list.drawn.loading, 0);
                    }
                    host.pane.cursor = 100;
                    host.pane.scroll_to(100. * list.drawn.metrics.row);
                });
            })
            .unwrap();
    }
}

#[test]
fn u01_presentation_early_relist_commit_anchors_nonzero_rows_at_the_live_height() {
    use crate::pane::{swap::Options, Pane};
    use flea::protocol::{Reply, RowFacts};
    for body in [12., 16.] {
        let row = crate::list::Metrics::at(gpui::px(body)).row;
        let mut pane = Pane::at_row_height("/fixture".into(), 620., row);
        let count = pane.first_count();
        let header = |pane: &mut Pane| {
            pane.receive_at(
                Reply::Listed {
                    path: "/fixture".into(),
                    total: 1000,
                },
                1,
            )
            .unwrap();
        };
        let answer = |pane: &mut Pane, start, listing| {
            assert!(pane
                .receive_at(
                    Reply::Rows {
                        start,
                        rows: vec![RowFacts::default(); count],
                        listing
                    },
                    2
                )
                .unwrap());
        };
        header(&mut pane);
        answer(&mut pane, 0, 1);
        pane.scroll_to(600. * row);
        assert!(pane.begin("/fixture".into(), Options::default(), 1000));
        header(&mut pane);
        answer(&mut pane, 450, 2);
        header(&mut pane);
        answer(&mut pane, 450, 3);
        assert_eq!((pane.cursor, pane.scroll_y), (450, 450. * row));
        assert_eq!(pane.requests.initial_first, Some(450));
        assert!(pane
            .held
            .row((pane.scroll_y / row).floor() as usize)
            .is_some());
    }
}

// Hyprland sends no event for a live glass edit, so the window is the tagger's only way to hear it (THEME-GLASS D2).
#[gpui::test]
fn theme_glass_a_new_presentation_hands_its_glass_inputs_to_the_tagger(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| super::root::Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    let (mut theme, parsed) = Theme::from_text_observed("foreground='#123456'");
    theme.glass = super::glass::Inputs {
        automatic: false,
        wanted_alpha: 153,
        ..Default::default()
    };
    super::glass_tag::events::HANDED.with(|handed| handed.set(None));
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                let fonts = crate::presentation::Fonts::from_sources(
                    "",
                    "",
                    shell.active_typeface().font.family.to_string(),
                );
                shell.apply_presentation(
                    crate::presentation::Snapshot {
                        theme,
                        parsed,
                        fonts,
                    },
                    window,
                    cx,
                );
            });
        })
        .unwrap();
    assert_eq!(
        super::glass_tag::events::HANDED.with(std::cell::Cell::get),
        Some(theme.glass)
    );
}

//! The first mounted paint after text shrink must use the current row extent.
use crate::{
    backend::Backend,
    list::{bench, pane::Held, Density},
    presentation::{Fonts, Snapshot},
    state::tests::shell_with,
    window::{pane::PaneHost, root::Root, startup::View},
};
use flea::protocol::Reply;
use gpui::{px, size, AppContext, TestAppContext, WindowHandle};
use std::rc::Rc;

struct Painted {
    frame: crate::input::Frame,
    list_scroll: f32,
    pane_scroll: f32,
    row: f32,
    header: f32,
    frames: u64,
    extent: Option<f32>,
    requests: usize,
    held: Rc<Held>,
}

fn open(cx: &mut TestAppContext, view: View, density: &str) -> WindowHandle<Root> {
    let (backend, _) = Backend::test_channel();
    cx.open_window(size(px(1100.), px(700.)), |_, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.view = view;
            let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
            right
                .pane
                .receive(Reply::Listed {
                    path: "/right".into(),
                    total: 100,
                })
                .unwrap();
            right
                .pane
                .receive(Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: bench::fixture("2k").into_iter().take(100).collect(),
                })
                .unwrap();
            shell.panes[1] = Some(right);
            shell.lifecycle.attach(1);
            for host in shell.panes.iter_mut().flatten() {
                host.backend = None;
                host.list.borrow_mut().density = Density::parse(density);
                let rows = bench::fixture("2k").into_iter().take(100).collect();
                host.pane.held = Rc::new(Held {
                    listing: host.pane.held.listing,
                    total: 100,
                    start: 0,
                    rows,
                });
                assert!(!host.pane.busy(), "the mounted fixture must be ready");
            }
            shell
        }),
    })
}

fn capture(window: &gpui::Window, cx: &gpui::App) -> Vec<Painted> {
    let root = window.root::<Root>().flatten().expect("mounted Root");
    let shell = root.read(cx).shell.read(cx);
    assert!(
        shell.seam.viewport().is_none(),
        "use actual window geometry"
    );
    let count = if shell.view == View::Dual { 2 } else { 1 };
    (0..count)
        .map(|side| {
            let host = shell.panes[side].as_ref().unwrap();
            assert!(host.input.band.is_none());
            let list = host.list.borrow();
            let frame = host.input.drawn.expect("mounted input painted").frame;
            assert_eq!(frame.bounds, list.drawn.bounds);
            Painted {
                frame,
                list_scroll: list.drawn.scroll_y,
                pane_scroll: host.pane.scroll_y,
                row: list.drawn.metrics.row,
                header: list.drawn.metrics.header,
                frames: list.drawn.frames,
                extent: host.pane.scroll_extent,
                requests: host.pane.list_requests,
                held: Rc::clone(&host.pane.held),
            }
        })
        .collect()
}

fn paint(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> Vec<Painted> {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        capture(window, cx)
    })
    .unwrap()
}

#[test]
fn g70_header_shrink_first_mounted_frame_clamps_current_extent() {
    for view in [View::List, View::Dual] {
        for density in ["compact", "normal", "tight"] {
            for bottom in [true, false] {
                let mut cx = bench::real_cx();
                cx.update(|cx| {
                    cx.set_global(Fonts::from_sources(
                        "[font]\nbase-size=20",
                        "",
                        "JetBrains Mono".into(),
                    ))
                });
                let handle = open(&mut cx, view, density);
                let initial = paint(&mut cx, handle);
                handle
                    .update(&mut cx, |root, _, cx| {
                        root.shell.update(cx, |shell, cx| {
                            for host in shell.panes.iter_mut().flatten() {
                                let y = if bottom {
                                    f32::MAX
                                } else {
                                    20.5 * host.pane.row_height()
                                };
                                host.pane.scroll_to(y);
                            }
                            cx.notify();
                        });
                    })
                    .unwrap();
                let before = paint(&mut cx, handle);
                for (first, old) in initial.iter().zip(&before) {
                    assert_eq!(old.header, 37.);
                    assert_eq!(old.extent, Some(old.frame.content));
                    assert_eq!(old.pane_scroll, old.list_scroll);
                    assert!(old.frame.viewport() > 0.);
                    assert!(Rc::ptr_eq(&first.held, &old.held));
                    if bottom {
                        assert!((old.list_scroll - old.frame.max_scroll()).abs() < 0.01);
                    }
                }

                // Capture inside this draw, before the outer test executor can service another frame.
                let after = cx
                    .update_window(handle.into(), |_, window, cx| {
                        let root = window.root::<Root>().flatten().unwrap();
                        root.update(cx, |root, cx| {
                            root.shell.update(cx, |shell, cx| {
                                let family = shell.active_typeface().font.family.to_string();
                                let (theme, parsed) = crate::palette::Theme::from_text_observed(
                                    "foreground='#cacccc'",
                                );
                                shell.apply_presentation(
                                    Snapshot {
                                        fonts: Fonts::from_sources(
                                            "[font]\nbase-size=14",
                                            "",
                                            family,
                                        ),
                                        theme,
                                        parsed,
                                    },
                                    window,
                                    cx,
                                );
                            });
                        });
                        window.refresh();
                        window.draw(cx).clear(cx);
                        capture(window, cx)
                    })
                    .unwrap();

                for (side, (old, new)) in before.iter().zip(&after).enumerate() {
                    assert!(new.frames > old.frames, "a fresh ListElement painted");
                    assert_eq!(new.header, 27.);
                    assert_eq!(new.extent, Some(new.frame.content));
                    assert_eq!(new.frame.content, 100. * new.row + new.header);
                    assert_eq!(new.pane_scroll, new.list_scroll);
                    assert_eq!(new.frame.scroll, new.list_scroll);
                    assert_eq!(new.requests, old.requests, "font changes must not relist");
                    assert!(Rc::ptr_eq(&old.held, &new.held));
                    let expected_row = match density {
                        "compact" => 31.,
                        "normal" => 37.,
                        "tight" => 23.,
                        _ => unreachable!(),
                    };
                    assert_eq!(new.row, expected_row);
                    if bottom {
                        assert!(
                            (new.list_scroll - new.frame.max_scroll()).abs() < 0.01,
                            "G70_HEADER_SHRINK_FIRST_PAINT: view={view:?} density={density}                              side={side} scroll={} max={} viewport={} header={} old_header={}",
                            new.list_scroll, new.frame.max_scroll(), new.frame.viewport(),
                            new.header, old.header
                        );
                    } else {
                        assert!(
                            (new.list_scroll / new.row - old.list_scroll / old.row).abs() < 0.001,
                            "a valid fractional row anchor must survive text shrink"
                        );
                    }
                }
            }
        }
    }
}

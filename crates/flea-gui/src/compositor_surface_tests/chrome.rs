use super::{frames, mounted, painted, update};
use crate::{
    g2::{context_menus, rail_places, trash_browser},
    list::{Palette, Typeface},
    path,
};
use flea::{
    ui_logic_context_menus::{fit::Rect, Entry, Text},
    ui_logic_rail_places::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry},
    },
};
use gpui::{
    div, prelude::*, px, AppContext, Context, Render, TestAppContext, VisualTestContext, Window,
};
use std::{sync::Arc, time::Duration};

struct Menu(gpui::Entity<crate::state::Shell>, bool);
impl Render for Menu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layer = if self.1 {
            self.0.update(cx, |shell, cx| shell.trash_menu_layer(cx))
        } else {
            context_menus::host_input::layer(&self.0, window, cx).map(IntoElement::into_any_element)
        };
        div().size_full().children(layer)
    }
}

// A menu opens on Reveal.qml's fade from clear (EY-555), so its frames are judged once the fade rests.
fn settle_open_fade(vcx: &mut VisualTestContext) {
    painted(vcx);
    vcx.executor().advance_clock(Duration::from_millis(
        flea::ui_logic_status_feedback::OPEN_MS,
    ));
}

pub(super) fn check(cx: &mut TestAppContext, corner: f32) {
    let (shell, view, vcx) = mounted(cx, corner, |shell, window, cx| {
        // The shell fixture and boards use text size 14, so its standalone toolbar must use the same rem.
        window.set_rem_size(px(14.));
        let path = cx.new(|cx| {
            let mut path = path::view::PathView::new(
                Arc::default(),
                Box::new(path::suggestions::Unranked),
                cx,
            );
            path.inputs.paths[0] = "/files".into();
            path
        });
        let view = cx.new(|cx| {
            path::chrome::ChromeView::new(
                path,
                path::chrome::Inputs {
                    view: crate::window::startup::View::List,
                    back: true,
                    up: true,
                    filter: true,
                    sort: true,
                    live: true,
                    corner: shell.corner,
                },
                cx,
            )
        });
        shell.chrome.view = Some(view.clone());
        view
    });
    vcx.update(|window, cx| view.update(cx, |view, cx| window.focus(&view.focus[0], cx)));
    // The focus ring is a control frame that follows the text size, so it keeps the half corner at every size.
    let ring =
        vcx.update(|window, cx| path::chrome::Sizes::at(&Typeface::for_window(window, cx)).ring);
    frames("chrome-focus-ring", &shell, vcx, corner, &[ring]);
    vcx.update(|window, cx| {
        let path = view.read(cx).path.clone();
        path.update(cx, |view, cx| {
            view.open(window, cx);
            view.jump.entries = vec![path::suggestions::Suggestion {
                path: "/files/child".into(),
                text: "child".into(),
                ..Default::default()
            }];
            cx.notify();
        });
    });
    frames("path-entry-and-jump-popup", &shell, vcx, corner, &[]);
    let sections = vec![RailSection {
        heading: Heading::None,
        rows: vec![RailEntry {
            group: Group::Network,
            kind: Kind::Share,
            label: "Share".into(),
            uri: "smb://test/share".into(),
            glyph: "folder",
            ..Default::default()
        }],
    }];
    let (shell, view, vcx) = mounted(cx, corner, |shell, window, cx| {
        shell.mount_rail(cx);
        let view = cx.new(|cx| {
            rail_places::producer::Producer::from_sections(
                Arc::default(),
                sections,
                "/missing/flea".into(),
                cx,
            )
        });
        shell.rail.as_mut().unwrap().places = view.clone();
        // Shell render syncs the rail before its first paint; run that production step here.
        shell.sync_rail(window, cx);
        view
    });
    view.update(vcx, |view, cx| {
        assert!(view.start_rename(0, cx));
    });
    frames("rail-rename-field", &shell, vcx, corner, &[]);
    let (shell, _, vcx) = mounted(cx, corner, |shell, window, cx| {
        let menu = shell.menu.menu.clone();
        let catalog = crate::test_catalogs::english();
        let typeface = Typeface::omarchy_default();
        let env = context_menus::lines::Env {
            catalog: &catalog,
            preset: "default",
            key_hints: false,
            typeface: &typeface,
            inset: 4.,
        };
        menu.borrow_mut().open(
            context_menus::menu::Face::File,
            vec![Entry {
                submenu: Some(vec![Entry::leaf("copy", Text::Data("Copy".into()))]),
                ..Entry::row("copyAs", Text::Data("Copy As".into()), "file")
            }],
            (100., 100.),
            Rect {
                x: 0.,
                y: 0.,
                w: 1100.,
                h: 700.,
            },
            "file",
            &env,
            window,
        );
        menu.borrow_mut()
            .open_submenu_for("copyAs", true, &env, window);
        assert!(menu.borrow().flyout.is_some());
        let shell = cx.entity();
        cx.new(|_| Menu(shell, false))
    });
    settle_open_fade(vcx);
    frames("context-menu-and-flyout", &shell, vcx, corner, &[]);
    let (shell, _, vcx) = mounted(cx, corner, |shell, window, cx| {
        window.set_rem_size(px(14.));
        shell.mount_trash(window, cx);
        let view = shell.trash.as_ref().unwrap().pane.clone();
        view.update(cx, |view, cx| {
            view.monitor_start = None;
            view.resize(1100., 700., cx);
        });
        view
    });
    frames("trash-strip-button", &shell, vcx, corner, &[]);
    let (shell, _, vcx) = mounted(cx, corner, |shell, window, cx| {
        shell.mount_trash(window, cx);
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, cx| {
            pane.monitor_start = None;
            pane.host.view.opened = true;
            pane.outside.menu(
                vec![flea::ui_logic_trash_browser::host::Item::Entry(
                    flea::ui_logic_trash_browser::host::Entry {
                        id: "restore",
                        action: "restore",
                        label_key: "trash.action.restore",
                        glyph: "folder",
                        disabled: false,
                        danger: false,
                    },
                )],
                trash_browser::seams::At { x: 100., y: 100. },
            );
            cx.notify();
        });
        let shell = cx.entity();
        cx.new(|_| Menu(shell, true))
    });
    settle_open_fade(vcx);
    frames("trash-context-menu", &shell, vcx, corner, &[]);
    scrollbar(cx, corner);
}

struct Scroller {
    shell: gpui::Entity<crate::state::Shell>,
    height: f32,
}
impl Render for Scroller {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::inherited(&window.text_style(), super::theme());
        div()
            .relative()
            .w(px(300.))
            .h(px(self.height))
            .child(crate::input::surface_with(
                self.shell.clone(),
                0,
                move |shell, bounds| {
                    let host = shell.panes[0].as_ref()?;
                    Some((
                        crate::input::Frame::list(
                            bounds,
                            &host.list.borrow().drawn.metrics,
                            &host.pane,
                            host.list.borrow().drawn.grow,
                        ),
                        palette,
                    ))
                },
            ))
    }
}

fn scrollbar(cx: &mut TestAppContext, corner: f32) {
    let (shell, view, vcx) = mounted(cx, corner, |shell, _, cx| {
        shell.panes[0]
            .as_mut()
            .unwrap()
            .input
            .moved(cx.background_executor().now());
        let shell = cx.entity();
        cx.new(|_| Scroller {
            shell,
            height: 600.,
        })
    });
    let mut mount = true;
    for height in [600., 4.] {
        view.update(vcx, |view, cx| {
            view.height = height;
            cx.notify();
        });
        for radius in [corner, 8. - corner, corner] {
            // The first paint uses the corner the thumb took at mount.
            if !std::mem::take(&mut mount) {
                update(&shell, vcx, radius);
            }
            let (scale, quads) = painted(vcx);
            let thumbs: Vec<_> = quads
                .iter()
                .filter(|quad| {
                    let width = quad.bounds.size.width.0 / scale;
                    (width - 6.).abs() < 0.1
                        && quad.bounds.size.height.0 / scale > 0.
                        && quad
                            .background
                            .as_solid()
                            .is_some_and(|ink| (ink.a - 0.5).abs() < 0.01)
                })
                .collect();
            assert!(
                !thumbs.is_empty(),
                "the scrolling listing must paint a thumb"
            );
            for thumb in thumbs {
                let expected = radius
                    .min(thumb.bounds.size.width.0.min(thumb.bounds.size.height.0) / scale / 2.);
                assert_eq!(thumb.corner_radii.top_left.0 / scale, expected);
            }
            println!("M9_CORNER surface=scrollbar corner={corner} track_height={height} live_radius={radius} PASS");
        }
    }
}

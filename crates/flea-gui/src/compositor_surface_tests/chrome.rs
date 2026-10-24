use super::{frames, painted};
use crate::{
    g2::{mn1, pl3, pl4},
    list::{Palette, Typeface},
    path,
};
use flea::{
    ui_logic_mn1::{fit::Rect, Entry, Text},
    ui_logic_pl4::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry},
    },
};
use gpui::{div, prelude::*, px, AppContext, Context, Render, TestAppContext, Window};
use std::{cell::RefCell, rc::Rc, sync::Arc};

struct Menu(Rc<RefCell<mn1::menu::Menu>>, f32);
impl Render for Menu {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(mn1::element::MenuElement::new(self.0.clone(), px(self.1)))
    }
}

struct Outside;
impl pl3::seams::Outside for Outside {
    fn send(&mut self, _: String) {}
    fn card(&mut self, _: Option<pl3::seams::Card>) {}
    fn menu(&mut self, _: Vec<flea::ui_logic_pl3::host::Item>, _: pl3::seams::At) {}
    fn status(&mut self, _: String, _: bool) {}
    fn result(&mut self, _: String, _: String, _: bool) {}
    fn count(&mut self, _: usize) {}
    fn focus_rail(&mut self) {}
    fn back(&mut self) {}
    fn action(&mut self, _: String) {}
    fn escape_taken(&mut self) -> bool {
        false
    }
}

pub(super) fn check(cx: &mut TestAppContext, corner: f32) {
    let (view, vcx) = cx.add_window_view(|_, cx| {
        let path = cx.new(|cx| {
            let mut path = path::view::PathView::new(
                Arc::default(),
                Box::new(path::suggestions::Unranked),
                cx,
            );
            path.corner = corner;
            path.inputs.paths[0] = "/files".into();
            path
        });
        path::chrome::ChromeView::new(
            path,
            path::chrome::Inputs {
                view: crate::window::startup::View::List,
                back: true,
                up: true,
                filter: true,
                sort: true,
                live: true,
                corner,
            },
            cx,
        )
    });
    vcx.update(|window, cx| view.update(cx, |view, cx| window.focus(&view.focus[0], cx)));
    frames("chrome-focus-ring", vcx, corner, &[]);
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
    frames("path-entry-and-jump-popup", vcx, corner, &[]);
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
    let (view, vcx) = cx.add_window_view(|_, cx| {
        pl4::producer::Producer::from_sections(Arc::default(), sections, "/missing/flea".into(), cx)
    });
    view.update(vcx, |view, cx| {
        view.model.borrow_mut().corner = corner;
        assert!(view.start_rename(0, cx));
    });
    frames("rail-rename-field", vcx, corner, &[]);
    let menu = Rc::new(RefCell::new(mn1::menu::Menu::default()));
    let (_, vcx) = cx.add_window_view(|window, _| {
        let catalog = flea::i18n::I18n::default();
        let typeface = Typeface::omarchy_default();
        let env = mn1::lines::Env {
            catalog: &catalog,
            preset: "default",
            key_hints: false,
            typeface: &typeface,
            inset: 4.,
        };
        menu.borrow_mut().open(
            mn1::menu::Face::File,
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
        Menu(menu.clone(), corner)
    });
    frames("context-menu-and-flyout", vcx, corner, &[]);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let palette = Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        let mut view = pl3::pane::TrashPane::new(
            Arc::default(),
            Box::new(Outside),
            pl3::ink::Ink::interim(&palette, px(corner)),
            "default",
            "/files".into(),
            cx,
        );
        view.monitor_start = None;
        view.resize(1100., 700., cx);
        view
    });
    frames("trash-strip-button", vcx, corner, &[]);
    scrollbar(cx, corner);
}

struct Scroller {
    shell: gpui::Entity<crate::state::Shell>,
    height: f32,
}
impl Render for Scroller {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::inherited(&window.text_style(), crate::palette::Theme::default());
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
                        ),
                        palette,
                    ))
                },
            ))
    }
}

fn scrollbar(cx: &mut TestAppContext, corner: f32) {
    let (backend, _requests) = crate::backend::Backend::test_channel();
    let (view, vcx) = cx.add_window_view(|_, cx| {
        let shell = cx.new(|cx| {
            let mut shell = crate::state::tests::shell_with(backend, cx);
            shell.corner = corner;
            shell.panes[0]
                .as_mut()
                .unwrap()
                .input
                .moved(cx.background_executor().now());
            shell
        });
        Scroller {
            shell,
            height: 600.,
        }
    });
    for height in [600., 4.] {
        view.update(vcx, |view, cx| {
            view.height = height;
            cx.notify();
        });
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
            let expected =
                corner.min(thumb.bounds.size.width.0.min(thumb.bounds.size.height.0) / scale / 2.);
            assert_eq!(thumb.corner_radii.top_left.0 / scale, expected);
        }
        println!("M9_CORNER surface=scrollbar corner={corner} track_height={height} PASS");
    }
}

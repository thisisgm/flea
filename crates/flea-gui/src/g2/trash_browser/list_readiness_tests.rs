//! Draw readiness follows actual window and parent clipping while retaining one list snapshot.
use super::join_tests::{call, draw, opened, root, shell, state};
use super::list::{Line, ListPaint, TrashList};
use crate::{list::bench::real_cx, row::marks::Mark};
use flea::jsondoc::Json;
use gpui::{div, point, prelude::*, px, size, Bounds, Context, Entity, Pixels, Render, Window};
use std::{cell::RefCell, rc::Rc};

struct ClipFrame {
    paint: Rc<RefCell<ListPaint>>,
    parent: Bounds<Pixels>,
    child: Bounds<Pixels>,
}

impl Render for ClipFrame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .absolute()
                .left(self.parent.origin.x)
                .top(self.parent.origin.y)
                .w(self.parent.size.width)
                .h(self.parent.size.height)
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .left(self.child.origin.x)
                        .top(self.child.origin.y)
                        .w(self.child.size.width)
                        .h(self.child.size.height)
                        .child(TrashList(self.paint.clone())),
                ),
        )
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

#[test]
fn u32_list_prepaint_rejects_empty_off_window_and_parent_clipped_bounds() {
    let mut cx = real_cx();
    let typeface = crate::list::Typeface::omarchy_default();
    let board = super::board::Board::at(&typeface);
    let mut initial = ListPaint::new(typeface.font, board, super::tests::ink());
    initial.total = 1;
    initial.committed = 1;
    initial.lines.push(Line {
        name: "alpha".into(),
        location: "/fixture".into(),
        deleted: "today".into(),
        mark: Mark::Folder,
        marked: false,
        thumb: None,
    });
    let paint = Rc::new(RefCell::new(initial));
    let window = cx.add_window(|_, _| ClipFrame {
        paint: paint.clone(),
        parent: rect(20., 30., 200., 200.),
        child: rect(0., 0., 120., 80.),
    });
    let width = window
        .update(&mut cx, |_, window, _| {
            f32::from(window.viewport_size().width)
        })
        .unwrap();
    for (label, parent, child, visible) in [
        (
            "visible positive bounds",
            rect(20., 30., 200., 200.),
            rect(0., 0., 120., 80.),
            true,
        ),
        (
            "zero width",
            rect(20., 30., 200., 200.),
            rect(0., 0., 0., 80.),
            false,
        ),
        (
            "zero height",
            rect(20., 30., 200., 200.),
            rect(0., 0., 120., 0.),
            false,
        ),
        (
            "off window right",
            rect(width + 10., 30., 200., 200.),
            rect(0., 0., 120., 80.),
            false,
        ),
        (
            "off window left",
            rect(-200., 30., 300., 200.),
            rect(0., 0., 120., 80.),
            false,
        ),
        (
            "viewport edge only",
            rect(width, 30., 200., 200.),
            rect(0., 0., 120., 80.),
            false,
        ),
        (
            "parent clips x",
            rect(20., 30., 80., 80.),
            rect(100., 0., 120., 80.),
            false,
        ),
        (
            "parent clips y",
            rect(20., 30., 80., 80.),
            rect(0., 100., 120., 80.),
            false,
        ),
        (
            "parent edge only",
            rect(20., 30., 80., 80.),
            rect(80., 0., 120., 80.),
            false,
        ),
        (
            "partial parent overlap",
            rect(20., 30., 80., 80.),
            rect(40., 0., 120., 80.),
            true,
        ),
        (
            "visible again",
            rect(20., 30., 200., 200.),
            rect(0., 0., 120., 80.),
            true,
        ),
    ] {
        window
            .update(&mut cx, |frame, _, cx| {
                frame.parent = parent;
                frame.child = child;
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let snapshot = paint.borrow();
        assert_eq!(snapshot.drawn(), visible, "{label}: {:?}", snapshot.bounds);
        assert_eq!(
            snapshot.laid_out,
            visible.then_some(snapshot.committed),
            "{label}"
        );
        assert_eq!(snapshot.committed, 1, "{label}");
        if child.size.width > px(0.) && child.size.height > px(0.) {
            assert!(snapshot.bounds.unwrap().size.width > px(0.), "{label}");
            assert!(snapshot.bounds.unwrap().size.height > px(0.), "{label}");
        }
        if !visible {
            assert!(snapshot.painted.is_empty(), "{label}");
        }
        drop(snapshot);
        window
            .update(&mut cx, |frame, _, _| {
                assert!(Rc::ptr_eq(&paint, &frame.paint))
            })
            .unwrap();
    }
    {
        let mut snapshot = paint.borrow_mut();
        snapshot.committed += 1;
        assert!(!snapshot.drawn(), "a new generation waits for prepaint");
    }
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    assert!(paint.borrow().drawn());
    paint.borrow_mut().invalidate_surface();
    assert_eq!(
        paint.borrow().laid_out,
        None,
        "removal drops its generation"
    );
    assert!(!paint.borrow().drawn());
}

struct ClipPane {
    pane: Entity<super::pane::TrashPane>,
    width: f32,
    clip_width: f32,
    clip_height: f32,
}

impl Render for ClipPane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .w(px(self.clip_width))
                .h(px(self.clip_height))
                .overflow_hidden()
                .child(div().w(px(self.width)).h_full().child(self.pane.clone())),
        )
    }
}

#[test]
fn u32_partial_parent_clip_keeps_the_live_ipc_centre_inside_the_visible_list() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    for (clip_width, clip_height) in [(40., 900.), (1., 900.), (40., 70.)] {
        shell(&mut cx, window, |shell, _, cx| {
            let pane = shell.trash.as_ref().unwrap().pane.clone();
            let width = pane.read(cx).width;
            let clipped = cx.new(|_| ClipPane {
                pane,
                width,
                clip_width,
                clip_height,
            });
            shell.slots.covers[0] = Some(clipped.into());
            cx.notify();
        });
        draw(&mut cx, window);
        let centre = call(&mut cx, window, "trashRowCentre", &["0"]);
        let x = centre
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<f32>()
            .unwrap();
        shell(&mut cx, window, |shell, _, cx| {
            let paint = shell.trash.as_ref().unwrap().pane.read(cx).paint.borrow();
            let bounds = paint.bounds.unwrap();
            assert!(bounds.size.width > px(40.));
            assert!(x >= f32::from(bounds.origin.x));
            assert!(x < f32::from(bounds.origin.x) + clip_width, "{centre}");
            assert_eq!(paint.visible_bounds.unwrap().size.width, px(clip_width));
            let y = centre
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<f32>()
                .unwrap();
            let visible = paint.visible_bounds.unwrap();
            assert!(y >= f32::from(visible.origin.y));
            assert!(y < f32::from(visible.bottom()), "{centre}");
        });
        assert_eq!(state(&mut cx, window).get("busy"), Some(&Json::Bool(false)));
    }
}

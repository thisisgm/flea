//! Modal paint order and keyboard ownership through the composed window.
use crate::{
    backend::Backend,
    g2::op1::{
        join::OperationUpdate,
        model::{CopyFocus, Dialog, Said},
        press::Press,
    },
    palette::Theme,
    state::{tests::shell_with, Shell},
    window::{input::ActionEvent, root::Root},
};
use gpui::{
    deferred, div, prelude::*, px, AppContext, Bounds, Context, Entity, Hsla, Pixels, Quad,
    TestAppContext, VisualTestContext, Window, WindowHandle,
};
use std::{cell::RefCell, rc::Rc};

struct WithMenu {
    root: Entity<Root>,
    shell: Entity<Shell>,
    menu: bool,
}

impl Render for WithMenu {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let size = window.viewport_size();
        let menu = self.menu.then(|| {
            deferred(
                div()
                    .debug_selector(|| "modal-test-menu".into())
                    .absolute()
                    .left(size.width / 2. - px(50.))
                    .top(size.height / 2. - px(40.))
                    .w(px(100.))
                    .h(px(80.))
                    .bg(Theme::default().foreground),
            )
            .with_priority(1)
        });
        div()
            .size_full()
            .relative()
            .child(self.root.clone())
            .children(menu)
    }
}

fn open(cx: &mut TestAppContext, backend: Backend, menu: bool) -> WindowHandle<WithMenu> {
    cx.add_window(|window, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = 1;
            host.selected = vec![1, 3];
            window.focus(&shell.focus, cx);
        });
        let root = cx.new(|_| Root {
            shell: shell.clone(),
        });
        WithMenu { root, shell, menu }
    })
}

fn shell<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<WithMenu>,
    update: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| update(shell, window, cx))
        })
        .unwrap()
}

fn frame(cx: &mut TestAppContext, handle: WindowHandle<WithMenu>) -> Vec<Quad> {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.painted_quads()
    })
    .unwrap()
}

fn bounds(
    cx: &mut TestAppContext,
    handle: WindowHandle<WithMenu>,
    tag: &'static str,
) -> Bounds<Pixels> {
    VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds(tag)
        .expect(tag)
}

fn order(quads: &[Quad], bounds: Bounds<Pixels>, ink: Hsla, scale: f32) -> u32 {
    quads
        .iter()
        .filter(|quad| {
            quad.bounds == bounds.scale(scale)
                && quad.background.as_solid().is_some_and(|fill| {
                    (fill.h, fill.s, fill.l, fill.a) == (ink.h, ink.s, ink.l, ink.a)
                })
        })
        .map(|quad| quad.order)
        .min()
        .expect("the frame paints this fill")
}

#[gpui::test]
fn a_modal_paints_after_a_deferred_menu(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = open(cx, backend, true);
    frame(cx, handle);
    let (palette, scale) = shell(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
        (
            shell.overlay.as_ref().unwrap().read(cx).palette,
            window.scale_factor(),
        )
    });
    let quads = frame(cx, handle);
    let menu = bounds(cx, handle, "modal-test-menu");
    let card = bounds(cx, handle, "op1-card");
    let scrim = bounds(cx, handle, "op1-scrim");
    assert!(menu.intersect(&card).size.width > px(0.));
    let menu_order = order(&quads, menu, Theme::default().foreground, scale);
    let scrim_order = order(
        &quads,
        scrim,
        palette
            .background
            .opacity(flea::ui_logic_op1::card::scrim_alpha(0)),
        scale,
    );
    let card_order = order(&quads, card, palette.surface, scale);
    assert!(menu_order < scrim_order && scrim_order < card_order,
        "the modal scrim and card must paint after the deferred menu: menu={menu_order}, scrim={scrim_order}, card={card_order}");
}

fn keys_stay_in_card(cx: &mut TestAppContext, steal_focus: bool) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend, false);
    frame(cx, handle);
    let entity = handle.update(cx, |root, _, _| root.shell.clone()).unwrap();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let seen = actions.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&entity, move |_, event: &ActionEvent, _| {
            seen.borrow_mut().push(event.action);
        })
    });
    let notices = Rc::new(RefCell::new(Vec::<Said>::new()));
    let published = notices.clone();
    let _updates = cx.update(|cx| {
        cx.subscribe(&entity, move |_, update: &OperationUpdate, _| {
            published.borrow_mut().extend(update.said.clone());
        })
    });
    let before = shell(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
        let host = shell.panes[0].as_ref().unwrap();
        (
            host.pane.cursor,
            host.selected.clone(),
            host.pane.held.clone(),
            host.path.clone(),
            shell.focus_side,
            host.pane.requests.sent,
        )
    });
    frame(cx, handle);
    while requests().is_some() {}
    // Typing replaces the opening selection; Delete at the end is inert, and at Home it erases.
    for (key, expected_path, submitted) in [
        ("j", "j", false),
        ("delete", "j", false),
        ("enter", "j", false),
        ("home", "j", false),
        ("delete", "", false),
        ("enter", "", false),
        ("/", "/", false),
        ("j", "/j", false),
        ("enter", "/j", true),
    ] {
        if steal_focus {
            shell(cx, handle, |shell, window, cx| {
                window.focus(&shell.focus, cx)
            });
        }
        VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(key);
        frame(cx, handle);
        let delivered: Vec<_> = std::iter::from_fn(&mut requests).collect();
        shell(cx, handle, |shell, window, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.pane.cursor, before.0,
                "{key} must not move the listing cursor"
            );
            assert_eq!(
                host.selected, before.1,
                "{key} must not change the listing selection"
            );
            assert!(
                Rc::ptr_eq(&host.pane.held, &before.2),
                "{key} must not change the held files"
            );
            assert_eq!(host.path, before.3, "{key} must not navigate the pane");
            assert_eq!(
                shell.focus_side, before.4,
                "{key} must not change the active pane"
            );
            assert!(
                actions.borrow().is_empty(),
                "{key} must not dispatch a listing action"
            );
            assert_eq!(
                host.pane.requests.sent, before.5,
                "{key} must not ask for another viewport window"
            );
            assert_eq!(
                delivered.len(),
                usize::from(submitted),
                "{key} sends only the submitted destination validation"
            );
            assert!(
                !shell.focus.is_focused(window),
                "the hidden listing must not hold focus"
            );
            let card = shell.overlay.as_ref().unwrap().read(cx);
            let handle = card
                .focus_handle()
                .expect("the open card holds a focus handle");
            assert!(
                handle.is_focused(window),
                "the card's own handle owns keyboard focus"
            );
            assert_eq!(window.focused(cx), Some(handle), "nothing else is focused");
            let Some(Dialog::CopyTo {
                path,
                focus,
                busy,
                error,
                ..
            }) = &card.model.dialog
            else {
                panic!("the Copy To card remains open after {key}");
            };
            assert_eq!(path, expected_path, "{key} must reach the path editor");
            assert_eq!(
                *focus,
                if submitted {
                    CopyFocus::Cancel
                } else {
                    CopyFocus::Field
                },
                "{key} must leave focus on the enabled card control"
            );
            assert_eq!(*busy, submitted, "only Return on a full path submits");
            if key == "enter" && expected_path == "j" {
                assert_eq!(*error, card.catalog.tr("jobs.copyto.relative"));
                assert!(notices.borrow().is_empty(), "the refusal stays on the card");
            }
            assert_eq!(
                delivered.len(),
                usize::from(submitted),
                "{key} reaches the card exactly when it acts"
            );
            assert!(
                card.model.outbox.is_empty(),
                "the composer drains each command"
            );
            if submitted {
                assert_eq!(
                    delivered[0],
                    r#"{"c":"menuaction","op":"validate","id":0,"action":"copyTo","dest":"/j"}"#,
                    "Return validates the edited destination exactly once"
                );
            }
        });
    }
}

#[gpui::test]
fn the_copy_field_owns_j_delete_and_return(cx: &mut TestAppContext) {
    keys_stay_in_card(cx, false);
}

#[gpui::test]
fn a_modal_guards_keys_even_if_the_listing_reclaims_focus(cx: &mut TestAppContext) {
    keys_stay_in_card(cx, true);
}

#[gpui::test]
fn a_closed_card_restores_focus_before_the_listing_paints(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = open(cx, backend, false);
    let own = frame(cx, handle);
    shell(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
    });
    frame(cx, handle);
    shell(cx, handle, |shell, _, cx| {
        shell.overlay.as_ref().unwrap().update(cx, |card, cx| {
            let catalog = card.catalog.clone();
            card.model.press(&catalog, Press::CopyCancel);
            cx.notify();
        });
    });
    let restored = frame(cx, handle);
    shell(cx, handle, |shell, window, _| {
        assert!(shell.focus.is_focused(window));
    });
    assert_eq!(
        format!("{restored:?}"),
        format!("{own:?}"),
        "the first closed frame restores the listing and its focus together"
    );
}

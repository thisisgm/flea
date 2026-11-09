//! The live List, Grid and Columns loading captions keep the shipped position and muted role.
use super::{
    spinner::Crawl,
    spinner_tests::{frame, load, open, shell},
};
use crate::{palette::Theme, window::startup::View};
use gpui::{prelude::*, px, size, Bounds, Hsla, Pixels, TestAppContext};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug)]
struct Caption {
    bounds: Bounds<Pixels>,
    slot: Bounds<Pixels>,
    ink: Hsla,
}

#[derive(Default)]
struct Captions(RefCell<Vec<Caption>>);
impl gpui::Global for Captions {}

/// Observe the caption's text ink and placement inside its real paint path.
pub(super) fn watch_caption(caption: gpui::Div, mark: Rc<Crawl>) -> gpui::Div {
    caption.relative().child(
        gpui::canvas(
            |bounds, _, _| bounds,
            move |_, bounds, window, cx| {
                if let Some(seen) = cx.try_global::<Captions>() {
                    seen.0.borrow_mut().push(Caption {
                        bounds,
                        slot: mark.slot(),
                        ink: window.text_style().color,
                    });
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full(),
    )
}

fn setup(cx: &mut TestAppContext) -> Theme {
    let resolved = Theme::default();
    let theme = Theme {
        muted: resolved.accent,
        metadata: resolved.background,
        ..resolved
    };
    assert_ne!(theme.muted, theme.metadata);
    cx.update(|cx| {
        cx.set_global(theme);
        cx.set_global(Captions::default());
    });
    theme
}

fn loading_caption(view: View, cx: &mut TestAppContext) {
    let theme = setup(cx);
    let (handle, _keep) = open(cx, false);
    cx.simulate_window_resize(handle.into(), size(px(1100.), px(700.)));
    shell(cx, handle, |shell, window, cx| {
        window.set_rem_size(px(14.));
        shell.view = view;
        cx.notify();
    });
    cx.update(|cx| cx.global::<Captions>().0.borrow_mut().clear());
    let mark = load(cx, handle);
    frame(cx, handle);
    assert!(
        mark.shown(),
        "the unanswered listing shows its loading mark"
    );
    shell(cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().drawn_view, Some(view));
    });
    assert_captions(view, theme, cx);
}

fn assert_captions(view: View, theme: Theme, cx: &mut TestAppContext) {
    let captions = cx.update(|cx| cx.global::<Captions>().0.borrow().clone());
    eprintln!("EY516 {view:?} painted captions: {captions:?}");
    assert!(!captions.is_empty(), "{view:?} must paint a caption");
    for caption in captions {
        assert!(caption.bounds.size.width > px(0.) && caption.bounds.size.height > px(0.));
        assert_eq!(
            caption.ink, theme.muted,
            "{view:?}: LoadingState.qml uses muted ink"
        );
        assert_eq!(caption.slot.size, size(px(66.), px(66.)));
        assert!(
            f32::from(caption.bounds.top() - caption.slot.bottom()).abs() < 1.,
            "{view:?}: caption moves up 9 px while the 66 px mark slot stays put"
        );
    }
}

#[test]
fn feedback_remainder_ey516_caption_lift_never_enters_the_mark_slot() {
    use flea::ui_logic_settings_pages::textsize::{body_small, STOPS};
    for base in STOPS {
        let lift = super::spinner::caption_lift(px(body_small(base) as f32));
        assert!(lift <= px(super::spinner::GAP), "text {base}: {lift:?}");
    }
    assert_eq!(super::spinner::caption_lift(px(13.)), px(9.));
}

#[gpui::test]
fn feedback_remainder_ey516_list_loading_caption_paints_muted(cx: &mut TestAppContext) {
    loading_caption(View::List, cx);
}

#[gpui::test]
fn feedback_remainder_ey516_grid_loading_caption_paints_muted(cx: &mut TestAppContext) {
    loading_caption(View::Grid, cx);
}

#[test]
fn feedback_remainder_ey516_columns_loading_caption_paints_muted() {
    use crate::window::columns_tests::host;
    let mut cx = host::context();
    let theme = setup(&mut cx);
    let (handle, _keep) = host::open_at(&mut cx, "/srv/a/b/c", 1100., 3);
    host::change(&mut cx, handle, host::settle);
    host::frame(&mut cx, handle);
    cx.update(|cx| cx.global::<Captions>().0.borrow_mut().clear());
    host::change(&mut cx, handle, |shell, _, cx| {
        assert_eq!(shell.shown_view(), View::Columns);
        let host = shell.panes[0].as_mut().unwrap();
        let now = host.pane.now();
        assert!(host
            .pane
            .begin("/srv/a/b/c/d01".into(), Default::default(), now));
        assert!(host.pane.tick(now + crate::pane::swap::CAP_MS));
        host.changed(&shell.locale);
        cx.notify();
    });
    host::frame(&mut cx, handle);
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        assert_eq!(
            shell.shown_view(),
            View::Columns,
            "entry fallback is not Columns proof"
        );
        let host = shell.panes[0].as_ref().unwrap();
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(crate::columns::geometry::Slot::Active)
            .unwrap()
            .borrow();
        let active = paint.drawn.as_ref().unwrap().bounds;
        assert!(host.pane.swap.mark.shown());
        assert!(active.contains(&host.pane.swap.mark.slot().center()));
    });
    assert_captions(View::Columns, theme, &mut cx);
}

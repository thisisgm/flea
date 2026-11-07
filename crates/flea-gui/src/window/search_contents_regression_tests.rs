//! G12 review regressions through the mounted search, painter and local opener.
use super::*;
use crate::g2::sr1::contents::BLOCK_H;
use gpui::{div, prelude::*, px, size, Context, IntoElement, Render, Window};
use std::cell::RefCell;

#[gpui::test]
fn g12_vfix_pending_generations_keep_the_drawn_match_lines(cx: &mut TestAppContext) {
    let (handle, _read) = contents_needle(cx);
    snapshot(cx, handle, 6, 16, "src/main.rs", true);
    for (gen, listing, text) in [(7, 17, "x"), (8, 18, "y")] {
        key(cx, handle, text, Some(text));
        reply(cx, handle, 0, &matches(gen, listing, "pending.rs", true));
        frame(cx, handle);
        edit(cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.held.listing, 16);
            let shown = shell
                .contents_blocks(0)
                .expect("drawn rows retain their lines");
            assert_eq!(shown.blocks[0].row.name, "main.rs");
            assert!(
                host.list.borrow().view.contents,
                "pending lines cannot shrink drawn blocks"
            );
            assert_eq!(host.list.borrow().file_metrics().row, BLOCK_H);
        });
    }
    snapshot(cx, handle, 8, 18, "replacement.rs", true);
    reply(cx, handle, 0, &matches(7, 17, "late.rs", false));
    edit(cx, handle, |shell, _, _| {
        let shown = shell.contents_blocks(0).unwrap();
        assert_eq!(
            (shown.listing, shown.blocks[0].row.name.as_str()),
            (18, "replacement.rs")
        );
        assert!(shell.panes[0]
            .as_ref()
            .unwrap()
            .search
            .owner
            .operable(18, 0));
    });
}

#[test]
fn g12_vfix_marked_blocks_paint_above_the_opaque_contents_ground() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _read) = contents_needle(&mut cx);
    edit(&mut cx, handle, |shell, _, _| {
        shell.view = crate::window::startup::View::List;
    });
    snapshot_n(&mut cx, handle, 6, 16, &paths("src/main.rs", 4), true);
    results(&mut cx, handle);
    for text in ["v", "j", "v", "j", "v"] {
        key(&mut cx, handle, text, Some(text));
    }
    let (host, held, rows) = edit(&mut cx, handle, |shell, window, cx| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, 1, 2]);
        let rows = shell.list_states(0, window, cx);
        let held = std::rc::Rc::new(shell.contents_blocks(0).unwrap());
        (shell.panes[0].take().unwrap(), held, rows)
    });
    // Isolate the production overlay so the underlying List cannot satisfy this assertion.
    let overlay = cx.open_window(size(px(900.), px(4. * BLOCK_H)), |_, _| Overlay {
        host,
        held,
        rows,
    });
    for (focused, hovered) in [
        (true, None),
        (true, Some(3)),
        (false, None),
        (false, Some(3)),
    ] {
        overlay
            .update(&mut cx, |view, _, _| {
                view.rows.focused = focused;
                view.rows.hovered = hovered;
            })
            .unwrap();
        cx.update_window(overlay.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let scale = window.scale_factor();
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            let palette = crate::list::Palette::inherited(&window.text_style(), theme);
            let quads = window.painted_quads();
            let last_ground = |index: usize| {
                let x = 860. * scale;
                let y = (index as f32 * BLOCK_H + 40.) * scale;
                quads
                    .iter()
                    .rfind(|quad| {
                        quad.bounds.left().as_f32() <= x
                            && quad.bounds.right().as_f32() > x
                            && quad.bounds.top().as_f32() <= y
                            && quad.bounds.bottom().as_f32() > y
                    })
                    .and_then(|quad| quad.background.as_solid())
            };
            let marked = if focused {
                palette.marked
            } else {
                palette.foreground.opacity(crate::list::TRACK)
            };
            for index in [0, 1] {
                assert_eq!(
                    last_ground(index),
                    Some(marked),
                    "marked block {index}, focused={focused}, hover={hovered:?}"
                );
            }
            assert_eq!(
                last_ground(2),
                Some(if focused {
                    palette.accent.opacity(crate::list::CURSOR_WASH)
                } else {
                    palette.foreground.opacity(crate::list::GREY_CURSOR)
                }),
                "cursor on a marked block draws only its rung"
            );
            assert_eq!(
                last_ground(3),
                Some(if hovered.is_some() {
                    palette.foreground.opacity(crate::list::HOVER)
                } else {
                    palette.background
                }),
                "unmarked block keeps its ground or hover wash"
            );
            let edges = quads
                .iter()
                .filter(|quad| {
                    quad.bounds.size.width.as_f32() == 2. * scale
                        && quad.bounds.size.height.as_f32() == BLOCK_H * scale
                })
                .count();
            assert_eq!(
                edges,
                usize::from(focused),
                "only the focused cursor draws an edge"
            );
        })
        .unwrap();
    }
}

struct Overlay {
    host: crate::window::pane::PaneHost,
    held: std::rc::Rc<crate::g2::sr1::contents::Held>,
    rows: crate::list::RowStates,
}

impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scroll = self.host.input.drawn(self.host.pane.scroll_y);
        div()
            .size_full()
            .children(crate::state::Shell::contents_element(
                &mut self.host,
                &flea::jsondoc::Json::Null,
                Some(std::rc::Rc::clone(&self.held)),
                self.rows.clone(),
                scroll,
                window,
                cx,
            ))
    }
}

thread_local! {
    static LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    LAUNCHED.with(|seen| seen.borrow_mut().push(argv.to_vec()));
    (Some(0), String::new())
}

#[gpui::test]
fn g12_vfix_local_opener_refuses_inoperable_and_missing_contents_identity(cx: &mut TestAppContext) {
    let (handle, _read) = contents_needle(cx);
    snapshot(cx, handle, 6, 16, "raw\u{fffd}.txt", false);
    edit(cx, handle, |shell, _, _| shell.opener[0].launch = record);
    LAUNCHED.with(|seen| seen.borrow_mut().clear());
    key(cx, handle, "enter", None);
    let explicit_open = |cx: &mut TestAppContext| {
        edit(cx, handle, |shell, window, cx| {
            shell.dispatch(
                crate::window::dispatch::Order::Action(crate::window::input::ActionEvent::bare(
                    "open",
                    flea::keys::Context::Listing,
                )),
                window,
                cx,
            );
        });
        cx.run_until_parked();
    };
    explicit_open(cx);
    LAUNCHED.with(|seen| {
        assert!(
            seen.borrow().is_empty(),
            "lossy display alias must never launch"
        )
    });
    edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_mut()
            .unwrap()
            .search
            .owner
            .matches
            .clear();
    });
    explicit_open(cx);
    LAUNCHED.with(|seen| {
        assert!(
            seen.borrow().is_empty(),
            "a contents placeholder without identity must never launch"
        )
    });
    reply(cx, handle, 0, &matches(6, 16, "raw\u{fffd}.txt", true));
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.held.listing, 16);
        assert!(host.search.owner.operable(16, host.pane.cursor));
    });
    explicit_open(cx);
    LAUNCHED.with(|seen| {
        let seen = seen.borrow();
        assert_eq!(
            seen.len(),
            1,
            "operable rows still use the production opener"
        );
        assert_eq!(seen[0][1], "--open");
    });
}

// EY-333: a contents failure for the newest gen stops the strip scanning and says why; an older gen's is dropped.
#[gpui::test]
fn ey333_contents_failure_settles_the_newest_gen_and_says_why(cx: &mut TestAppContext) {
    let (handle, _read) = contents_needle(cx);
    let failed = |gen: u64, why: &str| {
        format!(
            r#"{{"t":"error","where":"searchcontents","path":"/files","msg":"Contents search failed: {why}.", "gen":{gen}}}"#
        )
    };
    let total = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.held.total
    });
    let before = super::super::tests::call(cx, handle, "lastMessage");
    reply(cx, handle, 0, &failed(5, "an older gen"));
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let session = &shell.panes[0].as_ref().unwrap().search.owner.session;
        assert!(session.running, "an older gen's failure is dropped");
    });
    assert_eq!(
        super::super::tests::call(cx, handle, "lastMessage"),
        before,
        "an older gen's failure says nothing"
    );
    reply(cx, handle, 0, &failed(6, "ripgrep is unavailable"));
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let session = &host.search.owner.session;
        assert!(!session.running, "the strip stops scanning");
        assert_eq!((session.answered, session.total), (6, 0));
        assert_eq!(host.pane.state.kind, crate::pane::states::Kind::Ready);
        assert_eq!(host.pane.held.total, total, "the drawn rows stay");
    });
    assert_eq!(
        super::super::tests::call(cx, handle, "lastMessage"),
        flea::ipc::wire::Reply::value("Contents search failed: ripgrep is unavailable.")
    );
}

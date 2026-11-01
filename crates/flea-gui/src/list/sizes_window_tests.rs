//! A window answer on the held listing installs its own inline folder sizes, as a re-list's first answer does.
use super::*;
use crate::{backend::Backend, pane::requests::WindowRequest, state::tests::shell_with};
use flea::protocol::{Reply, RowFacts};
use gpui::{px, size, AppContext, TestAppContext};

const PATH: &str = "/fx/window";

fn row(name: &str, inline: Option<u64>) -> RowFacts {
    RowFacts {
        name: name.into(),
        dir: true,
        inline,
        ..RowFacts::default()
    }
}

#[gpui::test]
fn pb_rest_window_answer_inline_size_reaches_its_row(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.simulate_resize(size(px(640.), px(400.)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane = crate::pane::Pane::new(PATH.into(), 646.);
            host.pane
                .receive(Reply::Listed {
                    path: PATH.into(),
                    total: 500,
                })
                .unwrap();
            // The first answer is the pane's own first window, so the window check accepts it.
            let bound = host.pane.first_count();
            let first: Vec<RowFacts> = (0..bound).map(|at| row(&format!("d{at}"), None)).collect();
            host.pane
                .receive(Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: first,
                })
                .unwrap();
            host.changed(&shell.locale);
            let host = shell.panes[0].as_mut().unwrap();
            host.nav
                .after_reply(&mut host.pane, PATH, &mut host.selected);
            shell.settle_list_sizes(window, cx);
            // A scroll window on the same listing, adjacent to the held rows; its first row walked inline.
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.requests.outstanding = Some(WindowRequest {
                start: bound,
                count: 20,
                listing: 3,
            });
            let mut second: Vec<RowFacts> = (bound..bound + 20)
                .map(|at| row(&format!("d{at}"), None))
                .collect();
            second[0].inline = Some(96);
            host.pane
                .receive(Reply::Rows {
                    start: bound,
                    listing: 3,
                    rows: second,
                })
                .unwrap();
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.sizes.sizes.size_for(bound),
                Some(Size {
                    bytes: 96,
                    partial: false
                }),
                "a window answer's inline size lands on its absolute row"
            );
        })
    });
}

#[gpui::test]
fn ey391_a_relist_of_the_same_folder_walks_its_folders_again(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.simulate_resize(size(px(640.), px(400.)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane = crate::pane::Pane::new(PATH.into(), 646.);
            for listing in [3, 4] {
                let host = shell.panes[0].as_mut().unwrap();
                host.pane
                    .receive(Reply::Listed {
                        path: PATH.into(),
                        total: 2,
                    })
                    .unwrap();
                host.pane
                    .receive(Reply::Rows {
                        start: 0,
                        listing,
                        rows: vec![row("empty", None), row("full", None)],
                    })
                    .unwrap();
                host.changed(&shell.locale);
                let host = shell.panes[0].as_mut().unwrap();
                host.nav
                    .after_reply(&mut host.pane, PATH, &mut host.selected);
                shell.settle_list_sizes(window, cx);
                let host = shell.panes[0].as_mut().unwrap();
                assert_eq!(
                    host.sizes.held.listing, listing,
                    "the sizes follow the listing"
                );
                if listing == 3 {
                    // The walk answered before a move landed alpha.txt in full.
                    host.sizes.sizes.remember(
                        1,
                        Size {
                            bytes: 120,
                            partial: false,
                        },
                        CAP,
                    );
                }
            }
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.sizes.sizes.size_for(1),
                None,
                "v0.3.12 PaneSwap.applyListed empties DirSizes, so a re-list walks full again"
            );
        })
    });
}

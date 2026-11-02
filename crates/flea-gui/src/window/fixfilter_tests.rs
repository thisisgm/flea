//! EY-571, EY-572 and EY-573: a waiting child's background menu, the marked-row plate under a filter, and Escape's order with a filter and marks.
use crate::{
    backend::Backend,
    list::RowState,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{
    AppContext, Context, KeyDownEvent, Keystroke, Modifiers, TestAppContext, Window, WindowHandle,
};

type Handle = WindowHandle<Root>;

fn open(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (backend, read) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    (handle, read)
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

fn press(cx: &mut TestAppContext, handle: Handle, key: &str, text: Option<&str>, control: bool) {
    edit(cx, handle, |shell, window, cx| {
        let keystroke = Keystroke {
            key: key.into(),
            key_char: text.map(Into::into),
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
            ..Default::default()
        };
        shell.key(
            &KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            },
            window,
            cx,
        )
    });
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

/// "/", "n", the backend's three rows, Return and Ctrl+A: the committed filter with every shown row marked.
fn committed_and_marked(cx: &mut TestAppContext) -> Handle {
    let (handle, mut read) = open(cx);
    frame(cx, handle);
    while read().is_some() {}
    press(cx, handle, "/", Some("/"), false);
    press(cx, handle, "n", Some("n"), false);
    assert_eq!(
        read().as_deref(),
        Some(r#"{"c":"filter","query":"n","id":1}"#)
    );
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            Backend::test_reply_line(
                r#"{"t":"filtered","id":1,"query":"n","n":1000,"rows":[3,5,700],"ms":0.010,"listing":2}"#,
            ),
            window,
            cx,
        )
    });
    press(cx, handle, "return", None, false);
    edit(cx, handle, |shell, _, _| {
        let owner = &shell.panes[0].as_ref().unwrap().filter.owner;
        assert!(owner.up() && !owner.typing(), "Return commits the line");
    });
    press(cx, handle, "a", None, true);
    frame(cx, handle);
    handle
}

// EY-572: the footer counts three marks, so all three take the plate, which reads the selection snapshot.
#[gpui::test]
fn s1_select_all_on_a_committed_filter_plates_every_marked_row(cx: &mut TestAppContext) {
    let handle = committed_and_marked(cx);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, vec![3, 5, 700]);
        assert_eq!(
            host.input.selection.snapshot.as_deref(),
            Some(&[3, 5, 700][..]),
            "the plate's snapshot follows the marks the filter made"
        );
        let list = host.list.borrow();
        let plated: Vec<usize> = list
            .visible()
            .iter()
            .filter(|row| row.state.selected)
            .map(|row| row.index)
            .collect();
        assert_eq!(plated, vec![3, 5, 700], "every marked row draws the plate");
        assert!(list.visible().iter().all(|row| !row.bold_match));
    });
}

// EY-572, filter-select-all: a marked row carries no typed run; the cursor bolds its own, and search marks keep theirs.
#[test]
fn s1_a_marked_row_under_a_filter_carries_no_run_wash() {
    let marked = RowState {
        selected: true,
        ..RowState::default()
    };
    let cursor = RowState {
        cursor: true,
        ..RowState::default()
    };
    assert!(!marked.washes_run(false));
    assert!(marked.washes_run(true));
    assert!(!cursor.washes_run(false));
    assert!(RowState::default().washes_run(false));
}

// EY-573, Focus.js:187: Escape closes the committed filter and keeps the marks; the next Escape clears them.
#[gpui::test]
fn s1_escape_clears_the_filter_before_the_marks(cx: &mut TestAppContext) {
    let handle = committed_and_marked(cx);
    press(cx, handle, "escape", None, false);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.filter.owner.up(), "the filter went first");
        assert!(host.filter.shown.is_none(), "every row is back");
        assert_eq!(host.selected, vec![3, 5, 700], "the marks stay");
        assert!(shell.slots.strips[0].is_none());
    });
    press(cx, handle, "escape", None, false);
    edit(cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().selected.is_empty())
    });
}

#[gpui::test]
fn s1_filtered_empty_ground_opens_the_background_menu(cx: &mut TestAppContext) {
    use crate::window::startup::View;
    use gpui::{point, px, MouseButton, VisualTestContext};
    for view in [View::List, View::Grid] {
        let handle = committed_and_marked(cx);
        edit(cx, handle, |shell, _, _| shell.view = view);
        frame(cx, handle);
        let at = edit(cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let bounds = host.input.frame.unwrap().bounds;
            point(bounds.left() + px(40.), bounds.bottom() - px(40.))
        });
        VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
            at,
            MouseButton::Right,
            Modifiers::none(),
        );
        edit(cx, handle, |shell, _, _| {
            let capture = shell.menu.captured.as_ref().expect("ground opens a menu");
            assert!(capture.background, "{view:?} ground opened a row menu");
            assert!(capture.rows.is_empty());
            assert_eq!(shell.panes[0].as_ref().unwrap().selected, vec![3, 5, 700]);
        });
    }
}

use crate::{backend::Event, state::Shell};
use flea::{jsondoc::Json, protocol::RowFacts};
use gpui::{AppContext, Context, TestAppContext, TestDispatcher, Window, WindowHandle};

pub(super) fn context() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    )
}

pub(super) fn row(name: &str, dir: bool) -> RowFacts {
    RowFacts {
        name: name.into(),
        dir,
        icon: if dir { "folder" } else { "text-plain" }.into(),
        ..Default::default()
    }
}

pub(super) fn change(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) {
    handle.update(cx, change).unwrap();
}

pub(super) fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

/// Answers every drawn column still out with one folder row, which ends the entry hold on the List.
pub(super) fn settle(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    let (out, first, hidden, hidden_last) = {
        let host = shell.panes[0].as_ref().unwrap();
        let folders = host.nav.folders.borrow();
        let columns = host.columns.as_ref().unwrap();
        let out: Vec<String> = columns
            .area
            .drawn
            .iter()
            .filter(|path| {
                let key =
                    crate::columns::peeks::peek_key(path, folders.hidden, folders.hidden_last);
                columns.area.peeks.rows(&key).is_none()
            })
            .cloned()
            .collect();
        (
            out,
            host.pane.first_count(),
            folders.hidden,
            folders.hidden_last,
        )
    };
    for path in out {
        let text = |value: &str| Json::Str(value.into());
        let number = |value: usize| Json::Num(value.to_string());
        let document = Json::Obj(vec![
            ("t".into(), text("peeked")),
            ("path".into(), text(&path)),
            ("first".into(), number(first)),
            ("n".into(), number(1)),
            ("hidden".into(), Json::Bool(hidden)),
            ("hiddenLast".into(), Json::Bool(hidden_last)),
            ("failed".into(), Json::Bool(false)),
            ("mode".into(), number(0)),
            (
                "rows".into(),
                Json::Arr(vec![Json::Obj(vec![
                    ("n".into(), text("x")),
                    ("d".into(), Json::Bool(true)),
                ])]),
            ),
        ]);
        shell.receive(0, Event::Peeked(document), window, cx);
    }
}

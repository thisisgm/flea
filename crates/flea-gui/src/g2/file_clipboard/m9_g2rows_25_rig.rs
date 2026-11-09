//! The production Shell rig Carry's tests drive: a window over the test backend, real keystrokes, backend answers and the click the pointer hook hands over.
use super::carry::Carrying;
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{
    AppContext, Context, KeyDownEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext,
    Window, WindowHandle,
};

pub(super) type Handle = WindowHandle<Root>;

pub(super) fn open(
    cx: &mut TestAppContext,
    folder: &str,
    names: &[&str],
) -> (Handle, impl FnMut() -> Vec<String>) {
    let (backend, mut lines) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    let folder = folder.to_owned();
    let rows: Vec<_> = names
        .iter()
        .map(|name| flea::protocol::RowFacts {
            name: (*name).into(),
            dir: !name.contains('.'),
            ..Default::default()
        })
        .collect();
    edit(cx, handle, move |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin(folder.clone(), Default::default(), pane.now()));
        let total = rows.len();
        for reply in [
            flea::protocol::Reply::Listed {
                path: folder,
                total,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                listing: 3,
                rows,
            },
        ] {
            shell.receive(0, BackendEvent::Reply(reply), window, cx);
        }
        shell.panes[0].as_mut().unwrap().pane.cursor = 0;
    });
    frame(cx, handle);
    let drain = move || std::iter::from_fn(&mut lines).collect::<Vec<_>>();
    (handle, drain)
}

pub(super) fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

pub(super) fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

// The frame a window draws on its own: no forced refresh, so a cached Shell repaints only when something notified it.
pub(super) fn pending_frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
}

// One key as the platform delivers it: Shift on a cased letter carries its capital as text, as Linux sends it.
pub(super) fn press(
    cx: &mut TestAppContext,
    handle: Handle,
    key: &str,
    text: Option<&str>,
    shift: bool,
) {
    let mut window = VisualTestContext::from_window(handle.into(), cx);
    window.simulate_event(KeyDownEvent {
        keystroke: Keystroke {
            key: key.into(),
            key_char: text.map(str::to_owned),
            modifiers: Modifiers {
                shift,
                ..Modifiers::none()
            },
            is_keypad: false,
            us_key: None,
        },
        is_held: false,
        prefer_character_input: false,
    });
    window.run_until_parked();
    frame(cx, handle);
}

pub(super) fn carry_key(cx: &mut TestAppContext, handle: Handle) {
    press(cx, handle, "x", Some("X"), true);
}

pub(super) fn escape_key(cx: &mut TestAppContext, handle: Handle) {
    press(cx, handle, "escape", None, false);
}

pub(super) fn answer(cx: &mut TestAppContext, handle: Handle, paths: &[&str]) {
    let paths: Vec<String> = paths.iter().map(|path| (*path).to_owned()).collect();
    edit(cx, handle, move |shell, window, cx| {
        shell.receive(0, BackendEvent::Paths(paths), window, cx)
    });
    frame(cx, handle);
}

pub(super) fn notice(cx: &mut TestAppContext, handle: Handle) -> String {
    edit(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, _| messages.notice().to_owned())
    })
}

pub(super) fn footer_status(
    cx: &mut TestAppContext,
    handle: Handle,
) -> (String, crate::g2::status_feedback::line::Drawn) {
    edit(cx, handle, |shell, _, cx| {
        let footer = shell.footer(cx);
        let footer = footer.read(cx);
        (
            footer.counts.to_string(),
            footer.line.read(cx).drawn().clone(),
        )
    })
}

pub(super) fn errors(cx: &mut TestAppContext, handle: Handle) -> Vec<String> {
    edit(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, _| {
            messages
                .errors()
                .iter()
                .map(|entry| entry.text.clone())
                .collect()
        })
    })
}

pub(super) fn riding(cx: &mut TestAppContext) -> Option<Vec<String>> {
    cx.update(|cx| {
        cx.try_global::<Carrying>()
            .and_then(|carrying| carrying.carry.riding().map(|lifted| lifted.paths.clone()))
    })
}

pub(super) fn reply(cx: &mut TestAppContext, handle: Handle, line: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(line), window, cx)
    });
    frame(cx, handle);
}

// The click the pointer hook hands over: the folder it resolved and the keys held, then the background facts read and the landing.
pub(super) fn click(
    cx: &mut TestAppContext,
    handle: Handle,
    dest: &str,
    ctrl: bool,
    shift: bool,
) -> bool {
    let dest = dest.to_owned();
    let took = edit(cx, handle, move |shell, window, cx| {
        shell.carry_land(0, &dest, ctrl, shift, window, cx)
    });
    cx.run_until_parked();
    frame(cx, handle);
    took
}

pub(super) fn mark(cx: &mut TestAppContext, handle: Handle, rows: Vec<usize>) {
    edit(cx, handle, move |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = rows
    });
}

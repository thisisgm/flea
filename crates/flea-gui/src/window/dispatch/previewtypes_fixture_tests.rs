//! Shared native preview-column fixtures and backend request recording.
use crate::{
    backend::Backend,
    navigation::folder_state::Writer,
    state::Shell,
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
    },
};
use flea::protocol::RowFacts;
use gpui::{px, size, Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, sync::Arc, time::Duration};

type Lines = Box<dyn FnMut() -> Option<String>>;
thread_local! {
    static LINES: RefCell<Vec<Lines>> = RefCell::new(Vec::new());
}

/// The native constructor in the columns view, the backend replaced by one that keeps every request line.
pub(super) fn open<'a>(
    cx: &'a mut TestAppContext,
    path: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    open_at(cx, path, None)
}

/// The same constructor at an explicit text size, the way a user's text setting reaches the window.
pub(super) fn open_at<'a>(
    cx: &'a mut TestAppContext,
    path: &str,
    rem: Option<f32>,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    open_with_preview(cx, path, rem, "automatic")
}

/// The native constructor with the preview loading preference supplied at launch.
pub(super) fn open_with_preview<'a>(
    cx: &'a mut TestAppContext,
    path: &str,
    rem: Option<f32>,
    load_on: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let document = flea::jsondoc::parse(
        &format!(r#"{{"keys":"default","view":"columns","preview":{{"column":true,"loadOn":"{load_on}"}},"places":{{"rail":"shown","showHome":true,"showTrash":true}}}}"#),
    )
    .unwrap();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let launch = Launch::resolve(
        Settings::from_json(document),
        Arguments {
            path: Some(path.into()),
            select: None,
        },
        &home,
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    LINES.with(|slot| slot.borrow_mut().clear());
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, _| {
                let (backend, lines) = Backend::test_lines();
                LINES.with(|slot| slot.borrow_mut().push(Box::new(lines)));
                Ok(backend)
            },
            monitor: None,
        })
    });
    let (shell, cx) = cx.add_window_view(|window, cx| {
        if let Some(rem) = rem {
            window.set_rem_size(px(rem));
        }
        Shell::new(launch, locale, events, window, cx)
    });
    shell.update(cx, |shell, _| {
        *shell.panes[0].as_ref().unwrap().nav.writer.borrow_mut() = Writer::default();
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    (shell, cx)
}

/// A file row as the rows reply carries it, Kind description included.
pub(super) fn file(name: &str, icon: &str, kind: &str, mode: u32) -> RowFacts {
    RowFacts {
        name: name.into(),
        size: 600,
        mtime: 1,
        mode,
        icon: icon.into(),
        kind: kind.into(),
        ..RowFacts::default()
    }
}

/// Frames and time for the settle and the swap, then every request line the backend received.
pub(super) fn pump(cx: &mut VisualTestContext) -> Vec<String> {
    for _ in 0..6 {
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
    LINES.with(|slot| {
        let mut out = Vec::new();
        for receive in slot.borrow_mut().iter_mut() {
            out.extend(std::iter::from_fn(&mut *receive));
        }
        out
    })
}

/// The folder's storage class answered local, which is what the column waits on before it loads, then the asks.
pub(super) fn local(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &str) -> Vec<String> {
    let mut lines = pump(cx);
    answer(
        shell,
        cx,
        &format!(
            r#"{{"t":"fsinfo","path":"{path}","fs":"tmpfs","dev":1,"total":100,"free":90,"class":"local"}}"#
        ),
    );
    lines.extend(pump(cx));
    lines
}

/// One backend reply line, delivered the way the reader thread delivers it.
pub(super) fn answer(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Backend::test_reply_line(line), window, cx)
        })
    });
}

/// The request whose command is `c`, and one of its numeric fields.
pub(super) fn ask<'a>(lines: &'a [String], c: &str) -> &'a str {
    let tag = format!(r#""c":"{c}""#);
    lines
        .iter()
        .rev()
        .find(|line| line.contains(&tag))
        .unwrap_or_else(|| panic!("the column asked {c}: {lines:?}"))
}
pub(super) fn field(line: &str, key: &str) -> u64 {
    flea::jsondoc::parse(line)
        .unwrap()
        .get(key)
        .and_then(flea::jsondoc::Json::as_f64)
        .unwrap_or_else(|| panic!("{key} in {line}")) as u64
}

/// The meta reply for the column's last meta ask, with the extra fields given.
pub(super) fn meta(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    lines: &[String],
    extra: &str,
) {
    let asked = ask(lines, "meta");
    let (row, token) = (field(asked, "row"), field(asked, "token"));
    answer(
        shell,
        cx,
        &format!(r#"{{"t":"meta","row":{row},"token":{token}{extra}}}"#),
    );
}

/// A valid simulated producer answer echoes the identity on its recorded actual request.
pub(super) fn thumbed(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    lines: &[String],
    row: usize,
    file: &str,
) {
    use flea::{
        json::escape,
        jsondoc::{self, Json},
    };
    let request = lines.iter().rev().filter_map(|line| jsondoc::parse(line).ok()).find(|doc| {
        doc.get("c").and_then(Json::as_str) == Some("thumb")
            && doc.get("thumbToken").is_some()
            && doc.get("rows").and_then(Json::as_array).is_some_and(|rows| rows.iter().any(|value| matches!(value, Json::Num(value) if value.parse::<usize>().ok() == Some(row))))
    }).expect("recorded tagged preview request");
    let token = request.get("thumbToken").and_then(Json::as_str).unwrap();
    let path = request.get("thumbPath").and_then(Json::as_str).unwrap();
    let Some(Json::Num(listing)) = request.get("thumbListing") else {
        panic!("listing identity")
    };
    answer(
        shell,
        cx,
        &format!(
            r#"{{"t":"thumbed","row":{row},"file":"{}","thumbToken":"{}","thumbPath":"{}","thumbListing":{listing}}}"#,
            escape(file),
            escape(token),
            escape(path)
        ),
    );
}

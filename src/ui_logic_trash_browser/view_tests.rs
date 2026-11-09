//! tests/trash-view.qml over a scripted backend: order, landing, the m menu, restore and marks.
use super::dates_tests::english;
use super::host::{self, Host, Item};
use super::rows::Reply;
use super::view::{Anchor, Keys, Press, Signal, View};
use crate::jsondoc::{self, Json};

pub(super) const ROW: f32 = 31.;

/// The backend side of the session: the live Trash, every request seen, and replies held for the test to release.
#[derive(Default)]
pub(super) struct Backend {
    pub live: Vec<String>,
    pub sent: Vec<Json>,
    pub held: Vec<Json>,
    pub hold_ops: Vec<&'static str>,
}

fn uri(row: &str) -> String {
    jsondoc::parse(row).unwrap().get("uri").and_then(Json::as_str).unwrap().to_string()
}

impl Backend {
    pub fn with(rows: Vec<String>) -> Self {
        Self { live: rows, ..Self::default() }
    }
    pub fn ops(&self, op: &str) -> Vec<&Json> {
        self.sent.iter().filter(|m| m.get("c").and_then(Json::as_str) == Some("trashbrowse") && m.get("op").and_then(Json::as_str) == Some(op)).collect()
    }
    pub fn commands(&self, command: &str) -> Vec<&Json> {
        self.sent.iter().filter(|m| m.get("c").and_then(Json::as_str) == Some(command)).collect()
    }
    pub fn answer(&mut self, m: &Json, selected: &[String]) -> Reply {
        let num = |key: &str| m.get(key).and_then(Json::as_f64).unwrap_or(0.) as usize;
        let op = m.get("op").and_then(Json::as_str).unwrap_or("").to_string();
        let fields = match op.as_str() {
            "list" | "window" => {
                let start = num("start");
                let count = if m.get("count").is_some() { num("count") } else { 100 };
                let rows: Vec<&str> = self.live.iter().skip(start).take(count).map(String::as_str).collect();
                let live: Vec<String> = self.live.iter().map(|row| uri(row)).collect();
                let asked = m.get("uris").and_then(Json::as_array).unwrap_or(&[]);
                let present: Vec<String> = asked.iter().filter_map(Json::as_str).filter(|u| live.iter().any(|l| l == u)).map(String::from).collect();
                format!(r#","total":{},"start":{start},"rows":[{}],"selectionToken":0,"selectionCount":0,"selectionStale":false,"present":{},"recoveredCount":0,"recoveryErrors":[]"#,
                    self.live.len(), rows.join(","), crate::json::string_array(&present))
            }
            "summary" => r#","bytes":1024,"partial":false"#.into(),
            "prepare" => {
                let all = m.get("emptyTrash") == Some(&Json::Bool(true));
                let count = if all { self.live.len() } else { selected.len() };
                format!(r#","token":7,"all":{all},"count":{count},"bytes":20,"partial":false"#)
            }
            "check" => r#","valid":true"#.into(),
            "restore" | "delete" => {
                let gone: Vec<String> = if op == "restore" {
                    m.get("uris").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect()
                } else {
                    selected.to_vec()
                };
                self.live.retain(|row| !gone.contains(&uri(row)));
                format!(r#","done":{},"failed":0,"failures":[]"#, gone.len())
            }
            _ => String::new(),
        };
        let id = num("id");
        Reply::read(jsondoc::parse(&format!(r#"{{"t":"trashbrowse","id":{id},"op":"{op}","ok":true{fields}}}"#)).unwrap()).unwrap()
    }
    /// Sends the view's lines and answers each until nothing is left, holding the ops the test releases itself.
    pub fn pump(&mut self, host: &mut Host) {
        let i18n = english();
        loop {
            let lines = host.drain();
            if lines.is_empty() {
                return;
            }
            for line in lines {
                let m = jsondoc::parse(&line).unwrap();
                self.sent.push(m.clone());
                if m.get("c").and_then(Json::as_str) != Some("trashbrowse") {
                    continue;
                }
                if self.hold_ops.iter().any(|op| m.get("op").and_then(Json::as_str) == Some(op)) {
                    self.held.push(m);
                    continue;
                }
                let reply = self.answer(&m, &host.view.selection.uris());
                host.receive(&reply, &i18n, 0);
            }
        }
    }
    pub fn release(&mut self, host: &mut Host) {
        let i18n = english();
        for m in std::mem::take(&mut self.held) {
            let reply = self.answer(&m, &host.view.selection.uris());
            host.receive(&reply, &i18n, 0);
        }
        self.pump(host);
    }
}

pub(super) fn row(name: &str, deleted: &str) -> String {
    format!(r#"{{"uri":"trash:///{name}","original":"/home/gm/{name}","size":10,"deleted":"{deleted}","directory":false,"icon":"text-x-generic","identity":"id-{name}","bulkSelected":false,"storageClass":"","thumbnail":null}}"#)
}

fn names(view: &View) -> Vec<String> {
    view.rows.iter().map(|r| r.name().into_owned()).collect()
}

fn labels(items: &[Item]) -> Vec<&'static str> {
    items.iter().filter_map(|i| match i { Item::Entry(e) => Some(e.label_key), Item::Separator => None }).collect()
}

fn hidden() -> Vec<String> {
    ["delete", "openTerminal", "placeMenu"].iter().map(|s| s.to_string()).collect()
}

#[test]
fn trash_view_lands_on_the_newest_row_and_m_follows_the_marks() {
    let i18n = english();
    let mut backend = Backend::with(vec![row("photo.png", "2026-10-05T09:00:00"), row("notes.txt", "2026-10-04T09:00:00"), row("old-folder", "2026-10-03T09:00:00")]);
    let mut host = Host::new(View::new(ROW));
    host.view.set_viewport(0., 500.);
    host.open("");
    backend.pump(&mut host);
    let view = &mut host.view;
    assert_eq!(names(view), ["photo.png", "notes.txt", "old-folder"], "rows keep the order the backend gives");
    assert_eq!(view.cursor, 0, "the cursor opens on the newest row");
    assert!(view.is_selected("trash:///photo.png"), "and that row is marked in the selection look");
    assert_eq!(view.selection.uris(), ["trash:///photo.png"], "and only that row");
    assert_eq!(view.bytes, Some((1024, false)), "the summary follows the first list");
    view.signals();
    view.press(Press::Action("menu"), false, 0, &i18n, Keys::default());
    assert_eq!(view.signals(), [Signal::Context { at: Anchor::Row(0), selection: true }], "m opens under the cursor row, once");
    let menu = host::selection_menu(view.busy(), &hidden());
    assert_eq!(labels(&menu), ["trash.menu.restore"], "m on the cursor row holds Restore alone");
    host.action("restoreTrashSelection");
    backend.hold_ops = vec!["restore"];
    backend.pump(&mut host);
    let restore = backend.ops("restore")[0].clone();
    assert_eq!(restore.get("uris"), Some(&Json::Arr(vec![Json::Str("trash:///photo.png".into())])), "Enter on Restore restores the newest item");
    assert_eq!(restore.get("all"), Some(&Json::Bool(false)), "and not the whole Trash");
    assert_eq!(restore.get("identities"), Some(&Json::Arr(vec![Json::Str("id-photo.png".into())])), "with the identity the row was read with");
    backend.release(&mut host);
    let view = &mut host.view;
    assert_eq!(names(view), ["notes.txt", "old-folder"], "the restored row is gone and the next newest leads");
    assert!(view.selection.uris().is_empty(), "nothing is marked after a restore");
    assert!(view.signals().iter().any(|s| matches!(s, Signal::Result { headline, error: false, .. } if headline == "Restored 1 of 1 item")));
    view.press(Press::Action("menu"), false, 0, &i18n, Keys::default());
    assert_eq!(view.signals(), [Signal::Context { at: Anchor::Row(0), selection: true }], "m with nothing marked opens the cursor row's menu");
    assert_eq!(view.selection.uris(), ["trash:///notes.txt"], "having marked that row for the action");
    view.press(Press::Action("extendDown"), false, 0, &i18n, Keys::default());
    let mut marked = view.selection.uris();
    marked.sort();
    assert_eq!(marked, ["trash:///notes.txt", "trash:///old-folder"], "shift+Down marks a second row");
    view.press(Press::Action("menu"), false, 0, &i18n, Keys::default());
    assert_eq!(view.signals(), [Signal::Context { at: Anchor::Top, selection: true }], "two marked rows open at the top centre");
}

#[test]
fn trash_view_drops_stale_replies_and_reports_refusals() {
    let i18n = english();
    let mut backend = Backend::with(vec![row("a.txt", "2026-10-05T09:00:00")]);
    let mut host = Host::new(View::new(ROW));
    host.view.set_viewport(0., 500.);
    host.open("");
    backend.pump(&mut host);
    let id = host.view.request_id;
    let late = Reply::read(jsondoc::parse(&format!(r#"{{"t":"trashbrowse","id":{},"op":"list","ok":true,"total":9,"start":0,"rows":[]}}"#, id - 1)).unwrap()).unwrap();
    host.view.receive(&late, &i18n);
    assert_eq!(host.view.total, 1, "an answer to an older request is dropped");
    host.view.refresh();
    let lines = host.drain();
    assert_eq!(lines.len(), 1, "refresh sends one list");
    let failed = Reply::read(jsondoc::parse(&format!(r#"{{"t":"trashbrowse","id":{},"op":"list","ok":false,"stale":false,"error":""}}"#, host.view.request_id)).unwrap()).unwrap();
    host.view.receive(&failed, &i18n);
    assert_eq!(host.view.error_text.as_deref(), Some("That Trash action could not be completed."), "an empty refusal reads the fallback");
    assert!(!host.view.busy(), "and the request is no longer in flight");
    host.view.refresh();
    let lines = host.drain();
    let malformed = Reply::read(jsondoc::parse(&format!(r#"{{"t":"trashbrowse","id":{},"op":"list","ok":true,"total":1,"start":0,"rows":[{{"uri":"x"}}]}}"#, host.view.request_id)).unwrap()).unwrap();
    assert_eq!(lines.len(), 1);
    host.view.receive(&malformed, &i18n);
    assert!(host.view.signals().iter().any(|s| matches!(s, Signal::Status { error: true, .. })), "a malformed row fails the window closed");
    host.close();
    let lines = host.drain();
    assert!(lines.iter().any(|l| l.contains(r#""op":"cancel""#) && l.contains(r#""clearSelection":true"#)), "close cancels the session's selection");
    let after = Reply::read(jsondoc::parse(&format!(r#"{{"t":"trashbrowse","id":{},"op":"cancel","ok":true}}"#, host.view.request_id)).unwrap()).unwrap();
    host.view.receive(&after, &i18n);
    assert!(host.drain().is_empty(), "a closed view reads no reply");
}

#[test]
fn trash_view_keeps_listing_keys_and_forwards_window_keys() {
    let i18n = english();
    let mut backend = Backend::with((0..40).map(|i| row(&format!("f{i:02}"), "2026-10-05T09:00:00")).collect());
    let mut host = Host::new(View::new(ROW));
    host.view.set_viewport(0., 310.);
    host.open("");
    backend.pump(&mut host);
    let view = &mut host.view;
    view.signals();
    for (action, forwarded) in [("search", false), ("openTerminal", false), ("settings", true), ("keymapSheet", true), ("quit", true)] {
        view.press(Press::Action(action), false, 0, &i18n, Keys::default());
        let sent = view.signals().iter().any(|s| *s == Signal::Action(action.into()));
        assert_eq!(sent, forwarded, "{action}");
    }
    view.press(Press::Action("pageDown"), false, 0, &i18n, Keys::default());
    assert_eq!(view.cursor, 10, "a page is the window less two rows");
    assert!(view.scroll_y > 0., "and the view follows the cursor");
    view.press(Press::Action("cursorLast"), false, 0, &i18n, Keys::default());
    backend.pump(&mut host);
    assert_eq!((host.view.cursor, host.view.selection.uris()), (39, vec!["trash:///f39".to_string()]), "the last row is fetched and then chosen");
    let view = &mut host.view;
    view.signals();
    view.press(Press::Action("trashArm"), false, 1_000, &i18n, Keys { arm: "d", alone: "Delete" });
    assert_eq!(view.signals(), [Signal::Arm("Press d again to review permanent deletion, or Delete on its own.".into())]);
    view.press(Press::Action("trashArm"), true, 1_100, &i18n, Keys { arm: "d", alone: "Delete" });
    assert!(view.drain().is_empty(), "an auto-repeated d is no pair");
    view.press(Press::Action("trashArm"), false, 1_200, &i18n, Keys { arm: "d", alone: "Delete" });
    let lines = view.drain();
    assert!(lines.len() == 1 && lines[0].contains(r#""op":"prepare""#) && view.confirming, "dd reviews permanent deletion of the marks");
    let reply = backend.answer(&jsondoc::parse(&lines[0]).unwrap(), &view.selection.uris());
    host.receive(&reply, &i18n, 0);
    let view = &mut host.view;
    assert!(view.confirmation.is_some(), "the review answered, so the card is open");
    view.press(Press::Backspace, false, 0, &i18n, Keys::default());
    assert!(view.opened && view.drain().is_empty(), "the open card holds the keys, so Backspace does not close the view under it");
    view.confirmed(None);
    view.press(Press::Backspace, false, 0, &i18n, Keys::default());
    assert!(!view.opened, "Backspace closes the view once the card is gone");
}

#[test]
fn trash_view_reads_the_next_window_before_a_wheel_notch_reaches_unheld_rows() {
    let mut backend = Backend::with((0..200).map(|i| row(&format!("f{i:03}"), "2026-10-05T09:00:00")).collect());
    let mut host = Host::new(View::new(ROW));
    host.view.set_viewport(0., 20. * ROW);
    host.open("");
    backend.pump(&mut host);
    // Every wheel notch moves three rows; each must land on rows held before the backend answers anything.
    for notch in 1..=60 {
        host.view.set_viewport(notch as f32 * 3. * ROW, 20. * ROW);
        let top = (host.view.scroll_y / ROW).floor() as usize;
        let end = ((host.view.scroll_y + 20. * ROW) / ROW).ceil() as usize;
        assert!((top..end.min(200)).all(|index| host.view.row_at(index).is_some()), "notch {notch} shows unheld rows");
        backend.pump(&mut host);
    }
    let windows = backend.ops("window").len();
    assert!((1..=10).contains(&windows), "60 notches over 180 rows read {windows} windows");
}

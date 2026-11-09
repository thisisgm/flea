//! Compare in the Quick Look card: two regular files, one diff asked of the pane's backend (Ops.js open and openCompare).
use super::{token, Ask};
use crate::{
    g2::quick_look_overlay::{Body, Subject},
    state::Shell,
};
use flea::{
    json::escape,
    jsondoc::Json,
    protocol::RowFacts,
    ui_logic_context_menus::build::S_IFDIR,
    ui_logic_preview_core::{compare, kinds::State, rect::Shape},
};
use gpui::{Context, Window};

pub(super) fn close_line(id: u64) -> String {
    format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)
}

impl Shell {
    /// Ops.js open: two selected regular files open Compare, through a backend snapshot when a row is outside the held window; false for any other selection.
    pub fn open_compare(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.panes[side]
            .as_ref()
            .filter(|p| !p.pane.busy() && !p.stopped)
        else {
            return false;
        };
        let rows = pane.selected.clone();
        let held: Vec<Option<RowFacts>> = rows
            .iter()
            .map(|&index| pane.pane.held.row(index).cloned())
            .collect();
        let modes: Vec<u32> = held
            .iter()
            .flatten()
            .map(|row| if row.dir { S_IFDIR } else { row.mode })
            .collect();
        if modes.len() == rows.len() {
            if !compare::can_compare(&modes) {
                return false;
            }
            let picked: Vec<RowFacts> = held.into_iter().flatten().collect();
            let base = pane.pane.path().to_owned();
            let [a, b] = [&picked[0], &picked[1]]
                .map(|row| crate::navigation::history::join(&base, &row.name));
            self.open_compare_paths(side, a, b, &picked[0], window, cx);
            return true;
        }
        // The backend keeps one menu snapshot, so an open menu's is never replaced; a held row that cannot compare refuses at once.
        if rows.len() != 2
            || self.menu.captured.is_some()
            || modes
                .iter()
                .any(|&mode| !compare::can_compare(&[mode, mode]))
        {
            return false;
        }
        let (listing, cursor) = (pane.pane.held.listing, pane.pane.cursor);
        let id = token();
        let line = format!(
            r#"{{"c":"menuaction","op":"snapshot","id":{id},"listing":{listing},"rows":[{},{}],"cursor":{cursor}}}"#,
            rows[0], rows[1]
        );
        let host = self.quick_look_host(side, window, cx);
        host.token = id;
        host.ask = Ask::Resolve {
            listing,
            rows,
            mark: held.into_iter().next().flatten(),
            kinds: None,
        };
        self.quick_look_send(side, line);
        true
    }

    /// The snapshot's kinds, then its validated paths; any refusal or a moved selection closes the snapshot and opens nothing.
    pub(super) fn compare_resolved(
        &mut self,
        side: usize,
        id: u64,
        (listing, rows, mark, kinds): (usize, Vec<usize>, Option<RowFacts>, Option<Vec<u32>>),
        doc: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let still = self.panes[side].as_ref().is_some_and(|p| {
            !p.pane.busy() && !p.stopped && p.pane.held.listing == listing && p.selected == rows
        });
        let ok = still && doc.get("ok").and_then(Json::as_bool) == Some(true);
        let op = doc.get("op").and_then(Json::as_str).unwrap_or("");
        let list = |key| doc.get(key).and_then(Json::as_array).unwrap_or(&[]);
        match (op, kinds) {
            ("snapshot", None) if ok => {
                let kinds: Vec<u32> = list("kinds")
                    .iter()
                    .filter_map(|kind| match kind {
                        Json::Num(value) => value.parse::<u32>().ok(),
                        _ => None,
                    })
                    .collect();
                if compare::can_compare(&kinds) {
                    let host = self.quick_look.as_mut().expect("asked through the host");
                    host.token = id;
                    host.ask = Ask::Resolve {
                        listing,
                        rows,
                        mark,
                        kinds: Some(kinds),
                    };
                    let line = format!(
                        r#"{{"c":"menuaction","op":"validate","id":{id},"action":"compare"}}"#
                    );
                    self.quick_look_send(side, line);
                    return;
                }
            }
            ("validate", Some(kinds)) if ok => {
                let paths: Vec<String> = list("paths")
                    .iter()
                    .filter_map(Json::as_str)
                    .filter(|path| path.starts_with('/'))
                    .map(str::to_owned)
                    .collect();
                if let Ok([a, b]) = <[String; 2]>::try_from(paths) {
                    self.quick_look_send(side, close_line(id));
                    let mark = mark.unwrap_or(RowFacts {
                        mode: kinds[0],
                        ..RowFacts::default()
                    });
                    self.open_compare_paths(side, a, b, &mark, window, cx);
                    return;
                }
            }
            _ => {}
        }
        self.quick_look_send(side, close_line(id));
    }

    /// The Compare card for two paths; the bar's mark is `mark`'s, the first file's row.
    pub(crate) fn open_compare_paths(
        &mut self,
        side: usize,
        a: String,
        b: String,
        mark: &RowFacts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let catalog = self.locale.catalog.clone();
        let title = compare::title(compare::leaf(&a), compare::leaf(&b), None, &catalog);
        let subject = Subject {
            name: compare::leaf(&a).into(),
            icon: if mark.icon.is_empty() {
                "text-x-generic".into()
            } else {
                mark.icon.clone()
            },
            mode: mark.mode,
            state: State::Text,
            shape: Shape::unknown(State::Unsupported),
            body: Body::Refused {
                title,
                sentence: String::new(),
            },
        };
        let token = token();
        let line = format!(
            r#"{{"c":"compare","token":{token},"a":"{}","b":"{}"}}"#,
            escape(&a),
            escape(&b)
        );
        let host = self.quick_look_host(side, window, cx);
        host.token = token;
        host.row = None;
        host.answer.clear();
        host.patch.clear();
        host.ask = Ask::Compare { a, b };
        self.show(subject, false, Some(line), window, cx);
    }
}

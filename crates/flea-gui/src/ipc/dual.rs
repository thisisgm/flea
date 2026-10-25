//! Ipc.qml's dualState over the two pane hosts, read by case_dual, case_dualsort and case_duallaunch; a pane never built answers null.
use super::Adapter;
use flea::{ipc::wire::Reply, json::escape};

/// One pane as dualState reports it; None is a pane the window never built.
pub struct DualPane<'a> {
    pub path: &'a str,
    pub total: usize,
    pub cursor: usize,
    pub loading: bool,
    pub selected: &'a [usize],
    pub focused: bool,
    pub list_requests: usize,
}

fn pane(pane: &DualPane<'_>) -> String {
    let selected = pane
        .selected
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"path\":\"{}\",\"total\":{},\"cursor\":{},\"loading\":{},\"selected\":[{}],\"focused\":{},\"listRequests\":{}}}",
        escape(pane.path),
        pane.total,
        pane.cursor,
        pane.loading,
        selected,
        pane.focused,
        pane.list_requests
    )
}

pub struct DualAdapter<'a> {
    pub active: bool,
    pub focused: usize,
    pub panes: [Option<DualPane<'a>>; 2],
}

impl Adapter for DualAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if function != "dualState" {
            return None;
        }
        let panes = self
            .panes
            .iter()
            .map(|slot| slot.as_ref().map(pane).unwrap_or_else(|| "null".into()))
            .collect::<Vec<_>>()
            .join(",");
        Some(Reply::value(format!(
            "{{\"active\":{},\"focused\":{},\"panes\":[{}]}}",
            self.active, self.focused, panes
        )))
    }
}

/// Ipc.qml dragPaneGeometry: one pane's list, a folder row and its last row, for the drag cases.
pub(crate) struct DragGeometry<'a> {
    pub shell: &'a crate::state::Shell,
}

impl Adapter for DragGeometry<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if function != "dragPaneGeometry" {
            return None;
        }
        use crate::window::startup::View;
        let number = |at: usize| {
            arguments
                .get(at)
                .and_then(|text| text.trim().parse::<i64>().ok())
        };
        let (side, index) = (number(0).unwrap_or(-1), number(1).unwrap_or(-1));
        let host = usize::try_from(side)
            .ok()
            .and_then(|side| self.shell.panes.get(side))
            .and_then(Option::as_ref);
        let total = host.map_or(0, |host| host.pane.held.total);
        let (Some(host), Ok(index)) = (host, usize::try_from(index)) else {
            return Some(Reply::value("{}"));
        };
        if index >= total {
            return Some(Reply::value("{}"));
        }
        let list = host.list.borrow();
        let held = &host.pane.held;
        let listed = matches!(host.drawn_view, Some(View::List | View::Dual));
        let row = |at: usize| {
            let facts = held.row(at);
            let rect = if listed {
                super::rows::drawn_row_rect(&list, held, at)
            } else {
                String::new()
            };
            (
                facts.map_or("", |facts| facts.name.as_str()),
                facts.is_some_and(|facts| facts.dir),
                rect,
            )
        };
        let (name, directory, rect) = row(index);
        let (last_name, _, last_rect) = row(total - 1);
        let view = match host.drawn_view {
            Some(View::List | View::Dual) => "list",
            Some(View::Grid) => "grid",
            Some(View::Columns) => "columns",
            None => "",
        };
        Some(Reply::value(format!(
            "{{\"side\":{side},\"active\":{},\"focused\":{},\"path\":\"{}\",\"view\":\"{view}\",\"loading\":{},\"total\":{total},\"area\":\"{}\",\"folder\":{{\"index\":{index},\"name\":\"{}\",\"directory\":{directory},\"rect\":\"{rect}\"}},\"last\":{{\"index\":{},\"name\":\"{}\",\"rect\":\"{last_rect}\"}}}}",
            self.shell.view == View::Dual,
            self.shell.focus_side as i64 == side,
            escape(&host.path),
            host.pane.busy(),
            if listed { super::rows::list_area_rect(&list) } else { String::new() },
            escape(name),
            total - 1,
            escape(last_name),
        )))
    }
}

impl crate::state::Shell {
    pub(crate) fn dual_adapter(&self, window: &gpui::Window, cx: &gpui::App) -> DualAdapter<'_> {
        DualAdapter {
            active: self.view == crate::window::startup::View::Dual,
            focused: self.focus_side,
            panes: std::array::from_fn(|side| {
                self.panes[side].as_ref().map(|host| DualPane {
                    path: &host.path,
                    total: host.pane.held.total,
                    cursor: host.pane.cursor,
                    loading: host.pane.busy(),
                    selected: &host.selected,
                    focused: side == self.focus_side
                        && (side == 0 || self.view == crate::window::startup::View::Dual)
                        && !self.focus_view(window, cx).is_empty(),
                    list_requests: host.pane.list_requests,
                })
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dual_state_matches_the_oracle_shape() {
        let adapter = DualAdapter {
            active: true,
            focused: 1,
            panes: [
                Some(DualPane {
                    path: "/left",
                    total: 9,
                    cursor: 0,
                    loading: false,
                    selected: &[0, 2],
                    focused: false,
                    list_requests: 3,
                }),
                Some(DualPane {
                    path: "/right",
                    total: 4,
                    cursor: 1,
                    loading: true,
                    selected: &[],
                    focused: true,
                    list_requests: 1,
                }),
            ],
        };
        let reply = adapter.call("dualState", &[]).expect("dualState answers");
        assert_eq!(
            reply.stdout,
            "{\"active\":true,\"focused\":1,\"panes\":[\
            {\"path\":\"/left\",\"total\":9,\"cursor\":0,\"loading\":false,\"selected\":[0,2],\"focused\":false,\"listRequests\":3},\
            {\"path\":\"/right\",\"total\":4,\"cursor\":1,\"loading\":true,\"selected\":[],\"focused\":true,\"listRequests\":1}]}\n"
        );
    }

    #[test]
    fn hidden_dual_reports_a_null_second_pane() {
        let adapter = DualAdapter {
            active: false,
            focused: 0,
            panes: [
                Some(DualPane {
                    path: "/left",
                    total: 0,
                    cursor: 0,
                    loading: true,
                    selected: &[],
                    focused: true,
                    list_requests: 1,
                }),
                None,
            ],
        };
        let reply = adapter.call("dualState", &[]).expect("dualState answers");
        assert!(
            reply.stdout.contains("\"panes\":[{") && reply.stdout.ends_with(",null]}\n"),
            "second pane null, got {}",
            reply.stdout
        );
        assert!(adapter.call("path", &[]).is_none());
    }
}

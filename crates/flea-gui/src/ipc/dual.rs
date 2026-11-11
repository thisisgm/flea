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

fn pane_geometry(
    host: &crate::window::pane::PaneHost,
    index: usize,
    last: usize,
) -> (String, String, String) {
    use super::rows::rect_of;
    use crate::{columns::geometry::Slot, window::startup::View};

    match host.drawn_view {
        Some(View::List | View::Dual) => {
            let list = host.list.borrow();
            (
                super::rows::list_area_rect(&list),
                super::rows::drawn_row_rect(&list, &host.pane.held, index),
                super::rows::drawn_row_rect(&list, &host.pane.held, last),
            )
        }
        Some(View::Grid) => {
            let grid = super::grid_rows::MountedGrid {
                grid: host.grid.as_ref(),
                active: true,
            };
            let value = |name, arguments: &[String]| {
                grid.call(name, arguments)
                    .map(|reply| reply.stdout.trim_end_matches('\n').to_owned())
                    .unwrap_or_default()
            };
            (
                value("listAreaRect", &[]),
                value("rowRect", &[index.to_string()]),
                value("rowRect", &[last.to_string()]),
            )
        }
        Some(View::Columns) => {
            let paint = host
                .columns
                .as_ref()
                .and_then(|columns| columns.painters.get(Slot::Active))
                .map(|paint| paint.borrow());
            let Some(paint) = paint else {
                return Default::default();
            };
            let Some(drawn) = &paint.drawn else {
                return Default::default();
            };
            let bounds = drawn.bounds;
            let width = drawn
                .tracks
                .iter()
                .map(|view| view.x + view.width)
                .fold(0., f32::max);
            let row = |index| {
                if !paint.visible().any(|at| at == index)
                    || drawn
                        .held
                        .as_ref()
                        .is_none_or(|held| held.listing != host.pane.held.listing)
                {
                    return String::new();
                }
                let Some(position) = drawn.position_of(index) else {
                    return String::new();
                };
                let row = drawn.row_height;
                rect_of(
                    f32::from(bounds.origin.x),
                    f32::from(bounds.origin.y) + drawn.grow.top(position, row) - drawn.scroll,
                    (f32::from(bounds.size.width) - drawn.lane).max(0.),
                    drawn.grow.height(position, row),
                )
            };
            (
                rect_of(
                    f32::from(bounds.origin.x) - drawn.view().x,
                    f32::from(bounds.origin.y),
                    width,
                    f32::from(bounds.size.height),
                ),
                row(index),
                row(last),
            )
        }
        None => Default::default(),
    }
}

impl Adapter for DragGeometry<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        // One pane's list area, "x y width height", whether or not it lists a row; "" for a pane never built.
        if function == "paneListAreaRect" {
            let Some(side) = arguments
                .first()
                .and_then(|side| side.trim().parse::<usize>().ok())
                .filter(|side| *side < self.shell.panes.len())
            else {
                return Some(Reply::error(
                    2,
                    format!("FLEA_IPC_ERROR flea.paneListAreaRect side must be 0 or 1, got {arguments:?}"),
                ));
            };
            let host = self.shell.panes[side].as_ref();
            return Some(Reply::value(
                host.map_or_else(String::new, |host| pane_geometry(host, 0, 0).0),
            ));
        }
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
        let held = &host.pane.held;
        let (area, rect, last_rect) = pane_geometry(host, index, total - 1);
        let row = |at: usize| {
            let facts = held.row(at);
            (
                facts.map_or("", |facts| facts.name.as_str()),
                facts.is_some_and(|facts| facts.dir),
            )
        };
        let (name, directory) = row(index);
        let (last_name, _) = row(total - 1);
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
            area,
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
#[path = "dual_geometry_tests.rs"]
mod geometry_tests;

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

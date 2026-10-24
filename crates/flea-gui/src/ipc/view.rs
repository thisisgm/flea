//! Readers for the mounted List, using the bounded snapshot and geometry its last frame drew.
use super::{rows::RowsAdapter, Adapter};
use crate::list::{ListState, RowStates};
use crate::window::startup::View;
use flea::ipc::wire::Reply;

pub struct ViewAdapter<'a> {
    pub list: &'a ListState,
    /// The view the window drew for this pane; the List is the element it draws in every view until another view mounts its own.
    pub mode: Option<View>,
    /// The owner is drawing the List rather than an empty, loading or failed pane surface.
    pub shown: bool,
}

impl Adapter for ViewAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let list = self.list;
        let drawn = &list.drawn;
        let mounted = matches!(self.mode, Some(View::List | View::Dual));
        if !mounted && !matches!(function, "viewMode" | "rowAt") {
            return None;
        }
        let ready = mounted && self.shown && drawn.frames > 0;
        let value = match function {
            "viewMode" => match self.mode {
                Some(View::List | View::Dual) => "list",
                Some(View::Grid) => "grid",
                Some(View::Columns) => "columns",
                None => "",
            }
            .to_owned(),
            "drawnCount" => if ready { list.drawn_rows.total } else { 0 }.to_string(),
            "visibleRows" => {
                if drawn.frames == 0 {
                    return None;
                }
                ((f32::from(drawn.bounds.size.height) / drawn.metrics.row)
                    .ceil()
                    .max(1.) as usize)
                    .to_string()
            }
            "viewEndY" => {
                if drawn.frames == 0 {
                    return None;
                }
                let count = if ready { list.drawn_rows.total } else { 0 };
                let end = count as f32 * drawn.metrics.row - f32::from(drawn.bounds.size.height);
                ((end + 0.5).floor() as i64).to_string()
            }
            "visibleRowName" => {
                let index = arguments.first()?.parse::<usize>().ok();
                index
                    .filter(|_| ready)
                    .filter(|&index| {
                        list.visible()
                            .iter()
                            .any(|row| row.index == index && row.prepared.is_some())
                    })
                    .and_then(|index| list.drawn_rows.row(index))
                    .map(|row| row.name.clone())
                    .unwrap_or_default()
            }
            "rowAt" => {
                if !ready {
                    return Some(Reply::value("loading"));
                }
                return RowsAdapter {
                    list,
                    held: &list.drawn_rows,
                    states: &RowStates::default(),
                }
                .call(function, arguments);
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

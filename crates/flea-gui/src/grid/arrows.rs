/// Grid arrows and range-extension actions; Page, Home and End are shared cursor moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    Up,
    Down,
    Left,
    Right,
    ExtendLeft,
    ExtendRight,
    ExtendUp,
    ExtendDown,
}

/// The default no-wrap policy retained for existing grid navigation callers.
pub fn arrow(columns: usize, view: Option<usize>, total: usize, step: Arrow) -> Option<usize> {
    arrow_with_wrap(columns, view, total, step, false)
}

/// Grid.js arrow: the view position an arrow lands on, None when held at a row edge or listing end.
pub fn arrow_with_wrap(
    columns: usize,
    view: Option<usize>,
    total: usize,
    step: Arrow,
    wrap_rows: bool,
) -> Option<usize> {
    let (view, columns) = (view?, columns.max(1));
    let next = match step {
        Arrow::Down | Arrow::ExtendDown => view.checked_add(columns)?,
        Arrow::Up | Arrow::ExtendUp => view.checked_sub(columns)?,
        Arrow::Left | Arrow::ExtendLeft if view % columns == 0 => {
            if !wrap_rows || view == 0 {
                return None;
            }
            let previous_start = view - columns;
            previous_start + columns.min(total - previous_start) - 1
        }
        Arrow::Left | Arrow::ExtendLeft => view - 1,
        Arrow::Right | Arrow::ExtendRight if view + 1 == total => return None,
        Arrow::Right | Arrow::ExtendRight if (view + 1) % columns == 0 && !wrap_rows => {
            return None;
        }
        Arrow::Right | Arrow::ExtendRight => view + 1,
    };
    (next < total).then_some(next)
}

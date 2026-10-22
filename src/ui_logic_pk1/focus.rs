// The chooser's own Tab ring: Back, Up, the rail, the view, Filename, the filter, Cancel and the answer.

// One focus stop; a hidden, disabled or untabbable stop is skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stop<T> {
    pub item: T,
    pub visible: bool,
    pub enabled: bool,
    pub tab: bool,
}

pub fn tab_order<T: Copy>(
    chrome: &[Stop<T>],
    rail: Stop<T>,
    view: Stop<T>,
    save: &[Stop<T>],
    answers: &[Stop<T>],
) -> Vec<T> {
    chrome
        .iter()
        .chain([&rail, &view])
        .chain(save)
        .chain(answers)
        .filter(|stop| stop.visible && stop.enabled && stop.tab)
        .map(|stop| stop.item)
        .collect()
}

// No origin lands on Cancel when it is a stop, because a submission or a lost backend disables the rest.
pub fn step_focus<T: Copy + PartialEq>(
    order: &[T],
    from: Option<T>,
    back: bool,
    cancel: T,
) -> Option<T> {
    if order.is_empty() {
        return None;
    }
    let Some(from) = from else {
        return Some(if order.contains(&cancel) {
            cancel
        } else {
            order[0]
        });
    };
    let len = order.len() as i64;
    let at = order
        .iter()
        .position(|item| *item == from)
        .map_or(-1, |at| at as i64);
    let next = (at + if back { -1 } else { 1 } + len).rem_euclid(len);
    Some(order[next as usize])
}

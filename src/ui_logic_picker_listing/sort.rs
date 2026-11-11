//! Picker sort remembering over three orders, decided windowless.
pub const PICKER_ORDERS: [&str; 3] = ["name", "size", "mtime"];

// One column step: new column ascends, sorted reverses, unoffered asks nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerOrder {
    pub key: String,
    pub desc: bool,
}

pub fn picker_column_order(current: &PickerOrder, key: &str) -> Option<PickerOrder> {
    if !PICKER_ORDERS.contains(&key) {
        return None;
    }
    if current.key == key {
        return Some(PickerOrder {
            key: key.to_owned(),
            desc: !current.desc,
        });
    }
    Some(PickerOrder {
        key: key.to_owned(),
        desc: false,
    })
}

// The `s` step: name, size, modified, then name; never kind.
pub fn picker_next_order(current: &PickerOrder) -> PickerOrder {
    let at = PICKER_ORDERS.iter().position(|order| *order == current.key).unwrap_or(PICKER_ORDERS.len() - 1);
    let key = PICKER_ORDERS[(at + 1) % PICKER_ORDERS.len()];
    PickerOrder {
        key: key.to_owned(),
        desc: false,
    }
}

// The `S` step reverses the current order.
pub fn picker_reverse_order(current: &PickerOrder) -> PickerOrder {
    PickerOrder {
        key: current.key.clone(),
        desc: !current.desc,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortEffect {
    pub sort_by: String,
    pub sort_desc: bool,
    // The folder entry; absent when off, recent, or pathless.
    pub remember: Option<(String, String, bool)>,
    // The default leaf: mtime under the date key.
    pub leaf_key: String,
    pub leaf_reverse: bool,
}

// A user sort; None asks nothing (unsortable or unchanged).
pub fn request_sort(current: (&str, bool), sortable: bool, recent: bool, path: &str, remember_sort: bool, order: (&str, bool)) -> Option<SortEffect> {
    if !sortable || current == order {
        return None;
    }
    let (key, desc) = order;
    let remember = if !recent && !path.is_empty() && remember_sort {
        Some((path.to_owned(), key.to_owned(), desc))
    } else {
        None
    };
    Some(SortEffect {
        sort_by: key.to_owned(),
        sort_desc: desc,
        remember,
        leaf_key: if key == "mtime" { "date".to_owned() } else { key.to_owned() },
        leaf_reverse: desc,
    })
}

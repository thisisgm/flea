use super::{
    model::Model,
    wire::{word, Wire},
};
use crate::jsondoc::Json;
use std::io;

pub fn act(m: &mut Model, action: &str, w: &mut Wire) -> io::Result<()> {
    // The cursor's logical row: the loaded row, or in a listed/rows gap the anchor still waited for.
    let anchor = m.current_path().or_else(|| m.sort_anchor.clone());
    if action == "sortReverse" {
        m.reverse = !m.reverse;
    } else if action != "sortNext" {
        m.sort = if action.starts_with("sortBirth") {
            m.detail.mode = super::detail::Column::Birth;
            "btime"
        } else if action == "sortExtension" {
            "extension"
        } else if action == "sortRandom" {
            "random"
        } else if action.starts_with("sortDate") {
            "date"
        } else if action.starts_with("sortSize") {
            "size"
        } else {
            "name"
        }
        .into();
        m.reverse = action.ends_with("Reverse");
    } else {
        m.sort = match m.sort.as_str() {
            "name" => "size",
            "size" => "date",
            "date" => "kind",
            _ => "name",
        }
        .into();
        m.reverse = false;
    }
    let mut request = vec![
        ("c", word("sort")),
        ("by", word(&m.sort)),
        ("foldersByName", Json::Bool(m.sort == "size")),
        ("desc", Json::Bool(m.reverse)),
        ("foldersFirst", Json::Bool(m.folders_first)),
        ("groupByKind", Json::Bool(m.group_by_kind)),
    ];
    if let Some(anchor) = anchor.as_ref() {
        request.push(("anchor", word(&anchor.to_string_lossy())));
    }
    w.send(request)?;
    m.sort_anchor = anchor;
    m.invalidate_rows();
    if crate::uischema::SORT_KEYS.contains(&m.sort.as_str()) {
        super::actions::save(
            "sort",
            Json::Obj(vec![
                ("key".into(), word(&m.sort)),
                ("reverse".into(), Json::Bool(m.reverse)),
            ]),
            m,
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "sort_store_tests.rs"]
mod tests;

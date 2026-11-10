//! One immutable source listing survives unsolicited answers until its client remaps it.
use super::{
    listing::Listing,
    state::{Held, State},
};
use flea::{bytename::ExactText as _, jsondoc::Json};
use std::path::PathBuf;

#[derive(Default)]
pub(crate) struct Remap {
    gen: usize,
    scope: PathBuf,
    previous: Option<(u64, Listing)>,
}

#[test]
fn proto_search_remap_preserves_source_stamp_and_off_window_rows() {
    match super::proto::parse_request(
        r#"{"c":"searchremap","gen":7,"from":12,"rows":[0,900],"cursor":900}"#,
    ) {
        super::proto::Request::SearchRemap {
            gen,
            from,
            rows,
            cursor,
        } => {
            assert_eq!((gen, from, cursor), (7, 12, 900));
            assert_eq!(rows, [0, 900]);
        }
        _ => panic!("searchremap must parse as its own request"),
    }
}

pub(crate) fn freeze(st: &mut State) {
    if st.held == Held::Walk && st.search_remap.gen > 0 && st.search_remap.previous.is_none() {
        super::run::extend_search_listing(st, usize::MAX);
        st.search_remap.previous = Some((st.generation, st.listing.clone()));
    }
}

pub(crate) fn landing(st: &mut State, gen: usize, scope: &std::path::Path) {
    if st.search_remap.gen != gen || st.search_remap.scope != scope {
        st.search_remap.previous = None;
    }
    st.search_remap.gen = gen;
    st.search_remap.scope = scope.to_owned();
}

pub(crate) fn answer(
    st: &mut State,
    gen: usize,
    from: usize,
    rows: &[usize],
    cursor: usize,
) -> String {
    let source = st
        .search_remap
        .previous
        .as_ref()
        .filter(|(stamp, _)| *stamp == from as u64);
    let valid = gen > 0
        && gen == st.search_remap.gen
        && st.held == Held::Walk
        && st.base == st.search_remap.scope
        && source.is_some();
    let mut mapped = Vec::new();
    let mut pointer = Json::Null;
    if valid {
        let source = &source.unwrap().1;
        let paths: Vec<String> = rows
            .iter()
            .copied()
            .chain(std::iter::once(cursor))
            .filter(|&row| row < source.len())
            .map(|row| source.path(&st.base, row).exact_text().into_owned())
            .collect();
        let found: std::collections::HashMap<_, _> = st
            .listing
            .indices_of(&st.base, &paths)
            .into_iter()
            .collect();
        for &row in rows {
            if row < source.len() {
                let path = source.path(&st.base, row).exact_text().into_owned();
                if let Some(at) = found.get(path.as_str()) {
                    mapped.push(*at);
                }
            }
        }
        if cursor < source.len() {
            let path = source.path(&st.base, cursor).exact_text().into_owned();
            if let Some(at) = found.get(path.as_str()) {
                pointer = Json::Num(at.to_string());
            }
        }
        mapped.sort_unstable();
        mapped.dedup();
        // The client now holds this listing, and the next index change snapshots it again before mutating.
        st.search_remap.previous = None;
    }
    flea::jsondoc::render(&Json::Obj(vec![
        ("t".into(), Json::Str("searchremap".into())),
        ("ok".into(), Json::Bool(valid)),
        ("gen".into(), Json::Num(gen.to_string())),
        ("from".into(), Json::Num(from.to_string())),
        ("listing".into(), Json::Num(st.generation.to_string())),
        (
            "rows".into(),
            Json::Arr(
                mapped
                    .into_iter()
                    .map(|at| Json::Num(at.to_string()))
                    .collect(),
            ),
        ),
        ("cursor".into(), pointer),
    ]))
    .replace('\n', "")
}

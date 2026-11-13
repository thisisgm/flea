//! Optional per-composition census of tracks that actually painted, including missing and empty tracks.
use crate::columns::{geometry::Slot, paint::Painters, TrackRows, TrackView};
use gpui::{canvas, prelude::*, px, AnyElement, IntoElement};
use std::sync::LazyLock;

#[cfg(test)]
thread_local! {
    static RECORDS: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

fn enabled() -> bool {
    #[cfg(test)]
    if RECORDS.with(|records| records.borrow().is_some()) {
        return true;
    }
    static ENABLED: LazyLock<bool> =
        LazyLock::new(|| std::env::var("FLEA_COLUMNS_CENSUS").is_ok_and(|value| value == "1"));
    *ENABLED
}

pub(super) fn surface(path: &str, painters: &Painters, tracks: &[TrackView]) -> Option<AnyElement> {
    if !enabled() {
        return None;
    }
    let path = path.to_owned();
    let snapshots: Vec<_> = tracks
        .iter()
        .filter_map(|track| {
            let paint = painters.get(track.slot)?;
            Some((track.slot, paint.borrow().paint_serial, paint.clone()))
        })
        .collect();
    Some(
        canvas(
            |_, _, _| (),
            move |_, _, _, _| {
                let mut columns = Vec::new();
                let mut third = false;
                for (slot, serial, paint) in &snapshots {
                    let paint = paint.borrow();
                    let Some(drawn) = paint.drawn.as_ref().filter(|drawn| {
                        paint.paint_serial != *serial
                            && drawn.bounds.size.width > px(0.)
                            && drawn.bounds.size.height > px(0.)
                    }) else {
                        continue;
                    };
                    let rows: Vec<_> = paint
                        .visible()
                        .filter(|index| {
                            drawn.position_of(*index).is_some_and(|position| {
                                let y = drawn.grow.top(position, drawn.row_height) - drawn.scroll;
                                y < f32::from(drawn.bounds.size.height)
                                    && y + drawn.grow.height(position, drawn.row_height) > 0.
                            })
                        })
                        .collect();
                    let name = rows.first().and_then(|index| match &drawn.view().rows {
                        TrackRows::Listing => drawn.held.as_ref()?.row(*index).map(|row| row.name.as_str()),
                        TrackRows::Peek(rows) => rows.get(*index).map(|row| row.name.as_str()),
                        _ => None,
                    }).unwrap_or("");
                    let total = match &drawn.view().rows {
                        TrackRows::Listing => drawn.shown.as_ref().map_or_else(
                            || drawn.held.as_ref().map_or(0, |held| held.total),
                            |rows| rows.len(),
                        ),
                        TrackRows::Peek(rows) => rows.len(),
                        _ => 0,
                    };
                    let x = f32::from(drawn.bounds.origin.x);
                    columns.push((x, format!(
                        r#"{{"x":{x},"rows":{},"total":{total},"firstName":"{}","path":"{}","listing":{}}}"#,
                        rows.len(), flea::json::escape(name), flea::json::escape(&drawn.view().path),
                        drawn.held.as_ref().map_or(0, |held| held.listing),
                    )));
                    third |= *slot == Slot::Child;
                }
                columns.sort_by(|left, right| left.0.total_cmp(&right.0));
                let columns = columns.into_iter().map(|(_, json)| json).collect::<Vec<_>>().join(",");
                let json = format!(r#"{{"path":"{}","columns":[{columns}],"hasThird":{third}}}"#, flea::json::escape(&path));
                #[cfg(test)]
                RECORDS.with(|records| {
                    if let Some(records) = records.borrow_mut().as_mut() {
                        records.push(json.clone());
                    }
                });
                eprintln!("COLHOLD FRAME {json}");
            },
        )
        .absolute()
        .size_full()
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::columns_tests::host;
    use flea::jsondoc::{parse, Json};

    struct Recording;
    impl Drop for Recording {
        fn drop(&mut self) {
            RECORDS.with(|records| *records.borrow_mut() = None);
        }
    }

    fn latest() -> Json {
        RECORDS.with(|records| parse(records.borrow().as_ref().unwrap().last().unwrap()).unwrap())
    }

    #[test]
    fn fx_uish_columns_census_counts_painted_rows_and_excludes_a_cached_hidden_child() {
        RECORDS.with(|records| *records.borrow_mut() = Some(Vec::new()));
        let _recording = Recording;
        let mut cx = host::context();
        let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
        host::change(&mut cx, window, host::settle);
        host::frame(&mut cx, window);
        let complete = latest();
        let columns = complete.get("columns").and_then(Json::as_array).unwrap();
        assert_eq!(columns.len(), 3, "{complete:?}");
        assert!(columns
            .iter()
            .all(|column| column.get("rows").and_then(Json::as_f64).unwrap() > 0.));
        assert_eq!(complete.get("hasThird"), Some(&Json::Bool(true)));
        host::change(&mut cx, window, |shell, _, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            let session = &mut host.search.owner.session;
            session.mode = flea::ui_logic_file_search::Mode::Typing;
            session.gen = 1;
            session.query = "missing".into();
            session.running = false;
            host.pane.state.search = true;
            host.pane.state.query = "missing".into();
            host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
                listing: host.pane.held.listing,
                ..Default::default()
            });
            assert!(shell.no_match_headline(0).is_some());
            cx.notify();
        });
        host::frame(&mut cx, window);
        let empty = latest();
        let columns = empty.get("columns").and_then(Json::as_array).unwrap();
        assert_eq!(
            columns.len(),
            2,
            "cached third track is not painted: {empty:?}"
        );
        assert_eq!(empty.get("hasThird"), Some(&Json::Bool(false)));
        assert!(columns
            .iter()
            .any(|column| column.get("rows").and_then(Json::as_f64) == Some(0.)));
    }
}

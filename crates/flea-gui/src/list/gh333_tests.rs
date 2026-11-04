//! Issue #333 on the mounted List: the reported ui.json, read through the real store, on the production Shell.
#[path = "gh333_harness.rs"]
mod harness;

use crate::{
    list::{bench, columns::Column},
    state::Shell,
};
use flea::{
    jsondoc::{self, Json},
    uistore::Store,
};
use gpui::{px, size, Entity, VisualTestContext};
use harness::{ask, bytes, document, draw, drawn, open, root, unrelated, Drawn};

#[test]
fn sol_gh374_375_f4_fits_enabled_columns_dropped_at_a_narrow_width() {
    let mut cx = bench::real_cx();
    let data = jsondoc::parse(&document(r#"{"mode":"system"}"#)).unwrap();
    let (shell, vcx, _done) = open(&mut cx, data, 440.);
    let before = drawn(&shell, vcx);
    assert!(!before.row_names.split(',').any(|key| key == "kind"));
    assert_eq!(before.stored[Column::Kind.slot()], Some(295));
    vcx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    vcx.simulate_keystrokes("f4");
    draw(vcx);
    let fitted = drawn(&shell, vcx);
    assert!(
        fitted.stored[Column::Kind.slot()].is_some_and(|width| width < 295),
        "F4 left an enabled but dropped Kind column unfitted: {:?}",
        fitted.stored
    );
    assert_eq!(
        fitted.stored[Column::Mode.slot()],
        Some(480),
        "hidden Mode changed"
    );
    assert_eq!(fitted.sent, 1, "one F4 must write one widths patch");
    vcx.simulate_resize(size(px(1580.), px(900.)));
    vcx.run_until_parked();
    draw(vcx);
    assert_eq!(drawn(&shell, vcx).row_names, "name,size,date,kind");
    vcx.update(|window, cx| {
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let mut list = host.list.borrow_mut();
            list.view.set_recent(true);
            for column in [Column::Mode, Column::Kind] {
                let before = list.view.stored.get(column);
                assert!(list.autofit(column, &host.pane.held, window).is_none());
                assert_eq!(
                    list.view.stored.get(column),
                    before,
                    "Recent fitted {column:?}"
                );
            }
        })
    });
}

// One face, one window width: the mounted header and rows agree, the name keeps its floor, and only a column whose minimum no longer fits drops, in the oracle's order.
fn check(shell: &Entity<Shell>, vcx: &mut VisualTestContext, width: f32, face: &str) -> Drawn {
    vcx.simulate_resize(size(px(width), px(900.)));
    vcx.run_until_parked();
    draw(vcx);
    let d = drawn(shell, vcx);
    let at = format!("{face} window {width}");
    assert_eq!(
        d.header_names, d.row_names,
        "{at}: header and rows drew different columns"
    );
    assert_eq!(
        d.stored,
        [Some(480), Some(480), Some(480), Some(295)],
        "{at}: a resize rewrote the stored widths"
    );
    assert_eq!(d.sent, 0, "{at}: a resize wrote preferences");
    // Below the name's own slot no column is left to give room, so the floor holds wherever it can.
    if d.row >= d.base {
        assert!(
            d.name.1 >= d.name_min,
            "{at}: the name fell below its floor: {} < {}",
            d.name.1,
            d.name_min
        );
    }
    assert!(
        !d.row_names.contains("mode"),
        "{at}: a switched-off column drew"
    );
    // A column is drawn exactly when the switched-on minimums, kind dropping first, still fit the row.
    let on = [Column::Size, Column::Date, Column::Kind];
    let mut expect = vec!["name"];
    for (n, column) in on.iter().enumerate() {
        let need = d.base
            + on[..=n]
                .iter()
                .map(|c| d.least[c.slot()] + d.gap)
                .sum::<f32>();
        if need <= d.row {
            expect.push(column.key());
        }
    }
    assert_eq!(
        d.row_names,
        expect.join(","),
        "{at}: wrong drawn set for a {} px row",
        d.row
    );
    for column in Column::ALL {
        let Some((x, w)) = d.cells[column.slot()] else {
            assert_eq!(
                ask(shell, vcx, "headerCellRect", &[column.key()])
                    .split('|')
                    .nth(1),
                Some("0")
            );
            continue;
        };
        assert!(
            w + 0.5 >= d.least[column.slot()],
            "{at}: {column:?} shrank under its minimum"
        );
        assert!(
            x >= d.name.0 + d.name.1,
            "{at}: {column:?} overlaps the name"
        );
        // The header title cell is the row cell, to the pixel the probe rounds to.
        assert_eq!(
            ask(shell, vcx, "headerCellRect", &[column.key()]),
            format!("{}|{}", x.round() as i32, w.round() as i32),
            "{at}: header and row cells differ for {column:?}"
        );
    }
    assert_eq!(
        ask(shell, vcx, "columnSet", &["0"]),
        format!("{0}|{0}", d.row_names),
        "{at}: the probe's header and row sets differ"
    );
    // Every drawn row cell holds its painted text: nothing overflows a shrunk column.
    for index in ["0", "1", "10"] {
        assert_eq!(
            ask(shell, vcx, "rowCellOverflow", &[index]),
            "0|0|0|0",
            "{at}: row {index} overflows"
        );
        assert!(
            !ask(shell, vcx, "rowNameText", &[index]).is_empty(),
            "{at}: row {index} drew no name"
        );
    }
    d
}

#[test]
fn gh333_reported_widths_on_the_mounted_list_shrink_drop_restore_and_never_rewrite_ui_json() {
    for (face, text_size, base) in [
        ("text14", r#"{"mode":"system"}"#, 14.),
        ("text20", r#"{"mode":20}"#, 20.),
    ] {
        let root = root();
        let (state, config) = (root.0.join("state"), root.0.join("config"));
        let store = Store::at(&state, &config);
        let file = state.join("flea/ui.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, document(text_size)).unwrap();
        // The launcher settles the file before the window reads it, as main.rs does.
        store.settle().unwrap();
        let settled = bytes(&file);
        let data = store.read();
        assert_eq!(
            jsondoc::render(data.get("columnWidths").unwrap()),
            jsondoc::render(
                &jsondoc::parse(r#"{"size":480,"kind":295,"date":480,"mode":480}"#).unwrap()
            ),
            "{face}: the store's reader rewrote the reported widths"
        );
        let before = unrelated(&data);

        let mut cx = bench::real_cx();
        let (shell, vcx, done) = open(&mut cx, data.clone(), 1580.);
        assert_eq!(
            ask(&shell, vcx, "listAreaRect", &[]).split(' ').next(),
            Some("192"),
            "{face}: the 192 px sidebar is not where the list starts"
        );
        let wide = check(&shell, vcx, 1580., face);
        assert_eq!(wide.base_size, base, "{face}: wrong face");
        assert_eq!(
            wide.row_names, "name,size,date,kind",
            "{face}: a switched-on column dropped at 1580"
        );
        assert!(
            wide.cells.iter().flatten().any(|(_, w)| *w < 480.),
            "{face}: nothing shrank at 1580, so the fixture proves nothing"
        );
        // Narrowing walks every drop the face reaches; widening returns each column, then every stored width.
        let mut seen = vec![wide.row_names.clone()];
        for width in [1100., 900., 760., 680., 620., 560., 520., 480., 440.] {
            let d = check(&shell, vcx, width, face);
            if seen.last() != Some(&d.row_names) {
                seen.push(d.row_names.clone());
            }
        }
        let order: Vec<&str> = seen.iter().map(String::as_str).collect();
        let full = ["name,size,date,kind", "name,size,date", "name,size", "name"];
        assert!(
            full.starts_with(&order) && order.len() >= 3,
            "{face}: drops out of the oracle's order or too few reached: {order:?}"
        );
        for width in [620., 1100., 1580.] {
            check(&shell, vcx, width, face);
        }
        let back = check(&shell, vcx, 3200., face);
        assert_eq!(
            back.cells.map(|cell| cell.map(|(_, w)| w)),
            [None, Some(480.), Some(480.), Some(295.)],
            "{face}: a wide window did not get every stored width back"
        );
        assert_eq!(bytes(&file), settled, "{face}: resizing changed ui.json");

        // Reopen from the same file: the first frame is the same layout, and the file is still untouched.
        let again = store.read();
        assert_eq!(unrelated(&again), before);
        drop(shell);
        let mut cx2 = bench::real_cx();
        let (shell2, vcx2, done2) = open(&mut cx2, again, 1580.);
        let reopened = check(&shell2, vcx2, 1580., face);
        assert_eq!(
            reopened.cells, wide.cells,
            "{face}: reopen drew different cells"
        );
        assert_eq!(
            reopened.name, wide.name,
            "{face}: reopen drew a different name cell"
        );
        assert_eq!(bytes(&file), settled, "{face}: reopening changed ui.json");

        // A remembered edge writes one columnWidths key; merged by the real store, every other key stays.
        let size = reopened.cells[Column::Size.slot()].unwrap();
        let start = shell2.read_with(vcx2, |shell, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let header = list.header.drawn.bounds;
            gpui::point(header.left() + px(size.0), header.center().y)
        });
        vcx2.simulate_mouse_move(start, None, gpui::Modifiers::default());
        vcx2.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        let finish = start + gpui::point(px(-30.), px(0.));
        vcx2.simulate_mouse_move(
            finish,
            Some(gpui::MouseButton::Left),
            gpui::Modifiers::default(),
        );
        vcx2.simulate_mouse_up(finish, gpui::MouseButton::Left, gpui::Modifiers::default());
        draw(vcx2);
        let patches = shell2.update(vcx2, |shell, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .nav
                .writer
                .borrow_mut()
                .owed()
                .into_iter()
                .filter(|patch| !harness::session_only(patch))
                .collect::<Vec<_>>()
        });
        assert_eq!(
            patches.len(),
            1,
            "{face}: one drag wrote {} patches",
            patches.len()
        );
        let written = (size.1 + 30.).round() as u32;
        assert_eq!(
            jsondoc::render(&patches[0]),
            jsondoc::render(
                &jsondoc::parse(&format!(r#"{{"columnWidths":{{"size":{written}}}}}"#)).unwrap()
            ),
            "{face}: the drag patch"
        );
        let merged = store.update(&patches[0]).unwrap();
        assert_eq!(
            unrelated(&merged),
            before,
            "{face}: the merge lost an unrelated key"
        );
        let widths = merged.get("columnWidths").unwrap();
        for (key, want) in [
            ("size", f64::from(written)),
            ("kind", 295.),
            ("date", 480.),
            ("mode", 480.),
        ] {
            assert_eq!(
                widths.get(key).and_then(Json::as_f64),
                Some(want),
                "{face}: the merge left {key} wrong"
            );
        }
        assert_eq!(unrelated(&store.read()), before);
        drop((done, done2, shell2));
    }
}

//! #388: a contents search keeps the Columns peek tracks on the density row and changes only the active pitch.
use super::density::{open, render, DENSITIES};
use super::host;
use crate::columns::geometry::Slot;
use crate::g2::sr1::contents::{BLOCK_H, ROW_H};
use crate::row::tags::SWATCH_MARK;
use flea::ui_logic_sr1::{ContentsMatch, ContentsMatches, Mode};
use gpui::px;

#[test]
fn b23_a_contents_search_changes_only_the_active_pitch() {
    let mut cx = host::context();
    let (window, _requests) = open(&mut cx, true, false);
    render(&mut cx, window, "comfortable");
    // A contents search whose lines cover every held row turns the active track into blocks.
    host::change(&mut cx, window, |shell, _, _| {
        let host_pane = shell.panes[0].as_mut().unwrap();
        let held = &host_pane.pane.held;
        let (listing, start) = (held.listing, held.start);
        let matches = (start..start + held.rows.len())
            .map(|row| ContentsMatch {
                row,
                path: format!("/srv/a/b/c/d0{row}"),
                path_bytes: Vec::new(),
                line_no: 1,
                line: "hit".into(),
                operable: true,
            })
            .collect();
        let owner = &mut host_pane.search.owner;
        owner.session.mode = Mode::Results;
        let gen = owner.session.gen;
        let batch = ContentsMatches {
            gen,
            listing: listing as u64,
            start,
            matches,
        };
        assert!(owner.matches.take(batch, gen, listing), "the lines held");
    });
    host::frame(&mut cx, window);
    let painted = crate::bfx_cols_tests::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, _, _| {
        let host_pane = shell.panes[0].as_ref().unwrap();
        let held = &host_pane.pane.held;
        let owner = &host_pane.search.owner;
        assert!(
            host_pane.list.borrow().view.contents,
            "the search held: mode {:?} covers {} listing {} start {} rows {}",
            owner.session.mode,
            owner
                .matches
                .covers(held.listing, held.start, held.rows.len()),
            held.listing,
            held.start,
            held.rows.len()
        );
        let columns = host_pane.columns.as_ref().unwrap();
        let comfortable = DENSITIES[3].1;
        assert_eq!(columns.area.row(), comfortable, "peek pitch");
        for (slot, pitch, band) in [
            (Slot::Parent, comfortable, comfortable),
            (Slot::Active, BLOCK_H, ROW_H),
            (Slot::Child, comfortable, comfortable),
        ] {
            let paint = columns.painters.get(slot).unwrap().borrow();
            let drawn = paint.drawn.as_ref().unwrap();
            assert_eq!(
                (drawn.row_height, drawn.band),
                (pitch, band),
                "{slot:?} pitch and band"
            );
            // The first row's marks centre in its band, which is the result row for the active track.
            let top = drawn.bounds.top() - px(drawn.scroll);
            let marks: Vec<_> = painted
                .iter()
                .filter(|p| p.run == 0 && p.mark != SWATCH_MARK)
                .filter(|p| {
                    p.bounds.left() >= drawn.bounds.left()
                        && p.bounds.left() < drawn.bounds.right()
                        && p.bounds.bottom() > top
                        && p.bounds.top() < top + px(pitch)
                })
                .collect();
            assert!(
                marks.iter().any(|p| !p.mark.contains("chevron")),
                "{slot:?} row 0 paints its row mark: {marks:?}"
            );
            for mark in marks {
                let mid = mark.bounds.top() + mark.bounds.size.height / 2.;
                assert_eq!(mid, top + px(band / 2.), "{slot:?} row 0 {}", mark.mark);
            }
        }
    });
}

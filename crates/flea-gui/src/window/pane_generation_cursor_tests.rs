use super::*;
use crate::input::{Band, Pull};
use flea::protocol::{Reply, RowFacts};
use gpui::{point, px, TestAppContext};
use std::sync::Arc;

fn snapshot() -> Snapshot {
    Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    }
}

fn land(host: &mut PaneHost, listing: usize, total: usize) {
    assert!(!host
        .pane
        .receive_at(
            Reply::Listed {
                path: "/files".into(),
                total,
            },
            0,
        )
        .unwrap());
    let count = total.min(host.pane.first_count());
    let rows = (0..count)
        .map(|index| RowFacts {
            name: format!("file-{index}.txt"),
            ..Default::default()
        })
        .collect();
    assert!(host
        .pane
        .receive_at(
            Reply::Rows {
                start: 0,
                rows,
                listing,
            },
            0,
        )
        .unwrap());
    assert_eq!(host.pane.held.listing, listing);
    assert_eq!(host.pane.held.total, total);
    assert!(!host.pane.busy());
}

fn active_band(host: &mut PaneHost, saved_cursor: usize, cx: &mut TestAppContext) {
    host.pane.cursor = 7;
    host.selected = vec![7, 8];
    host.input.selection.changed(&host.selected);
    host.input.band = Some(Band {
        start: point(px(10.), px(10.)),
        anchor: (10., 10.),
        end: (30., 40.),
        before: vec![saved_cursor - 1, saved_cursor],
        cursor: saved_cursor,
        active: true,
        additive: false,
        shift: false,
        direction: 1.,
        last: Some(8),
        cells: None,
        basis: None,
        pointer: point(px(30.), px(40.)),
    });
    host.input.grab = Some(2.);
    host.input.timer = Some(cx.executor().spawn(std::future::pending::<()>()));
    host.input.stepped_at = Some(cx.executor().now());
    host.input.pulled = Some(Pull {
        from: saved_cursor,
        to: 7,
        listing: host.pane.held.listing,
    });
    assert!(host.input.band.as_ref().unwrap().active);
    assert!(host.input.selection.snapshot.is_some());
    assert!(host.input.timer.is_some());
}

fn old_host(saved_cursor: usize, cx: &mut TestAppContext) -> (PaneHost, Snapshot) {
    let locale = snapshot();
    let mut host = PaneHost::new("/files".into(), None, 620., &locale);
    land(&mut host, 1, 1000);
    host.changed(&locale);
    active_band(&mut host, saved_cursor, cx);
    (host, locale)
}

fn assert_discarded(host: &PaneHost) {
    assert!(host.input.band.is_none(), "old listing band retained");
    assert!(host.input.grab.is_none(), "old listing grab retained");
    assert!(host.input.timer.is_none(), "old listing timer retained");
    assert!(host.input.stepped_at.is_none());
    assert!(host.input.pulled.is_none());
    assert!(host.input.selection.snapshot.is_none());
    assert!(host.selected.is_empty());
}

#[gpui::test]
fn replacement_two_rows_keeps_zero_instead_of_band_250(cx: &mut TestAppContext) {
    let (mut host, locale) = old_host(250, cx);
    land(&mut host, 2, 2);
    assert_eq!(host.pane.cursor, 0, "real replacement must land at zero");
    host.changed(&locale);
    assert_eq!(host.pane.cursor, 0, "F1 replacement cursor: two rows");
    assert_discarded(&host);
}

#[gpui::test]
fn replacement_five_rows_keeps_zero_instead_of_band_900(cx: &mut TestAppContext) {
    let (mut host, locale) = old_host(900, cx);
    land(&mut host, 2, 5);
    host.changed(&locale);
    assert_eq!(host.pane.cursor, 0, "F1 replacement cursor: five rows");
    assert_discarded(&host);
}

#[gpui::test]
fn replacement_empty_keeps_zero_instead_of_old_band_cursor(cx: &mut TestAppContext) {
    let (mut host, locale) = old_host(900, cx);
    land(&mut host, 2, 0);
    host.changed(&locale);
    assert_eq!(host.pane.cursor, 0, "F1 replacement cursor: empty");
    assert_discarded(&host);
}

#[gpui::test]
fn replacement_preserves_valid_current_cursor_without_clamping(cx: &mut TestAppContext) {
    let (mut host, locale) = old_host(900, cx);
    land(&mut host, 2, 5);
    host.pane.cursor = 3;
    host.changed(&locale);
    assert_eq!(host.pane.cursor, 3, "F1 replacement cursor: valid current");
    assert_discarded(&host);
}

#[gpui::test]
fn replacement_still_applies_select_target_from_new_rows(cx: &mut TestAppContext) {
    let (mut host, locale) = old_host(900, cx);
    host.select = Some("/files/file-2.txt".into());
    land(&mut host, 2, 5);
    host.changed(&locale);
    assert_eq!(host.pane.cursor, 2, "new listing select target lost");
    assert!(host.select.is_none());
    assert_eq!(
        host.selected,
        [2],
        "the select target replaces the old marks as the lone mark"
    );
    host.selected.clear();
    assert_discarded(&host);
}

#[gpui::test]
fn same_generation_changed_preserves_active_band_and_selection(cx: &mut TestAppContext) {
    let (mut host, locale) = old_host(900, cx);
    host.changed(&locale);
    let band = host.input.band.as_ref().expect("same-generation band lost");
    assert!(band.active);
    assert_eq!(band.cursor, 900);
    assert_eq!(band.before, [899, 900]);
    assert_eq!(host.pane.cursor, 7);
    assert_eq!(host.selected, [7, 8]);
    assert_eq!(host.input.selection.snapshot.as_deref(), Some(&[7, 8][..]));
    assert_eq!(host.input.grab, Some(2.));
    assert!(host.input.timer.is_some());
    assert!(host.input.stepped_at.is_some());
    assert!(host.input.pulled.is_some());
}

#[gpui::test]
fn ordinary_same_generation_cancel_restores_band_selection_and_cursor(cx: &mut TestAppContext) {
    let (mut host, _locale) = old_host(900, cx);
    host.input.cancel(&mut host.selected, &mut host.pane.cursor);
    assert_eq!(host.pane.cursor, 900);
    assert_eq!(host.selected, [899, 900]);
    assert_eq!(
        host.input.selection.snapshot.as_deref(),
        Some(&[899, 900][..])
    );
    assert!(host.input.band.is_none());
    assert!(host.input.grab.is_none());
    assert!(host.input.timer.is_none());
    assert!(host.input.stepped_at.is_none());
}

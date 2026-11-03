//! The resolved cursor survives a replacement window that contains only its viewport.
use super::*;

fn rows(start: usize, count: usize) -> Vec<RowFacts> {
    (start..start + count)
        .map(|index| RowFacts {
            name: format!("f{index}"),
            ..Default::default()
        })
        .collect()
}

#[test]
fn retained_window_preserves_the_resolved_cursor_outside_its_rows() {
    let mut pane = Pane::new("/files".into(), 620.);
    let count = pane.first_count();
    pane.receive_at(
        Reply::Listed {
            path: "/files".into(),
            total: 10000,
        },
        0,
    )
    .unwrap();
    assert!(pane
        .receive_at(
            Reply::Rows {
                start: 0,
                rows: rows(0, count),
                listing: 1
            },
            0
        )
        .unwrap());
    pane.held = Rc::new(Held {
        listing: 1,
        total: 10000,
        start: 7000,
        rows: rows(7000, count),
    });
    pane.cursor = 5000;
    pane.scroll_y = 7200. * pane.row_height();
    let options = Options {
        in_place: true,
        restore_name: Some("f5000".into()),
        ..Options::default()
    };
    assert!(pane.begin("/files".into(), options, 10));
    pane.receive_at(
        Reply::Listed {
            path: "/files".into(),
            total: 10000,
        },
        11,
    )
    .unwrap();
    let mut answer = rows(6000, count);
    answer[0].name = "f5000".into();
    assert!(!pane
        .receive_at(
            Reply::Rows {
                start: 6000,
                rows: answer,
                listing: 2
            },
            12
        )
        .unwrap());
    assert_eq!(pane.held.listing, 1, "the old frame remains available");
    let request = pane.anchor_window(8200).unwrap();
    assert_eq!(request.listing, 2);
    assert!(!pane
        .receive_at(
            Reply::Rows {
                start: 8200,
                rows: rows(8200, count),
                listing: 2
            },
            13
        )
        .unwrap());
    assert!(pane.tick(14));
    assert_eq!(
        pane.cursor, 6000,
        "the resolved cursor is not looked up in another window"
    );
    assert_eq!(pane.scroll_y, 8200. * pane.row_height());
    let first = (pane.scroll_y / pane.row_height()).floor() as usize;
    assert!(pane.held.row(first).is_some());
    assert!(pane.held.row(first + pane.visible_rows() - 1).is_some());
}

//! UISH Columns readers and travel share the geometry the native cases inspect.
use super::columns_tests::{host, scroll};

#[test]
fn uish_columns_answers_the_list_readers_from_the_active_column() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
    });
    host::frame(&mut cx, window);
    // host::call fails on a nonzero code, so a FLEA_IPC_NOT_PORTED reply fails here too.
    fn read(
        shell: &mut crate::state::Shell,
        function: &str,
        args: &[&str],
        window: &mut gpui::Window,
        cx: &mut gpui::Context<crate::state::Shell>,
    ) -> String {
        host::call(shell, function, args, window, cx)
            .stdout
            .trim()
            .to_owned()
    }
    host::change(&mut cx, window, |shell, window, cx| {
        // Pane.qml listArea is the whole ColumnsArea in Columns, so the active column lies inside it.
        let area = read(shell, "listAreaRect", &[], window, cx);
        // Sample input: "495 27 302 646" (x y width height).
        let active: Vec<f32> = read(shell, "columnActiveRect", &[], window, cx)
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        // Sample input: "192 27 908 646" (x y width height).
        let [x, y, w, h]: [f32; 4] = area
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        assert!(active[0] >= x && active[0] + active[2] <= x + w && active[2] < w);
        assert_eq!((active[1], active[3]), (y, h));
        // Sample input: "646 74" (x y).
        let centre: Vec<f32> = read(shell, "rowCentre", &["1"], window, cx)
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        assert_eq!(centre.len(), 2, "row 1 is drawn, so it has a centre");
        assert!(centre[0] >= x && centre[0] < x + w && centre[1] >= y && centre[1] < y + h);
        assert!(
            centre[0] >= active[0]
                && centre[0] < active[0] + active[2]
                && centre[1] >= active[1]
                && centre[1] < active[1] + active[3],
            "rowCentre must hit the active column"
        );
        assert!(
            read(shell, "rowCentre", &["2"], window, cx).is_empty(),
            "no row 2 is drawn"
        );
        // Sample input: "21" (row count).
        let visible: usize = read(shell, "visibleRows", &[], window, cx).parse().unwrap();
        assert!(visible >= 2);
    });
}

// scrolllane_columns at base 12: the active rows end one rowPaddingX lane short of a column in a ColumnsArea-wide listAreaRect.
#[test]
fn ey444_columns_row_rect_keeps_the_text_size_lane_inside_the_whole_area() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        let fonts = crate::presentation::fonts::Fonts::from_sources(
            "[font]\nbase-size=12",
            "",
            shell.active_typeface().font.family.to_string(),
        );
        let face = fonts.typeface(&flea::jsondoc::Json::Null);
        shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow_mut()
            .set_typeface(face);
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
    });
    host::frame(&mut cx, window);
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        let drawn = shell.panes[0].as_ref().unwrap().drawn_view;
        assert_eq!(
            drawn,
            Some(super::startup::View::Columns),
            "the base 12 frame drew Columns"
        );
        // Sample inputs: "495 27 302 646" (rect), "11 10 12 27" (metrics), or "3" (count).
        let mut numbers = |function: &str, args: &[&str]| -> Vec<i64> {
            host::call(shell, function, args, window, cx)
                .stdout
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect()
        };
        let pad = numbers("metrics", &[])[2];
        assert_eq!(pad, 12, "base 12's rowPaddingX");
        let area = numbers("listAreaRect", &[]);
        let active = numbers("columnActiveRect", &[]);
        let row = numbers("rowRect", &["0"]);
        let count = numbers("columnCount", &[])[0];
        assert_eq!(
            row.len(),
            4,
            "row 0 is drawn in the active column, so it has a rect"
        );
        assert_eq!(active.len(), 4, "area {area:?} row {row:?} count {count}");
        assert_eq!((area[1], area[3]), (active[1], active[3]));
        let col = area[2] / count;
        assert_eq!(row[0], active[0]);
        assert_eq!(
            row[2],
            active[2] - pad,
            "the row keeps the base 12 lane, not body 14's"
        );
        assert!(
            (row[2] - (col - pad)).abs() <= 1,
            "rows {} in a {col} column",
            row[2]
        );
        let right = area[0] + (count - 1) * col - pad;
        assert!(
            (row[0] + row[2] - right).abs() <= 1,
            "rows end at {}, not {right}",
            row[0] + row[2]
        );
    });
}

#[test]
fn uish_columns_answers_row_name_end_header_and_background_from_the_active_column() {
    use crate::columns::geometry::{FOOTER, LANE};
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
    });
    host::frame(&mut cx, window);
    fn read(
        shell: &mut crate::state::Shell,
        function: &str,
        args: &[&str],
        window: &mut gpui::Window,
        cx: &mut gpui::Context<crate::state::Shell>,
    ) -> String {
        host::call(shell, function, args, window, cx)
            .stdout
            .trim()
            .to_owned()
    }
    // Sample inputs: "495 27 302 646" (rect), "646 74" (centre), or "27" (scalar).
    fn numbers(text: &str) -> Vec<f32> {
        text.split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect()
    }
    host::change(&mut cx, window, |shell, window, cx| {
        let area = numbers(&read(shell, "columnActiveRect", &[], window, cx));
        let (ax, ay, aw, ah) = (area[0], area[1], area[2], area[3]);
        let row = crate::list::pane::ROW_HEIGHT;
        assert_eq!(
            numbers(&read(shell, "rowRect", &["1"], window, cx)),
            vec![ax, ay + row, aw - LANE, row],
            "row 1's whole box sits one row down, the lane left out"
        );
        assert!(
            read(shell, "rowRect", &["2"], window, cx).is_empty(),
            "no row 2 is drawn"
        );
        let (rx, rw) = (ax, aw - LANE);
        let name = numbers(&read(shell, "rowNameRect", &["1"], window, cx));
        let (nx, ny, nw, nh) = (name[0], name[1], name[2], name[3]);
        assert!(nx > rx && nx + nw < rx + rw && ny >= ay + row && ny + nh <= ay + 2. * row);
        let centre = numbers(&read(shell, "rowNameCentre", &["1"], window, cx));
        assert!(centre[0] >= nx && centre[0] < nx + nw && centre[1] >= ny && centre[1] < ny + nh);
        let host = shell.panes[0].as_ref().unwrap();
        // The pointer's own name test agrees: the name centre is on the name, the point beside it is not.
        assert!(super::columns::on_name(host, 1, gpui::px(centre[0])));
        assert!(!super::columns::on_name(
            host,
            1,
            gpui::px((nx + nw + rx + rw) / 2.)
        ));
        assert_eq!(
            numbers(&read(shell, "listingBackgroundCentre", &[], window, cx)),
            vec![(ax + aw / 2.).round(), (ay + ah / 2.).round()]
        );
        assert_eq!(read(shell, "headerTop", &[], window, cx), ay.to_string());
        assert_eq!(
            read(shell, "viewEndY", &[], window, cx),
            (2. * row + FOOTER - ah).round().to_string()
        );
    });
}

#[test]
fn uish_columns_active_track_travels_past_its_last_row_by_the_footer() {
    use crate::columns::geometry::{Slot, FOOTER};
    let mut cx = host::context();
    let handle = scroll::mounted(&mut cx);
    let height = f32::from(scroll::bounds(&mut cx, handle, Slot::Active).size.height);
    let flush = 256. * crate::list::pane::ROW_HEIGHT - height;
    host::change(&mut cx, handle, |shell, _, _| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        pane.scroll_to(f32::MAX);
        // ColumnPane.qml's footer: End parks the last row flush, the true end of travel lies a footer below it.
        assert_eq!(pane.scroll_y, flush + FOOTER);
    });
}

// tests/ui-dirsortstale.sh hiddenlast_four_columns grows the window by COUNT_4_AT less this width, so it is the whole area.
#[test]
fn hiddenlast_columns_list_area_is_the_whole_columns_area() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        // Sample input: "495 27 302 646" (x y width height).
        let rect = |function: &str,
                    shell: &mut crate::state::Shell,
                    window: &mut gpui::Window,
                    cx: &mut gpui::Context<crate::state::Shell>|
         -> Vec<f32> {
            host::call(shell, function, &[], window, cx)
                .stdout
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect()
        };
        let area = rect("listAreaRect", shell, window, cx);
        let active = rect("columnActiveRect", shell, window, cx);
        let layout = shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .area
            .layout
            .clone();
        assert!(
            layout.count >= 3,
            "the test window draws at least three tracks"
        );
        assert_eq!(area.len(), 4);
        assert_eq!(
            area[2],
            layout.width.round(),
            "the area is every track wide, not the active one"
        );
        assert!(area[2] > active[2] * 2.);
        assert_eq!(area[0], (active[0] - layout.active_x()).round());
        assert_eq!((area[1], area[3]), (active[1], active[3]));
    });
}

#[test]
fn uishlist_review_footer_follows_the_text_size() {
    use crate::columns::geometry::Slot;
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        let fonts = crate::presentation::fonts::Fonts::from_sources(
            "[font]\nbase-size=20",
            "",
            shell.active_typeface().font.family.to_string(),
        );
        shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow_mut()
            .set_typeface(fonts.typeface(&flea::jsondoc::Json::Null));
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
        cx.notify();
    });
    host::frame(&mut cx, window);
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, _, _| {
        let pane = shell.panes[0].as_ref().unwrap();
        let paint = pane
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        assert_eq!(f32::from(paint.typeface.base), 20.);
        let drawn = paint.drawn.as_ref().unwrap();
        let total = drawn
            .shown
            .as_ref()
            .map_or(pane.pane.held.total, |rows| rows.len());
        // Theme.qml rowPaddingY is round(6 * 20 / 12), ten pixels at text size 20.
        let expected = drawn.grow.content(total, drawn.row_height) + 10.;
        assert_eq!(pane.pane.scroll_extent, Some(expected));
    });
}

// SCN-DEV-1 cols-drag-row-move: rowNameText in Columns reads the active column's drawn row, as rowRect does.
#[test]
fn fixreaders_columns_row_name_text_reads_the_active_column() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        let held = shell.panes[0].as_ref().unwrap().pane.held.clone();
        let expected = crate::row::decorated_name(held.row(0).expect("row 0 is listed"));
        assert!(!expected.is_empty());
        let reply = host::call(shell, "rowNameText", &["0"], window, cx);
        assert_eq!(reply.stdout.trim(), expected);
        assert_eq!(
            host::call(shell, "rowNameText", &["2"], window, cx)
                .stdout
                .trim(),
            "",
            "no row 2 is drawn"
        );
    });
}

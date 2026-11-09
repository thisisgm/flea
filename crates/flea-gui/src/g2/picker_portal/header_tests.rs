//! EY-269: a tap on a List column title re-sorts the chooser and moves the header's mark, as v0.3.12's PickerHeader does.
use super::ipc::PickerGeometry;
use super::{
    process::Line,
    tests::fixture::{fixture, Fixture},
};
use crate::list::columns::Column;
use gpui::{point, px, Modifiers, MouseButton, Pixels, Point, TestAppContext};

fn answer(f: &mut Fixture, listing: u64) {
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","n":2,"v":{listing},"w":true,"path":"/d/sub"}}"#
    )));
    f.listing.say(Line::Reply(format!(r#"{{"t":"rows","start":0,"listing":{listing},"rows":[{{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"}},{{"n":"b.txt","d":false,"s":9,"m":5,"p":33188,"i":"text-x-generic"}}]}}"#)));
    f.cx.run_until_parked();
}

fn title(f: &mut Fixture, column: Option<Column>) -> Point<Pixels> {
    f.cx.update(|_, cx| {
        let view = f.picker.read(cx).browse.listing.read(cx);
        let state = view.drawn();
        let header = &state.header.drawn;
        let cell = match column {
            Some(column) => header.layout.cell(column),
            None => crate::list::header::HeaderState::title_cell(
                &header.layout,
                &state.drawn.metrics,
                None,
                false,
            ),
        }
        .expect("a drawn title");
        point(
            header.bounds.left() + px(cell.x + cell.width / 2.),
            header.bounds.center().y,
        )
    })
}

fn tap(f: &mut Fixture, column: Option<Column>) -> (String, &'static str, bool) {
    let from = f.listing.sent().len();
    let at = title(f, column);
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    let line = f.listing.sent()[from..]
        .iter()
        .rev()
        .find(|line| line.contains(r#""c":"list""#))
        .cloned()
        .unwrap_or_default();
    let order = f.update(|picker, cx| picker.browse.listing.read(cx).drawn().view.order);
    (line, order.key, order.desc)
}

#[gpui::test]
fn picker_gm_title_taps_sort_name_size_and_date(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    f.cx.simulate_resize(gpui::size(px(1100.), px(700.)));
    answer(&mut f, 1);
    let titles = f.cx.update(|_, cx| {
        let picker = f.picker.read(cx);
        super::geometry::Live { picker, cx }.controls()
    });
    for title in [r#""Name""#, r#""Size""#, r#""Modified""#] {
        assert!(
            titles.contains(title),
            "{title} is a drawn control: {titles}"
        );
    }
    let (line, key, desc) = tap(&mut f, Some(Column::Size));
    assert!(
        line.contains(r#""by":"size","desc":false"#),
        "Size sorts ascending: {line}"
    );
    assert_eq!((key, desc), ("size", false), "the header marks Size");
    answer(&mut f, 2);
    let (line, key, desc) = tap(&mut f, Some(Column::Size));
    assert!(
        line.contains(r#""by":"size","desc":true"#),
        "Size again reverses: {line}"
    );
    assert_eq!((key, desc), ("size", true));
    answer(&mut f, 3);
    let (line, key, desc) = tap(&mut f, Some(Column::Date));
    assert!(
        line.contains(r#""by":"mtime","desc":false"#),
        "Modified sorts ascending: {line}"
    );
    assert_eq!((key, desc), ("mtime", false));
    answer(&mut f, 4);
    let (line, key, desc) = tap(&mut f, None);
    assert!(
        line.contains(r#""c":"list""#) && !line.contains(r#""by":"#),
        "Name lists on the default order: {line}"
    );
    assert_eq!((key, desc), ("name", false));
    answer(&mut f, 5);
    f.update(|picker, cx| picker.move_cursor(1, cx));
    let (from, at) = (f.listing.sent().len(), title(&mut f, Some(Column::Size)));
    f.cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        1,
        "a title press never moves the cursor"
    );
    assert_eq!(
        f.listing.sent().len(),
        from,
        "a title sorts on its release, not its press"
    );
    f.cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    f.cx.run_until_parked();
    assert!(
        f.listing.sent()[from..]
            .iter()
            .any(|line| line.contains(r#""by":"size""#)),
        "the release sorts"
    );
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        0,
        "the release re-lists from the first row, as v0.3.12's clearListing does"
    );
}

//! A settle samples newer geometry than the frame that armed it; its answers must share that plan.
use super::{demand::Demand, surface::Want, Key, Mode, Pictures, Policy, Slot};
use crate::list::pane::Held;
use flea::protocol::RowFacts;
use gpui::{div, AppContext, Context, IntoElement, Render, TestAppContext, Window};
use std::rc::Rc;

const PICTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/images/transfer.png"
);

fn held(rows: impl IntoIterator<Item = usize>) -> Held {
    Held {
        listing: 2,
        total: 3000,
        start: 0,
        rows: rows
            .into_iter()
            .map(|row| RowFacts {
                listing_index: Some(row),
                name: format!("photo-{row}.png"),
                icon: "image-x-generic".into(),
                mode: 0o100644,
                size: 100,
                mtime: 1_700_000_000,
                ..Default::default()
            })
            .collect(),
    }
}

fn images() -> Policy {
    Policy::read(&flea::jsondoc::parse(r#"{"preview":{"thumbnails":"images"}}"#).unwrap())
}

fn plan(pictures: &Pictures, before: Want) -> Key {
    let key = Key {
        listing: 2,
        span: before.span,
        only: before
            .only
            .as_ref()
            .map(|(rows, first, end)| (Rc::as_ptr(rows).cast::<usize>() as usize, *first, *end)),
        mode: Mode::Images,
        cache: Some(false),
    };
    let mut map = pictures.map.borrow_mut();
    map.listing = 2;
    map.path = "/files".into();
    map.class = Some(("/files".into(), "local".into()));
    map.policy = Some(images());
    map.drawn = Some(before);
    map.key = Some(key);
    key
}

fn answers(cx: &mut TestAppContext, pictures: &Rc<Pictures>, rows: &[usize]) {
    let owner = cx.new(|_| ());
    owner.update(cx, |_, cx| {
        for &row in rows {
            pictures.thumbed(2, "/files", row, PICTURE, cx);
        }
    });
    cx.run_until_parked();
    let map = pictures.map.borrow();
    assert_eq!(
        map.counters.answered,
        rows.len(),
        "nonempty replies for the settle's newly exposed rows cannot remain Asked"
    );
    assert_eq!(map.counters.decoded, rows.len());
    assert!(rows
        .iter()
        .all(|row| matches!(map.slots.get(row), Some(Slot::Ready { .. }))));
}

#[gpui::test]
fn fx477b_sparse_list_settle_accepts_the_seven_newly_exposed_answers(cx: &mut TestAppContext) {
    let shown: Rc<[usize]> = (0..50).map(|at| at * 60).collect();
    let pictures = Rc::new(Pictures::default());
    let key = plan(
        &pictures,
        Want {
            span: (0, 2520),
            only: Some((Rc::clone(&shown), 0, 43)),
        },
    );
    let current = Want {
        span: (420, 2940),
        only: Some((Rc::clone(&shown), 7, 50)),
    };
    let newly_exposed = &shown[43..50];
    let lines = pictures.ask(
        &held(newly_exposed.iter().copied()),
        false,
        key,
        Some(current),
    );
    assert_eq!(
        lines,
        [r#"{"c":"thumb","rows":[2580,2640,2700,2760,2820,2880,2940],"cacheOnly":false}"#]
    );
    answers(cx, &pictures, newly_exposed);
    assert!(pictures.map.borrow().drawn.as_ref().unwrap().contains(2940));
    assert!(!pictures.map.borrow().drawn.as_ref().unwrap().contains(0));
}

#[gpui::test]
fn fx477b_dense_list_settle_accepts_a_row_exposed_by_its_final_geometry(cx: &mut TestAppContext) {
    let pictures = Rc::new(Pictures::default());
    let key = plan(
        &pictures,
        Want {
            span: (2898, 2939),
            only: None,
        },
    );
    let lines = pictures.ask(
        &held([2940]),
        false,
        key,
        Some(Want {
            span: (2898, 2941),
            only: None,
        }),
    );
    assert_eq!(lines, [r#"{"c":"thumb","rows":[2940],"cacheOnly":false}"#]);
    answers(cx, &pictures, &[2940]);
}

struct Probe;
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn dense(first: usize, last: usize) -> Want {
    Want {
        span: (first, last),
        only: None,
    }
}

#[gpui::test]
fn fx477b_an_answer_for_a_row_the_viewport_left_is_asked_again_on_return(cx: &mut TestAppContext) {
    let pictures = Rc::new(Pictures::default());
    pictures.class("/files", "local");
    let held = Rc::new(held(100..150));
    let policy = images();
    let window = cx.add_window(|_, _| Probe);
    let frame = |cx: &mut TestAppContext, want: Want| {
        window
            .update(cx, |_, window, cx| {
                let demand = Demand {
                    held: &held,
                    path: "/files",
                    live: true,
                    busy: false,
                    want: Some(want),
                    policy,
                };
                pictures.sync(demand, window, cx, |_, _, _| {})
            })
            .unwrap()
    };
    let settle = |want: Want| {
        let key = pictures.map.borrow().key.unwrap();
        pictures.ask(&held, false, key, Some(want))
    };
    frame(cx, dense(100, 139));
    let first = settle(dense(100, 139));
    assert!(first[0].contains("\"rows\":[100,101,"), "{first:?}");
    // A frame scrolled ten rows on before row 100's answer arrives, then scrolled back before that settle fired.
    frame(cx, dense(110, 149));
    let owner = cx.new(|_| ());
    owner.update(cx, |_, cx| pictures.thumbed(2, "/files", 100, PICTURE, cx));
    frame(cx, dense(100, 139));
    assert_eq!(
        settle(dense(100, 139)),
        [r#"{"c":"thumb","rows":[100],"cacheOnly":false}"#],
        "a row whose answer arrived while it was off screen is asked again once it is back"
    );
    answers(cx, &pictures, &[100]);
}

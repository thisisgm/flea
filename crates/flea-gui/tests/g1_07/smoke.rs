//! The fixture itself: a first sync asks the drawn neighbours and a reply for one of them lands.
use super::check::Checks;
use super::fixture::{area, drain_asks, reply, row, Pane, Producers};
use flea_gui::columns::peeks::Landed;

#[test]
fn first_sync_asks_the_neighbours_and_a_reply_lands() {
    let mut c = Checks::new("tests/g1_07/smoke");
    let mut area = area();
    let mut producers = Producers::default();
    let pane = Pane::at(
        "/home/user/work",
        vec![row("docs", true), row("a.txt", false)],
    );
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let asks = drain_asks(&mut area);
    c.eq(1, "a default window draws three columns", area.count(), 3);
    c.eq(
        2,
        "the parent and the cursor's folder are asked",
        asks,
        vec!["/home/user".to_owned(), "/home/user/work/docs".to_owned()],
    );
    let landed = pane.with(|view| {
        area.peeked(
            &reply("/home/user", &[("work", true)]),
            &view,
            &mut producers.ports(),
            1,
        )
    });
    c.eq(3, "an asked reply lands", landed, Landed::Stored);
    c.finish();
}

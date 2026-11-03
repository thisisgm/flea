use super::{
    process::Line,
    tests::fixture::{fixture, Fixture},
};
use gpui::TestAppContext;

fn draw(f: &mut Fixture) {
    f.cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    f.cx.run_until_parked();
}

#[gpui::test]
fn picker_parity_recent_grid_captions_the_leaf_name(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    // The starting folder's request is answered first, so Recent's is the only one in flight.
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":0,"v":1,"w":true}"#.into(),
    ));
    f.cx.run_until_parked();
    f.update(|picker, cx| {
        picker.user_set_view("grid", cx);
        picker.open_folder(flea::ui_logic_pk1::RECENT, cx);
    });
    f.cx.run_until_parked();
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/","n":2,"v":2,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":2,"rows":[{"n":"home/u/Media/report.pdf","d":false,"s":3,"m":0,"p":33188,"i":"application-pdf"},{"n":"home/u/work/notes.txt","d":false,"s":5,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.total), 2);
    draw(&mut f);
    draw(&mut f);
    let listing = f.read(|picker| picker.browse.listing.clone());
    f.cx.update(|_, cx| {
        let state = listing.read(cx).grid_state();
        let state = state.borrow();
        let captions: Vec<_> = state
            .drawn()
            .iter()
            .map(|tile| tile.caption_text())
            .collect();
        assert_eq!(captions, [Some("report.pdf"), Some("notes.txt")]);
        assert_eq!(state.drawn()[1].name(), Some("home/u/work/notes.txt"));
    });
}

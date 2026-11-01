// yazi find in a folder: the backend stamps its found reply, while a folder's listed line carries no stamp.
use crate::{
    jsondoc::{self, Json},
    tui::{
        action_key_tests::{drain, listing},
        actions,
        input::Key,
        keymap::Map,
        wire::{self, count, text},
    },
};
fn parse(s: &str) -> Json {
    jsondoc::parse(s).unwrap()
}
// A folder's listed line carries no listing stamp, so the backend's stamped found reply must still move the cursor.
#[test]
fn tui_fix_1101_p2_find_in_a_folder_lands_on_the_stamped_reply() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    assert_eq!(w.listing, None);
    m.total = 5;
    m.cursor = 1;
    actions::act(&mut m, "findPrevious", &mut w).unwrap();
    actions::key(&mut m, &Key::character('a', ""), &Map::load(), &mut w).unwrap();
    let requests = drain(&mut w);
    let id = count(
        requests.iter().find(|v| text(v, "c") == "filter").unwrap(),
        "id",
    );
    let found = format!(r#"{{"t":"found","id":{id},"query":"a","listing":3,"rows":[0,2,4]}}"#);
    // The same reply landing while the folder is changing is dropped, and lands once it is not.
    m.pending = Some("/elsewhere".into());
    m.receive(parse(&found), &mut w).unwrap();
    assert_eq!(m.cursor, 1);
    m.pending = None;
    m.receive(parse(&found), &mut w).unwrap();
    assert_eq!(m.cursor, 0);
    wire::finish(w, reader);
}

//! tests/js/columns.js runPeekKey and runPeekPending as functions: keys and pending asks.
use super::check::Checks;
use flea_gui::columns::peeks::{order_tag, peek_key, sent_key, tag_of, Peeked, Pending};

#[test]
fn a_peek_key_names_what_ordered_the_rows() {
    let mut c = Checks::new("tests/js/columns.js");
    c.eq(
        193,
        "the plain order keys plain",
        peek_key("/a", false, false),
        "/a\n00".to_owned(),
    );
    c.eq(
        194,
        "hidden keys apart",
        peek_key("/a", true, false),
        "/a\n10".to_owned(),
    );
    c.eq(
        195,
        "hidden-last keys apart too",
        peek_key("/a", false, true),
        "/a\n01".to_owned(),
    );
    c.eq(
        196,
        "both keys apart together",
        peek_key("/a", true, true),
        "/a\n11".to_owned(),
    );
    // A reply that names neither flag keys the way the plain order does.
    let bare = flea::jsondoc::parse(r#"{"t":"peeked","path":"/a","rows":[]}"#).expect("JSON");
    let bare = Peeked::from_json(&bare)
        .expect("a peeked reply")
        .expect("a well formed reply");
    c.eq(
        197,
        "an absent flag reads as off",
        bare.key(),
        "/a\n00".to_owned(),
    );
    c.ok(
        198,
        "a repair count asks apart from the pane's own",
        sent_key(&peek_key("/a", false, false), 1, "")
            != sent_key(&peek_key("/a", false, false), 35, ""),
    );
    c.eq(
        200,
        "the same count asks the same",
        sent_key("/a\n00", 35, ""),
        "/a\n00\n35".to_owned(),
    );
    c.eq(
        201,
        "a folder order tags its key and direction",
        order_tag("size", true),
        "size1".to_owned(),
    );
    c.eq(
        202,
        "an ascending order tags 0",
        order_tag("mtime", false),
        "mtime0".to_owned(),
    );
    c.eq(
        203,
        "no order tags nothing",
        order_tag("", true),
        String::new(),
    );
    c.eq(
        204,
        "an absent order tags nothing",
        tag_of(None),
        String::new(),
    );
    c.eq(
        205,
        "a folder order joins the sent key",
        sent_key("/a\n00", 35, "size1"),
        "/a\n00\n35\nsize1".to_owned(),
    );
    c.eq(
        206,
        "name order sends the key it always did",
        sent_key("/a\n00", 35, ""),
        "/a\n00\n35".to_owned(),
    );
    let mut before = Pending::default();
    before.track(&sent_key("/a\n00", 35, "size1"));
    c.ok(
        207,
        "a reply under the order before a re-sort answers no ask under the new one",
        !before.has(&sent_key("/a\n00", 35, "")),
    );
    c.finish();
}

#[test]
fn only_a_reply_this_view_asked_for_is_pending() {
    let mut c = Checks::new("tests/js/columns.js");
    let key = peek_key("/a", true, false);
    let other = peek_key("/b", true, false);
    let mut one = Pending::default();
    one.track(&key);
    c.ok(223, "an ask is outstanding once tracked", one.has(&key));
    c.ok(
        224,
        "anything else is another client's",
        !Pending::default().has(&key),
    );
    let mut dropped = one.clone();
    dropped.drop_one(&key);
    c.ok(225, "a stored reply drops its own ask", !dropped.has(&key));
    let mut both = one.clone();
    both.track(&other);
    both.drop_one(&key);
    c.ok(226, "a stored reply keeps a sibling ask", both.has(&other));
    let own = sent_key(&peek_key("/a", false, false), 35, "");
    let repair = sent_key(&peek_key("/a", false, false), 1, "");
    let mut held = Pending::default();
    held.track(&own);
    c.ok(
        230,
        "a first-1 reply for the same path and flags is refused while the pane's own ask stands",
        !held.has(&repair),
    );
    c.ok(232, "while the pane's own reply is kept", held.has(&own));
    let mut after = held.clone();
    after.drop_one(&repair);
    c.ok(
        233,
        "and the repair reply drops no ask of its own",
        after.has(&own),
    );
    c.finish();
}

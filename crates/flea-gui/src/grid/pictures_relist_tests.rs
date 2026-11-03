//! Grid picture reuse and request policy across relists.
use super::*;

#[gpui::test]
fn a_relist_of_the_same_folder_carries_each_picture_by_its_source_facts(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "{}");
    first_settle(cx, handle, &mut lines);
    deliver(cx, handle, &thumbed(1, PICTURE));
    deliver(cx, handle, &thumbed(5, PICTURE));
    cx.run_until_parked();
    step(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            let mut rows = pane.held.rows.clone();
            rows.swap(1, 2);
            rows[5].size += 1;
            pane.held = Rc::new(Held {
                rows,
                listing: pane.held.listing + 1,
                ..*pane.held
            });
        })
        .unwrap();
    step(cx, handle);
    assert_eq!(
        read(cx, handle, |shell| drawn(shell, 2).1),
        Some((1280, 64)),
        "photo.png moved to row 2 with its picture"
    );
    assert_eq!(read(cx, handle, |shell| drawn(shell, 1).1), None);
    assert_eq!(
        read(cx, handle, |shell| drawn(shell, 5).1),
        None,
        "a changed file is not carried"
    );
}

#[gpui::test]
fn an_unknown_class_is_asked_once_at_the_settle_and_a_network_folder_asks_the_cache_only(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = open(cx, "{}");
    step(cx, handle);
    drained(&mut lines);
    settle(cx, handle, 70);
    assert_eq!(
        drained(&mut lines),
        [FSINFO],
        "no thumbnail is made before the class is known"
    );
    settle(cx, handle, 240);
    assert!(
        drained(&mut lines).is_empty(),
        "the class is asked once per listing"
    );
    deliver(
        cx,
        handle,
        r#"{"t":"fsinfo","path":"/files","class":"network"}"#,
    );
    step(cx, handle);
    settle(cx, handle, 70);
    let asked = drained(&mut lines);
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(
        asked[0].ends_with(r#"],"cacheOnly":true}"#),
        "network thumbnails are off by default: {}",
        asked[0]
    );
}

#[gpui::test]
fn thumbnails_off_ask_nothing_and_images_ask_pictures_only(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, r#"{"preview":{"thumbnails":"off"}}"#);
    let asked = first_settle(cx, handle, &mut lines);
    assert!(
        asked.iter().all(|line| !line.contains("thumb")),
        "{asked:?}"
    );
    let (handle, mut lines) = open(cx, r#"{"preview":{"thumbnails":"images"}}"#);
    let asked = first_settle(cx, handle, &mut lines);
    assert!(
        asked.contains(&r#"{"c":"thumb","rows":[1],"cacheOnly":false}"#.to_owned()),
        "{asked:?}"
    );
}

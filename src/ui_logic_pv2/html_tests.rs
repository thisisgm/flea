use super::*;

fn look(path: &str) -> Html {
    let mut html = Html::new("look");
    html.set_active(true, 0);
    html.set_frame(902., 625., 1., 0);
    html.set_path(path, 1600, 0);
    html
}

fn ready(token: u64, slot: &str) -> Reply {
    Reply {
        token,
        slot: slot.into(),
        state: "ready".into(),
        file: "/c/a.png".into(),
        total: 61,
        width: 902,
        title: "Ridgeline".into(),
    }
}

fn ask_now(html: &mut Html, now: u64, token: u64) -> Command {
    html.tick(now);
    let commands = html.take_commands();
    let ask = commands
        .into_iter()
        .find(|c| matches!(c, Command::Ask { .. }))
        .expect("an ask went out");
    html.sent(token);
    ask
}

// tests/html-preview.qml: the rendered frame uses the device width, and Quick Look asks the viewport's height too.
#[test]
fn html_asks_at_the_settle_with_the_device_size() {
    let mut html = look("/w/index.html");
    html.tick(SETTLE_MS - 1);
    assert!(
        html.take_commands().is_empty(),
        "nothing is asked before the frame settles"
    );
    let ask = ask_now(&mut html, SETTLE_MS, 1);
    assert_eq!(
        ask,
        Command::Ask {
            slot: "look".into(),
            path: "/w/index.html".into(),
            width: 902,
            height: 625
        }
    );
    assert!(
        html.loading(Answer::Pending, false),
        "the first page shows the loading mark"
    );
    assert!(html.reply(&ready(1, "look")));
    html.picture_loaded(1, true);
    assert_eq!(
        (html.total, html.rendered_width, html.picture),
        (61, 902, Picture::Ready),
        "the page and its whole-file count"
    );
    assert!(
        html.first_screen(Answer::Pending, false) && !html.loading(Answer::Pending, false),
        "the drawn page is the first screen"
    );
    let mut column = Html::new("column");
    column.set_active(true, 0);
    column.set_frame(274., 560., 2., 0);
    assert_eq!(
        (column.device_width, column.device_height),
        (548, 0),
        "the column keeps the helper's own height"
    );
}

// A resize burst renders only the settled width, and the drawn page stays up until the new render lands.
#[test]
fn html_resize_burst_asks_once_and_keeps_the_page() {
    let mut html = look("/w/index.html");
    ask_now(&mut html, SETTLE_MS, 1);
    html.reply(&ready(1, "look"));
    html.picture_loaded(1, true);
    let before = html.asks;
    for (i, width) in [800., 760., 720., 700.].into_iter().enumerate() {
        html.set_frame(width, 625., 1., 1000 + i as u64 * 20);
        html.tick(1000 + i as u64 * 20);
        assert!(
            html.take_commands().is_empty(),
            "no helper runs while the frame is being resized"
        );
        assert!(
            !html.loading(Answer::Pending, false),
            "the drawn page stays up with no loading mark"
        );
    }
    let ask = ask_now(&mut html, 1060 + SETTLE_MS, 2);
    assert!(
        matches!(ask, Command::Ask { width: 700, .. }),
        "the settled width is the one rendered"
    );
    assert_eq!(html.asks - before, 1, "one render for the whole burst");
    assert!(
        !html.loading(Answer::Pending, false),
        "a re-ask keeps the drawn image without a loading mark"
    );
}

#[test]
fn html_drops_stale_closed_and_source_replies() {
    let mut html = look("/w/a.html");
    ask_now(&mut html, SETTLE_MS, 4);
    assert!(!html.reply(&ready(3, "look")), "an older token is dropped");
    assert!(
        !html.reply(&ready(4, "column")),
        "another slot's reply is dropped"
    );
    html.set_view(View::Source, 200);
    assert!(
        html.take_commands().contains(&Command::Cancel {
            slot: "look".into(),
            width: 902
        }),
        "Source cancels the pending render"
    );
    assert!(
        !html.reply(&ready(4, "look")),
        "a reply after switching to Source is dropped"
    );
    html.set_view(View::Rendered, 300);
    html.set_active(false, 310);
    assert!(
        !html.reply(&ready(4, "look")),
        "a reply after closing is dropped"
    );
    html.set_active(true, 320);
    html.set_path("/w/b.html", 1, 320);
    assert!(
        html.image_file.is_empty() && html.total == 0 && html.zoom == 1.,
        "a new subject drops the old page"
    );
}

// The same path re-asked: the reply selects the new token, so the decode reads the replaced bytes.
#[test]
fn html_same_path_reload_takes_the_new_token() {
    let mut html = look("/w/reload.html");
    ask_now(&mut html, SETTLE_MS, 1);
    html.reply(&ready(1, "look"));
    html.picture_loaded(1, true);
    html.ask();
    html.take_commands();
    html.sent(2);
    assert!(
        !html.loading(Answer::Pending, false),
        "re-ask keeps the drawn image without a loading mark"
    );
    assert!(html.reply(&ready(2, "look")));
    assert_eq!(
        (html.image_token, html.pending, html.picture),
        (2, false, Picture::Loading),
        "the reply changes the image token"
    );
    html.picture_loaded(1, true);
    assert_eq!(
        html.picture,
        Picture::Loading,
        "the old decode's finish is not the new image"
    );
    html.picture_loaded(2, true);
    assert_eq!(html.picture, Picture::Ready, "same-path reply is loaded");
}

#[test]
fn html_refusals_use_the_code_frames() {
    let mut big = look("/w/big.html");
    big.set_path("/w/big.html", BYTE_GATE + 1, 0);
    big.tick(SETTLE_MS);
    assert!(
        big.take_commands()
            .iter()
            .all(|c| !matches!(c, Command::Ask { .. })),
        "an oversize page asks nothing"
    );
    assert_eq!(big.refusal(Answer::Pending), Some(Refusal::TooLarge));
    let mut failed = look("/w/a.html");
    ask_now(&mut failed, SETTLE_MS, 1);
    failed.reply(&Reply {
        state: "failed".into(),
        ..ready(1, "look")
    });
    assert_eq!(
        failed.refusal(Answer::Pending),
        Some(Refusal::Unreadable),
        "a failed render is the unreadable frame"
    );
    let mut cancelled = look("/w/a.html");
    ask_now(&mut cancelled, SETTLE_MS, 1);
    cancelled.reply(&Reply {
        state: "cancelled".into(),
        ..ready(1, "look")
    });
    assert_eq!(
        cancelled.refusal(Answer::Pending),
        Some(Refusal::Unreadable),
        "a cancelled reply never draws a page"
    );
    let mut broken = look("/w/a.html");
    ask_now(&mut broken, SETTLE_MS, 1);
    broken.reply(&ready(1, "look"));
    broken.picture_loaded(1, false);
    assert_eq!(
        broken.refusal(Answer::Pending),
        Some(Refusal::Unreadable),
        "an undecodable picture is the unreadable frame"
    );
    let mut bare = look("/w/a.html");
    bare.no_backend();
    assert_eq!(
        bare.refusal(Answer::Pending),
        Some(Refusal::Unreadable),
        "a host with no html route reads as unreadable"
    );
}

// tests/html-source-refusals.qml: Source's terminal answers stop loading and release first-screen readiness.
#[test]
fn html_source_refusals_release_the_first_screen() {
    for (answer, refusal) in [
        (Answer::Binary, Refusal::Binary),
        (Answer::Large, Refusal::TooLarge),
        (Answer::Failed, Refusal::Unreadable),
    ] {
        let mut html = look("/fixture/refusal.html");
        html.set_view(View::Source, 0);
        assert!(
            html.source_built,
            "Source builds its code view the first time it shows"
        );
        assert!(
            html.loading(Answer::Pending, false),
            "Source loads until its first screen"
        );
        assert_eq!(
            html.refusal(answer),
            Some(refusal),
            "{answer:?} uses the existing refusal state"
        );
        assert!(
            !html.loading(answer, false) && html.first_screen(answer, false),
            "{answer:?} stops loading and releases first-screen readiness"
        );
    }
}

// R flips only the open Quick Look; zoom walks the preview ladder and zero restores fit; Source never zooms.
#[test]
fn html_view_flip_and_zoom() {
    let mut html = look("/w/a.html");
    assert_eq!(html.view, View::Rendered, "HTML opens rendered");
    html.zoom_by(1);
    assert_eq!(html.zoom, 1.25, "plus zooms on the preview ladder");
    html.zoom_fit();
    assert_eq!(html.zoom, 1., "zero restores fit");
    html.set_view(html.view.flipped(), 0);
    html.zoom_by(1);
    assert_eq!(html.zoom, 1., "Source does not zoom");
    assert_eq!(View::Source.flipped(), View::Rendered);
    assert_eq!(
        picture_size(902., 1.25, (902, 2000)),
        (1127.5, 2500.),
        "the picture keeps its decode's proportion"
    );
    assert_eq!(
        picture_size(902., 1., (0, 0)),
        (902., 0.),
        "a picture not decoded yet has no height"
    );
}

#[test]
fn the_column_ask_leaves_the_height_to_the_helper_and_quick_look_names_its_own() {
    // The backend reads "height":0 as a 1 px viewport, so a short page decoded one line tall (EY-35).
    let column = ask_line(4, "column", "/a/code-html.html", 548, 0);
    assert_eq!(
        column,
        r#"{"c":"html","token":4,"slot":"column","path":"/a/code-html.html","width":548}"#
    );
    assert_eq!(
        ask_line(5, "look", "/a/x \"y\".html", 984, 625),
        r#"{"c":"html","token":5,"slot":"look","path":"/a/x \"y\".html","width":984,"height":625}"#
    );
}

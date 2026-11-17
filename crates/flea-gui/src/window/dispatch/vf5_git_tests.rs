use super::{
    preview_tests::*,
    test_shell::{land, land_on},
    vf5_tests::opened,
};
use crate::{backend::Backend, backend::Event as BackendEvent, window::startup::View};
use flea::jsondoc::Json;
use gpui::{TestAppContext, VisualTestContext};

/// Deliver a producer line to one side, so the second pane of a dual view can answer its own reads.
fn answer_side(
    shell: &gpui::Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
    side: usize,
    line: String,
) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(side, BackendEvent::Producer(line), window, cx)
        })
    });
    draw(cx);
}

#[gpui::test]
fn vf5_git_reply_paints_letters_folder_and_branch_and_rejects_stale(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(true))]),
                window,
                cx,
            )
        })
    });
    let mut folder = file("child", "inode-directory");
    folder.dir = true;
    land(
        &shell,
        cx,
        "/files",
        vec![
            file("modified.txt", "text-plain"),
            file("untracked.txt", "text-plain"),
            folder,
        ],
        1,
    );
    draw(cx);
    let asks = sent(&mut requests, "gitstatus");
    let id = asks
        .last()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{id},"path":"/files","ok":true,"root":"/files","label":"main · 3 changed"}}"#
        ),
    );
    let asks = sent(&mut requests, "gitrows");
    let rows = asks.last().expect("painted rows reach Git worker");
    assert!(rows.get("paths").and_then(Json::as_array).unwrap().len() <= 400);
    let rows_id = rows.get("id").and_then(Json::as_f64).unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitrows","id":{rows_id},"path":"/files","ok":true,"marks":[{{"path":"/files/modified.txt","status":"M","folder":false}},{{"path":"/files/untracked.txt","status":"?","folder":false}},{{"path":"/files/child","status":"","folder":true}}]}}"#
        ),
    );
    shell.read_with(cx, |shell, _| {
        let git = &shell.panes[0].as_ref().unwrap().git.paint;
        assert_eq!(git.label, "main · 3 changed");
        assert_eq!(
            git.painted.get(),
            3,
            "the list painter draws both letters and folder square"
        );
    });
    assert!(
        sent(&mut requests, "gitrows").is_empty(),
        "same viewport never queues another read"
    );
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{},"path":"/files","ok":true,"root":"/files","label":"stale"}}"#,
            id - 1
        ),
    );
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().git.paint.label,
            "main · 3 changed"
        )
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(false))]),
                window,
                cx,
            )
        })
    });
    draw(cx);
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().git.paint.label.is_empty())
    });
    close(&shell, cx);
}

#[gpui::test]
fn vf5_git_marks_rows_inside_a_folder_opened_in_place(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(true))]),
                window,
                cx,
            )
        })
    });
    let mut folder = file("src", "inode-directory");
    folder.dir = true;
    land(
        &shell,
        cx,
        "/files",
        vec![
            folder,
            file("src/lib.rs", "text-rust"),
            file("notes.txt", "text-plain"),
        ],
        1,
    );
    draw(cx);
    let asks = sent(&mut requests, "gitstatus");
    let id = asks
        .last()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{id},"path":"/files","ok":true,"root":"/files","label":"main · 1 changed"}}"#
        ),
    );
    let asks = sent(&mut requests, "gitrows");
    let rows = asks.last().expect("painted rows reach Git worker");
    let paths: Vec<_> = rows
        .get("paths")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert!(paths.contains(&"/files/src/lib.rs"), "{paths:?}");
    let rows_id = rows.get("id").and_then(Json::as_f64).unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitrows","id":{rows_id},"path":"/files","ok":true,"marks":[{{"path":"/files/src","status":"","folder":true}},{{"path":"/files/src/lib.rs","status":"M","folder":false}},{{"path":"/files/notes.txt","status":"","folder":false}}]}}"#
        ),
    );
    shell.read_with(cx, |shell, _| {
        let git = &shell.panes[0].as_ref().unwrap().git.paint;
        assert_eq!(
            git.marks.get("src/lib.rs").map(|mark| mark.letter),
            Some('M'),
            "a nested row's mark is keyed by its own row name"
        );
        assert_eq!(
            git.painted.get(),
            2,
            "the open folder's square and its child's letter both draw"
        );
    });
    close(&shell, cx);
}

#[gpui::test]
fn vf5_git_refreshes_a_sitting_folder_and_stops_with_the_setting(cx: &mut TestAppContext) {
    use crate::window::git_status::REFRESH_EVERY;

    let (shell, cx, mut requests) = opened(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(true))]),
                window,
                cx,
            )
        })
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    draw(cx);

    let first = sent(&mut requests, "gitstatus")
        .pop()
        .expect("the first listing asks for status");
    let first_id = first.get("id").and_then(Json::as_f64).unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{first_id},"path":"/files","ok":true,"root":"/files","label":"main · 1 changed"}}"#
        ),
    );
    let rows = sent(&mut requests, "gitrows")
        .pop()
        .expect("painted rows reach Git");
    let rows_id = rows.get("id").and_then(Json::as_f64).unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitrows","id":{rows_id},"path":"/files","ok":true,"marks":[{{"path":"/files/notes.txt","status":"M","folder":false}}]}}"#
        ),
    );
    draw(cx);
    assert!(
        sent(&mut requests, "gitstatus").is_empty(),
        "a still folder asks nothing before its interval"
    );

    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);

    let refresh = sent(&mut requests, "gitstatus")
        .pop()
        .expect("an idle folder still refreshes Git");
    assert_eq!(refresh.get("refresh").and_then(Json::as_bool), Some(true));
    let refresh_id = refresh.get("id").and_then(Json::as_f64).unwrap() as usize;
    assert!(refresh_id > first_id);
    assert!(
        sent(&mut requests, "gitrows").is_empty(),
        "no rows are asked until the refreshed snapshot replies"
    );
    shell.read_with(cx, |shell, _| {
        let git = &shell.panes[0].as_ref().unwrap().git.paint;
        assert_eq!(git.label, "main · 1 changed");
        assert_eq!(
            git.marks.get("notes.txt").map(|mark| mark.letter),
            Some('M'),
            "the last mark stays drawn while the refresh is in flight"
        );
    });

    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{refresh_id},"path":"/files","ok":true,"root":"/files","label":"main · 2 changed"}}"#
        ),
    );
    let next_rows = sent(&mut requests, "gitrows")
        .pop()
        .expect("the new snapshot re-asks the same viewport");
    let next_rows_id = next_rows.get("id").and_then(Json::as_f64).unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitrows","id":{next_rows_id},"path":"/files","ok":true,"marks":[{{"path":"/files/notes.txt","status":"A","folder":false}}]}}"#
        ),
    );
    shell.read_with(cx, |shell, _| {
        let git = &shell.panes[0].as_ref().unwrap().git.paint;
        assert_eq!(git.label, "main · 2 changed");
        assert_eq!(
            git.marks.get("notes.txt").map(|mark| mark.letter),
            Some('A')
        );
    });

    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(false))]),
                window,
                cx,
            )
        })
    });
    draw(cx);
    let disabling = sent(&mut requests, "gitstatus");
    assert_eq!(disabling.len(), 1, "turning Git off clears the snapshot");
    assert_eq!(
        disabling[0].get("enabled").and_then(Json::as_bool),
        Some(false)
    );
    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    assert!(
        sent(&mut requests, "gitstatus").is_empty(),
        "no wake survives the setting going off"
    );
    close(&shell, cx);
}

#[gpui::test]
fn vf5_git_failed_refresh_recovers_on_the_next_tick(cx: &mut TestAppContext) {
    use crate::window::git_status::REFRESH_EVERY;

    let (shell, cx, mut requests) = opened(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(true))]),
                window,
                cx,
            )
        })
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    draw(cx);

    let first_id = sent(&mut requests, "gitstatus")
        .pop()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{first_id},"path":"/files","ok":true,"root":"/files","label":"main · 1 changed"}}"#
        ),
    );
    let rows_id = sent(&mut requests, "gitrows")
        .pop()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitrows","id":{rows_id},"path":"/files","ok":true,"marks":[{{"path":"/files/notes.txt","status":"M","folder":false}}]}}"#
        ),
    );

    // An idle tick asks for a refresh, which then fails (e.g. a temporary timeout).
    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    let failed_id = sent(&mut requests, "gitstatus")
        .pop()
        .expect("an idle folder refreshes Git")
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(r#"{{"t":"gitstatus","id":{failed_id},"path":"/files","ok":false,"root":""}}"#),
    );
    shell.read_with(cx, |shell, _| {
        let git = &shell.panes[0].as_ref().unwrap().git.paint;
        assert!(
            git.label.is_empty() && git.marks.is_empty(),
            "a failed snapshot has nothing truthful left to draw"
        );
    });

    // The next tick must still start a fresh read: a failed reply is not an in-flight read.
    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    let retry = sent(&mut requests, "gitstatus")
        .pop()
        .expect("a failed read recovers on the next tick");
    let retry_id = retry.get("id").and_then(Json::as_f64).unwrap() as usize;
    assert!(retry_id > failed_id);
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{retry_id},"path":"/files","ok":true,"root":"/files","label":"main · recovered"}}"#
        ),
    );
    let retry_rows_id = sent(&mut requests, "gitrows")
        .pop()
        .expect("the recovered snapshot re-asks the viewport")
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitrows","id":{retry_rows_id},"path":"/files","ok":true,"marks":[{{"path":"/files/notes.txt","status":"R","folder":false}}]}}"#
        ),
    );
    shell.read_with(cx, |shell, _| {
        let git = &shell.panes[0].as_ref().unwrap().git.paint;
        assert_eq!(git.label, "main · recovered");
        assert_eq!(git.marks.get("notes.txt").map(|m| m.letter), Some('R'));
    });
    close(&shell, cx);
}

#[gpui::test]
fn vf5_git_delayed_reply_fetches_rows_before_the_next_tick(cx: &mut TestAppContext) {
    use crate::window::git_status::REFRESH_EVERY;

    let (shell, cx, mut requests) = opened(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(true))]),
                window,
                cx,
            )
        })
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    draw(cx);

    let first_id = sent(&mut requests, "gitstatus")
        .pop()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{first_id},"path":"/files","ok":true,"root":"/files","label":"main · 1 changed"}}"#
        ),
    );
    let rows_id = sent(&mut requests, "gitrows")
        .pop()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitrows","id":{rows_id},"path":"/files","ok":true,"marks":[{{"path":"/files/notes.txt","status":"M","folder":false}}]}}"#
        ),
    );

    // One tick starts a refresh.
    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    let slow_id = sent(&mut requests, "gitstatus")
        .pop()
        .expect("an idle folder refreshes Git")
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;

    // A second tick fires while that read is still out, so a refresh is already owed when its
    // reply lands. Fetching the completed read's rows must win over paying that owed tick.
    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{slow_id},"path":"/files","ok":true,"root":"/files","label":"main · 2 changed"}}"#
        ),
    );
    let delayed_rows = sent(&mut requests, "gitrows")
        .pop()
        .expect("the completed read's rows are fetched before another refresh");
    assert!(
        sent(&mut requests, "gitstatus").is_empty(),
        "no second status read begins before the completed read's rows are asked for"
    );
    let delayed_rows_id = delayed_rows.get("id").and_then(Json::as_f64).unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitrows","id":{delayed_rows_id},"path":"/files","ok":true,"marks":[{{"path":"/files/notes.txt","status":"A","folder":false}}]}}"#
        ),
    );
    shell.read_with(cx, |shell, _| {
        let git = &shell.panes[0].as_ref().unwrap().git.paint;
        assert_eq!(git.label, "main · 2 changed");
        assert_eq!(
            git.marks.get("notes.txt").map(|m| m.letter),
            Some('A'),
            "a read slower than the interval still lands its marks"
        );
    });
    assert!(
        sent(&mut requests, "gitstatus").is_empty(),
        "a tick during a slow read is not owed as a second, back-to-back read"
    );
    close(&shell, cx);
}

#[gpui::test]
fn vf5_git_mismatched_reply_does_not_strand_the_next_refresh(cx: &mut TestAppContext) {
    use crate::window::git_status::REFRESH_EVERY;

    let (shell, cx, mut requests) = opened(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(true))]),
                window,
                cx,
            )
        })
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    draw(cx);

    let first_id = sent(&mut requests, "gitstatus")
        .pop()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{first_id},"path":"/files","ok":true,"root":"/files","label":"main · 1 changed"}}"#
        ),
    );
    let rows_id = sent(&mut requests, "gitrows")
        .pop()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitrows","id":{rows_id},"path":"/files","ok":true,"marks":[{{"path":"/files/notes.txt","status":"M","folder":false}}]}}"#
        ),
    );

    // A tick asks for a refresh, but the reply echoes a path the pane no longer has (a rename).
    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    let stale_id = sent(&mut requests, "gitstatus")
        .pop()
        .expect("an idle folder refreshes Git")
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{stale_id},"path":"/renamed","ok":true,"root":"/renamed","label":"wrong"}}"#
        ),
    );
    shell.read_with(cx, |shell, _| {
        let git = &shell.panes[0].as_ref().unwrap().git.paint;
        assert_eq!(
            git.label, "main · 1 changed",
            "a mismatched reply is ignored"
        );
    });

    // The request is over even though its reply was dropped, so the next tick still refreshes.
    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    let retry = sent(&mut requests, "gitstatus")
        .pop()
        .expect("a mismatched reply must not strand future refreshes");
    assert!(retry.get("id").and_then(Json::as_f64).unwrap() as usize > stale_id);
    close(&shell, cx);
}

#[gpui::test]
fn vf5_git_failed_send_keeps_the_wake_alive(cx: &mut TestAppContext) {
    use crate::window::git_status::REFRESH_EVERY;

    let (shell, cx, _requests) = opened(cx);
    // A backend whose recording end is gone: every write fails, so no request ever reaches Git.
    let (dead, lines) = Backend::test_lines();
    drop(lines);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(dead);
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(true))]),
                window,
                cx,
            )
        })
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    draw(cx);
    shell.read_with(cx, |shell, _| {
        assert!(
            !shell.panes[0].as_ref().unwrap().git.in_flight(),
            "a request that never reached the pipe is not in flight"
        );
    });

    // The wake keeps offering a refresh: a failed send must not strand the pane behind a flag.
    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    shell.read_with(cx, |shell, _| {
        assert!(
            !shell.panes[0].as_ref().unwrap().git.in_flight(),
            "a later tick still retries instead of waiting on a reply that can never come"
        );
    });
    close(&shell, cx);
}

#[gpui::test]
fn vf5_git_hidden_second_pane_stops_refreshing(cx: &mut TestAppContext) {
    use crate::window::git_status::REFRESH_EVERY;

    let (shell, cx, mut left) = opened(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(true))]),
                window,
                cx,
            )
        })
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    draw(cx);
    let left_id = sent(&mut left, "gitstatus")
        .pop()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"gitstatus","id":{left_id},"path":"/files","ok":true,"root":"/files","label":"main"}}"#
        ),
    );
    let left_rows_id = sent(&mut left, "gitrows")
        .pop()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer(
        &shell,
        cx,
        format!(r#"{{"t":"gitrows","id":{left_rows_id},"path":"/files","ok":true,"marks":[]}}"#),
    );

    // Bring up a second pane and give it its own recording backend.
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Dual, window, cx)));
    let (right_backend, right_lines) = Backend::test_lines();
    let mut right: Lines = Box::new(right_lines);
    shell.update(cx, |shell, _| {
        shell.panes[1]
            .as_mut()
            .expect("dual has a second pane")
            .backend = Some(right_backend);
    });
    land_on(
        &shell,
        cx,
        1,
        "/files",
        vec![file("there.txt", "text-plain")],
        1,
        1,
    );
    draw(cx);
    let right_id = sent(&mut right, "gitstatus")
        .pop()
        .expect("a shown second pane asks for status")
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer_side(
        &shell,
        cx,
        1,
        format!(
            r#"{{"t":"gitstatus","id":{right_id},"path":"/files","ok":true,"root":"/files","label":"dev"}}"#
        ),
    );
    let right_rows_id = sent(&mut right, "gitrows")
        .pop()
        .unwrap()
        .get("id")
        .and_then(Json::as_f64)
        .unwrap() as usize;
    answer_side(
        &shell,
        cx,
        1,
        format!(r#"{{"t":"gitrows","id":{right_rows_id},"path":"/files","ok":true,"marks":[]}}"#),
    );

    // While both panes are shown, the second pane refreshes on the idle tick.
    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    assert!(
        !sent(&mut right, "gitstatus").is_empty(),
        "a shown second pane keeps its wake"
    );

    // Return to one pane: the second pane is hidden and must give its wake up.
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::List, window, cx)));
    draw(cx);
    sent(&mut right, "gitstatus"); // drain the hiding clear request

    cx.executor().advance_clock(REFRESH_EVERY);
    cx.run_until_parked();
    draw(cx);
    assert!(
        sent(&mut right, "gitstatus").is_empty(),
        "a hidden pane no longer runs Git reads"
    );
    close(&shell, cx);
}

#[test]
fn vf5_git_label_starts_after_the_whole_path() {
    let mut strip = crate::path::strip::Strip::default();
    strip.fit("/tmp/flea/work", "/home/gm", None, 80);
    let count = strip.crumbs().len();
    strip.place(std::iter::repeat_n(30., count));
    let (left, width) = strip.extent(count - 1).unwrap();
    assert_eq!(
        strip.end(),
        left + width,
        "the label follows the last crumb's right edge"
    );
    assert!(
        strip.end() > width,
        "a path of several crumbs ends past its last crumb's width"
    );
}

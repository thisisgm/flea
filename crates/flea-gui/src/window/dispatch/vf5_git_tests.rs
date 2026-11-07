use super::{preview_tests::*, test_shell::land, vf5_tests::opened};
use flea::jsondoc::Json;
use gpui::TestAppContext;

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

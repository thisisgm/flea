//! Operation results pass through the mounted window and its normal listing swap.
use super::*;
use flea::protocol::Reply;
use std::time::Duration;

fn choose(cx: &mut TestAppContext, handle: WindowHandle<Root>, action: &str, name: &str) {
    shell(cx, handle, |shell, _, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        cx.emit(Chosen {
            captured: Captured {
                side: 0,
                listing: host.pane.held.listing,
                rows: vec![host.pane.cursor],
                cursor: host.pane.cursor,
                path: host.path.clone(),
                identity: String::new(),
                id: 41,
                ready: true,
                kinds: Vec::new(),
                background: false,
                target: None,
            },
            action: action.into(),
            leaf: String::new(),
            paths: vec![format!("/files/{name}")],
            claimed: Rc::new(Cell::new(false)),
        });
    });
    cx.run_until_parked();
}

fn receive(cx: &mut TestAppContext, handle: WindowHandle<Root>, line: &str) {
    shell(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(line), window, cx);
    });
    cx.run_until_parked();
}

fn land(cx: &mut TestAppContext, handle: WindowHandle<Root>, rows: Vec<RowFacts>) {
    shell(cx, handle, |shell, window, cx| {
        for reply in [
            Reply::Listed {
                path: "/files".into(),
                total: rows.len(),
            },
            Reply::Rows {
                start: 0,
                rows,
                listing: 3,
            },
        ] {
            shell.receive(0, Event::Reply(reply), window, cx);
        }
    });
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(400));
        frame(cx, handle);
        cx.run_until_parked();
    }
}

#[gpui::test]
fn extract_result_reloads_and_lands_on_the_new_first_folder(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend);
    let mut folder = row("folder-40", "folder");
    folder.dir = true;
    let archive = row("archive-zip.zip", "package-x-generic");
    shell(cx, handle, |shell, window, cx| {
        hold(shell, vec![folder.clone(), archive.clone()]);
        // Settle the fixture's initial listing before the operation arms its landing target.
        shell.receive(0, Event::Reply(Reply::Other), window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 1;
        host.selected = vec![1];
    });
    frame(cx, handle);
    while requests().is_some() {}
    choose(cx, handle, "extract", "archive-zip.zip");
    let request = sent(&mut requests, "archive").expect("Extract reaches the backend");
    assert!(
        request.contains(r#""path":"/files/archive-zip.zip""#),
        "{request}"
    );
    receive(cx, handle, r#"{"t":"archivestarted","id":7}"#);
    receive(cx, handle, r#"{"t":"extractstarted","id":7}"#);
    // An extract's success names its archive and the folder it asked for.
    receive(
        cx,
        handle,
        r#"{"t":"archivedone","id":7,"ok":true,"verified":true,"err":"","path":"/files/archive-zip.zip","dest":"/files/archive-zip"}"#,
    );
    assert!(
        sent(&mut requests, "list").is_some(),
        "Extract re-reads the result's folder"
    );
    let mut extracted = row("archive-zip", "folder");
    extracted.dir = true;
    land(cx, handle, vec![extracted, folder, archive]);
    shell(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.pane.busy());
        assert_eq!(host.pane.held.total, 3);
        assert_eq!(host.pane.held.row(0).unwrap().name, "archive-zip");
        assert_eq!(host.pane.cursor, 0, "the extracted folder takes the cursor");
        assert!(
            host.selected.is_empty(),
            "the archive's old mark is cleared"
        );
    });
}

#[gpui::test]
fn compress_result_during_a_reread_clears_the_cursor_and_marks(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend);
    let before = row("notes.txt", "text-x-generic");
    shell(cx, handle, |shell, window, cx| {
        hold(
            shell,
            vec![
                row("alpha.txt", "text-x-generic"),
                before.clone(),
                row("zeta.txt", "text-x-generic"),
            ],
        );
        shell.receive(0, Event::Reply(Reply::Other), window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 1;
        host.selected = vec![1];
    });
    frame(cx, handle);
    while requests().is_some() {}
    // The folder watch re-reads as the archive writes notes.zip into it, so the re-read is still out when the result lands.
    shell(cx, handle, |shell, window, cx| {
        let stepped = shell.step(0, crate::window::navigate::Step::Reread, window, cx);
        assert!(matches!(stepped, Ok(true)), "the watch re-read starts");
    });
    frame(cx, handle);
    receive(
        cx,
        handle,
        r#"{"t":"archivedone","id":9,"ok":true,"verified":true,"err":""}"#,
    );
    land(
        cx,
        handle,
        vec![
            row("alpha.txt", "text-x-generic"),
            before,
            row("notes.zip", "package-x-generic"),
            row("zeta.txt", "text-x-generic"),
        ],
    );
    shell(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.pane.busy());
        assert_eq!(host.pane.cursor, 0, "a compress result takes the first row");
        assert!(
            host.selected.is_empty(),
            "a compress result leaves nothing marked: {:?}",
            host.selected
        );
    });
}

#[gpui::test]
fn convert_result_reloads_and_selects_the_created_file(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend);
    let before = row("notes.txt", "text-x-generic");
    let source = row("photo.png", "image-x-generic");
    shell(cx, handle, |shell, window, cx| {
        hold(shell, vec![before.clone(), source.clone()]);
        shell.receive(0, Event::Reply(Reply::Other), window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 1;
        host.selected = vec![1];
    });
    frame(cx, handle);
    focus_shell(cx, handle);
    while requests().is_some() {}
    choose(cx, handle, "convert", "photo.png");
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("enter");
    let request = sent(&mut requests, "convertbatch").expect("Convert reaches the backend");
    let id = field(&request, "requestId");
    receive(
        cx,
        handle,
        &format!(r#"{{"t":"convertstarted","id":8,"requestId":{id},"source":"/files/photo.png"}}"#),
    );
    receive(
        cx,
        handle,
        &format!(
            r#"{{"t":"convertdone","id":8,"requestId":{id},"source":"/files/photo.png","ok":true,"paths":["/files/photo (converted).jpg"],"path":"/files/photo (converted).jpg","err":""}}"#
        ),
    );
    let listing = sent(&mut requests, "list").expect("Convert re-reads the result's folder");
    assert_eq!(
        flea::json::field_str(&listing, "select").as_deref(),
        Some("/files/photo (converted).jpg")
    );
    land(
        cx,
        handle,
        vec![
            before,
            row("photo (converted).jpg", "image-x-generic"),
            source,
        ],
    );
    shell(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.pane.busy());
        assert_eq!(host.pane.held.total, 3);
        assert_eq!(
            host.pane.held.row(host.pane.cursor).unwrap().name,
            "photo (converted).jpg"
        );
        assert_eq!(
            host.selected,
            [host.pane.cursor],
            "only the converted file is marked"
        );
    });
}

#[gpui::test]
fn fixcards_ey255_compress_result_lands_on_the_first_row_with_nothing_marked(
    cx: &mut TestAppContext,
) {
    // EY-255: after "Archive written." v0.3.12 resets to the first row with nothing selected; no re-read is in flight here.
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend);
    let notes = row("notes.txt", "text-x-generic");
    let mut folder = row("A", "folder");
    folder.dir = true;
    shell(cx, handle, |shell, window, cx| {
        hold(shell, vec![folder.clone(), notes.clone()]);
        shell.receive(0, Event::Reply(Reply::Other), window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 1;
        host.selected = vec![1];
    });
    frame(cx, handle);
    while requests().is_some() {}
    choose(cx, handle, "compress", "notes.txt");
    receive(cx, handle, r#"{"t":"archivestarted","id":9}"#);
    receive(
        cx,
        handle,
        r#"{"t":"archivedone","id":9,"ok":true,"verified":true,"err":""}"#,
    );
    assert!(
        sent(&mut requests, "list").is_some(),
        "Compress re-reads its folder"
    );
    land(
        cx,
        handle,
        vec![folder, notes, row("notes.zip", "package-x-generic")],
    );
    shell(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.pane.busy());
        assert_eq!(host.pane.held.total, 3);
        assert_eq!(host.pane.cursor, 0, "the cursor goes back to A");
        assert!(
            host.selected.is_empty(),
            "nothing stays marked: {:?}",
            host.selected
        );
    });
}

//! A real backend query reaches the mounted result painter and its viewport.
use super::*;
use crate::g2::sr1::contents::PAINTED_LINES;
use crate::window::startup::View;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn real_query_paints(view: View) {
    let backend =
        std::env::var("FLEA_G12_BACKEND").expect("FLEA_G12_BACKEND is the retained backend");
    let made = Command::new("mktemp")
        .args(["-d", "/tmp/flea-g12-paint.XXXXXXXX"])
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = String::from_utf8(made.stdout).unwrap().trim().to_owned();
    std::fs::write(format!("{root}/.flea-test-sandbox"), "g12 mounted paint\n").unwrap();
    std::fs::write(
        format!("{root}/alpha.txt"),
        "first line\nneedle first match\nneedle later\n",
    )
    .unwrap();
    std::fs::write(format!("{root}/beta.txt"), "needle on first line\n").unwrap();
    std::fs::write(format!("{root}/other.txt"), "no match\n").unwrap();
    for index in 0..40 {
        std::fs::write(
            format!("{root}/extra-{index:02}.txt"),
            format!("needle extra {index:02}\n"),
        )
        .unwrap();
    }

    let mut child = Command::new(backend)
        .arg("--backend")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if send.send(line.unwrap()).is_err() {
                break;
            }
        }
    });

    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests) = open(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        host.path = root.clone();
        host.pane = crate::pane::Pane::new(root.clone(), 620.);
        host.pane.clock.use_manual();
        host.changed(&shell.locale);
        shell.set_view(view, window, cx);
    });
    writeln!(
        stdin,
        "{{\"c\":\"list\",\"path\":\"{root}\",\"first\":320}}"
    )
    .unwrap();
    loop {
        let line = receive
            .recv_timeout(Duration::from_secs(20))
            .expect("backend initial listing");
        let done = flea::json::field_str(&line, "t").as_deref() == Some("rows");
        reply(&mut cx, handle, 0, &line);
        if done {
            break;
        }
    }
    frame(&mut cx, handle);
    if view == View::Columns {
        edit(&mut cx, handle, |shell, window, cx| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .clock
                .advance_manual(Duration::from_millis(crate::pane::swap::CAP_MS + 20));
            shell.sync_columns(window, cx);
        });
        frame(&mut cx, handle);
        edit(&mut cx, handle, |shell, _, _| {
            assert_eq!(shell.shown_view(), View::Columns)
        });
    }
    let ancestors = edit(&mut cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .map(|columns| {
                columns
                    .area
                    .tracks()
                    .iter()
                    .filter(|track| track.slot.depth() > 0)
                    .map(|track| track.path.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    if view == View::Columns {
        assert!(!ancestors.is_empty(), "the active track has ancestors");
    }
    while requests().is_some() {}
    control_key(&mut cx, handle, "g");
    for letter in ["n", "e", "e", "d", "l", "e"] {
        key(&mut cx, handle, letter, Some(letter));
    }
    let mut query = None;
    while let Some(line) = requests() {
        if flea::json::field_str(&line, "c").as_deref() == Some("searchcontents") {
            query = Some(line);
        }
    }
    let query = query.expect("mounted search emits a contents query");
    assert_eq!(
        flea::json::field_str(&query, "path").as_deref(),
        Some(root.as_str())
    );
    assert_eq!(
        flea::json::field_str(&query, "query").as_deref(),
        Some("needle")
    );
    writeln!(stdin, "{query}").unwrap();
    let generation = flea::json::field_usize(&query, "gen").expect("query generation");
    let mut match_listing = None;
    let mut rows_listing = None;
    loop {
        let line = receive
            .recv_timeout(Duration::from_secs(20))
            .expect("backend result");
        let done = flea::json::field_str(&line, "t").as_deref() == Some("searched");
        match flea::json::field_str(&line, "t").as_deref() {
            Some("contentsmatches") => {
                assert_eq!(flea::json::field_usize(&line, "gen"), Some(generation));
                match_listing = flea::json::field_usize(&line, "listing");
            }
            Some("rows") => {
                assert_eq!(flea::json::field_usize(&line, "gen"), Some(generation));
                rows_listing = flea::json::field_usize(&line, "listing");
            }
            _ => {}
        }
        reply(&mut cx, handle, 0, &line);
        if done {
            break;
        }
    }
    assert!(match_listing.is_some());
    assert_eq!(
        match_listing, rows_listing,
        "real match and row snapshots share an identity"
    );
    eprintln!("view={view:?} generation={generation} listing={match_listing:?} fixture={root}");
    writeln!(stdin, "{{\"c\":\"quit\"}}").unwrap();
    drop(stdin);
    assert!(child.wait().unwrap().success());
    reader.join().unwrap();
    key(&mut cx, handle, "enter", None);
    PAINTED_LINES.with(|lines| *lines.borrow_mut() = Some(Vec::new()));
    frame(&mut cx, handle);
    let (listing, viewport) = edit(&mut cx, handle, |shell, _, _| {
        assert_eq!(
            shell.shown_view(),
            view,
            "requested surface must be mounted"
        );
        let host = shell.panes[0].as_ref().unwrap();
        let held = shell
            .contents_blocks(0)
            .expect("real results join their lines");
        assert_eq!(held.total, 42);
        assert_eq!(
            (held.blocks[0].line_no, held.blocks[0].line.as_str()),
            (2, "needle first match")
        );
        assert_eq!(
            (held.blocks[1].line_no, held.blocks[1].line.as_str()),
            (1, "needle on first line")
        );
        let input = host.input.frame.expect("mounted input viewport");
        assert_eq!(input.row, crate::g2::sr1::contents::BLOCK_H);
        let point = gpui::point(
            input.bounds.left() + gpui::px(40.),
            input.bounds.top() + gpui::px(input.cell_origin.1 + input.row + 40.),
        );
        assert_eq!(
            host.input.hit(point),
            Some(1),
            "the second match line hits its own block"
        );
        (held.listing, input.bounds)
    });
    PAINTED_LINES.with(|lines| {
        let lines = lines.borrow();
        let lines = lines.as_ref().unwrap();
        for (index, number, text) in [(0, 2, "needle first match"), (1, 1, "needle on first line")]
        {
            let painted = lines
                .iter()
                .find(|(row, n, line, bounds, number_ok, line_ok, _)| {
                    *row == index
                        && *n == number
                        && line == text
                        && *number_ok
                        && *line_ok
                        && bounds.top() >= viewport.top()
                        && bounds.bottom() <= viewport.bottom()
                        && bounds.left() >= viewport.left()
                        && bounds.right() <= viewport.right()
                });
            assert!(
                painted.is_some(),
                "view={view:?} listing={listing} viewport={viewport:?} actual={lines:?}"
            );
        }
    });
    for text in ["v", "j", "v"] {
        key(&mut cx, handle, text, Some(text));
    }
    PAINTED_LINES.with(|lines| *lines.borrow_mut() = Some(Vec::new()));
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, [0, 1]);
        if view == View::Columns {
            let after: Vec<_> = host
                .columns
                .as_ref()
                .unwrap()
                .area
                .tracks()
                .iter()
                .filter(|track| track.slot.depth() > 0)
                .map(|track| track.path.clone())
                .collect();
            assert_eq!(after, ancestors, "search retains ancestor tracks");
        }
    });
    PAINTED_LINES.with(|lines| {
        assert!(lines.borrow().as_ref().unwrap().iter().any(
            |(index, _, _, _, number, text, selected)| *index == 0 && *number && *text && *selected
        ));
    });
    key(&mut cx, handle, "end", None);
    PAINTED_LINES.with(|lines| *lines.borrow_mut() = Some(Vec::new()));
    frame(&mut cx, handle);
    let final_viewport = edit(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 41);
        assert!(
            host.pane.scroll_y > 0.,
            "the final block is revealed by scrolling"
        );
        host.input
            .frame
            .expect("final mounted input viewport")
            .bounds
    });
    PAINTED_LINES.with(|lines| {
        assert!(lines.borrow().as_ref().unwrap().iter().any(
            |(index, number, text, bounds, number_ok, line_ok, _)| *index == 41
                && *number == 1
                && text == "needle extra 39"
                && *number_ok
                && *line_ok
                && bounds.top() >= final_viewport.top()
                && bounds.bottom() <= final_viewport.bottom()
                && bounds.left() >= final_viewport.left()
                && bounds.right() <= final_viewport.right()
        ))
    });
}

#[test]
#[ignore = "requires FLEA_G12_BACKEND naming the matching backend"]
fn real_backend_contents_paints_list_lines_and_gutters() {
    real_query_paints(View::List);
}

#[test]
#[ignore = "requires FLEA_G12_BACKEND naming the matching backend"]
fn real_backend_contents_paints_columns_lines_and_gutters() {
    real_query_paints(View::Columns);
}

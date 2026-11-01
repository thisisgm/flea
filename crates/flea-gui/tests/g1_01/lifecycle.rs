use super::*;

#[test]
fn close_waits_for_every_live_pane_once_and_another_window_stays_open() {
    let mut first = Lifecycle::default();
    let mut second = Lifecycle::default();
    assert_eq!(first.live_count(), 0);
    assert!(first.attach(0));
    assert!(!first.attach(0));
    assert_eq!(first.live_count(), 1);
    assert!(second.attach(0));
    assert!(first.attach(1));
    assert!(first.begin_close());
    assert!(!first.begin_close());
    assert!(!first.attach(1));
    first.drained(0);
    assert_eq!(first.phase(), Phase::Draining);
    first.drained(0);
    assert_eq!(first.phase(), Phase::Draining);
    first.drained(1);
    assert_eq!(first.phase(), Phase::Closed);
    assert_eq!(second.phase(), Phase::Open);
    let mut early = Lifecycle::default();
    assert!(early.begin_close());
    assert_eq!(early.phase(), Phase::Closed);
}

#[test]
fn window_geometry_has_one_listing_and_only_dual_reserves_a_second() {
    let state = settings("{}");
    let single = Geometry::measure(1100., 700., &state, View::List, false);
    assert_eq!(
        (
            single.panes[0].x,
            single.panes[0].width,
            single.rows_height(false, 0.)
        ),
        (192., 908., 619.)
    );
    let tabs = Geometry::measure(1100., 700., &state, View::List, true);
    assert_eq!((tabs.tabs.height, tabs.rows_height(false, 0.)), (27., 592.));
    // TabBar.qml opens on more than one tab in every view, so Dual reserves the strip too.
    let dual = Geometry::measure(1100., 700., &state, View::Dual, true);
    assert_eq!(
        (
            dual.rail.width,
            dual.tabs.height,
            dual.panes[0].width,
            dual.panes[1].x
        ),
        (192., 27., 453.5, 646.5)
    );
    for (width, height) in [(2536., 1386.), (1024., 899.), (100., 50.)] {
        let geometry = Geometry::measure(width, height, &state, View::Dual, false);
        assert!(geometry.rows_height(true, 27.) >= 0.);
        assert!(geometry
            .panes
            .iter()
            .all(|pane| pane.width >= 0. && pane.height >= 0.));
    }
}

#[test]
fn real_backend_pipes_drain_on_quit_and_report_unexpected_eof() {
    for graceful in [true, false] {
        let root = root();
        let mut command = Command::new("python3");
        command.args(["-c", "import pathlib,sys,time\nr=pathlib.Path(sys.argv[1]); line=sys.stdin.readline(); (r/'list').write_text(line)\nif sys.argv[2]=='quit':\n line=sys.stdin.readline(); (r/'quit').write_text(line); time.sleep(.05); (r/'drained').write_text('done')\n"])
            .arg(&root).arg(if graceful { "quit" } else { "eof" });
        let backend = Backend::start_command(
            command,
            "/literal/\"name\n".into(),
            Some("/literal/select".into()),
            321,
        )
        .unwrap();
        if graceful {
            backend.quit();
            backend.quit();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut failures = Vec::new();
        loop {
            assert!(Instant::now() < deadline, "backend did not reap");
            match backend.replies.try_recv() {
                Ok(Event::Drained) => break,
                Ok(Event::Failed(error)) => failures.push(error.key),
                Ok(Event::Garbled(error)) => panic!("unexpected garbled reply: {error:?}"),
                Ok(Event::Operations(_))
                | Ok(Event::Producer(_))
                | Ok(Event::Plugins(_))
                | Ok(Event::Reply(_))
                | Ok(Event::Refused { .. })
                | Ok(Event::Session { .. })
                | Ok(Event::Changed { .. })
                | Ok(Event::ReloadChanged(_))
                | Ok(Event::Peeked(_))
                | Ok(Event::Located(_))
                | Ok(Event::TrashLine(_))
                | Ok(Event::Path(_))
                | Ok(Event::Paths(_))
                | Ok(Event::ListDetails(_))
                | Ok(Event::Filesystem(_))
                | Ok(Event::FilesystemDetails { .. })
                | Ok(Event::FilesystemSystem { .. })
                | Err(async_channel::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("missing drain event: {error}"),
            }
        }
        let listed = std::fs::read_to_string(root.join("list")).unwrap();
        let json = jsondoc::parse(&listed).unwrap();
        assert_eq!(
            json.get("path").and_then(Json::as_str),
            Some("/literal/\"name\n")
        );
        assert_eq!(json.get("first").and_then(Json::as_f64), Some(321.));
        if graceful {
            assert!(failures.is_empty(), "{failures:?}");
            assert_eq!(
                std::fs::read_to_string(root.join("quit")).unwrap(),
                "{\"c\":\"quit\"}\n"
            );
            assert_eq!(
                std::fs::read_to_string(root.join("drained")).unwrap(),
                "done"
            );
        } else {
            assert_eq!(failures, ["window.backend_eof"]);
        }
    }
}

use super::*;

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

#[test]
fn verbs_parse_to_one_window_request() {
    assert_eq!(
        parse(&args(&["--action", "viewGrid"])).unwrap(),
        Options {
            window: None,
            request: Request::Call {
                target: "flea".into(),
                function: "runAction".into(),
                arguments: vec!["viewGrid".into()]
            },
        }
    );
    assert_eq!(
        parse(&args(&["--actions", "--window", "4242"])).unwrap(),
        Options {
            window: Some(4242),
            request: Request::Call {
                target: "flea".into(),
                function: "actionIds".into(),
                arguments: vec![]
            },
        }
    );
    for bad in [
        &["--action"][..],
        &["--action", "--window", "1"],
        &["--action", "a", "b"],
        &["--actions", "--window"],
        &["--actions", "--window", "0"],
        &["--actions", "--window", "x"],
    ] {
        assert_eq!(parse(&args(bad)).unwrap_err().code, USAGE, "{bad:?}");
    }
}

#[test]
fn focus_order_reads_hyprland_history_and_skips_unfocusable_clients() {
    let clients = r#"[{"pid":30,"focusHistoryID":2},{"pid":10,"focusHistoryID":0},{"pid":20,"focusHistoryID":-1},{"pid":40,"focusHistoryID":1}]"#;
    assert_eq!(focus_order(clients), vec![10, 40, 30]);
    assert!(focus_order("not json").is_empty());
}

#[test]
fn the_focused_flea_window_wins_then_the_newest() {
    let windows = vec![
        (10, PathBuf::from("/r/10.sock")),
        (20, PathBuf::from("/r/20.sock")),
    ];
    assert_eq!(
        choose(&windows, None, &[99, 10, 20]).unwrap(),
        PathBuf::from("/r/10.sock"),
        "a terminal focused last is skipped"
    );
    assert_eq!(
        choose(&windows, None, &[]).unwrap(),
        PathBuf::from("/r/20.sock")
    );
    assert_eq!(
        choose(&windows, Some(20), &[10]).unwrap(),
        PathBuf::from("/r/20.sock")
    );
    assert_eq!(choose(&windows, Some(30), &[]).unwrap_err().code, NO_WINDOW);
    assert_eq!(choose(&[], None, &[]).unwrap_err().code, NO_WINDOW);
}

// A short marked mktemp root keeps Unix sockets within SUN_LEN even on long build paths.
struct Root(PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        if self.0.parent() == Some(Path::new("/tmp")) && self.0.join(".flea-test-sandbox").is_file()
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn root() -> Root {
    let output = Command::new("mktemp")
        .arg("-d")
        .arg("/tmp/flea-action.XXXXXX")
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    std::fs::write(root.join(".flea-test-sandbox"), "action tests\n").unwrap();
    Root(root)
}

// A fake window that answers one request with `reply` and hands back the line it read.
fn window(root: &Path, reply: Reply) -> (endpoint::Endpoint, std::thread::JoinHandle<String>) {
    let endpoint = endpoint::Endpoint::bind(root).unwrap();
    let listener = endpoint.listener.try_clone().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let line = super::super::wire::read_line(
            &mut std::io::BufReader::new(stream.try_clone().unwrap()),
            4096,
        )
        .unwrap();
        super::super::wire::write_line(&mut stream, &reply.encode()).unwrap();
        line
    });
    (endpoint, handle)
}

#[test]
fn no_running_window_is_its_own_status() {
    let root = root();
    let options = parse(&args(&["--action", "viewGrid"])).unwrap();
    let reply = execute(&root.0, &options, || {
        panic!("one window or none never asks the compositor")
    });
    assert_eq!(
        reply,
        Reply::error(
            NO_WINDOW,
            "flea: no Flea window is running, so there is nothing to run the action in"
        )
    );
}

// A socket file whose window has gone refuses the connection, which is no window, not a refusal by one.
#[test]
fn a_socket_left_by_a_closed_window_is_no_window() {
    let root = root();
    let path = endpoint::Endpoint::bind(&root.0).unwrap().path.clone();
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    assert_eq!(
        endpoint::instances(&root.0).unwrap().len(),
        1,
        "the stale socket is listed, so the connect is what fails"
    );
    let options = parse(&args(&["--action", "viewGrid"])).unwrap();
    let reply = execute(&root.0, &options, Vec::new);
    assert_eq!(
        reply,
        Reply::error(
            NO_WINDOW,
            "flea: no Flea window is running, so there is nothing to run the action in"
        )
    );
}

#[test]
fn an_older_window_without_the_function_is_unavailable() {
    let root = root();
    let (_endpoint, window) = window(&root.0, Reply::value("Function not found."));
    let reply = execute(
        &root.0,
        &parse(&args(&["--action", "viewGrid"])).unwrap(),
        Vec::new,
    );
    assert_eq!(
        reply,
        Reply::error(
            UNAVAILABLE,
            "flea: that window is an older Flea that cannot run actions"
        )
    );
    window.join().unwrap();
}

#[test]
fn the_actions_list_travels_through_execute_as_a_list() {
    let root = root();
    let (_endpoint, window) = window(&root.0, Reply::value("viewGrid\nexample.checksums:sha256"));
    let options = parse(&args(&["--actions"])).unwrap();
    let reply = execute(&root.0, &options, Vec::new);
    assert_eq!(
        reply,
        Reply::value("viewGrid\nexample.checksums:sha256"),
        "an id list is a list, never an unexpected answer"
    );
    assert_eq!(
        Request::decode(window.join().unwrap().trim_end()).unwrap(),
        options.request
    );
}

#[path = "action_disconnect_tests.rs"]
mod disconnect;

// The request travels the same socket `flea ipc` uses, and the window's own refusal reaches the caller unchanged.
#[test]
fn the_window_socket_carries_the_request_and_its_refusal() {
    let root = root();
    let (_endpoint, window) = window(
        &root.0,
        Reply::error(UNKNOWN, "flea: no action has the id nope"),
    );
    let options = parse(&args(&["--action", "nope"])).unwrap();
    let reply = execute(&root.0, &options, Vec::new);
    assert_eq!(
        reply,
        Reply::error(UNKNOWN, "flea: no action has the id nope")
    );
    let sent = Request::decode(window.join().unwrap().trim_end()).unwrap();
    assert_eq!(sent, options.request);
}

#[test]
fn window_answers_outside_the_protocol_are_refusals() {
    assert_eq!(checked(false, Reply::value("")).code, RAN);
    assert_eq!(
        checked(true, Reply::value("viewGrid\nexample.checksums:sha256")).code,
        RAN
    );
    assert_eq!(
        checked(false, Reply::error(UNKNOWN, "flea: unknown")).code,
        UNKNOWN
    );
    assert_eq!(checked(false, Reply::error(3, "")).code, UNAVAILABLE);
    assert_eq!(checked(false, Reply::error(255, "busy")).code, UNAVAILABLE);
    for transport in [
        "Target not found.",
        "Too many arguments provided (0 required but 1 were provided.)",
    ] {
        assert_eq!(
            checked(false, Reply::value(transport)),
            Reply::error(
                UNAVAILABLE,
                format!("flea: the window could not run the action: {transport}")
            )
        );
        assert_eq!(
            checked(true, Reply::value(transport)),
            Reply::error(
                UNAVAILABLE,
                format!("flea: the window could not list its actions: {transport}")
            )
        );
    }
    assert_eq!(
        checked(true, Reply::value("")),
        Reply::error(
            UNAVAILABLE,
            "flea: the window could not list its actions: it answered nothing"
        )
    );
    let noisy = Reply {
        code: UNAVAILABLE,
        stdout: "x\n".into(),
        stderr: "flea: busy\nmore\n".into(),
    };
    assert_eq!(
        checked(false, noisy),
        Reply::error(UNAVAILABLE, "flea: busy"),
        "a refusal is one stderr line"
    );
}

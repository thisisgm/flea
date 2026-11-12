use super::*;

#[path = "lifetime_tests.rs"]
mod lifetime;

#[test]
fn u09_backend_pipe_preserves_peeked_replies_for_the_columns_owner() {
    let mut command = Command::new("/bin/bash");
    command.args(["-c", r#"IFS= read -r line
printf '%s\n' '{"t":"peeked","path":"/files/child","first":320,"n":1,"rows":[{"n":"seed.txt","d":false}]}'
while IFS= read -r line; do :; done
"#]);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    let _completion = backend.test_completion().unwrap();
    let event = backend.replies.recv_blocking().unwrap();
    let Event::Peeked(document) = event else {
        panic!("the backend discarded a watched peek: {event:?}")
    };
    let peek = crate::columns::peeks::Peeked::from_json(&document)
        .unwrap()
        .unwrap();
    assert_eq!(
        (peek.path.as_str(), peek.rows[0].name.as_str()),
        ("/files/child", "seed.txt")
    );
    backend.close_stdin();
    backend.drained().recv_blocking().unwrap();
}

#[test]
fn backend_argv_and_environment_match_the_qml_child() {
    let command = command();
    assert_eq!(command.get_args().collect::<Vec<_>>(), ["--backend"]);
    assert_eq!(command.get_envs().count(), 0);
    assert!(command.get_current_dir().is_none());
}

#[test]
fn u32_trash_transport_retains_opaque_identity_and_refuses_other_commands() {
    for line in [
        r#"{"t":"trashbrowse","id":9,"op":"prepare","ok":true,"token":42,"count":1}"#,
        r#"{"t":"trashthumbed","id":7,"file":"/private/cache/image.png"}"#,
    ] {
        match transport::parse(line).unwrap() {
            Event::TrashLine(actual) => assert_eq!(actual, line),
            event => panic!("lost Trash payload: {event:?}"),
        }
    }
    let (backend, _requests) = Backend::test_channel();
    assert!(backend
        .trash_line(r#"{"c":"trashbrowse","op":"prepare","identities":["opaque"]}"#.into())
        .is_ok());
    assert!(backend.trash_line(r#"{"c":"quit"}"#.into()).is_err());
    assert!(backend
        .trash_line("{\"c\":\"trashbrowse\"}\n{\"c\":\"quit\"}".into())
        .is_err());
}

#[test]
fn a_reload_listed_line_carries_its_changed_count_and_others_carry_none() {
    let reload = r#"{"t":"listed","path":"/files","n":4,"changed":2}"#;
    let event = transport::parse(reload).unwrap();
    assert_eq!(transport::reload_changed(&event, reload), Some(2));
    let plain = r#"{"t":"listed","path":"/files","n":4}"#;
    let event = transport::parse(plain).unwrap();
    assert_eq!(transport::reload_changed(&event, plain), None);
    let watch = r#"{"t":"changed","path":"/files"}"#;
    let event = transport::parse(watch).unwrap();
    assert_eq!(transport::reload_changed(&event, watch), None);
}

#[test]
fn a_reload_listed_line_from_the_child_sends_its_count_before_the_reply() {
    let mut command = Command::new("/bin/bash");
    command.args([
        "-c",
        r#"IFS= read -r line
printf '%s\n' '{"t":"listed","path":"/files","n":4,"changed":2}'
while IFS= read -r line; do :; done
"#,
    ]);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    let _completion = backend.test_completion().unwrap();
    let first = backend.replies.recv_blocking().unwrap();
    assert!(
        matches!(first, Event::ReloadChanged(2)),
        "the count goes first: {first:?}"
    );
    let second = backend.replies.recv_blocking().unwrap();
    assert!(
        matches!(
            &second,
            Event::Reply(flea::protocol::Reply::Listed { path, total: 4 }) if path == "/files"
        ),
        "then the listed reply itself: {second:?}"
    );
    backend.close_stdin();
    backend.drained().recv_blocking().unwrap();
}

// EY-468: a child line that is not protocol does not fail the pane, and the next reply still lands.
#[test]
fn a_garbled_child_line_is_not_a_backend_failure_and_the_next_reply_lands() {
    let mut command = Command::new("/bin/bash");
    command.args([
        "-c",
        r#"IFS= read -r line
printf '%s\n' 'harness: this line is not backend protocol' '{"t":"listed","path":"/files","n":4}'
while IFS= read -r line; do :; done
"#,
    ]);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    let _completion = backend.test_completion().unwrap();
    let first = backend.replies.recv_blocking().unwrap();
    assert!(
        matches!(first, Event::Garbled(_)),
        "one bad line is not a dead backend: {first:?}"
    );
    let second = backend.replies.recv_blocking().unwrap();
    assert!(
        matches!(
            &second,
            Event::Reply(flea::protocol::Reply::Listed { path, total: 4 }) if path == "/files"
        ),
        "the reply after it still lands: {second:?}"
    );
    backend.close_stdin();
    backend.drained().recv_blocking().unwrap();
}

#[test]
fn ey468_a_non_utf8_child_line_is_garbled_and_the_next_reply_lands() {
    let mut command = Command::new("/bin/bash");
    command.args([
        "-c",
        r#"IFS= read -r line
printf '\377\n%s\n' '{"t":"listed","path":"/files","n":4}'
while IFS= read -r line; do :; done
"#,
    ]);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    let _completion = backend.test_completion().unwrap();
    let first = backend.replies.recv_blocking().unwrap();
    assert!(
        matches!(first, Event::Garbled(_)),
        "bad bytes are one garbled line: {first:?}"
    );
    let second = backend.replies.recv_blocking().unwrap();
    assert!(
        matches!(&second, Event::Reply(flea::protocol::Reply::Listed { path, total: 4 }) if path == "/files"),
        "the valid reply after bad bytes still lands: {second:?}"
    );
    backend.close_stdin();
    backend.drained().recv_blocking().unwrap();
}

// A backend that let go of its output but that the kernel still holds drains the pane at once.
#[test]
fn a_quit_drains_once_the_backend_lets_go_of_its_output_before_it_can_exit() {
    let mut command = Command::new("/bin/bash");
    command.args([
        "-c",
        r#"while IFS= read -r line; do case $line in *'"quit"'*) break ;; esac; done
exec 1>&- 0<&-
sleep 10
"#,
    ]);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    let _completion = backend.test_completion().unwrap();
    let started = Instant::now();
    backend.quit();
    backend.drained().recv_blocking().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the pane waited {:?} on a backend whose output had ended",
        started.elapsed()
    );
}

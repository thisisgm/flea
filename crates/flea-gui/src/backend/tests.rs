use super::*;

#[test]
fn u09_backend_pipe_preserves_peeked_replies_for_the_columns_owner() {
    let mut command = Command::new("/bin/bash");
    command.args(["-c", r#"IFS= read -r line
printf '%s\n' '{"t":"peeked","path":"/files/child","first":320,"n":1,"rows":[{"n":"seed.txt","d":false}]}'
while IFS= read -r line; do :; done
"#]);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
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

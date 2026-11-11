//! Observe every request used by the mounted filesystem regressions, without filtering replies.
use super::*;

impl Backend {
    pub(crate) fn native31_requests() -> (Self, impl FnMut() -> Option<String>) {
        let (backend, receiver) = Self::test_controls();
        let receive = move || {
            receiver.try_recv().ok().map(|request| match request {
                Control::Line(line) => line,
                Control::Rows { command, action } => {
                    let mut bytes = Vec::new();
                    transport::write_rows(&mut bytes, command, &action).unwrap();
                    String::from_utf8(bytes).unwrap()
                }
                _ => panic!("unexpected non-detail request in the settled native31 fixture"),
            })
        };
        (backend, receive)
    }
}

#[test]
fn native31_full_wire_keeps_both_projections_and_class_only_adapter_compatibility() {
    let line =
        r#"{"t":"fsinfo","path":"/files","fs":"btrfs","free":7,"class":"network","tagable":false}"#;
    match transport::parse(line).unwrap() {
        Event::FilesystemDetails { facts, class } => {
            assert_eq!(
                (facts.path.as_str(), facts.name.as_str(), facts.free),
                ("/files", "btrfs", Some(7))
            );
            assert_eq!(class, "network");
        }
        event => panic!("real fsinfo lost one consumer: {event:?}"),
    }
    assert!(matches!(
        transport::parse(r#"{"t":"fsinfo","path":"/files","class":"network"}"#).unwrap(),
        Event::ListDetails(_)
    ));
    // S8-15: a named filesystem with no blocks answers -1, room unknown; a named one with no free is malformed.
    match transport::parse(
        r#"{"t":"fsinfo","path":"/files","class":"network","fs":"fuse","free":-1}"#,
    )
    .unwrap()
    {
        Event::FilesystemDetails { facts, .. } => {
            assert_eq!((facts.name.as_str(), facts.free), ("fuse", None))
        }
        event => panic!("a share with unknown room lost its disk facts: {event:?}"),
    }
    assert!(
        transport::parse(r#"{"t":"fsinfo","path":"/files","class":"","fs":"btrfs"}"#).is_err(),
        "malformed disk facts became a class-only reply"
    );
}

//! Failed Actions scripts report their own final diagnostic once.
use super::{Actions, Said};

#[test]
fn uish_misc_a_failed_script_job_says_its_last_line_once() {
    let mut actions = Actions::default();
    let _ = actions.run("script:ocr.sh", vec!["/fx/a.txt".into()]);
    actions.receive(r#"{"t":"actionstarted","requestId":1,"id":40,"ok":true,"error":""}"#);
    actions.receive(r#"{"t":"job","job":{"id":40,"kind":"action","state":"running","reason":"","output":[],"cancelled":false}}"#);
    assert!(actions.take_said().is_empty(), "a running job says nothing");
    let failed = r#"{"t":"job","job":{"id":40,"kind":"action","state":"failed","reason":"Action exited with exit status: 2.","output":["no such page"],"cancelled":false}}"#;
    actions.receive(failed);
    actions.receive(failed);
    assert_eq!(
        actions.take_said(),
        [Said::Line("ocr.sh \u{b7} no such page".into())]
    );
    let _ = actions.run("script:quiet.sh", vec!["/fx/a.txt".into()]);
    actions.receive(r#"{"t":"actionstarted","requestId":2,"id":41,"ok":true,"error":""}"#);
    actions.receive(r#"{"t":"job","job":{"id":41,"kind":"action","state":"failed","reason":"Action exited with exit status: 3.","output":[],"cancelled":false}}"#);
    assert_eq!(
        actions.take_said(),
        [Said::Key {
            key: "actions.script.exited",
            args: vec!["quiet.sh".into(), "3".into()]
        }]
    );
}

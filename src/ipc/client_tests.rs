use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[test]
fn call_and_show_keep_positional_arguments_as_data() {
    assert_eq!(
        parse(&args(&["--pid", "42", "call", "flea", "rowAt", "-1"])).unwrap(),
        Options {
            pid: Some(42),
            request: Request::Call {
                target: "flea".into(),
                function: "rowAt".into(),
                arguments: vec!["-1".into()]
            }
        }
    );
    assert_eq!(
        parse(&args(&["--any", "show"])).unwrap(),
        Options {
            pid: None,
            request: Request::Show
        }
    );
    assert_eq!(
        parse(&args(&["call"])),
        Err(Reply::value("Target required to send message."))
    );
    assert_eq!(
        parse(&args(&["call", "flea"])),
        Err(Reply::value("Function required to send message."))
    );
    for arguments in [
        &["--pid", "0", "show"][..],
        &["--pid", "-1", "show"],
        &["--pid", "42", "--any", "show"],
        &["show", "extra"],
        &["--unknown", "show"],
    ] {
        assert_eq!(parse(&args(arguments)).unwrap_err().code, 2);
    }
}

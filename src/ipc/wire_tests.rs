use super::*;
use std::io::Cursor;

#[test]
fn every_argument_is_data_on_one_line() {
    let request = Request::Call {
        target: "flea".into(),
        function: "path".into(),
        arguments: vec![
            " spaces \"quotes\"\nnewline\t\\backslash\0é".into(),
            "".into(),
        ],
    };
    let line = request.encode();
    assert_eq!(line.bytes().filter(|&byte| byte == b'\n').count(), 1);
    assert_eq!(Request::decode(&line).unwrap(), request);
    assert_eq!(
        Request::decode(&Request::Show.encode()).unwrap(),
        Request::Show
    );
}

#[test]
fn returns_printed_bytes_and_exit_code_without_losing_empty_or_multiline_values() {
    for reply in [
        Reply::value(""),
        Reply::value("a\nb\0é"),
        Reply::error(3, "FLEA_IPC_NOT_PORTED flea.palette"),
        Reply {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
    ] {
        let line = reply.encode();
        assert_eq!(line.lines().count(), 1);
        assert_eq!(Reply::decode(&line).unwrap(), reply);
    }
}

#[test]
fn bounded_lines_refuse_truncation_invalid_utf8_and_oversize() {
    assert_eq!(
        read_line(&mut Cursor::new(b"one\ntwo\n"), 4).unwrap(),
        "one\n"
    );
    for bytes in [b"no newline".as_slice(), b"1234\n", b"\xff\n"] {
        assert!(read_line(&mut Cursor::new(bytes), 4).is_err());
    }
    for line in [
        "{}",
        "{\"call\":[]}",
        "{\"call\":[1,2]}",
        "{\"show\":true,\"call\":[]}",
        "{\"show\":false}",
    ] {
        assert!(Request::decode(line).is_err(), "{line}");
    }
    assert!(Reply::decode("{\"code\":256,\"stdout\":\"\",\"stderr\":\"\"}").is_err());
}

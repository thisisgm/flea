//! The wire builders and the t:clip parser: every line the window sends and every reply shape the backend prints, from src/clip/reply.rs.
use super::{clip_clear_line, clip_get_line, clip_set_line, clip_watch_line, parse_clip, paste_image_line, ClipEvent};

#[test]
fn set_get_and_clear_lines_match_the_backend() {
    assert_eq!(
        clip_set_line("cut", &["/a/f2".to_string()]),
        r#"{"c":"clipSet","op":"cut","paths":["/a/f2"]}"#
    );
    assert_eq!(clip_get_line(), r#"{"c":"clipGet"}"#);
    assert_eq!(clip_watch_line(), r#"{"c":"clipWatch"}"#);
    assert_eq!(
        clip_clear_line("cut-token", &[]),
        r#"{"c":"clipClear","token":"cut-token"}"#
    );
    assert_eq!(
        clip_clear_line("", &["/foreign/f2".to_string()]),
        r#"{"c":"clipClear","cut":["/foreign/f2"]}"#
    );
    assert_eq!(
        paste_image_line("/dest", "Pasted image 2026-09-23 at 10.14.32.png"),
        r#"{"c":"pasteimage","dest":"/dest","name":"Pasted image 2026-09-23 at 10.14.32.png"}"#
    );
}

#[test]
fn every_reply_shape_parses_once() {
    assert_eq!(
        parse_clip(r#"{"t":"clip","op":"set","ok":true,"token":"own"}"#).unwrap(),
        Some(ClipEvent::Set { ok: true, token: "own".into(), error: "".into() })
    );
    assert_eq!(
        parse_clip(r#"{"t":"clip","op":"set","ok":false,"error":"no compositor"}"#).unwrap(),
        Some(ClipEvent::Set { ok: false, token: "".into(), error: "no compositor".into() })
    );
    assert_eq!(
        parse_clip(r#"{"t":"clip","op":"changed","clip":"cut","paths":["/a/f2"],"token":"t","skipped":0}"#).unwrap(),
        Some(ClipEvent::Changed {
            clip: "cut".into(),
            paths: vec!["/a/f2".into()],
            token: "t".into(),
            error: "".into()
        })
    );
    assert_eq!(
        parse_clip(r#"{"t":"clip","op":"changed","clip":"none","paths":[],"token":"","skipped":0,"error":"x"}"#).unwrap(),
        Some(ClipEvent::Changed {
            clip: "none".into(),
            paths: Vec::new(),
            token: "".into(),
            error: "x".into()
        })
    );
    assert_eq!(
        parse_clip(r#"{"t":"clip","op":"get","ok":true,"clip":"copy","paths":["/f"],"token":"","skipped":0}"#).unwrap(),
        Some(ClipEvent::Get {
            ok: true,
            clip: "copy".into(),
            paths: vec!["/f".into()],
            token: "".into(),
            error: "".into()
        })
    );
    assert_eq!(
        parse_clip(r#"{"t":"clip","op":"clear","ok":false,"error":"foreign owner refused"}"#).unwrap(),
        Some(ClipEvent::Clear { ok: false, error: "foreign owner refused".into() })
    );
}

#[test]
fn other_lines_are_none_and_unknown_ops_refuse() {
    assert_eq!(parse_clip(r#"{"t":"job","op":"update"}"#).unwrap(), None);
    assert!(parse_clip(r#"{"t":"clip","op":"take"}"#).is_err());
    assert!(parse_clip(r#"{"t":"clip","op":"set","ok":true,"token":1}"#).is_err());
}

//! The actions wire against `src/backend/actions.rs`: request shapes and every refusal.
use super::wire::*;

fn listing(paths: &[&str]) -> Listing {
    parse_listing(&format!(
        "{{\"t\":\"actions\",\"id\":4,\"actions\":[{{\"id\":\"action:resize.toml\",\"name\":\"Resize to 1600 px\",\"mode\":\"background\",\"selection\":\"many\",\"script\":false,\"mime\":[\"image/*\",\"image/x-raw\"]}},{{\"id\":\"script:ocr.sh\",\"name\":\"ocr\",\"mode\":\"background\",\"selection\":\"many\",\"script\":true}}],\"errors\":[{{\"file\":\"broken.toml\",\"error\":\"unknown manifest key\"}}],\"error\":\"{}\"}}",
        paths.join(",")
    ))
    .unwrap()
}

#[test]
fn mn2_list_carries_the_menus_own_snapshot() {
    assert_eq!(
        list_request(4, &["/fx/a.jpg".into(), "/fx/b.jpg".into()]),
        r#"{"c":"actions","op":"list","id":4,"paths":["/fx/a.jpg","/fx/b.jpg"]}"#
    );
}

#[test]
fn mn2_run_names_the_action_and_the_snapshot() {
    assert_eq!(
        run_request(5, "action:resize.toml", &["/fx/a.jpg".into()]),
        r#"{"c":"actions","op":"run","id":5,"action":"action:resize.toml","paths":["/fx/a.jpg"]}"#
    );
}

#[test]
fn mn2_preview_names_the_cursor_row_alone() {
    assert_eq!(
        preview_request(6, "action:inspect.toml", &["/fx/a.jpg".into()]),
        r#"{"c":"actions","op":"preview","id":6,"action":"action:inspect.toml","paths":["/fx/a.jpg"]}"#
    );
}

#[test]
fn mn2_a_listing_keeps_actions_errors_and_its_error() {
    let back = listing(&[]);
    assert_eq!(back.id, 4);
    assert_eq!(back.actions.len(), 2);
    assert_eq!(
        (back.actions[0].id.as_str(), back.actions[0].script),
        ("action:resize.toml", false)
    );
    assert_eq!(
        (back.actions[1].id.as_str(), back.actions[1].script),
        ("script:ocr.sh", true)
    );
    assert_eq!(back.actions[0].mime, ["image/*", "image/x-raw"], "each action carries its own globs");
    assert!(back.actions[1].mime.is_empty(), "a reply without globs reads none");
    assert_eq!(
        back.errors,
        [Discovery {
            file: "broken.toml".into(),
            error: "unknown manifest key".into()
        }]
    );
    assert_eq!(back.error, "");
}

#[test]
fn mn2_a_malformed_or_foreign_reply_is_no_listing() {
    assert!(parse_listing("{broken").is_none());
    assert!(parse_listing(r#"{"t":"jobs","id":1}"#).is_none());
    assert!(parse_listing(r#"{"t":"actions","id":1}"#).is_none());
}

#[test]
fn mn2_a_preview_carries_code_text_or_its_fixed_failure() {
    let ok =
        parse_preview(r#"{"t":"actionpreview","id":6,"ok":true,"text":"1600x1200","error":""}"#)
            .unwrap();
    assert_eq!((ok.ok, ok.text.as_str()), (true, "1600x1200"));
    let failed = parse_preview(
        r#"{"t":"actionpreview","id":6,"ok":false,"text":"","error":"Preview output timed out."}"#,
    )
    .unwrap();
    assert_eq!(
        (failed.ok, failed.error.as_str()),
        (false, "Preview output timed out.")
    );
    assert!(parse_preview(r#"{"t":"actions","id":6}"#).is_none());
}

#[test]
fn mn2_a_start_names_the_job_or_its_refusal() {
    let ok = parse_started(r#"{"t":"actionstarted","requestId":5,"id":11,"ok":true,"error":""}"#)
        .unwrap();
    assert_eq!((ok.request, ok.id, ok.ok), (5, 11, true));
    let refused = parse_started(r#"{"t":"actionstarted","requestId":5,"ok":false,"error":"That action no longer matches the selection."}"#).unwrap();
    assert_eq!(
        refused.error,
        "That action no longer matches the selection."
    );
}

#[test]
fn mn2_every_backend_refusal_has_its_own_key() {
    let cases = [
        ("That action no longer matches the selection.", STALE_KEY),
        (
            "Preview actions run only for the cursor row.",
            CURSOR_ONLY_KEY,
        ),
        ("Unknown Actions request.", UNKNOWN_KEY),
        ("Actions require absolute selection paths.", PATHS_KEY),
        ("Action cancelled.", CANCELLED_KEY),
        (
            "Preview actions require only the cursor row.",
            PREVIEW_CURSOR_KEY,
        ),
        ("Preview sandbox unavailable.", PREVIEW_SANDBOX_KEY),
        ("Preview superseded.", PREVIEW_SUPERSEDED_KEY),
        (
            "Preview actions require a regular cursor file.",
            PREVIEW_FILE_KEY,
        ),
        ("Preview program is unavailable.", PREVIEW_PROGRAM_KEY),
        ("Preview output unavailable.", PREVIEW_OUTPUT_KEY),
        ("Preview action failed.", PREVIEW_FAILED_KEY),
        ("Preview output timed out.", PREVIEW_TIMEOUT_KEY),
        ("Preview output exceeds 1048576 bytes.", PREVIEW_LARGE_KEY),
    ];
    for (error, key) in cases {
        assert_eq!(refusal_key(error), Some(key), "{error}");
    }
    assert_eq!(
        refusal_key("Action exited with exit status: 2."),
        Some(EXITED_KEY)
    );
    assert_eq!(
        refusal_key("Action exited with signal: 9."),
        Some(EXITED_KEY)
    );
    assert_eq!(refusal_key("a script's own last line"), None);
}

#[test]
fn mn2_every_manifest_diagnostic_has_its_own_key() {
    let cases = [
        (
            "too many manifests",
            "actions.manifest.too_many",
            vec!["r.toml"],
        ),
        (
            "not a regular readable manifest",
            "actions.manifest.unreadable",
            vec!["r.toml"],
        ),
        (
            "manifest exceeds 16384 bytes",
            "actions.manifest.too_large",
            vec!["r.toml"],
        ),
        (
            "manifest is not UTF-8",
            "actions.manifest.encoding",
            vec!["r.toml"],
        ),
        (
            "unknown manifest key",
            "actions.manifest.unknown_key",
            vec!["r.toml"],
        ),
        (
            "duplicate manifest key",
            "actions.manifest.duplicate_key",
            vec!["r.toml"],
        ),
        (
            "unexpected text after value",
            "actions.manifest.trailing",
            vec!["r.toml"],
        ),
        (
            "name must be a nonempty string",
            "actions.manifest.empty_text",
            vec!["r.toml", "name"],
        ),
        (
            "selection must be one or many",
            "actions.manifest.selection",
            vec!["r.toml"],
        ),
        (
            "unknown action mode",
            "actions.manifest.mode",
            vec!["r.toml"],
        ),
        (
            "mime must be a nonempty string array",
            "actions.manifest.empty_list",
            vec!["r.toml", "mime"],
        ),
        (
            "MIME globs accept literal characters, * and ?",
            "actions.manifest.mime",
            vec!["r.toml"],
        ),
        (
            "the program must be literal",
            "actions.manifest.program",
            vec!["r.toml"],
        ),
        (
            "unclosed placeholder",
            "actions.manifest.unclosed",
            vec!["r.toml"],
        ),
        (
            "unknown placeholder",
            "actions.manifest.placeholder",
            vec!["r.toml"],
        ),
    ];
    for (error, key, args) in cases {
        let (found, found_args) = manifest_key("r.toml", error).unwrap();
        assert_eq!(
            (found, found_args),
            (key, args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        );
    }
    assert_eq!(manifest_key("r.toml", "a sentence no backend sends"), None);
}

#[test]
fn mn2_an_exited_refusal_carries_its_status_as_the_argument() {
    assert_eq!(
        refusal_args("Action exited with exit status: 2."),
        vec!["exit status: 2".to_string()]
    );
    assert!(refusal_args("That action no longer matches the selection.").is_empty());
}

#[test]
fn mn2_every_manifest_syntax_fault_has_its_own_key() {
    for error in [
        "too many array values",
        "expected =",
        "expected ]",
        "expected ,",
        "expected \"",
        "invalid string escape",
        "control character in string",
        "unclosed string",
    ] {
        assert_eq!(
            manifest_key("r.toml", error),
            Some((SYNTAX_KEY, vec!["r.toml".to_string(), error.to_string()])),
            "the syntax key with the file and the fault for {error}"
        );
    }
    assert!(manifest_key("r.toml", "a sentence no backend sends").is_none());
}

#[test]
fn mn2_a_manifest_read_fault_names_the_file_and_the_reason() {
    let (key, args) = manifest_key("r.toml", "Is a directory (os error 21)").unwrap();
    assert_eq!(key, "actions.manifest.io");
    assert_eq!(args, vec!["r.toml", "Is a directory (os error 21)"]);
}

#[test]
fn mn2_a_terminal_failure_is_said_not_lost() {
    let failed = parse_started(
        r#"{"t":"actionstarted","requestId":5,"ok":false,"error":"empty terminal action"}"#,
    )
    .unwrap();
    assert!(!failed.ok);
    assert_eq!(
        refusal_key(&failed.error),
        None,
        "a backend sentence stays data"
    );
}

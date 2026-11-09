use super::{protocol, Config, Runtime};
use crate::backend::testdir::TestDir;
use crate::jsondoc::{self, Json};
use crate::ui_logic_plugin_settings::{manifest, wire};
use std::time::{Duration, Instant};

fn json(text: &str) -> Json {
    jsondoc::parse(text).unwrap()
}

fn config(dir: &TestDir, script: &str) -> Config {
    dir.script("run", script);
    let manifest = manifest::parse(r#"{"schemaVersion":1,"id":"example.test","name":"Test","version":"1","author":"Flea","description":"Test plugin.","kinds":["menu-action","rail-section"],"entryPoints":{"run":"run"},"menuActions":[{"id":"run","label":"Run"}],"railSection":{"title":"Test"}}"#, false).unwrap();
    Config {
        folder: dir.path().to_owned(),
        manifest,
        hello: json(
            r#"{"op":"hello","protocolVersion":1,"flea":"0.4.0","language":"en","decimal":".","settings":{},"kinds":["menu-action","rail-section"]}"#,
        ),
        ready_timeout: Duration::from_millis(500),
        request_timeout: Duration::from_millis(500),
        idle_timeout: Duration::from_secs(30),
    }
}

const ECHO: &str = "#!/bin/sh\nread hello\nprintf '%s\\n' '{\"op\":\"ready\"}'\nwhile IFS= read -r line; do printf '%s\\n' '{\"op\":\"done\"}'; done\n";

#[test]
fn plugin_runtime_preserves_blocks_through_settings_wire() {
    let dir = TestDir::new("plugin-blocks");
    let config = config(&dir, ECHO);
    let plugin = wire::Plugin {
        manifest: config.manifest.clone(),
        source: wire::Source::Unknown,
        enabled: true,
        failure: None,
        mark_path: String::new(),
        settings: Vec::new(),
        shelf: None,
    };
    let line = wire::listing_line(&wire::Listing {
        id: 1,
        plugins: vec![plugin],
    });
    let wire::Reply::Listing(list) = wire::parse(&line).unwrap() else {
        panic!("listing")
    };
    assert_eq!(list.plugins[0].manifest.blocks, config.manifest.blocks);
    assert!(list.plugins[0].manifest.block("rail-section").is_some());
    assert!(list.plugins[0].manifest.block("place").is_none());
}

#[test]
fn plugin_runtime_reuses_one_process_and_shutdown_reaps_it() {
    let dir = TestDir::new("plugin-lasting");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' $$ >> '{}/starts'\n{}",
        dir.path().display(),
        ECHO.trim_start_matches("#!/bin/sh\n")
    );
    let runtime = Runtime::new(config(&dir, &script));
    runtime.ensure_ready().unwrap();
    for _ in 0..3 {
        runtime
            .request(json(r#"{"op":"run","action":"run","paths":[]}"#))
            .unwrap();
    }
    let starts = std::fs::read_to_string(dir.join("starts")).unwrap();
    assert_eq!(starts.lines().count(), 1);
    let pid = starts.trim();
    assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
    drop(runtime);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

#[test]
fn plugin_runtime_hang_crash_and_malformed_are_bounded() {
    for (tag, script) in [
        ("hang", "#!/bin/sh\nsleep 30\n"),
        ("crash", "#!/bin/sh\nexit 4\n"),
        ("malformed", "#!/bin/sh\nread line\necho not-json\n"),
        (
            "oversize",
            "#!/bin/sh\nread line\nhead -c 70000 /dev/zero | tr '\\000' x\nsleep 30\n",
        ),
    ] {
        let dir = TestDir::new(&format!("plugin-{tag}"));
        let runtime = Runtime::new(config(&dir, script));
        let started = Instant::now();
        assert!(runtime.ensure_ready().is_err(), "{tag}");
        assert!(started.elapsed() < Duration::from_secs(3), "{tag}");
        assert!(runtime.failure().is_some());
    }
}

#[test]
fn plugin_runtime_starts_again_on_each_request_after_a_failure() {
    // EY-553: the owner blocks a failed plugin itself, so the session holds no budget that refuses a fourth start.
    let dir = TestDir::new("plugin-restart");
    let script = format!(
        "#!/bin/sh\necho start >> starts\n[ \"$(wc -l < starts)\" -gt 3 ] || exit 4\n{}",
        ECHO.trim_start_matches("#!/bin/sh\n")
    );
    let runtime = Runtime::new(config(&dir, &script));
    for _ in 0..3 {
        assert!(runtime.ensure_ready().is_err());
        assert!(runtime.failure().is_some());
    }
    runtime.ensure_ready().unwrap();
    assert!(runtime.failure().is_none());
    assert_eq!(
        std::fs::read_to_string(dir.join("starts"))
            .unwrap()
            .lines()
            .count(),
        4
    );
}

#[test]
fn plugin_runtime_declared_kinds_fail_closed() {
    let dir = TestDir::new("plugin-kinds");
    let config = config(&dir, ECHO);
    assert!(protocol::operation(&config.manifest, &json(r#"{"op":"targets"}"#)).is_err());
    assert!(protocol::operation(
        &config.manifest,
        &json(r#"{"op":"run","action":"run","paths":["../outside"]}"#)
    )
    .is_err());
    assert!(protocol::parse(&vec![b' '; protocol::MAX_MESSAGE + 1]).is_err());
}

#[test]
fn plugin_runtime_protocol_data_matches_all_six_kinds() {
    let data = json(protocol::CONTRACT);
    assert_eq!(
        data.get("protocolVersion").and_then(Json::as_f64),
        Some(protocol::VERSION as f64)
    );
    let operations = data.get("operations").and_then(Json::as_array).unwrap();
    assert_eq!(operations.len(), protocol::OPERATIONS.len());
    for ((op, kind, response), row) in protocol::OPERATIONS.iter().zip(operations) {
        assert_eq!(protocol::text(row, "op").unwrap(), *op);
        assert_eq!(protocol::text(row, "kind").unwrap(), *kind);
        assert_eq!(protocol::text(row, "response").unwrap(), *response);
    }
    for (kind, _) in manifest::KINDS {
        assert!(protocol::OPERATIONS.iter().any(|(_, k, _)| *k == kind));
    }
}

#[test]
fn plugin_runtime_progress_arrives_before_completion_and_shutdown_cancels_a_hang() {
    let dir = TestDir::new("plugin-progress");
    let mut config = config(&dir, "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nread run\necho '{\"op\":\"progress\",\"done\":1,\"total\":2,\"message\":\"first\"}'\nsleep 30\n");
    config.request_timeout = Duration::from_secs(5);
    let runtime = Runtime::new(config);
    runtime.ensure_ready().unwrap();
    let cancel = runtime.clone();
    let mut seen = Vec::new();
    let started = Instant::now();
    let result = runtime.request_stream(
        json(r#"{"op":"run","action":"run","paths":[]}"#),
        &mut |message| {
            seen.push(message);
            cancel.shutdown();
        },
    );
    assert_eq!(result.unwrap_err(), "plugin stopped");
    assert_eq!(seen.len(), 1);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(protocol::response(&json(r#"{"op":"progress","done":3,"total":2}"#), "done").is_err());
}

#[test]
fn plugin_runtime_invalid_requests_do_not_kill_the_child() {
    let dir = TestDir::new("plugin-request-refusals");
    let script = format!(
        "#!/bin/sh\necho $$ >> '{}/starts'\n{}",
        dir.path().display(),
        ECHO.trim_start_matches("#!/bin/sh\n")
    );
    let runtime = Runtime::new(config(&dir, &script));
    runtime.ensure_ready().unwrap();
    let pid = std::fs::read_to_string(dir.join("starts")).unwrap();
    for _ in 0..4 {
        assert_eq!(
            runtime.request(json(r#"{"op":"unknown"}"#)).unwrap_err(),
            "unknown plugin operation"
        );
        assert!(runtime.failure().is_none());
        assert!(std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
    }
    assert_eq!(
        runtime
            .request(json(r#"{"op":"run","action":"run","paths":[]}"#))
            .unwrap()
            .message
            .get("op")
            .and_then(Json::as_str),
        Some("done")
    );
    assert_eq!(std::fs::read_to_string(dir.join("starts")).unwrap(), pid);
}

#[test]
fn plugin_runtime_operation_error_keeps_the_session_healthy() {
    let dir = TestDir::new("plugin-operation-error");
    let runtime = Runtime::new(config(&dir, "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nwhile read run; do echo '{\"op\":\"error\",\"message\":\"Destination unavailable\"}'; done\n"));
    for _ in 0..4 {
        let reply = runtime
            .request(json(r#"{"op":"run","action":"run","paths":[]}"#))
            .unwrap();
        assert_eq!(
            reply.message.get("op").and_then(Json::as_str),
            Some("error")
        );
        assert!(runtime.failure().is_none());
    }
}

#[test]
fn plugin_runtime_idle_stop_is_clean_and_next_request_restarts() {
    let dir = TestDir::new("plugin-idle-stop");
    let script = format!(
        "#!/bin/sh\necho $$ >> '{}/starts'\n{}",
        dir.path().display(),
        ECHO.trim_start_matches("#!/bin/sh\n")
    );
    let mut config = config(&dir, &script);
    config.idle_timeout = Duration::from_millis(40);
    let runtime = Runtime::new(config);
    runtime.ensure_ready().unwrap();
    let pid = std::fs::read_to_string(dir.join("starts"))
        .unwrap()
        .trim()
        .to_owned();
    let started = Instant::now();
    while std::path::Path::new(&format!("/proc/{pid}")).exists()
        && started.elapsed() < Duration::from_secs(2)
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert!(runtime.failure().is_none());
    runtime
        .request(json(r#"{"op":"run","action":"run","paths":[]}"#))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("starts"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn plugin_runtime_declared_action_input_is_validated_before_delivery() {
    let raw = r#"{"schemaVersion":1,"id":"example.verify","name":"Verify","version":"1","author":"Example","description":"Verify a checksum.","kinds":["menu-action"],"entryPoints":{"run":"run"},"menuActions":[{"id":"verify","label":"Verify Checksum…","input":{"key":"checksum","title":"Verify Checksum","label":"Checksum","submit":"Verify","maxChars":128}}]}"#;
    let manifest = manifest::parse(raw, false).unwrap();
    let descriptor =
        crate::ui_logic_plugin_settings::contributions::action_input(&manifest, "verify")
            .unwrap()
            .unwrap();
    assert_eq!(descriptor.key, "checksum");
    assert_eq!(descriptor.max_chars, 128);
    assert!(protocol::operation(
        &manifest,
        &json(r#"{"op":"run","action":"verify","paths":[],"input":{"checksum":"abc"}}"#)
    )
    .is_ok());
    for bad in [
        r#"{"op":"run","action":"verify","paths":[]}"#,
        r#"{"op":"run","action":"verify","paths":[],"input":{"other":"abc"}}"#,
        r#"{"op":"run","action":"verify","paths":[],"input":{"checksum":""}}"#,
        r#"{"op":"run","action":"verify","paths":[],"input":{"checksum":"abc\n"}}"#,
    ] {
        assert!(protocol::operation(&manifest, &json(bad)).is_err(), "{bad}");
    }
    assert!(manifest::parse(&raw.replace("\"maxChars\":128", "\"maxChars\":5000"), false).is_err());
}

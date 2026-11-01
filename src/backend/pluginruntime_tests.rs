use super::plugins_tests::{ask, code, env, get, plugin};
use super::testdir::TestDir;
use flea::jsondoc::{self, Json};
use flea::ui_logic_st2::wire;

fn running(dir: &TestDir, name: &str) -> String {
    std::fs::read_to_string(dir.join(name))
        .unwrap()
        .trim()
        .to_owned()
}

#[test]
fn plugin_runtime_backend_enable_run_disable_uses_one_child() {
    let dir = TestDir::new("plugin-runtime-lifecycle");
    let env = env(&dir);
    let source = format!(
        "#!/bin/sh\nprintf '%s\\n' $$ > '{}/pid'\nwhile IFS= read -r line; do\ncase $line in\n*'\"op\":\"hello\"'*) echo '{{\"op\":\"ready\"}}';;\n*) printf '%s\\n' \"$line\" >> '{}/requests'; echo '{{\"op\":\"done\"}}';;\nesac\ndone\n",
        dir.path().display(), dir.path().display()
    );
    plugin(
        &dir,
        "config/flea/plugins/example.lifecycle",
        "example.lifecycle",
        &source,
    );
    let (outcome, listing) = ask(
        &env,
        r#"{"c":"plugins","op":"enable","id":1,"plugin":"example.lifecycle"}"#,
    );
    assert_eq!(code(outcome), "");
    assert!(get(&listing, "example.lifecycle").failure.is_none());
    let pid = running(&dir, "pid");
    let selected = dir.file("selected.txt", "selected\n");
    let request = format!(
        r#"{{"c":"plugins","op":"runtime","id":2,"plugin":"example.lifecycle","request":{{"op":"run","action":"go","paths":["{}"]}}}}"#,
        selected.display()
    );
    let lines = super::plugins::handle(&request, &env);
    let reply = jsondoc::parse(&lines[0]).unwrap();
    assert_eq!(reply.get("t").and_then(Json::as_str), Some("pluginruntime"));
    assert_eq!(reply.get("ok").and_then(Json::as_bool), Some(true));
    assert_eq!(running(&dir, "pid"), pid);
    let received = jsondoc::parse(&running(&dir, "requests")).unwrap();
    assert_eq!(
        received.get("paths").and_then(Json::as_array).unwrap(),
        &[Json::Str(selected.display().to_string())]
    );
    ask(
        &env,
        r#"{"c":"plugins","op":"disable","id":3,"plugin":"example.lifecycle"}"#,
    );
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    let lines = super::plugins::handle(&request, &env);
    assert_eq!(
        jsondoc::parse(&lines[0])
            .unwrap()
            .get("ok")
            .and_then(Json::as_bool),
        Some(false)
    );
}

#[test]
fn plugin_runtime_backend_failed_answer_stays_failed_after_listing() {
    let dir = TestDir::new("plugin-runtime-bad-answer");
    let env = env(&dir);
    plugin(
        &dir,
        "config/flea/plugins/example.bad-answer",
        "example.bad-answer",
        "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nread run\necho broken\nsleep 30\n",
    );
    ask(
        &env,
        r#"{"c":"plugins","op":"enable","id":1,"plugin":"example.bad-answer"}"#,
    );
    let lines = super::plugins::handle(
        r#"{"c":"plugins","op":"runtime","id":2,"plugin":"example.bad-answer","request":{"op":"run","action":"go","paths":[]}}"#,
        &env,
    );
    assert_eq!(
        jsondoc::parse(&lines[0])
            .unwrap()
            .get("ok")
            .and_then(Json::as_bool),
        Some(false)
    );
    for id in 3..5 {
        let (_, listing) = ask(&env, &wire::request_line(id, "list", "", Vec::new()));
        assert_eq!(
            get(&listing, "example.bad-answer")
                .failure
                .as_ref()
                .map(|e| e.code),
            Some("start")
        );
    }
    ask(
        &env,
        r#"{"c":"plugins","op":"disable","id":5,"plugin":"example.bad-answer"}"#,
    );
}

#[test]
fn plugin_runtime_backend_refuses_undeclared_kind_without_starting() {
    let dir = TestDir::new("plugin-runtime-kind");
    let env = env(&dir);
    plugin(
        &dir,
        "config/flea/plugins/example.kind",
        "example.kind",
        super::plugins_tests::READY,
    );
    let request = r#"{"c":"plugins","op":"runtime","id":1,"plugin":"example.kind","request":{"op":"section"}}"#;
    assert_eq!(
        jsondoc::parse(&super::plugins::handle(request, &env)[0])
            .unwrap()
            .get("ok")
            .and_then(Json::as_bool),
        Some(false)
    );
    ask(
        &env,
        r#"{"c":"plugins","op":"enable","id":2,"plugin":"example.kind"}"#,
    );
    assert_eq!(
        jsondoc::parse(&super::plugins::handle(request, &env)[0])
            .unwrap()
            .get("ok")
            .and_then(Json::as_bool),
        Some(false)
    );
    ask(
        &env,
        r#"{"c":"plugins","op":"disable","id":3,"plugin":"example.kind"}"#,
    );
}

#[test]
fn plugin_runtime_shelf_kind_switches_the_named_omarchy_plugin_and_draws_its_schema() {
    use flea::ui_logic_st2::pages::{self, Row, Update};
    let dir = TestDir::new("plugin-runtime-shelf");
    let mut env = env(&dir);
    env.plugin_manager = dir.script(
        "omarchy",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}/manager'\n",
            dir.path().display()
        ),
    );
    plugin(
        &dir,
        "config/flea/plugins/example.shelf",
        "example.shelf",
        super::plugins_tests::READY,
    );
    dir.file("config/flea/plugins/example.shelf/manifest.json", r#"{"schemaVersion":1,"id":"example.shelf","name":"Team Shelf","version":"1","author":"Example","description":"A shelf plugin.","kinds":["shelf"],"entryPoints":{"run":"bin/run"},"shelf":{"omarchyPlugin":"example.team-shelf"},"schema":[{"key":"compact","type":"boolean","label":"Compact","defaultValue":false}]}"#);
    let (result, listing) = ask(
        &env,
        r#"{"c":"plugins","op":"enable","id":1,"plugin":"example.shelf"}"#,
    );
    assert_eq!(code(result), "");
    let found = get(&listing, "example.shelf");
    assert!(found.enabled && found.failure.is_none());
    let rows = pages::page_rows(
        found,
        &Update::Idle,
        &crate::i18n::test_catalogs::library_english(),
    );
    assert!(rows.iter().any(|row| matches!(row, Row::Check { id, label, available: true, .. } if id == "example.shelf:compact" && label == "Compact")));
    assert_eq!(running(&dir, "manager"), "plugin enable example.team-shelf");
    ask(
        &env,
        r#"{"c":"plugins","op":"disable","id":2,"plugin":"example.shelf"}"#,
    );
    assert_eq!(
        running(&dir, "manager").lines().last(),
        Some("plugin disable example.team-shelf")
    );
}

#[test]
fn plugin_runtime_shelf_retry_repeats_failed_external_sync_before_clearing_failure() {
    let dir = TestDir::new("plugin-shelf-retry");
    let mut env = env(&dir);
    env.plugin_manager = dir.script("manager", &format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{0}/attempts'\nif [ ! -f '{0}/first' ]; then touch '{0}/first'; exit 1; fi\n",
        dir.path().display()
    ));
    plugin(
        &dir,
        "config/flea/plugins/example.shelf-retry",
        "example.shelf-retry",
        super::plugins_tests::READY,
    );
    dir.file("config/flea/plugins/example.shelf-retry/manifest.json", r#"{"schemaVersion":1,"id":"example.shelf-retry","name":"Retry","version":"1","author":"Example","description":"A shelf plugin.","kinds":["shelf"],"entryPoints":{"run":"bin/run"},"shelf":{"omarchyPlugin":"example.retry"}}"#);
    let enable = r#"{"c":"plugins","op":"enable","id":1,"plugin":"example.shelf-retry"}"#;
    let (_, first) = ask(&env, enable);
    let failed = get(&first, "example.shelf-retry");
    assert!(failed.enabled && failed.failure.is_some());
    let (_, second) = ask(&env, enable);
    let retried = get(&second, "example.shelf-retry");
    assert!(retried.enabled && retried.failure.is_none());
    assert_eq!(running(&dir, "attempts").lines().count(), 2);
    ask(&env, enable);
    assert_eq!(running(&dir, "attempts").lines().count(), 2);
    ask(
        &env,
        r#"{"c":"plugins","op":"disable","id":2,"plugin":"example.shelf-retry"}"#,
    );
}

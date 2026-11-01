use super::{session_at, shutdown_at, Registry};
use crate::backend::plugins_tests::{env, plugin};
use crate::backend::testdir::TestDir;
use std::sync::{mpsc, Arc, Mutex};

#[test]
fn plugin_runtime_shutdown_closes_registration_before_delayed_workers_can_start() {
    let dir = TestDir::new("plugin-shutdown-registration");
    let env = env(&dir);
    plugin(&dir, "config/flea/plugins/example.shutdown", "example.shutdown", &format!(
        "#!/bin/sh\necho $$ >> '{}/starts'\nread hello\necho '{{\"op\":\"ready\"}}'\ncat >/dev/null\n",
        dir.path().display()
    ));
    let found = crate::backend::plugins::find(&env, "example.shutdown").unwrap();
    let registry = Arc::new(Mutex::new(Registry {
        closed: false,
        live: None,
    }));
    let runtime = session_at(&registry, &env, &found).unwrap();
    runtime.ensure_ready().unwrap();
    let pid = std::fs::read_to_string(dir.join("starts")).unwrap();
    let (resume, delayed) = mpsc::channel();
    std::thread::scope(|scope| {
        let worker_registry = registry.clone();
        let env = &env;
        let found = &found;
        let worker = scope.spawn(move || {
            delayed.recv().unwrap();
            match session_at(&worker_registry, env, found) {
                Ok(runtime) => runtime.ensure_ready(),
                Err(error) => Err(error.detail),
            }
        });
        shutdown_at(&registry);
        assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
        resume.send(()).unwrap();
        assert_eq!(worker.join().unwrap().unwrap_err(), "plugin host stopped");
    });
    assert_eq!(std::fs::read_to_string(dir.join("starts")).unwrap(), pid);
}

#[test]
fn plugin_runtime_mixed_previewer_kind_keeps_other_declared_operations_available() {
    let dir = TestDir::new("plugin-mixed-previewer");
    let env = env(&dir);
    plugin(&dir, "config/flea/plugins/example.mixed", "example.mixed", "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nwhile read request; do echo '{\"op\":\"done\"}'; done\n");
    dir.file("config/flea/plugins/example.mixed/manifest.json", r#"{"schemaVersion":1,"id":"example.mixed","name":"Mixed","version":"1","author":"Example","description":"Two kinds.","kinds":["menu-action","previewer"],"entryPoints":{"run":"bin/run"},"menuActions":[{"id":"go","label":"Go"}],"previewer":{"mime":["text/*"],"output":"text"}}"#);
    let mut found = crate::backend::plugins::find(&env, "example.mixed").unwrap();
    assert!(found.plugin.manifest.block("menu-action").is_some());
    assert!(found.plugin.manifest.block("previewer").is_some());
    let registry = Mutex::new(Registry {
        closed: false,
        live: None,
    });
    let runtime =
        session_at(&registry, &env, &found).unwrap_or_else(|error| panic!("{}", error.detail));
    let request = flea::jsondoc::parse(r#"{"op":"run","action":"go","paths":[]}"#).unwrap();
    let answer = runtime.request(request).unwrap();
    assert_eq!(
        answer
            .message
            .get("op")
            .and_then(flea::jsondoc::Json::as_str),
        Some("done")
    );
    found.plugin.manifest.kinds = vec!["previewer".into()];
    assert!(
        session_at(&registry, &env, &found).is_err(),
        "a previewer-only plugin must stay in the sandbox"
    );
    shutdown_at(&registry);
}

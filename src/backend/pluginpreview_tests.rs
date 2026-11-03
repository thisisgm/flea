use super::pluginpreview::{argv, preview_with, Limits, INPUT, OUTPUT};
use super::plugins_tests::{ask, code, env};
use super::testdir::TestDir;
use flea::jsondoc::{self, Json};
use std::path::Path;
use std::time::{Duration, Instant};

const TEXT: &str = "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nread req\nprintf '{\"op\":\"preview\",\"text\":\"first line\\\\nsecond\\\\u0007line\"}\\n'\n";

pub(super) fn previewer(dir: &TestDir, id: &str, output: &str, script: &str) {
    let folder = format!("config/flea/plugins/{id}");
    dir.dir(&format!("{folder}/bin"));
    dir.file(&format!("{folder}/manifest.json"), &format!(
        r#"{{"schemaVersion":1,"id":"{id}","name":"Peek","version":"1.0.0","author":"me","description":"A previewer.",
"kinds":["previewer"],"entryPoints":{{"run":"bin/run"}},"previewer":{{"mime":["text/*"],"output":"{output}"}},"access":["selection"]}}"#));
    dir.script(&format!("{folder}/bin/run"), script);
}

pub(super) fn limits(dir: &TestDir) -> Limits {
    Limits {
        ready: Duration::from_millis(1500),
        request: Duration::from_millis(1500),
        root: dir.dir("preview-root"),
    }
}

pub(super) fn request(path: &Path) -> Json {
    jsondoc::parse(&format!(
        r#"{{"op":"preview","paths":["{}"],"width":320,"height":200}}"#,
        path.display()
    ))
    .unwrap()
}

fn sandboxed(name: &str) -> bool {
    if super::sandboxprobe::skipped() {
        return false;
    }
    let there = super::sandbox::available();
    if !there {
        eprintln!("SKIP {name}: bwrap or prlimit is not on PATH");
    }
    there
}

#[test]
fn plugin_preview_argv_binds_only_the_plugin_the_input_and_its_output() {
    let a = argv(
        Path::new("/p/plug"),
        Path::new("/p/plug/bin/run"),
        Some(8),
        Path::new("/run/w"),
    );
    let joined = a.join(" ");
    assert!(joined.contains("--unshare-all"));
    assert!(joined.contains("--ro-bind /p/plug /p/plug"));
    assert!(joined.contains(&format!("--ro-bind-fd 8 {INPUT}")));
    assert!(joined.contains(&format!("--bind /run/w {OUTPUT}")));
    assert!(
        !joined.contains("--bind /home/u "),
        "the home folder must never be bound"
    );
    assert_eq!(
        a.iter().filter(|x| *x == "--bind").count(),
        1,
        "only the output folder is writable"
    );
    let none = argv(
        Path::new("/p/plug"),
        Path::new("/p/plug/bin/run"),
        None,
        Path::new("/run/w"),
    )
    .join(" ");
    assert!(!none.contains(INPUT), "the ready check binds no input");
}

#[test]
fn plugin_preview_refuses_two_paths_a_folder_and_an_undeclared_type_before_starting() {
    let dir = TestDir::new("plugin-preview-refusals");
    let env = env(&dir);
    previewer(
        &dir,
        "example.peek",
        "text",
        &format!("#!/bin/sh\ntouch '{}/started'\n", dir.path().display()),
    );
    let found = super::plugins::find(&env, "example.peek").unwrap();
    let text = dir.file("a.txt", "hello\n");
    let two = jsondoc::parse(&format!(
        r#"{{"op":"preview","paths":["{0}","{0}"]}}"#,
        text.display()
    ))
    .unwrap();
    assert_eq!(
        preview_with(&env, &found, &two, &limits(&dir))
            .err()
            .unwrap()
            .code,
        "request"
    );
    let folder = dir.dir("folder.txt");
    assert_eq!(
        preview_with(&env, &found, &request(&folder), &limits(&dir))
            .err()
            .unwrap()
            .code,
        "request"
    );
    let png = dir.file("b.png", "x");
    assert_eq!(
        preview_with(&env, &found, &request(&png), &limits(&dir))
            .err()
            .unwrap()
            .code,
        "request"
    );
    assert!(
        !dir.join("started").exists(),
        "a refused preview started the plugin"
    );
}

#[test]
fn plugin_preview_text_is_plain_and_runs_through_the_runtime_route() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if !sandboxed("plugin_preview_text_is_plain_and_runs_through_the_runtime_route") {
        return;
    }
    let dir = TestDir::new("plugin-preview-text");
    let env = env(&dir);
    previewer(&dir, "example.peek", "text", TEXT);
    let (outcome, _) = ask(
        &env,
        r#"{"c":"plugins","op":"enable","id":1,"plugin":"example.peek"}"#,
    );
    assert_eq!(code(outcome), "");
    let file = dir.file("a.txt", "hello\n");
    let line = format!(
        r#"{{"c":"plugins","op":"runtime","id":2,"plugin":"example.peek","request":{}}}"#,
        flea::ui_logic_st2::wire::compact(&request(&file))
    );
    let lines = super::plugins::handle(&line, &env);
    let reply = jsondoc::parse(&lines[0]).unwrap();
    assert_eq!(
        reply.get("ok").and_then(Json::as_bool),
        Some(true),
        "{lines:?}"
    );
    let message = reply.get("message").unwrap();
    assert_eq!(
        message.get("text").and_then(Json::as_str),
        Some("first line\nsecondline")
    );
    assert!(message.get("png").is_none());
}

#[test]
fn plugin_preview_cannot_read_home_or_write_outside_its_output() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if !sandboxed("plugin_preview_cannot_read_home_or_write_outside_its_output") {
        return;
    }
    let dir = TestDir::new("plugin-preview-jail");
    let env = env(&dir);
    let secret = dir.file("secret.txt", "secret\n");
    let script = format!(
        "#!/bin/sh\nread hello\necho '{{\"op\":\"ready\"}}'\nread req\nif cat '{}' >/dev/null 2>&1; then r=leak; else r=sealed; fi\ntouch '{}/escaped' 2>/dev/null\nif [ -r {INPUT} ]; then r=$r-input; fi\nprintf '{{\"op\":\"preview\",\"text\":\"%s\"}}\\n' \"$r\"\n",
        secret.display(),
        dir.path().display()
    );
    previewer(&dir, "example.jail", "text", &script);
    let found = super::plugins::find(&env, "example.jail").unwrap();
    let file = dir.file("a.txt", "hello\n");
    let reply = preview_with(&env, &found, &request(&file), &limits(&dir)).unwrap();
    assert_eq!(
        reply.message.get("text").and_then(Json::as_str),
        Some("sealed-input")
    );
    assert!(!dir.join("escaped").exists());
}

#[test]
fn plugin_preview_hang_malformed_and_wrong_output_fail_within_the_deadline() {
    if !sandboxed("plugin_preview_hang_malformed_and_wrong_output_fail_within_the_deadline") {
        return;
    }
    let dir = TestDir::new("plugin-preview-failures");
    let env = env(&dir);
    let file = dir.file("a.txt", "hello\n");
    for (id, output, script) in [
        ("example.hang", "text", "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nsleep 30\n"),
        ("example.junk", "text", "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nread req\necho '<b>not json</b>'\n"),
        ("example.lies", "png", TEXT),
        ("example.fake", "png", "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nread req\necho notpng > /out/preview.png\necho '{\"op\":\"preview\",\"png\":\"/out/preview.png\"}'\n"),
    ] {
        previewer(&dir, id, output, script);
        let found = super::plugins::find(&env, id).unwrap();
        let started = Instant::now();
        let error = preview_with(&env, &found, &request(&file), &limits(&dir)).err().unwrap_or_else(|| panic!("{id} answered"));
        assert_eq!(error.code, "start", "{id}");
        assert!(started.elapsed() < Duration::from_secs(5), "{id} outlived its deadline");
    }
    let shown = dir.join("preview-root/shown");
    assert_eq!(
        std::fs::read_dir(&shown).map(|d| d.count()).unwrap_or(0),
        0,
        "a refused picture was kept"
    );
}

#[test]
fn plugin_preview_png_header_is_checked_and_the_picture_moves_out_of_the_jail() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if !sandboxed("plugin_preview_png_header_is_checked_and_the_picture_moves_out_of_the_jail") {
        return;
    }
    let dir = TestDir::new("plugin-preview-png");
    let env = env(&dir);
    // A complete 1x1 PNG with CRC-checked IHDR, IDAT and IEND.
    let script = "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nread req\nprintf '\\211\\120\\116\\107\\015\\012\\032\\012\\000\\000\\000\\015\\111\\110\\104\\122\\000\\000\\000\\001\\000\\000\\000\\001\\010\\000\\000\\000\\000\\072\\176\\233\\125\\000\\000\\000\\012\\111\\104\\101\\124\\170\\332\\143\\140\\000\\000\\000\\002\\000\\001\\345\\047\\336\\374\\000\\000\\000\\000\\111\\105\\116\\104\\256\\102\\140\\202' > /out/preview.png\necho '{\"op\":\"preview\",\"png\":\"/out/preview.png\"}'\n";
    previewer(&dir, "example.pic", "png", script);
    let found = super::plugins::find(&env, "example.pic").unwrap();
    let file = dir.file("a.txt", "hello\n");
    let reply = preview_with(&env, &found, &request(&file), &limits(&dir)).unwrap();
    let png = reply.message.get("png").and_then(Json::as_str).unwrap();
    assert!(
        png.starts_with(&dir.join("preview-root/shown").display().to_string()),
        "{png}"
    );
    assert!(Path::new(png).is_file());
    assert_eq!(reply.message.get("width").and_then(Json::as_f64), Some(1.0));
    let work = std::fs::read_dir(dir.join("preview-root"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("work-"))
        .count();
    assert_eq!(work, 0, "the request's work folder was left behind");
}

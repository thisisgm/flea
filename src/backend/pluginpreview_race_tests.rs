use super::*;
use crate::backend::pluginpreview_tests::{limits, previewer, request};
use crate::backend::plugins_tests::env;
use crate::backend::testdir::TestDir;

#[test]
fn plugin_preview_keeps_stdin_open_until_async_reply_arrives() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if !sandbox::available() {
        eprintln!(
            "SKIP plugin_preview_keeps_stdin_open_until_async_reply_arrives: sandbox unavailable"
        );
        return;
    }
    let dir = TestDir::new("plugin-preview-async");
    let env = env(&dir);
    let script = r#"#!/usr/bin/python3
import json, sys, threading, time
json.loads(sys.stdin.readline())
print('{"op":"ready"}', flush=True)
json.loads(sys.stdin.readline())
def render():
    time.sleep(0.05)
    print('{"op":"preview","text":"asynchronous"}', flush=True)
threading.Thread(target=render, daemon=True).start()
for line in sys.stdin:
    pass
"#;
    previewer(&dir, "example.async", "text", script);
    let found = plugins::find(&env, "example.async").unwrap();
    let input = dir.file("chosen.txt", "selected\n");
    let reply = preview_with(&env, &found, &request(&input), &limits(&dir)).unwrap();
    assert_eq!(
        reply.message.get("text").and_then(Json::as_str),
        Some("asynchronous")
    );
}

#[test]
fn plugin_preview_binds_the_open_selected_file_after_path_is_replaced_with_directory_link() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if !sandbox::available() {
        eprintln!("SKIP plugin_preview_binds_the_open_selected_file_after_path_is_replaced_with_directory_link: sandbox unavailable");
        return;
    }
    let dir = TestDir::new("plugin-preview-bind-race");
    let env = env(&dir);
    previewer(&dir, "example.pinned", "text", "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\nread req\nif [ -d /in/file ]; then value=directory; else read value < /in/file; fi\nprintf '{\"op\":\"preview\",\"text\":\"%s\"}\\n' \"$value\"\n");
    let found = plugins::find(&env, "example.pinned").unwrap();
    let path = dir.file("chosen.txt", "selected\n");
    let input = selected(&found.plugin.manifest, &request(&path)).unwrap();
    std::fs::rename(&path, dir.join("original.txt")).unwrap();
    let unselected = dir.dir("unselected");
    std::os::unix::fs::symlink(unselected, &path).unwrap();
    let message = flea::jsondoc::parse(r#"{"op":"preview","path":"/in/file"}"#).unwrap();
    let (answer, _work) = run(&env, &found, Some(&input), Some(message), &limits(&dir)).unwrap();
    assert_eq!(
        answer.unwrap().get("text").and_then(Json::as_str),
        Some("selected")
    );
}

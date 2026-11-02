//! Exercise real tagger commands and process-wide warning suppression with an isolated fake hyprctl.
use super::{Sources, Tagger};
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn check(reply: &str, glassy: bool, warnings: usize) {
    let thread = std::thread::current();
    let name = thread.name().unwrap();
    if std::env::var("FLEA_GLASS_EVAL_CHILD").as_deref() != Ok(name) {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("FLEA_GLASS_EVAL_CHILD", name)
            .output()
            .unwrap();
        let (stdout, stderr) = (
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(output.status.success(), "child failed: {stdout}\n{stderr}");
        assert!(
            stdout.contains("test result: ok. 1 passed;"),
            "child ran no test: {stdout}"
        );
        assert_eq!(
            stderr.matches("window transparency tag skipped:").count(),
            warnings,
            "{stderr}"
        );
        if warnings > 0 && reply != "parse-options" {
            assert!(stderr.contains(reply), "wrong refusal: {stderr}");
        }
        return;
    }

    let root = crate::compositor::tests::Fixture::new();
    let (home, state) = (root.0.join("home"), root.0.join("state"));
    if glassy {
        let toggles = state.join("omarchy/toggles/hypr");
        std::fs::create_dir_all(&toggles).unwrap();
        std::fs::write(
            toggles.join("window-glassy.lua"),
            "o.window(\"org\\\\.gnome\\\\.Nautilus\", { opacity = \"0.75 0.75\" })\n",
        )
        .unwrap();
    }
    std::fs::write(root.0.join("reply"), reply).unwrap();
    let program = root.0.join("hyprctl");
    std::fs::write(
        &program,
        format!(
            r#"#!/bin/sh
d="${{0%/*}}"
case "$1" in
clients) printf '[{{"address":"0x1a","pid":{}}}]' ;;
getprop) echo 1 ;;
eval)
  if [ "$(cat "$d/reply")" = parse-options ]; then
    if [ "$2" != -- ]; then echo 'usage: hyprctl [flags] <command>'; exit 1; fi
    shift
    case "$2" in '-- flea-glassy '*) echo ok ;; *) echo 'unexpected Lua'; exit 1 ;; esac
  else cat "$d/reply"; fi ;;
dispatch)
  if [ "$2" = tagwindow ]; then printf '%s\n' "$*" >> "$d/tags"; echo ok
  else echo 'Invalid dispatcher'; fi ;;
esac
"#,
            std::process::id()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let tagger = Tagger::new(
        std::process::id(),
        program.into_os_string(),
        Sources::under(&home, &state),
    );
    tagger.pass();
    tagger.pass();
    if !glassy {
        assert!(tagger.lock().tagged.contains("0x1a"));
        assert_eq!(
            std::fs::read_to_string(root.0.join("tags")).unwrap(),
            "dispatch tagwindow +transparent address:0x1a\n".repeat(2)
        );
    }
    assert_eq!(
        tagger.lock().ruled,
        (reply == "parse-options").then_some(glassy),
        "a refused eval must never be recorded as applied"
    );
}

#[test]
fn sol_glass_eval_usage_silently_keeps_classic_tag() {
    check(
        "usage: hyprctl [flags] <command> [args...|--help]",
        false,
        0,
    );
}

#[test]
fn sol_glass_eval_unknown_commands_silently_keep_classic_tag() {
    check("unknown request", false, 0);
    check("unknown command", false, 0);
    check("unknown command: eval", false, 0);
}

#[test]
fn sol_glass_eval_other_refusal_warns_once_and_keeps_classic_tag() {
    check("error: Lua execution failed", false, 1);
}

#[test]
fn sol_glass_eval_legacy_config_refusal_still_warns_once() {
    check(
        "eval is only supported with the lua config manager",
        false,
        1,
    );
}

#[test]
fn sol_glass_eval_missing_glassy_opacity_still_warns_once() {
    check("usage: hyprctl [flags] <command>", true, 1);
}

#[test]
fn sol_glass_eval_comment_is_a_payload_not_a_cli_option() {
    check("parse-options", false, 0);
    check("parse-options", true, 0);
}

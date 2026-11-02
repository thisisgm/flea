use flea::jsondoc::{self, Json};
use flea::ui_logic_st2::{manifest, sentence_key, wire, ERROR_CODES};
use std::collections::BTreeSet;

const GUIDE: &str = include_str!("../PLUGINS.md");
const CATALOG: &str = include_str!("../i18n/en/flea.json");

#[test]
fn plugins_doc_repository_examples_obey_runtime_bounds_and_place_mutations() {
    let output = std::process::Command::new("python3")
        .args([
            "-B",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/plugins_examples.py"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn plugins_doc_owner_answers_runtime_and_list_when_user_paths_cannot_resolve() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("timeout")
        .args([
            "--signal=KILL",
            "3s",
            env!("CARGO_BIN_EXE_flea"),
            "--plugin-host",
        ])
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_STATE_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{\"c\":\"plugins\",\"op\":\"runtime\",\"id\":5,\"plugin\":\"example.paths\",\"request\":{\"op\":\"run\"}}\n{\"c\":\"plugins\",\"op\":\"list\",\"id\":6}\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let lines: Vec<_> = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|line| jsondoc::parse(line).unwrap())
        .collect();
    assert_eq!(
        lines.len(),
        2,
        "every accepted request needs a correlated terminal refusal"
    );
    for (id, kind) in [(5.0, "pluginruntime"), (6.0, "pluginop")] {
        let reply = lines
            .iter()
            .find(|line| line.get("id").and_then(Json::as_f64) == Some(id))
            .unwrap();
        assert_eq!(reply.get("t").and_then(Json::as_str), Some(kind));
        assert_eq!(reply.get("code").and_then(Json::as_str), Some("write"));
    }
}

fn manifests(guide: &str) -> Result<Vec<(String, String)>, String> {
    let mut examples = Vec::new();
    let mut opened: Option<(String, String, bool)> = None;
    for line in guide.lines() {
        if let Some((name, body, json)) = &mut opened {
            if line == "```" {
                if !name.is_empty() {
                    examples.push((name.clone(), body.clone()));
                } else if *json {
                    if let Ok(doc) = jsondoc::parse(body) {
                        if doc.get("schemaVersion").is_some() {
                            return Err("manifest example lacks a manifest= filename".into());
                        }
                    }
                }
                opened = None;
            } else {
                body.push_str(line);
                body.push('\n');
            }
        } else if let Some(info) = line.strip_prefix("```") {
            let name = info
                .strip_prefix("json manifest=")
                .unwrap_or("")
                .to_string();
            opened = Some((name, String::new(), info == "json"));
        }
    }
    if opened.is_some() {
        return Err("unclosed documentation fence".into());
    }
    if examples.is_empty() {
        return Err("no manifest examples found".into());
    }
    Ok(examples)
}

fn validate_examples(guide: &str) -> Result<usize, String> {
    let examples = manifests(guide)?;
    let mut names = BTreeSet::new();
    for (name, text) in &examples {
        if !names.insert(name) {
            return Err(format!("duplicate manifest example {name}"));
        }
        manifest::parse(text, false)
            .map_err(|error| format!("{name}: {} ({})", sentence_key(error.code), error.detail))?;
    }
    Ok(examples.len())
}

fn refusal_keys(guide: &str) -> BTreeSet<&str> {
    guide
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.' && ch != '_')
        .filter(|word| word.starts_with("plugins.invalid.") || word.starts_with("plugins.error."))
        .collect()
}

fn check_refusals(guide: &str) -> Result<(), String> {
    let catalog = jsondoc::parse(CATALOG).unwrap();
    let keys = refusal_keys(guide);
    for key in &keys {
        if catalog.get(key).and_then(Json::as_str).is_none() {
            return Err(format!("unknown refusal catalog key {key}"));
        }
    }
    for code in ERROR_CODES.iter().chain(wire::FAILURE_CODES.iter()) {
        let key = sentence_key(code);
        if !keys.contains(key.as_str()) {
            return Err(format!("undocumented refusal {key}"));
        }
    }
    Ok(())
}

#[test]
fn plugins_doc_every_manifest_passes_the_real_validator() {
    assert_eq!(validate_examples(GUIDE).unwrap(), 8);
}

#[test]
fn plugins_doc_every_kind_has_a_guide_manifest_and_a_repository_example() {
    let guide: Vec<_> = manifests(GUIDE)
        .unwrap()
        .into_iter()
        .map(|(_, text)| manifest::parse(&text, false).unwrap())
        .collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/plugins");
    let examples: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().join("manifest.json").is_file())
        .map(|entry| {
            let text = std::fs::read_to_string(entry.path().join("manifest.json")).unwrap();
            let parsed = manifest::parse(&text, false)
                .unwrap_or_else(|e| panic!("{}: {}", entry.path().display(), e.detail));
            assert!(
                entry.path().join(&parsed.run).is_file(),
                "{} has no entry point",
                parsed.id
            );
            parsed
        })
        .collect();
    for (kind, _) in manifest::KINDS {
        assert!(
            guide.iter().any(|m| m.block(kind).is_some()),
            "the guide has no {kind} manifest"
        );
        if kind != "shelf" {
            assert!(
                examples.iter().any(|m| m.block(kind).is_some()),
                "examples/plugins has no {kind} plugin"
            );
        }
    }
}

#[test]
fn plugins_doc_every_refusal_exists_and_every_known_refusal_is_explained() {
    check_refusals(GUIDE).unwrap();
}

#[test]
fn plugins_doc_broken_examples_and_unknown_codes_fail() {
    for (name, text) in manifests(GUIDE).unwrap() {
        let broken = text.replacen("\"schemaVersion\": 1", "\"schemaVersion\": 2", 1);
        assert_ne!(text, broken, "negative control must mutate {name}");
        let guide = format!("```json manifest={name}\n{broken}```\n");
        let error = validate_examples(&guide).unwrap_err();
        assert!(error.contains("plugins.invalid.schema_version"), "{error}");
    }
    assert!(
        check_refusals(&format!("{GUIDE}\nplugins.invalid.invented"))
            .unwrap_err()
            .contains("unknown refusal")
    );
    assert!(validate_examples("No examples").is_err());
    assert!(validate_examples("```json manifest=x\n{}").is_err());
}

#[test]
fn plugins_doc_quick_start_executable_answers_hello_and_settings() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let source = GUIDE
        .split("```python file=hello/bin/run\n")
        .nth(1)
        .unwrap()
        .split("\n```")
        .next()
        .unwrap();
    let mut child = Command::new("timeout")
        .args(["--signal=KILL", "3s", "python3", "-B", "-c", source])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"{\"op\":\"hello\",\"protocolVersion\":1}\n{\"op\":\"settings\",\"settings\":{}}\n",
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: Vec<_> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| jsondoc::parse(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 2);
    flea::plugin_runtime::protocol::ready(&replies[0]).unwrap();
    assert_eq!(replies[1].get("op").and_then(Json::as_str), Some("done"));
}

// Every `flea --action` line in a `sh action` fence parses as the real command line and names an id that resolves.
// Sample input: "if ! flea --action example.checksums:sha256 --window \"$flea_pid\"; then"
fn action_lines(guide: &str) -> Vec<Vec<String>> {
    let mut lines = Vec::new();
    for fence in guide.split("```sh action\n").skip(1) {
        let body = fence.split("\n```").next().unwrap_or("");
        for line in body.lines() {
            let Some((_, command)) = line.split_once("flea --action") else {
                continue;
            };
            let words = format!("--action{command}");
            let words: Vec<String> = words
                .split_whitespace()
                .take_while(|word| !matches!(*word, ";" | "then" | "&&" | "||"))
                .map(|word| {
                    word.trim_end_matches(';')
                        .trim_matches('"')
                        .replace("$flea_pid", "4242")
                })
                .collect();
            lines.push(words);
        }
    }
    lines
}

fn example_plugin_actions() -> BTreeSet<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/plugins");
    let mut ids = BTreeSet::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path().join("manifest.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let parsed = manifest::parse(&text, false).unwrap();
        for c in flea::ui_logic_st2::contributions::menu_actions(&parsed).unwrap() {
            ids.insert(flea::ui_logic_mn2::plugins::leaf(&c));
        }
    }
    ids
}

#[test]
fn plugins_doc_action_examples_parse_and_name_real_ids() {
    use flea::ipc::{action, wire::Request};
    use flea::keys::actions::{parse, Id};
    let lines = action_lines(GUIDE);
    let plugins = example_plugin_actions();
    let (mut builtin, mut plugin, mut list) = (0, 0, 0);
    for words in &lines {
        let options = action::parse(words).unwrap_or_else(|e| panic!("{words:?}: {}", e.stderr));
        let Request::Call {
            function,
            arguments,
            ..
        } = options.request
        else {
            unreachable!()
        };
        if function == "actionIds" {
            list += 1;
            continue;
        }
        match parse(&arguments[0]) {
            Some(Id::Builtin(_)) => builtin += 1,
            Some(Id::Plugin { .. }) if plugins.contains(&arguments[0]) => plugin += 1,
            other => panic!("{} names no real action ({other:?})", arguments[0]),
        }
    }
    assert!(
        builtin > 0 && plugin > 0 && list > 0,
        "the guide shows a built-in, a plugin action and the list"
    );
    for code in [
        action::RAN,
        action::USAGE,
        action::UNKNOWN,
        action::UNAVAILABLE,
        action::NO_WINDOW,
    ] {
        assert!(
            GUIDE.contains(&format!("| `{code}` |")),
            "exit status {code} is documented"
        );
    }
    assert!(
        action_lines("```sh action\nflea --action noSuchAction\n```\n")
            .iter()
            .all(|w| parse(&w[1]).is_none())
    );
}

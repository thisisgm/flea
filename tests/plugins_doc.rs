use flea::jsondoc::{self, Json};
use flea::ui_logic_st2::{manifest, sentence_key, wire, ERROR_CODES};
use std::collections::BTreeSet;

const GUIDE: &str = include_str!("../PLUGINS.md");
const CATALOG: &str = include_str!("../i18n/en/flea.json");

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
    assert_eq!(validate_examples(GUIDE).unwrap(), 7);
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

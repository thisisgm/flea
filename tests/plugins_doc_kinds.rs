//! PLUGINS.md's kind manifests, exchanges and menu marks match the examples and Flea's own plugin runtime.
use flea::jsondoc::{self, Json};
use flea::plugin_runtime::protocol;
use flea::ui_logic_st2::manifest::{self, Manifest};
use flea::ui_logic_st2::wire::compact;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[path = "plugins_doc_kinds/sandbox.rs"]
mod preview_namespace;

const GUIDE: &str = include_str!("../PLUGINS.md");
// A torrent of two files, the bytes the guide's previewer exchange describes.
const TORRENT: &[u8] = b"d4:infod5:filesld6:lengthi412e4:pathl8:lake.pngeed6:lengthi99e4:pathl9:notes.txteee4:name6:photosee";

// Sample input: "```json exchange=checksums\n{\"op\":\"run\"}\n```" gives [("checksums", "{\"op\":\"run\"}\n")].
fn fences(guide: &str, info: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut open: Option<(String, String)> = None;
    for line in guide.lines() {
        if let Some((name, body)) = &mut open {
            if line == "```" {
                found.push((name.clone(), std::mem::take(body)));
                open = None;
            } else {
                body.push_str(line);
                body.push('\n');
            }
        } else if let Some(name) = line.strip_prefix("```").and_then(|l| l.strip_prefix(info)) {
            open = Some((name.to_owned(), String::new()));
        }
    }
    assert!(open.is_none(), "unclosed {info} fence");
    found
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn example(name: &str) -> (PathBuf, Manifest) {
    let folder = root().join("examples/plugins").join(name);
    let text = std::fs::read_to_string(folder.join("manifest.json")).unwrap();
    (folder, manifest::parse(&text, false).unwrap())
}

// Object keys sorted, so a guide manifest laid out for reading still equals its file.
fn canon(value: Json) -> Json {
    match value {
        Json::Obj(mut pairs) => {
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            Json::Obj(pairs.into_iter().map(|(k, v)| (k, canon(v))).collect())
        }
        Json::Arr(items) => Json::Arr(items.into_iter().map(canon).collect()),
        other => other,
    }
}

// A marked temporary root holding the fixtures the guide names, removed only when its marker is still there.
struct Sandbox(PathBuf);

impl Sandbox {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("flea-plugins-doc-{name}-{}", std::process::id()));
        assert!(path.is_absolute() && !path.exists());
        std::fs::create_dir_all(path.join("home/me")).unwrap();
        std::fs::create_dir_all(path.join("in")).unwrap();
        std::fs::write(path.join(".flea-test-sandbox"), "plugins doc\n").unwrap();
        std::fs::write(path.join("home/me/notes.txt"), "test").unwrap();
        std::fs::write(path.join("home/me/photos.torrent"), TORRENT).unwrap();
        std::fs::write(path.join("in/file"), TORRENT).unwrap();
        Sandbox(path)
    }

    // The guide's fixture paths, moved inside this root.
    fn localize(&self, line: &str) -> String {
        let root = self.0.display();
        line.replace("/home/me/", &format!("{root}/home/me/"))
            .replace("\"/in/file\"", &format!("\"{root}/in/file\""))
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        for (key, leaf) in [
            ("HOME", "home"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_RUNTIME_DIR", "runtime"),
        ] {
            std::fs::create_dir_all(self.0.join(leaf)).unwrap();
            command.env(key, self.0.join(leaf));
        }
        command.env_remove("FLEA_EXAMPLE_FOLDERS");
        command
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if self.0.is_absolute() && self.0.join(".flea-test-sandbox").is_file() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

type Turns = Vec<(Json, Vec<Json>)>;

// Sample input: "{\"op\":\"run\",...}\n{\"op\":\"progress\",...}\n{\"op\":\"done\",...}" gives one turn of a request and two answers.
fn exchange(name: &str, body: &str) -> Result<Turns, String> {
    let (_, parsed) = example(name);
    let mut turns: Turns = Vec::new();
    let mut expected: Option<&str> = None;
    for line in body.lines() {
        let value = protocol::parse(line.as_bytes()).map_err(|e| format!("{name}: {e}"))?;
        match expected {
            None => {
                let want = protocol::operation(&parsed, &value);
                expected = Some(want.map_err(|e| format!("{name}: {line}: {e}"))?);
                turns.push((value, Vec::new()));
            }
            Some(want) => {
                if protocol::response(&value, want).map_err(|e| format!("{name}: {line}: {e}"))? {
                    expected = None;
                }
                turns.last_mut().unwrap().1.push(value);
            }
        }
    }
    if expected.is_some() || turns.is_empty() {
        return Err(format!("{name}: the exchange ends without a final answer"));
    }
    Ok(turns)
}

// Runs the example's own executable on the exchange's requests and compares every line it answers.
fn direct(sandbox: &Sandbox, name: &str, body: &str) -> Result<(), String> {
    let turns = exchange(name, &sandbox.localize(body))?;
    let (folder, parsed) = example(name);
    let mut input = String::from("{\"op\":\"hello\",\"protocolVersion\":1}\n");
    for (request, _) in &turns {
        input.push_str(&compact(request));
        input.push('\n');
    }
    let mut child = sandbox
        .command("timeout")
        .args(["--signal=KILL", "10s"])
        .arg(folder.join(&parsed.run))
        .current_dir(&folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    if !output.status.success() {
        return Err(format!("{name}: the example exits {}", output.status));
    }
    let text = String::from_utf8(output.stdout).unwrap();
    let text = text.replace(&sandbox.0.display().to_string(), "");
    let answers = text
        .lines()
        .map(|l| protocol::parse(l.as_bytes()).map(canon))
        .collect::<Result<Vec<_>, _>>()?;
    protocol::ready(answers.first().ok_or(format!("{name}: no ready"))?)?;
    let documented: Vec<Json> = exchange(name, body)?
        .into_iter()
        .flat_map(|(_, a)| a)
        .map(canon)
        .collect();
    if answers[1..] != documented[..] {
        return Err(format!("{name}: the example answers {:?}", &answers[1..]));
    }
    Ok(())
}

fn exchanges() -> Vec<(String, String)> {
    let found = fences(GUIDE, "json exchange=");
    let mut names: Vec<_> = found.iter().map(|(n, _)| n.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "checksums",
            "send-target",
            "syncthing",
            "torrent-preview",
            "virtual-place"
        ]
    );
    found
}

#[test]
fn plugins_doc_kind_manifests_are_the_repository_examples() {
    let mut kinds = Vec::new();
    for (path, body) in fences(GUIDE, "json manifest=examples/plugins/") {
        let name = path.strip_suffix("/manifest.json").unwrap();
        let file = std::fs::read_to_string(root().join("examples/plugins").join(&path)).unwrap();
        assert_eq!(
            canon(jsondoc::parse(&body).unwrap()),
            canon(jsondoc::parse(&file).unwrap()),
            "the guide's {name} manifest differs from its example"
        );
        let (folder, parsed) = example(name);
        assert!(
            folder.join(&parsed.run).is_file(),
            "{name} has no entry point"
        );
        kinds.extend(parsed.kinds);
    }
    // The guide says shelf alone has no example, since it drives an Omarchy bar plugin.
    assert!(GUIDE.contains("Each kind below except shelf has a complete working plugin"));
    for (kind, _) in manifest::KINDS.iter().filter(|(k, _)| *k != "shelf") {
        assert!(
            kinds.iter().any(|k| k == kind),
            "no guide example for {kind}"
        );
    }
}

#[test]
fn plugins_doc_exchanges_are_what_the_examples_answer() {
    let sandbox = Sandbox::new("direct");
    for (name, body) in exchanges() {
        direct(&sandbox, &name, &body).unwrap();
    }
}

#[test]
fn plugins_doc_examples_add_enable_and_answer_through_the_runtime() {
    let flea = env!("CARGO_BIN_EXE_flea");
    let sandbox = Sandbox::new("runtime");
    let preview = preview_namespace::preview_supported();
    let mut ids = Vec::new();
    for (name, _) in exchanges() {
        let (folder, parsed) = example(&name);
        for args in [
            vec!["plugin", "validate", folder.to_str().unwrap()],
            vec!["plugin", "add", folder.to_str().unwrap()],
            vec!["plugin", "enable", &parsed.id],
        ] {
            if args[1] == "enable" && name == "torrent-preview" && !preview {
                continue;
            }
            let out = sandbox.command(flea).args(&args).output().unwrap();
            assert!(
                out.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        ids.push((name, parsed.id));
    }
    let listing = sandbox
        .command(flea)
        .args(["plugin", "list", "--json"])
        .output()
        .unwrap();
    assert!(listing.status.success(), "plugin list --json failed");
    let listing = jsondoc::parse(&String::from_utf8(listing.stdout).unwrap()).unwrap();
    let rows = listing
        .get("plugins")
        .and_then(Json::as_array)
        .expect("plugin listing");
    for (name, id) in &ids {
        let row = rows
            .iter()
            .find(|r| r.get("id").and_then(Json::as_str) == Some(id))
            .unwrap_or_else(|| panic!("{id} is not listed"));
        assert_eq!(
            row.get("enabled").and_then(Json::as_bool),
            Some(name != "torrent-preview" || preview),
            "{id}"
        );
        assert!(
            row.get("failure")
                .and_then(Json::as_str)
                .unwrap_or("")
                .is_empty(),
            "{id} failed: {row:?}"
        );
    }
    let mut host = sandbox
        .command("timeout")
        .args(["--signal=KILL", "90s", flea, "--plugin-host"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = host.stdin.take().unwrap();
    let mut output = BufReader::new(host.stdout.take().unwrap());
    let mut next = 0;
    for ((name, body), (_, id)) in exchanges().into_iter().zip(&ids) {
        if name == "torrent-preview" && !preview {
            continue;
        }
        for (request, answers) in exchange(&name, &sandbox.localize(&body)).unwrap() {
            next += 1;
            // Flea asks a previewer with the selected path and maps it to /in/file inside the sandbox itself.
            let request = if name == "torrent-preview" {
                let path = sandbox.0.join("home/me/photos.torrent");
                jsondoc::parse(&format!(
                    r#"{{"op":"preview","paths":["{}"]}}"#,
                    path.display()
                ))
                .unwrap()
            } else {
                request
            };
            let line = format!(
                r#"{{"c":"plugins","op":"runtime","id":{next},"plugin":"{id}","request":{}}}"#,
                compact(&request)
            );
            writeln!(input, "{line}").unwrap();
            input.flush().unwrap();
            let reply = loop {
                let mut text = String::new();
                assert!(
                    output.read_line(&mut text).unwrap() > 0,
                    "{name}: the host ended"
                );
                let reply = jsondoc::parse(text.trim_end()).unwrap();
                if reply.get("t").and_then(Json::as_str) == Some("pluginruntime") {
                    break reply;
                }
            };
            let last = answers.last().unwrap().clone();
            let ok = last.get("op").and_then(Json::as_str) != Some("error");
            assert_eq!(
                reply.get("ok").and_then(Json::as_bool),
                Some(ok),
                "{name}: {reply:?}"
            );
            assert_eq!(
                reply.get("message").cloned().map(canon),
                Some(canon(last)),
                "{name}: the runtime answer differs from the guide"
            );
        }
    }
    drop(input);
    assert!(host.wait().unwrap().success());
    eprintln!("OPR ordinary runtime assertions completed");
}

// The checks fail on an answer the example does not give, a request the kind does not declare, and a missing answer.
#[test]
fn plugins_doc_kind_checks_refuse_a_wrong_answer_and_an_undeclared_request() {
    let sandbox = Sandbox::new("negative");
    let (_, body) = exchanges()
        .into_iter()
        .find(|(n, _)| n == "checksums")
        .unwrap();
    let wrong = body.replacen("Checksum matches.", "Checksum matched.", 1);
    assert_ne!(wrong, body);
    assert!(direct(&sandbox, "checksums", &wrong)
        .unwrap_err()
        .contains("answers"));
    assert!(exchange("syncthing", "{\"op\":\"targets\"}\n")
        .unwrap_err()
        .contains("declare"));
    assert!(exchange("syncthing", "{\"op\":\"section\"}\n")
        .unwrap_err()
        .contains("final"));
}

// The menu-action marks are `hash`, mn2's one handed mark, and exactly the file menu's own marks.
#[test]
fn plugins_doc_menu_marks_are_the_drawable_marks() {
    use flea::ui_logic_mn1::entry::MARK_NAMES;
    use flea::ui_logic_mn2::rows::{static_glyph, MARKS};
    // Sample input: "`hash`\nor one of the file menu's own marks (`app-window archive ... cloud-upload`)."
    let list = GUIDE
        .split("`hash`\nor one of the file menu's own marks (`")
        .nth(1)
        .unwrap();
    let mut names: Vec<&str> = list[..list.find('`').unwrap()].split_whitespace().collect();
    names.sort_unstable();
    let mut drawable = MARK_NAMES.to_vec();
    drawable.sort_unstable();
    assert_eq!(names, drawable);
    assert!(
        names
            .iter()
            .chain(&["hash"])
            .all(|n| static_glyph(n) == Some(*n)),
        "{names:?}"
    );
    assert_eq!(
        MARKS.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        ["hash"]
    );
}

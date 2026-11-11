use super::*;
use crate::backend::testdir::TestDir;
use crate::jsondoc::{parse, Json};
use std::io::Write;
use std::sync::mpsc::channel;

fn ask(line: &str) -> Ask {
    Ask::parse(line)
}

fn file_ask(path: &Path, extra: &str) -> Ask {
    ask(&format!(r#"{{"c":"code","token":7,"slot":"t","path":"{}"{extra}}}"#, escape(&path.to_string_lossy())))
}

fn doc(line: &str) -> Json {
    parse(line).expect("a reply is one JSON object")
}

fn state(line: &str) -> String {
    doc(line).get("state").and_then(Json::as_str).expect("a state").to_string()
}

fn total(line: &str) -> usize {
    doc(line).get("total").and_then(Json::as_f64).expect("a total") as usize
}

// The first run of every line, as (role, text).
fn firsts(line: &str) -> Vec<(usize, String)> {
    let reply = doc(line);
    let lines = reply.get("lines").and_then(Json::as_array).expect("lines");
    lines
        .iter()
        .filter_map(|l| {
            let runs = l.as_array()?;
            Some((runs.first()?.as_f64()? as usize, runs.get(1)?.as_str()?.to_string()))
        })
        .collect()
}

fn line_total(line: &str) -> usize {
    doc(line).get("lines").and_then(Json::as_array).expect("lines").len()
}

// A reply for a request nothing else shares a slot with.
fn answered(slot: &str, request: &Ask, bat: Option<&str>) -> String {
    let ticket = begin(slot);
    answer(request, bat, &ticket).expect("nothing supersedes this request")
}

// The path of a bat the production jail can run, or a printed reason the test stops: East has no bat, and the suite proves these.
fn jailed_bat() -> Option<String> {
    let who = std::thread::current().name().unwrap_or("a code test").to_string();
    let found = if sandbox::available() { bat_in(&std::env::var("PATH").unwrap_or_default()) } else { None };
    let Some(found) = found else {
        std::io::stderr().write_all(format!("SKIP {who}: no bat, bwrap or prlimit on PATH here\n").as_bytes()).ok();
        return None;
    };
    if crate::backend::sandboxprobe::skipped() {
        return None;
    }
    Some(found)
}

// Short lines, so only the size is in question.
fn text_of(len: usize) -> String {
    (0..len).map(|i| if i % 100 == 99 { '\n' } else { 'a' }).collect()
}

#[test]
fn a_file_is_large_above_one_mebibyte_and_not_at_it() {
    assert_eq!(judge(text_of(1_048_577).as_bytes(), false), Verdict::Large);
    assert_eq!(judge(text_of(1_048_576).as_bytes(), false), Verdict::Styled);
    let d = TestDir::new("codereqlarge");
    let over = answered("codereq-large-a", &file_ask(&d.file("over.txt", &text_of(1_048_577)), ""), None);
    let at = answered("codereq-large-b", &file_ask(&d.file("at.txt", &text_of(1_048_576)), ""), None);
    assert_eq!((state(&over), line_total(&over)), ("large".to_string(), 0));
    assert_eq!(state(&at), "plain", "at the mark the file is read, here with no bat");
}

#[test]
fn a_nul_in_the_first_kibibyte_makes_the_file_binary() {
    let nul_at = |at: usize| {
        let mut data = vec![b'a'; 2048];
        data[at] = 0;
        data
    };
    assert_eq!(judge(&nul_at(1023), false), Verdict::Binary);
    assert_eq!(judge(&nul_at(1024), false), Verdict::Styled);
    let d = TestDir::new("codereqbinary");
    let early = answered("codereq-bin-a", &file_ask(&d.file("early.bin", &format!("{}\0{}", "a".repeat(1023), "b")), ""), None);
    let late = answered("codereq-bin-b", &file_ask(&d.file("late.bin", &format!("{}\0{}", "a".repeat(1024), "b")), ""), None);
    assert_eq!(state(&early), "binary");
    assert_eq!(state(&late), "plain");
}

#[test]
fn the_rules_run_size_then_nul_then_line_length() {
    let mut data = vec![b'a'; LARGE_BYTES + 1];
    data[3] = 0;
    assert_eq!(judge(&data, false), Verdict::Large, "size is read before the NUL");
    let mut long = vec![b'a'; LONG_LINE + 1];
    long[3] = 0;
    assert_eq!(judge(&long, false), Verdict::Binary, "the NUL is read before the line length");
}

#[test]
fn a_line_over_five_thousand_bytes_is_plain_and_five_thousand_is_not() {
    assert_eq!(judge(&[b"ok\n".as_slice(), &vec![b'a'; 5001]].concat(), false), Verdict::Plain);
    assert_eq!(judge(&[b"ok\n".as_slice(), &vec![b'a'; 5000]].concat(), false), Verdict::Styled);
    assert_eq!(judge(&vec![b'a'; 5001], true), Verdict::Styled, "a minified JSON line is what jq breaks up");
    let d = TestDir::new("codereqlong");
    let line = answered("codereq-long", &file_ask(&d.file("long.rs", &format!("fn a() {{}}\n{}\nlast\n", "x".repeat(5001))), ""), Some("/never/run/bat"));
    assert_eq!((state(&line), total(&line)), ("plain".to_string(), 3), "the long line is answered without any jail");
    assert_eq!(firsts(&line)[0], (0, "fn a() {}".to_string()));
}

#[test]
fn with_no_bat_the_answer_is_plain_lines_read_directly() {
    let d = TestDir::new("codereqnobat");
    let body = "one\r\ntwo\n\nfour";
    let line = answered("codereq-nobat", &file_ask(&d.file("a.rs", body), r#","first":2,"count":2"#), None);
    assert_eq!((state(&line), total(&line)), ("plain".to_string(), 4));
    assert!(line.contains(r#""first":2"#));
    assert!(line.ends_with(r#""lines":[[0,"two"],[]]}"#), "{line}");
    let raw = answered("codereq-raw", &file_ask(&d.file("a.csv", "a,b\n1,2\n"), r#","raw":true"#), Some("/never/run/bat"));
    assert_eq!(state(&raw), "plain", "raw skips bat even when bat is there");
}

#[test]
fn a_file_that_cannot_be_read_fails_and_a_text_is_judged_like_a_file() {
    let d = TestDir::new("codereqfail");
    let gone = answered("codereq-gone", &file_ask(&d.join("nope.rs"), ""), None);
    let relative = answered("codereq-rel", &ask(r#"{"c":"code","token":1,"slot":"t","path":"a.rs"}"#), None);
    assert_eq!((state(&gone), state(&relative)), ("failed".to_string(), "failed".to_string()));
    let big = format!(r#"{{"c":"code","token":2,"slot":"t","text":"{}","name":"x.rs"}}"#, "a".repeat(1_048_577));
    assert_eq!(state(&answered("codereq-bigtext", &ask(&big), None)), "large");
    let nul = r#"{"c":"code","token":2,"slot":"t","text":"a\u0000b","name":"x.rs"}"#;
    assert_eq!(state(&answered("codereq-nultext", &ask(nul), None)), "binary");
}

#[test]
fn bat_counts_only_where_the_jail_can_see_it() {
    let d = TestDir::new("codereqpath");
    assert_eq!(bat_in(""), None);
    assert_eq!(bat_in(&d.path().to_string_lossy()), None);
    d.file("bat", "");
    assert_eq!(bat_in(&d.path().to_string_lossy()), None, "a bat outside /usr is not in the jail");
    if Path::new("/usr/bin/bat").is_file() {
        assert_eq!(bat_in("/nonexistent:/usr/bin").as_deref().map(Path::new).map(|p| p.starts_with("/usr")), Some(true));
    }
}

#[test]
fn a_bat_counts_by_where_its_link_leads_and_not_by_where_the_path_names_it() {
    let d = TestDir::new("codereqlink");
    let (system, home) = (d.join("usr"), d.join("home"));
    for dir in ["bin", "local/bin", "../home"] {
        std::fs::create_dir_all(system.join(dir)).unwrap();
    }
    let real = d.file("usr/bin/bat", "");
    let outside = d.file("home/bat", "");
    std::os::unix::fs::symlink(&outside, system.join("local/bin/bat")).unwrap();
    let on = |dirs: &[&str]| dirs.iter().map(|dir| system.join(dir).to_string_lossy().into_owned()).collect::<Vec<_>>().join(":");
    assert_eq!(bat_under(&on(&["local/bin"]), &system), None, "a link out of the prefix is not in the jail");
    assert_eq!(bat_under(&on(&["../home"]), &system), None, "a path that climbs out of the prefix is not in the jail");
    assert_eq!(bat_under(&home.to_string_lossy(), &system), None);
    assert_eq!(bat_under(&on(&["bin"]), &system).as_deref(), Some(real.canonicalize().unwrap().to_str().unwrap()));
    std::fs::remove_file(system.join("local/bin/bat")).unwrap();
    std::os::unix::fs::symlink(&real, system.join("local/bin/bat")).unwrap();
    assert_eq!(bat_under(&on(&["local/bin"]), &system).as_deref(), Some(real.canonicalize().unwrap().to_str().unwrap()), "a link inside the prefix resolves to its target");
}

#[test]
fn a_newer_request_on_a_slot_drops_the_older_reply() {
    let (tx, rx) = channel();
    let older = begin("codereq-slot");
    let other = begin("codereq-other-slot");
    let newer = begin("codereq-slot");
    older.finish("older".to_string(), &tx);
    assert!(rx.try_recv().is_err(), "the superseded reply is never sent");
    other.finish("other".to_string(), &tx);
    newer.finish("newer".to_string(), &tx);
    let sent: Vec<String> = rx.try_iter().map(|m| match m { OpMsg::Meta { line } => line, _ => panic!("a meta line") }).collect();
    assert_eq!(sent, ["other", "newer"], "another slot is its own queue");
}

#[test]
fn a_newer_request_ends_the_older_jail() {
    let older = begin("codereq-kill");
    let jailed = jail::spawn_jailed(&["sleep".to_string(), "30".to_string()], |cmd| {
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    })
    .expect("sleep starts");
    let pid = jailed.child.id();
    assert!(older.attach(jailed));
    let newer = begin("codereq-kill");
    assert!(older.reap().is_none(), "the older job learns it was cancelled");
    let proc = format!("/proc/{pid}");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while Path::new(&proc).exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!Path::new(&proc).exists(), "the older jail is gone");
    assert!(!newer.superseded());
    assert!(older.superseded());
}

#[test]
fn a_job_cancelled_before_its_jail_exists_never_starts_one() {
    let older = begin("codereq-early");
    let _newer = begin("codereq-early");
    let d = TestDir::new("codereqearly");
    let request = file_ask(&d.file("a.rs", "fn main() {}\n"), "");
    assert!(answer(&request, Some("/never/run/bat"), &older).is_none(), "a superseded job answers nothing");
}

#[test]
fn the_json_rule_takes_one_line_json_files_only() {
    for (name, yes) in [("a.json", true), ("A.JSON", true), (".json", true), ("a.jsonl", false), ("json", false), ("a.json.txt", false), ("a.é", false)] {
        assert_eq!(is_json(name), yes, "{name}");
    }
    let d = TestDir::new("codereqjson");
    let formatted = |name: &str, body: &str| {
        let path = d.file(name, body);
        let request = file_ask(&path, "");
        let data = load(&request.source).expect("readable");
        (takes_jq(&request.source, &data), crate::backend::linecount::count_lines(&path).lines)
    };
    assert_eq!(formatted("min.json", "{\"a\":[1,2]}"), (true, 1));
    assert_eq!(formatted("min2.json", "{\"a\":1}\n"), (true, 1), "a closing newline is still one line");
    assert_eq!(formatted("pretty.json", "{\n\"a\": 1\n}\n"), (false, 3));
    assert_eq!(formatted("min.txt", "{\"a\":1}"), (false, 1));
    assert_eq!(formatted("empty.json", ""), (false, 0));
    let argv = json_argv("/usr/bin/bat", "1:400");
    assert_eq!((argv[0].as_str(), argv[4].as_str(), argv[5].as_str()), ("sh", "1:400", "/usr/bin/bat"));
    assert!(argv[2].contains("cat > /tmp/i") && argv[2].contains("jq --indent 2 . /tmp/i") && argv[2].contains("--language=json"), "{}", argv[2]);
}

#[test]
fn the_requests_read_their_fields_and_clamp_the_count() {
    let file = ask(r#"{"c":"code","token":4,"slot":"look","path":"/d/a.rs","first":40,"count":20}"#);
    assert_eq!((file.token, file.slot.as_str(), file.first, file.count, file.raw), (4, "look", 40, 20, false));
    assert!(matches!(&file.source, Source::File(p) if p == "/d/a.rs"));
    let text = ask(r#"{"c":"code","token":5,"slot":"fence:2","text":"a\nb","name":"x.rs","lang":"rust {.n}","raw":true}"#);
    assert_eq!((text.first, text.count, text.raw), (1, 400, true));
    assert!(matches!(&text.source, Source::Text { text, name, lang } if text == "a\nb" && name == "x.rs" && lang == "rust"));
    for (given, want) in [(0, 1), (1, 1), (400, 400), (401, 400), (9999, 400)] {
        assert_eq!(ask(&format!(r#"{{"c":"code","path":"/a","count":{given}}}"#)).count, want, "count {given}");
    }
    assert_eq!(ask(r#"{"c":"code","path":"/a","first":0}"#).first, 1);
    assert_eq!(ask(r#"{"c":"code","path":"/a","count":3,"first":5}"#).range().as_deref(), Some("5:7"));
}

#[test]
fn a_window_that_ends_past_the_largest_line_is_refused_and_not_wrapped() {
    let last = usize::MAX;
    let edge = ask(&format!(r#"{{"c":"code","path":"/a","first":{},"count":2}}"#, last - 1));
    assert_eq!(edge.range().as_deref(), Some("18446744073709551614:18446744073709551615"), "the last window that fits");
    let over = ask(&format!(r#"{{"c":"code","path":"/a","first":{last},"count":2}}"#));
    assert_eq!(over.range(), None);
    assert_eq!(ask(&format!(r#"{{"c":"code","path":"/a","first":{last}}}"#)).range(), None, "an absent count is the capped 400");
    let d = TestDir::new("codereqrange");
    let path = d.file("a.rs", "fn a() {}\n");
    let line = answered("codereq-range", &file_ask(&path, &format!(r#","first":{last},"count":2"#)), None);
    assert_eq!((state(&line), total(&line), line_total(&line)), ("failed".to_string(), 0, 0), "refused with the failed shape, no panic");
    let text = answered("codereq-range-text", &ask(&format!(r#"{{"c":"code","token":1,"slot":"t","text":"a\n","name":"x.rs","first":{last}}}"#)), None);
    assert_eq!(state(&text), "failed");
}

#[test]
fn a_jail_gets_a_private_tmpfs_tmp_and_no_bind_of_the_file() {
    let jail = sandbox::wrap_readonly_extra(&text_argv("/usr/bin/bat", "/d/a.rs", "", "1:9"), &[]);
    let at = jail.iter().position(|a| a == "--tmpfs").expect("a tmpfs");
    assert_eq!(jail[at + 1], "/tmp", "the json script's scratch names are the jail's own, not the host's");
    assert!(jail.iter().any(|a| a == "--unshare-all") && !jail.iter().any(|a| a == "--bind" || a == "/d/a.rs"));
}

#[test]
fn the_argv_names_exactly_the_asked_range() {
    let flags = text_argv("/usr/bin/bat", "/d/a.rs", "", "40:59");
    assert_eq!(&flags[..3], ["env", "HOME=/tmp", "/usr/bin/bat"]);
    assert!(flags.contains(&"--theme=ansi".to_string()) && flags.contains(&"--no-config".to_string()));
    assert!(flags.contains(&"--no-custom-assets".to_string()) && flags.contains(&"--wrap=never".to_string()));
    assert_eq!(&flags[flags.len() - 2..], ["--file-name=/d/a.rs", "--line-range=40:59"], "the file reaches bat on stdin, named only for its language");
    assert!(!flags.iter().any(|a| a == "/d/a.rs"), "no argument opens the path");
    assert!(text_argv("bat", "x.rs", "", "1:9").contains(&"--file-name=x.rs".to_string()));
    let both = text_argv("bat", "x.rs", "toml", "1:9");
    assert!(both.contains(&"--language=toml".to_string()) && !both.iter().any(|a| a.starts_with("--file-name")));
    assert!(!text_argv("bat", "", "", "1:9").iter().any(|a| a.starts_with("--file-name") || a.starts_with("--language")));
}

#[test]
fn a_reply_escapes_quote_backslash_and_tab_and_stays_one_json_line() {
    let request = ask(r#"{"c":"code","token":9,"slot":"fence:\"1\"","path":"/a"}"#);
    let line = reply(&request, "styled", 2, &[vec![(5, "say \"hi\"\\\t<b>".to_string()), (0, " x".to_string())], vec![]]);
    assert_eq!(
        line,
        r#"{"t":"code","token":9,"slot":"fence:\"1\"","state":"styled","first":1,"total":2,"lines":[[5,"say \"hi\"\\\t<b>",0," x"],[]]}"#
    );
    assert!(!line.contains('\n') && !line.contains('\t'));
    let parsed = doc(&line);
    assert_eq!(parsed.get("lines").and_then(Json::as_array).and_then(|l| l[0].as_array()).and_then(|r| r[1].as_str()), Some("say \"hi\"\\\t<b>"));
    assert_eq!(doc(&reply(&request, "binary", 0, &[])).get("lines").and_then(Json::as_array).map(<[Json]>::len), Some(0));
}

#[test]
fn a_rust_file_is_coloured_in_the_jail_by_line_range() {
    let Some(bat) = jailed_bat() else { return };
    let d = TestDir::new("codereqrust");
    let body: String = (1..=132).map(|i| format!("fn f{i}() {{ let s = \"x\"; }}\n")).collect();
    let line = answered("codereq-rust", &file_ask(&d.file("a.rs", &body), r#","first":40,"count":20"#), Some(bat.as_str()));
    assert_eq!(state(&line), "styled", "{line}");
    assert_eq!((total(&line), line_total(&line)), (132, 20));
    assert!(line.contains(r#""first":40"#) && line.contains(r#"[5,"fn",0," ",4,"f40"#), "{line}");
    assert!(line.contains(r#"2,"\"x\""#) || line.contains(r#"2,"\"""#), "a string is green: {line}");
    assert!(!line.contains('\u{1b}') && !line.contains("\\u001b"), "no escape reaches the wire");
}

#[test]
fn markup_in_a_file_comes_back_as_literal_text() {
    let Some(bat) = jailed_bat() else { return };
    let d = TestDir::new("codereqhtml");
    let line = answered("codereq-html", &file_ask(&d.file("a.html", "<img src=\"http://127.0.0.1:9/x\">\n"), ""), Some(bat.as_str()));
    assert_eq!(state(&line), "styled", "{line}");
    assert!(line.contains("http://127.0.0.1:9/x") && line.contains("<"), "{line}");
}

#[test]
fn a_text_is_coloured_by_name_or_language_and_an_unknown_language_is_plain() {
    let Some(bat) = jailed_bat() else { return };
    let by_name = answered("codereq-name", &ask(r#"{"c":"code","token":1,"slot":"t","text":"fn main() {}\n","name":"x.rs"}"#), Some(bat.as_str()));
    assert_eq!(state(&by_name), "styled", "{by_name}");
    assert_eq!(firsts(&by_name)[0], (5, "fn".to_string()));
    let by_lang = answered("codereq-lang", &ask(r#"{"c":"code","token":1,"slot":"t","text":"[package]\nname = \"x\"\n","lang":"toml"}"#), Some(bat.as_str()));
    assert_eq!(firsts(&by_lang)[0], (0, "[".to_string()), "{by_lang}");
    assert!(by_lang.contains(r#"3,"package""#), "the table header is yellow: {by_lang}");
    let unknown = answered("codereq-unknown", &ask(r#"{"c":"code","token":1,"slot":"t","text":"hello\n","lang":"nosuchlang"}"#), Some(bat.as_str()));
    assert_eq!((state(&unknown), total(&unknown)), ("plain".to_string(), 1));
}

#[test]
fn minified_json_is_formatted_in_the_jail_and_refused_json_is_coloured_as_it_is() {
    let Some(bat) = jailed_bat() else { return };
    let d = TestDir::new("codereqjq");
    let min = answered("codereq-jq", &file_ask(&d.file("data.json", "{\"a\":[1,2,{\"b\":null}],\"c\":\"x\"}"), ""), Some(bat.as_str()));
    assert_eq!(state(&min), "styled", "{min}");
    assert_eq!(total(&min), 10, "jq's own lines: {min}");
    assert_eq!(line_total(&min), 10);
    let ranged = answered("codereq-jqr", &file_ask(&d.file("r.json", "{\"a\":[1,2,{\"b\":null}],\"c\":\"x\"}"), r#","first":2,"count":2"#), Some(bat.as_str()));
    assert_eq!((total(&ranged), line_total(&ranged)), (10, 2));
    let bad = answered("codereq-jqbad", &file_ask(&d.file("bad.json", "{\"a\":"), ""), Some(bat.as_str()));
    assert_eq!((state(&bad), total(&bad), line_total(&bad)), ("styled".to_string(), 1, 1), "{bad}");
    let pretty = answered("codereq-jqpretty", &file_ask(&d.file("p.json", "{\n  \"a\": 1\n}\n"), ""), Some(bat.as_str()));
    assert_eq!(total(&pretty), 3, "a formatted file keeps its lines");
}

#[path = "codereq_encoding_tests.rs"]
mod encoding;

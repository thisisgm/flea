//! Only the documented prior invocation is outside the first-window helper budget.
pub(crate) const PRIOR_ARGV: &str = r#""zoxide" "query" "--list" "--all" "--score""#;
const FORK: &str = "fork path/argv: \"\"";

// Sample input: "\"zoxide\" \"zoxide\" \"query\"", a quoted path, one space, then the quoted argv.
fn after_path(payload: &str) -> Option<&str> {
    let tail = payload.strip_prefix('"')?;
    let mut escaped = false;
    for (offset, byte) in tail.bytes().enumerate() {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return tail[offset + 1..].strip_prefix(' ');
        }
    }
    None
}

// Sample input: posix_spawnp path/argv: "zoxide" "zoxide" "query" "--list" "--all" "--score", one record per line.
pub(crate) fn prior_count(records: &str) -> Result<usize, String> {
    let (mut forks, mut spawns, mut execs) = (0, 0, 0);
    for line in records.lines() {
        if line == "no exec or posix_spawn argv observed" {
            continue;
        }
        if line == FORK {
            forks += 1;
            continue;
        }
        let Some((operation, payload)) = line.split_once(" path/argv:") else {
            return Err(format!("unrecognised helper record: {line}"));
        };
        if after_path(payload.trim_start()) != Some(PRIOR_ARGV) {
            return Err(format!("unexpected helper argv: {line}"));
        }
        match operation {
            "posix_spawn" | "posix_spawnp" => spawns += 1,
            "execve" | "execvp" | "execv" => execs += 1,
            _ => return Err(format!("unrecognised helper operation: {line}")),
        }
    }
    match (forks, spawns, execs) {
        (0, 0, 0) => Ok(0),
        (0, 1, 0) | (0..=1, 0, 1) => Ok(1),
        _ => Err(format!(
            "more than one helper: forks={forks} spawns={spawns} execs={execs}"
        )),
    }
}

#[test]
fn exact_prior_argv_is_required_and_a_second_prior_is_rejected() {
    let prior = format!("posix_spawnp path/argv: \"zoxide\" {PRIOR_ARGV}");
    assert_eq!(prior_count(&prior).unwrap(), 1);
    assert!(prior_count(&format!("{prior}\n{prior}\n")).is_err());
    for argv in [
        r#""other" "query" "--list" "--all" "--score""#,
        r#""zoxide" "query" "--list" "--score""#,
        r#""zoxide" "query" "--list" "--all" "--score" "extra""#,
    ] {
        assert!(prior_count(&format!("posix_spawnp path/argv: \"zoxide\" {argv}")).is_err());
    }
    assert!(prior_count(FORK).is_err());
    assert_eq!(
        prior_count(&format!(
            "{FORK}\nexecvp path/argv: \"zoxide\" {PRIOR_ARGV}"
        ))
        .unwrap(),
        1
    );
}

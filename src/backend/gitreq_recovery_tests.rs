use super::*;
use std::path::Path;
use crate::backend::{gitfixture::Fixture, iomount};
use crate::json::{field_bool, field_usize};

fn status(path: &Path) -> String {
    format!(
        "{{\"c\":\"gitstatus\",\"path\":{},\"enabled\":true}}",
        quote(&path.to_string_lossy())
    )
}

#[test]
fn git_failed_discovery_retries_after_stuck_mount_recovers_and_caches_success() {
    let d = Fixture::new();
    let body = format!(
        "1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n",
        d.0.display()
    );
    let _body = iomount::test_hold_body(body);
    iomount::test_reset();
    let stuck = iomount::test_hold_stuck(d.0.clone());
    let mut session = Session::default();
    let request = status(&d.0);
    let first = session.answer(&request);
    assert!(
        !field_bool(&first, "ok") && first.contains("is not responding."),
        "{first}"
    );
    let throttled = session.answer(&request);
    assert_eq!(
        throttled, first,
        "the existing stuck-mount guard still refuses work"
    );
    drop(stuck);
    let recovered = session.answer(&request);
    assert!(field_bool(&recovered, "ok"), "{recovered}");
    let calls = iomount::test_calls();
    assert_eq!(calls, 3, "failed requests retry through the mount guard");
    assert_eq!(session.answer(&request), recovered);
    assert_eq!(
        iomount::test_calls(),
        calls,
        "a successful load stays cached"
    );
}

#[test]
fn git_unmerged_worktree_modes_keep_present_conflicts_out_of_deleted_rows() {
    let d = Fixture::new();
    d.write(".git/HEAD", b"ref: refs/heads/main\n");
    for (xy, modes, expected) in [
        ("UD", "100644 100644 000000 100644", 'M'),
        ("DU", "100644 000000 100644 100644", 'M'),
        ("UU", "100644 100644 100644 100644", 'M'),
        ("AA", "000000 100644 100644 100644", 'M'),
        ("DD", "100644 000000 000000 000000", 'D'),
    ] {
        if expected == 'D' {
            let file = d.path("conflict");
            d.check(&file);
            std::fs::remove_file(file).unwrap();
        } else {
            d.write("conflict", b"retained modified contents\n");
        }
        let bytes = format!("# branch.head main\0u {xy} N... {modes} a b c conflict\0");
        let snapshot = super::super::gitstatus::parse(bytes.as_bytes()).unwrap();
        assert_eq!(
            snapshot.mark(Path::new("conflict")),
            (Some(expected), false),
            "{xy}"
        );
        let mut session = Session {
            path: d.0.to_string_lossy().into_owned(),
            enabled: true,
            repo: gitprobe::discover(&d.0).unwrap(),
            snapshot: Some(snapshot),
            error: None,
        };
        let reply = session.answer(&format!(
            "{{\"c\":\"gitchanges\",\"path\":{}}}",
            quote(&d.0.to_string_lossy())
        ));
        let parsed = crate::jsondoc::parse(&reply).unwrap();
        let entries = parsed.get("entries").unwrap().as_array().unwrap();
        assert_eq!(field_usize(&reply, "total"), Some(1));
        assert_eq!(
            entries[0].get("status").unwrap().as_str(),
            Some(expected.to_string().as_str())
        );
        assert_eq!(
            entries[0].get("deleted").unwrap().as_bool(),
            Some(expected == 'D')
        );
    }
    let deleted = super::super::gitstatus::parse(
        b"# branch.head main\x001 .D N... 100644 100644 000000 a b gone\0",
    )
    .unwrap();
    assert_eq!(deleted.mark(Path::new("gone")), (Some('D'), false));
}

#[test]
fn git_readonly_argv_assertions_run_without_an_external_git_tool() {
    if super::super::sandboxprobe::skipped() {
        return;
    }
    let output = std::process::Command::new("/usr/bin/bwrap")
        .args([
            "--ro-bind", "/", "/", "--bind", "/tmp", "/tmp", "--setenv", "TMPDIR", "/tmp", "--dev", "/dev",
            "--proc", "/proc", "--ro-bind", "/usr/bin/false", "/usr/bin/git", "--",
        ])
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "backend::gitreq_process_tests::status_argv_is_readonly_disables_integrations_and_delimits_paths",
            "--nocapture",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "{stdout} {stderr}"
    );
    assert!(
        !stderr.contains("SKIP"),
        "argv assertions were skipped: {stderr}"
    );
}

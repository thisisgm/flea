// Transfer submission validates sources and queues work on the loop-owned job list.
use super::*;

pub(super) fn start_transfer_checked(out: &mut impl Write, ops: &mut Ops, op: &str, paths: Vec<String>, dest: &str,
                          checks: crate::backend::opsreq::TransferChecks<crate::backend::collide::Ask>) {
    let crate::backend::opsreq::TransferChecks { selection, destination, collision: collide } = checks;
    if ops.live.running().is_some() {
        busy(out, "transfer");
        return;
    }
    // Jobs retain permission failures so Retry can run them after the folder becomes writable.
    let dest = match crate::backend::opsreq::directory_dest(dest) {
        Ok(d) => d,
        Err(e) => {
            writeln!(out, "{}", error_line(&e)).ok();
            out.flush().ok();
            return;
        }
    };
    // A source or target on a pending mount waits; writes elsewhere run beside the held write.
    let mut sources: Vec<PathBuf> = paths.iter().map(flea::bytename::exact).collect();
    if let Some(items) = &selection {
        sources.extend(items.iter().map(|item| item.path.clone()));
    }
    let mut dests = vec![dest.clone()];
    if let Some(held) = &destination {
        dests.push(held.path.clone());
    }
    if pending_busy_for(out, ops, "transfer", &sources, &dests) {
        return;
    }
    // Anything that is not exactly "move" is a copy, so a malformed op can never remove a source.
    let moving = op == "move";
    let n = paths.len();
    ops.transfer_retry = (0, Vec::new());
    let id = ops.claim_id();
    writeln!(out, "{}", transferstarted_line(id, n, moving)).ok();
    out.flush().ok();
    let policy = collide.policy(ops.question.take(), &dest);
    let target = dest.clone();
    ops.jobs.submit(
        out,
        id,
        crate::backend::jobwork::Spec {
            kind: if moving { "move" } else { "copy" },
            label: "",
            target: Some(target),
        },
        crate::backend::jobwork::Work::Transfer {
            moving,
            paths,
            dest,
            selection,
            destination,
            policy,
        },
        &ops.detached,
        &ops.tx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::mpsc::{channel, Receiver};
    use std::time::Duration;

    fn settle(ops: &mut Ops, out: &mut Vec<u8>, rx: &Receiver<OpMsg>) {
        while ops.jobs.unfinished() > 0 {
            let event = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("queued job completes");
            crate::backend::opsdispatch::report_op(out, ops, event);
        }
    }

    extern "C" {
        fn geteuid() -> u32;
    }

    #[test]
    fn queued_permission_failures_remain_retryable() {
        // Root writes into a 0o500 folder, so only a non-root run can produce the permission failure.
        if unsafe { geteuid() } == 0 {
            println!("SKIP queued_permission_failures_remain_retryable: root bypasses mode 0o500");
            return;
        }
        let root = TestDir::new("queued-readonly-destination");
        let source = root.file("source", "payload");
        let dest = root.dir("readonly");
        root.assert_contains(&dest);
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o500)).unwrap();
        assert!(usable_dest(dest.to_str().unwrap()).is_err());
        let (tx, rx) = channel();
        let mut ops = Ops::new(tx);
        let mut out = Vec::new();
        start_transfer_checked(
            &mut out,
            &mut ops,
            "copy",
            vec![source.to_string_lossy().into()],
            dest.to_str().unwrap(),
            crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: crate::backend::collide::Ask::parse("{}") },
        );
        assert!(
            String::from_utf8_lossy(&out).contains("\"t\":\"transferstarted\""),
            "permission failures must be admitted as retryable jobs: {}",
            String::from_utf8_lossy(&out)
        );
        settle(&mut ops, &mut out, &rx);
        assert!(matches!(
            ops.jobs.rows[0].state,
            crate::backend::jobs::State::Failed
        ));
        assert!(!ops.jobs.rows[0].failed.is_empty());
        root.assert_contains(&dest);
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o700)).unwrap();
        let request = format!(
            r#"{{"c":"jobs","op":"retry","id":{}}}"#,
            ops.jobs.rows[0].id
        );
        let _ = crate::backend::jobevents::request(&mut out, &mut ops, &request);
        settle(&mut ops, &mut out, &rx);
        assert!(matches!(
            ops.jobs.rows[0].state,
            crate::backend::jobs::State::Done
        ));
        assert_eq!(
            std::fs::read_to_string(dest.join("source")).unwrap(),
            "payload"
        );
    }

    fn roots(out: &[u8], kind: &str) -> Vec<Vec<String>> {
        let named = format!(r#""kind":"{kind}","paths""#);
        String::from_utf8_lossy(out)
            .lines()
            .filter(|line| line.starts_with(r#"{"t":"roots""#))
            .inspect(|line| assert!(line.contains(r#""job":true"#), "a job names itself: {line}"))
            .inspect(|line| assert!(line.contains(&named), "a job names its kind: {line}"))
            .map(|line| crate::json::field_str_array(line, "paths"))
            .collect()
    }

    #[test]
    fn a_job_and_its_retry_each_name_the_paths_they_read_and_write() {
        if unsafe { geteuid() } == 0 {
            println!("SKIP a_job_and_its_retry_each_name_the_paths_they_read_and_write: root bypasses mode 0o500");
            return;
        }
        // A retry names its job's own kind, so a move's retry says move.
        for kind in ["copy", "move"] {
            let root = TestDir::new(&format!("job-roots-retry-{kind}"));
            let source = root.file("source", "payload");
            let dest = root.dir("readonly");
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o500)).unwrap();
            let (tx, rx) = channel();
            let mut ops = Ops::new(tx);
            let mut out = Vec::new();
            let named = vec![source.to_string_lossy().into_owned()];
            let ask = crate::backend::collide::Ask::parse("{}");
            start_transfer_checked(
                &mut out,
                &mut ops,
                kind,
                named,
                dest.to_str().unwrap(),
                crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: ask },
            );
            let both = vec![
                dest.to_string_lossy().into_owned(),
                source.to_string_lossy().into_owned(),
            ];
            assert_eq!(
                roots(&out, kind),
                std::slice::from_ref(&both),
                "{}",
                String::from_utf8_lossy(&out)
            );
            settle(&mut ops, &mut out, &rx);
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o700)).unwrap();
            out.clear();
            let request = format!(
                r#"{{"c":"jobs","op":"retry","id":{}}}"#,
                ops.jobs.rows[0].id
            );
            let _ = crate::backend::jobevents::request(&mut out, &mut ops, &request);
            assert_eq!(
                roots(&out, kind),
                [both],
                "a retry runs again, so it names its paths again"
            );
            settle(&mut ops, &mut out, &rx);
        }
    }

    #[test]
    fn a_menu_transfer_names_each_selected_source_where_it_lives() {
        use crate::backend::menu_actions::MenuActions;
        let root = TestDir::new("job-roots-menu");
        root.dir("stick");
        root.dir("home");
        let found = [root.file("stick/a.jpg", "a"), root.file("home/b.jpg", "b")];
        let dest = root.dir("dest");
        let (tx, rx) = channel();
        let mut ops = Ops::new(tx.clone());
        let menu = MenuActions::new(tx);
        let rows = found
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        menu.request(r#"{"op":"snapshot","id":5}"#.into(), rows, None);
        let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
            panic!("snapshot reply")
        };
        assert!(line.contains(r#""ok":true"#), "{line}");
        ops.menuactions = Some(menu);
        let mut out = Vec::new();
        let ask = crate::backend::collide::Ask::parse("{}");
        crate::backend::opsdispatch::start_menu_transfer(
            &mut out,
            &mut ops,
            "copy",
            5,
            dest.to_str().unwrap(),
            ask,
        );
        let mut expected: Vec<String> = found
            .iter()
            .chain([&dest])
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        expected.sort();
        assert_eq!(
            roots(&out, "copy"),
            [expected],
            "rows from two folders, as search results are: {}",
            String::from_utf8_lossy(&out)
        );
        settle(&mut ops, &mut out, &rx);
    }

    #[test]
    fn a_send_job_names_its_kind_and_the_request_that_started_it() {
        use crate::backend::jobwork::{Spec, Work};
        let (tx, _rx) = channel();
        let mut ops = Ops::new(tx);
        let mut out = Vec::new();
        let paths = vec!["/nonexistent/flea-send-roots/a.png".to_string()];
        // An empty captured selection fails the send before it runs anything.
        let local = Work::Send {
            peer: "Desk".into(),
            name: String::new(),
            paths: paths.clone(),
            request: 7,
            selection: Some(Vec::new()),
            airdrop: false,
        };
        let spec = Spec {
            kind: "send",
            label: "Sending",
            target: None,
        };
        ops.jobs
            .submit(&mut out, 3, spec, local, &ops.detached, &ops.tx);
        let text = String::from_utf8_lossy(&out);
        let roots = text
            .lines()
            .find(|line| line.starts_with(r#"{"t":"roots""#));
        assert_eq!(
            roots,
            Some(
                r#"{"t":"roots","id":3,"job":true,"kind":"send","requestId":7,"paths":["/nonexistent/flea-send-roots/a.png"]}"#
            ),
            "{text}"
        );
        let rclone = Work::Rclone(crate::backend::rclonerun::Send {
            remote: "gdrive".into(),
            folder: String::new(),
            paths,
            request: 9,
            selection: None,
        });
        assert_eq!(
            rclone.request(),
            Some(9),
            "an rclone send names its request too"
        );
    }

    #[test]
    fn queued_destinations_preserve_validation_and_path_spelling() {
        let root = TestDir::new("queued-destination-validation");
        let folder = root.dir("folder");
        let spelling = format!("{}/../folder/", folder.display());
        let checked = crate::backend::opsreq::directory_dest(&spelling).unwrap();
        assert_eq!(checked.as_os_str(), std::ffi::OsStr::new(&spelling));
        let file = root.file("file", "payload");
        for destination in [
            "relative/path".to_string(),
            root.join("missing").to_string_lossy().into(),
            file.to_string_lossy().into(),
        ] {
            let (tx, _rx) = channel();
            let mut ops = Ops::new(tx);
            let mut out = Vec::new();
            start_transfer_checked(
                &mut out,
                &mut ops,
                "copy",
                Vec::new(),
                &destination,
                crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: crate::backend::collide::Ask::parse("{}") },
            );
            assert!(ops.jobs.rows.is_empty());
            assert_eq!(String::from_utf8_lossy(&out).lines().count(), 1);
            assert!(String::from_utf8_lossy(&out).contains("\"t\":\"error\""));
        }
    }
}

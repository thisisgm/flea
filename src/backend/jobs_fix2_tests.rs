use super::testdir::TestDir;
use super::undo::ItemIdentity;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc::channel, Arc};
use std::time::Duration;

#[test]
fn job_offer_revision_round_trips_independently_of_redo_push_generation() {
    let legacy = r#"{"v":3,"gen":7,"undo":[],"redo":[]}"#;
    let mut doc = super::undocodec::decode(legacy).unwrap();
    assert_eq!((doc.push_gen, doc.job_gen), (7, 0));
    for revision in [0, 1, u64::MAX] {
        doc.job_gen = revision;
        let staged = super::undostage::Staged::new(&doc);
        let text = staged.text();
        assert_eq!(staged.len(), text.len() as u64);
        assert_eq!(
            text,
            crate::jsondoc::render(&super::undocodec::encode(&doc))
        );
        let back = super::undocodec::decode(&text).unwrap();
        assert_eq!((back.push_gen, back.job_gen), (7, revision));
        assert_eq!(text.contains("jobGen"), revision != 0);
    }
    for invalid in ["null", "-1", "1.5", "\"1\"", "[]", "18446744073709551616"] {
        let text = legacy.replace("\"gen\":7", &format!("\"gen\":7,\"jobGen\":{invalid}"));
        assert!(super::undocodec::decode(&text).is_none(), "{invalid}");
    }
}

#[test]
fn initial_archive_failures_keep_menu_selection_identities_for_both_modes() {
    for compressing in [false, true] {
        let root = TestDir::new("jobs-archive-menu-identity");
        let source = root.file("failed.zip", "original selected file");
        let original = ItemIdentity::inspect(&source).unwrap();
        let mut work = super::jobwork::Work::Archive {
            compressing,
            paths: if compressing {
                vec![source.to_str().unwrap().into()]
            } else {
                Vec::new()
            },
            format: "zip".into(),
            archive: source.clone(),
            dest: root.join("output"),
            formats: Arc::new(super::archive::Formats::from_tools(false, false)),
            selection: Some(vec![super::menu_actions::Selected::inspect(
                source.to_str().unwrap(),
            )
            .unwrap()]),
            password: String::new(),
            encrypt_names: false,
        };
        let (tx, rx) = channel();
        let mut ops = super::opsdispatch::Ops::new(tx.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        work.start(1, Arc::clone(&cancel), tx);
        ops.jobs.adopt(
            &mut Vec::new(),
            1,
            if compressing { "compress" } else { "extract" },
            "",
            1,
            cancel,
        );
        ops.jobs.rows[0].work = Some(work);
        let message = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        root.replace_file(&source, "unrelated replacement");
        super::opsdispatch::report_op(&mut Vec::new(), &mut ops, message);
        let job = &mut ops.jobs.rows[0];
        assert!(job.state == super::jobs::State::Failed);
        assert_eq!(job.failed.len(), 1);
        assert!(job.failed[0].1.same_item(&original));
        assert!(job.work.as_mut().unwrap().retry(&job.failed).is_err());
        assert_eq!(
            std::fs::read_to_string(source).unwrap(),
            "unrelated replacement"
        );
        assert!(!root.join("output").exists());
    }
}

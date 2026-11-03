use super::*;

#[test]
fn foreign_and_newer_journals_survive_attach_and_first_push() {
    for original in [
        "{\"foreign\":true}\n",
        "{\"v\":999,\"gen\":1,\"undo\":[],\"redo\":[]}",
        "unparsable",
    ] {
        let d = TestDir::new("undo-preserve");
        let dir = runtime_0700(&d);
        std::fs::write(dir.join(JOURNAL_FILE), original).unwrap();
        let mut journal = shared_journal(&d);
        assert_eq!(
            std::fs::read_to_string(dir.join(JOURNAL_FILE)).unwrap(),
            original
        );
        let (_, _, steps) = rename_steps(&d, "before", "after");
        journal.push(Entry {
            op: "rename".into(),
            steps,
        });
        assert_eq!(
            std::fs::read_to_string(dir.join(JOURNAL_FILE)).unwrap(),
            original
        );
        assert_eq!(journal.undo().unwrap(), "rename");
    }
}

#[test]
fn a_malformed_file_stays_unchanged_and_push_uses_memory() {
    let sandbox = TestDir::new("xundo-malformed");
    let dir = runtime_0700(&sandbox);
    std::fs::write(dir.join(JOURNAL_FILE), "{ this is not json").unwrap();
    let mut journal = shared_journal(&sandbox);
    // The push falls back to its local history and preserves the unsupported document.
    let (_, to, steps) = rename_steps(&sandbox, "a.txt", "b.txt");
    journal.push(Entry {
        op: "rename".to_string(),
        steps,
    });
    let text = std::fs::read_to_string(dir.join(JOURNAL_FILE)).unwrap();
    assert_eq!(text, "{ this is not json");
    assert_eq!(journal.undo().unwrap(), "rename");
    assert!(!to.exists());
}

#[test]
fn foreign_nested_fields_are_preserved_instead_of_defaulted_or_discarded() {
    for original in [
        r#"{"v":3,"gen":{"foreign":true},"undo":[],"redo":[]}"#,
        r#"{"v":3,"undo":[{"op":"x","steps":[],"future":{"keep":true}}],"redo":[]}"#,
        r#"{"v":3,"undo":[],"redo":[{"err":{"where":"redo","path":{},"msg":"keep"}}]}"#,
        r#"{"v":3,"undo":[{"op":"x","steps":[{"k":"barrier","future":true}]}],"redo":[]}"#,
    ] {
        let d = TestDir::new("undo-nested-preserve");
        let dir = runtime_0700(&d);
        std::fs::write(dir.join(JOURNAL_FILE), original).unwrap();
        let mut journal = shared_journal(&d);
        let (_, _, steps) = rename_steps(&d, "before", "after");
        journal.push(Entry {
            op: "rename".into(),
            steps,
        });
        assert_eq!(
            std::fs::read_to_string(dir.join(JOURNAL_FILE)).unwrap(),
            original
        );
        assert_eq!(journal.undo().unwrap(), "rename");
    }
}

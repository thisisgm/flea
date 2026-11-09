//! A transfer held after its job line finishes keeps the head it drew, in the window's language (S7C-5).
use super::model::Operations;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_file_ops::jobs;

// A German catalog holding only the copying head, loaded from a fresh directory that is removed once read.
fn german() -> I18n {
    let root = std::env::temp_dir().join(format!("flea-held-head-{}", std::process::id()));
    std::fs::create_dir(&root).expect("a fresh directory, never one already there");
    let registry = root.join("omarchy/default/i18n");
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::write(
        registry.join("locales.json"),
        r#"{"de":{"name":"Deutsch","direction":"ltr"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "de\n").unwrap();
    std::fs::create_dir_all(root.join("catalogs/de")).unwrap();
    std::fs::write(
        root.join("catalogs/de/flea.json"),
        r#"{"jobs.head.copying": "Kopieren nach %3: %1 von %2"}"#,
    )
    .unwrap();
    let catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalogs")],
    });
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(catalog.language(), "de");
    catalog
}

fn job(fields: &str) -> String {
    format!(r#"{{"t":"job","job":{{"id":7,"kind":"copy",{fields}}},"unfinished":1}}"#)
}

#[test]
fn a_held_transfer_keeps_its_built_head_when_the_finished_line_carries_a_receipt() {
    let catalog = german();
    let mut ops = Operations::new("default");
    ops.line(
        &catalog,
        r#"{"t":"transferstarted","id":7,"n":3,"moving":false}"#,
        0,
    );
    let running = job(
        r#""state":"running","head":"Copying 3 of 3 to gdrive","done":2,"total":3,"target":"gdrive","at":3"#,
    );
    ops.line(&catalog, &running, 0);
    let head = |ops: &Operations| {
        let row = ops
            .jobs
            .rows
            .iter()
            .find(|row| row.id == 7)
            .expect("the transfer's job row");
        (row.state.clone(), jobs::head_line(&catalog, row))
    };
    let drawn = head(&ops);
    assert_eq!(drawn.1, "Kopieren nach gdrive: 3 von 3");
    let finished = job(
        r#""state":"done","head":"Copied 3 items to gdrive","done":3,"total":3,"phase":"checked","receipt":"Copied 3 items to gdrive","target":"gdrive","at":3"#,
    );
    ops.line(&catalog, &finished, 0);
    assert_eq!(
        head(&ops),
        drawn,
        "the held row keeps its running state and German head"
    );
}

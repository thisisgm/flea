// Running job heads come from the row's structured fields through the catalog, in every language (S7C-5).
use crate::ui_logic_file_ops::jobs;
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::{parse, Event, JobRow};

fn job(fields: &str) -> JobRow {
    let line = format!(r#"{{"t":"job","job":{{"id":7,{fields}}},"unfinished":1}}"#);
    match parse(&line) {
        Some(Event::Job { job, .. }) => job,
        other => panic!("not a job line: {other:?}"),
    }
}

fn german() -> Catalog {
    let mut cat = Catalog::with_language("de");
    let path =
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/i18n")).join("de/flea.json");
    cat.write_catalog("de", &std::fs::read_to_string(path).unwrap());
    cat
}

const SEND: &str = r#""kind":"send","state":"running","head":"Sending 1 of 1 to iPhone","done":0,"total":1,"phase":"","receipt":"","target":"iPhone","at":1,"label":"Sending""#;

#[test]
fn a_running_send_head_is_german_under_german() {
    let cat = german();
    let row = job(SEND);
    let head = jobs::head_line(&cat.i18n, &row);
    assert_eq!(head, "Senden an iPhone: 1 von 1");
    assert_eq!(jobs::block(&cat.i18n, &row).head, head);
    assert_eq!(jobs::strip(&cat.i18n, &jobs_of(row)).unwrap().0, head);
}

fn jobs_of(row: JobRow) -> jobs::Jobs {
    let mut jobs = jobs::Jobs::default();
    jobs.update(row);
    jobs
}

#[test]
fn every_running_kind_builds_its_head_from_fields_not_the_english_sentence() {
    let cat = german();
    let cases = [
        (
            r#""kind":"copy","state":"running","done":1,"total":3,"target":"NAS","at":2"#,
            "Kopieren nach NAS: 2 von 3",
        ),
        (
            r#""kind":"move","state":"running","done":1,"total":3,"target":"NAS","at":2"#,
            "Verschieben nach NAS: 2 von 3",
        ),
        (
            r#""kind":"compress","state":"running","done":0,"total":3,"target":"a.zip","at":1"#,
            "3 Elemente werden komprimiert",
        ),
        (
            r#""kind":"extract","state":"running","done":6,"total":18,"target":"x","at":7"#,
            "Entpacken: 7 von 18",
        ),
        (
            r#""kind":"copy","state":"waiting","done":0,"total":1,"target":"NAS","at":0"#,
            "1 Element nach NAS kopieren",
        ),
        (
            r#""kind":"move","state":"waiting","done":0,"total":4,"target":"NAS","at":0"#,
            "4 Elemente nach NAS verschieben",
        ),
        (
            r#""kind":"extract","state":"waiting","done":0,"total":4,"target":"x","at":0,"label":"Extracting""#,
            "Wird entpackt",
        ),
        (
            r#""kind":"copy","state":"running","phase":"checking","done":0,"total":2,"target":"gdrive","at":0"#,
            "2 Elemente auf gdrive werden geprüft",
        ),
        (
            r#""kind":"action","state":"running","done":3,"total":9,"target":"","at":4,"label":"Resize""#,
            "Resize · 4 von 9",
        ),
        (
            r#""kind":"action","state":"waiting","done":0,"total":9,"target":"","at":0,"label":"Resize""#,
            "Resize",
        ),
    ];
    for (fields, want) in cases {
        let row = job(&format!(r#"{fields},"head":"English""#));
        assert_eq!(jobs::head_line(&cat.i18n, &row), want, "{fields}");
    }
}

#[test]
fn a_line_without_the_fields_or_with_a_receipt_keeps_the_backend_head() {
    let cat = german();
    let old = job(
        r#""kind":"send","state":"running","head":"Sending 1 of 1 to iPhone","done":0,"total":1"#,
    );
    assert_eq!(jobs::head_line(&cat.i18n, &old), "Sending 1 of 1 to iPhone");
    let sent = job(
        r#""kind":"send","state":"running","head":"Sent 3 items to gdrive","done":3,"total":3,"phase":"checked","receipt":"Sent 3 items to gdrive","target":"gdrive","at":3"#,
    );
    assert_eq!(jobs::head_line(&cat.i18n, &sent), "Sent 3 items to gdrive");
    let unnamed = job(
        r#""kind":"action","state":"waiting","head":"Action","done":0,"total":9,"target":"","at":0,"label":"""#,
    );
    assert_eq!(jobs::head_line(&cat.i18n, &unnamed), "Action");
    // An encrypted extract reads as done for the password card, and keeps its failure head.
    let locked = job(
        r#""kind":"extract","state":"failed","head":"Extraction failed","done":0,"total":4,"target":"x","at":1,"reason":"encrypted""#,
    );
    assert_eq!(jobs::state(&locked), jobs::State::Done);
    assert_eq!(jobs::head_line(&cat.i18n, &locked), "Extraction failed");
}

#[test]
fn english_heads_read_as_the_backend_wrote_them() {
    let cat = Catalog::english();
    // The backend's head is replaced, so each sentence must come from the rebuild, never the fallback.
    let english = |fields: &str| {
        let mut row = job(fields);
        row.head = "unbuilt".into();
        jobs::head_line(&cat.i18n, &row)
    };
    assert_eq!(english(SEND), "Sending 1 of 1 to iPhone");
    assert_eq!(
        english(r#""kind":"copy","state":"running","done":9,"total":18,"target":"NAS","at":10"#),
        "Copying 10 of 18 to NAS"
    );
    assert_eq!(
        english(
            r#""kind":"send","state":"waiting","done":0,"total":2,"target":"Pixel","at":0,"label":"Sending""#
        ),
        "Sending"
    );
}

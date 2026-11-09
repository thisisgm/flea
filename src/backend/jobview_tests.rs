// The jobs wire carries a head's structured fields, and the front end's English head built from them is the backend's own.
use super::super::jobs::{Jobs, State};
use std::sync::{atomic::AtomicBool, Arc};

fn rebuilt(json: &str) -> (flea::ui_logic_file_ops::JobRow, String) {
    let line = format!(r#"{{"t":"job","job":{json},"unfinished":1}}"#);
    let Some(flea::ui_logic_file_ops::Event::Job { mut job, .. }) =
        flea::ui_logic_file_ops::parse(&line)
    else {
        panic!("not a job line: {line}");
    };
    // The backend's head is replaced, so a match proves the rebuild, never the fallback.
    job.head = "unbuilt".into();
    let head = flea::ui_logic_file_ops::jobs::head_line(
        &crate::i18n::test_catalogs::library_english(),
        &job,
    );
    (job, head)
}

#[test]
fn every_head_the_backend_writes_is_rebuilt_from_its_fields() {
    let mut jobs = Jobs::default();
    let mut out = Vec::new();
    let kinds = [
        ("copy", ""),
        ("move", ""),
        ("compress", "Extracting"),
        ("extract", "Extracting"),
        ("send", "Sending"),
        ("action", "Resize"),
    ];
    let mut checked = 0;
    for (id, (kind, label)) in kinds.into_iter().enumerate() {
        jobs.adopt(
            &mut out,
            id,
            kind,
            label,
            3,
            Arc::new(AtomicBool::new(false)),
        );
        let job = jobs.rows.last_mut().unwrap();
        job.target = "NAS".into();
        for (state, phase, index) in [
            (State::Waiting, "", None),
            (State::Running, "", Some(1)),
            (State::Running, "checking", None),
        ] {
            (job.state, job.phase, job.index, job.done) = (state, phase.into(), index, 1);
            let json = job.json();
            assert!(
                json.contains(r#","target":"NAS","#)
                    && json.contains(&format!(r#""label":"{label}""#)),
                "{json}"
            );
            let (row, head) = rebuilt(&json);
            assert!(row.at.is_some(), "{json}");
            assert_eq!(head, job.head(), "{kind} {phase}");
            checked += 1;
        }
    }
    assert_eq!(checked, 18);
}

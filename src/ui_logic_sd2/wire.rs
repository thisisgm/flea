//! The backend's rclone and job replies, parsed once into events.
use crate::jsondoc::{self, Json};

/// One `job` object as the backend serializes it; only the fields a send reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Job {
    pub id: u64,
    pub kind: String,
    /// waiting, running, failed or done.
    pub state: String,
    /// sending, checking, checked or finished; empty before the producer's first status.
    pub phase: String,
    /// none, checksum or size.
    pub check: String,
    pub file: String,
    pub done: usize,
    pub total: usize,
    pub bytes: u64,
    pub total_bytes: u64,
    pub item_bytes: u64,
    pub rate: u64,
    pub left: Option<u64>,
    /// The backend's own English failure sentence; shown only as a diagnostic argument.
    pub reason: String,
    pub cancelled: bool,
}

/// The terminal `rclonesent` notification, which follows the terminal job update.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub id: u64,
    pub request: u64,
    pub ok: bool,
    pub cancelled: bool,
    pub check: String,
    /// Non-empty only after a checked send or a cancel that already copied something.
    pub receipt: bool,
    pub note: String,
    pub error: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The remotes rclone lists; not visible hides the menu row.
    Remotes {
        id: u64,
        visible: bool,
        remotes: Vec<String>,
    },
    /// A send was accepted as job `job`, or refused with the backend's sentence.
    Started {
        request: u64,
        job: Option<u64>,
        error: String,
    },
    /// One job update or every job in a snapshot, in order.
    Jobs {
        jobs: Vec<Job>,
        snapshot: bool,
    },
    Sent(Outcome),
    /// A jobs request the backend would not take.
    Refused {
        job: u64,
        error: String,
    },
}

fn text(doc: &Json, key: &str) -> Result<String, String> {
    match doc.get(key) {
        None | Some(Json::Null) => Ok(String::new()),
        Some(Json::Str(value)) => Ok(value.clone()),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn number(doc: &Json, key: &str) -> Result<Option<u64>, String> {
    match doc.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Num(value)) => value
            .parse()
            .map(Some)
            .map_err(|_| format!("{key} must be an unsigned integer")),
        Some(_) => Err(format!("{key} must be a number")),
    }
}

fn flag(doc: &Json, key: &str) -> bool {
    doc.get(key).and_then(Json::as_bool).unwrap_or(false)
}

fn job(doc: &Json) -> Result<Job, String> {
    let n = |key| number(doc, key).map(Option::unwrap_or_default);
    Ok(Job {
        id: number(doc, "id")?.ok_or("a job needs an id")?,
        kind: text(doc, "kind")?,
        state: text(doc, "state")?,
        phase: text(doc, "phase")?,
        check: text(doc, "check")?,
        file: text(doc, "file")?,
        done: n("done")? as usize,
        total: n("total")? as usize,
        bytes: n("bytes")?,
        total_bytes: n("totalBytes")?,
        item_bytes: n("itemBytes")?,
        rate: n("rate")?,
        left: number(doc, "left")?,
        reason: text(doc, "reason")?,
        cancelled: flag(doc, "cancelled"),
    })
}

/// Sample input: {"t":"rclonestarted","requestId":72,"id":12,"ok":true,"error":""}; a reply no send reads is None.
pub fn parse(line: &str) -> Result<Option<Event>, String> {
    // The type tag is matched, not a name that happens to start with rclone or job.
    if !line.contains(r#""t":"rclone"#) && !line.contains(r#""t":"job"#) {
        return Ok(None);
    }
    let doc = jsondoc::parse(line)?;
    let id = || number(&doc, "id").map(Option::unwrap_or_default);
    Ok(match doc.get("t").and_then(Json::as_str) {
        Some("rcloneremotes") => {
            let remotes = match doc.get("remotes") {
                Some(Json::Arr(items)) => items
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .map(str::to_string)
                            .ok_or("remotes must be strings")
                    })
                    .collect::<Result<_, _>>()?,
                None => Vec::new(),
                Some(_) => return Err("remotes must be an array".into()),
            };
            Some(Event::Remotes {
                id: id()?,
                visible: flag(&doc, "visible"),
                remotes,
            })
        }
        Some("rclonestarted") => Some(Event::Started {
            request: number(&doc, "requestId")?.unwrap_or_default(),
            job: if flag(&doc, "ok") {
                Some(number(&doc, "id")?.ok_or("an accepted send needs a job id")?)
            } else {
                None
            },
            error: text(&doc, "error")?,
        }),
        Some("job") => Some(Event::Jobs {
            jobs: vec![job(doc.get("job").ok_or("job is missing")?)?],
            snapshot: false,
        }),
        Some("jobs") => match doc.get("jobs") {
            Some(Json::Arr(rows)) => Some(Event::Jobs {
                jobs: rows.iter().map(job).collect::<Result<_, _>>()?,
                snapshot: true,
            }),
            _ => return Err("jobs must be an array".into()),
        },
        Some("rclonesent") => Some(Event::Sent(Outcome {
            id: id()?,
            request: number(&doc, "requestId")?.unwrap_or_default(),
            ok: flag(&doc, "ok"),
            cancelled: flag(&doc, "cancelled"),
            check: text(&doc, "check")?,
            receipt: !text(&doc, "receipt")?.is_empty(),
            note: text(&doc, "note")?,
            error: text(&doc, "error")?,
        })),
        Some("joberror") => Some(Event::Refused {
            job: id()?,
            error: text(&doc, "error")?,
        }),
        _ => None,
    })
}

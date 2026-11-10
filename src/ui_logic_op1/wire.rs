// The backend lines the operations surfaces read, parsed once into typed events; anything else is None.
use crate::jsondoc::{self, Json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collision {
    pub name: String,
    pub dir: bool,
    pub icon: String,
}

// One name a FAT drive cannot hold and the name it gets; the mark follows the item as a collision's does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub old: String,
    pub new: String,
    pub dir: bool,
    pub icon: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransferDone {
    pub id: u64,
    pub ok: u64,
    pub failed: u64,
    pub skipped: u64,
    pub cancelled: bool,
    pub retry_paths: Vec<String>,
    pub durable: bool,
    pub note: String,
    pub merged: u64,
    pub kept_both: u64,
    pub into: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct JobRow {
    pub id: u64,
    pub kind: String,
    pub head: String,
    pub to: String,
    pub file: String,
    pub done: u64,
    pub total: u64,
    pub bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub item_bytes: u64,
    pub rate: u64,
    pub left: Option<u64>,
    pub state: String,
    pub reason: String,
    pub output: Vec<String>,
    pub undo: bool,
    pub cancelled: bool,
}

// A reply to a Trash or menu request the confirmation card sent: delete, check, prepare, validate, close or cancel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub id: u64,
    pub op: String,
    pub ok: bool,
    pub stale: bool,
    pub valid: bool,
    pub error: String,
    // The review token as the backend wrote it, raw.
    pub token: String,
    pub count: u64,
    pub bytes: u64,
    pub all: bool,
    pub deleted: u64,
    pub failed: u64,
    pub cancelled: bool,
    pub action: String,
    pub dest: String,
    pub paths: Vec<String>,
    // The new file's path, from a newFile reply.
    pub path: String,
    // A creation receipt's own word: "made" or "duplicated". Empty on every other reply.
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Collisions {
        id: u64,
        total: u64,
        names: Vec<Collision>,
        mergeable: bool,
    },
    MergeCollisions {
        id: u64,
        total: u64,
        names: Vec<Collision>,
    },
    FatNames {
        id: u64,
        renames: Vec<Rename>,
        fstype: String,
        volume: String,
    },
    TransferStarted {
        id: u64,
        n: u64,
        moving: bool,
        extract: bool,
    },
    TransferProgress {
        id: u64,
        index: u64,
        name: String,
        bytes: u64,
        total: u64,
        scanned: u64,
        phase: String,
        drive: String,
        items: bool,
        checked: u64,
        item_size: u64,
    },
    TransferItem {
        id: u64,
        index: u64,
        name: String,
        ok: bool,
        err: String,
    },
    TransferDone(TransferDone),
    Trashed {
        ok: u64,
        failed: u64,
        err: String,
    },
    TrashStarted {
        cancellable: bool,
    },
    TrashDeleteStarted {
        id: u64,
        request_id: u64,
    },
    Jobs {
        jobs: Vec<JobRow>,
        unfinished: u64,
    },
    Job {
        job: JobRow,
        unfinished: u64,
    },
    JobError {
        id: u64,
        error: String,
        code: String,
    },
    Undone {
        op: String,
        ok: bool,
    },
    Redone {
        op: String,
        ok: bool,
    },
    TrashBrowse(Reply),
    MenuAction(Reply),
}

/// A number field's digits, if the value is a number at all
fn uint(value: Option<&Json>) -> Option<u64> {
    match value {
        Some(Json::Num(text)) => text.parse().ok(),
        _ => None,
    }
}

/// A string field, or empty when it is missing or not a string
fn text(doc: &Json, key: &str) -> String {
    doc.get(key).and_then(Json::as_str).unwrap_or("").to_owned()
}

/// A boolean field, false when it is missing or not a boolean
fn flag(doc: &Json, key: &str) -> bool {
    doc.get(key).and_then(Json::as_bool).unwrap_or(false)
}

/// A number field the line must carry, so the whole event is refused without it
fn need(doc: &Json, key: &str) -> Option<u64> {
    uint(doc.get(key))
}

/// A string array, dropping anything that is not a string
fn strings(doc: &Json, key: &str) -> Vec<String> {
    doc.get(key)
        .and_then(Json::as_array)
        .map_or_else(Vec::new, |items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
}

/// The collisions a names array holds, refused when an entry loses its name
fn names(doc: &Json) -> Option<Vec<Collision>> {
    doc.get("names")?
        .as_array()?
        .iter()
        .map(|name| {
            Some(Collision {
                name: name.get("n")?.as_str()?.to_owned(),
                dir: name.get("d").and_then(Json::as_bool).unwrap_or(false),
                icon: text(name, "i"),
            })
        })
        .collect()
}

/// One job row; its id is the only field the line must carry
fn job(doc: &Json) -> Option<JobRow> {
    Some(JobRow {
        id: need(doc, "id")?,
        kind: text(doc, "kind"),
        head: text(doc, "head"),
        to: text(doc, "to"),
        file: text(doc, "file"),
        done: need(doc, "done").unwrap_or(0),
        total: need(doc, "total").unwrap_or(0),
        bytes: need(doc, "bytes"),
        total_bytes: need(doc, "totalBytes"),
        item_bytes: need(doc, "itemBytes").unwrap_or(0),
        rate: need(doc, "rate").unwrap_or(0),
        left: need(doc, "left"),
        state: text(doc, "state"),
        reason: text(doc, "reason"),
        output: strings(doc, "output"),
        undo: flag(doc, "undo"),
        cancelled: flag(doc, "cancelled"),
    })
}

// Sample input: {"t":"trashbrowse","id":3,"op":"prepare","ok":true,"token":9,"all":false,"count":2,"bytes":10,"partial":false}
/// A trashbrowse or menuaction reply; its id is the only field the line must carry
fn reply(doc: &Json) -> Option<Reply> {
    Some(Reply {
        id: need(doc, "id")?,
        op: text(doc, "op"),
        ok: flag(doc, "ok"),
        stale: flag(doc, "stale"),
        valid: flag(doc, "valid"),
        error: text(doc, "error"),
        token: match doc.get("token") {
            Some(Json::Num(raw)) => raw.clone(),
            _ => String::new(),
        },
        count: need(doc, "count").unwrap_or(0),
        bytes: need(doc, "bytes").unwrap_or(0),
        all: flag(doc, "all"),
        deleted: need(doc, "deleted").unwrap_or(0),
        failed: need(doc, "failed").unwrap_or(0),
        cancelled: flag(doc, "cancelled"),
        action: text(doc, "action"),
        dest: text(doc, "dest"),
        paths: strings(doc, "paths"),
        path: text(doc, "path"),
        kind: text(doc, "kind"),
    })
}

// Sample input: {"t":"collisions","id":7,"total":1,"names":[{"n":"a.png","d":false,"i":"image-x-generic"}],"mergeable":false}
/// The event a backend line carries, or none for a line this surface does not read
pub fn parse(line: &str) -> Option<Event> {
    let doc = jsondoc::parse(line).ok()?;
    let kind = doc.get("t")?.as_str()?;
    Some(match kind {
        "collisions" => Event::Collisions {
            id: need(&doc, "id")?,
            total: need(&doc, "total")?,
            names: names(&doc)?,
            mergeable: flag(&doc, "mergeable"),
        },
        "mergecollisions" => Event::MergeCollisions {
            id: need(&doc, "id")?,
            total: need(&doc, "total")?,
            names: names(&doc)?,
        },
        "fatnames" => Event::FatNames {
            id: need(&doc, "id")?,
            renames: doc
                .get("renames")?
                .as_array()?
                .iter()
                .map(|r| {
                    Some(Rename {
                        old: r.get("old")?.as_str()?.to_owned(),
                        new: r.get("new")?.as_str()?.to_owned(),
                        dir: r.get("d").and_then(Json::as_bool).unwrap_or(false),
                        icon: text(r, "i"),
                    })
                })
                .collect::<Option<_>>()?,
            fstype: text(&doc, "fstype"),
            volume: text(&doc, "volume"),
        },
        "transferstarted" => Event::TransferStarted {
            id: need(&doc, "id")?,
            n: need(&doc, "n")?,
            moving: flag(&doc, "moving"),
            extract: false,
        },
        "extractstarted" => Event::TransferStarted {
            id: need(&doc, "id")?,
            n: 1,
            moving: false,
            extract: true,
        },
        "transferprogress" => Event::TransferProgress {
            id: need(&doc, "id")?,
            index: need(&doc, "index").unwrap_or(0),
            name: text(&doc, "name"),
            bytes: need(&doc, "bytes").unwrap_or(0),
            total: need(&doc, "total").unwrap_or(0),
            scanned: need(&doc, "scanned").unwrap_or(0),
            phase: text(&doc, "phase"),
            drive: text(&doc, "drive"),
            items: text(&doc, "unit") == "items",
            checked: need(&doc, "checked").unwrap_or(0),
            item_size: need(&doc, "itemSize").unwrap_or(0),
        },
        "transferitem" => Event::TransferItem {
            id: need(&doc, "id")?,
            index: need(&doc, "index")?,
            name: text(&doc, "name"),
            ok: flag(&doc, "ok"),
            err: text(&doc, "err"),
        },
        "transferdone" => Event::TransferDone(TransferDone {
            id: need(&doc, "id")?,
            ok: need(&doc, "ok").unwrap_or(0),
            failed: need(&doc, "failed").unwrap_or(0),
            skipped: need(&doc, "skipped").unwrap_or(0),
            cancelled: flag(&doc, "cancelled"),
            retry_paths: strings(&doc, "retryPaths"),
            durable: flag(&doc, "durable"),
            note: text(&doc, "note"),
            merged: need(&doc, "merged").unwrap_or(0),
            kept_both: need(&doc, "keptBoth").unwrap_or(0),
            into: text(&doc, "into"),
        }),
        "trashed" => Event::Trashed {
            ok: need(&doc, "ok").unwrap_or(0),
            failed: need(&doc, "failed").unwrap_or(0),
            err: text(&doc, "err"),
        },
        "trashstarted" => Event::TrashStarted {
            cancellable: doc.get("cancellable").and_then(Json::as_bool) != Some(false),
        },
        "trashdeletestarted" => Event::TrashDeleteStarted {
            id: need(&doc, "id")?,
            request_id: need(&doc, "requestId").unwrap_or(0),
        },
        "jobs" => Event::Jobs {
            jobs: doc
                .get("jobs")?
                .as_array()?
                .iter()
                .map(job)
                .collect::<Option<_>>()?,
            unfinished: need(&doc, "unfinished").unwrap_or(0),
        },
        "job" => Event::Job {
            job: job(doc.get("job")?)?,
            unfinished: need(&doc, "unfinished").unwrap_or(0),
        },
        "joberror" => Event::JobError {
            id: need(&doc, "id").unwrap_or(0),
            error: text(&doc, "error"),
            code: text(&doc, "code"),
        },
        "undone" => Event::Undone {
            op: text(&doc, "op"),
            ok: flag(&doc, "ok"),
        },
        "redone" => Event::Redone {
            op: text(&doc, "op"),
            ok: flag(&doc, "ok"),
        },
        "trashbrowse" => Event::TrashBrowse(reply(&doc)?),
        "menuaction" => Event::MenuAction(reply(&doc)?),
        _ => return None,
    })
}

use crate::backend::undo::{Entry, ItemIdentity, Step};
use crate::error::FleaError;
use std::path::PathBuf;

// What an operation thread sends back, joined onto the same receiver every other event already arrives on.
pub enum OpMsg {
    SendStatus {
        id: usize,
        phase: String,
        check: String,
        receipt: String,
        rate: u64,
        left: Option<u64>,
    },
    // Entry and unpacked-byte counts for extraction and adopted operation workers.
    JobEntries {
        id: usize,
        done: usize,
        total: usize,
        file: String,
        bytes: u64,
        total_bytes: u64,
        item_bytes: u64,
    },
    JobReady {
        id: usize,
        result: Result<(String, String), FleaError>,
    },
    JobSample {
        id: usize,
        done: usize,
        total: usize,
        file: String,
        output: Vec<String>,
    },
    JobDone {
        id: usize,
        failed: Vec<(PathBuf, ItemIdentity)>,
        error: String,
        line: String,
    },
    ActionReady {
        request: usize,
        result: Result<(crate::backend::actions::Action, Vec<String>), String>,
    },
    // scanned is the batch's own total, 0 until the sweep beside the copy settles on one.
    Progress { id: usize, index: usize, name: String, bytes: u64, total: u64, scanned: u64 },
    Item { id: usize, index: usize, name: String, ok: bool, err: String },
    // refused names each entry a drive could not hold inside a copied tree, by its path under dest, at most unheld::NAMED.
    TransferDone { id: usize, ok: usize, failed: usize, skipped: usize, cancelled: bool, entry: Entry,
                   retry: Vec<(PathBuf, ItemIdentity)>, durable: bool, note: String, refused: Vec<(String, String)> },
    MergeDone { id: usize, ok: usize, failed: usize, skipped: usize, cancelled: bool, entry: Entry,
                retry: Vec<(PathBuf, ItemIdentity)>, durable: bool, note: String, merged: usize, kept_both: usize, into: String,
                refused: Vec<(String, String)> },
    // The outcome line alone, sent the moment the disk shows it: the slot frees here and only the batch's own journal entry stays pending.
    TrashAnnounced { id: usize, request: usize, path: String, ok: usize, failed: usize, reason: String, moved: Vec<String> },
    // announced is true once TrashAnnounced already wrote this batch's line, so the wire carries it once.
    Trashed { id: usize, request: usize, ok: usize, failed: usize, entry: Entry, reason: String, announced: bool },
    Linked { ok: usize, failed: usize, skipped: usize, entry: Entry, note: String, first_err: String, dest: String },
    Duplicated { ok: bool, path: String, err: String, entry: Entry },
    // The legacy operation still owns its journal and slot; the optional GUI receipt is published with it.
    CreationReceipt { request: usize, inner: Box<OpMsg> },
    RedoDone { journal: Box<crate::backend::undo::Journal>, result: Result<String, FleaError>, touched: crate::backend::undo::Reached },
    MenuDeleteDone { line: String },
    // Legacy archive worker receipt used by archivequit_tests and archivepass_tests.
    #[cfg(test)]
    SlotDone { line: String },
    DetachedDone { id: usize, line: String },
    // A convert's terminal line, with a step for each output it published, journalled as one convert.
    Converted { id: usize, line: String, steps: Vec<Step> },
    // A collisions answer, kept as the latest question before its line goes out; it claims no slot.
    Asked { turn: usize, question: crate::backend::collide::Question, line: String },
    // A pasted clipboard image, journaled beside its line; it claims no slot either.
    Pasted { line: String, entry: Option<Entry> },
    // Rename execution returned; the dispatcher hands its entry to the journal publisher.
    RenameDone { id: usize, result: Result<(PathBuf, Vec<Step>), FleaError> },
    RenameSlow { id: usize, line: String },
    JournalDone { journal: Box<crate::backend::undo::Journal>, renames: Vec<(usize, crate::backend::renamework::Outcome)> },
    MkdirDone { id: usize, result: Result<(PathBuf, Vec<Step>), FleaError> },
    // A slow bulk rename reporting late: the landed steps with the batch counts.
    BulkRenameDone { id: usize, result: Result<crate::backend::bulkrename::BulkOutcome, FleaError> },
    SetTagsDone { id: usize, result: crate::backend::tagsreq::Written },
    // Meta rides this channel because a media probe is a subprocess; it claims no operation slot.
    Meta { line: String },
}


use super::jobs::{Job, Jobs, State};
use super::jobwork::Work;
use super::opscancel::DetachedJobs;
use super::opsreq::OpMsg;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

impl Jobs {
    pub fn submit(
        &mut self,
        out: &mut impl Write,
        id: usize,
        spec: super::jobwork::Spec<'_>,
        work: Work,
        flags: &DetachedJobs,
        tx: &Sender<OpMsg>,
    ) {
        let super::jobwork::Spec {
            kind,
            label,
            target,
        } = spec;
        self.invalidate_undo(out);
        self.rows.retain(|job| job.state != State::Done);
        let cancel = Arc::new(AtomicBool::new(false));
        flags.insert(id, &cancel);
        let total = if kind == "extract" {
            0
        } else {
            work.paths().len()
        };
        let file = work.file();
        let roots = super::jobview::roots_line(id, true, kind, work.request(), &work.roots());
        writeln!(out, "{roots}").ok();
        let target_name = match &work {
            Work::Rclone(send) => send.remote.clone(),
            Work::Send { peer, name, .. } if name.is_empty() => peer.clone(),
            Work::Send { name, .. } => name.clone(),
            _ => target
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|name| name.to_string_lossy().into())
                .unwrap_or_default(),
        };
        self.next_order += 1;
        self.rows.push(Job {
            id,
            order: self.next_order,
            kind: kind.into(),
            label: label.into(),
            phase: String::new(),
            check: String::new(),
            receipt: String::new(),
            target: target_name,
            state: State::Waiting,
            file,
            done: 0,
            total,
            bytes: 0,
            total_bytes: 0,
            item_bytes: 0,
            rate: 0,
            left: None,
            reason: "checking target".into(),
            output: Vec::new(),
            undo: false,
            undo_key: None,
            cancelled: false,
            failed: Vec::new(),
            work: Some(work),
            cancel,
            drive: None,
            prepared: target.is_none(),
            started: Instant::now(),
            reported: Instant::now(),
            moved: 0,
            index: None,
        });
        self.snapshot(out);
        if let Some(target) = target {
            check_target(id, target, tx);
        }
        self.advance(out, flags, tx);
    }
}

// The target's drive is read off the loop, so a stalled mount delays only its own job.
pub(crate) fn check_target(id: usize, target: PathBuf, tx: &Sender<OpMsg>) {
    let tx = tx.clone();
    let body = super::iomount::mount_body();
    std::thread::spawn(move || {
        let result = super::jobdrive::drive_of(&target, &body);
        let _ = tx.send(OpMsg::JobReady { id, result });
    });
}

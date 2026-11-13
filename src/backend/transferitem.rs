// One top-level item of a transfer, copied or moved, with its journal step.
use super::{OpMsg, Pace};
use crate::backend::collide::{identity_in, Table};
use crate::backend::copyfile::{copy_any, move_any, Progress};
use crate::backend::undo::{self, Step};
use crate::error::FleaError;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::mpsc::Sender;

// A directory reports its tree's bytes so far and no total, and its partial journals in `steps` either way.
pub(super) fn one_item(
    item: super::TransferItem<'_>,
    moving: bool,
    cancel: &AtomicBool,
    tx: &Sender<OpMsg>,
    (settled, pace, table): (&AtomicU64, &Pace, &Table),
    steps: &mut Vec<Step>,
    durability: &mut crate::backend::durable::Durability,
) -> Result<(), FleaError> {
    let super::TransferItem { id, index, name, src, dst, source } = item;
    // scanned is still 0 while the sweep counts, and the batch's own total from the moment it settles.
    let mut sink = |done: u64, total: u64| pace.report(tx, id, (index, name), (done, total), settled);
    let mut p = Progress { cancel, on_bytes: &mut sink, partial: None, tree: None, manifest: if moving { crate::backend::copymanifest::writer_for_move(src, dst) } else { crate::backend::copymanifest::writer_for(src, dst) }, durability: Some(durability), for_move: moving };
    let mut outcome = if moving { move_any(src, dst, &mut p) } else { copy_any(src, dst, &mut p) };
    pace.ended(index, name);
    match &outcome {
        Ok(()) if moving => steps.push(undo::moved(src, dst, source)?),
        Ok(()) => steps.push(undo::copied_as(src, dst, source, identity_in(dst, table)?)),
        // The partial is this operation's, so it is journaled and undo removes it like any created path.
        Err(_) => {
            if let Some(path) = p.partial.take() {
                let (manifest, loud) = crate::backend::copymanifest::finish_loud(p.manifest.take());
                if let (Err(outcome), Some(e)) = (&mut outcome, loud) {
                    outcome.msg = crate::backend::ops::manifest_failed(&outcome.msg, &e);
                }
                steps.push(undo::copied_partial(src, &path, source, manifest)?);
            }
        }
    }
    outcome
}

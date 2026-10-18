// Transfer submission validates sources and queues work on the loop-owned job list.
use super::*;

pub(super) fn start_transfer_checked(out: &mut impl Write, ops: &mut Ops, op: &str, paths: Vec<String>, dest: &str,
                          selection: Option<Vec<crate::backend::menu_actions::Selected>>, destination: Option<crate::backend::menu_actions::Selected>, collide: crate::backend::collide::Ask) {
    if ops.live.running().is_some() {
        busy(out, "transfer");
        return;
    }
    let dest = match usable_dest(dest) {
        Ok(d) => d,
        Err(e) if e.msg == "that folder cannot be written" => PathBuf::from(dest),
        Err(e) => {
            writeln!(out, "{}", error_line(&e)).ok();
            out.flush().ok();
            return;
        }
    };
    // A source or target on a pending mount waits; writes elsewhere run beside the held write.
    let mut sources: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    if let Some(items) = &selection {
        sources.extend(items.iter().map(|item| item.path.clone()));
    }
    let mut dests = vec![dest.clone()];
    if let Some(held) = &destination {
        dests.push(held.path.clone());
    }
    if pending_busy_for(out, ops, "transfer", &sources, &dests) {
        return;
    }
    // Anything that is not exactly "move" is a copy, so a malformed op can never remove a source.
    let moving = op == "move";
    let n = paths.len();
    ops.transfer_retry = (0, Vec::new());
    let id = ops.claim_id();
    writeln!(out, "{}", transferstarted_line(id, n, moving)).ok();
    out.flush().ok();
    let policy = collide.policy(ops.question.take(), &dest);
    let target = dest.clone();
    ops.jobs.submit(
        out,
        id,
        crate::backend::jobwork::Spec {
            kind: if moving { "move" } else { "copy" },
            label: "",
            target: Some(target),
        },
        crate::backend::jobwork::Work::Transfer {
            moving,
            paths,
            dest,
            selection,
            destination,
            policy,
        },
        &ops.detached,
        &ops.tx,
    );
}


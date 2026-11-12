// Adopts a completed device scan through the current listing and search owners.
use super::*;

// A landed scan becomes the listing and a failed one leaves it alone; true when it became the listing.
pub fn finish<W: Write>(
    out: &mut W,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    watch: &mut Watch,
    poller: &Poller,
    landed: Landed,
) -> bool {
    let out =
        &mut requestgen::Writer::new(out, crate::json::field_usize(&landed.job.line, "pickerGen"));
    let Landed { job, done } = landed;
    let Job {
        path,
        first: _,
        line: _,
        hidden: _,
        want_changed,
        session: _,
    } = job;
    if crate::backend::unmount::crossed_answers(
        out,
        watch,
        &flea::bytename::view(&path),
        &done.result,
    ) {
        out.flush().ok();
        return false;
    }
    let landed = match done.result {
        Ok(listed) => {
            watch.set_incoming(listed.watch_wd, listed.watch_mount.clone());
            watch.commit(listed.listing.outline_watches.clone());
            // Said once per listing, because a folder nobody can watch goes stale in silence.
            if watch.refused() {
                eprintln!(
                    "flea: {} will not follow outside changes, inotify refused a watch on it",
                    path
                );
            }
            adopt_listed(out, st, pool, tb, &path, listed, want_changed);
            watch.ancestry.follow(&st.base);
            out.flush().ok();
            true
        }
        Err(failed) => {
            // The listing did not move, so the loop removes the watch the failed scan armed.
            watch.abandon_wd(failed.watch_wd);
            if !crate::backend::unmount::gone_answers(
                out,
                st,
                watch,
                poller,
                &flea::bytename::view(&path),
                &failed,
            ) {
                say_failed(out, &failed);
            }
            false
        }
    };
    out.flush().ok();
    crate::prefetch::first_rows_sent();
    landed
}

// An empty query's folder answer, written only while its session still stands at the gen it claimed.
pub fn finish_session<W: Write>(
    out: &mut W,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    watch: &mut Watch,
    landed: Landed,
    scan: ForSession,
) {
    let Landed { job, done } = landed;
    let current = st.search_session.as_ref().is_some_and(|session| {
        session.last_answered_gen == scan.gen && session.folder == *flea::bytename::view(&job.path)
    });
    match done.result {
        Ok(listed) => {
            // The folder's own listing answers, so the armed watch the scan added goes back.
            watch.abandon_wd(listed.watch_wd);
            if !current {
                return;
            }
            let n = listed.listing.len();
            if let Some(session) = st.search_session.as_mut() {
                session.note_folder_count(n);
            }
            let ms = listed.read_ms + listed.sort_ms;
            adopt_session(out, st, pool, tb, &job.path, listed, scan.gen);
            st.held = Held::Walk;
            let reason = st
                .search_session
                .as_ref()
                .and_then(|session| session.partial_reason());
            writeln!(
                out,
                "{}",
                crate::backend::searchreq::searched_status_line(
                    n,
                    n,
                    ms,
                    scan.gen,
                    scan.walking,
                    reason
                )
            )
            .ok();
        }
        Err(failed) => {
            watch.abandon_wd(failed.watch_wd);
            if current {
                say_failed(out, &failed);
            }
        }
    }
    out.flush().ok();
}

// The denial's mode rides the answer from the worker, because a stat here would hang the loop on a stalled mount.
fn say_failed(out: &mut impl Write, failed: &ListErr) {
    if failed.mode == 0 {
        writeln!(out, "{}", error_line(&failed.error)).ok();
    } else {
        writeln!(out, "{}", error_line_with_mode(&failed.error, failed.mode)).ok();
    }
}

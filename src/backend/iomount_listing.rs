// Listing I/O shared by the bounded scanner and deadline-bound direct callers.
use super::{call_with_flag, CALL_DEADLINE, STUCK_TTL};
use crate::error::FleaError;

// One listing's syscalls, computed on a worker with the listing, never on the loop.
pub struct ListOut {
    pub listing: crate::backend::listing::Listing,
    pub read_ms: f64,
    pub sort_ms: f64,
    pub sized: Vec<Option<crate::backend::dirsize::DirSize>>,
    pub dev: u64,
    pub writable: bool,
    pub first_metas: Vec<crate::backend::meta::Meta>,
    pub first_start: usize,
    pub first_ms: f64,
    pub watch_wd: i32,
    pub watch_mount: std::path::PathBuf,
}
// A failed listing carries its mode for the denial tile, or zero when no stat ran, with the armed descriptor or -1 when none needs removing.
#[derive(Debug)]
pub struct ListErr {
    pub error: FleaError,
    pub mode: u32,
    pub watch_wd: i32,
}
// The loop gave up first, so every descriptor the scan armed, expanded children included, goes back for its guarded drop.
pub(super) fn hand_back_late(
    out: &Result<ListOut, ListErr>,
    tx: &crate::backend::events::Sender<crate::backend::events::Event>,
) {
    if let Ok(done) = out {
        for &wd in &done.listing.outline_watches {
            let _ = tx.send(crate::backend::events::Event::AbandonWatch(wd));
        }
    }
    let wd = match out {
        Ok(done) => done.watch_wd,
        Err(failed) => failed.watch_wd,
    };
    if wd >= 0 {
        let _ = tx.send(crate::backend::events::Event::AbandonWatch(wd));
    }
}
// Sample request: {"c":"list","path":"/hung/dir","first":350,"hidden":false} on an nfs mount.
// Eight inputs: the request fields plus the mount table they classify against.
#[allow(clippy::too_many_arguments)]
pub fn list_dir(
    path: String,
    hidden: bool,
    first: usize,
    line: String,
    mime: std::sync::Arc<crate::backend::mime::Db>,
    fd: std::ffi::c_int,
    loop_tx: crate::backend::events::Sender<crate::backend::events::Event>,
    body: &str,
) -> Result<ListOut, ListErr> {
    let path_buf = flea::bytename::exact(&path);
    let late_tx = loop_tx.clone();
    call_with_flag(
        &path_buf,
        body,
        "scan",
        CALL_DEADLINE,
        STUCK_TTL,
        move |out: Result<ListOut, ListErr>| hand_back_late(&out, &late_tx),
        move || scan_dir(path, hidden, first, line, mime, fd, loop_tx),
    )
    .map_err(|timeout| ListErr {
        error: timeout,
        mode: 0,
        watch_wd: -1,
    })
    .and_then(|inner| inner)
}

// The bounded listing scanner owns this read through its late reply; no second timeout worker is started.
pub(crate) fn scan_dir(
    path: String,
    hidden: bool,
    first: usize,
    line: String,
    mime: std::sync::Arc<crate::backend::mime::Db>,
    fd: std::ffi::c_int,
    loop_tx: crate::backend::events::Sender<crate::backend::events::Event>,
) -> Result<ListOut, ListErr> {
    let base = flea::bytename::exact(&path);
    let wd = crate::backend::watch::Watch::add_raw(fd, &base);
    match crate::backend::scan::scan(&path, hidden) {
        Ok((mut l, read_ms)) => {
            crate::backend::picker::filter_listing(&mut l, &mime, &line);
            let (pass_ms, sort_ms, mut sized) =
                match crate::backend::ordering::request(&mut l, &base, &mime, &line) {
                    Ok(timing) => timing,
                    Err(msg) => {
                        let error = FleaError {
                            where_: "sort".to_string(),
                            path: path.clone(),
                            msg: msg.to_string(),
                        };
                        return Err(ListErr {
                            error,
                            mode: 0,
                            watch_wd: wd,
                        });
                    }
                };
            let (child_read, child_sort) = crate::backend::outline::expand(
                &mut l,
                &base,
                hidden,
                &line,
                mime.clone(),
                fd,
                loop_tx.clone(),
            );
            // Child rows shift every row after them, so sizes keyed by row index would land on the wrong rows.
            if !l.outline_open.is_empty() {
                sized.clear();
            }
            let dev = crate::backend::fsinfo::dev_of(&base);
            let writable = crate::backend::ops::dir_writable(&base);
            let first_start = l.first_start(
                &base,
                crate::json::field_str(&line, "select").as_deref(),
                first,
            );
            let (first_metas, first_ms) =
                crate::backend::meta::stat_range(&base, &l, first_start, first);
            Ok(ListOut {
                listing: l,
                read_ms: read_ms + pass_ms + child_read,
                sort_ms: sort_ms + child_sort,
                sized,
                dev,
                writable,
                first_metas,
                first_start,
                first_ms,
                watch_wd: wd,
                watch_mount: crate::backend::watch::live_mount_of(&base),
            })
        }
        Err(e) => {
            let mode = crate::backend::scan::mode_of(&path);
            Err(ListErr {
                error: e,
                mode,
                watch_wd: wd,
            })
        }
    }
}

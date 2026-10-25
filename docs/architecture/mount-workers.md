## Mount workers

Path syscalls leave the loop for mount-keyed workers, which is what keeps one hung server from
wedging the window. `src/backend/iomount.rs` owns the mechanism. `mount_key` names the mount a
path sits under lexically, never by stat, so a dead server costs no syscall there, and `is_remote`
says whether its syscalls can wedge: gvfs, network fstypes and any FUSE daemon. A local mount runs
inline with no hop. A remote one runs on a worker with a deadline, `CALL_DEADLINE` of 5 s for a
single call and `BULK_DEADLINE` of 15 s for a bulk pass, and past it the loop answers
"<mount> is not responding." A mount past its deadline is marked stuck for `STUCK_TTL` of 30 s, and
a call on one answers at once without a worker until that passes, when the next request probes it
again. A worker that dies before answering is this machine's fault and marks nothing: only a
deadline still running marks the mount stuck. Every `rename`, local or remote, runs on its own worker,
and its journal publication runs off the dispatcher too. A remote `mkdir` runs on its own worker with
CALL_DEADLINE, as reads do; a local `mkdir` stays inline with no hop. A rename or remote mkdir past
CALL_DEADLINE answers a `slow` line while the dispatcher keeps serving requests, never marking the
mount stuck for a write still running. An in-time write answers exactly as before. The worker
stays tracked and reports through the op channel when it lands, journalled exactly as the in-time path
would, so one journal entry per request and one undo reverses it. A `link` claims the one-operation slot
through `start_link` and runs every item on a worker from the start, so the loop stays responsive with
no `slow` line; one `linked` line and one journal entry land with the batch and release the slot. A batch in which every link failed answers one `error` line
instead (`<source>: this drive cannot hold links` for EPERM on vfat or exfat, else the failure's own reason, as EEXIST or ENOENT give), records no journal entry and still releases the slot.
Only `rename`, `mkdir`, `bulkrename` and `settags` answer `slow`. A worker that dies before answering marks nothing,
like a dead read worker.

What runs where: `list_dir` scans, sorts and stats the first window on the worker; `search` reads
each directory as one bound call returning (name, is_dir) pairs; `window` stats inline with no
listing clone on a local mount and moves only the window's rows to the worker on a remote one;
`sort` re-sorts on the bulk worker; `peek`, `thumb` and `listpaths` bound their per-row stats the
same way; the search root's dev and writability go through the bound before any walk starts.
`listpaths`' first window goes through `write_window`, which runs `window_metas`: a local base stats
inline on the loop's thread, a remote one moves its rows to the mount worker under the same deadline. A
window whose call fails answers its rows with zeroed metadata, then one error line naming the mount, so
the listing hold ends and the failure is still said. The zeroes are not cached in `window_meta`, so the
next window over those rows asks the mount again. A plain `window` request still answers only its error
line (`backend/run.rs`). The watch is armed on the list worker beside the current one: `Watch::add_raw` before the
scan, `set_incoming` plus `commit` on success, and on failure the loop drops the armed descriptor through `abandon_wd`;
a worker that outlives its call hands its descriptor back as `Event::AbandonWatch` for the same
guarded drop, kept when a re-list aliased it onto the live watch.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

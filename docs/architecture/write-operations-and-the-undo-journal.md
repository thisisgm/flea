## Write operations and the undo journal

Fifteen of the main backend's requests write. Twelve are file operations of their own: `transfer`,
`transfercancel`, `trash`, `rename`, `duplicate`, `mkdir`, `undo`, `link`, `redo`, `permissionsBatch`, `settags`
(file tags, journalled one `Tags` step per changed file, `tagsreq.rs`) and `bulkrename` (one journal entry for the
batch, `bulkrename.rs`); a New File from the menu writes
through `menuaction` and journals `MadeFile` (`opsdispatch.rs` `do_newfile`), and `archive` and
`convert` write below. One helper writes on a command loop of its own and journals nothing: the trash
browser's `restore` and `delete` (`trashbrowse.rs`, `trashdelete.rs`). The permissions dialog's single-item
`apply` (`permissions.rs`) journals the same `Mode` step `permissionsBatch` does, under the same `permissions` op,
so Ctrl+Z restores that mode rather than undoing whatever older write sits under it.
A `convertbatch` journals one `convert` entry with a `Copied` step per output it published, from the file it read,
a failed or cancelled batch included (`convertbatch.rs` `step`, landed by `OpMsg::Converted`), so undo removes an
output only while it is still the untouched result. That step's `from` names the source, and a copy of the source is
not the conversion, so `Replay::capture` refuses a `convert` entry and redo answers that it cannot be redone, as it
does for a pasted image. A Convert whose output name is taken lands under Duplicate's next free copy name
(`photo (converted) copy.jpg`, then `copy 2`) while one can be made and written, picked at the no-replace
rename, and otherwise refuses with the reason; the GUI card's output probe names the same one (`check.rs`
`converted`). Combine names its own output, so a taken Combine name refuses at once.
Trash recovery uses `RecordLeft` before its record is removed and `RecordUnconfirmed` after unlink
when the recovery folder cannot be reopened or synced: the latter says the record was removed but
Flea could not confirm that removal on disk, with the cause and the completed delete, restore or recovery state.
The completion refusal tests delete the claimed payload or restore it with the production no-overwrite
rename, then assert its absence from quarantine or its original content at the restored path beside the sentence.
A recovery folder entry read error uses `RecoveryUnreadable(root)`, is logged in `RecoveryReport.failures`
and ends the walk while preserving the recovered count; its sentence names the folder and the unread
records left there, while payloads stay in the quarantines their records name.
Recovery runs on a fresh Trash open or an explicit `list` with `recover:true`; ordinary refresh does not retry it.
The trash browser lists from `gio list -u -h -a trash::orig-path trash:///`, which escapes a line feed or any other control, non-ASCII or backslash byte in an original as `\xNN`, so one line is one item; an item gio cannot describe (no record, no `Path`, a relative or non-UTF-8 original, or a uri listed twice) is left out and logged, and the rest still list, where one such item used to refuse the whole Trash.
The trash browser's `restore` recreates an original folder that is gone before it moves the item back, the same as undo and gio, so a gone folder no longer fails the restore; one that cannot be made refuses and leaves the item in the Trash.
Rollback keeps the first claim refusal and its cause visible, with any record cleanup failure logged separately.
Completed steps name their temporary folder through its stable path, never an expired descriptor alias.
Recovery log assertions inspect only lines after a `seen` offset captured before the operation.
`docs/protocol.md` carries the wire; this is the part a reader of the code needs that the wire does not
say.

**One operation on the shared journal is one parse, one render and one walk (0.3.8).** The file is one JSON
document of up to 32 MiB, and every push, take and finish rewrites it whole, so the cost is linear in its size by
design. It was once that cost several times over: the file read and parsed twice, rendered three times for one
store, and walked once per Moved step of the new entry. `undoshare.rs` `load` reads and parses once, `Staged`
renders once and trims by the pieces' byte sizes, and `RebaseMap` rebases every stored step in one walk however many
moves the entry holds. `undocost_tests.rs` pins the counts (decodes, renders, steps visited, identity comparisons,
bytes read and written) with per-thread counters in `undoprobe.rs`, never wall time, and `undostage_tests.rs` pins
the written bytes to `tests/fixtures/undo-journal-golden.json`, which the code before the change produced.

**A write names its files once, when it arrives.** The viewport's read requests (`window`, `thumb`, `dirsize`) name
a row of the current listing, because a viewport is a fact about the listing. A write outlives the
listing it started from: a copy of a large tree is still running when the user navigates away, and a row
index would name a different file by then. So a write resolves whatever names its files once, when the
request arrives, and never consults the listing again: absolute paths are taken as they are, and the
`rows` forms of `trash` and `transfer` are resolved against the listing in force at that moment. Those
rows carry the number of the listing they were read in, as do `paths` and `menuaction`, and
`rowguard.rs` refuses a number that is no longer in force (see "The listing swap"). `collisions`, the read a paste asks before its write, names
its sources the way the `transfer` it precedes will: paths, rows resolved at request time exactly as
the transfer's are, or a menu's captured selection.

**One of `transfer`, `trash`, `duplicate` or `link` runs at a time.** `opsdispatch.rs` holds `Ops::running`, and a second `transfer`,
`trash`, `duplicate` or `link` while one is live answers an `error` line rather than queueing. `permissionsBatch` and
`settags` are refused the same way while one runs, without taking the slot. The reason is the
surface, not the backend: the operations design gives transfers the status bar's single transient slot,
so a second concurrent operation would have nowhere to report itself. `rename` and `mkdir` are exempt on a local mount, where
neither spawns at all; a remote one records a pending slow write for its mount instead, see below. An `archive` extract takes the transfer slot, so a copy, move or second extract
is refused busy while one runs; a compress and a convert never claim it: `Ops::claim_id` numbers them
and they run alongside by design, tracked in the detached registry a quit cancels, so the cap was never one write of any kind.

**A local `rename` or `mkdir` answers on the loop's thread; a remote one runs on a worker.** Both normally take one
syscall, but neither compatibility path below is one: an rclone directory rename copies the whole
tree and a GVFS WebDAV rename copies whatever the path is, file or tree, before removing the source,
and the copied tree keeps source file and directory modification times best effort through
`copy_any`. Errors setting those timestamps are ignored, so a Date Modified column or sort retains
the source history when preservation succeeds. On a local mount that copy runs inline on the loop's thread, as it always has.
On a remote mount, rclone and WebDAV included, which "Mount workers" classifies without a syscall, the write runs on
`slow_write_with`'s worker instead: past `CALL_DEADLINE` the loop answers a `slow` line and moves on, and the worker
reports late through `Event::Op`, journalled exactly as the in-time answer would have been. The window no longer freezes,
though the copy still has no progress and no cancel, because the worker runs the copy to its end and the late result
carries no byte counts and answers no cancel id. The `slow` line releases the one-at-a-time slot for its own id
at once, so writes on other mounts and on local paths run while the held write is still going; the write is recorded
as pending under the remote mount root its paths sit on instead. A write any of whose sources or target sits on a
mount with a pending slow write answers busy and journals nothing, and `undo` and `redo` answer busy naming the
pending path until the late entry lands, so no reversal races it. The late Done journals exactly as the in-time
answer would have and clears only its own pending entry, and one undo still reverses the late write like any other
entry. There is still no cancel and no progress for the running write.
`settags` follows the same rule through `tagsreq.rs` `apply_tags_with`: a change whose rows all sit on local mounts
writes inline, and one with any row on a remote mount runs on `slow_write_with`'s worker, keyed by that row's folder,
with the same `slow` line, pending claim and late journal (`OpMsg::SetTagsDone`).
A `settags` toggle over several rows is one decision for all of them, made in `tagsreq.rs` `resolved` from the
attributes `write_all` reads before its first write: where every target carries the tag it is removed from all,
otherwise it is added to all, so a selection where only some files carry it never loses the tag from those files
(Tags board callout 8). The `settagged` line names the op applied, `add`, `remove` or `clear`, and
`ui/TagState.qml` words its status line from that op rather than from its own guess, and says nothing when `ok` is 0.
The flyout's checked and mixed marks come from the held rows while every target is inside the held window; a
selection reaching past it sends one `tagstates` read when its menu opens, answered on a thread as `shebang` is,
and the open flyout redraws when it lands. `tagstates` only reads, so `rowguard.rs` does not refuse it; its line
names the numbering its rows resolved in, and the client drops an answer from any other. Pending tag changes are a
queue in send order, and each `settagged` answer or `settags` refusal (`where` `settags`, or `stale` naming
`settags`) takes the oldest. A remote change that answered `slow` can land after a later local one, and the two
status lines then name each other's tag; the counts and the window refresh stay right.
`trash` shells to `gio` twice for the list diff plus once to trash; `duplicate` may copy a
whole tree; a `transfer` is unbounded; a `link` Replace moves the name already there through `gio` first. Those four send their results back through `Event::Op`,
joined onto the loop's receiver exactly the way the thumbnail pool's `Event::Thumb` already is, so
the loop stays the only writer of stdout.

**Every write creates its target exclusively, and this is the module's whole safety story.** A file copy
opens with `create_new`, a directory copy and a `mkdir` use `create_dir`, a symlink copy uses `symlink`, and a rename
or a move uses `renameat2` with `RENAME_NOREPLACE`. None of them can destroy a file that is already
there. Four triggers send `rename` and its undo down one compatibility path that copies rather than replaces, and send a
transfer's move and its redo (`move_any`) down `move_cross_device`, the path its `EXDEV` already takes, so that copy
reports progress, can be cancelled and is journaled like any cross-device move (issue 282).
rclone 1.75 returns `EINVAL` for `RENAME_NOREPLACE` on a directory under a mount identified exactly
as `fuse.rclone` in `/proc/self/mountinfo`, and GVFS returns `EIO` for a rename under a
`/run/user/*/gvfs/dav:` WebDAV mount; `fuse.megafs` answers `EINVAL` the same way, and a rename or
its undo that crosses filesystems answers `EXDEV`. Every other mount that answers `EINVAL`, a kernel nfs or nfs4
export among them (PR 245), never copies: `noreplace_fallback` refuses a taken name, links a file or plain-renames a folder, and
`src/backend/renamecompat_tests.rs` pins that the folder keeps its inode, a taken name or empty folder still refuses, and nfs stays off the copy fallback.
corner: the lstat-then-rename window can replace a destination created in between; copy-fallback mounts never reach it because rename_noreplace keeps their EINVAL.
Ordinary rclone directory rename is never used because it was
proven to replace even a non-empty target. `renamecompat::rename_path` instead builds the target
through the existing exclusive copy primitives, removes the source only after the copy completes,
and uses the same path for undo. On a durable destination (a dav share classifies network) the copy's
folder is confirmed before the source goes; a confirm that fails takes the landed copy back and answers
`rename` with `RENAME_UNCONFIRMED`, the drive did not confirm the folder, so the rename was undone.
**When that take-back fails too, a whole or partial copy stays under the new name beside the whole source,
and nothing tells the operator that either.** The error answers `rename`, whose fixed sentences reproduce
neither `path` nor `msg`, and it is never `rename-kept`, whose sentence trusts the copy and doubts the source,
the inverse of this state; the distinct wire kind deferred for the two leftovers below would cover it too.
A copy failure removes only the partial target this operation
created. **When that removal itself fails, nothing tells the operator what was left behind.** The
error answers `where` of `rename`, which `ui/js/Errors.js` words as one of its two ordinary rename
sentences, neither of which mentions a leftover, and which `ui/PaneWire.qml` does not re-read the
listing on, so the partial tree sits on disk unmentioned until some later listing shows it. The
honest repair is a distinct wire kind, deferred beside the sibling case R14 already recorded rather
than added at round six. A source-removal failure takes the copy back only on proof
the source survived whole: a source that still stats as anything but a directory after the failed
removal, because `remove_file` removes every other kind with one unlink that either takes effect or
does not. That case removes the copy
again, or reports that it could not, and answers `rename` either way. Every other state keeps the
complete target copy, because `remove_dir_all` stops at its first failure and a source that no longer
stats, already gone or unstattable, proves nothing on the flaky mounts this path exists for. **When
taking the copy back also fails, the copy stays under the new name and nothing tells the operator
that either.** The error line does carry it in `path`, but `ui/js/Errors.js` words `rename` as one of
two fixed sentences that reproduce neither `path` nor `msg`, and `ui/PaneWire.qml` does not re-read
the listing on it, so the duplicate sits on disk unmentioned until some later listing shows it,
exactly as the partial target above does. An archive compress, extract or convert publishes its staged result
through the same path (issue 370), by `renamecompat::rename_cancellable` with the job's own cancel: a cancel during
the copy removes the partial target and the job answers cancelled, and the copy reports no progress past the job's
last sample. Every other filesystem and error, and every
`rename_noreplace` caller but `rename`, its undo, `move_any` and those three publishes, stays on the atomic syscall. Mountinfo's escaped mount point is decoded and the deepest
enclosing mount wins, so a nested non-rclone mount cannot inherit the exception.
That kept copy answers `rename-kept` rather than `rename`, because the rename half
succeeded and the name it came from may be whole, a remnant, or already gone. The error line names the source in `path` and carries the removal's own message in `msg`;
neither field names the target. `ops::rename` records nothing for it, because only its `Ok` arm
carries a step list, and `do_undo` never pushes at all, so no undo in either direction removes the
kept copy: `remove_any` answers with one error and no account of how far `remove_dir_all` got, so the
source may be whole, a remnant, or already gone, and the backend cannot say which.
`ui/PaneWire.qml` re-reads the listing on that kind instead, since the pane would otherwise draw one
row beside a sentence the listing denies. `undo` reverses a rename through the same call and answers
the same kind, so the sentence names no direction, warns that the name the copy came from may now be
incomplete, and leaves the refreshed listing to show which names are on disk. The journal spends its
entry either way, because a failed reversal that stayed would block every older undo behind a step
that keeps failing.
A twin stranded by a failed move-back answers `rename-stranded` rather than `rename-kept`: the temp sibling holds the file under a hidden dot name, so `path` is the source, which closes the rename request, and `msg` names the temp leaf with the move-back cause ("the file was left as .flea-case-N-M in this folder: the drive could not read or write the data"). `ui/js/Errors.js` capitalises it as it does a transfer, `ui/PaneWire.qml` treats it as `rename-kept` for the refresh, and a move through move_any (copyfile.rs:607) carries the same `msg`, on a `redo` error for a redo and on the transfer item error for a non-regular item.
corner: the copy is not snapshot-isolated, so a source replaced after the copy completes is destroyed
by the removal that follows, and a concurrent write into the operation-created partial target is lost
with it; both are accepted rather than defended against.
corner: `remove_any` removes a directory with `remove_dir_all`, which stops at its first failure, so
the name a kept copy came from may be left whole, half emptied, or already gone; the sentence
promises the copy only, says that name may now be incomplete, and tells the operator to check it
before deleting anything. Removing the duplicate is the operator's call, not Ctrl+Z's.

**One undo history for every Flea window.** Each backend keeps its in-memory journal until the session dir validates, then every push and take goes through the file under `$XDG_RUNTIME_DIR/flea/` (`FLEA_UNDO_DIR` when set) under an flock, so the newest entry from any window is what Ctrl+Z undoes. A take is one locked read-modify-write, so two windows pressing Ctrl+Z undo two different operations; the filesystem work runs after the claim, so a slow undo never holds the lock. The published doc stays under 32 MiB by dropping oldest undo then redo records until it fits; an entry too big alone stores a small barrier through the normal store path, its op name only and no paths, and no window keeps the payload. Any window's undo that claims a barrier consumes it in the same locked claim and answers `That operation was too large to undo.`, the next undo continues with older entries, and a barrier never enters redo. The codec versions the file: a decoder meeting a newer version treats the file as unavailable, falls back to memory, and never rewrites the newer file. A key added to an existing record is optional and keeps the version: an older decoder ignores it and its re-render drops it, so a reader treats a record without it as the older record (`gen` reads 0, a trash step without `before` and `time` refuses its undo as 0.3.8 did). Only a record an older decoder would reject moves the version, as the barrier moved it to 2. A claim stamps the push generation, and the finish replays only when no push ran since, otherwise the stale replay is dropped.

**The journal records only what an operation created or moved.** `undo.rs`'s `Step` has nine shapes the
product writes: `Moved` (rename back), `Copied` (remove the copy; a failed tree copy carries a manifest, a success carries none; a merge top carries `merge: true` and never takes the whole-tree fallback; a copy made under the FAT card's Rename carries `rename_fat: true`, and so does a `Moved` step made under it because a cross-device move copies first, journaled as `"fat":1` only when set, so a redo writes the same names, an older journal reads as made without it, and the journal version stays 3 because an older build that ignores the key undoes the copy alike and only redoes it under the names as they were, which the drive refuses and names), `MadeDir` (remove it
while it is still empty, because whatever is inside it now was put there by someone else), `MadeFile`
(remove it while it is still the untouched empty file), `Trashed` (restore it, written by a trash and
by a transfer that replaced an item, ahead of that item's own step), `Linked` (remove the link only while it is still that link), `RemovedEmptyDir` (recreate an emptied merge source with its mode), `Mode` (restore bits, skipping a replaced file with a note) and `Tags` (restore tags, one step per changed file, skipping a replaced file with a note); `Created` exists only
for the tests that drive undo's own ladder, and `Barrier` marks an entry too big to journal. A path an operation merely read is never recorded, so an undo
cannot delete a file the operation did not put there once the step is journaled. An operation
whose step list is empty is not pushed at all, so a refused rename leaves nothing to undo. Steps reverse
newest first, and a failing step stops the rest rather than half-reversing, except a `Mode` or `Tags` step whose
file was replaced, rechmodded or retagged since: that step is skipped with a note while the rest restore, and redo
skips the same way. A `Tags` step pins the file's dev and inode beside the attribute before and after, and
`tagsundo.rs` writes only onto that same file still holding the value it expects, `after` for undo and `before`
for redo; a file replaced since undo fails redo's source check first (`redo.rs` `source`) and refuses the whole
redo, as a replaced move source does. That check judges a `Tags` step by its dev and inode alone, since the tag
write itself moves the file's ctime: a retagged file passes it and is skipped by the replay's tag compare. A partial `permissions` undo captures the restored steps for redo, so redo replays
the undone half before anything older. A hard link is removed only while its source still holds the same
dev and inode, so undo leaves the last name in place when the source is gone or replaced. A copy that fails short of a
cancel (ENOSPC, EPERM, a pulled device) leaves nothing under a name it created: a new folder was built under a hidden
sibling and a file was unnamed or under one (see "A copied file or folder takes its name only once it is whole" below), and
that goes with the failure, so there is nothing to journal. What a copy wrote in place stays, because removing it on a
transient error would destroy the files that landed whole: a tree on a FUSE destination, the entries a merge landed in a
folder that already existed, and a sibling whose removal itself failed. `copyfile.rs` reports that path in
`Progress.partial` so `transfer` and `duplicate` journal it as a `Copied` step. A failed tree copy records every path it creates in `copymanifest.rs` as it runs, each identity captured at create from the copy's own descriptor (fstat) or its at-path pin and buffered to an anonymous file on the runtime filesystem in 64 KiB batches, never on the destination, so a full destination cannot fail the manifest and the journal holds no descriptor on the mount being ejected; every directory move is manifested, since EXDEV through a symlinked parent defeats a device check and a rename drops it unread. A failed append latches and the transfer reports it loud beside the copy error. Finish stats nothing, so a failed or cancelled copy answers at once. Each `Copied` manifest stays in the recording backend keyed by a per-entry nonce stored in the file, so a claim by that backend reattaches it and walks it deepest first, removing each recorded path only while it still holds the recorded identity and keeping a file edited after the copy, a path replaced by another inode, and a directory left non-empty by a stray, reporting each kept path with its cause; a claim elsewhere finds no key and takes the whole-tree fallback, and a manifest that never verified a record falls back the same way, while a success journals the plain step with no manifest. A destination that already existed is never reported, because nothing was created there.
corner: a copy is not snapshot-isolated, so a concurrent write into a recorded path that keeps its identity goes with the tree; only identity mismatch keeps a path.

**An entry a drive cannot hold is skipped and named, and never ends the tree around it.** `src/backend/unheld.rs`
decides it from the create that failed, in a copy and in a move's first half alike: a link refused with
`EOPNOTSUPP` or `ENOSYS`, or `EPERM` under vfat or exfat, whose kernel drivers have no symlink or mknod operation;
a fifo, socket or device node refused the same way; a name refused with `EINVAL`, `EILSEQ` or `ENAMETOOLONG`;
`EEXIST` under vfat or exfat beside a source sibling that differs only in case, in trailing dots and spaces, or in
the characters the FAT card's Rename replaces (only in a copy that answered Rename, `Durability.rename_fat`, where two names that land as one after Rename, inside a folder, are
named here and not a hard failure; under Skip or no answer the names are compared as they are), which is the drive folding two names into one (on any other drive, or with no such sibling, the name is another writer's
and the item still fails, so a stray planted mid-copy keeps the undo guarantee `undo_tests.rs` pins); and a file of
4 GiB or more onto vfat.
`copy_dir_entries` records it and goes on with the next entry, naming the first 100 and counting the rest, so a tree
of a million links stays bounded. `EPERM` anywhere else stays the copy's own error, because on a drive that holds
links it is a permission and not a capability. A link is never followed in place of the link: a link to a folder
would duplicate a tree onto the drive, a cycle would never end, and a broken link has nothing to follow. A tree that
landed without some entries is a failed item and a partial copy: `copy_any` sets `Progress.partial` and answers one
sentence with the count and the first entry, the manifest makes undo remove exactly what landed, and
`run_transfer_checked` still confirms what landed on a durable drive. A move walks the same way and then keeps its
source whole, since a move never drops an entry to succeed: every move enters the walk only through `copy_any`, a
folder alone through `move_cross_device`, and a batch stages regular files only. `transferdone` names in `refused`
each entry an item's sentence counts, and none for an item that ended on another error; the status bar draws the
item's sentence, the first entry and the count.

**A cross-device move of many items confirms its folders once per batch instead of once per item.**
`src/backend/movebatch.rs` stages each copy and only removes a batch's sources after one confirm.
On a block vfat stick each copy holds its file and skips its own fsync.
Elsewhere each copy fsyncs its file as today.
The stick is found from the destination's resolved path, the one `dest_is_durable` already classifies, so a destination reached through a symlink batches like the stick and a link into an rclone mount stays rclone; `resolved_in` in `src/backend/extclass.rs` walks the link chain one component at a time with `readlink` (at most 40 links), looks up each prefix's mount in the mount table before the next syscall, and at the first prefix inside a mount that can wedge a syscall (a share, a FUSE mount, a gvfs path) stops and answers that prefix plus the unresolved tail, so no syscall ever lands inside such a mount; `classify_in` and the fsinfo answer call that same walk, so no classifier canonicalizes through a link into a share.
That resolved-path decision moves two ceilings, each re-derived with `wc -l`: `src/backend/durable.rs` 803 to 870 for the decision and its walk, and `src/backend/durable_tests.rs` 1305 to 1396 for its five gates. Moving the walk into `src/backend/extclass.rs` (300 to 381, inside the hard cap) takes `src/backend/durable.rs` back from 870 to 809, re-derived with `wc -l`.
A confirm runs one syncfs on a clone of the first held file while every original is open, drops the clone, hands the held files to a close wave and returns, then runs one folder fsync.
The wave closes its files on background threads while the next batch copies and confirms.
It is joined before the next wave starts, so two batches of descriptors are the most ever open, and before the operation reports its result, so no descriptor is left on the drive when the done line goes out.
`finish` joins the last wave for a transfer and a single-item move, and `Durability`'s drop does for a duplicate, a fallback rename and a redo, each before its caller reports.
A failed confirm hands its files to a wave the same way.
`Durability::flush_dirs_for_many` is that confirm, or each filled parent once with no durability context.
A batch closes after `BATCH_ITEMS` (64, which bounds one unconfirmed batch to one folder fsync while keeping a cancel's cleanup cheap).
It also closes when the destination folder changes, on cancel, on the first failure, and at the end.
A failed confirm keeps every source of that batch whole and journals each copy as a partial exactly as the single-item failure does.
A cancel completes the landed items as moves and abandons only the in-flight one.
A same-filesystem rename and a replace never stage and run exactly as before, and `move_any` keeps its single-item contract for redo.
A single-item move, a duplicate, a fallback rename and a redo confirm through the same `flush_dirs` primitive, so none removes a source before its bytes are on the drive.
A batch's item lines go out when the batch closes rather than when each copy landed, so an early item's `ok` waits for the batch behind it.
Progress lines still stream per item while it copies.

**A paste or a drop onto names that exist asks once, and the answer covers only what was asked.**
Before it sends a transfer, `ui/CollideHost.qml` sends `collisions`, which `collide.rs` `ask_beside` serves
read-only: the sources whose name `dest` already holds, each kept in `Ops::question` with the identity
of the item at that name, the latest question only. The transfer then carries `collide` and
`collideId`, and `collide.rs` `Policy::place` applies the choice to an item only while that question
listed its source for this destination and the name still holds the same item (device, inode, type, plus birth time when the filesystem reports it on both sides).
Every other existing name goes on to the exclusive create and is refused by it exactly as before, which
is the whole race story: a name that appears while the card is open is never replaced, kept or skipped.
Keep both is `ops.rs` `free_copy_path` worked out in `dest`, the name Duplicate gives. Skip counts the
item in `skipped` with no item line, the way a cancel counts one it never started. **Replace never
deletes: it trashes, then creates exclusively.** `replacedefer.rs` `replacing_deferred` (re-exported by `mergeapply.rs`) trashes the item already
there through the transfer's one deferred session (`Deferred`, begun by the first Replace, which lists the trash
before and once more when the batch settles, so a batch of N costs two lists, not 2N) and pushes its `Trashed` step, then the ordinary transfer pushes its `Copied` or
`Moved` step into the same entry, so one undo removes the incoming item and then restores the old one to
its name. An item on the home Trash's own filesystem skips gio: `hometrash.rs` `rename_to_home` reserves its info file, renames the item into the Trash in-process and names its own entry, and the batch syncs the folders those renames touched once when it settles. The info file is synced before its item moves only where `durable.rs` `dest_is_durable` holds for the item's folder, read once per batch; on a fixed disk it is written unsynced, as gio writes it, because 300 such syncs were 27% of a 5,000-file merge with 300 Replaces on East (base 0.663 s against 0.482 s without them). A transfer that fails with nothing holding the name, a cancel included, restores the old item
at once and drops its step, so a refused copy leaves the destination as it found it. When that restore
fails, the step stays for undo and the item's error says the old item is still in Trash, a cancel's
too, and that item counts as failed rather than skipped, since its name no longer holds what it held,
while the transfer still reports the cancel. A partial copy holding the name keeps
both steps, and undo meets the partial under the manifest rule above: it removes only the recorded paths that still hold their identity and keeps any stray, reporting each kept path with its cause. A tree copy that failed after writing any file inside it still removes every file it made, keeping only what another writer added or changed after the copy, so the old item stays in Trash for the trash browser to restore only when the partial keeps a stray. A folder is replaced whole, the old one going to Trash, and never merged. Where the destination has no Trash of its own (no GVFS backend has one, which `src/backend/hometrash.rs` `trashable` reads off the path, shared with the TUI), the old item goes to the home Trash by copy instead: `info/<name>.trashinfo` first with O_EXCL and a unique name past a collision, then the whole copy into `files/<name>` with a symlink kept a link, both synced, then the original removed, journaled as today's `Step::Trashed` with its `trash:///` URI so undo restores it to its GVFS path; a copy that fails leaves no partial and the original in place, a full home Trash filesystem has its own refusal sentence, and any other refusal keeps today's message. A trash that
gio refuses on a trashable destination fails that item with today's message and touches nothing. An item
already there that holds any source the batch names, its own or another item's, is refused rather than
trashed with that source inside it, whatever order the batch runs in; so is one an incoming symlink
resolves to now, or whose text leads back to that name once the link sits there, which `collide.rs`
`walk` looks up as the kernel would, through any other link on the way, because the copy would be a link
to itself; a lookup that passes 40 links without reaching that name is its own outcome, let through like
a dangling link, since the kernel refuses it either way and the old item waits in Trash for undo.
Skip's items are left out of the sweep's
batch total too, by the same test `Policy::place` applies (the question listed the source and the name
still holds that item), run on the sweep's own thread, so the time left counts only what will move.
`redo.rs` learned one rule for it: a step whose destination an earlier `Trashed` step of the same entry
vacates skips the up-front "destination already exists" check, and meets it again right before it
runs, after that trash; the up-front pass collects those names as it goes and checks the cancel flag
per step, so a redo of a 100,000-item copy stays linear and can be stopped before its first item.
**The question runs beside the loop.** `collide.rs` `ask_beside` captures a menu's selection on the
loop's thread, because the close that expires it may be the very next request, and does the rest on a
thread of its own: measured on the aarch64 build container, 0.9 s for 100,000 local sources (the list
alone 0.13 s), and a network mount pays a round trip per `lstat`. The answer comes back as
`OpMsg::Asked`, and `landed` keeps it only if no later question was asked meanwhile, and only then is
its line written: an earlier question that finishes after a later one was asked is dropped unanswered,
the way a superseded listing's result is. The client asks one question at a time: `ui/CollideHost.qml` refuses a second while one is in
flight or its card is open, and says so in the status bar (`Collide.refusal`), rather than overwriting
the waiting transfer, which used to drop the first one without a word. **Any `collide` value
also settles the same-folder case**: a copy into its own folder takes Duplicate's name without a
question, and a move onto itself is counted in `skipped` with no error and no journal step; without
`collide` both still fail `already in that folder`, so the TUI and older clients are unchanged. The
card is `ui/CollideConfirm.qml` on `ui/TrashConfirm.qml`'s card, keys and look, with Keep both focused
because it is the one choice that changes nothing already there. The card draws the MergeFolders header through `ui/DialogHeader.qml`
(`ui/DialogTitle.qml` with esc, the board's air and a hairline rule, shared with the run and unlock cards, as is the
scrim `ui/DialogScrim.qml`) and a 12 px bottom pad. The cut a paste spends leaves the
clipboard only when its transfer is sent, so Cancel keeps it. Paste, move-paste and both drop paths
ask, the shelf's drop asking about the paths its drag carries while its token still names what moves,
and so do the menu's Copy to, Move to and Move to Dropbox, one behaviour wherever a destination is
picked. Their question carries the `menuId`, and `collide.rs` keeps the menu's selection and
destination as the question saw them, because Copy to closes its dialog right after it approves, and
that close expires the live selection before the answer lands; `menu_sources` hands the transfer that
capture, with its identities still checked per item. Move to Dropbox lost its own "Moving N items to
Dropbox" sticky with this, because a move waiting on the card would have left it standing after a
Cancel; transferstarted names the move a moment later. The shelf's own `flea shelf` actions and the
TUI pass no choice and refuse as before. **The pane does not navigate behind the card**:
`ui/js/MouseNav.js` `refused` stops `mouseBack` and `forward` while `pane.collide.opened`, the way it stops them
behind the context menu, since the transfer waiting on the card names the folder it asked about; the
keyboard and the chrome's own back and up buttons are covered by the card's focus and backdrop.
`tests/ui-operations-design.sh` and `tests/ui-providers.sh`
drove their error and retry footers with a real name collision through Copy to and Move to Dropbox;
a collision now asks instead, so the Copy to flow fails on an unreadable source file and Move to Dropbox
fails on a source folder that cannot be written, each a real failure that is not a name.
corner: replacing N items through gio costs two `gio` lists per batch, one before the first trash and one when the batch settles, plus one attribute list per Replace whose landing failed. A Replace that goes to the home Trash in-process, renamed there on its own filesystem or copied there from a place with no Trash of its own, names its own entry, so it neither lists nor is refused when gio cannot list; the trashes themselves stay per call, because one batch trash up front would have to restore every untouched item on a cancel.

**The trash URI is captured at trash time, and that is forced by a measured fact.**
`gio trash --restore` refuses an original path (`Location given doesn't start with trash:///`), and two
files trashed from one path both list that same original, so a later lookup cannot tell them apart.
`trash.rs` therefore reads `gio trash --list` before and after the call and keeps the entries that are
new. A `trash` request first renames each item whose own device is the home Trash's into it in-process (`hometrash.rs` `rename_to_home`, the same path a Replace takes), which names its own `trash:///` entry, so that item costs no gio and no list; only the paths left over (another filesystem, an item inside the Trash, a rename that failed) take the gio path and its two lists, and the request syncs the folders its renames touched once. For the paths that take gio, when a settle list fails or lists nothing new, a trash step (`trash_checked`) and a Replace step both keep the before list's URIs for their own path (none when the before list itself failed) and a time (the moment the chunk holding the path went to `gio trash`; a Replace trashes one path and keeps the moment its call began, taken before `gio` runs like a chunk's), and undo resolves the step from a fresh list only when exactly one entry for that path is new since and its deletion date falls from five seconds before that time to five seconds past the longest a `gio` call runs (`CHUNK_SECS`, the 10 s timeout and 1 s kill delay); a step with no trash time (an old journal record only) and a path with two new entries both refuse with "cannot be restored" rather than guess, and a list that fails at undo refuses with "the Trash could not be listed", never claiming the item is gone. The operations design says restore takes the original path; it does not, and this is the deviation. Undo puts a top-level home Trash entry back in-process (`homerestore.rs`), as gvfsd-trash does, when its info file's `Path=` names the step's own path byte for byte, refusing an occupied original and leaving the entry; any other entry goes through `gio trash --restore`. Each in-process restore recreates a missing original folder first, as gio does, and an undo holds the folder syncs of all its restores (`homerestore.rs` `SyncBatch`) and syncs each folder they touched once, before the journal records the undo, where a sync per item made a 100,000-item undo take 95 to 102 s against 5 s.

**Which paths were trashed is read off the filesystem, not off `gio`'s exit status**, which covers the
whole batch. A path still present afterwards is counted `failed`.

**A cancel stops the item in flight and removes its partial destination.** `copyfile.rs` checks the
cancel flag between chunks and unlinks what it had written. The operations design says the in-flight
item finishes; it does not here, because a cancel that waits out a multi-gigabyte copy is not a cancel
and a half-written file is not a result anyone asked for. A `quit` or a closed stdin cancels the same
way and waits for the terminal line, so shutting down mid-copy leaves nothing half-written either. Both cancel at the
door, as `transfercancel` does: `events.rs` sets the slot operation's flag and every detached job's (a copy is one) as the line or the end of input is read,
because a `quit` behind a listing still out on a busy device (a phone's MTP mount serves one request at a time) waited
for that scan, up to its mount deadline, while the copy ran on, and a device pulled meanwhile failed the read short of a
cancel. After the shutdown the backend points stdin and stdout at `/dev/null` (`run::let_go_of_client`) before it
exits, because a scan worker the kernel still holds in a dead mount's `readdir` keeps the process, and with it the pipe,
alive until that call returns; the window takes the end of output as the drain and reaps the process off its own thread
(`crates/flea-gui/src/backend/worker.rs`).

**A copied file or folder takes its name only once it is whole.** A copy that creates a new folder (a folder
copied or moved across filesystems, or a subfolder a merge adds) builds it under a hidden sibling in the destination folder,
`.flea-part-<boot>-<pid>-<n>` (the first eight hex digits of the boot id, the backend's pid and a per-process count), writes
every entry inside it in place, gives it the source's mode and mtime last, and then one `rename_noreplace` gives it the
name, so a tree of ten thousand files pays one rename. A file copied into a folder that already exists (a loose selection,
or a merge) is created exclusively with the source's mode as an `O_TMPFILE` inode with no name, and `linkat` through its
`/proc/self/fd` entry names it once whole; a filesystem whose first `O_TMPFILE` create is refused (vfat, exfat, nfs, cifs)
is remembered by its magic for the life of the backend, and its files go to a hidden sibling of the same shape and a
`rename_noreplace`. Either way the mtime and tags (through the descriptor) go on first, a durable destination confirms the
bytes, and only then does the name appear (a `batch_syncfs` stick names a file at once and confirms it at its next batch
`syncfs`, still before any move removes a source), so a name another writer took meanwhile answers `EEXIST` through the same
`unheld::refused` the exclusive create used, the staged copy goes, and the other file or folder stays. Mid-copy the
destination shows nothing under the name, so a kill or a crash leaves at most a hidden sibling, and an unnamed file leaves
nothing at all; a power loss can still leave a name short while its bytes are unconfirmed (on a destination the copy does
not confirm, or on a `batch_syncfs` stick before its batch `syncfs`), as before. The next `transfer` into the folder removes
this user's regular files and folders whose whole name has that shape and whose copy cannot finish: same boot and its
process gone, or another boot (on a kernel network share, `nfs`, `cifs`, `smb2`, `9p` or `ceph`, only once unchanged for an
hour by its ctime, since another machine may still be writing it, and never another boot's folder there, whose ctime
does not move for writes deeper in it); `copysweep.rs` does this. A name with anything around
that shape and a link are never matched.
corner: a FUSE destination (rclone, gvfs MTP, WebDAV and SMB, any other daemon), or one whose `statfs` failed, writes in place as before, because its
rename may be refused, done as a copy on the device or start a second upload, so a kill there still leaves a partial under
the name; a cancel and a quit still remove it. corner: a kernel without `RENAME_NOREPLACE` (nfs) lands the sibling through
`noreplace_fallback`: a file by a hard link, which refuses a taken name, and a folder, or a file where links are refused,
by a checked plain rename, whose window between the check and the rename can replace a name created in it (for a folder,
only an empty one).
corner: with hidden files shown, a copy's sibling is listed while it fills, under its own hidden name.
A `transfercancel` naming a compress or convert id still does nothing, there is no UI cancel for
either, but a quit or a closed stdin sets each detached flag and drains them under the one 25 s
shutdown budget, so each compress and convert `Work` cleanup runs before the process exits and
no `.flea-work-*` folder is left behind. A job whose tool has not exited ten seconds after the cancel keeps
its folder in place and its terminal `err` names it; one still running when the budget ends is abandoned with the process.

**Testing them is `tests/ops.sh`**, which drives the real binary over a FIFO rather than a pipe: an
operation answers asynchronously, so a piped script would send `undo` before the operation it meant to
reverse had reported. Its sandbox is under `FLEA_FIXTURE_ROOT` and not `/tmp`, because `gio trash`
refuses `/tmp` and `/var/tmp` with "Trashing on system internal mounts is not supported"; `/home` is the
only mount on this box a trash round trip can be exercised on. Hard rule 9's guard is in the script
itself, and the Rust tests use `TestDir`. `collide_replace_tests.rs` drives Replace, its undo and its redo
against a stand-in for `gio` that `trash.rs` takes from a thread-local in test builds only, because a
build container's `gio` has no `trash://` to list or restore; the stand-in's trash is a folder in the
test's own sandbox.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

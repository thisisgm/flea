# Direct cloud copies (opt-in prototype)

**Upload to cloud…** in a file/folder context menu copies one selected local item
using rclone directly, without the mount's VFS write-back cache. Ordinary paste,
move and mount monitoring are unchanged. Install/configure rclone separately, then
create a regular user-owned `$XDG_CONFIG_HOME/flea/cloud-targets.json` file (not
writable by other users):

```json
{"targets":[{"id":"photos","label":"Photo uploads","remote":"gdrive","root":"Flea uploads"}]}
```

`remote` is an existing rclone remote **name without a colon**, not a connection
string. Flea never reads credentials. `root` and the dialog's destination folder
are relative paths; traversal, control characters, colons and backslashes are
refused. The selected basename is preserved: choosing `album` and folder `2026`
above targets `gdrive:Flea uploads/2026/album`. Multiple configured targets cycle
with the target button (Enter/Space also work).

The job first produces a complete MD5 manifest, copies, then runs **rclone checksum
MD5** against that manifest and requires a positive match for every file. Backends
without MD5 support fail verification; size-only comparisons never count as
verified. Metadata identities are compared before/after to reject changed local
sources. Verification describes the completed sample, not a continuous guarantee
against later local or remote changes.

Originals are **never deleted**. Existing destination files are skipped, then
checked: matching copies succeed and differing copies fail without replacing them.
The command uses `copy --ignore-existing --immutable --checksum`; `--immutable`
alone on the tested rclone 1.75.1 single-file `copyto` path did not prevent a
replacement, which the integration suite caught. One job per configured remote
name is enforced across Flea windows with a private runtime lock. **Do not run
another uploader against the same destination paths at the same time**: rclone's
existence check is not an atomic no-replace guarantee against external writers or
another alias of the same remote. A dedicated destination is recommended initially.

Hidden files are included. Empty directories are omitted; an empty-only selection
is refused. Symlinks, special files, non-UTF-8/control-character names and network
or cloud-mounted sources are refused. Each job is limited to 100,000 source entries
and 32 MiB of command output. Preparation and verification read source metadata
outside the GUI thread; no recursive work is added to listing or scrolling.

States are Preparing, Uploading (transfer traffic/rate where reported), Verifying,
verified completion, error and cancellation. A zero rate is **not** diagnosed as a
Google quota error. The byte line reports rclone transfer accounting, including
retry traffic, not the unique size of the selected files. Both transferred bytes
and rclone's estimated total can grow when data is retried; they are not a
folder-size measurement or checksum-confirmed completion percentage. Cancel and
window close stop only this job; uploaded/partial
remote objects may remain, originals remain intact. Retry rechecks complete files;
partial-file resume and persistence across application restarts are not promised.
The owned worker has a 24-hour job bound, rclone connection/idle timeouts, bounded
retries, and child cleanup on cancellation or owner death. Configuration loading
and failed executable startup have five-second UI deadlines.

Read-only GUI acceptance state: `qs ipc --pid <pid> call flea cloudUploadState`.
The underlying CLI helper is `flea --cloud-copy <target-id> <relative-folder>
<absolute-source>`; its stdin must stay open and EOF or input cancels the job.
`flea --cloud-targets` lists only Flea's allowlisted labels/destinations. It does not
list rclone credentials or discover other remotes.

Optional checks: `tests/cloud-copy.sh` uses an isolated local rclone alias, while
`tests/cloud-copy-ui.sh` drives the real QML process lifecycle with a fake transport.
Neither uses an actual cloud account.

### UI composition and acceptance scope

There is no identical upstream cloud-copy dialog. This addition composes Flea's
existing `MenuActionDialog.qml` card geometry and `DialogTitle`, `DialogField`,
`DialogButton`, `CardScroll` primitives at upstream baseline `f738261`, using existing Theme
and Commons Style roles. It adds no Controls import, palette, animation system or
new shared tokens. The dialog is lazy and restores the originating list's focus.
The offscreen suite verifies behavior/imports only; live dark/light, narrow-window,
keyboard/pointer and installed-state acceptance remain separate checks.

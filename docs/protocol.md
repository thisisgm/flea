# Flea backend protocol

One JSON object per line, newline-delimited, over the `flea --backend` child's stdin
and stdout. No batching, no length prefix: `Quickshell.Io.Process` reads with a
`SplitParser` on `\n`. Implemented in `src/backend/proto.rs` over the field scanner and
escaper in `src/json.rs`, dispatched by `src/backend/run.rs`.

## Invariants

1. A successful `list` is always followed by a `rows` line, even when `first` is
   0, in which case the array is empty. A `list` whose path cannot be read
   answers a single `error` line instead, and no `rows` line follows.
2. Errors are messages, never exit statuses. `--backend` runs until it reads `quit`
   or stdin closes, and returns exit code 0 either way; every failure is an `error`
   line the client reads off stdout, not a process exit code.

## Requests

### list

`{"c":"list","path":"<string>","first":<uint>,"hidden":<bool>}`

Example: `{"c":"list","path":"/home/gm","first":350,"hidden":false}`

Scans `path` (phase 1: names and the directory bit only), sorts by name with
directories first, and answers a `listed` line followed immediately by a `rows` line
covering rows `0..first`. A `path` that fails to scan answers a single `error` line
instead and leaves the previously listed directory in place: a failed `list` cannot
mix a new `path` with the old listing. A missing `path` defaults to the empty string,
a missing `first` defaults to `0`.

`hidden` of `false`, or a missing `hidden`, drops every name starting with `.` before
it ever reaches the listing: the filter runs inside the scan itself, not as a later
pass over it, so a hidden directory costs nothing beyond the `readdir` entry it was
always going to read. `hidden` of `true` scans dotfiles in too, ordered the same as
everything else. There is no separate flag to ask for dotfiles without also
re-scanning: `sort` reorders whichever listing `list` last produced and cannot add or
remove rows, so changing `hidden` always means a fresh `list`, which is also what
clears the cursor and selection back to row 0.

### window

`{"c":"window","start":<uint>,"count":<uint>}`

Example: `{"c":"window","start":1200,"count":350}`

Stats rows `start..start+count` of the current listing (phase 2) and answers one
`rows` line. `start` and `count` are clamped to the listing's length, so a stale or
out-of-range window never panics, it just returns fewer rows or none. Missing fields
default to `0`.

### sort

`{"c":"sort","by":"<string>","desc":<bool>}`

Example: `{"c":"sort","by":"size","desc":true}`

Re-sorts the current listing by `by` and answers a `listed` line. `by` is one of three keys,
and directories come first under every one of them, in both directions:

- `"name"` works on phase-1 data alone: `read` is `0.0` and `sort` is the sort.
- `"size"` and `"mtime"` pay the metadata pass first, one `lstat` per row of the whole
  listing split across the cores, then sort. `read` is that pass in milliseconds and `sort`
  is the sort, so the two costs stay readable apart on the wire.

Inside each group the key decides and the name order breaks ties, so two equal sizes list
the same way every run, and `desc` is the exact reverse of ascending inside the group,
tie-break included. A size order lists directories by name, because a directory's `st_size`
is not a size anyone means; an mtime order lists them by time like everything else. The stat
is the same `lstat` that `rows` reports `s` and `m` from, so the order always agrees with the
column, symlinks included, and a row that vanished between the listing and the pass sorts as
the zeroes `rows` would send for it.

**Nothing is cached between requests.** Reversing a size order stats the directory again,
because a listing in name order, which is the order every listing starts in and the one the
field measures, must not carry 16 bytes a row it is not using. The stats live for the one
request: on the 100,000 file fixture the backend's PSS read 4781 kB after the listing and
4945 kB after a size sort (one `smaps_rollup` reading each), with a transient peak 3.2 MB
above that while the pass ran. Measured on that fixture, warm, on 2026-09-02, from the
`read` and `sort` fields of the `listed` line: the pass took 25 to 38 ms on twelve cores and
125 to 135 ms pinned to one with `taskset -c 0`, and the sort behind it 6 to 7 ms against
5 ms for name; on a three-row directory the whole request stayed under half a millisecond.
Cold is IO-bound, and the KB measured the same pass at about 1 s serial and 0.3 s on twelve
threads. The pass runs inside the loop, so a size or date sort of a very large directory on
a slow mount stalls every other request for its duration: it is the one request whose cost
scales with the whole directory rather than the viewport, and a client only ever sends it
for a click.

**Any other `by`, a missing one included, answers an `error` line naming the key it refused**,
never a `listed` line in name order: a client that
draws its sort mark from the key it sent would otherwise show "Kind" over a name-ordered
listing, and a descending sort would be silently undone under it. A refused `sort` changes
nothing; the listing keeps the order it had. `sort` never emits a `rows` line on its own;
follow it with `window` to see the reordered rows. A missing `desc` defaults to `false`.

The key names the stat field the order reads, not the column's label: the wire says `mtime`,
the same field `rows` carries as `m`, and a client that labels that column "Date Modified"
translates at its own edge and sends `mtime`.

### search

`{"c":"search","path":"<string>","query":"<string>","hidden":<bool>}`

Example: `{"c":"search","path":"/home/gm","query":"dwnhelp","hidden":false}`

Walks the whole subtree under `path` and streams every entry whose path relative to `path`
contains `query` as a case-insensitive subsequence, into a fresh listing that replaces the
current one. The match is fuzzy rather than a substring, and it runs over the whole relative
path rather than the base name, so `dwnhelp` finds `downloads/helper.txt`. `hidden` follows
`list`'s rule exactly: `false`, or missing, drops dot-prefixed names before they are
counted or descended, so a `.git` costs one `readdir` entry and nothing more.

`path` is whatever the client asks for, and the backend walks exactly that and nothing else:
a client searching a whole home directory sends home as the `path`, and one searching a mount
sends the mount. The backend has no notion of home and no scope of its own.

**Results are ranked, and the rank is what makes a subsequence match usable.** A subsequence
over a hundred thousand entries matches far too much to read, so every match carries a score
and the walk answers in score order. Characters matching consecutively score highest; a
character starting the candidate, a path segment, a word or a camelCase hump scores next; a
character in the file's own name beats one in a parent directory the query merely passed
through; and every candidate character skipped between two matches is charged back. Ties go
to the shorter path, then to the listing's own name order, so one walk over one tree always
answers in exactly one order.

**A search result is a listing like any other, and that is what makes it cheap.** Each match
is pushed as its path relative to `path`, and `path` becomes the listing's base, so
`window`, `thumb`, `dirsize` and every other per-row facility keep working with no special
case anywhere. The client splits the last `/` itself to draw the name and its location
column; the backend never sends a second shape.

Matches are appended in discovery order and never re-sorted mid-walk, so a `window` the
client already holds stays valid as the count grows. The order changes exactly once, at the
end: `searched` is written after the rows have been ranked, so a client re-reads its window
when it sees that line and every row index it held before then is stale. That is the same
rule a `sort` follows, for the same reason. The walk is answered in three parts:

1. One `listed` with `n` of 0, immediately, because the client's old rows are gone the
   moment the request is read.
2. A `searching` line carrying the growing count and the entries scanned so far, at most one
   every 100 ms while the walk runs.
3. One terminal `searched` line, written after the ranking.

There is no trailing `listed` or `searching`: `searched` carries the final count itself, so a walk that a
new listing replaced never announces a total it no longer has.

The walk runs in bounded slices inside the same single-threaded loop everything else uses,
four directories per slice, so a `searchcancel` or any other request is never queued behind
a subtree. A symlink reports its own type, so a link to a directory is listed but never
descended and no loop is possible. An unreadable directory is skipped in silence, the same
way `scan` skips an unreadable entry. A `path` that cannot be read at all is not an error:
the walk finishes at once with `n` and `scanned` both 0.

`list` and `sort` both end a running walk before they touch the listing, and answer their
own lines after the walk's terminal `searched`.

### searchcancel

`{"c":"searchcancel"}`

Stops the running walk and answers one `searched` line with `cancelled` true. Whatever the
walk already found stays in the listing, is ranked the same way a walk that ran to the end is,
and stays windowable. Unlike `thumbcancel` there is
no `rows` form: one walk runs at a time, so a cancel can only mean that one. A
`searchcancel` with no walk running does nothing and answers nothing.

### thumb

`{"c":"thumb","rows":[<uint>,...]}`

Example: `{"c":"thumb","rows":[2,17,140]}`

Asks for a thumbnail for those row indices of the current listing, and answers one
`thumbed` line per row. **This is the only thing that ever generates a thumbnail.** The
backend never walks a directory looking for work: a row that no client named is never
looked at, at any priority.

Each index is clamped to the listing: a row past the end is skipped in silence, because
there is no row to answer for. A missing or malformed `rows` is an empty array and the
request does nothing. Indices may repeat and need not be sorted; the newest request for a
row replaces the older one.

Four of the answers cost no work at all and come back on the same line the request
arrived on. A directory row, a row that is not a regular file, and a row whose MIME type no
validated thumbnailer declares all answer immediately with an empty `file`; that is the same
condition the `t` flag in a `rows` object reports, so a client that honours `t` never asks.
A row already in the shared cache at the file's current mtime answers immediately with that
entry's path, and a row recorded as failed at that mtime answers immediately with an empty
`file`. Only a genuine cache miss is queued, and its `thumbed` line arrives later.

The queue is bounded, and a request larger than it is still answered in full. When the queue
is full the oldest job in it is dropped to make room, and the row that job belonged to is
answered at once with an empty `file` rather than left waiting.

**What the shipped client sends.** `ui/Pane.qml` sends `thumb` only when the list settles,
which is a 120 ms timer restarted by every scroll and by every arriving window of rows, so a
fling issues nothing at all until it stops. One request names only rows currently visible,
only rows whose `rows` object carried `t:true`, and only rows the client's own map does not
already hold.

**That map has four states and only two of them are terminal.** Absent means unknown, `null`
means asked and waiting, a non-empty string is the answer's path, and an empty string is a
real answer meaning this row has no thumbnail. The two string states are terminal: every
answer is recorded, an empty `file` included, and an answered row is never asked about again
while this listing stands. The other two are not. Rows that left the viewport while still
waiting are named in a `thumbcancel` sent immediately before the same settle's `thumb`, and a
cancelled row drops back to absent rather than to a terminal state, because the backend
forgets the job it drops and would otherwise never answer it. **A row that scrolls out while
waiting and scrolls back is therefore asked for a second time, and that is correct.** A client
that treats "asked once" as terminal leaves a permanently empty slot on every such row.
`ui/js/Thumbs.js` is the shipped implementation, its `forget` is the drop back to absent, and
`tests/js/thumbs.js` holds it under the name "a cancelled row can be asked for again".

Neither request is ever sent with an empty `rows`, because that form cancels everything.
Opening a directory clears the client's whole map, since an index names a different file
afterwards. See AGENTS.md "Thumbnail requests in the GUI".

### thumbcancel

`{"c":"thumbcancel","rows":[<uint>,...]}`

Example: `{"c":"thumbcancel","rows":[17,140]}`

Drops queued jobs for those rows. An empty or missing `rows` cancels everything queued.
A job that a worker has already started cannot be cancelled and still answers with a
`thumbed` line; only work still in the queue is dropped. A cancelled row is not answered,
so a client that cancels must stop waiting for it. No response line.

A cancelled row can be asked for again straight away, in either form of the request: cancelling
forgets the row as well as its job, so a later `thumb` for it queues fresh work rather than
being deduplicated against the job that was just dropped.

### dirsize

`{"c":"dirsize","rows":[<uint>,...]}`

Example: `{"c":"dirsize","rows":[4,9]}`

Asks for the recursive apparent size of those row indices, and answers one `dirsized` line
per row that names a directory. **This is the only thing that ever walks a directory looking
for size**, the same rule `thumb` follows for thumbnails: a row no client named is never
walked. A row that is not a directory, or past the end of the listing, is skipped in silence.

Unlike `thumb`, there is no thread pool: the backend walks one directory at a time, inline in
the same event loop that answers every other request. Between each directory it rechecks for a
newer request (in particular `dirsizecancel`) before starting the next, so a walker never blocks
the loop for longer than one directory's own 2000&nbsp;ms deadline. A row already answered for
the current listing answers again at once from that cache; a row already queued costs nothing
extra.

**What the shipped client sends.** `ui/List.qml` sends `dirsize` only when the list settles, the
same 120&nbsp;ms timer `thumb` already waits on, so a fling issues nothing at all. One request
names only the directory rows currently visible and not already known.

### dirsizecancel

`{"c":"dirsizecancel"}`

Drops every directory row still queued to be walked. Unlike `thumbcancel`, there is no rows
form: the walker is one at a time, so a stale row left over from a scrolled-past viewport would
delay the row the new viewport actually wants, and the client always means "everything" when it
sends this. A row already answered is untouched; only the queue is cleared. No response line.

A directory a `dirsizecancel` dropped can be asked for again straight away: cancelling forgets
the row, so a later `dirsize` for it queues fresh work.

### transfer

`{"c":"transfer","op":"<string>","paths":["<string>",...],"dest":"<string>"}`

Example: `{"c":"transfer","op":"copy","paths":["/home/gm/a.txt","/home/gm/photos"],"dest":"/home/gm/backup"}`

Copies or moves each top-level path into `dest`. `op` of `"move"` moves; **anything else, including a
missing `op`, copies**, so a malformed request can never remove a source. `paths` are absolute; `dest`
is an absolute directory that must already exist, because Flea does not create a destination as a side
effect of a transfer. A `dest` that is missing, relative, or not a directory answers a single `error`
line with `where` of `transfer` and nothing is started.

Unlike `thumb` and `dirsize`, this names paths rather than row indices: a transfer outlives the listing
it was started from, and a row index would name a different file by the time it ran.

**A `rows` array of indices into the current listing may be sent instead of `paths`**, because a client
can only build a path for a row inside the window it holds, and a selection can be wider than that. The
indices are resolved against the listing at request time and the operation runs on the resulting paths,
so it still owns a snapshot that outlives whatever the listing does next. `paths` wins when both are
present, and an index past the end of the listing is dropped in silence.

**One of `transfer`, `trash` or `duplicate` runs at a time.** One of those arriving while another is
still running answers an `error` line saying so and touches nothing. The cap is one because the status
bar carries one transient slot for the running operation, so a second concurrent operation would have
nowhere to report. `rename` and `mkdir` never take that slot, and an `archive` or a `convert` is keyed by its own
`id` and runs alongside by design, so the cap was never one write of any kind.

The answer is a `transferstarted` line, then per top-level item a bounded stream of `transferprogress`
lines and exactly one `transferitem`, then one `transferdone`.

**Semantics that are decided here rather than left to the caller.** A same-filesystem move is a
`rename(2)`; a cross-filesystem move is a copy followed by removing the source, and the source is only
removed once the copy is complete, so a process killed mid-move leaves the source intact and a partial
file at the destination, never the reverse. A symlink is copied as a symlink and never followed. A
destination that already exists is refused for that item rather than overwritten, because every write
here creates its target exclusively. Directory recursion is invisible on this wire: the backend walks a
tree to copy it and the client sees only the top-level item's lines, so the wire's shape does not depend
on how deep a folder is.

### transfercancel

`{"c":"transfercancel","id":<uint>}`

Example: `{"c":"transfercancel","id":12}`

Cancels the running transfer if `id` names it, and does nothing otherwise, so a cancel aimed at an
operation that already finished can never reach the one after it. There is no response line of its own:
the running transfer answers with its own `transferdone` carrying `cancelled` true.

**The item in flight is stopped rather than allowed to finish, and what it had already written is
removed: a partial file by `copy_file`, a partly-copied directory by `copy_dir`.** A cancel that waited out a multi-gigabyte copy would not be a cancel, and a half-written file
at the destination is not a result anyone asked for. That item is reported `ok:false` with an `err` of
`cancelled`; every item not yet started is counted in `skipped`.

A `quit`, or stdin closing, cancels a running operation the same way and waits for its terminal line
before the process exits, so shutting down mid-copy also leaves nothing half-written behind.

### trash

`{"c":"trash","paths":["<string>",...],"rows":[<uint>,...]}`

Example: `{"c":"trash","paths":["/home/gm/old.txt"]}`
Example: `{"c":"trash","rows":[4,9]}`

`rows` is the same alternative to `paths` that `transfer` documents above, resolved the same way.

Moves each path to the freedesktop trash by running `gio trash`, and answers one `trashed` line.
**Nothing about the freedesktop trash specification is implemented in this codebase**, only an argv and
a result: `gio` already handles the same-filesystem-move-versus-copy question, the `.trashinfo`
metadata, and the per-mount `.Trash-$uid` fallback for a volume with no home-relative trash.

Which paths actually went is read off the filesystem afterwards rather than from `gio`'s exit status,
which covers the whole batch and cannot attribute a failure to one path.

**The trash URI of each item is captured at trash time, and this is a measured requirement rather than a
preference.** `gio trash --restore` refuses an original path outright (`Location given doesn't start
with trash:///`), and two files trashed from the same path both list that same original, so recovering
the URI later is ambiguous. The backend therefore reads `gio trash --list` immediately before and after
the call and keeps the entries that are new, which is what makes the operation reversible.

There is no confirmation step anywhere in this request, because the undo journal is the safety.

### rename

`{"c":"rename","path":"<string>","to":"<string>"}`

Example: `{"c":"rename","path":"/home/gm/old.txt","to":"new.txt"}`

Renames one file within its own directory and answers one `renamed` line. `to` is a bare name, not a
path: an empty `to`, `.`, `..`, or anything containing `/` or an interior NUL is refused with an `error`
line before any syscall runs, because a separator would move the file out of its own directory.

**The rename refuses to overwrite.** It runs `renameat2` with `RENAME_NOREPLACE` rather than
`rename(2)`, which on Unix silently replaces the target; for a file manager that is unrecoverable data
loss, and the check-then-rename alternative leaves a window in which another process can create the
target. Renaming a file to the name it already has is not an error and is not work: it answers `ok` and
records nothing to undo.

Unlike the three above, this answers on the loop's own thread: it is a single syscall, so spawning a
thread would cost more than the work.

### duplicate

`{"c":"duplicate","path":"<string>"}`

Example: `{"c":"duplicate","path":"/home/gm/photo.jpg"}`

Copies one path to a free sibling name and answers one `duplicated` line. The name walks
`"<stem> copy<ext>"`, `"<stem> copy 2<ext>"` and so on until one is free; `Path`'s own stem and
extension split the last dot only, so `backup.tar.zst` becomes `backup.tar copy.zst` and a dotfile keeps
its whole name as the stem. A directory row duplicates its whole tree.

### mkdir

`{"c":"mkdir","path":"<string>","name":"<string>"}`

Example: `{"c":"mkdir","path":"/home/gm","name":"Invoices"}`
Example: `{"c":"mkdir","path":"/home/gm"}`

Makes one empty directory inside `path` and answers one `made` line. `path` is the absolute parent: a
relative or missing `path` is refused before any syscall, because a bare name would otherwise land in
the backend's own working directory, which no listing ever names. `name` is a bare name under the rule
`rename` applies to `to`: `.`, `..`, anything containing `/` or an interior NUL is refused with an
`error` line whose `where` is `mkdir`. A name of only spaces is a legal name and is created as sent,
as it would be by `rename`; trimming is the field's job.

**A missing or empty `name` asks for the first free default name**, `New Folder`, then `New Folder 2`
and up, the same way `duplicate` picks ` copy N` itself rather than leaving it to the client: a client
holds a window of the listing and not the directory, so it cannot know which names are taken, and a
name taken by anything, a file included, is stepped past. That is the form an inline
create-then-rename flow sends; the `made` line then carries the name that was chosen. A typed name is
never renumbered.

**The directory is created with `mkdir(2)` and never merged into one already there.** A name already
taken by anything, file, directory or symlink, answers `a folder or file with that name already
exists` and touches nothing, the same rule `archivework.rs` states for its own staging directories.
Every other failure carries the OS's own sentence as `msg`: a parent that vanished since the listing
answers `No such file or directory (os error 2)`, a parent the user cannot write `Permission denied
(os error 13)`, a read-only mount `Read-only file system (os error 30)`, a name past `NAME_MAX` `File
name too long (os error 36)`.

Like `rename`, this answers on the loop's own thread and never takes the one-operation slot.

### meta

`{"c":"meta","row":<uint>,"text":<bool>,"media":<bool>,"archive":<bool>}`

Example: `{"c":"meta","row":4,"text":false,"media":false,"archive":true}`

The per-row extras the preview column names and a listing row does not carry, for **one row that a
client actually asked for**, never a sweep. `row` indexes the current listing, and a `row` outside it
is dropped in silence, so a client that changed directory has to re-ask once the new `rows` arrive.
Answers one `meta` line on a thread, because a media row costs an `ffprobe` and the loop waits on it.

The three booleans are the client's own hints, taken from the classification it already made: `text`
asks for a line count, `media` for a duration and sample rate, `archive` for the index. Each costs
something, so none is inferred here. A file whose header parses as an image is never counted for
lines whatever `text` says, because the newlines in a bitmap are a number nothing should be shown.

### undo

`{"c":"undo"}`

Reverses the most recent completed operation and answers one `undone` line naming which kind it was.
An empty journal, or a reversal that itself fails, answers an `error` line with `where` of `undo`.

**The journal is an in-memory ring of the last 50 completed operations and is not persisted**, so it
does not survive a restart. Each kind reverses as follows: a rename or a move renames back (still
refusing to clobber, because something may occupy the old name by now), a copy or a duplicate removes
what that operation created, and a trash restores through `gio trash --restore` using the URI captured
when it was trashed. A `mkdir` removes the folder it made only while it is still empty: a folder the
user has filled since is theirs, so that reversal answers an `error` line, leaves it and its contents in
place, and is spent like any failed reversal, so the next `undo` reaches the operation before it.

**Only what an operation actually created or moved is recorded**, never a path it merely read, so an
undo can never delete a file the operation did not put there. An operation that changed nothing records
nothing, and a cancelled batch records only the items that succeeded. An item that failed short of a
cancel, and had already created its destination, records that partial destination too: the failure
leaves it on disk, because removing it on a transient error would destroy data, and the journal is what
lets `undo` remove it. A destination that already existed is never recorded, because nothing was created
there. The steps of one operation reverse
newest first, and a step that fails stops the rest rather than leaving the operation half-reversed with
nothing recording which half.

### quit

`{"c":"quit"}`

Stops the read loop, then answers the thumbnail work already asked for: every queued and
running job is drained and reported before the process exits, because a worker inside a
child owns a temp file in the shared cache that only its own return publishes or removes.
Draining is bounded; see `thumbed` below. Apart from those `thumbed` lines, no response.

**A queued `dirsize` row is not drained.** It answers nothing that outlives this process, unlike
a thumbnail's shared on-disk cache, so a row still waiting when `quit` arrives is simply dropped;
the client that asked for it is going away too.

## Responses

### listed

`{"t":"listed","n":<uint>,"read":<float>,"sort":<float>,"v":<uint>}`

Example: `{"t":"listed","n":100000,"read":26.400,"sort":2.500,"v":56}`

`n` is the row count. `read` and `sort` are milliseconds, formatted to three decimal
places (`{:.3}`). Sent after a successful `list` and after a successful `sort`. `read` is
the scan after `list`, the metadata pass after a `sort` by `size` or `mtime`, and `0.0`
after a `sort` by `name`, which reads nothing.

`v` is the listing directory's own filesystem id, sent once here rather than on every row because
every file in the directory shares it. A client compares it against the `v` of the directory row
being dropped on: equal is one volume and the drag moves, different is two and it copies. `v` is 0
when the directory could not be stat'd, and a client reads 0 as unknown and copies, because copying
where a move was meant is an annoyance and moving where a copy was meant loses the original.

### rows

`{"t":"rows","start":<uint>,"rows":[{"n":<string>,"d":<bool>,"s":<uint>,"m":<int>,"p":<uint>,"i":<string>,"t":<bool>,"k":<uint>[,"v":<uint>]},...],"kinds":[<string>,...],"ms":<float>}`

Example:
`{"t":"rows","start":0,"rows":[{"n":"say \"hi\".txt","d":false,"s":12,"m":1787790423,"p":33188,"i":"text-x-generic","t":false,"k":0},{"n":"photos","d":true,"s":4096,"m":1787790424,"p":16877,"i":"folder","t":false,"k":1,"v":56}],"kinds":["Plain text document","Folder"],"ms":1.250}`

`start` echoes the requested start, clamped to the listing's length: a `window`
whose `start` lands past the end of the listing answers with `start` equal to the
listing's length and an empty `rows` array, not the value that was requested.

Each row object holds a name (`n`, JSON-escaped; a valid UTF-8 name may carry any
character including a quote or a newline, but a non-UTF8 name arrives lossy and then
cannot be stat'd, so it reports zeroes, see AGENTS.md "Deliberate corners"), a
directory flag (`d`, the link's own type rather than its target's: a symlink to a
directory reports `d:false` and carries `S_IFLNK` in `p`), a size in bytes (`s`), an
mtime as a unix timestamp (`m`), a raw `st_mode` (`p`), a freedesktop icon name
(`i`), a thumbnailable flag (`t`), and a Kind index (`k`, into the envelope's own
`kinds` array).

**`v` is the row's filesystem id, and only a directory row carries it.** A drop destination is
always a directory, so a file row's device would never be read, and the scale fixture is 100,000
files with no directories at all: sending it per row would be paid 100,000 times on the one path
that produces the headline listing number, for a field nothing reads. A client compares the `v` of
the rows being dragged against the `v` of the directory being dropped on: equal means one volume and
the drag moves, different means two and it copies, which is what Finder does. A directory whose stat
failed reports `p` 0 and `v` 0 with it. `ms` is the phase-2 stat time for this window, formatted to three
decimal places. Sent after `list` and after `window`.

A row whose stat failed sends `s`, `m` and `p` all 0, and `p` is what says so: 0 is outside
`st_mode`'s domain, because a real one always carries its file-type bits. So `p` 0 needs no flag
beside it the way `lines` 0 needs `lfailed`, and it condemns that row's `s` and `m` with it: those
two are only readable when `p` is non-zero.

**`kinds` is a dictionary, not a string per row, because an earlier plan measured the
per-row-string shape and it cost milliseconds.** A directory has few distinct Kind
strings even at 100,000 files (a handful of file types, plus `"Folder"`), so every row
paying one integer instead of a repeated string keeps the response small without a
second round trip. A directory's Kind is always the literal `"Folder"`; a file's Kind
is `src/backend/kind.rs`'s freedesktop `<comment>` for its resolved MIME type,
canonicalised through `/usr/share/mime/aliases` the same way the thumbnailer spec
lookup already is, falling back to the literal `"Data"` both for a type with no comment
at all (`application/x-ms-dos-executable` is the one shipped example) and for a name no
glob matched. **An icon name is an internal string and this column is a human
description, so one never leaks into the other**, and a name nothing identified must not
claim to be text however far the icon ladder itself falls. Like the icon and
MIME tables, the comment cache is read once per distinct type for the life of the
backend process, never per row: the client is given a human description and never the
MIME type, because a display should not classify.

**The parser rule is the first `<comment>` carrying no `xml:lang` attribute, not the first
`<comment>` of any kind, and this was measured rather than assumed.** 8 of the first 200
`/usr/share/mime/*.xml` files on this box open with a translated comment before their
untranslated one; `application/msword.xml` is one of them, opening `xml:lang="zh-Hant-TW"`
with its untranslated comment further down the file, after every alias and glob line. A type
whose file carries no untranslated comment at all, or no comment element of any language,
answers `None`, which is why `Kinds::comment` returns `Option<String>` rather than an empty
string.

**No wording in that file is a property of the format, and nothing may assert one.** Every
`/usr/share/mime/<type>.xml` is generated by `update-mime-database` from the files in
`/usr/share/mime/packages/`, and no package owns the generated file, so the untranslated
string is whatever the applications installed on the box supply. `application/msword` reads
`Word document` from `shared-mime-info`'s own `freedesktop.org.xml` and `Microsoft Word
Document` from `libreoffice-fresh`'s `libreoffice.xml`, which outranks it; a box with
LibreOffice and a box without therefore disagree on the same distribution and the same
`shared-mime-info` version. Issue 1 was a test asserting the second string. The parser is
tested against a fixture and the live database is asserted only for shape: reachable, ASCII,
and free of markup.

Note that `t` inside a row object is a boolean, while the `t` at the top of every
response line is the message type, a string. They never collide: one is a member of a
row, the other a member of the envelope.

`i` is never empty. The backend resolves the row's name to a MIME type against
`/usr/share/mime/globs2`, then that type to an icon name against
`/usr/share/mime/generic-icons`, falling back to the media class (`image-x-generic`
and friends) and finally to `text-x-generic`; a directory is always `folder`. Both
tables are read once per backend process, never per row. The client is given the icon
name and never the MIME type, because a display should not classify.

Two rules sit above that resolution and neither follows from the name alone. **The icon of a
symlink follows the link's target while `d` still describes the link**: a symlink to a
directory reports `d:false`, because `d` is what the client navigates on, and `i` is `folder`,
because that is what the row looks like. Only symlink rows pay for it, through one extra stat
inside the window the client asked for. **An `application/*` type with no `generic-icons` entry
resolves by the row's own execute bit**, not by its class: 190 of the 613 such types in
`globs2` have no entry on this box, so the execute bits in `p` decide between
`application-x-executable` and `application-x-generic`. Both counts move with the installed
applications, the same merge described above: a root built from `freedesktop.org.xml` alone
gives 138 of 552. Without that rule every one of the 190 drew as a program, which is
how a `.pem` came to look executable.

`t` is true only when a validated thumbnailer declares the row's MIME type or an alias
of it. The backend resolves the row's name to a MIME type the same way it resolves the
icon, then looks that type up in the thumbnailer specs read from the freedesktop search
path, whose declarations and queries are both canonicalised through
`/usr/share/mime/aliases`, so either side of an alias pair matches. A spec whose
program is not a runnable executable is dropped when the table is built, so `t` is
false for a declared type with no working thumbnailer. `t` also reads the file type out of the
row's own `p`: a directory, a fifo, a socket and a device node are always false however they are
named, and a row that vanished between the listing and the stat reports `p` of 0 and is false
too. A symlink is true when its name resolves to a declared type, because the thumbnail request
stats the target; a symlink to a special file is then refused when the row is actually asked
for. Like
the icon tables, the alias and spec tables are read once per backend process, never per
row; the whole per-row cost of `t` is one hash lookup, tens of nanoseconds per row, so
it is invisible at a viewport of a few hundred rows. The client is still never given the
MIME type itself: `t` exists so a display does not have to reason about content types to
know whether asking for a thumbnail is worthwhile.

### searching

`{"t":"searching","n":<uint>,"scanned":<uint>,"ms":<float>}`

Example: `{"t":"searching","n":812,"scanned":41200,"ms":300.114}`

The streaming progress of a `search`, at most one every 100 ms while the walk runs. `n` is
the match count so far and is what the client sets its row total to; `scanned` is directory
entries looked at, which is the number the status bar counts up. It is deliberately not a
`listed` line: a mid-walk update is not a fresh listing and carries no `read` or `sort`
timing to report.

Matches are only ever appended, so a `window` the client already holds stays valid across
every one of these.

### searched

`{"t":"searched","n":<uint>,"scanned":<uint>,"ms":<float>,"cancelled":<bool>}`

Example: `{"t":"searched","n":6252,"scanned":45807,"ms":48.711,"cancelled":true}`

The terminal line of a `search`. `n` is the final match count and is authoritative: no
`listed` follows it. `scanned` is how many directory entries the walk looked at, which is
what the status bar counts up while it runs. `ms` is the whole walk, to three decimal
places. `cancelled` is true when a `searchcancel`, a `list` or a `sort` ended the walk early,
and false when it ran the subtree out. The rows are in rank order by the time this line is
written, so a client holding a window from before it has stale indices and re-reads.

### thumbed

`{"t":"thumbed","row":<uint>,"file":"<string>","ms":<float>}`

Example: `{"t":"thumbed","row":2,"file":"/home/gm/.cache/thumbnails/large/b98fa4082faa4a9cbb9728c94831a4b5.png","ms":75.823}`

Answers one row of one `thumb` request. `row` is the index that was asked for. `file` is
the absolute path of the PNG in the shared thumbnail cache, JSON-escaped like every other
string on this wire, and `ms` is milliseconds to three decimal places: the whole job for a
generated row, or just the lookup for a row answered from the cache, which is tens of
microseconds here against tens of milliseconds and up for a decode.

**`file` is empty rather than absent on failure**, so a client never waits forever for a
row that will not arrive. Every reason a row cannot be thumbnailed reads the same on the
wire: a directory, a file that is not a regular file, no thumbnailer for the type, a recorded
failure, a thumbnailer that failed, hung, or exited successfully having written nothing, or a
shutdown that cut the job short. A client that asked for a row
therefore gets exactly one `thumbed` line for it, unless it cancelled the row itself or
replaced the listing under it.

**A result for a superseded listing is dropped, never reported against the current one.**
The backend maps only the rows a client actually asked for back to their paths, and a
`list` or a `sort` clears that map and cancels the queue, because both change which row an
index names. A job already inside a worker still runs to its own end, and its result is
discarded on arrival rather than printed against a row it no longer describes.

Shutdown is bounded at 25 seconds in total, which is longer than the pool's own 20-second
job deadline, so no single running job can be cut short by it. If the budget does run out,
every row still unanswered is answered with an empty `file` before the process exits.

### dirsized

`{"t":"dirsized","row":<uint>,"bytes":<uint>,"partial":<bool>,"ms":<float>}`

Example: `{"t":"dirsized","row":4,"bytes":1048576,"partial":false,"ms":12.500}`

Answers one row of one `dirsize` request. `row` is the index that was asked for, `bytes` is the
recursive apparent size (every entry's own `st_size`, symlinks not followed and counted at their
own small size rather than their target's), and `ms` is the walk's own wall-clock time. The
target's own directory entry counts too, matching what `du -s` reports for the directory itself.

**`partial` is true when the 2000&nbsp;ms deadline cut the walk short, or a subtree inside it
answered permission denied.** Either way `bytes` is a floor, honestly labelled, never a wrong
exact number: everything the walk actually saw before it had to stop is still counted. The
shipped client renders a partial answer with a leading `>`.

**A result for a superseded listing is dropped, never reported against the current one.** A
`list` or a `sort` clears the answered-row cache and cancels the queue, the same rule and the
same reason `thumbed` follows: both change which row an index names.

### transferstarted

`{"t":"transferstarted","id":<uint>,"n":<uint>,"moving":<bool>}`

Example: `{"t":"transferstarted","id":12,"n":2,"moving":true}`

`id` names this operation for the life of the process and is what `transfercancel` takes. `n` is the
count of top-level paths, **never a recursive file count**: computing that before answering would be a
metadata sweep over an unbounded subtree, which is the shape this codebase refuses everywhere else, and
a transfer's first line is on the critical path of every operation.

`moving` is the verb the request actually resolved to, after the rule that anything which is not
exactly `"move"` copies. It is on the wire because the client cannot derive it: a paste spends a cut
clipboard before this line arrives, and a move to Dropbox never touches the clipboard at all, so a
client reading its own clipboard reports a move as a copy.

### meta

`{"t":"meta","row":<uint>,"w":<uint>,"h":<uint>,"ms":<uint>,"rate":<uint>,"entries":<uint>,"unpacked":<uint>,"afailed":<bool>,"names":[{"n":"<string>","d":<bool>},...],"lines":<uint>,"partial":<bool>,"lfailed":<bool>,"target":"<string>","targetdir":<bool>,"owner":"<string>"}`

Example: `{"t":"meta","row":4,"w":0,"h":0,"ms":0,"rate":0,"entries":214,"unpacked":3400,"afailed":false,"names":[{"n":"ui","d":true}],"lines":0,"partial":false,"lfailed":false,"target":"","targetdir":false,"owner":"gm"}`

Every field a row did not ask for is zero, false or empty. `w` and `h` are pixels, read from the
file's own header; `ms` and `rate` come from the probe; `lines` is a newline count with `partial`
true when it stopped at its 1 MiB budget, so the column states it as a floor.

`entries` and `unpacked` are **exact totals whenever they are non-zero**, because the index is
streamed and counting all of it costs no memory. `names` is capped at the first `ARCHIVE_NAME_CAP`
entries, which is the only part that is bounded: the tile lists those and states the difference as
its own "+ N more" line.

`lfailed` is true when the row produced no count at all, which on this box
means permission denied, a row that is not a regular file, or a row that vanished between the
listing and the request. **Nothing but a regular file is ever opened here or for `w` and `h`**, because
opening a FIFO with no writer never returns: a FIFO, a socket, a device or a directory answers its
`stat` facts with no dimensions and no count, rather than leaving the row waiting forever. It is
what tells `lines` 0 apart from an empty file, whose `lines` is also 0: zero is a real count, so
unlike `mode` on an `error` line it cannot carry the failure itself. A row that never asked for a
count sends `lines` 0 and `lfailed` false, the same as a row whose count really is zero, because
nothing was attempted; the client knows which kind it asked about.

`afailed` is true when the listing was not completed, which covers a tool that could not read the
archive and a read that outran its wall-clock budget. **A failed read sends `entries` 0, `unpacked` 0
and an empty `names`**, so a partial count never reaches a client that has been told the number is a
total. That is a different answer from an archive holding nothing, which sends the same zeroes with
`afailed` false, and the two are drawn as the error state and the empty state respectively.

`target` is a symlink's own target and `targetdir` says whether that target is a directory, so the
column's mark can follow the target the way a listing row's does.

`owner` is the login name of the user who owns the path itself (a symlink reports the link's owner, as
its row does), resolved from `/etc/passwd` alone and never through `getpwuid`: that call goes through
NSS and can wait on a network directory, and the meta thread must never hang the column on one. A uid
no local account carries answers the empty string, never the number dressed as a name.

### transferprogress

`{"t":"transferprogress","id":<uint>,"index":<uint>,"name":"<string>","bytes":<uint>,"total":<uint>}`

Example: `{"t":"transferprogress","id":12,"index":0,"name":"a.txt","bytes":40000000,"total":120000000}`

Throttled to at most one line per 150 ms per item, so a fast copy of a small file may emit none at all
before its terminal line; that is correct and not a missing message.

**Only a regular file reports bytes.** A directory has no total without the sweep this codebase does not
do, and a same-filesystem move is a single `rename(2)` with nothing to report partway through, so
neither emits these lines at all. A client renders an item with no progress as indeterminate.

### transferitem

`{"t":"transferitem","id":<uint>,"index":<uint>,"name":"<string>","ok":<bool>,"err":"<string>"}`

Example: `{"t":"transferitem","id":12,"index":0,"name":"a.txt","ok":true}`
Example: `{"t":"transferitem","id":12,"index":1,"name":"photos","ok":false,"err":"permission denied"}`

Exactly one per top-level item, in the order the items were named. **`err` rides only on a failure**, so
a successful item's line carries no empty field to reason about, and a permission error on one file is
that item's data rather than the operation's: the batch carries on to the next item. What a failed item
had already written stays where it is and is journaled as that operation's own creation, so an `undo`
removes it; only a cancel removes its partial itself, see `transfercancel`.

### transferdone

`{"t":"transferdone","id":<uint>,"ok":<uint>,"failed":<uint>,"skipped":<uint>,"cancelled":<bool>}`

Example: `{"t":"transferdone","id":12,"ok":1,"failed":1,"skipped":0,"cancelled":false}`

The whole operation's terminal line. `skipped` counts items a cancel reached before they started;
`cancelled` is true when a `transfercancel`, a `quit` or stdin closing ended it early.

### trashed

`{"t":"trashed","ok":<uint>,"failed":<uint>}`

Example: `{"t":"trashed","ok":1,"failed":0}`

Counts only. Unlike `transferitem` there is no per-path error text, because trash is one `gio` call for
the batch and its exit status cannot attribute a failure to a single path; a path that is still on disk
afterwards is counted in `failed`.

### renamed

`{"t":"renamed","ok":<bool>,"path":"<string>"}`

Example: `{"t":"renamed","ok":true,"path":"/home/gm/new.txt"}`

`path` is the full path the file now has. A refusal is an `error` line instead, never this line with
`ok` false.

### duplicated

`{"t":"duplicated","ok":<bool>,"path":"<string>"}`

Example: `{"t":"duplicated","ok":true,"path":"/home/gm/photo copy.jpg"}`

`path` is the sibling that was written. A failure is an `error` line with `where` of `duplicate`, and a
copy that failed partway leaves its partial sibling in place and journals it, so an `undo` removes it.

### made

`{"t":"made","ok":<bool>,"path":"<string>"}`

Example: `{"t":"made","ok":true,"path":"/home/gm/New Folder"}`

`path` is the full path of the directory that now exists, the chosen default included when the request
sent no `name`, which is what lets a client re-list with that row under the cursor and open its rename
field on it. A refusal is an `error` line with `where` of `mkdir` instead, never this line with `ok`
false.

### undone

`{"t":"undone","op":"<string>","ok":<bool>}`

Example: `{"t":"undone","op":"move","ok":true}`

`op` is the kind of operation that was reversed, one of `rename`, `duplicate`, `mkdir`, `trash`, `copy`
or `move`, which is what lets the status bar say what it just put back.

### error

`{"t":"error","where":"<string>","path":"<string>","msg":"<string>"[,"mode":<uint>]}`

Example: `{"t":"error","where":"scan","path":"/root","msg":"permission denied","mode":16872}`

`where` names the failing operation, `path` names the input that failed, `msg` is the
underlying message. Over `--backend`'s stdout, `where` is `scan` (a `list` whose path
failed to read), `sort` (a `sort` whose key names no order this wire defines; `path`
carries the key as sent), or `read` (the
stdin stream itself could not be decoded; the loop stops right after emitting this
line, because the framing cannot be trusted past that point; thumbnail work already
running is still drained after it, so a `thumbed` line can follow). `error_line`
is also how `flea --prewarm` reports a failure, to stderr rather than over this wire
protocol. `where` names whichever operation actually failed: a missing or unreadable
`path` propagates `scan`'s error unchanged, so `where` reads `scan` there too; only a
failure inside prewarm's own file handling (creating, writing or renaming the temp
file) reports `where":"prewarm"`. An `error` line is sent instead of `listed` or
`rows`; it never changes the backend process's exit code.

`mode` is the `st_mode` of the path a `scan` could not read, in decimal, and it is written only when
the backend actually read it. It is a directory's mode in the case the field exists for; a `list` of
a regular file fails with `ENOTDIR` and carries that file's mode instead, which no client draws,
because only a denial reaches the Locked state. The stat outlives the denial: `/root` answers 16872, which is
`0o40750`, while `opendir` on it is refused. No other `where` ever carries the field, and a `scan`
whose path could not be stat'd either leaves it out, which a client reads as "I could not look" and
draws as a sentence with no permission string under it. It exists because a typed path, or
`FLEA_PATH`, reaches the denial with no parent listing to remember the mode from.

## Prewarm reuses this wire format

`flea --prewarm <path> <first> <dest>` writes exactly a `listed` line then a `rows`
line, the same two lines `--backend` would print for an equivalent `list`, to `dest`
instead of stdout. See `AGENTS.md`, "Prewarm".

`--prewarm` exits 0 if and only if `dest` holds a complete listing of `path`; on
failure it writes an error line to stderr, exits non-zero, leaves no temp file, and
does not touch `dest`, so a caller must check the exit status before reading the
file.

## Undocumented requests

`peek`, `paths`, `archive`, `convert`, `formats` and `fsinfo` are on the wire and are not documented
here yet. `tools/flea-acceptance` derives its checklist from this file, so each one is a gap in that
battery until its section is written.

One `peek` field is worth naming ahead of that section, because it is new. A `peeked` line whose scan
failed carries `"failed":true`, with `"mode":<uint>` beside it under the same rule the `error` line
uses, so a column can tell an unreadable directory from an empty one instead of drawing both as the
empty state. A `peeked` line that succeeded carries neither field, whatever `n` is.

Two more `peek` fields are worth naming for the same reason. Every `peeked` line, failed or not,
echoes the `hidden` its request carried, because two clients read this wire at once: `ui/ColumnsArea.qml`
peeks the pane's ancestors with the listing's own flag, and the path bar's Tab peeks with whatever
the typed leaf asks for. `path` alone cannot tell one client's reply from the other's, and `path`
plus `hidden` can, which is all the correlation either needs: the same pair answers the same rows,
so no request id has to be threaded through. A client that ignores the field reads the line exactly
as it did before.

## Known gaps

- `listed` and the two-line prewarm file carry no requested path. A stale prewarm file
  cannot be matched to the requested directory, so production ignores `FLEA_PREWARM`.
  Add a path or correlation field and prove a first-paint win before re-enabling a
  reader.
- The pool is built when the backend starts, not when the first `thumb` arrives. That
  costs every backend process about 1.2 ms of startup and about 0.5 MB of PSS for a
  subsystem a client may never use; see AGENTS.md "Thumbnail requests".
- `thumbcancel` names rows, and only rows this process actually queued can be cancelled.
  Cancelling a row that was answered from the cache, or that no thumbnailer declares, is
  silently a no-op, because there was never a job to drop.
- `t` is true for a symlink whose name resolves to a declared type, whatever the link points at.
  The flag reads the file type out of the row's own `p`, which for a symlink is `S_IFLNK` and says
  nothing about the target, so a symlink to a directory, to a FIFO, to a loop, or to nothing at
  all still reports `t:true`. Every one of them is answered with an empty `file` in microseconds
  when it is actually asked for, because the request path stats the target, so nothing blocks and
  no marker is written. It is an over-promise on the wire rather than a hazard, and a client that
  renders per-row state from `t` alone will show a thumbnail slot that never fills. The shipped
  GUI renders defensively instead of the wire narrowing: an empty `file` is a terminal answer in
  its row map and the delegate falls back to the themed icon, so such a row simply keeps the icon
  it already drew. Narrowing the flag stays undone on purpose, because it is a wire change with
  its own documentation and test surface and half-narrowing it is worse than leaving it whole.
- A non-UTF8 filename arrives lossy, and the thumbnail path inherits that. Two names in one
  directory differing only in their invalid bytes collapse to the same string, so the backend
  maps one path where the client named two rows and the second row's `thumbed` line arrives
  only at the next `list`, `sort` or shutdown. See AGENTS.md "Deliberate corners".
- The `file` a `thumbed` line names is read from the shared cache, which every application on
  the box writes, and nothing sandboxes the client that decodes it. Generation runs under
  `bwrap`; display does not. The shipped GUI does hand this path straight to a QML `Image`, so
  Qt's PNG decoder runs unconfined in the shell process on bytes it did not write. That is a
  known residual risk, parked for a later plan rather than closed here. See AGENTS.md
  "Deliberate corners".

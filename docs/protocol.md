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
3. Exactly one line arrives unasked: `changed`, which says the listed directory was
   altered by another program. It can land between any request and its reply, so a
   client that counts lines rather than reading their `t` field will misread the wire.
4. A row index means a file only in the numbering it was read from. Every `rows` line
   names that numbering as `listing`, and a `trash`, `transfer`, `link`, `paths`, `collisions`, `settags`
   or `menuaction` that names an older one is refused before anything resolves; see "listing" below.

## Requests

### listing

`"listing":<uint>` on a request that carries `rows`

Example: `{"c":"trash","rows":[0],"menuId":0,"listing":7}`

The backend numbers its rows, and the number moves whenever what an index names changes: a
successful `list` or `listpaths`, a `search`, an accepted `sort`, and a walk's ranking. Every `rows`
line carries the numbering it was written in as its last field, `"listing":<uint>`, and the first
listing a backend makes is 1. A client names the numbering of the rows it read an index from on any
request that carries `rows`. A `trash`, `transfer`, `link`, `paths`, `collisions`, `settags` or `menuaction` whose `listing` is not
the numbering in force is refused before a single index is resolved: `trash`, `transfer`, `link`, `paths`,
`collisions` and `settags` answer `{"t":"error","where":"stale","path":"<the command>","msg":"..."}` and nothing else, and a
`menuaction` answers its own reply with `"ok":false` and an `error` sentence. Nothing was done in
either case, so a client that wants the action reads the new rows and asks again.

A request that names no `listing` is resolved against the listing in force, exactly as before the
field existed, so an older client and a hand-written line keep working. `thumb`, `thumbcancel`,
`dirsize`, `meta`, `tagstates` and `window` read and never act on a file, so they are not refused; a client drops
their answers for a listing it has left. The race this closes is a request written after a `list`
and before its `rows` reached the client: the backend has the new listing by then, and without the
field it resolved the old index against it, so `list A`, `list B`, `paths [0]` answered B's first file.

### list

`{"c":"list","path":"<string>","first":<uint>,"hidden":<bool>}`

Example: `{"c":"list","path":"/home/gm","first":350,"hidden":false}`

Scans `path` (phase 1: names and the directory bit only), applies ordering, and
answers a `listed` line followed immediately by a `rows` line
covering rows `0..first`, or the window `select` below moves onto its row. A `path` that fails to scan answers a single `error` line
instead and leaves the previously listed directory in place: a failed `list` cannot
mix a new `path` with the old listing. A missing `path` defaults to the empty string,
a missing `first` defaults to `0`.

Optional `by`, `desc`, `foldersFirst`, and `groupByKind` fields use the ordering
rules described under `sort`. A list with no ordering fields uses name ascending,
directories first, without kind groups. Unlike an explicit `sort`, a `list` may
omit `by`. An invalid ordering key refuses the new listing and keeps the previous
directory and rows.

Optional `hiddenLast`, `false` unless `true`: with it on, dotfiles form an outer
partition, a visible block then a hidden block, for each of the four keys in both
directions, and `foldersFirst` or `groupByKind` orders inside each block, so visible
folders come first, then visible files, then hidden folders, then hidden files. Off
leaves dotfiles to follow the sort key like any other name. Absent is off, so an
older client keeps it.

`hidden` of `false`, or a missing `hidden`, drops every name starting with `.` before
it ever reaches the listing: the filter runs inside the scan itself, not as a later
pass over it, so a hidden directory costs nothing beyond the `readdir` entry it was
always going to read. `hidden` of `true` scans dotfiles in too, ordered the same as
everything else. There is no separate flag to ask for dotfiles without also
re-scanning: `sort` reorders whichever listing `list` last produced and cannot add or
remove rows, so changing `hidden` always means a fresh `list`, which is also what
clears the cursor and selection back to row 0.

Optional `wantChanged`, `false` unless `true`: with it on, a `list` that re-reads the
directory already listed names added plus removed rows in `changed`. Absent is off, so
every navigation and watched re-read stays silent and pays no count.

Optional `select`, a full path such as `"/home/gm/notes.txt"`: when it names a row of the new
listing at index `first` or past it, the `rows` line covers the `first` rows that hold that row
near their middle, starting at `index - first / 2` and clamped so the window ends at the last
row, instead of `0..first`; its `start` says where. A row inside `0..first`, a name the listing
does not hold, a path in another directory or an absent `select` answers `0..first` as before.
The lookup reads only the listing's name arena, matching names exactly the way `locate` does,
and stats nothing beyond the window it answers. The client asks for the directory with the row
it will put the cursor on, so a row sorted far down lands in the listing's first rows reply.

### locate

`{"c":"locate","path":"/directory/selected.txt"}`

Returns `{"t":"located","directory":"/directory","path":"/directory/selected.txt","index":42}`
for a name already in the current listing, or `index:-1` when absent or invalid. This reads only
the listing's name arena; it does not stat files, resolve symlinks or scan another directory.
The client must match both `directory` and the requested `path` before using the index, then
fetch only the viewport containing that row. This is location in the current listing, not proof
that the filesystem object still exists or has the same identity for a later operation.

`{"c":"locate","paths":["/directory/selected.txt"],"id":7,"menuId":4}` resolves a batch
in one scan of the name arena. The reply carries `directory`, the request `id`, `ok`, `error`,
and `matches:[{"path":"/directory/selected.txt","index":42}]`. Missing paths are omitted,
duplicates produce one match, and results follow listing order. No unrequested path is returned.
Without `menuId`, this has the same name-only semantics as the single-path form. With `menuId`,
each matched survivor must still have the device, inode and file type captured by that completed
permanent deletion. This explicitly requested survivor check stats only matched paths.
The backend retains one completed deletion's identities for this read-only purpose even after
`menuaction/close` has expired the mutable selection. An expired restoration identity returns
`ok:false` and no matches. Clients must also reject replies after navigation, selection changes,
or replacement of the pending request, and fetch metadata only for the visible window.

### menuaction identity and deletion

`{"c":"menuaction","op":"activate","id":4,"action":"duplicate"}` revalidates the
selection captured by `snapshot` before the GUI dispatches a selected-item menu action.
The response echoes `id`, `op`, and `action`, with `ok` and `error`. Captured selection `paths`
are returned for `validate`, and for `activate` when the action needs resolved paths for client-side
dispatch; other activations return an empty array. `taildrop:*` and `sharelink` return the captured
cursor path instead.
The client also rechecks its listing identity and current action eligibility before dispatch.

An optional `menuId` on `transfer`, `trash`, `duplicate`, `rename`, `archive`, or `convert`
binds that operation to the captured selection. An expired selection, replacement object,
or uncaptured requested source is refused. Transfer uses the captured sources directly;
Trash, Duplicate, Archive, and Convert require the entire requested source set to match.
Identity checks compare the device, inode and file type at the worker boundary; Trash checks again immediately before its existing GIO
batch handoff. These checks do not make subsequent external-helper filesystem operations
atomic. Omitting `menuId` preserves the existing keyboard and protocol entry paths.

`{"c":"menuaction","op":"snapshot","id":4,"rows":[0,2]}` captures the selected identities
from the active listing and returns `count` and `kinds`, an array of each captured item's own file
type bits in request order, including symlinks without following them. `prepareDelete` with that
`id` reviews the captured trees and returns a
fresh `token`, selected `count`, and total `bytes`. `checkDelete` with the `id` and `token` returns
`valid` without mutating files. A changed set requires `refreshDelete`: it reviews only the same
captured paths, drops missing paths, captures replacement identities, and issues a fresh token.
Unrelated paths cannot widen that confirmation. The client shows this new confirmation with
Cancel initially focused; no prior destructive activation authorizes the replacement token.

`{"c":"menuaction","op":"delete","id":4,"token":12}` consumes that exact token and reserves
the ordinary mutation slot. A failed preflight returns `stale:true` and requires a fresh review;
it does not delete any item. A completed attempt returns `deleted`, `failed`, `cancelled`,
`error`, and `remaining` absolute paths whose original identities survive. Results report partial
completion rather than implying rollback. The caller can use the identity-checked batched
`locate` form to select survivors after refreshing the listing.
`close` expires the token and cancels pending work. An already claimed deletion root completes
or restores its survivors before cancellation stops the next root. The undo journal does not
cover permanent deletion.

`{"c":"menuaction","op":"contents","id":4}` counts what the one captured folder holds, for Properties on a folder.
It walks with the folder size walker and never crosses into a nested mount or follows a symbolic link. A local folder
walks on the menu worker with no deadline. A folder on a remote mount, or holding one anywhere below it (the walk
still reads the mount point's own entry), walks on a worker beside the menu worker under the bulk deadline (15 s): past
it the reply is the floor counted so far, `partial:true` and `done:true`, the walk stops, and the remote mounts it
could touch are marked stuck for 30 s. While one is marked, the same request answers the empty floor at once
(`bytes:0`, `items:0`, `partial:true`, `done:true`) without a walk. `items` counts every entry the walk visits below the folder (files, folders, links and other entries), not the
folder itself, and `bytes` is the folder size walk's total, which includes the folder's own entry.
The walk runs under the request's own cancellation, so `close` or a newer `snapshot` stops it within one directory
read, and a remote one is answered at once. It answers a progress line every 150 ms while it walks, and none for a folder that
finishes sooner: `{"t":"menuaction","id":4,"op":"contents","ok":true,"bytes":2104332800,"items":812,"partial":false,"done":false}`.
The last line carries `done:true`. `partial` is true when the walk could not read a directory or one vanished, and the
numbers are then a floor. A cancelled walk answers once with `cancelled:true` and `"done":false`. An `id` that is not the
captured snapshot, a selection of several items, or an item that is not a directory (a symbolic link to one is a link)
answers `ok:false` with an `error`.

`{"c":"menuaction","op":"facts","id":4}` reads the codecs, bit rate and photo facts for Properties on a file, for the one captured
item. Properties sends it once, right after the `properties` reply, for a file that is not a folder or a link, so the card opens on
what the listing already knows and the new pairs arrive beside it. It answers one line on the menu worker, under the request's own
cancellation: `{"t":"menuaction","id":4,"op":"facts","ok":true,"vcodec":"h264","acodec":"aac","bps":8100000,"make":"Canon","model":"Canon EOS R6","lens":"RF35mm F1.8 MACRO IS STM","shutter":[1,250],"fnumber":[28,10],"iso":400,"taken":"2026:09:12 18:31:07"}`.
Every field is raw and a fact the file does not have is `""` or `0` (`[0,0]` for a rational), so the card shows no pair for it.
`vcodec`, `acodec` and `bps` come from the same `ffprobe` header read `meta` uses (`format=duration,bit_rate` and the streams'
`codec_name`; the first video and the first audio stream name the file), and only for a video container by extension. The photo
fields come from the file's own EXIF, read from the head and never decoded: a JPEG's first Exif APP1, a TIFF-based RAW (CR2, NEF,
ARW, DNG) from its IFD0 and Exif IFD in the first MiB, and a CR3 from the CMT1, CMT2 and CMT3 boxes of its Canon `uuid` box in the
`moov`. Text is cut to 120 characters with control characters turned to spaces. A cancelled request answers `ok:true` with
`cancelled:true` and no facts. An `id` that is not the captured snapshot, a selection of several items, or an item that is not a
regular file answers `ok:false` with an `error`; a fifo or a device is never opened.

### listpaths

`{"c":"listpaths","paths":[<string>,...],"first":<uint>}`

Example: `{"c":"listpaths","paths":["/home/gm/Pictures/a.png","/home/gm/Downloads"],"first":80}`

Builds a fresh listing out of the paths the client names, replacing the current one, and answers a
`listed` line followed immediately by a `rows` line covering rows `0..first`, exactly as `list`
does. The listing's base is `/` and each entry is its absolute path with the leading slash removed,
so `window`, `thumb`, `paths` and every other per-row facility keep working with no special case
anywhere; that is the same shape a `search` listing takes, for the same reason. The client splits
the last `/` itself when it wants to draw a name rather than a path.

**Nothing is sorted.** The order the client sent is the order it gets back, because the callers
are the picker's Recent location and the main window's Recent place, and their order is the
history's own, newest first; a sort by name would throw that away. `read` on the `listed` line is the time this build took and `sort` is always
`0.0`.

**A path that does not exist is dropped, not listed.** The build `lstat`s each one, so a history
entry whose file has since been deleted never reaches the client, rather than arriving as a row
whose `p`, `s` and `m` are all 0. A path that is not absolute, and the root itself, are dropped the
same way: this list is read out of a file every application on the desktop writes, so it is checked
here rather than trusted. The type recorded in `d` is the link's own, the same rule `list` follows,
so a symlink to a directory is listed as a file and a symlink to nothing is still an entry.

There is no cancel and no streaming: the build is one `lstat` per path inside the read loop, and the
one caller sends a few hundred at most.

A re-read over the listing already held names added plus removed rows in `changed`, so a reload
over Recent says how many history entries came or went; a first `listpaths` carries no `changed`.

### window

`{"c":"window","start":<uint>,"count":<uint>}`

Example: `{"c":"window","start":1200,"count":350}`

Stats rows `start..start+count` of the current listing (phase 2) and answers one
`rows` line. `start` and `count` are clamped to the listing's length, so a stale or
out-of-range window never panics, it just returns fewer rows or none. Missing fields
default to `0`.

### sort

`{"c":"sort","by":"<string>","desc":<bool>,"foldersFirst":<bool>,"groupByKind":<bool>,"hiddenLast":<bool>}`

Example: `{"c":"sort","by":"size","desc":true}`
Re-sorts the current listing by `by` and answers a `listed` line. The four orders are:

- `"name"` works on phase-1 data alone: `read` is `0.0` and `sort` is the sort.
- `"size"` and `"mtime"` pay the metadata pass first, one `lstat` per row of the whole
  listing split across the cores, then sort. A size sort also walks every folder's subtree
  under one shared 250 ms budget for the whole sort, and that walk is part of the pass,
  so its time lands in `read` too. `read` is that pass in milliseconds and `sort`
  is the sort, so the two costs stay readable apart on the wire.
- `"kind"` compares MIME type strings obtained from filenames, with
  `application/octet-stream` for an unknown name and `inode/directory` for a folder.
  Files with equal kind order by extension, none first with the Finder comparison, then by name; folders keep name order.
  It does not read file contents or run a metadata pass; `read` is `0.0`.

`foldersFirst` defaults to `true`: folders remain before files in both directions.
With `foldersFirst:false`, the key orders folders and files together. `groupByKind`
defaults to `false`; when true it takes precedence over `foldersFirst` and fixes
three groups in this order: folders, images (MIME type starts with `image/`), then
all remaining files. Groups do not reverse with `desc`. Kind grouping uses filename
MIME lookup, not content probing. `hiddenLast` defaults to `false`; when true it
is an outer partition, a visible block then a hidden block, and `foldersFirst`
or `groupByKind` orders inside each block. Neither block reverses with `desc`.

Inside each group, or across the whole listing when ungrouped, the key decides and
the name order breaks ties, except files with equal kind order by extension first, none first, then by name, so two equal sizes list the same way every run, and
`desc` is the exact reverse of ascending inside the group,
tie-break included. A size order walks each folder's subtree and orders folders by that
recursive size, the same number the `dirsized` line reports, so the order agrees with the
size column the way it already did for files; an mtime order lists them by time like
everything else. A folder that hit the shared budget orders by the floor it counted.
A size sort seeds the answered-row cache only with the folders it finished walking, so a
later `dirsize` for one of them is answered from the cache rather than walked twice. A folder
the budget cut off seeds nothing, and is walked again by the worker at its own per-row
deadline when the client asks, because a floor must not stand in for a size.

On a network or phone listing (the classes the client refuses folder sizes on) a size sort walks no folder: each folder
orders by its own entry size, the floor a walk past its deadline would give, and seeds nothing.

**A size order is best effort on a directory large enough to exhaust the 250 ms budget.** A
folder cut off by it is placed on the floor that was counted, and that floor is used for the
order alone: it is never sent, so nothing on the wire says the order was built from one. The
size the column shows for that folder arrives later, from a separate walk at the two second
per-row deadline, and the two walks carry no ordering relation to each other, so that number
can be larger or smaller than the floor the row was placed on. Its `dirsized` line carries
`partial` true only when that second walk ran out as well. On such a directory the column can
therefore disagree with the order it was sorted into, and a re-sort can still place two
folders by their floors. A client holding both the order and the later sizes can see that
disagreement; what it cannot tell from the wire is whether a floor or a changed tree caused
it. The budget is what keeps the whole pass short while it runs on the event loop, and this is the cost of that
choice.
The stat
is the same `lstat` that `rows` reports `s` and `m` from, so the order always agrees with the
column, symlinks included, and a row that vanished between the listing and the pass sorts as
the zeroes `rows` would send for it.

**Sort metadata is not retained between requests.** Reversing a size order stats the directory again and walks the folders again, bounded by the same budget each time,
because a listing in name order must not carry metadata it is not using. The stats
live for one request. Historical measurements from 2026-09-02, using the default
name ordering and directory grouping: on the 100,000 file fixture the backend's PSS
read 4781 kB after the listing and
4945 kB after a size sort (one `smaps_rollup` reading each), with a transient peak 3.2 MB
above that while the pass ran. Measured on that fixture, warm, on 2026-09-02, from the
`read` and `sort` fields of the `listed` line: the pass took 25 to 38 ms on twelve cores and
125 to 135 ms pinned to one with `taskset -c 0`, and the sort behind it 6 to 7 ms against
5 ms for name; on a three-row directory the whole request stayed under half a millisecond.
Cold is IO-bound, and an earlier measurement in the maintainer's notes, not in this repository,
put the same pass at about 1 s serial and 0.3 s on twelve threads. The pass runs inside the
loop, so a size or date sort of a very large directory on a slow mount stalls other backend
requests for its duration. An initial `list` explicitly requesting one of those orders pays
that same metadata pass.

**An unsupported, empty, missing, or non-string `by` answers an `error` line**,
never a `listed` line in name order. The error names the supplied string key, or an
empty key when no string was supplied. This prevents a refused request from silently
resetting the existing order. A refused `sort` leaves the listing in its existing
order. `sort` never emits a `rows` line on its own;
follow it with `window` to see the reordered rows. A missing `desc` defaults to `false`.

An optional `anchor` names the cursor's logical row for this re-sort as an absolute path,
`{"c":"sort","by":"size","anchor":"/home/gm/amber"}`, and the `listed` line answers it with
`"anchor"` echoing that path and `"anchorIndex"` carrying its index in the new order, or `-1`
when that path is not in the listing. Each reply answers only its own request's anchor, so a
burst of re-sorts needs no shared generation count: the client moves the cursor only when the
reply's anchor is still the newest one it sent, and ignores a superseded or foreign one. A
request without `anchor` answers exactly the `listed` line it always has, so every older client
keeps parsing what it knows.

`mtime` names the same stat field that `rows` carries as `m`; the GUI labels it
"Modified". `date` is also accepted as an alias for `mtime`, matching the saved
Settings value. `mode` is not a supported sort key.

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

**A query may carry `tag:<name>` terms beside the fuzzy text**, `tag:Red holiday` matching files
that carry the Red tag and fuzzy-match `holiday`. The name matches case-insensitively, and any
Flea or foreign name is accepted. Every named tag must be present. The attribute is read only on
files the walk visits, one `getxattr` each, never swept ahead of it.

### searchcancel

`{"c":"searchcancel"}`

Stops the running walk and answers one `searched` line with `cancelled` true. Whatever the
walk already found stays in the listing, is ranked the same way a walk that ran to the end is,
and stays windowable. Unlike `thumbcancel` there is
no `rows` form: one walk runs at a time, so a cancel can only mean that one. A
`searchcancel` with no walk running does nothing and answers nothing.

With a live session open the walk stops and the session stands: the standing query is
re-answered once from the partial index, `listed`, `rows`, then `searched` with the query's
own gen, `walking` false and `cancelled` true, and a later query is still answered from the
kept index. With no standing query, or an empty one, only that `searched` line answers, for
the last answered gen. The session never closes on a cancel; `searchclose` is what drops it.

### searchopen

`{"c":"searchopen","path":"<string>","hidden":<bool>,"folder":"<string>"}`

Opens a live session for the scope `path`. `folder` is the pane's folder the
session was opened from; scope and folder differ when the scope is home.
The walk starts at once on a background thread and the current listing is
untouched: no `listed`, no `rows`, no row invalidation. Answers one line:

`{"t":"searchopened","path":"<scope>"}`

`hidden` follows `list`'s rule exactly. The scope walks once per session with
one mountinfo read; a local scope walks with bounded worker threads, a remote
one with the single bounded worker `search` uses. One session runs at a time;
opening a second closes the first in silence. A `list`, `listpaths`, `sort`,
`search` or `searchclose` closes the session too, and drops
its index. Closing mid-walk cancels it: no directory opens after close.

### searchquery

`{"c":"searchquery","query":"<string>","gen":<uint>,"count":<uint>[,"sizes":<uint>]}`

Asks the open session. With no session open it answers nothing. The query is
answered from the index with no walk. A `tag:<name>` term keeps the entries that carry that tag, the
same case-insensitive rule `search` uses; the names are read once per row on a worker
thread after the walk lands, and until they are in the answer says `walking` true and
the landing re-runs the query. `gen` is the client's number, newer per
keystroke; a query whose `gen` is not newer than the last answered one is
dropped in silence, and a burst answers only its newest query. `count` is the
first window's size, the same meaning as `window`'s count; missing is the
list's own first-window default. `sizes` is how many leading rows the client
draws a folder size for, and 0 asks for none; a query without it keeps the
last one the session was given, so a burst answered from its newest query
keeps it too. Each answered query is three lines, in this order, the order
the listing swap already follows for `list`:

1. One `listed` line with `n` the match count and `path` the scope, carrying the query
   gen beside them, so a line landing after the session closed names no folder.
2. One `rows` line for the first window from row 0, ranked, stamped with the
   new generation and carrying the query gen beside it, so held row indices
   invalidate exactly as after a `sort` and a late line lands on no folder.
   With `sizes` above 0 it may end with `"z":[[<row>,<bytes>],...]` after
   `gen`, in row order, the same bytes a whole `dirsized` answer carries for
   each folder row among the first `sizes` rows whose complete size is known.
3. One terminal line naming the generation:

`{"t":"searched","n":<matches>,"scanned":<entries>,"ms":<ms>,"cancelled":false,"gen":<gen>,"walking":<bool>}`

`scanned` is the index size the query scored over. `walking` is true while
the walk is still running: the answer comes from the partial index, and the
standing query is re-run once when the walk lands, with its own gen, only if
no newer query has been answered since; the landing answer has `walking`
false and `ms` the walk's time. A newer query's answer replaces the window
whole: no blank listing stands between two queries.

A query asked while the walk still runs waits for the walk to land, up to
150 ms after the first query held, the search hold (`ui/js/Swap.js`
`HOLD_MS`, not the listing swap's cap `Swap.CAP_MS`, 66 ms), so it is answered once from the whole index; a newer query asked
meanwhile replaces it and answers in its place. Past `HOLD_MS` the held query
is answered from the partial index with `walking` true, and every later query
of that walk is answered at once. A `searchcancel` answers a held query as
its stop.

An empty query keeps the session and its index and answers the open folder's
own listing, the folder the session was opened from, in its own sort, with
`listed.path` that folder, in the same listed, rows, searched order with that
gen. No walk of the scope runs; one readdir of the folder is all it costs. Its
rows line carries `z` for the folder's own folder rows the same way.

A size is known when the session learned it earlier, from a `dirsize` answer
given while it is open or from its own walk for an earlier answer, keyed by
path; or when the walk this answer runs finishes it, all of that answer's
folders sharing one 3 ms budget. A folder left out of `z` is asked with
`dirsize` exactly as in any listing. A remote scope never walks for `z`, and a
scope where a walk that started misses the budget walks for no later answer in
that session; both still carry the sizes the session knows. A walk the
scheduler had not started by the deadline marks nothing. Every patch of the
index, from a write or the open folder's watch, forgets the known sizes.

The index is the session's snapshot: no per-directory watches and no
per-query mtime sweep. The backend's own writes under the scope patch the
index when they land, the open folder's existing watch patches that one
directory's entries when it reports a change, and reopening the session or
changing scope re-walks. An outside change deeper than the open folder shows
after a reopen. The index holds at most 200,000 entries; past it the walk
stops adding and the count stands.

The scorer and ranking contract are `search`'s own. A query extending the
previous one scores only the previous matches; any other change scores the
index. The first window is selected and ordered without a full sort; the rest
stays scored but unranked until a row past the window is asked for, when the
next chunk is ordered with the same comparator and appended behind the head,
the request's need or one window, whichever is larger, at least doubling the
adopted length, so no window ever shows a row from a different order than the
one the `listed` line counted. A path lookup and a sort take the whole rest in
one go, as does any request whose need spans it, select-all among them.

### searchclose

`{"c":"searchclose"}`

Drops the index and answers one line:

`{"t":"searchclosed","path":"<scope>"}`

With no session open it answers nothing.

### thumb

`{"c":"thumb","rows":[<uint>,...]}`

Example: `{"c":"thumb","rows":[2,17,140]}`

Asks for a thumbnail for those row indices of the current listing, and answers one
`thumbed` line per row. **This is the only thing that ever generates a thumbnail.** The
backend never walks a directory looking for work: a row that no client named is never
looked at, at any priority.

An off storage class still asks, and the backend answers from the cache alone. A client
on a network, phone or USB directory whose switch is off sends
`{"c":"thumb","rows":[<uint>,...>],"cacheOnly":true}`, and a genuine cache miss then
answers an empty `file` at once rather than queueing a decode: one stat of the source
per visible row and no read, so a NAS folder costs a metadata round trip instead of
about 48 MB. A row already in the shared cache answers with that entry's path, whether
or not the source can still be opened, and thumbnails another application already made
still show. Absent `cacheOnly` is today's full path, so a local folder and an older
client are unchanged.

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

### fsinfo

`{"c":"fsinfo"}`

Answers one `fsinfo` line for the directory the current listing came from:
`{"t":"fsinfo","fs":"<string>","free":<uint>,"path":"<string>","class":"<string>","tagable":<bool>}`.
`fs` is the filesystem's own name and `free` its bytes available to an unprivileged
process; `path` is the directory they are of, so a client that has since moved can
tell they describe somewhere else. `tagable` is that directory's once-per-change probe
for user extended attributes, so the Tags menu row is absent where the filesystem
cannot store them. A repeated `fsinfo` for the same directory reuses the answer and makes no
call. On a `network` or `phone` directory the probe runs on the statfs worker, never the loop:
the field reads `true` until it lands, and a refusal sends the line again with `false`. `class` is that directory's storage class,
`network`, `phone`, `usb` or `""` for local, computed once per directory change
beside this line and never per row: network is cifs, smb3, nfs, sshfs, rclone, 9p,
ceph and any gvfs FUSE share, phone is gvfs mtp, gphoto2 and afc, and usb is a block
device whose sysfs path runs through USB or whose removable flag is 1. An unreadable
path answers an empty `fs` with a free of 0 rather than a wrong number.
A local directory held by a system mountpoint adds `"system":true` after `class`, and no other line carries the key.
The mounts are matched by the directory's own device number in `/proc/self/mountinfo`, so a symlink,
a bind mount or a second mountpoint of one volume answers as the volume does, and a mountpoint counts as a
system one when it is `/`, `/home`, `/boot`, `/var` or anything else the rail's own list of system mounts names.
On a slow mount, a gvfs share or a kernel network filesystem, the statfs behind the
figures is a network round trip, so it runs on a worker thread, one per mount at a time, the
newest folder asked for going next: a share starts it once its `list` rows are sent, for a client that has asked `fsinfo`
before, and a kernel mount when `fsinfo` does. The statfs reads the listed folder itself,
because an sftp host or a phone's storages answer differently below one mount. On a gvfs share
it reads the folder's first plain file instead when the listing has one: gvfs answers a statfs
through `smbc_statvfs` on that exact path, and libsmbclient opens a folder for it by listing every
entry in it, where a file is one open and close. A file gone since the listing falls back to the
folder. `fsinfo`
answers at once with the class and that folder's last known figures, or unknown figures
(`"fs":""`, `"free":0`) on a first visit. When the worker returns, a second `fsinfo` line for
the same `path` carries the fresh figures, sent only when they differ from what that folder
last answered and only while it is still the listed directory. A client must accept that line
at any time: it updates the figures in place and changes nothing else.
Local and USB directories answer figures and class in the one line, exactly as before.

### dirsize

`{"c":"dirsize","rows":[<uint>,...]}`

Example: `{"c":"dirsize","rows":[4,9]}`

Asks for the recursive apparent size of those row indices, and answers one `dirsized` line
per row that names a directory. **This is the only thing that ever walks a directory looking
for size**, the same rule `thumb` follows for thumbnails: a row no client named is never
walked. A row that is not a directory, or past the end of the listing, is skipped in silence.

One persistent background worker walks one directory at a time. The event loop remains
available for navigation while a walk runs. A list, sort or `dirsizecancel` invalidates
unfinished size work using a generation counter. Cancelled or stale replies cannot populate
the current listing's cache. Cancellation and the 2000 ms deadline are checked between
filesystem calls; a blocked filesystem call may delay further sizes but never holds up the
navigation event loop. No replacement worker is spawned while the old one is still busy.
A row already answered for the current listing answers again from its cache; a row already
queued or running costs nothing extra.

**What the shipped client sends.** `ui/List.qml` sends `dirsize` only when the list settles, the
same 120&nbsp;ms timer `thumb` already waits on, so a fling issues nothing at all. One request
names only the directory rows currently visible and not already known.

### dirsizecancel

`{"c":"dirsizecancel"}`

Drops every directory row still queued and cancels the running walk cooperatively. Unlike `thumbcancel`, there is no rows
form: the walker is one at a time, so a stale row left over from a scrolled-past viewport would
delay the row the new viewport actually wants, and the client always means "everything" when it
sends this. A row already answered is untouched; queued and unfinished work is discarded. No response line.

A directory a `dirsizecancel` dropped can be asked for again straight away: cancelling forgets
the row, so a later `dirsize` for it queues fresh work.

### transfer

`{"c":"transfer","op":"<string>","paths":["<string>",...],"dest":"<string>"}`

Example: `{"c":"transfer","op":"copy","paths":["/home/gm/a.txt","/home/gm/photos"],"dest":"/home/gm/backup"}`

Copies or moves each top-level path into `dest`. `op` of `"move"` moves; **anything else, including a
missing `op`, copies**, so a malformed request can never remove a source. `paths` are absolute; `dest`
is an absolute directory that must already exist, because Flea does not create a destination as a side
effect of a transfer. A `dest` that is missing, relative, not a directory, or cannot be written answers a single `error`
line with `where` of `transfer` and nothing is started.

Unlike `thumb` and `dirsize`, this names paths rather than row indices: a transfer outlives the listing
it was started from, and a row index would name a different file by the time it ran.

**A `rows` array of indices into the current listing may be sent instead of `paths`**, because a client
can only build a path for a row inside the window it holds, and a selection can be wider than that. The
indices are resolved against the listing at request time and the operation runs on the resulting paths,
so it still owns a snapshot that outlives whatever the listing does next. `paths` wins when both are
present, and an index past the end of the listing is dropped in silence.

**One of `transfer`, `trash`, `duplicate` or an archive `extract` runs at a time.** One of those
arriving while another is still running answers an `error` line saying so and touches nothing. `link`
and `permissionsBatch` are refused the same way while one runs, without taking the slot: `link`
answers an `error` line and `permissionsBatch` answers its `permissions` line with `ok` false. The cap
is one because the status bar carries one transient slot for the running operation, and an extract
drives that same card, so a second concurrent operation would have nowhere to report. `rename` and
`mkdir` never take that slot, and an archive `compress` and a `convert` are keyed by their own `id` and
run alongside by design, so the cap was never one write of any kind.

The answer is a `transferstarted` line for a file transfer, then per top-level item a bounded stream of
`transferprogress` lines and exactly one `transferitem`, then one `transferdone`. An archive `extract`
uses `extractstarted` for its activity card and `archivedone` for its terminal line.

**Semantics that are decided here rather than left to the caller.** A same-filesystem move is a
`rename(2)`; a cross-filesystem move is a copy followed by removing the source, and the source is only
removed once the copy is complete, so a process killed mid-move leaves the source intact and a partial
file at the destination, never the reverse. A symlink is copied as a symlink and never followed. A
fifo or a socket is recreated at the destination with the source's own mode rather than opened,
because neither holds contents to stream and an open of one would wait for a writer or fail. A device
node takes the same path, but creating one needs `CAP_MKNOD`, so an unprivileged copy fails that item
with `EPERM` instead of recreating it; either way nothing streams from a device that never ends. A
destination that already exists is refused for that item rather than overwritten, because every write
here creates its target exclusively, unless the request carries a choice for it, below; even `replace`
never overwrites, it moves the item already there to the trash and then still creates exclusively.
Directory recursion is invisible on this wire: the backend walks a
tree to copy it and the client sees only the top-level item's lines, so the wire's shape does not depend
on how deep a folder is.

**An entry the destination cannot hold never ends the walk of the tree around it.** Inside a folder
being copied or moved, the backend skips and records an entry the drive refuses for that entry alone,
and goes on with every other one: a link where `symlink(2)` answers `EOPNOTSUPP` or `ENOSYS`, or `EPERM`
under vfat or exfat (`this drive cannot hold links`); a fifo, socket or device node where `mknod(2)`
answers the same (`this drive cannot hold special files`); a name the create or `mkdir` refuses with
`EINVAL` or `EILSEQ` (the drive's own rule, as `':' is not allowed in a name on this vfat drive`, or `this
drive does not allow that name`) or `ENAMETOOLONG` (`that name is too long for this drive`); `EEXIST`
under vfat or exfat beside a source sibling that differs only in case or in trailing dots and spaces, which
is the drive folding two names into one (`this drive treats that name as the same as another in the
folder`), while `EEXIST` on any other drive, or with no such sibling, is another writer's and ends the
item; and a file of 4 GiB or more onto vfat. A link
is never followed in place of the link. Any other failure ends the item. A tree that landed without some
entries is a failed item, never a success: its `transferitem` answers `ok:false` with one whole sentence, `<path> could not be
copied: <reason>` for one entry, `<n> items inside could not be copied; the first is <path>: <reason>` for
more, the path relative to the item and the verb `moved` for a move, whose source stays whole. What
landed is journaled as a partial copy with its manifest, so `undo` removes exactly it, and on a durable
destination it is still confirmed before `transferdone`. At the top of the selection the same refusal is
the item's own failure with the reason as its `err`. Each entry is named on `transferdone`'s `refused`.

**Copies onto removable, phone and network targets are durable before "done".** When the
destination classes usb, phone or network, or its filesystem is vfat, exfat or ntfs, each file is
fsync'd after its last write and every directory Flea created or wrote into is fsync'd before the
transfer reports done. An rclone mount is the exception: the copy lands in rclone's cache first,
so no fsync forces its upload and the done line names the background upload instead. Local btrfs and ext4 targets keep today's behaviour. A failed file fsync fails
that item like any other copy error and journals the partial for undo. The same rule covers a
same-directory duplicate, a redo of a copy, and a same-filesystem rename onto a durable target:
the new files are fsync'd, and the destination directory (and the source directory for a move)
with them; a rename that already landed reports success either way, because claiming otherwise
would lie about what happened.

**A choice for names that already exist rides on two optional fields**, `collide` and `collideId`:

Example: `{"c":"transfer","op":"copy","paths":["/home/gm/Desktop/screenshot.png"],"dest":"/home/gm/Pictures","collide":"replace","collideId":7}`

`collide` is `"keep"`, `"replace"`, `"skip"`, `"refuse"` or `"merge"`, and `collideId` is the `id` of the
`collisions` question it answers. A merge carries one more field, `inner`, the second card's answer
for files that exist in both, with the same four words and no `merge` of its own.
Example: `{"c":"transfer","op":"copy","paths":["/home/gm/trip"],"dest":"/home/gm/Pictures","collide":"merge","collideId":8,"inner":"keep"}`.
A missing `collide` is the transfer exactly as it was before this
field existed, so an older client is unchanged; any word but those five refuses, so a malformed choice
can never replace. **The choice covers only what the question listed**: an item whose source that
question named for this same `dest`, whose name there still holds the same item it held then (device,
inode and file type). A name that appears after the question, or whose item was swapped since, is
refused exactly as before whatever the choice, and so is every name when `collideId` names no
question, another destination, or a question an earlier transfer already spent.

`keep` lands the incoming item under the name `duplicate` would give it, worked out in `dest`:
`photo copy.png`, then `photo copy 2.png`. `skip` leaves the item where it is and counts it in
`transferdone`'s `skipped`, with no `transferitem` line. `merge` folds a folder landing on a folder
together instead of replacing it whole: everything missing in the destination is copied or moved
in, and folders inside merge the same way all the way down; see `mergecollisions`. A folder is
merged only while it still holds the item the question saw, and files that exist in both take the
transfer's `inner` answer, `keep`, `replace`, `skip` or `refuse`, asked once by that walk before
anything is written. No answer ever overwrites without it. `replace` moves the item already there to the
freedesktop trash first, through the same `gio` call and URI capture `trash` uses, and then transfers
the incoming one under the name; a folder is replaced whole and never merged into. A trash that refuses
fails that item with `the item already there could not be moved to Trash, so nothing was replaced` and
touches nothing. An item already there that holds any source this transfer names, the item's own or
another's, fails with `the item already there holds the one being moved in, so it was not replaced`,
because trashing it would take that source along; and an incoming symlink that resolves to the item
already there now, or whose own text leads back to that name once the link sits there (looked up the way
the kernel would, through any other link on the way), fails with `the incoming link points at the item
already there, so it was not replaced`, because the copy would be a link to itself. A lookup that passes
40 links without reaching that name is let through like a dangling link: the kernel refuses it either
way, and the item it replaces waits in Trash for `undo`. A replace whose
transfer then fails, a cancel included, puts the trashed item straight back when nothing took its name,
and otherwise leaves it for `undo`. When that put-back itself fails, the item's `err` gains `; the item
it replaced is still in the Trash` and the reason, and the trash step stays in the journal so `undo` can
still restore it. A cancel gains it too, as `cancelled; the item it replaced is still in the Trash (<reason>)`,
and that item is counted in `failed`, not `skipped`, because its name no longer holds what it held;
`transferdone` still says `cancelled`. The trash and the transfer are one journal entry, see `undo`.

**Any `collide` word also settles an item that already lives in `dest`.** A copy lands under
`duplicate`'s name, and a move is left where it is and counted in `skipped`, with no error. Without
`collide` both still fail with `already in that folder`.

### collisions

`{"c":"collisions","id":<uint>,"paths":["<string>",...],"dest":"<string>"}`

Example: `{"c":"collisions","id":7,"paths":["/home/gm/Desktop/screenshot.png","/home/gm/Desktop/notes"],"dest":"/home/gm/Pictures"}`

Asks, before a `transfer` is sent, which of the items it would name already have their name taken in
`dest`, and answers one `collisions` line carrying the same `id`, unless a later `collisions` is sent
before it is done, below. `rows` may be sent instead of
`paths` and is resolved against the listing exactly as `transfer` resolves it, and a `menuId` asks
about that menu's captured selection instead, the one a `transfer` with that `menuId` runs on.

Example: `{"c":"collisions","id":8,"menuId":31,"dest":"/home/gm/Pictures"}`

It is read-only and never walks a tree: one `lstat` per source, one per destination name and a resolve
of each source's folder. That still measured 0.9 s for 100,000 local sources, and a network mount pays a
round trip for each, so the question runs on a thread of its own and the loop keeps answering other
requests; the `collisions` line arrives whenever it is done, and a client waits for it before sending
the `transfer`. A source that is not absolute, that no longer exists, or that already lives in `dest`
is not counted, because the transfer settles those without a question. A `dest` that is missing,
relative, not a directory, or cannot be written answers a `total` of 0, and the `transfer` that follows answers its own
`error`.

**The backend keeps the latest question**: each colliding source with the identity of the item its
name holds in `dest`, which is what lets a `transfer` naming this `id` in `collideId` apply one choice
to exactly those names. It is kept before its `collisions` line is written, so a transfer sent after
that line always finds it. The latest question asked is the one kept and the only one answered: an
earlier question still being asked when a later one arrives is dropped without a line, the way a result
for a superseded listing is, so it is never answered after the later one's; a client asks one question
at a time and waits for its line. The next file transfer spends the kept question whether or not it names it. **A `menuId` question also keeps the menu's selection and destination as it saw them**,
and a `transfer` carrying that `menuId` and naming this question in `collideId` runs on that capture:
Copy to closes its dialog, which sends `menuaction` `close` and expires the live selection, before the
answer comes back. The capture holds the same device, inode and type identities the live selection
does, and they are still checked per item when the transfer runs. Collision destination identities
also compare birth time when the filesystem reports it on both sides. A `menuId` whose selection has
already expired answers a `total` of 0, and the `transfer` then answers `Selected items changed; reopen the menu.`.

**FAT-family destinations are answered inside this round trip.** When `dest` is on `vfat`, `exfat`,
`fat`, `fat12`, `fat16`, `fat32`, `msdos`, `ntfs` or `ntfs3` (read off `/proc/self/mountinfo`, never
from a name) and the question carries no `fat`, a source whose name that filesystem refuses is answered
with one `fatnames` line (below) in place of the `collisions` line. The client asks the same question
again with `"fat":"rename"` or `"fat":"skip"` and gets the `collisions` line for the names that will
land: Rename and copy asks about the renamed leaves, Skip them leaves the refused names out. Rename and
copy also renames, by the same rule, every UTF-8 name the transfer writes inside a folder it copies or merges into, and writes a name that is not UTF-8 as it is;
Skip them skips only the listed top-level names and leaves the names inside as they are. A redo of that copy writes the same names. The
`transfer` or `link` that follows names the same answer in `fat` beside `collideId`, so the names the
card promised are the names the operation takes, and a skipped name never lands at all. Each forbidden
character (`: ? * " < > |`) becomes an underscore, trailing dots and spaces are trimmed, and a name of
only dots becomes `_`. A `dest` on any other filesystem, or one whose mount cannot be read, never
answers `fatnames`, and a `fat` sent to it changes nothing, so nothing is ever renamed on a guess.
Without `fat` both requests behave exactly as before, which is every older client. The merge walk
takes the answer from the `collisions` question it follows, so under Rename a name inside a merging folder
is asked about under the name the transfer writes, and a renamed name that meets an existing one gets the second card's question.

### mergecollisions

`{"c":"mergecollisions","id":<uint>,"paths":["<string>",...],"dest":"<string>"}`

Example: `{"c":"mergecollisions","id":8,"paths":["/home/gm/trip"],"dest":"/home/gm/Pictures"}`

Asks, after the first card answered `merge`, which files inside the merging folders already have
their name taken in `dest`, and answers one `mergecollisions` line carrying the same `id`. `rows`
may be sent instead of `paths` and a `menuId` asks about that menu's captured selection, exactly
as `collisions` does, and the walk reuses the first card's capture rather than the live selection
its dialog already expired. It is read-only and runs beside the loop like `collisions`: one walk
of the sources against the destination, made before anything is written, gathering every relative
name both sides hold except folders that merge further down. The transfer that follows names this
question in `collideId` and carries the second card's answer in `inner`; a name that appears after
the walk, or whose item was swapped since, is refused exactly as before whatever `inner` says.

### pasteimage

`{"c":"pasteimage","dest":"<string>","name":"<string>"}`

Example: `{"c":"pasteimage","dest":"/home/gm/Pictures","name":"Pasted image 2026-09-23 at 10.14.32.png"}`

Writes the clipboard's image as a named file in `dest`, the p key with no paths on the
clipboard. The bytes never cross this wire: the backend runs `wl-paste --type image/png`
itself, refuses an image past 256 MiB and one that never arrives within 30 s, and streams the
rest into the destination through an O_EXCL temp published by a no-replace rename, which a FAT
destination takes where it takes no hard link, so a name that appeared meanwhile is never replaced.
A name already taken lands beside it as `stem (n).ext`; a clipboard holding no image answers an
`error` line carrying `"code":"emptyclipboard"`, which the UI words itself; a name that is empty, `.` or `..`, or carries `/`, is refused before anything runs. The answer
is the same `made` line a new file gets, and one journal entry named `paste`, so z undoes it; redo
refuses it, because a clipboard has no recorded source to replay.

### link

`{"c":"link","op":"<string>","paths":["<string>",...],"dest":"<string>"}`

Example: `{"c":"link","op":"relative","paths":["/home/gm/a.png"],"dest":"/home/gm/Pictures"}`

Creates one link per top-level path inside `dest`. `op` of `"absolute"` stores
the full path, `"hard"` creates a hard link, and anything else, including a
missing `op`, links relative from the destination, so a malformed request can
never write an absolute path by accident. Like `transfer`, this names paths
rather than row indices, and a `rows` array of indices into the current
listing may be sent instead of `paths`, resolved the same way; `paths` wins
when both are present. A `link` whose `listing` is not the numbering in force
is refused with `where` of `stale` before a single index is resolved, the
same rule `transfer` follows.

Like `transfer`, this runs beside the loop and takes the one-operation slot:
the busy check and the collide answer are decided on the loop, then one
thread links each source, replacing through the trash when the card chose
it, and the answer is written from its message. A source or the destination
on a mount with a pending slow write waits first, the same gate every other
write passes. A Replace trashes through
gio, which stalls on an unresponsive mount, so that work never runs on the
loop's own thread. A `link` arriving while an operation runs
answers an `error` line carrying `an operation is already running` and journals nothing. The answer is one `linked` line,
`{"t":"linked","ok":<uint>,"failed":<uint>,"skipped":<uint>}`, and one journal
entry, so one undo removes every link this request created. `note` rides on that line whenever a link this request
made could be neither verified nor removed, plain or replacing, naming each leftover link and its cleanup error as `the link at <path> could not be removed: <reason>`, followed by
`; the item it replaced is still in the Trash` where a replaced item was kept, and ahead of that a mixed batch's first
failure, so a batch that lands some links still names what the rest failed with. An all-failed batch answers an `error` line with `where` of `link` carrying the first
failure with that note appended, naming the failing source as `<source>: <message>`. A source that no longer exists is refused
for that item and counts in `failed`, so one missing source never stops the rest. A name that
already exists is refused for that item unless the request carries the
`collide` and `collideId` choice a `transfer` carries, applied by the same
rule: only what the question listed, only while the name still holds the same
item. `keep` lands the link under the name `duplicate` would give it, `skip`
leaves the item where it is and counts it in `skipped`, and `replace` moves
the item already there to the trash first and then links under the name. A
hard link across drives is refused with both filesystem names in the
sentence, and a hard link to a folder is refused outright. A `dest` that is missing, relative, not a directory,
or cannot be written answers a single `error` line with `where` of `link` and nothing is started.

### linktarget

`{"c":"linktarget","path":"<string>","id":<uint>}`

Example: `{"c":"linktarget","path":"/home/gm/latest","id":3}`

Answers one `linktarget` line,
`{"t":"linktarget","path":"<string>","directory":"<string>","name":"<string>","id":<uint>}`,
naming the folder the symlink's target lives in and the target's own leaf, the
same path Show in folder uses. `id` echoes the request's, so a reply landing
after the pane navigated elsewhere is answered only when it still names the
pending one. `directory` is the target's folder with every
symlinked folder and `..` resolved the way the kernel follows them, falling back
to the link text's own split when it cannot be resolved, and the line is answered
from a thread, as `meta` is. A path that is not a symlink answers an `error`
line with `where` of `linktarget` and touches nothing.

### permissionsBatch

`{"c":"permissionsBatch","paths":["<string>",...],"modes":["<string>",...],"id":<uint>}`

Example: `{"c":"permissionsBatch","paths":["/home/gm/a.txt","/home/gm/b.txt"],"modes":["600","600"],"id":7}`

Changes the mode of every named path to its own target mode, each following
the same three-or-leading-zero-four octal rule the dialog's own `apply`
enforces, and answers one `permissions` line with `op` of `applyMany`,
`{"t":"permissions","id":7,"op":"applyMany","ok":true,"mode":"0600","error":""}`.
One journal entry holds every path the Apply changed, so one undo restores
them all. A `permissionsBatch` arriving while an operation runs answers `ok` false with
`an operation is already running` and changes nothing. Octal, Owner, Group and the change preview drop out for several
items: the grid and Apply are the whole card. A mixed box the operator never
touched keeps each file's own bit, because the client sends that file's own
target mode rather than one mode for all.

### transfercancel

`{"c":"transfercancel","id":<uint>}`

Example: `{"c":"transfercancel","id":12}`

Cancels the running transfer or archive `extract` if `id` names it, and does nothing otherwise, so a
cancel aimed at an operation that already finished can never reach the one after it. A compress
or a convert has no operator cancel, so a `transfercancel` naming either `id` does nothing and the
job runs to its end. There is no
response line of its own: a transfer answers with its own `transferdone` carrying `cancelled` true, and
an extract answers its own `archivedone` with `ok` false and an `err` of `cancelled`.

**The item in flight is stopped rather than allowed to finish, and what it had already written is
removed: a partial file by `copy_file`, a partly-copied directory by `copy_dir`.** A cancel that waited out a multi-gigabyte copy would not be a cancel, and a half-written file
at the destination is not a result anyone asked for. That item is reported `ok:false` with an `err` of
`cancelled`; every item not yet started is counted in `skipped`. A cancelled extract stops and reaps
its sandboxed child, removes its staging directory and publishes no destination, so the same rule holds
there without a partial tree. When the archive tool has not exited ten seconds after the cancel, its work
folder is left in place rather than deleted under a live writer, and the `err` names that folder.

A `quit`, or stdin closing, cancels a running operation the same way and waits for its terminal line
before the process exits, so shutting down mid-copy also leaves nothing half-written behind. A
quit sets each detached compress and convert flag too and drains them under the one 25 s shutdown
budget, so each job's `Work` cleanup runs before the process exits and no `.flea-work-*` folder is
left beside the destination. A job whose tool has not exited ten seconds after the cancel keeps its folder in place
and its terminal `err` names it; one still running when the budget ends is abandoned with the process.

### flashimage

`{"c":"flashimage","path":"<string>"}`

Example: `{"c":"flashimage","path":"/home/gm/Downloads/raspios-arm64.img.xz"}`

Classifies one image and answers one `flashimage` line. The kind comes from the name and is
confirmed by magic bytes: `.iso` (ISO 9660 `CD001` at 32769), `.img` and `.raw` (anything),
`.img.xz` (`FD 37 7A 58 5A 00`), `.img.gz` (`1F 8B`), `.img.zst` (`28 B5 2F FD`),
`.img.bz2` (`BZh`), `.zip` (`PK 03 04`). A name and magic that disagree answer `"kind":""`
and the row is refused with "This is not a disk image Flea can flash." The unpacked size is
read in the sandbox, never parsed in Flea's own process: `xz --robot --list` for xz,
`zstd -lv` for a frame that records its content size, and the archive listing Flea already
runs for a zip. gzip's size field wraps at 4 GiB and bzip2 has none, so both answer
`known:false`. A zip qualifies only when its listing has exactly one member and that member
is an image by the rules above.

`{"t":"flashimage","path":"<string>","kind":"iso|img|xz|gz|zst|bz2|zip","bytes":<uint>,"unpacked":<uint>,"known":<bool>,"member":"<string>"}`

`bytes` is the file's own size. `unpacked` is the size that will be written, or 0 when
`known` is false. `member` names the zip's one image member and is empty otherwise. A
refused row answers `"kind":""` with `unpacked` 0, `known` false and `member` empty.

### flashtargets

`{"c":"flashtargets","image":"<string>"}`

Example: `{"c":"flashtargets","image":"/home/gm/Downloads/debian-13.iso"}`

Answers one `flashtargets` line listing every eligible drive in `/sys/block` order, with
the refusals the image causes. `image` may be empty, which is a pane opened from a drive:
every drive then lists with no `small` refusal. Never lists a drive the safety model
excludes; see "The image flasher" in `AGENTS.md`.

`{"t":"flashtargets","drives":[{"dev":"sda","seq":<uint>,"serial":"<string>","label":"128GB","model":"General USB Flash Disk","bytes":<uint>,"volumes":["128GB"],"refused":""}]}`

`seq` is the kernel's `diskseq`, which never repeats within a boot, so it names this
plugging of this medium. `label` is the rail's name when the drive shows as one rail row,
else the model. `model` joins vendor and model the way `omarchy-drive-info` does. `volumes`
names the drive's partitions. `refused` is one of `small`, `readonly`, `holdsimage`,
`member` (part of a RAID or volume group), or empty when the drive takes the image.

### flash

`{"c":"flash","id":<uint>,"image":"<string>","dev":"<string>","seq":<uint>,"serial":"<string>","bytes":<uint>}`

Example: `{"c":"flash","id":31,"image":"/home/gm/Downloads/debian-13.iso","dev":"sda","seq":2,"serial":"2003007157171692","bytes":124623257600}`

Writes the image to the drive the Flash step named and verifies it. The identity fields
are the ones the Flash step showed when it was pressed; if any differs when the backend
re-reads it, nothing is touched and the answer is `flashdone` with `err` `changed`. A
flash is keyed by its own `id` and runs alongside a transfer, because its target is a
device no transfer can reach; one flash runs at a time per backend, and a second answers
an `error` line with `where` `flash` rather than queueing.

Answers: one `flashstarted`, a stream of `flashprogress` throttled to one line per
150 ms, the transfer rule, and exactly one `flashdone`.

`{"t":"flashstarted","id":<uint>,"total":<uint>,"known":<bool>}`

`total` is the unpacked size when `known` is true and the file's own size otherwise.

`{"t":"flashprogress","id":<uint>,"phase":"prepare|password|write|flush|verify","bytes":<uint>,"total":<uint>,"permille":<uint>}`

`bytes` counts bytes the device accepted (write) or read back (verify). `permille` is
the share done: bytes over total when the size is known, else the offset of the
compressed file the decompressor has consumed over its size. Once the write ends, `total`
is the byte count written, so flush and verify measure against it even when the size was
unknown. `prepare` runs while the backend re-reads the drive and asks the helper to confirm
it, `password` runs from the helper's spawn to its descriptor, `flush` covers the final
`fsync`.

`{"t":"flashdone","id":<uint>,"ok":<bool>,"phase":"<string>","at":<uint>,"err":"<string>","blank":<bool>}`

`err` rides only on a failure: `cancelled`, `denied` (the prompt was dismissed or
refused), `busy` (a volume would not unmount), `changed` (identity differs), `gone`
(the device vanished), `io` (a write or read failed), `verify` (read-back differs,
`at` is the first differing offset), `nospace` (a stream outgrew the drive),
`decompress` (the decompressor failed or stalled), `sandbox` (bwrap or prlimit
missing), `helper` (the helper or udisks failed). `blank` says whether the drive's ends
were wiped, and is true only when the wipe itself succeeded: a write that ends `cancelled`,
`nospace`, `decompress` or `io`, a cancel or quit during the final flush and a `verify`
mismatch wipe. `blank` is false where nothing is wiped: `sandbox`, which is found before
the first byte moves, every refusal before the descriptor, a drive that is `gone`, and a
cancel during verify, which leaves the whole image on the drive and offers Eject alone. A pane that sent an image no probe answers
receives `phase` `probe` with `err` `io`; the pane never sends one, because Flash is
unavailable until the probe answers a kind.

### flashcancel

`{"c":"flashcancel","id":<uint>}`

Cancels the flash `id` names and nothing else, as `transfercancel` does. No line of its
own; the flash answers its own `flashdone` with `err` `cancelled`. Only Cancel stops a
flash: Esc leaves the pane and never cancels one, and a `quit`, or stdin closing,
cancels the flash and waits for its `flashdone`, so the drive ends blank rather than
half written. A cancel that lands during the verify is the one exception: the image is
whole and proven so far, so it stays and `blank` is false.

### flasheject

`{"c":"flasheject","dev":"<string>","seq":<uint>}`

Example: `{"c":"flasheject","dev":"sda","seq":2}`

Answers `{"t":"flasheject","ok":<bool>,"err":"busy|gone|changed|error"}`. Unmounts every
mounted filesystem on the drive, then powers it off. The message the user reads is the
rail's own, so "Ejected 128GB, it is safe to unplug." is the shipped string. `seq` names
the plugging the step showed; a drive that changed since answers `changed`. A `dev` that is
not a bare kernel name, or a drive the flasher would not list (a system disk), answers
`error` before the helper runs.

### trash

`{"c":"trash","paths":["<string>",...],"rows":[<uint>,...]}`

Example: `{"c":"trash","paths":["/home/gm/old.txt"]}`
Example: `{"c":"trash","rows":[4,9]}`

`rows` is the same alternative to `paths` that `transfer` documents above, resolved the same way.

Moves each path to the freedesktop trash by running `gio trash` under a 10 s deadline, and answers
one `trashed` line.
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

**A rename that cannot prove the source survived whole answers `rename-kept`.** Measured mounts that cannot serve
`RENAME_NOREPLACE` include `fuse.rclone` directories and `fuse.megafs` paths, which answer `EINVAL`, and a path under a
`/run/user/*/gvfs/dav:` WebDAV mount answers `EIO`. On those, the backend builds the new name through the same exclusive copy primitives every
other write uses and removes the source only once that copy is complete. The copy is taken back only on proof the source
survived whole: a source that still stats as anything but a directory after the failed removal, since
`remove_file` removes every other kind with one unlink that either takes effect or does not. That answers a plain `rename`
error, and so does a failure to take the copy back, in which case the copy stays under the new name
and that error's `path` is the copy rather than the source. Every other state keeps the copy, a directory source because `remove_dir_all`
stops at its first failure, and a source that no longer stats because an errno alone cannot tell a
removal that took effect from one that did not. The request then answers an `error` line whose
`where` is `rename-kept`, whose `path` is
the source, and whose `msg` is the removal's own message; the target is on neither field. Neither
direction journals the kept copy, so no `undo` removes it: the removal can stop partway, and one error
with no account of how far it got cannot tell a whole source from a remnant or from one already gone. An `undo` reverses a
rename through the same call, so a reversal that half succeeds answers this same `where`. A case twin
whose move-back from its temp sibling fails answers `rename-stranded` instead: its `path` is the
source, its `msg` names the temp leaf and the move-back's cause, and the file stays under that
hidden name.

Unlike the three above, this answers on the loop's own thread on a local mount: the ordinary
case is one `renameat2`, which costs less than spawning a thread. On a remote mount it runs on its
own worker with the single-call deadline and answers `slow` first, then its `renamed` line when the
write lands, journalled exactly as the in-time path would journal it; see `slow`. The compatibility
paths above are not one syscall, and on a remote mount they run on that worker too, so a directory
rename on rclone or MEGA no longer holds the loop while it copies. See `AGENTS.md`, "Write operations
and the undo journal".

### bulkrename

{`"c":"bulkrename","paths":["<string>",...],"names":["<string>",...]`}

Example: `{"c":"bulkrename","paths":["/home/gm/a.txt","/home/gm/b.txt"],"names":["b.txt","a.txt"]}`

Renames every named path to its bare new name in the same directory and answers one `bulkrenamed` line,
`{"t":"bulkrenamed","ok":<uint>,"unchanged":<uint>}`. The two arrays run in parallel: `names[i]` is the new bare name for `paths[i]`. The whole batch is validated before anything moves: an empty batch, a length mismatch, a non-absolute or repeated path, paths in more than one folder, a missing source or one replaced by another item before the first move, an empty name, `.`, `..`, a name containing `/`, NUL, CR or LF, a name over 255 bytes, two new names that collide, or a destination that already exists outside the batch all refuse the whole batch with an `error` line whose `where` is `bulkrename`, and nothing on disk moves. A destination that is itself one of the batch sources is allowed: swaps and cycles move through temporary names in the same folder, so a to b and b to a works. A name equal to its own source is unchanged rather than work: it counts in `unchanged` and records nothing. The whole batch is one journal entry, so one `undo` reverses every rename it made, newest first; a failure partway journals what landed the same way, so `undo` still reverts the partial batch. Like rename it answers inline on a local mount and on a remote mount runs on its own worker with the single-call deadline, answering `slow` first and its `bulkrenamed` line when the batch lands; see `slow`.

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
Every other failure carries `src/error.rs`'s plain sentence as `msg`: a parent that vanished since the listing
answers `item not found`, a parent the user cannot write `permission denied`, a read-only mount `the drive
is read-only`, a name past `NAME_MAX` `that name is too long`.

Like `rename`, this answers on the loop's own thread on a local mount and never takes the
one-operation slot; on a remote mount it runs on its own worker with the single-call deadline and
answers `slow` first, then its `made` line when the write lands; see `slow`.

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

### shebang

`{"c":"shebang","path":"<string>","id":<uint>}`

Example: `{"c":"shebang","path":"/home/gm/run.sh","id":3}`

The cursor row's two-byte `#!` probe behind the Make executable menu row, for **one path the
client named**, never a sweep: only a regular file without its owner execute bit is ever asked
about. Answers one `shebang` line, `{"t":"shebang","path":"<string>","hasShebang":<bool>,"id":<uint>}`,
on a thread, because an open on a hung mount never returns and the loop waits on nothing. The
`id` echoes the request so a later answer never arms an earlier row: a client that asked twice
keeps the newest `id` and drops any line naming an older one. A path that is not a regular file,
or that cannot be read, answers `hasShebang` false rather than an error, so the row stays absent.

### code

`{"c":"code","token":<uint>,"slot":"<string>","path":"<string>"|"text":"<string>","name":"<string>","lang":"<string>","first":<uint>,"count":<uint>,"raw":<bool>,"csv":"<delimiter>","record":<uint>}`

Example: `{"c":"code","token":4,"slot":"look","path":"/home/gm/a.rs","first":40,"count":20}` and
`{"c":"code","token":5,"slot":"fence:2","text":"fn main() {}","name":"x.rs","lang":"","first":1,"count":10}`

The syntax-coloured lines `first` to `first + count - 1` of **one file or one text a client
actually asked for**, never a sweep. A request holding `text` sends the bytes the client already
holds, with `name` picking the language when `lang` is empty (a Markdown info string names it in
its first word); without `text` the backend opens `path`, which a client never sends for a file on
a network or phone mount. `first` counts from 1 and `count` is capped at 400 lines (the line cap,
`lineCap` in `ui/CodeLines.qml`, which asks a longer window in parts), absent meaning the cap. `raw` true skips the colouring and answers the lines plain. Answers one `code` line on a
thread. A newer request on the same `slot` ends the older job and the older reply is never sent,
so a client matches the `token` of what it last sent and drops any other. At most 1,048,576 bytes
of a file are ever read, and `bat` runs in the same read-only jail as the other helpers, on the
bytes the verdict was made on.

A raw table request may carry `csv`: `","` for CSV or `"\t"` for TSV. The backend scans
the already loaded bytes as RFC 4180 records, with quotes opening only at a field start,
`""` escaping a quote inside quotes and quotes in unquoted fields staying literal. It moves
`first` back to the start of the record containing the asked first line, and extends the
asked end through its record's last line, or EOF for an unterminated quote. Thus the reply
contains whole records and can exceed the 400-line request cap, bounded by the 1 MiB size
gate. No second file read or additional stat is made. An absent or invalid `csv`, or one
without `raw: true`, keeps the ordinary line window.

A raw `csv` ask may also carry `record`, a 1-based record ordinal with the header at 1,
instead of locating its window by the physical `first` line. The backend locates that
record in the same scan and asks `count` physical lines from its start, extending the
end through the last record as above. This lets a table jump directly by delegate index
without fetching a prefix or receiving a file-wide index. `record` is ignored without a
valid raw `csv` delimiter; zero is clamped to 1. A record beyond EOF returns no lines.

### html

`{"c":"html","token":<uint>,"slot":"<string>","path":"<absolute path>","width":<uint>,"height":<uint>,"cancel":<bool>}`

Example: `{"c":"html","token":3,"slot":"look","path":"/home/gm/index.html","width":984,"height":625}`.

Renders the single page a client is previewing, never a listing. Width is the frame's device
width, clamped to 1 through 4096 pixels. Optional height is the viewport's height at that width;
when width is clamped, height scales with it, rounded up and bounded to 1 through 8000 pixels.
An omitted height retains the column helper's 600-pixel viewport. The page must be a readable regular file at most
1,048,576 bytes. A newer request in the same slot cancels the older helper and suppresses its
reply. `cancel:true` ends that slot without opening a page. HTML and code slots are independent.

The qml6 helper runs offscreen in its own bwrap namespace with network unshared, a 30-second
CPU limit, and a 15-second wall deadline. Chromium needs large virtual reservations, so this
helper has no address-space limit. Its profile is off the record, JavaScript is disabled, and
navigation to another page is refused. Only the page folder is bound as /page, read-only; a
content security policy restricts subresources to that folder and embedded data images/fonts.
The one writable bind is a private helper directory under the thumbnail cache. All helper
files are created exclusively. The confined input is bound over the page's own filename and
loaded from that URL, preserving relative URLs and the document's own base element.
The PNG is at least the requested viewport height and at most 8000 pixels tall, and cached by canonical
path, size, nanosecond mtime, inode, device, width and viewport height. A hit also requires every
engine-recorded local resource to retain its size, nanosecond mtime, inode and device. Records
hold at most 256 local resources and 64 KiB; an unreadable or incomplete record forces another render.
The companion cache uses the preview-cache bounds
of 4096 files and 512 MiB, with a 60-second reader grace and one scan per 32 publications.

### compare

`{"c":"compare","token":<uint>,"a":"<string>","b":"<string>"}`

Example: `{"c":"compare","token":3,"a":"/home/gm/notes-a.md","b":"/home/gm/notes-b.md"}`

The unified diff of two regular files a client actually named, never a sweep; `a` is the first of the
pair in the listing's order and `b` the second. Absolute paths and readable regular files are checked
first; a refusal answers `failed`. Either file over 1,048,576 bytes then answers `over`, judged on both
files before reading their contents. The bounded contents of each file are read in pair order: a read
failure answers `failed`, growth past the limit answers `over`, a NUL byte in its first 8,192 bytes
answers `binary`, and invalid UTF-8 anywhere answers `failed`. Code preview displays invalid UTF-8
lossily, but a copied patch must preserve bytes, so Compare refuses it.
Otherwise `/usr/bin/diff -u --label <a> --label <b> -- <a> <b>` runs once in the same read-only jail as
the other helpers, with the two files bound read-only and nothing writable, so the patch names the real
paths. The diff has a 2 s deadline; expiry kills its process tree and answers `failed`. Invalid UTF-8
in the diff output also answers `failed`, so the clipboard never receives replacement bytes.
Answers one `compare` line on a thread, and `token` echoes the request's so a client matches the
one it last sent and drops any other.

### pdfcopy

`{"c":"pdfcopy","id":<uint>,"slot":"<string>","path":"<string>"}`

Example: `{"c":"pdfcopy","id":3,"slot":"column","path":"/run/media/gm/128GB/doc.pdf"}`

Fetches one PDF the preview actually opened into a session-private copy under an 8 s deadline, and
answers one `pdfcopied` line: `{"t":"pdfcopied","id":3,"path":"<local copy>"}` on success, or
`{"t":"pdfcopied","id":3,"err":"that file is not responding","stalled":true}` when the wait
runs out. `err` rides only on a failure, and `stalled` only on the wait that ran out, so a client
never reads `err` to tell the two apart. The `id` is minted by the one counter in `ui/Backend.qml`,
so two viewers never share one; `slot` names the viewer, so a newer fetch supersedes only its own
viewer's copy.
The copy runs beside the loop, so a dead mount costs the viewer a sentence and
never the window; a stale reply is dropped by its `id`, the way a superseded listing's result is.

### officepdf

`{"c":"officepdf","id":<uint>,"path":"<string>"}`

Example: `{"c":"officepdf","id":4,"path":"/home/gm/Documents/report.docx"}`

Converts one DOCX, ODT, XLSX, ODS, PPTX or RTF file to a cached PDF with LibreOffice and answers
`officepdf` lines carrying the request `id`: `{"t":"officepdf","id":4,"state":"ready","path":"<cached pdf>"}`
when the PDF exists, `{"t":"officepdf","id":4,"state":"converting"}` when a conversion started, and
`{"t":"officepdf","id":4,"state":"failed","err":"<string>"}` when it cannot be had. A `converting`
line is always followed by one `ready` or `failed` line for the same `id` once the job ends, so
a client waits for that second line and treats `converting` as progress only. A request that
needs no conversion answers `ready` or `failed` at once and sends no follow-up.
`unavailable` rides on a `failed` line only when this file or this box will never convert:
`{"t":"officepdf","id":4,"state":"failed","err":"LibreOffice is not installed on this box.","unavailable":true}`.
A client that reads it keeps its existing preview and does not ask again; a `failed` line without
it (a converter error, a timeout, a request replaced by a newer one, a mount that is not
responding) may succeed on a later request, except that a converter error (an exit status or no
PDF) is remembered for that file until its modification time changes; a timeout is not.
The path is resolved and its cache entry looked up beside the loop under the mount deadline, so a
hung mount costs one `failed` line and never the window. One conversion runs at a time and the
newest request replaces the one queued behind it, whichever request finished its lookup first; a request older than the newest one started, running or already finished, is answered replaced at once and never converts.

### settags

`{"c":"settags","rows":[<uint>,...],"op":"<string>","tag":"<string>","listing":<uint>}`

Example: `{"c":"settags","rows":[2,17],"op":"toggle","tag":"Red","listing":7}`

Adds, removes or toggles one tag across those rows of the current listing, or clears all tags
with `"op":"clear"`, and answers one `settagged` line. A `toggle` is one decision for every row it
names, made from the attributes read before the first write: where every row already carries the
tag it is removed from all of them, otherwise it is added to all of them, so a set where only some
rows carry it never loses the tag from those rows and never ends half toggled. Flea's seven names are `Red`, `Orange`,
`Yellow`, `Green`, `Blue`, `Purple` and `Gray`, matched case-insensitively; a name Flea does
not know is kept untouched, never rewritten and never dropped. **Every file or none**: a
filesystem without user extended attributes refuses with an `error` line naming its filesystem
before the first write, each filesystem the rows sit on asked once, and a file that vanished or
a write that failed refuses all with exactly the files already written put back, so nothing is
half-applied. A row that is neither a file nor a folder is skipped and named in the settagged answer's skipped array while the other rows are still written, so only a real write failure rolls anything back. The change is one undo journal step per file it changed, restoring that file's
previous attribute exactly, absent included, and only onto the same file (its device and inode)
still holding the attribute the change wrote: a file replaced or retagged since is left in place
and named in the `error`, while the other files are restored. Redo writes only onto the attribute
undo left: a file retagged since is skipped and named, and a file replaced since refuses the redo. A `settags` is refused with `an operation is already running` while a
`transfer`, `trash`, `duplicate` or `link` holds the slot, though a local `rename` is not. Rows on local mounts
are written on the loop; when any row sits on a remote mount the write runs on a worker, and past
the single-call deadline it answers `slow` first and its `settagged` or `error` line later, the
same way a remote `rename` does.

### tagstates

`{"c":"tagstates","rows":[<uint>,...],"id":<uint>,"listing":<uint>}`

Example: `{"c":"tagstates","rows":[0,4000],"id":3,"listing":7}`

Reads which of Flea's seven tags those rows of the current listing carry and answers one
`tagstates` line with the same `id`. It reads only, so a stale `listing` is not refused; the answer
names the numbering its rows resolved in instead. The shipped client sends it when a menu opens over
a selection that reaches past the held window, whose rows it holds no tags for, and draws the Tags
flyout's checked and mixed marks from the answer. The read runs on its own thread, the way `shebang`
does, because an attribute read on a hung mount never returns; a file that cannot be read counts as
carrying no tag.

### undo

`{"c":"undo"}`

Reverses the most recent completed operation and answers one `undone` line naming which kind it was.
An empty journal answers an `error` line with `where` of `undo`, and so does a reversal that fails
removing what an operation created or restoring from the trash. A reversal that renames back goes
through the same call a `rename` does, so its failure answers `rename`, `rename-kept` or `rename-stranded` instead.

**The journal is the session journal under $XDG_RUNTIME_DIR/flea**, newest entry from any window, surviving a backend restart but not the session, with an in-memory fallback when the dir is refused. An entry too big for the file stores a small barrier, its op name only and no paths, and no window keeps the payload: the undo that claims it answers `That operation was too large to undo.`, the next undo continues with older entries, and a barrier never enters redo. A file written by a newer version is unavailable, never rewritten: the reader falls back to memory. Each kind reverses as follows: a rename or a move renames back (still
refusing to clobber, because something may occupy the old name by now), a copy or a duplicate removes
what that operation created, a merge is undone only in the window that made it and elsewhere refuses
rather than falling back to the whole folder, a link removes only the link it made, and a trash restores through
`gio trash --restore` using the URI captured
when it was trashed. A hard link whose source is gone or was replaced stays as the last name holding
those bytes. A transfer that replaced an item reverses both halves in that one step, newest
first: the incoming item is removed or moved back, and then the item it replaced is restored from the
trash to its name. A reversal that fails stops the rest and is spent, except a `Mode` step whose file
was replaced or whose mode changed since: that step is skipped with a note while the rest restore, and
redo skips the same way. So when the incoming half cannot
go (a partial folder copy with a file inside newer than its root) or the trash cannot be read back (a
`gio` with no `trash://` to list), the replaced item stays in the Trash, restorable from the Trash
browser rather than by `undo`. A partial `permissions` undo answers `N items were left in place` (`1 item was left in place` for one) and, when it restored any,
names how many it restored for redo, which replays the undone half before anything older. A `mkdir` removes the folder it made only while it is still empty: a folder the
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
nothing recording which half, except a `Mode` step, which is skipped with a note while the rest restore.

### jump

`{"c":"jump","id":<uint>,"ranking":<uint>,"favourites":[<string>,...],"recent":[<string>,...]}`

Example: `{"c":"jump","id":3,"favourites":["/home/gm/Projects"],"recent":["/home/gm/Pictures/screenshots/shot.png"]}`

A whole ask that follows a provisional one names it: `{"c":"jump","id":4,"ranking":3,"favourites":["/home/gm/Projects"],"recent":["/home/gm/Pictures/screenshots/shot.png"]}`.

The path bar's folder jump, asked **once per open of the bar**, never per keystroke: the client filters
the answer itself as the line changes. `favourites` are Flea's own favourites in their rail order, and
`recent` is the desktop's `recently-used.xbel` newest first, which the client reads with Qt's XML reader
because the backend has none, and keeps until the file changes. The backend adds the third source,
zoxide's ranking, from one `zoxide query --list --all --score`, and answers one `jumped` line from a thread, because zoxide is a subprocess
and a stat can block on a network mount. `id` is the client's own number for this open and comes back
on the answer, so an answer to an earlier open is told apart from this one's. `ranking` is optional (absent
or 0 names none): an open whose recent history is stale asks twice, a provisional ask with what the client
has kept and then the whole ask under a new `id`, and the whole ask's `ranking` is the provisional ask's `id`.
The backend keeps the zoxide ranking of the newest run that answered in time, with the `id` it answered, and
answers a `ranking` naming that id from it with no second zoxide run, so every open runs zoxide exactly once;
the cross-source dedup still happens in each answer. A `ranking` naming any other id, or one no run kept
(zoxide still running or past its limit, the ranking replaced by a newer ask), runs zoxide as an ask with none does.

**Nothing is written.** `--all` is what keeps zoxide from pruning its own database on a query Flea made:
without it zoxide deletes entries missing for 90 days and saves. zoxide is optional: absent, it is an
empty source and says nothing. A zoxide that runs past 2 s is killed and draws nothing, and its answer is
read up to 1 MiB and its first 1,000 rows. Only one zoxide runs at a time: an open while an earlier one is
still running draws the last ranking that answered in time rather than starting a second, and an
open with no ranking kept yet draws no zoxide. Each source is checked on a thread of its
own against one shared 1 s budget, so a stat wedged on a dead mount drops that source's rows from that one on
and never the other sources. A recent file is checked only once the folder holding it resolved, on that
same budget and under that same key rule, so a wedged recent file costs its own row and never the answer. A check still running past the budget of the open that started it is wedged, and
the next open skips what it names rather than wedge behind it: its whole mount when that mount is NFS, SMB,
9p, Ceph, AFS or FUSE (read lexically from `/proc/self/mountinfo`, never through the mount), its own path
anywhere else. A dead share therefore holds at most one thread per source for good, not more per open, and a
check merely slow inside its budget is run again rather than skipped.

### devs

`{"c":"devs","paths":[<string>,...],"id":<uint optional>}`

Example: `{"c":"devs","paths":["/","/home/gm","/home/gm/Documents"]}`

The device number of each path named, symlinks followed, so a path reached through a link names the volume the
link points at. The device is the number `listed`'s `v` already gives for the listing itself, which is what lets
a client compare the two without a table of mounts: a plain drop onto a path moves when its device equals the
drag's source device and copies otherwise. The client asks **once per listing** for the ancestors of the listed
path, the breadcrumb's drop targets, and **once per drag** for the visible folder rows of the neighbour columns,
whose peeked rows carry no device, and keeps the answers by path. Answers one `devs` line from a thread of its
own, because a stat on a mount that stopped answering never returns and the loop waits on nothing. A path on a
network or FUSE mount runs under the same 5 s bound a listing has, then the mount is marked stuck for 30 s and
answers 0 at once, so a dead share costs one thread and one wait, never one per ask. An absent `paths` asks nothing.
Rail hover requests include a positive `id`, echoed in the answer, so leaving or changing a row discards its late answer.

### kind

`{"c":"kind","paths":[<string>,...],"id":<uint optional>}`

Example: `{"c":"kind","paths":["/home/gm/Downloads","/home/gm/notes.txt"]}`

The file kind of each path named, symlinks followed, so a link names the kind of the directory it
points at. A directory answers `"dir"`, every other answer `"file"`, and a missing path, a relative
path or a mount that does not answer in time answers `""`, which no drop arms on: the FAVORITES
heading's pin drop asks this for the one path a drag carries, and pins only a directory. Answers one
`kind` line from a thread of its own and marks a stuck mount exactly as `devs` does, with the same
positive-`id` echo, so leaving the heading discards a late answer. An absent `paths` asks nothing.

### clipSet

`{"c":"clipSet","op":"copy"|"cut","paths":[<string>,...]}`

Owns the system clipboard for those files, so another Flea window (or Nautilus, or
Thunar) pastes what this one copied. `paths` are absolute; anything relative, holding a
NUL or climbing through `..` is refused with an `ok:false` clip line and nothing is
owned. The backend makes a token, spawns a detached `flea --clip-own` holding the same
binary, writes it the payload and waits up to 2 s for its `ready`. Runs off the request
thread, the way `clipGet` does, so an owner that never answers holds up no other request.
Answers one `clip` line with `op` of `set`; see `clip` below.

### clipGet

`{"c":"clipGet"}`

Reads the current system selection as files. Runs off the request thread, the way other
slow reads do, because a foreign source can take up to 2 s to close its pipe. The
owner's own token type is read first, then GNOME's copied-files shape, then the
uri-list with KDE's cut marker; an empty or text-only selection answers `none`. Answers
one `clip` line with `op` of `get`; see `clip` below.

### clipClear

`{"c":"clipClear","token":<string>}` or `{"c":"clipClear","cut":[<string>,...]}`

Clears the selection only while it still carries that token, the one `clipSet`
answered with or `clipGet` read from another Flea process. A live owner started by
this backend is withdrawn first. Otherwise, including when that owner has exited,
the token form uses the same token check, queued-selection drain and null selection
as `flea --clip clear`, so it can clear a selection another Flea process owns.
`cleared:true` means a live owner was signalled or the verified clear sent the null
selection. Both clear forms run on the clipboard mutation queue, off the request
thread. The `cut` form is a spent cut from another application, which
Nautilus and Thunar clear after pasting: it clears only when the current selection is
still a cut whose path list equals the given one exactly, same order, and never
clears a copy. The verified token clear, `cut` form and `flea --clip clear` check the
selection, then drain what the compositor queued behind it with one more round trip,
and send the null selection only while the checked offer is still the current one;
a newer copy answers `cleared:false`. The protocol has no compare-and-clear request,
so a copy made after that last round trip and before the null selection lands can
still be wiped. Answers one `clip` line with `op` of `clear`, saying whether it
cleared; see `clip` below.

### clipWatch

`{"c":"clipWatch"}`

Starts the clipboard watcher: one thread holding one data-control connection, reporting
every clipboard selection (never the primary one, so a text highlight changes nothing)
as files the same way `clipGet` reads them. Idempotent; the UI sends it
once at start and later ones answer nothing. No reply of its own: file selections arrive
as `clip` lines with `op` of `changed`, an empty or text-only selection as `none`, and
two identical selections in a row emit once. No compositor or manager ends the thread
after one `changed` `none` line carrying the error; a dropped connection reconnects at
most once a second, at most 5 times.

Flea's private `application/x-flea-clip` payload is `copy|cut TOKEN PID`, where TOKEN
is 32 hex characters and PID is the detached owner's process id. The older
`copy|cut TOKEN` payload remains readable but supplies no owner-exit trigger.
The two-field form was written only by 0.3.8 development builds, never by a release.
Such a build rejects the three-field payload and reads the selection through the GNOME
fallback, so the paths and the cut operation still arrive and only the token is lost.
Flea 0.3.7 and older read no file selection.
Every watching backend, including windows that did not start the owner, holds a
pidfd for a reported Flea selection's live owner. A pid already gone at report time
(pidfd open returns ESRCH or the pidfd is already readable) triggers the same single
guarded fresh read as a later exit. A missing pid, a live process whose executable
or arguments do not identify this Flea binary's `--clip-own`, or another pidfd open
failure supplies no trigger and no report.
The watcher uses one reusable waiter thread and replaces its pidfd wait slot when
the reported selection changes; dropping the watching connection closes the slot
and stops the waiter. No per-copy thread or descriptor accumulates.

An owner's exit triggers a fresh connection and the same selection-reading path
as `clipGet`. It is never evidence that the clipboard is empty: a clipboard manager
may still be re-serving that owner's token. The reading shares the watcher's
deduping emitter and is applied only if that owner's token is still the last
reported token and no newer selection event has arrived when the reading returns.
A watcher offer read captures the report generation when its selection event arrives
and is discarded if an exit read begun after that event reports while its bytes are
pending, so a delayed offer cannot restore an ended token.
Reading the same token emits nothing; a failed reading emits nothing
and is not retried. The spawning backend's reaper only reaps, with no separate
clipboard notification hook.

Hyprland gives data-control clients no event when a selection is emptied. The
pidfd trigger covers Flea owners; a selection owned by another program and emptied
on Hyprland reports nothing until the next selection. Paste always reads fresh,
so no file operation can act on that stale watcher state. See `clip` below.

### quit

`{"c":"quit"}`

Stops the read loop, then answers the thumbnail work already asked for: every queued and
running job is drained and reported before the process exits, because a worker inside a
child owns a temp file in the shared cache that only its own return publishes or removes.
Draining is bounded; see `thumbed` below. Apart from those `thumbed` lines, no response.

**A queued `dirsize` row is not drained.** It answers nothing that outlives this process, unlike
a thumbnail's shared on-disk cache, so a row still waiting when `quit` arrives is simply dropped;
the client that asked for it is going away too. A `collisions` question still being asked is dropped the
same way, which is why a scripted client waits for its line before sending `quit`.

## Responses

### listed

`{"t":"listed","n":<uint>,"read":<float>,"sort":<float>,"v":<uint>,"w":<bool>,"path":<string>}`

Example: `{"t":"listed","n":100000,"read":26.400,"sort":2.500,"v":56,"w":true,"path":"/home/gm"}`

`n` is the row count. `read` and `sort` are milliseconds, formatted to three decimal
places (`{:.3}`). Sent after a successful `list` and after a successful `sort`. `read` is
the scan plus any requested metadata pass after `list`, the metadata pass after a
`sort` by `size` or `mtime` (including its `date` alias), and `0.0` after a `sort` by
`name` or `kind`, which needs no metadata pass.

`v` is the listing directory's own filesystem id, sent once here rather than on every row because
every file in the directory shares it. A client compares it against the `v` of the directory row
being dropped on: equal is one volume and the drag moves, different is two and it copies. `v` is 0
when the directory could not be stat'd, and a client reads 0 as unknown and copies, because copying
where a move was meant is an annoyance and moving where a copy was meant loses the original.
`w` is whether this user can create or delete entries in that directory. A drag from a directory
with `w` false copies, because a move would have to delete the originals and cannot. A reply that
omits `w` is an older backend, and a client reads that the same as true.

`path` is the directory exactly as the `list` that made this listing spelled it, byte for byte, with a
trailing slash, a symlink or a `..` left unresolved, because a client drops any `listed` line whose
`path` differs from the one it asked for (`ui/js/Swap.js` `onListed`).

`changed` rides only on a `list` with `wantChanged` that re-read the directory
already listed, and on a `listpaths` that re-read the listing it already held, and names
added plus removed rows between the two scans, so a rename counts 2 against a net delta of 0.
A first listing and a navigation carry no `changed`, and a client reads a missing one as unknown rather than 0.

### rows

`{"t":"rows","start":<uint>,"rows":[{"n":<string>,"d":<bool>,"s":<uint>,"m":<int>,"p":<uint>,"i":<string>,"t":<bool>,"k":<uint>[,"l":<string>][,"v":<uint>][,"tg":[<string>,...]][,"a":true][,"x":false]},...],"kinds":[<string>,...],"ms":<float>,"listing":<uint>}`

Example:
`{"t":"rows","start":0,"rows":[{"n":"say \"hi\".txt","d":false,"s":12,"m":1787790423,"p":33188,"i":"text-x-generic","t":false,"k":0},{"n":"photos","d":true,"s":4096,"m":1787790424,"p":16877,"i":"folder","t":false,"k":1,"v":56}],"kinds":["Plain text document","Folder"],"ms":1.250,"listing":1}`

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

**`x:false` means the reported execute bits do not establish per-file executable status.** Only those
rows carry it; ordinary rows gain no wire bytes. The icon and executable name colour use
the same backend decision, while `p` stays the reported mode for permission display and
operations. A symlink's name still uses symlink colour, and a lifted name uses foreground.

**`l` is where a symlink points, and only a symlink row carries it.** It is the link's own bytes,
verbatim and unresolved, so a relative target stays relative and a broken link still names where it
points; a `readlink` that fails leaves the field off entirely, the same as any other row. The row it
sits on is the one that already pays a second stat for its icon, so the extra call lands on the rows
that were already the exception and on no others, and the scale fixture holds no links at all. A row
never carries both `l` and `v`: `d` is the link's own type, so a symlink's `d` is `false` and it is
never the directory row `v` is for. The client draws the target beside the name and the word `link`
in the size column, because a link's own `st_size` is the length of that target path and not a size
anyone means.

**`tg` names the file tags Dolphin reads, and only a tagged row carries it.** It is the
`user.xdg.tags` value filtered to names Flea knows, in the stored order and canonical case, so a
row draws its swatches with no call per row; an untagged row carries no `tg` at all, and a
foreign word stays untouched and shows in Properties instead. One `getxattr` per emitted row
reads it, which is exactly the rows a window stats and never a sweep; on a network or phone
mount no row carries it, because each read there would be a round trip.

**`a` marks a thumbnailable AppImage, and only that row carries it.** Its own icon is its identity, so
the Images thumbnail mode draws it while a row with any other executable icon, such as a Windows
installer a third-party thumbnailer handles, stays a mark; every other row carries no `a` at all.

**`v` is the row's filesystem id, and only a directory row carries it.** A drop destination is
always a directory, so a file row's device would never be read, and the scale fixture is 100,000
files with no directories at all: sending it per row would be paid 100,000 times on the one path
that produces the headline listing number, for a field nothing reads. A client compares the `v` of
the rows being dragged against the `v` of the directory being dropped on: equal means one volume and
the drag moves, different means two and it copies, which is what Finder does. A directory whose stat
failed reports `p` 0 and `v` 0 with it. `ms` is the phase-2 stat time for this window, formatted to three
decimal places. Sent after `list` and after `window`. The last field is `listing`, the numbering these
rows are in; see "listing".

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

### settagged

`{"t":"settagged","ok":<uint>,"op":"<string>"[, "skipped":["<name>",...]]}`

Example: `{"t":"settagged","ok":2,"op":"remove"}`

Answers one `settags` request. `ok` is the number of files whose attribute changed; a request
that changed nothing answers 0 and records nothing to undo. `op` is what was applied, `add`,
`remove` or `clear`, so a `toggle` answers the direction it resolved to, and the client words its
status line from it. `skipped` is present only when a row was skipped, naming the rows that were neither a file nor a folder, so the client words that line instead. A refusal answers an `error`
line with `where` of `settags` instead, and nothing was written.

### tagstates

`{"t":"tagstates","id":<uint>,"applied":["<name>",...],"mixed":["<name>",...],"listing":<uint>}`

Example: `{"t":"tagstates","id":3,"applied":["Red"],"mixed":["Blue"],"listing":7}`

The answer to one `tagstates` request, carrying its `id` back. `applied` names, in menu order, the
tags every row carries and `mixed` the tags only some rows carry; a tag no row carries is in neither.
A name Flea does not know is never listed. `listing` is the numbering the rows resolved in, so a
client drops an answer whose numbering is not the one it asked in.

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

### extractstarted

`{"t":"extractstarted","id":<uint>}`

An archive `extract` sends this line before its one zero-byte `transferprogress` line. A current client
routes it into the existing transfer card as `n` 1, `moving` false and `extract` true, so the card says
Extracting without inventing bytes, a rate or an ETA. Older clients ignore this new type, while the
existing `archivedone` terminal line remains unchanged for archive clients.

### meta

`{"t":"meta","row":<uint>,"w":<uint>,"h":<uint>,"orient":<uint>,"ms":<uint>,"rate":<uint>,"entries":<uint>,"unpacked":<uint>,"afailed":<bool>,"names":[{"n":"<string>","d":<bool>},...],"lines":<uint>,"partial":<bool>,"lfailed":<bool>,"frames":<uint>,"photo":<object|null>,"target":"<string>","targetdir":<bool>,"raw":{"file":"<string>","w":<uint>,"h":<uint>,"orient":<uint>}|null,"owner":"<string>"}`

Example: `{"t":"meta","row":4,"w":0,"h":0,"orient":1,"ms":0,"rate":0,"entries":214,"unpacked":3400,"afailed":false,"names":[{"n":"ui","d":true}],"lines":0,"partial":false,"lfailed":false,"frames":0,"photo":null,"target":"","targetdir":false,"raw":null,"owner":"gm"}`

Every field a row did not ask for is zero, false or empty. `w` and `h` are pixels, read from the
file's own header, as stored and before any turn. `ms` and `rate` come from the probe. `lines` is a
newline count with `partial` true when it stopped at its 1 MiB budget, so the column states it as a floor.

`photo` is the camera file's EXIF facts as stored, from the same RAW walk and in the same reply: `{"make":"Canon","model":"Canon EOS R6","lens":"RF35mm F1.8 MACRO IS STM","exp":[1,250],"fnum":[28,10],"iso":400,"focal":[35,1],"taken":"2026:09:12 18:31:00","w":6000,"h":4000}`, where `exp`, `fnum` and `focal` are numerator and denominator pairs, `w` and `h` are the sensor size from the EXIF pixel dimensions (sent together or not at all) and an absent fact is left out. It is `null` for every row that is not a RAW and for a RAW whose walk names none of them; a JPEG's facts are never in this reply. The `properties` reply of `menuaction` carries the same `photo` object, and for a `.jpg` or `.jpeg` it is read from the first `Exif` APP1 within the file's first 64 KiB.

`orient` is the EXIF orientation, 1 to 8, from the same read of the same open. The walk that finds a
JPEG's frame header reads the head of its first `Exif` APP1 on the way past. It is 1 for every file that
names none, every non-JPEG included. Orientations 5 to 8 swap the sides, and the Columns frame and Quick
Look ask Qt for their decode size turned the same way, because Qt fits its decode before it turns.

`entries` and `unpacked` are **exact totals whenever they are non-zero**, because the index is
streamed and counting all of it costs no memory. `names` is capped at the first `ARCHIVE_NAME_CAP`
entries, which is the only part that is bounded: the tile lists those and states the difference as
its own "+ N more" line.

`lfailed` is true when the row produced no count at all, which on this box
means permission denied, a row that is not a regular file, or a row that vanished between the
listing and the request. **Nothing but a regular file is ever read here or for `w` and `h`**: a
`stat` refuses every other kind before any open, so a FIFO that `stat` sees is never opened at all
and its waiting writer is left where it was, and the open behind that stat is `O_NONBLOCK` with a
second `fstat` on the descriptor, so a row swapped for a FIFO inside that window is closed again
instead of leaving the row waiting forever. That one open does wake a writer parked in `open(2)`, and
closing it unread hands that writer an `EPIPE` on its next write if nothing else is reading: it is
the cost of refusing a swap the `stat` cannot see, not a case the `stat` avoids. A FIFO, a socket, a
device or a directory answers its `stat` facts with no dimensions and no count. It is what tells
`lines` 0 apart from an empty file, whose `lines` is also 0: zero is a real count, so
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

`raw` is an object for a camera RAW whose largest embedded JPEG preview was copied out, and `null` for every
other file and for a RAW holding none. `file` is the copy under `$XDG_CACHE_HOME/flea/raw/`, named by the md5
of the RAW's file URI and carrying the RAW's mtime, so a changed RAW is copied again and an unchanged one is
neither walked nor written twice.
The directory is held to 4096 copies and 512 MiB, least recently used first (by ctime, since a copy's mtime is its RAW's:
a cache hit sets the copy's atime to now, which moves the ctime), and sweeps temps a minute old. A copy made or served less
than a minute ago is never evicted, because the thread that has it is still reading it. Each process scans on its own first
new copy and then on one in 32 of its own, and the directory is shared, so it can stand over its caps: by the copies of the
last minute, which stay until the first scan after they age, and by fewer than 32 more per process. `w`, `h` and `orient` read like
the top-level fields, taken from the copy: stored
pixels and the EXIF turn, which the copy carries even when the camera put it only in the RAW's IFD0. The top-level
`w` and `h` stay 0 for a RAW, because they name the file's own pixels and the preview is smaller.

`owner` is the login name of the user who owns the path itself (a symlink reports the link's owner, as
its row does), resolved from `/etc/passwd` alone and never through `getpwuid`: that call goes through
NSS and can wait on a network directory, and the meta thread must never hang the column on one. A uid
no local account carries answers the empty string, never the number dressed as a name.

### shebang

`{"t":"shebang","path":"<string>","hasShebang":<bool>,"id":<uint>}`

Example: `{"t":"shebang","path":"/home/gm/run.sh","hasShebang":true,"id":3}`

The answer to one `shebang` request, carrying its `id` back. `hasShebang` is true only when the
path opened as a regular file and its first two bytes were `#!`; anything else, a directory, a
FIFO, a missing path or an unreadable file, answers false. The open is the backend's own
regular-file open, so a FIFO is refused before any open and a row swapped for one inside that
window is closed again, the same rule `meta` follows.

### code

`{"t":"code","token":<uint>,"slot":"<string>","state":"<string>","first":<uint>,"total":<uint>,"lines":[[<role>,"<string>",...],...][,"records":<uint>,"firstRecord":<uint>]}`

Example: `{"t":"code","token":4,"slot":"look","state":"styled","first":1,"total":2,"lines":[[5,"fn",0," main"],[]]}`

The answer to one `code` request, echoing its `token` and `slot`. `first` is the actual first
line returned: normally the asked line, or the record start for a raw `csv` table request.
`state` is `styled`
(`bat` coloured the lines), `plain` (the lines are the file's own text, every role 0: a line over
5,000 bytes, or no `bat` on the host), `binary` (a NUL in the first 1,024 bytes), `large` (over
1,048,576 bytes) or `failed` (unreadable, or `bat` refused it); the last three carry no lines.
`total` is the line count of the whole text. Each entry of `lines` is one line as a flat run list,
a role from 0 to 6 then the run's text, in order, an empty line being an empty list; the lines
returned are those asked for that exist, extended to whole records for a raw `csv` table
request. `total` remains the physical-line count in either case. A plain raw `csv` reply
also carries `records`, the whole file's record count including its header, and `firstRecord`,
the ordinal of the first returned record. An unterminated quote at EOF still counts as one
record. An empty file has zero records; an empty reply beyond EOF has the requested record
ordinal, or the next ordinal after the file for a physical-line ask. Every other reader's
reply omits these two fields. The roles are the foreground (0) and bat's ANSI colours
31 to 36 in order: red, green, yellow, blue, magenta, cyan. A one-line `.json` file is formatted
with `jq` first, so its `total` and lines are those of the formatted text.

### html

`{"t":"html","token":<uint>,"slot":"<string>","state":"<string>","file":"<PNG path>","total":<uint>,"width":<uint>,"title":"<string>"}`

Example: `{"t":"html","token":3,"slot":"look","state":"ready","file":"/cache/thumbnails/flea-html/a.png","total":61,"width":984,"title":"Local page"}`.

The answer to the latest request in one HTML slot. State is `ready`, `failed`, or `cancelled`;
only `ready` names a PNG and a whole-file line count. Failure uses the existing unreadable-file
frame. Width is the render width, not a CSS zoom. Title is the helper's observed static page
title on a new render and empty on a cache hit. The client matches token and slot, and drops
replies after closing or switching to Source.

### compare

`{"t":"compare","token":<uint>,"state":"<string>","patch":"<string>"}`

Example: `{"t":"compare","token":3,"state":"diff","patch":"--- /a\n+++ /b\n@@ -1 +1 @@\n-x\n+y\n"}`

The answer to one `compare` request, echoing its `token`. `state` is `diff` (`diff` exited 1 and `patch`
holds what it printed, headers included), `identical` (it exited 0), `binary` (screening found a NUL in
a file's first 8,192 bytes), `over` (either file is over 1,048,576 bytes) or `failed` (a path
that cannot be read as a regular file, invalid UTF-8 in a file or the patch, any other exit, `diff` or
the jail not starting, or a patch past 8 MiB). `patch` is the empty string in every state but `diff`.

Screening first requires both absolute paths to open as regular files with readable lengths;
either initial length over 1,048,576 bytes returns `over`. It then reads `a`, followed by `b`, up to
1,048,577 bytes each, checking read failure (`failed`), growth past the limit (`over`), a NUL in the
first 8,192 bytes (`binary`), then UTF-8 validity (`failed`), stopping at the first refusal. An earlier
file's `failed` or `over` therefore takes precedence over a later file's NUL.

### collisions

`{"t":"collisions","id":<uint>,"total":<uint>,"names":[{"n":"<string>","d":<bool>,"i":"<string>"},...],"mergeable":<bool>}`

Example: `{"t":"collisions","id":7,"total":2,"names":[{"n":"screenshot.png","d":false,"i":"image-x-generic"},{"n":"notes","d":true,"i":"folder"}],"mergeable":true}`

`total` counts every colliding source. `names` is the first three of them in request order, which is
all a client's card lists before its "and N more" line, so the answer stays small however wide the
selection. `n` is the incoming item's name, `d` whether it is a directory and `i` the same icon name a
`rows` row carries, for the card's kind mark. `mergeable` is true when a folder lands on a folder,
which is the only shape the card offers `merge` for. A `total` of 0 means nothing collides; the shipped client
then sends its transfer with `collide` `refuse`, so a name that appears in the meantime is still refused.

### fatnames

`{"t":"fatnames","id":<uint>,"fstype":"<string>","volume":"<string>","renames":[{"old":"<string>","new":"<string>","d":<bool>,"i":"<string>"},...]}`

Example: `{"t":"fatnames","id":7,"fstype":"exfat","volume":"128GB","renames":[{"old":"clip: final.mp4","new":"clip_ final.mp4","d":false,"i":"video-x-generic"}]}`

Answers a `collisions` question that carried no `fat` for a FAT-family destination, with the same
`id`, in place of the `collisions` line. `fstype` is the mount table's own word and `volume` the
owning mount's folder name, so the card names the drive from fields and never from a sentence.
`renames` lists every source whose name the filesystem refuses, in request order, `old` as it is and
`new` as Rename and copy would land it; `d` and `i` are the `rows` row's directory bit and icon name.

### mergecollisions

`{"t":"mergecollisions","id":<uint>,"total":<uint>,"names":[{"n":"<string>","d":<bool>,"i":"<string>"},...]}`

Example: `{"t":"mergecollisions","id":8,"total":3,"names":[{"n":"IMG_4120.HEIC","d":false,"i":"image-x-generic"}]}`

`total` counts every file-level name both sides hold; folders that merge further down are not
listed because they need no answer. `names` is the first three in walk order with the same `n`,
`d` and `i` the card draws, `n` relative to the merging folder when one folder merges and to the
destination when several do. A `total` of 0 sends no second card: the transfer goes out at once
with `inner` `refuse`.

### clip

`{"t":"clip","op":"set","ok":<bool>,"token":<string>}`
`{"t":"clip","op":"get","ok":<bool>,"clip":"copy"|"cut"|"none","paths":[<string>,...],"token":<string>,"skipped":<uint>}`
`{"t":"clip","op":"clear","ok":<bool>,"cleared":<bool>}`
`{"t":"clip","op":"changed","clip":"copy"|"cut"|"none","paths":[<string>,...],"token":<string>,"skipped":<uint>}`

One shape for all three clipboard requests, read by `clipResult`. A refusal carries
`ok:false` and an `error` sentence instead of the success fields. A `get` with no
files on the clipboard answers `clip:none` with empty `paths` and `token`; `skipped`
counts the offered URIs that were refused (bad escapes, NUL, relative paths, `.` or
`..` parts, non-UTF-8, duplicates, other schemes) rather than failing the read. The
`token` on a `get` names Flea's own copy when the selection carries it, which is what
a later `clipClear` hands back; a foreign selection answers an empty one. `changed`
is the watcher's line and carries no `ok`: it reports the selection as files, with the
same fields as `get`, and a failed watch reports `none` with an `error` sentence.
A selection event supersedes any exit read begun before it, while an exit read begun after that event invalidates its pending watcher bytes only if it reports, with both guards ordered by sequences under the emitter lock.


### transferprogress

`{"t":"transferprogress","id":<uint>,"index":<uint>,"name":"<string>","bytes":<uint>,"total":<uint>[,"phase":"writing"]}`

Example: `{"t":"transferprogress","id":12,"index":0,"name":"a.txt","bytes":40000000,"total":120000000}`

Throttled to one window per 150 ms for the whole operation, not per item: the first sample goes out
at once and a run of small files shares one window, and a window sends the item's own line with at most
one owed line beside it (below), so a fast copy of a thousand small files sends a few
lines (plus one `transferitem` per item, below), and a single large file sends one per window as it
always did. A thousand small files once sent a progress line and an item line each, so the card model
was written 2002 times; the throttle cuts the progress lines from 1000 to 2 and the writes to 1004. An item
that sends no line of its own is correct and not a missing message.

**Bytes ride on the next line, never get lost.** A client counts an item's bytes from the last sample it
saw for that item (`ui/js/Transfer.js` `itemDone`), so the bytes of items that sent no line go out once, ahead
of the next item's first line: a sample with `total` 0 under the `index` and `name` of the last item to end,
which the client has already closed, so it joins the running sum and moves nothing on the card. The next item
then sends its own `bytes` and `total`, so a large file keeps its size on every line. A closing sample of the
same shape follows the last item when anything is still unreported, ahead of the drive flush below. What the
card draws is then the figure an unthrottled run draws, to the byte, whatever the
throttle skipped; an item that ends short (a failure or a cancel) counts as the total its last line
named, as it always did. Only a copy or a same-device move sent by the transfer loop paces this way; a
cross-device move batch and a folder merge keep their own per-item windows.

**Only a regular file reports bytes.** A directory has no total without the sweep this codebase does not
do, and a same-filesystem move is a single `rename(2)` with nothing to report partway through, so
neither emits these lines at all. A client renders an item with no progress as indeterminate.
On a durable target (usb, phone, network, or vfat/exfat/ntfs) those bytes are reported only after
the drive confirms them, so the rate is the drive's real rate.

**The final phase rides one more of these lines.** After the last file, the transfer emits
`{"t":"transferprogress","id":<uint>,"index":0,"name":"","bytes":0,"total":0,"scanned":0,"phase":"writing","drive":"<string>"}`
while it fsyncs every directory it created or wrote into. Cancel is not honoured there because
every file is already complete. `drive` is the destination's own name, so the card's headline
names where the flush is going without guessing from timing. Older clients ignore the fields
they never asked for.

**An extract's one progress line is not a transfer's.** It carries the archive's own file name as
`name` with `bytes` and `total` both 0: the unpacking tool streams no per-item bytes, so any figure for
it would be invented and no percentage or time left is offered for it.

### transferitem

`{"t":"transferitem","id":<uint>,"index":<uint>,"name":"<string>","ok":<bool>,"err":"<string>"}`

Example: `{"t":"transferitem","id":12,"index":0,"name":"a.txt","ok":true}`
Example: `{"t":"transferitem","id":12,"index":1,"name":"photos","ok":false,"err":"permission denied"}`

Exactly one per top-level item the transfer starts, in the order the items were named; an item it
never starts, one a cancel reached first, one a `skip` left or a move already in place, answers none. **`err` rides only on a failure**, so
a successful item's line carries no empty field to reason about, and a permission error on one file is
that item's data rather than the operation's: the batch carries on to the next item. What a failed item
had already written stays where it is and is journaled as that operation's own creation, so an `undo`
removes it; only a cancel removes its partial itself, see `transfercancel`.

### transferdone

`{"t":"transferdone","id":<uint>,"ok":<uint>,"failed":<uint>,"skipped":<uint>,"cancelled":<bool>,"retryPaths":[<string>,...],"durable":<bool>,"note":"<string>"[,"merged":<uint>,"keptBoth":<uint>,"into":"<string>"][,"refused":[{"name":"<string>","err":"<string>"}]]}`

Example: `{"t":"transferdone","id":12,"ok":1,"failed":1,"skipped":0,"cancelled":false,"retryPaths":[],"durable":false,"note":""}`
Example: `{"t":"transferdone","id":13,"ok":0,"failed":1,"skipped":0,"cancelled":false,"retryPaths":["/home/a/thesis"],"durable":true,"note":"","refused":[{"name":"thesis/refs/paper.pdf","err":"this drive cannot hold links"}]}`

The whole operation's terminal line. `skipped` counts the items the transfer did not start by design:
those a cancel reached before they started, the colliding items a `collide` of `skip` left in place,
and an item a `collide` move would have put back where it already is. A cancelled `replace` whose
put-back failed is counted in `failed` instead, see `transfer`. `cancelled` is true when a
`transfercancel`, a `quit` or stdin closing ended it early. `durable` is true only when the
destination needed durability and every flush succeeded, so the UI can say "written to the drive";
local targets report false and never claim it, and so do rclone ones, which skip the flush by design.
`note` carries the sentence the UI prints beside the done count: `copied, but the drive did not
confirm the folder` when the files landed but a folder flush did not, and `rclone uploads them in
the background` for a copy onto an rclone mount, and is empty otherwise. `refused` rides only when an item
failed for entries a drive refused inside its tree, see "An entry the destination cannot hold" above: one
object per entry, its path under `dest` and its reason, in the order the walk met them and at most 100 of
them across the transfer; that item's own `err` carries its full count. An item that ended on any other
error adds nothing to `refused`. A merge carries two more
counts, `merged` for every entry it landed (the inner entries of folded folders and every other
landed item, including ones that waited on a move batch) and `keptBoth` for the landed entries its
answer kept both, top-level and inner alike, and `into` naming the one merging folder, or the
destination several folders folded into, so the status line needs no client state; older clients
ignore the fields they never asked for. A merge's `refused`, when present, follows `into`.

### flashstarted

`{"t":"flashstarted","id":<uint>,"total":<uint>,"known":<bool>}`

The flash worker's first line, answering what will be written: `total` is the unpacked size
when `known` is true and the file's own size otherwise. Every `flashprogress` and the one
`flashdone` behind it carry the same `id`.

### flashprogress

`{"t":"flashprogress","id":<uint>,"phase":"prepare|password|write|flush|verify","bytes":<uint>,"total":<uint>,"permille":<uint>}`

At most one line per 150 ms, the transfer rule, whatever the drive answers. `password` runs
from the helper's spawn to its descriptor, so the card says "waiting for your password";
`flush` covers the final `fsync`, during which Cancel is dimmed, and `prepare` precedes
the password prompt while the drive is re-read. See `flash` for what
`bytes` and `permille` count in each phase.

### flashdone

`{"t":"flashdone","id":<uint>,"ok":<bool>,"phase":"<string>","at":<uint>,"err":"<string>","blank":<bool>}`

The flash worker's one terminal line, which releases the backend's flash slot. `phase`
names where it ended: `verify` on success, the failing phase otherwise. `at` is the first
differing offset on a `verify` failure and 0 elsewhere. See `flash` for every `err` and
what `blank` promises about the drive.

### flasheject

`{"t":"flasheject","ok":<bool>,"err":"busy|gone|changed|error"}`

The helper's eject verdict: `busy` keeps the rail's shipped "still mounted" sentence,
`gone` names a drive that left, `changed` one that changed since it was chosen, and
`error` everything else.

### trashed

`{"t":"trashed","ok":<uint>,"failed":<uint>}`

Example: `{"t":"trashed","ok":1,"failed":0}`

Counts, plus the batch's own reason when anything failed: `{"t":"trashed","ok":0,"failed":1,"err":"<string>"}`.
Unlike `transferitem` there is no per-path error text, because trash is one `gio` call for
the batch and its exit status cannot attribute a failure to a single path; a path that is still on disk
afterwards is counted in `failed`. `err` rides only on a failure, so a successful line is byte-identical
to before. `gio` runs under a 10 s deadline, so a hung mount answers "The Trash took too long to answer"
instead of holding the single operation slot.
A batch that ran `gio` sends the line once `gio` has returned and each path has been checked on disk (`lstat`), whether
or not every path went, so a failure goes out the same way with its `failed` count and `err`. It is sent before the second
`gio trash --list`, which only names the undo journal's URIs. The operation slot is freed with that line: the next write
(a second `trash` included) is accepted at once, while `undo` and `redo` answer busy naming the batch's first path until
the list lands and the batch's undo entry is journaled, so an undo never reverses an earlier entry in its place. Entries
journal in the order operations were accepted, so a write accepted while the list is out lands after the trash. A batch
that ran no `gio` (every path renamed into the home Trash, or none present) sends its line once, with its journal entry.
The line is sent once per batch.

### slow

`{"t":"slow","op":"<string>","path":"<string>","msg":"<string>"}`

Example: `{"t":"slow","op":"rename","path":"/hung/a.txt","msg":"/hung is slow. The rename will finish on its own."}`

A remote `rename`, `mkdir`, `bulkrename` or `settags` past the single-call deadline answers this line and moves
on: the write stays running on its own worker, so the loop keeps answering every other request,
and the mount is never marked stuck for a write still running. `op` names the request (`rename`,
`mkdir`, `bulkrename` or `settags`), `path` the path it is writing (the rename source, the mkdir parent, the
bulk batch's folder or the folder of the first remote row a settags names), and `msg` the sentence the client shows as information, never as an error. The
`slow` line releases the one-at-a-time slot at once, so writes on other mounts and on local paths
still run; the write stays pending under its mount root instead. A write touching a mount with a
pending slow write answers `an operation is already running` and journals nothing, and `undo` and
`redo` answer the same while any slow write is pending, naming its path. The late reply clears
only its own pending entry. There is still no cancel id and no progress for the running write.
The write's own reply follows whenever it lands: the same `renamed`, `made`, `bulkrenamed` or `settagged` line the
in-time path writes, or the same `error` line if the write failed, journalled exactly once either
way, so one `undo` reverses it.

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

`op` is the kind of operation that was reversed, one of `rename`, `duplicate`, `mkdir`, `trash`, `copy`,
`move` or `merge`, which is what lets the status bar say what it just put back. A copy merge journals
`merge`, and the bar words its undo whole: "Undid the merge." Its redo refuses instead, since the
pre-existing folder is still there. A move merge journals `move`, so its redo reads as a move.

### jumped

`{"t":"jumped","id":<uint>,"favourites":[<string>,...],"zoxide":[<string>,...],"recent":[<string>,...],"frecency":{<string>:<float>,...},"ms":<float>}`

Example: `{"t":"jumped","id":3,"favourites":["/home/gm/Projects"],"zoxide":["/home/gm/Documents"],"recent":["/home/gm/Pictures/screenshots"],"frecency":{"/home/gm/Documents":80},"ms":4.210}`

The answer to one `jump`: the folders of each source that exist now, each source in its own order.
A favourite or a zoxide row must itself be a directory; a recent entry stands for the folder it sits in,
unless it is a folder itself. A folder appears once, in the first source that names it, so a favourite
zoxide also ranks is drawn as the favourite. `frecency` is zoxide's score for every folder answered that
zoxide ranks, whichever source draws it, and the client ranks the one list by it after the match itself.
Anything that is not an absolute path is dropped, and so is a score that is not a finite number. `ms` is
the whole answer's time, zoxide and the existence checks together.

### devs

`{"t":"devs","devs":{<string>:<uint>,...},"id":<uint optional>}`

Example: `{"t":"devs","devs":{"/":66305,"/home":66306,"/home/gm/gone":0}}`

The answer to one `devs`: each asked path once, in the order asked, with its device number. A path that cannot
be read, a missing one, an empty one, a relative one (it would name the backend's own directory, which the client
never means) and one on a mount that is not answering all answer 0, and the client reads 0 as unknown and copies:
copying where a move was possible is an annoyance, and moving across volumes loses the original. The line carries
the request's positive `id` when supplied; the rail accepts only the current hover's id and path. Untagged answers
continue to fill the breadcrumb and column device map, while tagged hover answers never enter that cache.

### changed

`{"t":"changed","path":"<string>"}`

Example: `{"t":"changed","path":"/home/gm/Downloads"}`

**The one line no request asks for.** Every other response answers a request; this one says the
directory the current listing came from is no longer what `list` answered with, because another
program created, deleted, renamed, wrote or chmod'd something in it. Nothing the backend holds
changes with it: the rows, the count and the sort order are exactly what they were, and a client
that wants the new directory sends `list` again. `path` is the watched directory, so a client that
has navigated since the notification was written can tell it is not about the folder it is on now,
and drop it.

A `list` starts watching its path **before it reads the directory**, not after, because a change
landing while the read runs is missing from the rows that `list` is about to answer with and is
therefore exactly the change the client has to be told about. It is armed **beside** the watch the
client is already on rather than in place of it, so a `list` that then fails to scan costs the
directory still listed nothing at all: its watch was never removed, and the descriptor its own
events carry is still the current one. `search` and `listpaths` both stop watching, because a set of
matches and a set of named paths are not directories.

**A column's directory is watched too, and answers this same line.** A `peek` that carries
`"watch":true`, which only the columns view sends, arms a non-recursive watch on its directory before
it scans, on a second inotify descriptor of its own so that removing one can never remove the listed
folder's watch. The watch fires the `changed` line with `path` naming the peeked directory, and the
client re-asks that column; a client on another path drops the line as it always did. Only the
directories the latest watching `peek` names in `"keep"` stay watched, and a peek without the flag watches nothing.
`"keep":[...]` is every column directory the client draws, the peeked one among them: the backend unwatches every
other peeked directory before it arms this one, and unwatches at once a directory armed after the client moved on.
Eight directories is a guard for a client that sends no `keep`, the oldest dropped first, and a directory the
kernel stops watching (deleted, unmounted) leaves the set after its last `changed` line.

The mechanism is one inotify watch on that one directory, non-recursive, with the mask
`IN_ATTRIB | IN_CLOSE_WRITE | IN_MOVED_FROM | IN_MOVED_TO | IN_CREATE | IN_DELETE | IN_MOVE_SELF`,
which is exactly the set of events that changes what a listing says: which names are in it, and the
size, date and mode its columns draw. Deleting the watched directory needs no bit of its own: the
kernel removes the watch along with it and reports that removal whatever the mask holds.
**A file growing under an open handle is not one of them.** `IN_MODIFY` fires on every `write(2)`
and a listing does not draw a partial size, so a row's size follows the writer closing the file
rather than the writer writing to it.

**One burst is one line.** The event payload is read only far enough to name its watch descriptor,
never for which file moved, and the reader then pauses 100 ms before reading again, so a directory
being rewritten costs one `changed` line per 100 ms rather than one per file. Anything the kernel
drops in that window costs nothing, because every event in a burst says the same thing to a client
that re-reads the whole directory anyway.

`--backend` on a box whose inotify instance limit is exhausted prints one sentence to stderr at
startup and then never sends this line, and a `list` whose directory the kernel refuses a watch on
(`max_user_watches`, most often) prints one naming that directory; every other request answers
exactly as before. A client must therefore treat a live listing as an improvement it may not get,
not as a guarantee. A
network mount is the other case: inotify sees the local kernel's own view of a directory, so a
change another machine makes to an NFS or SMB share is not delivered, and Flea is stale there in
exactly the way it was before this line existed.

### error

`{"t":"error","where":"<string>","path":"<string>","msg":"<string>"[,"mode":<uint>][,"code":"<string>"]}`

Example: `{"t":"error","where":"scan","path":"/root","msg":"permission denied","mode":16872}`

`where` names the failing operation, `path` names the input that failed, `msg` is the
underlying message. `code` rides only on a refusal the UI words itself (`emptyclipboard`, from `pasteimage`). Over `--backend`'s stdout, `where` is `scan` (a `list` whose path
failed to read), `sort` (a `sort` whose key names no order this wire defines; `path`
carries the key as sent), `window` (a remote metadata window whose mount did not
respond or whose worker stopped; `path` matches the current listing header's `path`
byte for byte, including its original spelling), `stale` (a row-indexed request naming
a numbering the listing has left; `path` carries the command, see "listing"), `flash` (a second
flash while one runs; `path` carries the device), or `read` (the
stdin stream itself could not be decoded; the loop stops right after emitting this
line, because the framing cannot be trusted past that point; thumbnail work already
running is still drained after it, so a `thumbed` line can follow). The write
operations answer over the same stdout and name themselves the same way, and
`rename-kept` and `rename-stranded` (see `rename`) are the only values on this wire that are not one lowercase
word, so a client matching this field must allow the hyphen. `error_line`
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

`peek`, `paths`, `archive`, `convert` and `formats` are on the wire and are not documented
here yet. `tools/flea-acceptance` derives its checklist from this file, so each one is a gap in that
battery until its section is written.

The `formats` and `archive` parts of that set are named here exactly, because they changed. A `formats` reply's `extract`
object carries one bit per extension class: `{"archive":<bool>,"sevenZip":<bool>,"zip":<bool>}`, where
`archive` is the tar class on `bsdtar`, `sevenZip` is the `.7z` and package (`.deb`, `.rpm`, `.apk`, `.jar`,
`.cab`, `.cpio`, `.AppImage`) class on `7z`, and `zip` is the
`.zip`/`.rar` class served by whichever of those two is installed. The client offers Extract on a package only
when `sevenZip` is set; the backend's own listing of a package still falls back to `bsdtar` where `7z` is missing,
as v0.3.8 did, except an `.AppImage`, whose squashfs image only `7z` reads. An `archive` request with an `op` of
`extract` takes the one-at-a-time slot `transfer` describes, drives that same transfer card and answers
`archivedone`, never `transferdone`; a `transfercancel` naming its `id` cancels it, and it is not
journaled, so no `undo` reverses it.

An `archive` request is `{"c":"archive","op":"compress"|"extract","paths":[<string>,...],"path":"<string>","dest":"<string>","format":"<string>","password":"<string>","encryptNames":<bool>}`.
A `compress` names absolute `paths` sharing one parent and the `format` to write; an `extract` names the
one archive in `path`. `password` is empty for a plain operation: only a 7z compress or an extract that
7z reads (a `.zip` too, where 7z is installed) takes a non-empty one, and any other reader answers `err` of `"only an archive that 7-Zip reads
can take a password"` without piping the secret. The secret travels this stdin wire and leaves it only on
7z's own stdin pipe, never in argv, an environment variable, a log or `/proc/<pid>/cmdline`. A compress
argv carries a bare `-p` (7z then reads one stdin line), an extract carries none (7z reads a bare `-p`
as an empty password). `encryptNames` hides the entry names of a password `.7z` (`-mhe=on`) and is
ignored for `.zip`, which a password writes as AES-256 (`-mem=AES256`). Absent `password` is today's
plain path, so every older client is unchanged. A plain `.zip` is written by `7z -tzip` when 7z is
installed and by `bsdtar` otherwise, and every 7z compress stores a symbolic link as a link (`-snl`), as
bsdtar does, so a dangling one no longer fails the archive.

Before a plain extract starts, an archive that 7z reads is probed with `7z l -slt` and stdin closed: exit
255 (the headers are encrypted) or exit 0 with an `Encrypted = +` line (entries are encrypted) answers
`err` of `"encrypted"`, and so does a plain extract that still exits 255. A password run that exits 2
answers `"wrong-password"` only when the same technical listing proves a password guards the archive: 7z
exits 2 for a wrong password, for damaged encrypted data and for a file that is no archive at all, so
without that proof the run answers as a plain failure and the card never asks again. Both are codes, never sentences, read from exit
codes and technical listing keys and never from the tool's words; the card words its own sentence from a
template, the client asks again on both rather than messaging, and Cancel runs nothing. A line carrying
either code also names the retry: `{"t":"archivedone","id":<uint>,"ok":false,"verified":true,"err":"encrypted"|"wrong-password","path":"<archive>","dest":"<destination>"}`,
so the card opens over the archive that asked and no client-side stash can name another extract's.

A `.iso` lists and is never extracted: an `extract` naming one answers one `archivedone` with `ok` false
and `err` of `"a disk image opens as a listing and is never extracted"`, and starts nothing, so no `archivestarted`
line is sent and no slot is taken. Before extracting, the archive's unpacked
size from its own index is weighed against the destination's free space, and a fit that cannot land is
refused the same way: `{"t":"archivedone","id":<uint>,"ok":false,"verified":true,"err":"Extracting
<leaf> needs <needed>, and <target> has <free> free."}`, where `<target>` is the name of the folder the
extract lands in, or `/` for the root. An index that cannot be read refuses nothing, because unknown is not evidence
the extract cannot fit.

One `peek` field is worth naming ahead of that section, because it is new. A `peeked` line whose scan
failed carries `"failed":true`, with `"mode":<uint>` beside it under the same rule the `error` line
uses, so a column can tell an unreadable directory from an empty one instead of drawing both as the
empty state. A `peeked` line that succeeded carries neither field, whatever `n` is.

More `peek` fields are worth naming for the same reason. Every `peeked` line, failed or not,
echoes the `hidden`, `hiddenLast` and `first` its request carried, because three clients read this wire at once: `ui/ColumnsArea.qml`
peeks the pane's ancestors with the listing's own flags and the pane's window size, `ui/NetworkMounts.qml` peeks with a first of 1 or of 512,
and the path bar's Tab peeks with whatever the typed leaf asks for. The repair legs ask 512 and the folder check asks 1. `path` alone cannot tell one client's reply from another's, and `path`
plus `hidden` plus `hiddenLast` plus `first` can, which is all the correlation any needs: the same quad answers the same rows,
so no request id has to be threaded through. A client that ignores the fields reads the line exactly
as it did before. A `peeked` line echoes `by` and `desc` only when its request carried `by`, so a column re-asked after its folder's remembered order changed tells a reply sorted under the old order from the one it waits for, and a request without `by` gets the line it always got. A `peek` request carries optional `hiddenLast`, `false` unless `true`: with
it on the peek sorts dotfiles last the way the listing does, so the columns beside a hidden-last
listing draw the same order instead of today's dotfiles-first one. A `peek` request carries optional `by` (name, size or mtime) and `desc`, sent only by the columns view for a folder with its own remembered order while remembering is on; absent is name order, a key the wire never defined answers the same `"where":"sort"` error a `sort` does and no `peeked`, and only size and mtime pay a stat per entry. A `peek` request carries optional `watch`,
`false` unless `true`, and `keep`, a list of paths: the columns view sets both so the backend arms a watch on that
directory, see "A column's directory is watched too" above; the `peeked` reply is the same either way. A `peeked` line also carries `v`, the directory's device, and `w`, whether it can be written, the
same two fields a `listed` line carries, read once per peek so a drag from a column takes the verb a drag from the
listing takes; a failed peek carries neither.

## Known gaps

- The two-line prewarm file names its directory, as every `listed` line does, but nothing
  in it says whether that directory changed after it was written, so a stale file cannot be
  told from a fresh one and production ignores `FLEA_PREWARM`. Add a freshness proof and
  prove a first-paint win before re-enabling a reader.
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

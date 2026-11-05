## Thumbnail requests

`backend/run.rs` owns the policy; `backend/thumbs.rs` owns the mechanism. **A thumbnail is
generated only for a row a client named in a `thumb` request.** Nothing in the loop walks
the listing looking for work at any priority, and the proof is on the record: listing the
100,000-file fixture and stat'ing every row through two full windows grows
`~/.cache/thumbnails/large` by zero files and never creates `fail/flea`.

**`THUMB_WORKERS` is 6 since 0.3.2, and it is a measured choice and not a core count.** It lives here and
not in `thumbs.rs` because it is scheduling policy and belongs with the request policy.

**The reason this paragraph used to give was half wrong, and the box said so.** It claimed the
box has 12 cores and that a decoder child is itself multithreaded, so four children already
reach twelve-way parallelism. Neither half survives. The box is an i7-8700T with **6 physical
cores and 12 logical**, two threads per core, which `lscpu` reports and the six distinct entries
in `/sys/devices/system/cpu/cpu*/topology/core_id` confirm. And one `ffmpegthumbnailer` decode of
this fixture costs **0.088 to 0.118 CPU-seconds, median 0.104**, against 0.079 to 0.110 wall
seconds, median 0.095, over 15 warm runs on five videos; the artefact is `~/bench/t4/cpuwall.csv`.
CPU-seconds is the number to quote and not the ratio: a stalled run stretches `real` while
`user + sys` holds, which turns the ratio into noise, and a child decoding three ways would show
0.3 CPU-seconds against the same tenth of a second of wall. One child is one thread and a little.
Four children are about four and a half cores of work, not twelve.

**Throughput keeps rising with width, so throughput is not what picks the number.** Through the
real pool, the media fixture's first 35 videos, this run's own cache entries dropped before every
run, 12 measured runs an arm over three batches of a four-arm Latin square: the median time from
the `thumb` request to the last `thumbed` line is 1108.5 ms at 4 workers, 909.0 at 6, 801.5 at 8
and 709.5 at 12. **That one is monotone at the individual run level**, which is the strongest form
available: the slowest run of each arm beats the fastest run of the next, 1077 against 956, 886
against 817, 790 against 725. Sublinear everywhere, negative nowhere.

**Two things get worse as the pool widens, and both are what a person actually sees.**

The first is the first thumbnail. From those same runs, the median time to the FIRST filled row is
121.5 ms at 4 workers (116 to 127), 123.5 at 6 (111 to 136), 135 at 8 (116 to 142) and 204.5 at 12
(185 to 214). **That one is monotone only in the medians.** At the run level only 8 against 12
separates; the 4, 6 and 8 ranges all overlap, and 10 of 12 8-worker runs land above the 4-worker
maximum of 127 while 2 do not. A wide pool starts its whole first wave at once and the wave
finishes together, so the screen stays empty 83 ms longer at 12 in exchange for filling 399 ms
sooner. The fill curve is a rotation and not a translation, and this is derived from all 35 rows of
the artefact rather than off a sampled table: 4 workers are lowest at rows 1 to 4, 6 workers at 5
and 6, 8 workers at 7, 8, 13 to 16 and 25, and 12 workers at 9 to 12, 17 to 24 and 26 to 35. Read
the other end the same way: 12 workers are LAST at rows 1 to 4, 6 workers at 7 and 8, 262.0 and
273.5 against 248.5 and 255.5, 4 workers everywhere else, and 8 workers are never last at any row.

The second is input latency while the pool is busy, which is the half of the old argument that was
right. **It has to be timed by the UI's own clock.** The first version of this measurement pressed
a key and polled `rowAt` until it answered, and reported 361 / 430.5 / 464 / 635.5 ms at the four
widths; almost all of that was the harness. One `omarchy-drive ipc` call costs 213 ms idle here and
287, 323, 389 and 482 ms under 4, 6, 8 and 12 worker bursts, with 213 again on the return-to-idle
control, and `ready()`, which returns a constant and reads no UI state, costs the same as `rowAt`
at every load. Those numbers are the harness measuring itself. `inputToRows()` exists for this; see
"Testing".

Measured with it: a Flea window on the 100,000-file fixture, `omarchy-drive key --window flea G`
and `g` to jump to the far end and back, a 210-row generation burst at the arm's own width running
beside it, eight rounds an arm with every arm twice in every position. With no burst every arm
reads a median of 38 ms over 48 samples; that is a sanity check and not a result, since each
baseline is taken before the burst process launches, on the identical shipped tree, so no mechanism
exists by which the arms could differ there.

**The last jump of a round can outrun the burst, and every figure below drops it.** The harness
checked burst liveness once per jump PAIR, and a jump costs half a second of quiet window plus a
200 to 480 ms read, so the final sample of a round could be answered after the load had ended. It
shows up in the artefact: the sixth and last sample of the 8-worker arm reads 39, 37, 38, 39, 39,
40, 39, 37 across the eight rounds, which is that arm's own no-burst level, while no other position
of any arm sits near baseline. **The last sample of every round is therefore dropped in every arm,
symmetrically, no arm favoured.** Untrimmed the medians of medians are 48.25, 52.25, 51.5 and
73.25, which is NOT monotone: it puts 8 workers below 6. Trimmed they are **48.0, 51.0, 53.5 and
73.0**, per-round medians spanning 45 to 49, 49 to 57, 50 to 57 and 56 to 83, and the series is
monotone. The trimmed figures are the ones to quote. The harness now checks liveness before each
jump AND again after its read, and reports what it dropped as `offload_dropped`; the numbers here
predate that gate and come from the symmetric trim.

**Each of 6, 8 and 12 is above the 4-worker arm in 8 rounds out of 8** either way, which is
p = 0.004 for a sign test, and at the sample level the trimmed arm is the slower of a cross pair
72.6, 76.6 and 88.8 percent of the time against a null of 50. **6 and 8 are not separated from each
other**, trimmed or not: head to head the 6-worker median is above the 8-worker one in 2 rounds
and below in 6 after the trim, and above in 5, below in 2 and tied in 1 before it, so no ordering
between those two arms is a finding.

**Hyprland is not what is starved**: it holds at 1.8 to 2.5 percent of one core at every width. Do
not read the `qs` process's falling percentage-of-core as starvation either. Its CPU per COMPLETED
jump pair does not fall in the direction that claim needs: 87.2, 96.7, 90.4 and 105.6 ms across the
four widths over all eight rounds, ending higher than it started. The percentage falls because
fewer jumps complete per second, not because anything was denied to it.

**The rule, written before the numbers: take the highest width whose input latency is inside the
noise of the 4-worker arm and whose first thumbnail is too.** On the trimmed series, 12 fails both,
by 25.0 ms of latency and 83 ms of first thumbnail; 8 fails both, by 5.5 ms and 13.5 ms; and 6
passes the first-wave check outright, 123.5 against 121.5 with overlapping ranges, and fails the
latency check by 3.0 ms while sitting above the 4-worker arm in 8 of 8 rounds. Nothing above 4
survives both, so **4 stayed**, until 0.3.2 below.

**Say the margin out loud, because it is small and the trade is not.** The honest cost of 6
workers is **3.0 ms** of input-to-rows latency on the trimmed series, 51.0 against 48.0, and 4.0 ms
untrimmed, 52.25 against 48.25; the trim is the one described above and it is the figure to quote.
Three milliseconds is under a fifth of a 60 Hz frame. What it would buy is 199.5 ms off the 35th
thumbnail and a tie on the first. The rule disqualified 6 because the cost is measurable, 8 rounds
out of 8, not because it is large, and the 69.5 ms that looked decisive before the instrument was
fixed was the harness timing itself. Anyone revisiting this constant should start from 3.0 against
199.5 and not from the conclusion.

**0.3.2 moved it to 6, on a different question.** Strata's settle on the media fixture came level with
Flea's through a reusable sandbox worker of its own, and the harness was measured at
both widths on the final 0.3.2 candidate, two interleaved batches of three: settled 2701 to 2860 ms
at 4 and 2333 to 2631 at 6, against strata's 2804 to 3071. The rule for that release was to
keep 4 only if 6 cost input latency a person would feel. Re-timed with `inputToRows()` by the method
above, eight rounds, arm order alternating, a second backend generating 210 media rows at the arm's
own width, every sample the burst had already finished by dropped and the last survivor of each
round trimmed: idle 46.5 against 46.5 ms over 48 samples each, and under the burst **65.0 at 4 and
70.5 at 6**, 33 and 24 samples, 6 above in 6 of 8 rounds. **5.5 ms is a third of a 60 Hz frame**,
which nobody feels, so 6 ships. The 6-worker burst ended sooner, 5.2 against 6.5 s, so more of its
samples were dropped, and every one it kept was taken with its whole pool busy. The artefact is
`fw-lat.csv` in the 0.3.2 release evidence.

**One thing this measurement does not contain.** The burst runs in a SECOND backend process, so
the window under the input has an idle pool of its own and the contention a real burst adds to
`run.rs`'s single event loop, where every `thumbed` line is written and flushed, is in no number
here. That omission is conservative for the decision, since it can only understate what a wider
pool costs the window it lives in, which is why it is said rather than left out.

**Four answers cost no job at all.** A directory row, a row that is not a regular file, and a
row whose type no validated thumbnailer declares are answered with an empty `file` on the spot,
which is the same condition the `t` flag already reports to the client. A row the shared cache
already holds
at the file's current mtime is answered with that path, and a row recorded as failed at
that mtime with an empty `file`. Only a real miss is queued. A cache hit costs about 12
microseconds end to end on the wire here, against 9 for `Cache::lookup` alone: treat both
as magnitudes and remeasure.

**Only a regular file is ever handed to a thumbnailer.** `scan` fills `is_dir` from `d_type`, so
a fifo, a socket or a device node reads as an ordinary file and its NAME alone decides whether a
thumbnailer declares it. Four fifos called `*.mp4` were enough to take the whole pool down: all
four came back `"t":true`, all four workers blocked on the open for the full 20 second job
timeout, each wrote a permanent `fail/` marker, and every visible row behind them waited. Seventy
such names kill the pool for six minutes, and a filename is the whole trigger, which is the threat
model this section exists for. `thumb_rows` therefore takes the `std::fs::metadata` it already
needed for the mtime and gates on `m.is_file()`, which costs nothing and subsumes the directory
test, and `rows_line` mirrors it through `meta::thumbnailable`, which reads the file-type bits out
of the `st_mode` the window already carries rather than adding a second stat per row. That flag
accepts a symlink as well as a regular file, because the request path stats the target: a symlink
to an image still thumbnails, and a symlink to a fifo is refused when it is asked for.

**The row-to-path map is `State::asked`, and it holds only rows a client asked for.** It is
not a map of the directory: building one would be a per-file pass over the listing, which is
the one thing this product does not do. A `list` or a `sort` clears it and cancels the queue,
because both change which row an index names, so a result that arrives afterwards finds no
entry and is dropped rather than printed against a row it no longer describes. That is this
tree's answer to the generation problem, and it is strictly stronger than a generation counter
carried in the `Job`: a counter bumped on `list` alone would still misreport a row after a
`sort` reordered the listing under an outstanding job. `tests/thumbs.sh` proves it by listing a
second directory between the request and the result and asserting no `thumbed` line at all;
removing the clear from the `list` arm reddens exactly that check and nothing else.

**The map is bounded by the pool, and the loop never trims it.** It holds one entry per queued,
waiting or running job, so `MAX_QUEUE`, `MAX_DEFERRED` and the worker count bound it and nothing in `run.rs`
removes an entry the pool or `Deferred` still owns. An earlier version trimmed the map from
the FRONT past that number, justified as dropping "the entries the pool silently dropped to make
room". The box disproved that: workers pop from the front, so the oldest mappings are the RUNNING
jobs. A single request for 80 rows against the 70-job queue and four workers answered 74 of them
and stranded the first six, four of which finished inside a worker and had their results discarded
because `report_done` found no mapping, which breaks the contract that a client never waits forever
for a row that will not arrive. `submit` therefore returns the `Job`s it dropped rather than a
count. `queue_row` maps an uncached row and puts its job at the tail of `Deferred`, then pumps into the pool's room;
it also pumps before checking the deferred cap, so newly free pool room prevents an unnecessary empty answer.
The pump unmaps every job `submit` returns, answers its row with an empty `file` and releases its outstanding count
in release builds too. Normal requests do not drop (see "Thumbnail pool"): an 80-row request answers none of its
rows early and waits for its jobs, and `tests/thumbs.sh` still asserts one `thumbed` line per row.

**The map is also the dedupe.** A path already in it has a job queued or running for this
listing, so `queue_row` returns without submitting a second one. Without that, `rows:[0,0,0]`
queues three identical decodes, because all three iterations run before the first job finishes
and so all three see `Hit::Miss`. Each duplicate also gets a mapping, so all three results are answered,
and three of a four worker pool spend tens to hundreds of milliseconds each on work already in flight, which is the exact
resource `MAX_QUEUE` and the worker count exist to ration. `tests/thumbs.sh` proves it without
timing anything: it saturates the four workers, queues a victim row, then floods one row with 75
repeats. The FIFO queues the victim first; without the dedupe the repeated row is answered 75 times,
which the second check catches. The first check guards against eviction, and the dedupe keeps the request to six distinct rows.

**A cancelled row leaves the map with its job.** `thumbcancel` with an explicit `rows` unmaps each
row whose queued job it dropped or whose active video cancellation it sent. An active
cancellation keeps its outstanding slot until the worker reports. A replacement can be
requested immediately, and the cancelled job's report leaves the replacement mapping alone.
`thumbcancel` with an empty or malformed `rows` cancels and unmaps queued jobs. The asymmetry that made the empty
form poison the map was real and reproduced: `list` and `sort` clear the map through `forget_rows`,
`thumbcancel` did not, so every cancelled mapping went stale, `queue_row`'s dedupe then swallowed
every later request for those paths until the next `list`, and shutdown answered them all with an
empty `file`. A viewport-change handler sends exactly that request, so the headline feature stopped
after the first cancel-all. `tests/thumbs.sh` cancels all of twelve queued rows and asks for them
again; without the unmap only the four already inside a worker come back with a file.

**`submit`, `cancel` and `cancel_all` each return the queued jobs that will now never report**,
taken under the pool's own lock. That is what keeps `State::outstanding` exact and what lets the
loop answer a dropped row instead of stranding it. Reading the queue length and then clearing it in
two calls would race a worker's own pop and leave the count short, which at shutdown means exiting
while a job is still inside a child.

**Shutdown answers the work the client asked for, then cancels.** The plan's text said to
call `cancel_all` before exiting on `quit`; the running code drains first and cancels after,
because `cancel_all` on its own loses the race with the worker that has not popped the job
yet, and the client is told nothing about a row it explicitly requested. Draining also
matters for the shared cache: a worker inside a child owns a `thumbwrite::exclusive_temp`
file there, and only its own return renames that temp into place or removes it. The whole
drain is bounded at `DRAIN_LIMIT`, 25 seconds, which is longer than the pool's own 20-second
job deadline so no single running job can be cut short by it; any row still unanswered when
it runs out is answered with an empty `file`, because the contract is that a client never
waits forever.

**A drain that gives up sweeps this process's own temps.** The one path a worker cannot clean up
after itself is the one where the drain runs out of budget: the process exits and the four worker
threads die inside `run_with_timeout`, after `thumbwrite::exclusive_temp` and before `discard`.
Reproduced with eight hung jobs against four workers under a scratch cache root: four zero-byte
`.flea-<pid>-<hex>.png` files were left behind, which matches the ten found in the
real cache. `drain` therefore calls `thumbwrite::sweep_own_temps` on the `large/` directory after
`cancel_all` and only when `outstanding` is still above zero. The order is load bearing: with the
queue emptied first, no worker can start a new job, so the temps on disk at that moment are exactly
the abandoned ones and the sweep cannot race a fresh one. The name carries this process's pid, so
the sweep provably cannot touch a published entry, another application's file, or a concurrent
flea's in-flight temp; a unit test plants all three next to a real temp and asserts only ours goes.
Two alternatives were rejected. Sweeping stale `.flea-*` at STARTUP would put a `readdir` of a
possibly huge shared directory on the product path, which is the one thing this product does not
do, and "stale" needs an age heuristic that races a concurrent flea. Registering each temp in a
shared list would be exact but replaces one function with an invariant that every return path in
`run_one` has to maintain, which is the class of bug this is. Neither this nor any other design
covers a `SIGKILL`; nothing running inside the process can.

**Termination no longer rests on the channel disconnecting.** Before the pool existed the
reader thread held the only `Sender<Event>`, so a reader death ended the loop by
disconnection. The pool's workers and the forwarder now hold senders for the life of the
process, so `rx.recv()` can never return `Err` and the backstop is gone. What replaces it is
that `spawn_reader` sends exactly one terminal event on every exit it has: `Closed` when
stdin ends, `ReadError` when a line will not decode, and nothing at all only when its own
send already failed, which means the loop is gone. All four shutdown paths were exercised
live: `quit`, stdin closed with no `quit`, junk then close, and invalid UTF-8, which still
prints exactly one `{"t":"error","where":"read",...}` line and exits 0.

**The pool is built when the backend starts, not when the first `thumb` arrives, and that
costs.** Measured on the media fixture against the pre-plan tree, the backend child's peak
PSS went from 1004 kB to 1528 kB, three runs each and no overlap, and backend startup went
from about 2.5 ms to about 3.7 ms, slower in 15 of 15 interleaved pairs. Decomposed on the
same fixture: the pre-plan tree runs 1 thread at about 1200 kB, Task 8's reader thread makes
it 2 threads at about 1400 kB, and this task's four workers plus the forwarder make it 7
threads at about 1760 kB. **That measurement predates `8fa64ee`, and the rest of it is gone.** The
second copy of the alias and thumbnailer tables was what carried it, and `Pool::new` now parses
nothing: both tables arrive as `Arc`s from the caller (`src/backend/thumbs.rs:76`,
`src/backend/run.rs:104`), which is the 0.11 MiB recorded above. What eager construction still
costs is four worker threads blocked on a condvar, and a backend whose client never asks for a
thumbnail still pays for those, `flea --backend` driven from a script being exactly that client.
Constructing the pool on the first `thumb` request would give back 0.042 MiB and about 0.32 ms;
it is dropped on its size alone, and the measurement behind those two numbers is in "Backend
memory levers that were measured and dropped".

### Thumbnail requests in the GUI

The QML-specific launch and geometry discussion below describes the retained reference components; the production GPUI window is created by `crates/flea-gui/src/window/mod.rs`.

`ui/Pane.qml` decides which rows are asked about and `ui/js/Thumbs.js` does the arithmetic, as
pure functions with no QML imports so `./tests/js.sh` can redden on a mutation. **`Thumbs.viewport`
is the range rule itself**, the viewport in row indices clamped to the last row of the listing, and
it lives there rather than inline in `requestThumbs` so a mutation reddens `tests/js.sh`: dropping
its clamp fails "a listing shorter than the screen clamps to its last row". **The request
names visible rows and nothing else**, which is the client half of the rule the backend enforces:
nothing prefetches, warms, or asks for the screen ahead, with two exceptions: Quick Look at rest asks
for the next row's cache entry only (cacheOnly, never a decoder, never a meta, none for an off class
or unknown storage), pinned by tests/thumbs-qlprefetch.qml, while nosweep still covers the listing; and
a shown image whose final decode failed asks once for its own cursor row (never cache-only, once per
show, never under a manual hold), pinned by tests/preview-decode.qml.

**`viewport` measures the window in pixels, because a `contentY` that is not row aligned straddles
one more row than the window holds.** `first` is `floor(contentY / rowHeight)` and `last` is
`ceil((contentY + visibleRows * rowHeight) / rowHeight) - 1`, still clamped to `total - 1`. At this
box's constants, a 1312 px list with a 37 px row and `visibleRows` 36, the old
`first + visibleRows - 1` left a 37th row up to 16 px on screen with no thumbnail for every
`contentY mod 37` in 21 to 36. The bound in `tests/ui.sh thumbs` is therefore
`requests * (visibleRows + 1)`. **The extent is derived from `visibleRows` and not from
`list.height`, which the function is not given**, and `visibleRows` is `ceil(height / rowHeight)`,
so the padded extent also over-asks by exactly one row for a `contentY mod 37` in 1 to 20. That is
one decode for the row immediately below the fold, it errs toward the safe direction, and the exact
rule would need the pixel height threaded through for no behaviour anybody can see.

**Two other places compute the same pair and neither is this rule.** `ui/Pane.qml`'s
`ListView.onContentYChanged` clamps the cursor with `first + visibleRows - 1`, where clamping to
fully visible rows is what is wanted, and `requestIfDrifted` uses `firstVisible + visibleRows` with
no `- 1` as a refetch margin heuristic. Do not unify them with `Thumbs.viewport`.

**One settle timer, restarted by every scroll, by every arriving window of rows and by every
viewport change.** A `Timer` runs at `firstSettleMs`, 70 ms, until a listing has issued its first
request and at `settleMs`, 120 ms, after that, and `open()` puts it back to 70 for every new
listing. It is restarted from `ListView.onContentYChanged`, from `onRows` and from
`onVisibleRowsChanged`. While the list is moving, `contentYChanged` fires every
frame and keeps pushing the deadline out, so a fling issues no request until it stops. `tests/ui.sh thumbs` holds that closed with a budget:
one fling across a 200-row fixture must leave the request count between one and two higher than it
started, the upper bound catching a request per scrolled frame and the lower one catching a settle
timer no scroll restarts. Its sampling loop, which watches the cursor move while the count does
not, is corroboration and not the gate. `tests/ui.sh nosweep` asserts a full traversal of the
100,000-file fixture leaves the request count at zero and the shared cache the size it started.
120 ms is a feel decision, confirmed on the box against 60 ms and 250 ms rather than measured.

**The first screen races the compositor's resize, and `firstSettleMs` exists to lose that race on
purpose.** Flea's `FloatingWindow` declares `implicitHeight: 600` at `ui/boot/shell.qml:20`, Hyprland
then tiles it to the full screen, and the viewport a request would name depends on which of the two
arrives first. Before the resize the list is **526 px** tall and `visibleRows` is **15**; both
`ui/Header.qml` and `ui/StatusBar.qml` declare `implicitHeight: Theme.rowHeight`, which the IPC
seam reports as 37 here, so 526 plus 37 plus 37 is exactly the declared 600. After the resize the
list is 1312 px and `visibleRows` is 36.

**The resize latency is a range and the sign is not stable**, measured with `console.log(Date.now())`
stamps inside the UI and read back from the qs log. On a 200-file fixture, where the listing is fast,
the resize landed **4 to 41 ms AFTER the rows in 14 of 20 runs and 34 to 40 ms before them in the
other 6**, and the negatives all clustered at the end of the batch as the box warmed. On the
2001-file media fixture the listing is slow enough that the resize landed **33 to 42 ms before the
rows, 6 of 6**. So neither event can be assumed to be first.

Plan 5 Task 5a first tried removing the timer from the initial listing altogether. The request went
out 9 to 10 ms after `onRows` instead of 134 to 140, and the media `settled` column improved in 9 of
9 pairs by a median 113 ms, **but on a fast listing it named the 15-row viewport and 21 of the 36
rows the user was looking at kept their generic icon until something scrolled**, because nothing
re-requested. `tests/ui.sh thumbs` showed it as `added=51` against base's `added=72`, in 5 of 7 runs
rather than all of them, because it is a race and not a deterministic bug.

**What shipped instead debounces both events rather than betting on either.** `firstSettleMs` is
70 ms, `onVisibleRowsChanged` restarts the settle as well as `onRows`, and `requestThumbs` latches
`settle.interval` to the 120 ms fling debounce the first time it actually runs. Whichever of the two
events is last re-arms the timer, so the request names the final viewport either way. **70 is chosen
against the measured 41 ms worst case with 29 ms of margin**, and the failure mode if that margin is
ever blown is loud rather than silent: the timer fires early, the viewport change re-arms it, and the
first screen takes two requests, which `tests/ui.sh thumbs` fails on. It does not go back to missing
thumbnails.

Measured on the shipped tree: `requests=1` and 36 of 36 rows filled in 8 of 8 runs on the fast
fixture, the request issued at `h=1312 vis=36` with `interval=70` in 10 of 10 probe runs, the
request going out after `onRows` against base's 134 to 140, and the media `settled` column better
in 12 of 18 interleaved pairs across two nine-pair batteries, median to median 40 ms then 24 ms,
pooled median 33 ms per pair. **The time after `onRows` is a range across two batteries and not a
constant**: the second read 83 to 100 ms over 12 runs against the first's 81 to 87, so the honest
span is 81 to 100 ms.

**The media PSS cost this change was first reported to carry did not reproduce, and the honest
statement is that there is no established cost.** The first battery had it 216 KiB higher, 8 of 9;
the second, same comparison and same harness, had it 122 KiB LOWER, 6 of 9. Pooled over 18 pairs it
is 87 KiB higher, 11 of 18, against a within-arm spread of **341 to 586 KiB across all four arms of
the two batteries**. The 437 to 586 first quoted here dropped its own lowest arm, 341, in the
direction that supports the withdrawal it justifies; read it as a range over every arm, not as a
floor. Two batteries flipping the sign majority is the same instability the field benchmark shows,
so treat any sub-megabyte PSS claim from a nine-pair batch as unproven until a second batch agrees
with it.

**`open()` resets the interval, so every listing gets the short settle and not only the first**, and
`requestThumbs` latches only once `work.ask` is non-empty, so a directory it asks nothing about does
not spend the first screen's budget. Both measured, 3 of 3, by logging `settle.interval` on entry
and on exit through three nested directories: images then images then text-only gave entry 70, 70,
70 and exit 120, 120, **70**, with two `thumbRequests` for the three listings.

**Two thresholds, and only one of them is the request count.** Write `g` for the resize's arrival
relative to `onRows`, so `g` is negative when the resize is first. Base fires at `onRows + 120`; the
repair fires at `max(onRows, resize) + firstSettleMs`, which is `onRows + 70` when `g <= 0` and
`onRows + g + 70` otherwise. So the first screen takes **two requests when `g > 70`**, and the
repair is **slower than base when `g > 50`**, by at most 20 ms in the band between. Against the
measured worst `g` of 41 ms that is 29 ms of margin on the first and 9 ms on the second.

**The slower-than-base band does not touch a field column, and that was measured rather than
assumed.** Both field fixtures are slow listings, and `g` came out NEGATIVE in every pair of both
batteries taken. **Read its size as a range across those two batteries and not as a constant:** the
reproduction got 38 to 53 ms negative on the media fixture and 38 to 72 ms negative on the
100,000-file scale fixture, against the first battery's 6 of 6 at 33 to 42 and 37 to 43, so the
honest span is 33 to 53 negative on media and 37 to 72 negative on scale. The scale fixture also
has no thumbnailable row in it, so its request is empty and this timer cannot move its column at
all. The positive-`g` case only appears on a directory small enough to list before the compositor
answers, which is a 200-file fixture here.

**70 is the tunable and 120 is the one-constant rollback.** The yield is exactly `120 - firstSettleMs`
on any slow listing, so setting `firstSettleMs: 120` restores the old behaviour in one line and gives
back the whole latency win. Lowering it widens the two-request margin and narrows the
slower-than-base one; raising it does the reverse. 70 was chosen because both margins clear the
measured 41 ms worst case.

**One known corner comes with it, recorded rather than fixed.** The anti-fling settle is 70 ms
rather than 120 from the listing arriving until a settled viewport actually names an un-asked
thumbnailable row, because `requestThumbs` latches to 120 only when `work.ask` is non-empty
(`ui/Pane.qml:367-368`). A slow wheel scroll started inside that window can issue a request
mid-scroll where it previously would not. **On a text-only listing the window never closes at
all**: nothing is ever asked for, so the interval stays at 70 for that listing's whole life. That
case is verified benign rather than argued: a full 2400-detent traversal reports
`NOSWEEP requests=0` with the cache count unmoved, because `Backend.thumb([])` early-returns and
never sends or counts a request. Either way it is one request naming one viewport, so it breaks no
rule; no test covers the mid-scroll request, and `tests/ui.sh thumbs` cannot reach it because its
fling runs entirely after the latch.

**Do not reach for the two repairs that look obvious and are not.** Gating on the window reaching
its declared size fails, because at 526 px it IS at its declared size. Requesting on the first
`visibleRows` change fails, because on a slow listing that change has already happened before the
rows arrive. And a bare short interval with no `onVisibleRowsChanged` restart is a margin bet
against a 41 ms worst case that was measured at 40 ms twice in twenty runs.

**The request count cannot prove the rule on its own, so the cache does.** `thumbRequests` counts
LINES, not rows, so an implementation that swept a whole directory in one `thumb` naming every row
would score one request and satisfy every count assertion. `tests/ui.sh thumbs` therefore asserts
what `~/.cache/thumbnails/large` grew by against a viewport-derived bound, the requests issued
times `pane.visibleRows`, which the read-only seam reports as `visibleRows()`. Every listing request
names a subset of one viewport (Quick Look's cacheOnly next-row ask above never decodes, so it
generates nothing this bound counts; the shown-row ask, one row once per show after a failed final decode and never under a manual hold, is one more one-row request, so the bound still
holds), so the rows generated cannot exceed a screenful a request, and a clean
run sits exactly ON that bound rather than under it. The WINDOW's own height over the row pitch was
tried first and rejected: it exceeds the list's by the header and the status bar, and one request
can never generate more than `MAX_QUEUE` plus the worker count, 74 rows, however large the
directory, so on this box the loose form bounded 74 by 74 and a whole-directory sweep passed it.
The bound is paired with a floor, because `added` is a delta over a cache every application shares
and a warm one makes the ceiling vacuous: the fixture carries the run's pid so its entries cannot
pre-exist, and the floor fails the case if a screenful was never generated at all. Together they
are the only end-to-end witness in the tree that a request names visible rows and nothing else.

**The map has three states and `undefined` is the fourth.** Absent means unknown, `null` means
asked and waiting, a string is the answer, and an empty string is a real answer meaning this row
has no thumbnail. Only the last two are terminal: an answered row is never asked about again for
this listing, and only a row still `null` can be cancelled, because only that row has a job the
backend could drop. `open()` clears the whole map, since an index names a different file after a
new listing.

**The `t` flag over-promises and the renderer absorbs it rather than the wire narrowing.** `t`
is true for every symlink whose name resolves to a declared type, whatever the link points at,
so a symlink to a directory, to a FIFO, to a loop or to nothing at all is asked for like any
other row. Every one of them is answered with an empty `file` in microseconds, because the
request path stats the target and refuses it there, and an empty string is a terminal answer in
the map. `Row.iconSource` reads it as no thumbnail and falls through to the themed icon, so the
row simply keeps the icon it already drew: nothing blinks, nothing waits and no slot is left
holding a gap. That is the deliberate answer to the over-promise recorded in
`docs/protocol.md`, and it is why this plan renders defensively instead of narrowing the flag,
which would be a wire change with its own documentation and test surface.

**`Backend.thumbcancel` never sends an empty `rows`.** That form cancels EVERYTHING queued, so a
viewport-change handler that sent one would strand every row it did not name; `Backend.thumb`
guards the same way so an empty request never leaves the process. `requestThumbs` deliberately
does NOT test the two lengths itself: a caller-side guard makes the guard here dead code, and this
is the class of bug that already cost this project a branch, so the invariant stays on the live
path with exactly one author. `thumbRequests` counts ATTEMPTS, not lines on the wire: the
increment precedes `send`, which may queue the line until the child starts or fail it outright.
It is what the settle gate asserts through the IPC seam.

**`ui/Backend.qml` carries two `// Sample input:` lines and that is deliberate.** `receive`
consumes four shapes, `listed`, `rows`, `error` and `thumbed`, and the two carrying row data are
sampled; each sample is a real line a reader can check the parser against, so folding them into one
comment would cost the thing the comment is for.

**`onThumbed` drops a result while a listing is in flight.** `run.rs` clears `State::asked` while
handling `list` and drops every later result, so on the wire every `thumbed` for a superseded
listing precedes the new `listed` line. The guard drops exactly those. It is argued from the wire
order, not proven by a test: deleting it reddens nothing in a single-directory case.

**`thumbCap` is 240 and bounds a policy bug, not normal use.** Only rows a viewport held are ever
recorded, so seven screens of history at this row height is already generous; the cap exists so a
future handler that records more costs memory slowly rather than without limit.

Three behaviours a later session will meet, each known and none fixed here.

- The cap evicts by insertion order and not by distance from the viewport, so returning to the top
  of a listing after visiting more than 240 rows evicts and re-asks the rows now on screen. It
  self-heals through a cache hit, so it is a flicker and not a loss.
- A window resize that exposes rows without moving `contentY` used to leave them unasked until the
  next scroll. **Task 5a closed this**: `onVisibleRowsChanged` restarts the settle, so a resize is
  debounced exactly like a scroll and the rows it exposes are asked for once the viewport stops
  changing.
- `open()` clears the map, and `Backend.sort` invalidates every row index on the backend side just
  as hard. `sort` has no caller in the shipped UI today; whoever wires the header's sort controls
  has to clear the map there too.

**`case_thumbs`'s own live-sampled fling check is this project's documented known red, and the
root cause is measured rather than assumed.** The check backgrounds `omarchy-drive scroll down
$fling_clicks` and polls `cursor`/`thumbRequests` in a loop, looking for a sample where the cursor
moved and the counter did not. Measured directly on this box: `omarchy-drive scroll down N` takes
at minimum ~140 ms even at the smallest `N` that moves the cursor at all, rising to roughly two
seconds at the fling's own 1500 clicks, and every separate `omarchy-drive ipc` round trip costs a
further ~190 ms on its own. Both figures already exceed the 120 ms settle debounce, so any check
taken through a shell-level IPC call, whether immediately after a scroll command returns or
live-polled during one, races the debounce and reliably loses: by the time the round trip
completes, settle has already fired. This is a property of the CLI tool in this environment, not
of the settle logic it is trying to observe. Task 16's directory-heavy twin to `case_nosweep`
(`ui.sh`, `nosweep-dirs` fixture) hit the identical `during_samples=0` symptom on first write,
reproduced it on unmodified `thumbs` before the twin existed to rule out a Task 16 defect, and
shipped instead with a before/after delta bound (`1 <= delta <= 2`, one settle's worth of requests
per fling rather than a literal zero), the same upper bound this section's own settled-state check
already accepts and for the identical reason.

Touchpad history: since 2026-10-01 touchpad input matches Finder's feel, so the Motion.js
animation-free rule and the old touchpad 1:1 no longer hold for it; the 2.5 gain is GTK4's own
factor from gtkscrolledwindow.c, and a wheel notch is unchanged.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

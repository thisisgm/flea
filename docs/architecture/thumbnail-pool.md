## Thumbnail pool

`backend/thumbs.rs` is the only thing that generates a thumbnail, directly or through the
worker above, and it generates one only for a `Job` a caller submits. **Nothing in it enumerates
a directory**, at any
priority: the load-bearing rule that every per-file operation stays scoped to the viewport
is what keeps Flea's cost on a large directory low, and a sweep here would forfeit it.

`Pool::new(workers, results, root, aliases, specs)` spawns `workers.max(1)` threads that pop from
one `Mutex<VecDeque<Job>>` under a `Condvar` and report every finished job down one `Sender<Done>`.
The worker count is the caller's, not a constant here: it is a scheduling policy that belongs
with the request policy in `run.rs`, and the only thing this module insists on is that zero
workers is not a pool, outside the test-only `Pool::idle` that request-policy tests read back. The MIME alias table, the thumbnailer specs and the `Cache` live in one
`Arc<Tables>` shared by every worker, so the caller's one parse and the one cache root reach four
threads as a single handle rather than as four copies. **No worker reads either file on any path
now**: both arrive as `Arc`s from the caller, `Cache::at` only stores the root, and even
`with_specs` builds its tables in the constructor.

**Both tables are the caller's, passed in as `Arc<Aliases>` and `Arc<Thumbnailers>`.** `run.rs`
has already parsed both to answer the `rows` line, and `Pool::new` used to parse them a second
time at startup, so a client that never asks for a thumbnail paid for two tables it could not
use. Sharing that one parse is worth **0.11 MiB of the backend child's PSS**, measured over 12
interleaved pairs on each of three trees with no run overlapping the other arm: 5075 to 4957 KiB
on the 100,000-file fixture, 1525 to 1409 KiB on the media fixture and 1453 to 1345 KiB on a
12-file directory, `Pss` read from `smaps_rollup` in KiB with THP disabled in the child as the
shipped launcher leaves it. `with_specs` still builds its own, because a test names thumbnailers
no shipped file declares.

**The cache root is a constructor argument, not `Cache::new()`.** Production passes
`thumbcache::default_root()`; every test in this module passes a per-pid directory under the
temp dir. The shared cache at `~/.cache/thumbnails` is the operator's, and a suite that
records a `fail/flea/<md5>.png` marker there on every run would poison it for every other
application on the box.

`submit`, `cancel` and `cancel_all` each return the queued `Job`s they dropped, taken under the
same lock that dropped them, so `run.rs` can unmap exactly those rows and answer them rather
than only counting them; see "Thumbnail requests".

`MAX_QUEUE` is 70, two viewports of about 35 rows, and `submit` drops from the FRONT when it
is full. The request path never fills it past that bound: a row that finds the queue full waits in
`thumbdefer.rs`'s `Deferred`, in request order, and `thumbdefer::pump` submits it as workers take jobs
off the pool. The drop was the wrong answer for a row the client still wants. The client cancels each pending row
once it leaves the viewport, the preview row aside, so what is left to evict is still wanted. A request
wider than 70 uncached rows evicted its own first rows and answered them empty, and `Thumbs.plan` keeps any
row that has an answer, so an empty one stayed blank for good. `Deferred`
holds at most `MAX_DEFERRED` (4096) rows; past that a row is answered empty at once. `cancel` and
`cancel_all` remove queued jobs and waiting ones alike; a job already inside a child runs to its own
end or to the timeout.

`JOB_TIMEOUT` is 20 seconds, and `backend/child.rs` enforces it. The child is waited on
exactly, not polled: `pidfd_open` gives a descriptor that becomes readable when the child exits,
and one `poll` on it with the remaining time as its timeout is BOTH the wait and the deadline, so
a child still alive at the deadline is killed and reaped and answers `Ran::TimedOut`, which records
a marker only for a local file and never for network or USB. `poll` returning `POLLIN` says only that the child exited, so the status itself still
comes from `wait`. The two symbols are `extern "C"` declarations in the idiom `src/thp.rs` already
uses, because `std` links the system libc and this project takes zero crates; glibc has shipped a
`pidfd_open` wrapper since 2.36, so no `syscall()` is needed. The descriptor is held in an
`OwnedFd`, which closes it on Drop, so the deadline path and both error paths release it without
anyone remembering to. `EINTR` is a retry and is the only reason the loop goes round twice, and
the retry recomputes the remaining time against the same deadline rather than restarting it, so a
stream of signals cannot hand a hung child an unbounded wait.

**There is deliberately no fallback to a sleep loop.** A `pidfd_open` that fails leaves a child
already running with no way to enforce a deadline, so it is killed and reaped and reported as
`Ran::NotStarted`. A second wait implementation, reachable only on fd exhaustion on a box whose
kernel always has `pidfd_open`, would be code nobody ever runs and therefore code nobody has
tested when it matters.

**A syscall failure is the machine's fault, so it never records a marker.** `Ran::NotStarted` is
the variant whose call-site behaviour is right for it: `run_one` discards the temp and writes
nothing to `fail/`. Reading the variant by its NAME rather than by what the call site does with it
argues the other way, since the child did start and did read the file, and that reading is wrong.
The distinction this enum draws is whether a thumbnailer's verdict on these bytes exists, and a
`pidfd_open`, a `poll` or a `wait` this process could not complete produces no verdict at all,
exactly like a fork that failed under memory pressure.

**Two of the eight ways a job can end always record a marker, and the deadline records only for local files.**
That count is the `yes` column of the table below, counted off it rather than carried in from
anywhere else. `run_with_timeout` itself returns `Ran::TimedOut` for a child still alive at the
deadline and `Ran::Failed` for a child that exited non-zero; the empty-output case is `run_one`'s.
Every way a job can end:

| Path | Variant | Marker |
|---|---|---|
| `spawn` failed | `NotStarted` | no |
| `pidfd_open` failed | `NotStarted` | no |
| `poll` failed, or was ready with no `POLLIN` | `NotStarted` | no |
| `wait` failed | `NotStarted` | no |
| still alive at the deadline | `TimedOut` | local only |
| exited non-zero | `Failed` | **yes** |
| exited 0 having written nothing | `Succeeded`, then recorded by `run_one` | **yes** |
| exited 0 having written a thumbnail | `Succeeded`, published by `run_one` | no |

`run_one` records through `records_marker` (`src/backend/thumbs.rs:records_marker`), which a
success reaches only when it wrote nothing, and `tests/thumbs.sh` exercises that arm on every
run. What the recording paths share is that a decoder ran on the file and produced no answer.

The `wait` row was `Failed` until 2026-08-30, which contradicted the rule the rest of this
section states: an `Err` from `wait` is `ECHILD` or nothing, never a verdict, and it wrote a
permanent marker for a file that may be perfectly sound. The tail of `run_with_timeout` is
`verdict`, its own function, so the failing wait is reachable from a test: it spawns a child,
reaps it directly with `waitpid`, and leaves `verdict` a wait with nothing to reap. Against the
old single `_ => Ran::Failed` arm that test fails and the other six in the file pass.

**A machine-caused SIGKILL is NOT told apart here, and the attempt was refused on evidence.**
The proposal was to read `status.signal() == SIGKILL` as `NotStarted`, since the deadline path
kills and returns before `verdict`, so a SIGKILL arriving there is not ours. It fails on three
counts, each measured on this box against the real `sandbox::wrap` argv by
`~/bench/sigkill-probe.py`.

**It cannot see the kill it was proposed for.** bwrap's man page says under `EXIT STATUS` that it
returns the exit status of the initial application process, and its `--json-status-fd` entry
documents the encoding as "n if it exited normally with status n, or 128+n if it was killed by
signal n". So a decoder SIGKILLed inside the sandbox reaches the direct child as **exit 137, not
as a signal**, and the direct child is `prlimit` having exec'd `bwrap`, which is one process
either way.

**A file's own death is the same number.** `prlimit --cpu=N` sets the soft limit EQUAL to the hard
limit, read back from inside the sandbox as `Max cpu time 2 2 seconds`, and with them equal the
kill that arrives is SIGKILL and not SIGXCPU. Measured both ways, a bomb that ignores `SIGXCPU`
and a plain one that does not: **exit 137 for both**, never 152. That is a decompression bomb, a
file verdict that must record, and it is indistinguishable from the OOM kill at the only layer
where either is visible. A discriminator keyed on 152 does not exist to be built.

**The premise is close to unreachable here anyway, for one decoder.** The sandbox caps address
space at 2 GiB, so a memory bomb surfaces as the decoder's own non-zero exit long before the box is
short: re-measured at the 2 GiB cap, the probe's `as_rlimit_bomb` row is still exit 1, a Python
`MemoryError` and not a kill. The box carries 19 GiB of RAM and 38 GiB of swap. **The cap bounds
address space and not resident memory, so it does not net against those 19 GiB**: `sandbox.rs`'s
own test has the kernel admit the same `PROT_NONE` reservation at 1536 MiB and refuse it at
3072 MiB while `VmRSS` stays under the test's 256 MiB ceiling, so the cap decided on the mapping
and not on a resident page. What the cap change moves is the composed case, and it moves it in
permitted address space: `src/backend/run.rs`'s `THUMB_WORKERS` is 6 since 0.3.2, so six decoders
multiply the permitted total to 12 GiB; at four workers it was 8. The case
that matters is the bomb that faults its pages in instead of reserving them sparsely, because that
is the only one that turns permitted address space into memory the box has to find. Not run,
deliberately: six decoders each faulting in a 2 GiB bomb at once is the state this paragraph warns
about, not a test to schedule on the box.

Only a SIGKILL of the sandbox launcher itself arrives as `signal=9`, and that process decodes
nothing, allocates nothing and burns no CPU, so it can reach neither rlimit; with
`--die-with-parent` the realistic producer of that signal is something killing Flea's whole
process tree, at which point nothing records anything. The probe's six rows are `inner_sigkill`
exit 137, `as_rlimit_bomb` exit 1, `cpu_rlimit_ignoring_xcpu` exit 137, `inner_exit_1` exit 1,
`outer_sigkill` signal 9 and `cpu_rlimit_plain` exit 137.

**Two discriminators were looked for and rejected, so nobody re-derives them.** `--json-status-fd`
carries the same `128 + n` encoding as the exit status and so adds nothing. `wait4` rusage could
separate a CPU-limit kill from an OOM kill by `utime`, but it needs raw FFI replacing
`Child::wait`, and on this box there is nothing for it to discriminate.

This replaced a `try_wait` plus `sleep(25 ms)` loop on 2026-08-29. It cost up to a full 25 ms of
pure latency per job: measured through the real pool at 4 workers on 35 videos, 61 of 70 `child`
durations sat within 1 ms of a multiple of 25 ms, and after the change 3 of 70 did, which is what
chance alone gives. The `child` median fell from 125.65 ms to 110.49 ms.

**What is recorded in `fail/` and what is not.** A marker means "this content cannot be
thumbnailed at this mtime", and every other application on the box honours it, so it is
written only when a thumbnailer actually ran on a readable file and failed or hung. A missing
thumbnailer for the type is not recorded, and neither is an input that will not canonicalise:
a file that vanished between the listing and the job is not a broken file, and recording it
would stop every application from ever thumbnailing that path again if it came back with the
same mtime, which a `mv` back, an `rsync -t` or a `git checkout` all produce. The failure path
is one `canonicalize` syscall with no child spawned, so the retry it allows is cheap and the
queue bounds it.

**A child that exits 0 having written nothing IS recorded.** `glycin-thumbnailer` exits 0 on
bytes it cannot decode and leaves the output file empty, measured here on 4 kB of `/dev/urandom`
named `.jpg`; a real jpeg truncated to 1 kB, by contrast, still yields a 1456-byte thumbnail and
is a genuine success, so emptiness and not partiality is the test. `run_one` therefore checks the
child's own output at the child boundary, immediately after `Ran::Succeeded`, and treats an empty
file as a failure to record. **The placement is deliberate.** Judging it at `stamp` instead would
be simpler by one line and wrong, because a `stamp` error also covers a write this process could
not complete, which is ours and not the file's; judging the child's own output keeps `stamp`'s
failure meaning "we could not publish" and keeps this one meaning "the decoder ran against these
bytes and produced nothing", which is exactly what a marker says. Before this, such a file was
retried at a full sandbox spawn on every viewport pass, measured at about 25 ms on both the first
and the second request, against about 0.05 ms once the marker answers it. **Stopping the rule at
emptiness is deliberate.** A non-empty but malformed output still discards without a marker,
exactly as before, because no thumbnailer on this box was observed to produce one, and retrying
is the conservative direction against recording a verdict nothing measured.

**Three failures were moved the other way, onto the not-recorded side.** A missing `bwrap` or
`prlimit` records nothing, see "Thumbnail sandbox". And `run_with_timeout` returns four states
rather than a bool: a child that could not be SPAWNED at all is `NotStarted` and records nothing,
because a fork that failed under memory pressure says nothing about the file, and so is a child
this process could not `wait` on, while a child that ran and exited non-zero is `Failed` and does
record and a child killed at the deadline is `TimedOut` and records only for a local file.
The rule that separates them is whether a decoder ever looked at the bytes. The seam that
bound the raw path while `argv` handed the child the canonical one, see "Thumbnailer specs",
was in the recording class and wrote a permanent marker for every video under a symlinked
directory; it is closed at the source rather than by widening this rule, because from inside
`run_one` a bad bind and a corrupt file are the same non-zero exit and cannot be told apart.
The `NotStarted` distinction cannot be reached from a test through `run_one`, because
`sandbox::available()` has already checked `prlimit` microseconds earlier and both paths
`discard` without a marker anyway; it is pinned at the `run_with_timeout` level instead.

**The destination is never handed to a child.** The output is a `thumbwrite::exclusive_temp`
name in the target directory, and the entry is published by `rename`, which is atomic. `create_new` is what makes the guard load bearing: with `create(true)` a
symlink planted at that name is followed and the file outside is created, measured here.
Every failing path removes its own temp before returning, so a directory of dangling symlinks
cannot litter the shared cache.

**The sandbox binds the temp FILE, not its directory.** The spec asks for the output temp file
to be the only writable path, and Task 6 measured both real thumbnailers writing in place
through a single-file bind, so `run_one` passes the temp to `sandbox::wrap` and the child can
write nothing else; see "Thumbnail sandbox".

**The temp, the stamping and the marker are `backend/thumbwrite.rs`**, documented under
"Thumbnail cache" beside the `tEXt` reader they have to agree with. This module calls three of
its functions and owns none of them.

**Cancellation is tested against a pinned worker, and the pin is load bearing.** `submit` then
`cancel` on the main thread races the worker's own pop, so a test that just does the two in a
row passes or fails on scheduling: with a 50 ms delay inserted between them it goes red here
while `cancel` is working perfectly. The tests therefore submit a job whose spec is a
`/usr/bin/sleep` of their own unique duration first, wait until that child appears in `/proc`,
and only then submit and cancel the job under test. **The gate is the child's own cmdline and
not its temp file**, because a child that has exec'd has already made every bind: gating on the
temp instead lets the test's own teardown delete it while `bwrap` is still setting up, which
fails the bind, which is a child failure, which records a `fail/` marker and recreates the
directory the test just removed. That reproduced 5 times in 5 under three concurrent suites and
0 times in 8 after the gate moved. `Pool::pending` exists for these tests and is `#[cfg(test)]`.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

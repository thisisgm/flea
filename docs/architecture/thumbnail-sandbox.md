## Thumbnail sandbox

`backend/sandbox.rs` builds the argv that confines a thumbnailer child. Untrusted bytes
reach media decoders there, which is the richest CVE seam in this subsystem, so the child
gets no network, no writable path outside its output directory, and a bounded amount of CPU
and address space. The module only builds the argv; nothing in it runs a process, and the
argv is exec'd directly and never through a shell.

**The sandbox is mandatory, and its absence fails the job closed.** `sandbox::available()` looks
for `bwrap` and `prlimit` on `PATH`, and when either is missing `run_one` returns `Outcome::Failed`
before spawning anything: the row answers with an empty `file` and nothing is written to the shared
cache. It used to run the UNWRAPPED argv instead, with no log line, no `fail` marker and no wire
signal, which was reproduced by pointing `PATH` at an empty directory and watching glycin produce a
real 60 kB thumbnail from an untrusted file with no namespace, no network isolation, no rlimits and
full access to `$HOME`. The composed case is the point: with the sandbox up, a hostile
`~/.local/share/thumbnailers/x.thumbnailer` is contained, and with `bwrap` absent that same file was
unconfined code execution as the user. One missing package must not turn the first into the second.
**No `fail/` marker is written for this**, on the same reasoning as a vanished input under
"Thumbnail pool": a missing package is not a broken file, and a marker keyed by URI plus mtime would
stop every application on the box from ever thumbnailing that file again. The backend also checks
once at startup and prints one line on stderr, so the cause is stated rather than silent; it is not
fatal, because listing directories does not need the sandbox. `tests/thumbs.sh` runs the real binary
with `PATH` pointed at an empty directory and asserts the empty `file`, no cache entry and no marker.

The default decoder's shape is `prlimit --cpu=30 --as=2147483648 bwrap <flags> <inner>`; the archive class
below is that same argv with the `--cpu=` argument dropped and nothing else. **`prlimit` is the
outermost program because `bwrap` has no rlimit option**, verified against `bwrap --help`
on bubblewrap 0.11.2 here. Setting the limits from Rust would need raw `setrlimit`, which
`std` does not expose and which the zero-dependency rule forbids reaching for through
`libc`, so stock `/usr/bin/prlimit` from util-linux 2.42.2 carries them. `prlimit` is
outermost because it keeps the argv one flat exec and covers `bwrap`'s own setup as well
as the child, not because the limits would otherwise miss the decoder: rlimits survive
fork and exec, so the reverse nesting was measured to kill the same spin and to report
the same `/proc/self/limits`.

The two numbers: `--cpu=30` seconds, because a 1080p decode is well under a second of CPU
here and 30 s is a runaway rather than a slow file; `--as=2147483648`, two GiB, because
`--as` bounds address space and not resident memory, and issue #17's own trace is a
thumbnailer that could not `mmap` a thread stack. The original one GiB was set from
`ffmpegthumbnailer`'s tens of megabytes on the media fixture, which measured the wrong
thing: issue #17 reported `glycin-thumbnailer` exhausting one GiB on a large ICC-tagged
JPEG and aborting. What inside `glycin` reserves that much is not measured here and is not
claimed, but it is not the profile: the same 6000x3375 pixels generated with `-strip` abort at
exactly the same cap as the profiled ones, so PR #39's "the same image without ICC stays below
it" does not hold on this box. The amount is measured. Driven through the production argv on
`tests/thumbs.sh`'s own 6000x3375 ICC fixture, `glycin` 2.1.5-2 on this box aborts with
status 134 at `--as=536870912` and writes a 601-byte PNG at `--as=671088640`, so it wants
between 512 and 640 MiB for 81 MB of RGBA pixels. **This box therefore does not reproduce
the reporter's one-GiB abort**: the same fixture and the same argv pass at
`--as=1073741824` here (status 0, a 601-byte PNG), so their `glycin` wanted more address
space than ours does, and the headroom is bought on their number and not on this one. **Two
is the smallest value the ticket records as working**, and it is chosen on that rather than
on a judgement: issue #17 says increasing `--as` to 2 GiB or omitting it succeeds without
error, so 2 GiB is the reporter's own known-good number and three times what this box's
`glycin` was measured to want. The same fixture and argv pass at `--as=2147483648` here
too, status 0 and the same 601-byte PNG. The reporter proposes 4 to 8 GiB, so two leaves no
margin over their range and a larger image on their box could reopen the issue; that is
accepted, because the rung here is the smallest thing that works and the number moves again
when somebody brings a measurement. It is still finite and still refuses a decompression
bomb.

**Issue #357 gives f3d alone a 3 GiB address-space cap.** Its llvmpipe/LLVM mappings
can exhaust 2 GiB while resident memory stays much smaller. With f3d 3.5.0, Mesa
26.2.5 and llvmpipe at 32 threads, a 48,400-vertex OBJ sphere aborted at exactly
2 GiB with `LLVM ERROR: Unable to allocate section memory!`; at 3 GiB it rendered
with about 2.02 GiB virtual space and 401 MiB RSS. The executable's final path
component must be exactly `f3d`; mentioning it in an argument changes nothing.
Other decoders, read-only probes, archives and the forked video worker keep 2 GiB.
The 30 CPU seconds, 20-second thumbnail deadline, namespace and allocator arena
limit remain. `RLIMIT_RSS` is not a substitute: this kernel allowed a 192 MiB
allocation under a 64 MiB RSS limit. Address-space limits apply independently to
each process and survive fork and exec; they do not bound a whole process tree's
combined RSS. A cgroup would provide that different bound but adds a service
manager dependency and a separate launch per thumbnail. Concurrent sparse-mapping
and forked allocation-bomb tests pin the chosen finite per-process limit.

**Extract and compress run without the CPU cap, and that is issue #211.** `wrap` is
the decoder's wrapper. `sandbox::wrap_archive` is the one `run_boxed_cancellable` builds for
both archive jobs, the same flags and the same 2 GiB address-space cap with the `--cpu`
argument left out, and `prlimit` stays outermost so that cap still arrives. Convert runs
on the capped cancellable runner now and keeps the decoder's `--cpu=30`, because convert decodes
untrusted input and has no UI cancel. The office conversion runs in its own jail, `sandbox::wrap_office`,
with `--as=8589934592`, `sandbox::OFFICE_ADDRESS_SPACE_BYTES`: LibreOffice 26.8.1.1 converted nothing at 2 GiB
(`std::bad_alloc` or status 139), not every file below 3.5 GiB, and every file at 4 GiB. It has no CPU cap:
LibreOffice runs several threads, so a 200,000-row workbook spent 30 CPU seconds in 28 s of wall clock and
was killed (status 137), and the same file converts in 43 s with no cap. The office service's 60 s wall clock
and its cancel bound the run instead. The jail binds the system font cache, `/var/cache/fontconfig`, read-only:
without it every conversion rescanned the fonts, 1.3 to 1.6 s for a short document against 0.7 s with it, and
it holds nothing a user cannot already read. A converter killed by SIGKILL is reported by its signal, never by
its last stderr line, which for that workbook was fontconfig's list of unwritable cache directories; any other failure skips fontconfig's lines and reads the rest. A legitimate compress past 30 CPU
seconds is work rather than a runaway, the ticket's own finding for extract. Extract carries
the operator's cancel, which kills and reaps the child and discards the staging directory.
When the archive tool has not exited ten seconds after the cancel, its work
folder is left in place rather than deleted under a live writer, and the err names that folder.
Compress carries a quit-set flag, so a cancelled compress stops with the job; a UI cancel
for it is post-0.4.0 design work. The ticket is a 55 GiB Zip64,
74 GiB unpacked in 676 members, whose legitimate extract died under `prlimit --cpu=30`
after 30 s with status 137 and about 24 GiB of the staging tree written and nothing on
stderr, and the status bar reported that as
**"The archive tool failed."**: a sentence that names a bad archive for a job the kernel killed.
Neither zstd nor the tool choice is the cause, as the ticket's own measurements say.
`libarchive` 3.8.9 reads the archive's method 93, a one-member extract of that member inside
this jail is reported to succeed, and `7z` would be launched through the same wrapper and
die the same way. **No CPU-second
number replaces 30**, because every finite one is smaller than the next archive somebody brings, and a
slow unpack is legitimate work rather than the runaway this bound exists for. What bounds an
extract is the address-space cap, `--die-with-parent` and the operator's cancel. A compress
has the first two and the quit's cancel, so a hostile one runs until the tool finishes or a quit stops it. **The index read keeps the decoder's wrapper
deliberately**, and its own bound is why: `archivelist.rs` stops a read at `ARCHIVE_READ_MS`, 2 s of
wall clock, `archive_produced_count_inner` answers `None` for a read that failed, timed out or was
killed, and `extract` already reads that as unverified rather than as a failure, so a cap biting there
cannot turn into a published lie. **A job that died on a signal now says so.** One that wrote nothing
used to reach the status bar as the same empty-stderr fallback as a tool that exited non-zero in
silence, whatever had actually happened, so `archivework.rs`'s `failure_message` reads the status when
the tool wrote nothing: bwrap renders an application the kernel killed as exit 128+n and keeps a real
signal for its own death (measured here; see "Thumbnail pool"), so 137 is reported as "was killed by
signal SIGKILL (9)", a real signal as the same sentence, and any other non-zero exit as "exited with
status N". The tool's own last non-empty line of stderr still wins over both, because that is the
better diagnosis for a genuinely bad archive, and a blank or whitespace-only stderr no longer becomes
an empty message. **What this costs is accepted and stated.** The 30 s cap was incidentally a bound on
how much of the disk one hostile archive could fill before it was killed. An extract now runs until
the operator's Cancel reaches it, which is exactly what `7z x` on the same file outside Flea does.
A compress runs until the tool finishes or a quit cancels it, with no UI cancel yet. A
memory bomb is still refused, by the address-space cap.

The flags, and why each is there:

- `--unshare-all` drops every namespace, which is what removes the network.
- `--die-with-parent` means a wedged decoder cannot outlive the backend.
- `--new-session` detaches the controlling terminal so the child cannot inject input with
  `TIOCSTI`, and it puts bwrap's sandboxed side in a session, and so a process group, of its
  own. That is what bounds `src/backend/mediaprobe.rs`'s watchdog: measured on the box, the
  launcher's group holds exactly one process, the `prlimit` that exec'd `bwrap`, while the
  inner `bwrap` and the decoder share a group of their own, so `kill(-pid)` reaches only the
  launcher and what ends the decoder is `--die-with-parent` plus the PID namespace dying with
  it. Both were gone two seconds after the group kill.
- `--clearenv` empties the environment, so no `LD_PRELOAD`, no `XDG_*`, no session bus.
- `--ro-bind /usr /usr` and `--ro-bind /etc /etc` give the decoder its libraries and its
  loader configuration, read-only.
- `--symlink usr/lib /lib`, `/lib64`, `--symlink usr/bin /bin`, `/sbin` reproduce this
  box's merged-`/usr` layout. `ls -ld /lib /lib64 /bin /sbin` shows all four are symlinks
  into `usr`, so binding `/usr` plus these four lines is the whole filesystem the child
  needs.
- `--proc /proc` and `--dev /dev` are the minimal kernel interfaces a decoder expects.
- `--tmpfs /tmp` gives it scratch space that is discarded with the namespace.
- `--json-status-fd 7` names the sandbox init: bwrap writes `{"child-pid": N}` there, so a cancel
  landing during startup, before `--die-with-parent` is armed, kills that pid as well as bwrap
  instead of leaving the tool running. The pipe is made with `pipe2` `O_CLOEXEC` and only fd 7
  survives exec through the child's own `dup2`, so a spawn from any other backend thread mid-jail
  inherits nothing; a reported pid is killed only while `/proc/<pid>/stat` still names this jail's
  bwrap as its parent (field 4), and a forged one is refused.
- `--ro-bind <input> <input>` is the one file it may read, apart from the program below.
- `--ro-bind <target> <program>` puts the thumbnailer's program in reach (issue #171). `backend/thumbbind.rs`
  adds it between the output bind and the inner argv, from the stored program path P and its canonical
  target C. The jail roots are `/usr`, `/etc`, `/bin`, `/sbin`, `/lib` and `/lib64`: the jail mounts `/usr` and
  `/etc` and links the other four into `/usr`. P under a jail root, C under `/usr` or `/etc`, and every hop of P's link chain
  under a jail root: no bind. P under a jail root otherwise: `--ro-bind C L` for each hop L of the chain that lies outside
  all six jail roots, and `--ro-bind C C` when C lies outside `/usr` and `/etc`, because the jail follows P's link
  text and a hop it does not hold answers `ENOENT`. P anywhere else: `--ro-bind C P`, so argv[0] stays P.
  Every bind is the same read-only file, so the program's directory and its siblings stay out of the jail. A job that needs a
  bind is refused before the jail starts, through `discard` with no `fail/` marker like a missing `bwrap`, when C is not
  a regular file with an execute bit, is owned by neither root nor the backend's uid, or is writable by
  group or others, when P, C or any hop of the chain is under `/proc`, `/dev` or `/sys` or equals the input or output
  path, or when P is relative or holds a `..`. `--bind <out>` below stays the only writable bind.
- `--bind <out> <out>` is the one path it may write, which production makes the temp file itself.

**A bare program name resolves without a `PATH`.** Five of the nine shipped `.thumbnailer`
files name their program by bare name, see "Thumbnailer specs", and `--clearenv` means the
child has no inherited `PATH` at all: the historical `env` probe inside the sandbox printed only
`PWD=/`; the current wrapper also explicitly sets `LC_ALL=C.UTF-8` and `MALLOC_ARENA_MAX=2`, so a
present-day probe prints those two alongside `PWD=/`. `bwrap` execs
through `execvp`, whose glibc fallback when `PATH` is unset is `confstr(_CS_PATH)`, measured
here as `/bin:/usr/bin`. All four installed bare-name thumbnailers live in `/usr/bin`, so
they resolve, and a bare-name `ffmpegthumbnailer` under the shipped flags produces a real
PNG from the video fixture. **No `--setenv PATH` is needed and none is set.** The fallback
does not decide a thumbnailer's program, because load stores the absolute path validation found
(see "Thumbnailer specs"), so a bare name that lives in `/usr/local/bin` or `~/.local/bin` runs by
its path.

**Measured overhead, nine interleaved pairs with the arm order alternating.** Against a
trivial `/usr/bin/true` child, which isolates the wrapper from the decode, the full
`prlimit` plus `bwrap` wrapper costs in the low single-digit milliseconds: the nine deltas
ran 3.3 to 4.1 ms with a median near 3.7 ms, and `prlimit`'s own share of that was 0.2 to
0.9 ms, median near 0.4 ms. That leaves `bwrap` at roughly 3 ms, consistent with the 3.2 ms
recorded for `bwrap` alone, so adding `prlimit` costs well under a millisecond. Against a
real `glycin-thumbnailer` run the same nine pairs gave 1.2 to 5.8 ms, median near 2.5 ms,
but the decode itself is about 45 ms and drifts by as much as the effect, so the isolated
number is the one to quote. Either way the wrapper is under a tenth of one thumbnail's
cost. Do not treat any of these as a citable constant; two batches of the same harness on
this box have disagreed by more than 2x. `systemd-run --user` was measured at 23.8 ms and
rejected on that number.

**Confinement results, all measured on the box.** It produces a real 256 px PNG through the
full argv; the canary file outside the output directory survives and a write to it fails
with `No such file or directory`; `/home` is not visible at all and the root holds only
`bin dev etc lib lib64 proc sbin tmp usr`; `getent hosts example.com` exits 2 with no
resolution; `ulimit -v` inside reads 2097152 KiB, so the address-space limit is applied;
and a spin under `--cpu=2` is killed rather than returning 0. **The `sh -c` in those probes
is a test OF the sandbox, not production code.** Production execs the argv directly, and
`grep -rn 'sh -c' src/` finds nothing.

**The writable path is whatever the caller binds, and production binds one file.** `wrap`'s
third argument is the single path mounted `--bind`, and `thumbs.rs` passes the pre-created
temp file rather than its parent directory, so that temp is the only path the child can
write. Binding a single pre-created file was measured to work for both `glycin-thumbnailer`
and `ffmpegthumbnailer`, both writing a valid PNG through a `--bind <out>/t.png <out>/t.png`,
and `glycin-thumbnailer` was re-checked against a 1x1 PNG when the pool started using it. A
caller may still pass a directory, which is the looser bind and what a thumbnailer that wrote
to a temporary name and renamed would need; only those two thumbnailers were probed.

**A cancel kills the sandbox init as well as bwrap (`src/backend/jail.rs`).** `spawn_jailed`
learns the init pid from `--json-status-fd`, and `kill_tree` waits a bounded 200 ms (`STATUS_WAIT_MS`)
for that pid when none is stored yet, SIGKILLs it only while `/proc/<pid>/stat` still names this
jail's bwrap as its parent (field 4, so a forged pid is refused), then the launcher process group
and the child, reaps bwrap within `REAP_WAIT_MS` (500 ms), and hands a reap the kernel still holds to a detached thread (`src/backend/jailreap.rs`), so no caller waits on it; then it waits up to 2 s (`WAIT_GONE_SECS`) for the killed pid to vanish.
The wait ends at once when the reader sees EOF, which it records as 0 when no pid arrived; there is
no kill after the reap, since the kernel reparents the init there and no parent check can pass.
The metareq and mediaprobe watchdogs kill through the same gate while the child is still alive,
and the reader thread is built with `Builder::spawn`, whose error kills the started child and
answers the caller's `NotStarted`.

Every pid signal goes through `src/procsignal.rs` (`signal_group`, `signal_process`), which refuses any pid <= 1 before the syscall so no computed pid can become `kill(0)` or `kill(-1)`; never declare `kill` anywhere else.
**A video the worker takes is confined without this argv.** `sandbox::wrap_worker` runs
`flea --thumb-worker` under the same flags minus the two binds and minus `prlimit`, and each child
it forks sets the same two limits itself through an `extern "C"` `setrlimit` from the system libc
that `std` already links, so no crate is taken and the zero-dependency rule above holds.
"Thumbnail worker" has what stands in for the binds.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

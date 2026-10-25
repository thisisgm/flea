## Thumbnail worker

**42 of the 97 ms a video thumbnail cost was `ffmpegthumbnailer` linking itself**, about a hundred
shared libraries, once per file; the sandbox around it is about 5 ms. `flea --thumb-worker`
(`backend/thumbworker.rs`) links `libffmpegthumbnailer.so.4` once and forks one child per video,
and `backend/workerlink.rs` is the backend's side. It changes how a video is decoded and nothing
about what may be thumbnailed: the `.thumbnailer` files stay the only source of that.

**Which jobs it takes.** `workerlink::worker_shape` accepts exactly the program ffmpegthumbnailer
ships, `Exec=ffmpegthumbnailer -i %i -o %o -s %s -f`, bare or by path, with `-f` optional. Any
other program, any other flag, `%u` for `%i`, or a repeated flag keeps the job on the exec path
described under "Thumbnail pool", so the worker never guesses at an option it does not implement.

**The image is the program's own, pixel for pixel.** `-s N` is `video_thumbnailer_set_size(N, N)`
and `-f` is the struct's `overlay_film_strip`; measured on minipc with ffmpegthumbnailer 2.3.1,
`generate_thumbnail_to_buffer` so set wrote the CLI's exact bytes for all 43 videos on the media
fixture's first screen and for 360x640, 640x360, 200x200 and 1080x1920 test clips, each handed its
real path. `set_size(N, 0)` differs on every portrait clip, so it is not the CLI's `-s`. **Through
Flea the published entry differs in one optional key**: the program writes `Thumb::Mimetype` from
the input's extension, and the worker's `/proc/self/fd/3` has none, so the worker's entry omits it.
The pixels and every other key match, which `tests/thumbs.sh` asserts on the entries both paths
publish for the same file, and the key's presence is what tells that test which path made each
entry; the freedesktop spec makes the key optional and Flea's cache reads only `Thumb::URI` and
`Thumb::MTime`. **libav does not lean on the missing extension**: on 2026-09-21 the program run on
`/proc/self/fd/3` gave the same pixels as its run on the real path for all fifteen containers
probed, mp4, mkv, webm, avi, mov, ts, mpg, flv, 3gp, wmv, m4v, ogv, m2ts, raw h264 and vob.

**One worker for the pool, started lazily.** The first job that qualifies spawns it under the pool's
own bwrap flags from `sandbox::wrap_worker`: every namespace flag, `--clearenv`, read-only `/usr`
and `/etc`, and the flea executable itself bound read-only, and no input or output bind at all.
There is no `prlimit` around it, because the limits are per job and every child sets its own after
the fork. A cap on the worker would be inherited by every child anyway, and the 30 s `RLIMIT_CPU`
would also count the worker's own CPU for the whole session, which `RLIMIT_CPU` accumulates and
which never includes its reaped children: measured on minipc at 9 ticks for 60 videos, about 1.5 ms
a job, so such a cap would end the worker after roughly 20,000 videos. Pool threads meeting their
first video together start one worker between them, because it is spawned under the link's lock.
**It stays resident for the rest of the backend's life**, and that is its cost: about 20 MB of PSS
for the worker, 37.5 MB resident, and a third of a megabyte for its two `bwrap` processes, against
the backend's 2.5 MB, in one sample on minipc after one video. The libraries it keeps linked are
that number; nothing else in Flea maps them.

**One job sees one file, and can write nothing else or signal anyone.** The backend opens the
input read-only and the pre-created temp write-only and passes both, with a reply socket, over the
worker's stdin as `SCM_RIGHTS` (`backend/fdpass.rs`). The child the worker forks for it then:

- keeps `/dev/null` on 0 to 2, the input on 3 and the output on 4, and closes everything else, which
  is the request socket, every other job's reply socket and every sibling's pidfd;
- sets `RLIMIT_CPU` 30 s and `RLIMIT_AS` 2 GiB, the values `prlimit` applies on the exec path;
- sets `no_new_privs` and a Landlock ruleset handling every right that writes, creates, removes,
  renames or truncates, with no rule granting any, and from Landlock ABI 6 scoped for signals, so it
  cannot signal a sibling, the worker or anything else outside its own domain. Reading is not
  handled, so it still reads its libraries and its input;
- installs a seccomp filter that answers `EPERM` to every call changing a file's mode, owner, times
  or extended attributes, to every `ioctl` and to `io_uring_setup`, whose ring has xattr operations
  of its own, answers `EPERM` to every x32 call, whose number carries `__X32_SYSCALL_BIT` under the
  x86_64 arch, and kills a call from any arch but the one it was built for, x86_64 or aarch64 (any
  other target fails to compile until it has its own table). Landlock leaves all of those to the file's
  owner: measured on minipc, kernel 7.2 at Landlock ABI 10, a process under this ruleset alone
  reopened descriptor 3 for writing and got `EACCES`, then `fchmod`, `futimens` and `fsetxattr`
  through that same read-only descriptor all changed the operator's file, where the exec path's
  `--ro-bind` answers `EROFS`. The same filter refuses `fork`, `vfork` and a `clone` without
  `CLONE_THREAD`, so a job can start threads, which die with it, and never a process that would
  outlive the `SIGKILL` its deadline sends, where each exec-path job's own bwrap took every
  descendant down with it; `clone3` answers `ENOSYS`, because its flags sit behind a pointer the
  filter cannot read, and glibc then falls back to `clone`. The numbers are each arch's own from
  `asm/unistd_64.h`; aarch64 has no `chmod`, `chown`, `utime`, `fork` or `vfork` at all, only the
  `*at` forms and `clone`. On aarch64 only the two thumbworker tests below have measured it, under
  kernel 6.8 at Landlock ABI 4, so there without the signal scope (issue 187);
- hands libav `/proc/self/fd/3` and writes the encoded PNG to descriptor 4.

**The Landlock step is not optional, and here is why.** A read-only descriptor is not a read-only
file: measured inside this exact bwrap, a child holding one reopened `/proc/self/fd/3` for writing
and could have rewritten the operator's video, which the exec path's `--ro-bind` makes impossible.
With the ruleset the same reopen is `EACCES`, a direct open of the path is refused too, the read
reopen still works and the file is untouched. `thumbworker::tests::
a_confined_child_holds_only_its_job_and_can_write_or_signal_nothing` runs twice, once with its two
files on 3 and 4, where recvmsg lands a job in an idle worker, and once well above them, where it
lands one in a busy worker, plants a socket on descriptor 0, where the worker's request socket sits,
runs `confine()` itself in a copy of the test binary and reports through descriptor 4, the way a job
writes its PNG, that exactly descriptors 0 to 4 are open, 0 to 2 are `/dev/null`, 3 is the input,
both limits are set, neither the input nor its path opens for writing, the path cannot be truncated,
the input still reads, the parent can no longer be signalled, its mode and times can no longer be set, `fsetxattr`, `ioctl` and
a `fork`, which glibc makes through `clone`, answer `EPERM` and a thread still starts; the `_before`
facts are the negative controls, taken unconfined with a `fchmod` to the file's own mode, a
`futimens` that omits both times, a `truncate` to the file's own length and a fork whose child only
exits, so they change nothing. The
probe makes a few of the refused calls for real;
`thumbworker::tests::every_call_that_changes_a_file_or_starts_a_process_is_refused` covers all of
them without making any, by running the built filter the way the kernel does for each call named in
its own list, at the number `/usr/include/asm/unistd_64.h` gives, and checks every entry of the
table against that header by name, so an entry deleted from the table or typed with the wrong number
goes red; the thread bit is checked against `/usr/include/linux/sched.h` the same way, with the
flags glibc's `pthread_create`, `fork` and `posix_spawn` pass. **The signal scope is what lets the
children share one PID namespace**: every exec-path job had a namespace of its own, and without the
scope a decoder compromised by one video could kill a sibling mid-decode, or the worker. corner: a
kernel at Landlock ABI 3 to 5 still gets the worker, without that scope. A kernel without Landlock,
or with one before ABI 3, which cannot deny a truncation, gets no worker at all: the worker answers
`K` and the exec path takes every video. The
worker is also not dumpable, which children inherit, so no process of the same user can ptrace it or
read its descriptors.

**The worker's only final verdict is a thumbnail.** It reaps each child through a pidfd, kills one
still running at `JOB_TIMEOUT`, and writes one byte on that job's reply socket: `S` for exit 0, `F`
for a refusal, a signal or the deadline, and `N` for exit 2, a failure of this machine inside the
child such as a confinement step refused or a write that failed. `S` publishes the thumbnail. `F`
sends the job down the exec path, which judges the file exactly as it always has, so only the
thumbnailer program ever writes a `fail/` marker: the worker's child is more confined than the
program, with no writable `/tmp`, and a file only the worker failed must not be recorded broken on
its word. `N` retires the worker, as below. corner: a video that hangs the decoder costs two
deadlines, one in the worker and one on the exec path, paid once locally and on every request
off-local, where a timeout records no marker. `workerlink::tests::
only_a_thumbnail_is_final_and_a_machine_failure_retires_the_worker` answers one request each way
from a stand-in worker and pins that only `S` publishes, that `F` keeps the worker and that `N`
retires it, and `gone_because` is what names an `N` apart from a worker that stopped answering.
`thumbworker::tests::a_verdict_is_read_from_how_the_child_ended` forks a real child for each ending,
exit 0, 1 and 2, a signal, and exit 0 or a hang past the deadline, and pins the byte `finish`
answers. The child never holds its reply socket, so a reply that closes with no byte can only mean
the worker died.

**Every failure of the worker falls back, and none judges a file.** No worker, a library that will
not load, no Landlock that can deny a truncation, a request that cannot be sent, an `N`, a reply that closes with no byte, or
no word within `JOB_TIMEOUT` plus 5 s all retire the worker for the rest of the process and send
that job, and every later one, down the exec path. One stderr line names the cause, such as `flea:
the thumbnail worker did not start (libffmpegthumbnailer.so.4 did not load), so videos use the
thumbnailer program`, or no Landlock, no answer within 5 s, an exit before any answer or `bwrap` not
starting, and once it ran, `failed a job on this machine`, `stopped answering` or `stopped taking
requests`. The backend says it because the worker's stderr is `/dev/null`, like every child's, so
libav's logs never reach the operator's. Before this, a hard `RLIMIT_AS` below 2 GiB made every
child refuse its confinement and every video answer empty with nothing on stderr and no fallback;
reproduced on minipc under `prlimit --as=1900000000:1900000000`. Under that limit the exec path it
now falls back to fails too, because `prlimit` cannot raise a hard limit, and records a `fail/`
marker for the video exactly as it already did for every image; the stderr line is what says so.
corner: minipc's hard `RLIMIT_AS` is unlimited, in the Hyprland session as in a login shell, so that
limit exists only where someone sets it. An input that no longer opens, or is no longer a regular
file, is answered as `NotStarted` without touching the worker and records nothing, where the exec
path writes a `fail/` marker for a file it cannot read; `tests/thumbs.sh` asserts the missing
marker for an unreadable video, which is what pins that branch. The request policy already answers
a row that is not a regular file, so only a file swapped after that check reaches the other branch,
and `workerlink::tests::an_input_that_is_not_a_file_to_judge_never_reaches_the_worker` drives a
fifo and a vanished path through `generate` itself. `FLEA_THUMB_WORKER=off` starts with the worker retired:
`tests/thumbs-exec.sh` runs all of `tests/thumbs.sh` that way inside `tests/run-all.sh`, and it is
how an operator gets the exec path back.

**Measured before it was built**, outside Flea, from the research session's `prefork.c`: the first
43 videos of the media fixture took 828 ms at 4 at a time against 1489 through the sandboxed exec
path, and 2595 against 4387 one at a time.

**One libav thread per video.** `Decoder::load` looks up `av_cpu_force_count` through the same `entry` lookup and calls it with 1 when found, so the worker forces the count once before it serves and every forked child inherits it; a missing symbol changes nothing. Measured on West (ffmpeg 9.0.1, ffmpegthumbnailer 2.3.1, 12 CPUs, 43 first-screen videos): serial child CPU 27.6 to 28.2 ms per video down to 21.3 to 21.4 ms, 6 at once 31.9 to 32.5 ms down to 25.3 to 26.0 ms, and the PNG bytes were identical for all 43 fixture videos and six varied synthetic clips. On idle West the wall time to one thumbnail fell too, for 4K HEVC 91.8 to 87.3 ms, 4K AV1 35.3 to 30.7 ms and 1080p HEVC 30.0 to 26.2 ms, with identical PNGs, because decoders keep libavcodec's default of one thread and only the automatic pools change.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

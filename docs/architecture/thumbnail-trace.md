## Thumbnail trace

`FLEA_THUMB_TRACE`, set and non-empty, makes the backend write one line per reported job to
**stderr**. Off by default, and off it costs a `None` on `Job` and nothing else: the environment
is read once for the process behind a `OnceLock`, and an untraced job reads no clock and carries
no marks.

```
flea: trace row=12 depth=8 queued=182.41 setup=0.31 child=94.77 after=1.02 total=278.51
```

`row` is the listing row the job answers and `depth` is the number of jobs already queued when
this one was submitted, which is what says whether the workers were starved. The four durations
are milliseconds and they SUM to `total`, because all of them are deltas from one `Instant`
captured in `thumbs::trace` at submit rather than four clocks of their own. Each names a
different suspect:

- `queued`, submit to the worker's pop, is queueing.
- `setup`, the pop to the moment the argv is handed to the spawner, is worker startup, the cache
  key, `create_dir_all`, `exclusive_temp`, the argv build and the sandbox wrap.
- `child`, that moment to `run_with_timeout` returning, is the decode. **It is not pure decode.**
  It carries the `bwrap` and `prlimit` fork and exec, and about a millisecond of notice latency,
  because `child.rs` waits on a pidfd; see "Thumbnail pool". Until 2026-08-29 it also carried up
  to a full 25 ms poll step, which is how the poll was caught: every one of 70 sampled `child`
  durations landed on a multiple of 25 ms.
- `after`, the child to the row being flushed to stdout, is `stamp`, the `rename`, the result
  channel, the forwarder hop and the write itself.

**The line is written in `run.rs`, not in the pool**, so `after` covers the whole tail including
the single-threaded report path. The consequence is that it is a line per REPORTED job, and THREE
paths answer a row without reporting it, so a trace-line count is not a job count:

1. A job whose listing was superseded is dropped in `report_done` before it is reported.
2. A row the drain deadline cut short is answered in `drain` rather than by a result.
3. A row past `MAX_DEFERRED` is answered by `queue_row` with a bare `thumbed_line` and never enters
   the pool, so it carries no `Trace`. It is not a defect, since the row IS answered, but a debugger
   reconciling counts will trip on it.

**stderr is not negotiable.** stdout is the wire, `ui/Backend.qml` parses every line of it, and a
trace line there is reported to the user as a line this build cannot read. `tests/thumbs.sh`
pins all three of on-stderr, never-on-stdout and silent-when-unset.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

## Backend memory levers that were measured and dropped

Measured on 2026-08-30 by Plan 5's Task 5b, warm, against a backend driven over its own wire with
THP disabled in the child exactly as the shipped launcher leaves it. Every `Pss` here is
`smaps_rollup`'s `Pss` in KiB; divide by 1024 for MiB, and never subtract these against a figure
taken with a divisor of 1000.

**Read "The listing arena returns to the OS" before this section.** The glibc threshold lever is
NOT in this list: it SHIPPED, and the first round of this task wrongly recorded it here as a null.
The measurement that produced the null opened one large directory and then a small one, and that
sequence is structurally incapable of showing the ratchet, because the ratchet only strands the
NEXT large allocation and there was never a second one. **A null is only as general as the
sequence that produced it.**

**Building the thumbnail `Pool` lazily is worth 0.042 MiB and about 0.32 ms, and both are below
anything that matters.** Once the duplicate table parse is gone (see "Thumbnail pool") all that
eager construction still costs is four worker threads blocked on a condvar. Measured by building the
same tree at `THUMB_WORKERS` 1 instead of 4: three fewer threads move `Pss` from a 1337 KiB median
to 1305 over nine interleaved pairs, about 10.7 KiB a thread, so four are about 43 KiB, which is
0.9 percent of the 4957 KiB the backend holds on the scale fixture. The startup half of the cost
was measured separately over forty interleaved pairs, fork to the first `listed` line: 3.3665 ms
against 3.1281, so three threads are 0.238 ms and four are about 0.32 ms, against a 77 ms startup
budget. The sign split is 35 of 40. The 202,800 KiB of `VmSize` those three threads also give back
is glibc arena and stack reservation with zero resident pages.

**Nothing blocks the laziness, and that was checked before it was dropped.** There is no recorded
reason for the eager construction anywhere: not in `9d26434`, which introduced it, and not in this
file. The nearest constraint is `run.rs`'s note that the workers hold `Sender<Done>` clones so the
forwarder can never see a disconnect, and that survives trivially by holding the original sender
until the pool is built. Nor is there a race, because `worker` only waits on the condvar while the
queue is empty, so a job pushed before a worker has run is found on that worker's first lock. It
is dropped on its size alone.

**`glibc.malloc.arena_max=1` is not a memory lever.** It removes 393 MiB of `VmSize`, 410,988 KiB
down to 17,904, which is arena address-space reservation with zero resident pages. Its effect on
`Pss` was 1374 against 1394 KiB on a **single** unpaired sample, which is one sample and is not
evidence of a direction; a second party's single sample pointed the other way. It is recorded
here only as the control that proves this box reads `GLIBC_TUNABLES` at all. That is the backend, whose arenas hold
nothing resident; the TUI is the case where a helper thread's arena does keep pages, see "The listing arena returns to
the OS".

**0.3.8's backend growth is mostly the clipboard owner, measured 2026-10-03 (1b08a2f7 against
759fd8eb).** The release binary grew 182,616 bytes, 3,514,144 to 3,696,760, and the backend's
`Pss` grew with it, 3,896 to 4,086 KiB with THP off, 180 KiB of that file-backed. `Cargo.toml` and
`Cargo.lock` are identical at both, so no dependency arrived. By section (`size -A`), `.text` is
+155,056 bytes, `.eh_frame` +13,768, `.gcc_except_table` +6,868 and `.rodata` +3,808; the unwind
tables follow the code and `.rodata`'s share was not attributed. By symbol (`nm`), the growth is
154,018 bytes: 84,594 are the owner's own code (`src/clip/` and `src/backend/clipreq.rs`), 20,255
are std generics that name neither the owner nor the picker (9,421 of `hashbrown` for six new key
types and about 6,700 for the channel of the owner's ready line), 9,727 are the picker, 1,884 are
the rest of Flea, and the remaining 37,558 are std generics typed by the owner (channels, thread
shims, drop glue). So about four fifths of the new code is the owner's by name, and the rest is
the picker and generics no symbol ties to a feature.
Three levers were measured or sized and dropped. Sending the owner's ready line as a `String` in
place of `(bool, String)` returns 12,160 bytes of binary and no `Pss` a ten-run batch can see,
4,074 against 4,074 KiB. Replacing the `SetQueue` channel with a mutex and a condvar was sized at
about 12 kB from its named symbols and not built, for the same reason. `panic = "abort"` returns
567,208 bytes and 355 KiB, and is refused: `SetQueue` and the backend's threads contain a
panicking request with `catch_unwind`. So the two channel levers are below what `Pss` can show,
and the one lever that is not gives up panic containment.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

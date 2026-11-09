## Why the listing is an arena

`Listing` (`listing.rs`) holds one `String` with every entry's name written back to
back, plus a `Vec<Span>` of `{off: u32, len: u32, is_dir: bool}` indexing into it. A
100,000-entry directory then costs one allocation for the names and one for the spans,
instead of 100,000 individual `String` allocations. Sorting moves 12-byte `Span` values
(the compiler pads the struct to 12 bytes for `u32` alignment), never the name bytes
themselves, so `sort_by_name` never touches the arena at all. Offsets are `u32`, so the
arena tops out at 4 GiB of names, which no directory reaches but a later search phase
reusing this arena could.

**The comparator is macOS Finder's, not a byte compare, and it costs about 2.3x.** Three properties
apply per segment as two names are walked together: a run of digits compares by value so `file_2`
comes before `file_10`; letters compare case-insensitively so `Apple` sits beside `apple` instead of
in an upper-case block above it; and leading zeros do not change a value, so `file_01` and `file_1`
are equal and fall through to a tie-break. **The tie-break is a raw byte compare and it is not
optional**: case-insensitivity makes `README` and `readme` equal, and a stable sort would then
preserve readdir order, which is not reproducible across runs.

A digit run is compared without parsing, because a 40-digit run overflows every integer type: skip
leading zeros, longer remaining run wins, then compare the digits as bytes. **corner: ASCII only.**
Finder also collates a diacritic beside its base letter, which needs a collation table, and this
tree ships zero dependencies with `Cargo.lock` byte-identical as a gate, so a byte above `0x7f`
falls back to byte order, which for UTF-8 is code point order. A name that is not UTF-8 reaches the
comparator as its exact text, its lossy spelling then a NUL and its bytes in hex (see "A name that is
not UTF-8 is carried exactly"), so it sorts by that spelling and its tail, and never ties a sibling.

Measured on this box, release build, seven runs each, medians: **100,000 rows 2.02 ms to 4.62 ms,
2,000 rows 0.225 ms to 0.586 ms.** So the listing hot path pays about 2.6 ms once on the largest
directory this product is benched against, which is inside the noise of a settle time measured in
hundreds of milliseconds. Zero allocation per comparison: `u8::to_ascii_lowercase` inline, no
`String`, nothing collected.

**The name sort is stable on purpose, and `sort_unstable_by` is a measured LOSS here.** Plan 5's
Task 5b made the substitution, proved it output-identical, and reverted it on time.

The order argument, for whoever revisits this. The comparator's key is `(is_dir, name)`, it
returns `Equal` only when both fields match, and POSIX gives each entry in a directory a unique
name, so the order is total and the substitution cannot change the output. Before 0.4.2 there was
one hole: `scan.rs` pushed the lossy spelling, so two distinct non-UTF-8 names in one directory
could tie. Each now carries its exact bytes in its text, so no two rows tie. While the hole stood it
was exercised rather than argued: a fixture with three file names and two directory names colliding
on two lossy strings sorted identically under both. **The output was byte-identical in all ten comparisons taken**,
four trees plus that fixture, ascending and descending, 100,000 rows down to 7, with the only
difference anywhere being the `rows` line's own `ms` stat-timing field, which is a measurement.

**The reason to revert is time.** btrfs returns `readdir` in creation order, which is largely
name order, so the stable sort's run detection is what this product's directories actually hand
it. The 100,000-file fixture comes back in 5 ascending runs and `sort_by` costs a 2.09 ms median
against `sort_unstable_by`'s 14.50 ms, twelve interleaved pairs with the arms alternating and no
run of either arm overlapping the other. The media fixture (401 ascending runs over 2001 entries)
reads 0.227 against 0.243 ms and `/usr/lib` (388 over 4734) 0.698 against 0.727 ms, nine pairs
each. **Those two are a not-a-loss and not a win**: the medians favour stable by 0.016 and
0.029 ms, but the distributions overlap heavily, media's A maximum of 0.289 sitting above B's
minimum of 0.228 and `/usr/lib`'s B minimum of 0.668 below A's minimum of 0.689. The conclusion
rests on the scale fixture and the shuffled control alone. Only a deliberately shuffled
20,000-file directory (9989 runs)
favoured unstable, 3.64 against 5.08 ms, and nothing on this box produces that shape by itself.
The 600 KiB of stable-sort scratch that motivated the change never showed up either: peak RSS
moved 4 KiB on the scale fixture, because the scratch reuses pages the arena's own growth had
already faulted in.

**The 7x magnitude is doubly fixture-dependent, and the direction is not.** Beyond the run count,
every name in that fixture shares the 15-character prefix `some_file_name_`, so each comparison
chews 15 identical bytes before it can diverge. Both arms pay that, but the arm doing several
times more comparisons pays several times more of it. A directory of short, early-diverging names
would narrow the ratio without changing its sign.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

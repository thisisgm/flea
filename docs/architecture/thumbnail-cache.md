## Thumbnail cache

The freedesktop thumbnail spec names every cache entry by the MD5 of the file's `file://`
URI: `~/.cache/thumbnails/large/<md5>.png`, with the same rule for `normal/` and for the
`fail/` markers every application on this box honours. There is no way to read or write the
shared cache without computing that digest, and reading it is the whole point, so
`backend/md5.rs` is a hand-written MD5.

It is hand written because the crate has zero Cargo dependencies and `std` ships no hash
that produces this digest. It is MD5 because the spec says MD5, not because MD5 was chosen:
the digest is a cache filename, never an authentication or integrity check, so its collision
weakness costs nothing here. A review finding that flags MD5 as a weak hash is answered with
that fact rather than with a different algorithm, which would simply miss every entry the
rest of the desktop wrote.

`hex` takes the URI bytes and returns the 32-character lowercase digest. Its tests are RFC
1321's own four vectors, the 55, 56 and 64 byte lengths that straddle the padding boundary,
and one real filename that exists in this box's cache,
`4b971187c53a6a6ff1925d1147d8dacf.png`, which is what proves the implementation
interoperates rather than merely being self-consistent. The two mutations it is proved
against are a zeroed padding byte and a big-endian length append; the second leaves the
empty-string vector green, because a zero length has the same eight bytes in either order.

`backend/thumbcache.rs` reads that cache. The root is `$XDG_CACHE_HOME/thumbnails`, or
`$HOME/.cache/thumbnails` when the variable is unset or empty, which is what runs over a
non-interactive ssh here. Under it, `large/<md5>.png` holds the 256 px thumbnails and
`fail/<application>/<md5>.png` holds the markers that say a file could not be thumbnailed.
`fail/` is namespaced by application because one program failing on a file says nothing
about another; Flea's namespace is `flea`, and the only namespace this box already carries
is `gnome-thumbnail-factory`. `Cache::new` computes that root, and `Cache::at` takes one,
so a test writes its markers into a temporary directory and never into the shared cache.

`lookup` checks `fail` before `large`, deliberately. A file that failed to thumbnail may
still have an old thumbnail sitting in `large` from before it was edited, and checking
`large` first would hand that stale image back and then retry the generation that already
failed. Three permanent tests pin this, all through `Cache::at` on a temp root: one writes
both entries for a single URI and asserts `Hit::Failed`, one writes only a `large` entry
and looks it up under a different mtime for `Hit::Miss`, and one looks the same entry up
under its own mtime for `Hit::Ready` naming the file it found. The third is not redundant:
without it the second passes on an implementation that returns `Miss` for everything.

An entry is only a hit when its `Thumb::MTime` text equals the source file's mtime. That is
the spec's whole staleness rule: the thumbnail carries the modification time of the file it
was made from, so an edited file misses and gets regenerated rather than showing its old
picture. A missing, unparsable or different `Thumb::MTime` is a miss, never a hit.

A `fail/flea` marker also carries an `X-Flea-Table` `tEXt` key, a 16 character `xxh64` hex
digest of the loaded thumbnailer table's sorted (MIME type, resolved program, arguments)
entries that `Thumbnailers` computes once when it is built and hands to `lookup` and
`record_failure`. A marker is `Hit::Failed` only when its mtime and its `X-Flea-Table` both
match, so a marker from another table, or one with no such key (every marker written before it
existed), is a `Hit::Miss`, the file is tried once more and a new failure rewrites the marker
under the current table; nothing sweeps old markers.

The metadata lives in uncompressed PNG `tEXt` chunks, and only those are read. `zTXt` and
`iTXt` would need a DEFLATE decompressor, which this crate has no dependency for, and no
thumbnailer shipped on this box writes them; every one of the 94 entries here is `tEXt`
written by `GNOME::ThumbnailFactory`. The chunk walk steps length, type, payload and the
4-byte CRC, stops at `IDAT` or `IEND` because no text follows the pixels, and returns
`None` rather than panicking on any truncation.

**A published entry carries exactly one of each key this application owns.** `stamp` inserts
`Thumb::URI`, `Thumb::MTime` and `Software` straight after `IHDR`, so a first-match reader such as
`png_text` sees ours. That is not enough: `ffmpegthumbnailer` writes a `Thumb::URI` of its own, a
bare canonical path with no `file://` scheme, and it survives later in the same file, so a
last-match reader saw a URI that is not the cache key. We own the entry we publish and two of one
key in it is a defect whichever one a reader picks, so `write_stamped` strips any existing `tEXt`
chunk carrying one of those three keys before inserting its own. Keys the thumbnailer owns are
left exactly where it put them: `ffmpegthumbnailer`'s `Thumb::Mimetype`, `Thumb::Movie` and
`Thumb::Size` all survive untouched. The strip pass is a rewriter and not a validator, so bytes
that do not parse as a chunk are copied through rather than dropped.

**Two URIs are built from one file, and collapsing them breaks half the thumbnailers.** The
CACHE KEY is `uri_for(job.path)`, the URI of the path the user actually named, because that is
what every other application on the box keys on and what `Thumb::URI` stamped into the PNG has
to equal or nobody can read the entry back, this tree included. The CHILD-FACING URI is
`uri_for(abs)`, built inside `thumbargv::argv` from the canonical path, because that is the one
path `sandbox::wrap` binds into the namespace. `%i` and `%u` are two spellings of that one
input; the cache key is a different thing that happens to have the same type. Giving the child
the raw-path URI while binding the canonical path lost every `%u` thumbnailer under a symlink,
which on this box is 54 of the 172 types the loaded table serves, every still image through
glycin and everything evince handles, against video, audio, office and xournalpp on `%i`. It
failed silently for the glycin types, because `glycin-thumbnailer` prints
`Error opening file ...: No such file or directory` and **exits 0**, writing a zero-byte file,
so the job read as a success and only died later when `stamp` refused the empty PNG. `argv`
therefore builds the child URI itself and takes no URI argument at all, so the two cannot be
handed the same value by mistake.

`uri_for` builds the URI the digest is taken over, and its literal set is GLib's, not RFC
3986's unreserved set. This was measured on the box rather than read off the spec: a probe
of every printable ASCII byte through `GLib.filename_to_uri` leaves `!$&'()*+,-.:=@_~`
and the alphanumerics literal, along with the `/` that separates components and is
structure, and escapes space, `"`, `#`, `%`, `;`, `<`, `>`, `?`, `[`,
`\`, `]`, `^`, `` ` ``, `{`, `|`, `}`. Non-ASCII is escaped one UTF-8 byte at a time. The
narrower RFC set is not merely stricter, it is wrong: this box's cache holds
`67471ae1929105f9f387addc0af2eb20.png` for
`file:///home/gm/Downloads/CleanShot%202026-08-17%20at%2018.25.25@2x.png`, whose `@`
GNOME left literal, and percent-encoding it yields a different digest and a permanent
miss. `a_uri_matches_the_one_gnome_wrote_for_a_real_entry` pins that entry, and the same
rule is what keeps the very common `file (1).jpg` shape addressable.

A warm hit on this box costs single-digit microseconds in a release build, roughly 9 for
the median across five runs, against roughly 24 in a debug build, for one failed open on
the `fail` path plus a 9912-byte read and the chunk walk. The maximum in a 50-iteration
batch runs 4 to 5 times the median on scheduler noise, so treat this as a magnitude and
remeasure rather than quoting a constant.

### The writer, `backend/thumbwrite.rs`

The writer lives next to this reader on purpose: the two agreeing is the only property either
of them has, so a change to one that is not made to the other is invisible until the desktop
stops matching Flea's entries. `png_text` above reads a `tEXt` chunk; `thumbwrite.rs` writes
one, and `a_stamped_png_reads_back_through_the_cache_parser` is the test that pins them
together against a real file on disk.

It owns the whole of publishing one entry safely: `exclusive_temp` opens
`.flea-<pid>-<16 hex>.png` with `create_new` at 0600, `stamp` reads back the bare PNG a
thumbnailer wrote and inserts three `tEXt` chunks directly after IHDR, and `write_marker`
puts the same three chunks, and `X-Flea-Table` when it is given a table, in a 67-byte 1x1
greyscale PNG for the `fail/` namespace. The keys are `Thumb::URI`, `Thumb::MTime`,
`Software` and, on a marker given a table, `X-Flea-Table`.

**The CRC-32 is in-tree because the cache is shared.** A `tEXt` chunk needs a CRC that `std`
does not have, and the alternative, skipping the chunks and keeping the mtime in a Flea-owned
sidecar, was rejected: `lookup` reads `Thumb::MTime` out of the PNG itself, so a cache Flea
writes but cannot re-read is worthless, and a `fail/` entry no other application honours
defeats the only reason for using the shared cache. It is twenty lines, its table is built
once in a `OnceLock` because it runs three times per stamped file, and
`the_crc_matches_the_standard_vector` pins it to `crc32("123456789") == 0xCBF43926`.

**`ONE_PIXEL` was generated and CRC checked on this box, not transcribed.** 67 bytes, three
chunks, `PNG 1x1 8-bit Grayscale`. An earlier hand-transcribed version was 66 bytes with a
wrong IDAT CRC that ImageMagick silently tolerated, so do not tidy that array. Both it and a
real generated entry were walked chunk by chunk against python's `binascii.crc32`; `xxd` is
not installed here, so use `python3` or `od`.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

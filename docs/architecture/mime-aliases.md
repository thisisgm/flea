## MIME aliases

`/usr/share/mime/aliases` is one pair per line, the alias first and its canonical name
second, separated by a single space. On this box it holds 357 lines with no comment line,
no blank line, no tab and no line carrying a third field, so `split_once(' ')` reads all of
it. `Aliases::from_str` still skips a blank line, a `#` line and a line with no space,
because the file is shipped by `shared-mime-info` and its shape is not Flea's to promise.

A thumbnailer may declare either side of a pair, and the two pairs that matter here fall on
opposite sides:

| Pair in `aliases` | Thumbnailer | Declares |
|---|---|---|
| `image/heic image/heif` | `glycin-heif.thumbnailer` | `image/heif`, the canonical name |
| `video/x-matroska video/matroska` | `ffmpegthumbnailer.thumbnailer` | `video/x-matroska`, the alias |

So neither "always canonicalise" nor "always alias" works: a rule fixed in either direction
misses one of the two container types. The rule is that **both** sides pass through
`Aliases::canonical` before they are compared, which maps each spelling onto the
equivalence class it names. The matcher owns that comparison; this module owns only the
table.

The table is many aliases to one canonical name, so it must never be inverted. `image/heif`
is the canonical name for three aliases here, `image/heic`, `image/heic-sequence` and
`image/heif-sequence`, and an inverted table would keep only whichever of the three came
last in the file. Reversing the insert is one of the two mutations this module's tests are
proved against.

`canonical` returns a borrow of its input when there is no alias, so a caller that
canonicalises every row pays no allocation for the common case. An unknown type and the
empty string both come back unchanged rather than erroring, and a missing file yields an
empty table rather than a panic, because a box without `shared-mime-info` should fall back
to icons rather than fail to list.

`Aliases::load` measured a p50 of 78 to 82 microseconds across three release runs of 50
iterations each, min 78 and max 195, against `mime::Db::load`'s 390 to 679.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

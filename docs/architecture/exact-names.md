## A name that is not UTF-8 is carried exactly

A Linux file name is bytes, and the wire, the listing arena and most of the code carry text. Before
0.4.2 `scan.rs` turned every readdir name into `String::from_utf8_lossy`, so a name such as
`col<0xff>` became `col\u{fffd}`, and every path built from that row named the sibling whose real
name is `col\u{fffd}`, when there was one. Trash, copy, move, rename, duplicate, tags and permanent
delete all acted on that sibling and left the shown file alone; with no sibling, they failed.

`src/bytename.rs` is the fix. A path component that is not valid UTF-8 is written as text in one
canonical form:

    <its lossy spelling>\0<its raw bytes in lowercase hex>

so `col<0xff>` is the text `col\u{fffd}\0636f6cff`. Three properties carry the design:

- **No real name holds a NUL**, so this text can never be the name of another file. An unconverted
  filesystem call on it is refused by the kernel with `EINVAL` instead of resolving to a sibling, so
  any call site the conversion has not reached fails safe rather than acting on the wrong file.
- **Every UTF-8 name is its own text, byte for byte.** The listing arena, the wire and every row of a
  UTF-8 directory are exactly what they were; only a row whose name is not UTF-8 grows, by one byte
  plus two per raw byte.
- **Only the one canonical spelling decodes.** `text_path` turns the tail back into bytes only when it
  is even-length lowercase hex, the bytes are not UTF-8, hold no `/` or NUL, and their lossy spelling
  equals the prefix. Anything else keeps its NUL, so a forged tail is refused, never resolved.

The text goes everywhere a path goes. The listing stores it in the arena (`Listing::name`), the wire
carries it in `n` and in every path field, `Listing::path(base, row)` and `text_path`/`view`/`exact`
turn it into the exact `PathBuf` at each filesystem call, and `path_text` (the `ExactText` trait's
`exact_text`) turns a path back into text wherever `to_string_lossy` used to. The undo journal, the
recovery records and the Trash review records store the text and decode it on replay. Clipboard and
drag URIs percent-encode the exact bytes, and a dropped or pasted URI whose bytes are not UTF-8 arrives
as the same text.

An argument vector cannot hold a NUL, so the text never reaches a child process as it is.
`bytename::args` and `text_name` turn each argument back into the bytes it names: the jail that runs
bat, the thumbnailers and the archive tools, the action runner, gio, rclone, LocalSend, AirDrop and
Taildrop sends, and the window launch. A process that starts Flea with such a path (`flea --gui`,
`flea-gui`, the launch helpers) reads its own `args_os` back into the text with `path_text`.

**Display stays lossy.** `bytename::display` drops each exact tail, so a person reads `col\u{fffd}`.
The GUI applies it in `list::outline::display`, the columns row cache, the grid caption, the path
crumbs, the rename editor, the column preview title, the Quick Look name bar and the contents search
results; the TUI through `Row::shown` and its path header. The name's kind, icon and search match are
read off the lossy spelling, where the extension is. Copy As writes text forms
lossily, because a text clipboard cannot carry raw bytes, and writes the URI form exactly and the
quoted form as a shell `$'...'` string with `\xHH` escapes, which reproduces the bytes.

Rename edits the lossy spelling: the source keeps its exact bytes and the new name is the text the
person typed. A surface no conversion reaches draws the tail as it is, and its writes refuse; neither
can touch the lossy twin.

The regression gate is `tests/exact_names.rs`, which drives a real backend through a folder holding
`col<0xff>` beside `col\u{fffd}` and a folder holding a lone `bad<0xff>.txt`: list, trash by row and
by path, copy and move by row and by path, undo of each, `paths`, tags, rename, duplicate, a
folder whose own name is not UTF-8, the menu snapshot and permanent delete, and a forged tail. For the
lone file it also runs meta, the code preview, fuzzy search and contents search, each hit resolving to the
file through `paths`.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

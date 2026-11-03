## Thumbnailer specs

`backend/thumbspec.rs` reads the freedesktop `.thumbnailer` files, which say which program
turns a file of a given MIME type into a PNG. The search path is `$XDG_DATA_HOME` then every
entry of `$XDG_DATA_DIRS`, each with `thumbnailers/` appended. Over a non-interactive ssh on
this box both variables are empty, so the defaults apply: `~/.local/share/thumbnailers`,
which does not exist here, then `/usr/local/share/thumbnailers`, then
`/usr/share/thumbnailers`, which holds all nine shipped files.

Files are parsed in search-path order, and within one directory in sorted path order, so a
duplicate declaration always resolves the same way; see "Deliberate corners".

**Only the `[Thumbnailer Entry]` group is read.** The field scan used to have no group awareness
at all, so an `Exec=` under any other heading was honoured and the last one in the file won. That
is a key naming a program to run, read out of a file in a user-writable directory, which is the
one place in this subsystem where being quiet is not an option. Keys before the first group and
keys in any other group are now skipped, and a file that never opens the group declares nothing.

The first directory on that path is inside the user's home and is writable without root, so
a `.thumbnailer` file is untrusted input naming a program to run. Two rules follow. First,
the program is validated before it can ever be run: `is_runnable` refuses anything that is
not a regular file with an execute bit, resolving a bare name through `PATH` and an absolute
name directly. Second, the argv `argv` returns is exec'd directly and never through a shell,
so a file named `a; rm -f canary.jpg` reaches the child as one argument and its semicolon
means nothing. Both were proved on the box: the same string through `sh -c` deletes the
canary and through a direct exec does not.

`TryExec` is optional, and **both it and the `Exec` program are validated**, not one or the
other. `com.github.xournalpp.xournalpp.thumbnailer` ships without a `TryExec`, so the first
token of `Exec` has to be checked anyway; checking only `TryExec` when it was present meant a
file could name `TryExec=/bin/true` and any `Exec` at all and still report `t:true`, which put a
row on the wire that no thumbnailer can serve. All nine shipped files
resolve here, and five of them name their program by bare name rather than by absolute path
(`xournalpp-thumbnailer`, `evince-thumbnailer`, `ffmpegthumbnailer` twice,
`gsf-office-thumbnailer`); only the four `glycin` files give `/usr/bin/glycin-thumbnailer`.
A bare name is why `resolve` (the former `is_runnable`) searches `PATH` at load.

**The `Exec` program is stored as the absolute path validation found (issue #171).** `resolve`
answers the path it accepted: a program written with a `/` as written, a bare name as the first
absolute `PATH` entry whose file is a regular file with an execute bit. `Spec.exec[0]` holds that path, so
nothing after load searches for a name, and the argv is still a direct exec. A thumbnailer outside
`/usr` therefore has to be self-contained: the jail holds `/usr`, `/etc` and the one program file
(see "Thumbnail sandbox"), so a program that needs a library or an asset outside `/usr` does not
run, nor does a script whose interpreter line names a program outside `/usr`; `#!/usr/bin/env sh`
and `#!/bin/sh` resolve. `worker_shape` matches the file name, so it accepts the same specs as
before.

`argv`, in `backend/thumbargv.rs`, substitutes four placeholders: `%i` the input path, `%u` its
`file://` URI, `%o` the output path and `%s` the pixel size. A code expands anywhere inside an
argument, so F3D's `--output=%o --resolution=%s,%s` works; `%%` is one `%` and an unknown code
stays as written. The program token is never expanded. The input is put through `canonicalize` first, so a
relative path or one carrying `..` reaches the child absolute and cannot be read as a flag.
It returns `Option<(PathBuf, Vec<String>)>` and answers `None` when that call fails, which is
the file being deleted between the listing and the thumbnail, a permission denied on an
intervening directory, or a symlink loop. Refusing is the whole point: passing the raw path
through on failure would hand the child a non-absolute string in exactly the case the guarantee
exists for. A caller turns `None` into a failed thumbnail, never into a run.

**One call produces the bind, `%i` and `%u`, and that is the whole point of the signature.** The
sandbox binds one input read-only by the path its caller gives it, and the child opens whatever
`%i` or `%u` names, so all three have to be the same path; see "Thumbnail cache" for why the
cache key deliberately is not. This has been got wrong twice, in opposite directions, and both
were silent.

First the bind and `%i` diverged: `run_one` passed the raw `job.path` to `sandbox::wrap` while
`argv` canonicalised it for `%i`, so a file reached through a symlink or a symlinked parent was
bound under one name and opened under another. Inside the shipped wrapper that is
`ffmpegthumbnailer -i .../real/clip.mp4` against a bind of `.../link/clip.mp4`, which exits 255
with `Could not open input file`, and it wrote a permanent `fail/` marker each time.

Fixing only that inverted the defect onto the larger half. `%u` was still built by the caller
from the raw path while the bind had become canonical, so **54 of the 172 types the loaded table
serves** stopped thumbnailing under a symlink, while the 118 `%i` types, video and audio through
`ffmpegthumbnailer`, office through `gsf-office-thumbnailer` and three note formats through
`xournalpp-thumbnailer`, started working.

**Count the table, not the declaration lines.** The five `%u` files carry 57 `MimeType=` entries
between them, which is the number a `grep` of the field gives and the number this paragraph
wrongly quoted twice. The table holds 54, because `image/tiff` is declared by both
`glycin-image-rs` and `evince` and because two of evince's comic-book pairs canonicalise
together, `application/x-cbz` onto `application/vnd.comicbook+zip` and `application/x-cbr` onto
`application/vnd.comicbook-rar`. **57 is the raw declaration count and 54 is what the table
serves**, and every claim below is about the table. Re-derive it by loading `Thumbnailers::load`
and counting `by_mime` by `exec[0]` rather than by counting declarations; the fix report carries
the command.

The 54 split **22 glycin and 32 evince**, and the two halves failed differently. The 22 glycin
types failed silently: `glycin-thumbnailer --input file://<raw>` inside the real sandbox prints
`Error opening file ...: No such file or directory` and **exits 0** with a zero-byte output, so
the child read as a success and the job only died later when `stamp` refused the empty PNG. The
32 evince types failed permanently: `evince-thumbnailer` takes the ordinary non-zero exit,
measured at **254** on the same shape, and each one wrote a `fail/` marker, the same damage the
first direction did to video. So the regression lost 22 types silently and 32 permanently, and
either way the row never got a thumbnail and burned a full sandbox spawn on every viewport pass.
The zero-byte case is now recorded as a failure in its own right; see "Thumbnail pool".

`argv` therefore takes NO URI argument. It canonicalises once and derives the returned path,
`%i` and `%u` from that one value, which makes the divergence unrepresentable rather than
merely fixed. `tests/thumbs.sh` now carries four symlink checks, a video and an image through a
symlinked parent directory and a video and an image through a symlink to the file itself,
because the single video check it had before is exactly why the second defect shipped.

Matching is alias-aware in both directions: the declared type and the queried type both pass
through `Aliases::canonical` before they are compared, because the two pairs that matter fall
on opposite sides of the declarations, see "MIME aliases".

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

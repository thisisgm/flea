## Fixtures

There are four, they measure different halves of the product, and **none may live on
tmpfs**. `drop_caches` cannot evict tmpfs pages, so a cold number taken there is a
fabrication. `/tmp` on this box is tmpfs, which is why all three default to `/home/flea-sandbox`, on the same btrfs filesystem as home but outside it, and
all three scripts refuse to build anywhere `findmnt` calls tmpfs or cannot name at all.

| Fixture | Built by | Holds | Measures |
|---|---|---|---|
| `/home/flea-sandbox/flea-bench-btrfs` | `tools/flea-bench`, warm run | 100,000 empty `.txt` files | listing: readdir, sort, window stat |
| `/home/flea-sandbox/flea-media-btrfs` | `tools/flea-media-fixture` | 2,000 real media files, 4.7 GB | thumbnail generation and icon resolution |
| `/home/flea-sandbox/flea-matched-btrfs` | `tools/flea-matched-fixture` | the media fixture minus heic and mkv, 1,700 files, 4.0 GB | the same comparison with every entrant at equal capability |
| a `mktemp -d` of its own | `tools/flea-gvfs-fixture` | a read-only loopback DAV endpoint (`tools/flea-gvfs-dav.py`) mounted through `gvfsd-dav` | the NETWORK rail and anything needing a real FUSE mount, at zero privilege |

The listing fixture contains no media at all, so it cannot exercise a thumbnailer. The
media fixture is small enough that its listing time says nothing. Report both.

The GVFS fixture is the odd one out: it measures nothing and exists so a mount can be driven
without a share, a server or a password. It carries the wedge-stub shapes for `gio` and `lsblk`,
because both rails froze on an unbounded listing and the suite missed it: `case_sharebrowser`
stubs `gio mount -l` to print nothing, and **a stub that cannot hang cannot catch a hang**.

**The third one exists because an entrant that declines a format finishes sooner for it.** heic is
dropped because dolphin renders none and mkv because nautilus and pcmanfm render none, which is
measurement and not guesswork; the 200 txt stay, because every entrant renders zero of them. It is
`cp --reflink=auto` of the media fixture's own files, one to one, so it spans 1,700 distinct
physical copies and a cold read touches each once. Neither it nor the media fixture may be published
alone: the full one is what a user gets, the matched one removes the discount a declined format buys.
**It equalises capability and not work**, because each entrant still chooses how much of the directory
to sweep, so a ratio taken from it carries the same work-rate caveat the media table does.

- The media mix is per 20 files: 6 JPEG, 2 PNG, 2 WebP, 2 HEIC, 4 MP4, 1 MKV, 1 WebM and
  2 plain text. At the default `COUNT=2000` that is 600/200/200/200/400/100/100/200. The
  text files are the long tail, so icon resolution is exercised beside the thumbnails.
- Overrides are `DIR` and `COUNT`. `COUNT` must be at least 20 or the mix drops a format.
- **A media run drops the thumbnail cache as well as the page cache.** A warm
  `~/.cache/thumbnails` makes a generation benchmark measure nothing at all:

  ```bash
  rm -rf ~/.cache/thumbnails
  sudo sh -c 'sync; echo 3 > /proc/sys/vm/drop_caches'
  ```

- Every file is byte-identical on a rebuild. `magick plasma:fractal` is random by default,
  so the seed frame passes `-seed 7`, and `-strip` drops the timestamp chunk that made the
  PNG bytes differ while the pixels matched. `ffmpeg` takes `-fflags +bitexact` so the
  Matroska muxer stops writing a date and a random segment id, and the VP9 seed takes
  `-threads 1`, because row threading makes the encode differ run to run. `-fflags` is a
  per-file option, so it goes before the OUTPUT: on the input it parses, changes nothing,
  and leaves the MKV and WebM seeds differing on every rebuild.
- One seed per format, then plain copies: the build runs seven encoders, not two thousand.
  Per-file `heif-enc` alone would cost 1.6 s times 200. WebM is the one seed a remux cannot
  make, since no WebM container accepts the H.264 stream, so it is a real VP9 encode.
- The copies pass `--reflink=never`. On btrfs a plain `cp` shares extents, and a cold read
  of 400 reflinked clips would hit one physical copy 400 times.
- A build took 84 s, 208 s and 327 s across three runs on this box, and CPU time was 39 s
  in all three, so the spread is time waited rather than time computed and it is not the
  encoders; no mechanism beyond that was established. Deleting a previous fixture is not
  the cause: `rm -rf` of the whole 4.7 GB takes 0.5 s.
- The fixture directory carries a `.flea-media-fixture` marker, and the script refuses to
  `rm -rf` any existing `DIR` that does not have one. That is the only thing standing
  between a mistyped `DIR` and the deletion, so do not remove the marker by hand. It is a
  dotfile, so the fixture lists 2,000 visible entries and 2,001 in total.

### MKV has no thumbnailer until Plan 4 resolves aliases

`mime::Db::lookup` reads `/usr/share/mime/globs2` and returns the canonical name.
`xdg-mime query filetype` on this box does not read that database at all: with `DE=generic`
it runs `file --brief --dereference --mime-type`, so it sniffs content with libmagic's own
name table. `gio`, which does read the XDG database, agrees with `Db::lookup` on all eight
fixture types. Two of them are alias pairs, and they fall on opposite sides:

| Extension | `Db::lookup` and `gio` | `file` and `xdg-mime` | Thumbnailer declares |
|---|---|---|---|
| `.heic` | `image/heif` | `image/heic` | `image/heif`, the canonical name |
| `.mkv` | `video/matroska` | `video/x-matroska` | `video/x-matroska`, the alias |

`/usr/share/mime/aliases` carries both pairs, `image/heic image/heif` and
`video/x-matroska video/matroska`, alias first and canonical second. So no single spelling
matches every thumbnailer on this box, and Plan 4's matcher has to compare the alias
equivalence class rather than the string. Until it does, Plan 5 shows an icon rather than a
thumbnail for MKV. Nothing about this is a defect in `mime.rs`: alias resolution belongs to
the code that matches thumbnailers, which Plan 4 owns.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

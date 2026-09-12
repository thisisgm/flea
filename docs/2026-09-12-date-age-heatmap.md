# A date column that shows its age

Proposal. Written 2026-09-12 against `main` @ `c6a0149` (the v0.2.1 tag). Every code
path below was read, and every count came from the repo's own files, not from memory.
Nothing here is implemented.

## Outcome in one sentence

Flea already draws the fact a heatmap needs — every row carries `m`, the mtime, and
`ui/Row.qml`'s one date cell already formats it — so **this is not a new surface**; it
is an added rendering of a cell that exists, which the columns algebra and the file
budget both push back on.

## What was asked for

One Commander's File Age column, which tints each cell by recency: red at the top for
something touched seconds ago, through orange and yellow for hours, green for days, to
cyan for weeks and months. The age itself is abbreviated — `4'`, `1 h 19'`, `20 h`,
`5 d`, `26 d` — and the screenshot shows it beside Date Modified, which is the
redundancy its own users complain about: *"How do I hide the File Age column? There is
already the date column so I don't need file age."*

## The grounding, before any design judgment

| Claim | What the code says |
|---|---|
| The mtime is already on every row | `docs/protocol.md`: the row carries `m`, and "`mtime` names the same stat field that `rows` carries as `m`" |
| **No protocol or backend change** | `ui/Row.qml:364` already formats `root.row.m`; `null` marks "a row with no real mtime yet" |
| **No new metadata cost** | `docs/protocol.md:155` — only a `size` or `mtime` *sort* pays the stat pass. Drawing already has `m` |
| It is one cell, in one view | `dateText`/`row.m`: `Row.qml` 6 refs, `GridTile.qml` 0, `ColumnRow.qml` 0 |
| The data can already be ordered by it | `ui/js/Sort.js:14` — `ORDERS = ["name","size","mtime","kind"]` |
| **No relative-time formatter exists** | `Format.date` is absolute (`"Today, 13:47"`, `"16 Aug 2026"`); `Format.duration` is media clock (`"1:19"`), not an age |
| The color roles are deliberately few | `Theme.color` exposes `background`, `foreground`, `accent`, `error`, `muted`, `surface`, `symlink`, `executable` — and `muted` is derived, not read |
| A theme palette does carry the ring | Palette.js parses `red yellow green cyan blue magenta orange`, so a ramp needs no new palette key |

## Model A, the age column

Add `age` as a fifth optional column, so `Columns.js`'s `DROP_ORDER` becomes
`["kind", "age", "date", "size", "mode"]`, `Theme.column` gains an `age` width,
`Header.qml` gains a title, `ViewState.hiddenCols` gains a key, and
`src/uischema.rs`'s `OPTIONAL_COLUMNS: [&str; 4]` becomes five — a Rust constant with
its own test. Sorting can stay absent, because `mtime` already orders it.

This is the honest shape of what a screenshot shows. It is also the expensive one.

## Model B, the date column shows its age

No new column. The cell `ui/Row.qml` already draws gains a tint, and *optionally* a
relative form: `26 d` in cyan where it draws `16 Aug 2026` today, `4'` in red where it
draws `Today, 13:47`. One setting decides whether the cell reads as a stamp or an age,
beside the existing `ViewState.addressBar` choice between path and breadcrumb — which
is the same kind of question and already has that shape.

**This model serves everything the ask actually needs.** The red cell is the finding
mechanism; the column header is not. Nothing in the ask requires age and date side by
side — the screenshot shows that only because One Commander's column is redundant, the
way its own users say it is.

## The crux

Both models need the same work: an age formatter, five colors, and a tint on a cell.
They differ in **one** thing — whether that tint lands on a cell that exists or on a
cell that must be introduced. Model A pays the entire columns algebra, the header and
title, the persisted user preference, the Rust constant and its test, and the two
assertions on the exact header string for a second cell that says what the first one
already says. Model B pays none of that.

By the method's own rule — two use cases differing on one parameter are served by one
crux, never by a second surface — this is Model B, and the column is a separate
capability that should earn its own proposal.

## Derived before built

The age is **derived, not stored**: `now - row.m`, computed at draw time from a fact
already on the wire. Nothing is persisted, no backend request is added, and the whole
feature is a pure function plus a color. Reported as a finding: there is no new state
here at all, which is why the cost is plumbing rather than machinery.

## What the simplification pass removes

Against the design one would reach for first — a new column, a new persisted
preference, a new sort key, a stored relative string:

- the fifth column key, in `Columns.js`, `Theme.qml`, `Header.qml`, `uischema.rs` and
  `ViewState`
- the Rust `OPTIONAL_COLUMNS` change and its test
- a new header title, which today is a literal in `ui/Header.qml:151` asserted verbatim by
  `tests/ui.sh:2919`
- a new `DROP_ORDER` entry and its width floor
- a stored age string that would go stale, replaced by a derived one
- **a second cell rendering one fact**, which is the redundancy One Commander's users
  ask to hide

What it adds: one formatter, one ramp function, and a tint on one existing cell.

## The real cost, and where it is not the colors

Three obstacles, in order of how much they actually cost:

1. **`ui/Row.qml` is at `410, recorded 410`.** The budget ledger freezes it: the file may
   not grow a line. A tint plus an optional relative form cannot land there as written,
   so the date cell — or the formatting seam — must come out first. This is the single
   largest piece of work in the change and it is a precondition, not a detail.
   `ui/Ipc.qml` (762/762) and `ui/Theme.qml` (394, over soft) are the neighbours with
   the same problem.
2. **A time-dependent tint has no repaint owner.** `text: root.dateText()` is a plain
   binding, and there is no `repeat: true` timer in `ui/List.qml`, `ui/Pane.qml`,
   `ui/Row.qml` or `ui/shell.qml` — the existing `Timer`s (`coalesce`, `settle`,
   `preferences`) are one-shot thumbnail and listing work, and none is a clock. Rows
   therefore repaint on a listing, a scroll or a selection, never on time passing. A ramp
   in *whole days* barely notices — a cell crossing midnight is one row of one directory,
   and any interaction repaints it. A ramp that reaches *minutes* can sit visibly wrong on
   an idle window, which the screenshot's `4'` and `1 h 19'` do reach. Either the ramp
   stays coarse enough that a stale cell is unremarkable, or one shared low-rate repaint
   tick is introduced — a new mechanism, and therefore a decision.
3. **The ramp itself.** `Theme.color` has no success/warning/info roles and a
   documented habit of *deriving* secondary ink rather than reading palette keys, so a
   ramp that reads six palette keys is a departure. The cheap, consistent version
   interpolates a single hue between `accent` and `muted` by age. The full ring needs
   `Palette.js` to surface more roles and needs a colorblind-safe answer and a fallback
   for every theme that models no palette at all.

## Decision record

Locked by this proposal's own analysis (factual, not chosen):

- The age is derived from `row.m` at draw time; nothing new is stored.
- No backend, protocol, or metadata-pass change is required.
- The formatter is new; `Format.date` and `Format.duration` are not reusable as ages.

Open — these are product decisions, and they are GM's, not this document's:

1. **Model B (tint the date cell) or Model A (a real Age column)?** This proposal argues
   for B and would defer A.
2. **Does the cell read as an age, or keep the stamp and only take the tint?** Keeping
   the stamp is smaller and answers the screenshot; the relative form is what makes the
   cell scannable at a glance.
3. **Should the column be sortable in its own right** if A is chosen, or is `mtime`
   enough? Today `mtime` already orders it.
4. **One interpolated hue, or the full six-color ring?** The ring is what the screenshot
   shows; one hue is what the rest of the chrome does.
5. **The repaint question in obstacle 2**: a coarse ramp accepted as-is, or one shared
   tick introduced.

## Verification plan, if it is built

- `tests/js/format.js` drives the new formatter's boundaries: `4'`, `1 h 19'`, `20 h`,
  `5 d`, `26 d`, and the `m === null` row that must not tint at all.
- A ramp test at both ends and the middle, asserting a color per band.
- `./tests/run-all.sh` for the headless suites, and `tests/ui.sh` for the drawing — which
  **cannot be run here**: it needs `omarchy-drive`, which is not public. That is the same
  gap the open PR #127 reports, and it should be stated rather than discovered.
- Live check against a real directory spanning the bands, read back by screenshot, since
  a color assertion in a unit test does not prove the row drew it.

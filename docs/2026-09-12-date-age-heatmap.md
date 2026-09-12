# A real age column, tinted by recency

Proposal. v-next, written 2026-09-12 against `main` @ `c6a0149` (the v0.2.1 tag).

**Supersedes v1** of this file, which argued for tinting the existing date cell
(Model B). The operator has chosen a real column. Model B is not withdrawn as an
alternative — it is recorded below as the deferred design, because two of its findings
still govern the column's cost.

Every code path and count below was read from the tree, not from memory. Nothing is
implemented.

## Outcome in one sentence

A fifth optional column that draws each row's age, tinted by recency, ordered by the
`mtime` the backend already produces — with the date cell as its **derived default**, so
the column adds a rendering without adding a cell that says something new.

## The design

`age` joins the optional columns, and the columns algebra it must not break is the bulk
of the work. One rule keeps the new column from being the redundant cell its own users
complain about (One Commander's: *"There is already the date column so I don't need file
age"*): **the age column is the age rendering of the date fact, so it defaults on when
`date` is off and off when `date` is on.** A user shows one or the other; neither is
forced. This is not new machinery — `ui/js/Settings.js:318` already serves exactly this
shape as the Address bar choice (`path` | `breadcrumb`, one surface, two renderings),
and `ui/ViewState.qml:119` stores it.

The tint is the feature that was actually asked for. The age text alone is what `date`
already gives in absolute form.

## Blast radius, censused

The skill's rule is census, not sample. Everything below is a real caller, found by grep:

| Surface | What must change | Evidence |
|---|---|---|
| `ui/js/Columns.js` | `DROP_ORDER` gains a 5th entry; `set()` gains a key; `dualSet()` decides it | 64 lines, `DROP_ORDER` at :12 |
| `ui/Theme.qml` | `column.age` width token, from an `ageChars` count | `column` at :84, `dateChars: 16` at :156 |
| `ui/Header.qml` | a fifth `PanelSectionHeader`, its `cell()` case, its `titles()`, its width binding | `headerDate` at :99, `titles()` at :151 |
| `ui/Row.qml` | a fifth cell, its `ageShown`/`ageWidth`/`ageText()` | the `size`/`modified` cells at :249, :265 |
| `ui/ViewState.qml` | `hiddenCols`'s `optional` list; the derived-default rule | `optional` at :42 |
| `ui/js/Menu.js` | the columns toggle list and its glyph | `headerEntries` at :230, list at :235, glyphs at :236 |
| `src/uischema.rs` | **two** constants, not one | `OPTIONAL_COLUMNS: [&str; 4]` at :41, `COLUMN_KEYS` at :67 |
| `src/uistate.rs` | iterates `OPTIONAL_COLUMNS` to rebuild `columns` | at :23 |
| `ui/Ipc.qml` | nothing new — `columnSet`/`headerTitles` read through `Header`, so they follow | :454, :465 |
| `tests/js/columns.js` | 13 of 34 checks pin an exact column string, e.g. `"name,mode,size,date,kind"` | :55, :61, :110 |
| `tests/ui.sh` | `case_header` asserts the title literal verbatim | :2919 |

**Sorting needs no new key.** `ui/js/Sort.js:14`'s `ORDERS` already carries `mtime`, and
`ui/Header.qml:109` already sends `sortRequested("mtime")`. The age column's header click
maps to `mtime` and the existing sort mark moves — nothing in `src/backend/sort.rs` or
`ordering.rs` is touched. This is a finding: the column is a *rendering* of an order that
already exists.

## The three conditions a fifth column breaks, and what they are

The skill requires that where the simpler design fails, the concrete case is shown
clearly. Each of these is a real constraint with a stated resolution, not a hedge:

1. **The floors nest, and a fifth column changes every existing threshold.**
   `Columns.floors()` walks `DROP_ORDER` from narrowest to widest, accumulating a running
   total; `set()` compares the width against each. Inserting `age` moves the floor of
   every column that outlives it. `tests/js/columns.js` pins several of those boundaries
   as exact pixel values (`507`, `373`, `294`, `646`, `647`) — so the new drop position is
   a decision that reddens those tests deliberately, not by accident. **Placement does
   not matter to the tint**: the ramp is per-cell, so whether `age` sits before or after
   `date` changes the arithmetic and not the feature. Recommend `age` drops first (it is
   the rendering with a derived default, so it is the most disposable), which keeps
   `date`'s floor exactly where it is today and confines the churn to the new key.

2. **`dualSet` is a second, hand-written width path.** `Columns.dualSet()` does not use
   `floors()` at all — it computes `base` and adds `size` then `date` explicitly. A fifth
   column is either added there by hand or left out of dual mode. Leaving it out is the
   smaller change and is visible (`dualSet` already returns `mode: false, kind: false`),
   but it is a decision: an age column that exists in one view and not the other.

3. **The header title literal is asserted verbatim.** `ui/Header.qml:151` returns
   `"Name|Mode|Size|Date Modified|Kind"` and `tests/ui.sh:2919` compares the string
   exactly. A fifth title reddens it. That test **cannot be run here** — it needs
   `omarchy-drive`, which is not public — so this is the one change in the set whose
   verification has to happen on the maintainer's box. Stated, not discovered.

## The crux this proposal lands on

Two surfaces would be worse than one, so the age column and the date column must not be
independently toggleable into showing the same fact twice. The crux is a single derived
predicate over the existing store: the age column's default is a function of whether
`date` is shown, and a user who turns both on has made a choice the panel permits and the
feature does not need. That is one rule in `ViewState.hiddenCols`, not a new setting and
not a new stored field.

## Derived before built

The age is **derived, not stored** — `now - row.m`, computed at draw time. `row.m` is
already on every row (`docs/protocol.md`: "`mtime` names the same stat field that `rows`
carries as `m`"), and `docs/protocol.md:155` confirms only a `size`/`mtime` *sort* pays
the stat pass. So:

- no backend, protocol, or metadata-pass change
- no stored age string that would go stale
- no new state anywhere — the column is a pure function of a fact already on the wire

## What is genuinely new work, in cost order

1. **`ui/Row.qml` is at `410, recorded 410`** — the budget ledger forbids it growing a
   line, so a fifth cell cannot land there as written. The date cell must come out first,
   or the new cell must share a component with it. This is a precondition and the largest
   single piece of work in the change. `ui/Ipc.qml` (762/762) and `ui/Theme.qml` (394,
   over soft) are the neighbours with the same problem.
2. **A time-dependent tint has no repaint owner** — verified, not inferred:
   `text: root.dateText()` is a plain binding and there is no `repeat: true` timer in
   `ui/List.qml`, `ui/Pane.qml`, `ui/Row.qml` or `ui/shell.qml`. Rows repaint on a
   listing, a scroll or a selection, never on the clock. A whole-days ramp barely
   notices; the screenshot's `4'` and `1 h 19'` can sit visibly wrong on an idle window.
   Either the ramp stays coarse, or one shared low-rate tick is introduced — the latter
   is a new mechanism and therefore a decision.
3. **The formatter.** `Format.date` is absolute and `Format.duration` is a media clock;
   neither is an age. New function, with its own bands (`4'`, `1 h 19'`, `20 h`, `5 d`,
   `26 d`) and the `m === null` row that must render `--` and tint nothing
   (`ui/Row.qml:360`).
4. **The ramp.** `Theme.color` exposes `background foreground accent error muted`
   `surface symlink executable`, and derives `muted` rather than reading a palette key.
   The cheap, house-consistent version interpolates `accent` → `muted`; the full ring in
   the screenshot needs `Palette.js` to surface `red yellow green cyan blue` and needs a
   colorblind-safe answer plus a fallback for every theme that models no palette.

## Decision record

Locked by the operator this session:

- **Model A: a real age column**, not a tint on the date cell.
- **The column is tinted by recency** — that is the feature, and the age text alone is
  only what `date` already gives.

Locked by this document's own analysis (factual, not chosen):

- The age is derived from `row.m` at draw time; nothing new is stored.
- No backend, protocol, or metadata change. No new sort key — `mtime` already orders it.
- `ui/Row.qml`'s budget is a precondition, not a detail.

Open — these remain product decisions:

1. **Where `age` drops.** Recommended first, to keep `date`'s floor unchanged; this
   reddens pinned boundaries in `tests/js/columns.js` either way.
2. **In dual-pane mode or not.** `dualSet` is a separate width path; either is defensible
   and leaving it out is smaller.
3. **The ramp's extent.** One interpolated hue (`accent` → `muted`), or the full
   palette ring with a colorblind-safe answer and a no-palette fallback.
4. **The repaint question in cost 2**: a coarse ramp accepted as-is, or one shared tick.
5. **The age column's default.** Recommended derived from `date` (on when `date` is off),
   which is what stops the redundancy. A stored, independent default is the alternative.

## Verification plan

- `tests/js/format.js` drives the new formatter's bands and the `null` row.
- `tests/js/columns.js` is **expected to change** — the pinned sets gain the key; the new
  boundaries are asserted deliberately.
- A ramp test at both ends and the middle, one color per band.
- `./tests/run-all.sh` for the headless suites. `tests/ui.sh case_header` — the verbatim
  title assertion — **cannot run here** (no `omarchy-drive`) and must run on the
  maintainer's box. The same gap the open PR #127 reports.
- Live read-back against a directory spanning the bands, by screenshot, because a color
  assertion in a unit test does not prove the row drew it.

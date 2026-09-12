# A real age column, tinted by recency

Proposal. v-next, written 2026-09-12 against `main` @ `c6a0149` (the v0.2.1 tag).

**Supersedes v1** of this file, which argued for tinting the existing date cell
(Model B). The operator has chosen a real column. Model B is not withdrawn as an
alternative — it is recorded below as the deferred design, because two of its findings
still govern the column's cost.

Revised the same day: **the derived date↔age default is withdrawn.** The age column
joins the toggles as an ordinary optional column; whether the panel says the fact twice
is carried by the shipped default set alone, not by a rule coupling two toggles.

Revised again the same day: **the operator locked the four product decisions this file
left open, and overrode one recommendation.** The shipped default is
`name,size,date,age` — `date` stays on — over this file’s `age`-in-place-of-`date`
recommendation; the full palette ring replaces the `accent` → `muted` interpolation;
one shared repaint tick replaces the coarse ramp; and `age` is drawn in dual-pane mode
too. The fifth open item, placement, is settled by this file’s own arithmetic below.

Every code path and count below was read from the tree, not from memory. Nothing is
implemented.

## Outcome in one sentence

A fifth optional column that draws each row’s age, tinted by recency, ordered by the
`mtime` the backend already produces — shipped on by default beside the date cell
(today’s set plus `age`), tinted with the full palette ring, repainted by one shared
tick, and drawn in dual—pane mode too.

## The design

`age` joins the optional columns, and the columns algebra it must not break is the bulk
of the work. It joins as an ordinary member: one key in
`ViewState.hiddenCols`'s `optional` list (`ui/ViewState.qml:42`), one row in
`ui/js/Menu.js`'s toggle list, one entry in `DROP_ORDER`. Nothing about the pair
`date`/`age` is special — each is independently toggleable, as `mode`, `size` and
`kind` already are.

The One Commander complaint (*"There is already the date column so I don’t need file
age"*) is answered by the **shipped default**, not a rule — and the operator’s answer
is to ship both: the default set is today’s plus `age`, so the out-of-box panel does
carry the fact twice, in its two renderings, by choice and not by oversight. A user who
agrees with the complaint turns `date` off; the panel permits both-on because the
toggles are independent, which is the machinery every column already shares. A default
is one choice in `src/uischema.rs`’s `DEFAULTS` and its mirrors; a coupling rule would
be a hidden dependency no other column pair has, and it would not even prevent the
redundancy it aims at — the panel permits both-on either way. An earlier revision of
this file proposed the coupled default, after `Settings.js:318`’s `path` |
`breadcrumb` choice; that precedent is one surface with a radio between two renderings
of itself, not two independent toggles, and the proposal is withdrawn.

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
| `ui/ViewState.qml` | `hiddenCols`'s `optional` list; `defaultColumns` if the shipped set moves | `optional` at :42, `defaultColumns` at :27 |
| `ui/js/Menu.js` | the columns toggle list and its glyph | `headerEntries` at :230, list at :235, glyphs at :236 |
| `src/uischema.rs` | **two** constants and the shipped default | `OPTIONAL_COLUMNS: [&str; 4]` at :41, `COLUMN_KEYS` at :67, `DEFAULTS`'s `columns` at :8 |
| `src/uistate.rs` | iterates `OPTIONAL_COLUMNS` to rebuild `columns` | at :23 |
| `ui/Ipc.qml` | nothing new — `columnSet`/`headerTitles` read through `Header`, so they follow | :454, :465 |
| `tests/js/columns.js` | 13 of 34 checks pin an exact column string, e.g. `"name,mode,size,date,kind"` | :55, :61, :110 |
| `tests/js/menu.js` | pins `headerEntries`' action list verbatim, so a fifth toggle reddens it | `"col:mode,col:size,col:date,col:kind,toggleHidden"` at :106 |
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
   `date` changes the arithmetic and not the feature. Recommend `age` drops first, which
   keeps `date`'s floor exactly where it is today and confines the churn to the new key.

2. **`dualSet` is a second, hand-written width path.** `Columns.dualSet()` does not use
   `floors()` at all — it computes `base` and adds `size` then `date` explicitly. A fifth
   column is either added there by hand or left out of dual mode. **Resolved: added.**
   The operator locked `age` into dual-pane, so `dualSet()` gains its explicit term and
   both layouts draw the same column set; the smaller change — an age column that
   exists in one view and not the other — is recorded as declined.

3. **The header title literal is asserted verbatim.** `ui/Header.qml:151` returns
   `"Name|Mode|Size|Date Modified|Kind"` and `tests/ui.sh:2919` compares the string
   exactly. A fifth title reddens it. That test **cannot be run here** — it needs
   `omarchy-drive`, which is not public — so this is the one change in the set whose
   verification has to happen on the maintainer's box. Stated, not discovered.

## The crux this proposal lands on

The columns algebra is the crux, and it is already written: every optional column is one
key in one stored array, subtracted from what the width affords, and `age` joins it as a
plain member. The default set carries the operator’s choice — both renderings on out of
the box — and a user who wants the fact once turns either cell off. An earlier revision
coupled the two defaults with a derived predicate; it is withdrawn, because no other
column pair behaves that way and the coupling did not prevent the both-on state it was
aimed at.

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
   notices; the screenshot’s `4'` and `1 h 19'` can sit visibly wrong on an idle window.
   **Resolved: one shared low-rate tick.** The operator chose the correct repaint over
   the coarse ramp; the tick is a new mechanism in a tree that has none, it must be one
   timer for the whole panel rather than one per row, and its rate is chosen against the
   formatter’s finest band.
3. **The formatter.** `Format.date` is absolute and `Format.duration` is a media clock;
   neither is an age. New function, with its own bands (`4'`, `1 h 19'`, `20 h`, `5 d`,
   `26 d`) and the `m === null` row that must render `--` and tint nothing
   (`ui/Row.qml:360`).
4. **The ramp: the full palette ring, as locked.** `Theme.color` exposes `background
   foreground accent error muted` `surface symlink executable`, and derives `muted`
   rather than reading a palette key. The ring needs `Palette.js` to surface
   `red yellow green cyan blue` — its parser already collects whatever keys a theme’s
   `colors.toml` sets, but nothing in the tree queries a ring key today — plus a
   colorblind-safe answer and a fallback for every theme that models no palette. The
   cheap, house-consistent `accent` → `muted` interpolation is recorded as declined.

## Decision record

Locked by the operator this session:

- **Model A: a real age column**, not a tint on the date cell.
- **The column is tinted by recency** — that is the feature, and the age text alone is
  only what `date` already gives.
- **Dual-pane shows it** — `dualSet()` gains the term; the smaller change, an age
  column that exists in one view and not the other, is declined.
- **The tint is the full palette ring** — `red yellow green cyan blue` as the screenshot
  drew it, not the `accent` → `muted` interpolation.
- **One shared low-rate tick** owns the repaint, not a coarse ramp left to redraws.
- **The default set is today’s plus `age`** — `name,size,date,age`, with `date` left on,
  over this file’s `name,mode,size,kind,age` recommendation and the carries-the-fact-once
  rationale that came with it.

Locked by this document’s own analysis (factual, not chosen):

- The age is derived from `row.m` at draw time; nothing new is stored.
- No backend, protocol, or metadata change. No new sort key — `mtime` already orders it.
- `ui/Row.qml`’s budget is a precondition, not a detail.
- `age` is an ordinary toggle — the same machinery as `mode`/`size`/`date`/`kind`, no
  coupling with `date`.
- **`age` drops first.** Placement does not matter to the tint, and first keeps
  `date`’s floor exactly where it is today, confining the churn to the new key. The
  pinned boundaries in `tests/js/columns.js` redden deliberately either way.

The overridden recommendations are recorded, not erased: the both-renderings default
supersedes the “carries the fact once” outcome this file first argued for, and the One
Commander complaint is answered by the off switch rather than the default. The default
set is shipped in `DEFAULTS` (`src/uischema.rs:8`) and mirrored in `ui/ViewState.qml:27`,
`ui/js/Settings.js:309` and `:385`, and pinned by the Rust tests (`src/uistore.rs:324`,
`src/uistate.rs:395`, `src/uischema.rs:206`; `tests/uistate.sh:61` samples the QML line) —
appending `age` moves all of them.

## Shipped, 2026-09-12 (same day)

The operator's corrections during the live read-back, recorded in the order they were made:

1. **The ramp is absolute, not theme-read.** The first implementation read the ring from the
   theme's palette keys with a contrast walk and ANSI aliases — the full-palette-ring choice
   above, built as specified — and it was bloat: a second JS file, its own suite, HSL floors,
   all to paint five words. Deleted. `ui/Age.qml` holds six literal colours, and the theme
   affects nothing.
2. **Six rungs, with orange.** The five-colour ring read as missing a step between red and
   yellow. The hour band splits at the minute mark: red is the first minute (the "now" band
   exactly), orange the rest of the hour, yellow the day, green the week, cyan the month, blue
   everything older — so every boundary below the day is a boundary the age text draws too.
3. **Saturation is not negotiable.** The everforest rungs were pastels and read as one grey
   band; the absolute ramp is vivid by construction.

What shipped beside the ramp: `Format.age`/`ageBand` (pure, `tests/js/format.js`), `ui/Age.qml`
(the 30 s shared tick, the widths, the ramp), `ui/RowCells.qml` (the five cells extracted whole
from `ui/Row.qml`, which lands at 346 and leaves the budget ledger's over-cap list entirely),
`age` first in `DROP_ORDER` with every pre-existing floor unchanged, `age` in `dualSet` behind
`date`, `Picker.HIDDEN_COLS` extended so the chooser never draws it, `ColumnAge=55` in the
metrics contract, the `col:age` toggle with the history glyph, and the shipped default set
`name,size,date,age`. Verified: 3114 JS checks, 28 state tests, budget gate green, and the
live read-back on the compositor — the operator's own eyes on the running window, the one
verification a colour assertion cannot substitute for. `tests/ui.sh case_header` and the
metrics gate still need the maintainer's box (`omarchy-drive` is not public; same gap as
PR #127).

## Verification plan

- `tests/js/format.js` drives the new formatter's bands and the `null` row.
- `tests/js/columns.js` is **expected to change** — the pinned sets gain the key; the new
  boundaries are asserted deliberately.
- `tests/js/menu.js` is **expected to change** — `:106` pins `headerEntries`' action
  list verbatim and the list gains `col:age`.
- A ramp test at both ends and the middle, one color per band.
- `./tests/run-all.sh` for the headless suites. `tests/ui.sh case_header` — the verbatim
  title assertion — **cannot run here** (no `omarchy-drive`) and must run on the
  maintainer's box. The same gap the open PR #127 reports.
- Live read-back against a directory spanning the bands, by screenshot, because a color
  assertion in a unit test does not prove the row drew it. The same read-back is the
  tick’s test: a row held idle across a band boundary changes its text and tint with no
  listing, scroll or selection to repaint it.

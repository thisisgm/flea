DONE

FAM-QLBODY: the Quick Look card now mounts each kind's own view, the same views the preview column uses, and asks
for them on slot "look". Worktree: /home/gm/flea-work/fam-qlbody. Nothing is committed. Code gates are green
apart from base failures, failures that come from the brief's `-p` scope, and one flaky test. Native proof against
the boards is still owed: the brief forbids minipc, so no capture was taken. Each board row below is "mounted,
test-proven" and none is "matches the board".

## Boards: final state

"Mounted" means the card draws the view, and a dispatch test proves it through the production Shell and the state
IPC. The board strings are quoted from each board .html and need a native check.

| board | view in the card | final state | board strings to check natively |
|---|---|---|---|
| ql-text | CodeView unnumbered, inset 31 | mounted; `w` wraps (test) | "notes.txt" "4 lines" |
| ql-code | CodeView numbered, gutter ink, code_inks | mounted (test) | "main.rs" "7 lines" |
| ql-code-wrap | CodeView.set_wrap through `w` | `w` restored (test) | "long.rs" "164 lines" |
| ql-at-match | CodeView | mounted; NOT done: no seam opens it at a search match line (see Unconfirmed) | "long.rs" "164 lines" |
| ql-markdown, -badges, -bar, -closefocus, -crlf, -figures, -html, -nesting, -tables, -source | MarkdownView column=false; bar words Rendered/Source; r; close focus | mounted (test); the variants are not driven one by one | "Rendered" "Source" "Remote image not loaded · cdn.example.com" |
| ql-html, ql-html-source | HtmlView; r, w, zoom | mounted (test) | "61 lines" "Rendered" "Source" |
| ql-office-doc, -slides, -converting, -locked | OfficeView look=true; PDF strip; the lock failure recipe with Open With | mounted (test, docx); -locked and -converting not driven | "1 / 3" "This document is password protected." |
| ql-office-sheet | OfficeView sheets; the sheet words step through the sheets | same mount path; xlsx not driven | "13 rows" "Monthly" "Savings" |
| ql-pdf, ql-pdf3 | PdfView single page; pager, counter, zoom, expand and close strip | mounted (test); column `e` opens the card at the column's page (no test) | "1 / 1" "1 / 3" |
| ql-error | the view's fail_sentence under the bar, with Open With | wired (doc_card.rs:108); not driven | "This file could not be read." "Open With…" |
| ql-video, ql-audio | MediaPlayer with pv3 surface::QuickLook; autoplay; p, m, Left/Right seek; Space closes | mounted, autoplay start (test) | "0:02 / 0:02", "Ogg audio · 0:02 · 44.1 kHz · 6.9 kB" |
| ql-stl, ql-stl-orbit, ql-3mf, ql-gcode | Model3dView Surface::QuickLook; plate words; orbit and zoom keys through act() | mounted, stl and gcode (test) | "60.0 × 40.0 × 40.0 mm" "Plate 1" "Layer 128 of 240 · 25.60 mm" |
| ql-font | FontView; its first fontlook is asked at mount | mounted (test) | "Adwaita Sans · Regular · 879.8 kB · 2,937 glyphs" |
| ql-archive | ArchiveView with its own name and count head; the names come with the meta ask (archive:true) | mounted, meta ask sent (test) | "1 entry · 156 B unpacked" |
| ql-image, ql-raw, ql-symlink | the existing pv1 bodies | unchanged | |
| ql-unsupported | CannotShow | unchanged; this is the negative control (test) | "This file cannot be previewed." |

## Files changed (new unless marked M)

- crates/flea-gui/src/g2/pv1/doc.rs:29 `View` (code, markdown, table, html, archive, office, font, pdf, media,
  model); :147 kind; :171 chrome; :236 failure; :274 ready; :308 shape (a page kind sizes to its first page).
- crates/flea-gui/src/g2/pv1/doc_keys.rs:82 action (wrap, view, zoom, sheet, plate, media keys); :169 word; :214
  press; :222 settle (opens at the column's page); :236 close (media stops, every view stops asking).
- crates/flea-gui/src/g2/pv1/chrome.rs:62 bar: the mark, the name and its tail, the pager and counter, the words,
  the tools and close.
- crates/flea-gui/src/g2/pv1/doc_card.rs:51 has_doc; :58 doc_action (Space and Escape close, the cursor keys
  follow); :93 doc_changed; :108 doc_parts.
- crates/flea-gui/src/g2/pv1/act.rs and svg.rs: activate, strip and action, and the SVG raster methods, moved out
  of mod.rs. The moved code is unchanged except for `pub(super)` and the Doc arms. mod.rs was 565 lines on base and
  is 320 now.
- M crates/flea-gui/src/g2/pv1/mod.rs: Hit::Word, Hit::Strip, doc_watch, opening and closing a doc.
- M crates/flea-gui/src/g2/pv1/body.rs:54: Body::Doc.
- M crates/flea-gui/src/g2/pv1/card.rs:58: a mounted view lays out unseen while the card is held back, so its first
  ask goes out before the reveal.
- M crates/flea-gui/src/g2/pv1/bar.rs: ring_slot and the strip ids.
- M crates/flea-gui/src/g2/pv1/glyph.rs:44: Glyph::named, the chevrons and the lock.
- M crates/flea-gui/src/palette.rs:25: Theme.code_inks and gutter, the exact gm/0400-m9-s-previewtypes hunk.
  `git diff gm/0400-m9-s-previewtypes -- crates/flea-gui/src/palette.rs` prints nothing.
- crates/flea-gui/src/window/quick_look_wire.rs:20 Wire, the outbox, with a wake channel signalled on every send;
  reply parsers :171 code_reply and :211 font_reply.
- crates/flea-gui/src/window/quick_look_docs.rs:114 Mounting::mount, which builds each view at its board's sizes.
- crates/flea-gui/src/window/quick_look_mount.rs:25 Mount; :41 Mount::of; :105 show_doc (:139 takes the PDF page
  from the column); :182 quick_look_drain; :193 quick_look_doc_line; :242 on_line; :301 on_meta.
- M crates/flea-gui/src/window/quick_look.rs: :171 show_row mounts a view first; :91 the drain task; Ask::Code
  paging removed.
- M crates/flea-gui/src/window/quick_look_keys.rs: :110 a tab switch closes the card; :145 drains after each
  action; :163 quick_look_tab_switch (QuickLook::close).
- M crates/flea-gui/src/window/quick_look_replies.rs:25: routes the mounted view's lines first; the old Code arm
  is removed.
- M crates/flea-gui/src/window/quick_look_readers.rs: :36 previewKind reads doc.kind(); :62 previewState reads the
  view's failure sentence. M events_ipc.rs fills that sentence in. M events.rs:54 passes the window.
- M crates/flea-gui/src/window/preview_input.rs: :82 `expand` on the column's PDF opens Quick Look there; :107
  column_pdf.
- crates/flea-gui/src/window/dispatch/quick_look_body_tests.rs; M preview_tests.rs; M dispatch/mod.rs.

## Tests and negative controls

All tests in window/dispatch/quick_look_body_tests.rs run through the production Shell.

- :167 `kinds!` makes one test per kind: notes.txt, main.rs, readme.md, page.html, table.csv, report.docx,
  book.pdf, Inter-Bold.ttf, pack.zip, benchy.stl, benchy.gcode, song.ogg, clip.mp4. Each test checks four things:
  - the card mounts Body::Doc of that kind, not CannotShow;
  - previewKind matches the kind;
  - the view's first ask goes out on slot "look";
  - the bar or the media surface draws.

  Each test then closes the card with Escape.
- :184 is the negative control. disk.img mounts no view and shows CannotShow, "This file cannot be previewed.". It
  sends no code ask and starts no media.
- :201 checks that `w` flips CodeView.wrap and the card stays open. ctrl-pagedown (tabNext) then closes the card,
  and the keyboard goes back to Listing.
- :232 checks media. Autoplay starts the scripted decoder once, in the Media context. p, m, Right and Left keep the
  card open, and Space closes it.
- :259 checks that the cursor keys still move under a mounted view, and that the card follows onto main.rs.
- In preview_tests.rs, notes.txt now mounts the unnumbered CodeView on slot "look". Space, Escape, follow and
  unsupported behave as before.
- Targeted run, unit m9-qlb-test4 (`-- dispatch:: g2::pv1`, .superpowers/test4.log): `test result: ok. 33 passed;
  0 failed`.

## Gates (last lines)

- check, unit m9-qlb-check, .superpowers/check.log: `Finished dev profile [unoptimized + debuginfo] target(s) in
  1m 23s`. Its one warning, `field last_cells is never read` (grid/tile.rs:29), was already on base.
- test, unit m9-qlb-test (`-p flea-gui --no-fail-fast`), .superpowers/test.log: `error: 4 targets failed:`
  (`-p flea-gui --lib`, `--test g1_02`, `--test g1_07`, `--test uish_driver`). 10 tests failed. Compared with
  /home/gm/flea-work/ctl-gates/joins-1024/base-failures.txt, 4 are not on that list:
  - ipc::server::tests::headless_cli_and_harness_contract fails with "gui-ipc: build flea and the flea-gui
    ipc-fixture example first". uish_driver_real_window and uish_driver_rejects_stale_or_foreign fail with
    "UISH_CONSTANT_SURFACE_REJECTED rowCentre status=3". All three drive the built `flea` binary and the ipc
    fixture, which `-p flea-gui` does not build. All three pass in the workspace run below.
  - g2::pk1::tests::real_stalled_children_are_reaped_after_a_cancel fails with "the picker did not settle". It
    passed in the workspace run, and passed alone in unit m9-qlb-test5 (.superpowers/test5.log: `1 passed`). It
    is a real-process timing test in the file picker, untouched here, and is flaky under load.
- workspace test, unit m9-qlb-testws (`--workspace --no-fail-fast`, the form joins-1024/gates.sh uses),
  .superpowers/test-ws.log: `error: 4 targets failed`. All 7 failing tests are in base-failures.txt; 0 are new.
  This run came before a formatting-only pass and the has_doc move; check and the targeted tests reran after both.
- tools/flea-file-budget, .superpowers/budget.log: rc=1. 9 files are OVER CAP, all of them on base; base had 10,
  and g2/pv1/mod.rs left the list. This unit adds no over-cap file. card.rs is 398 lines.
- `git diff --check`: clean (exit 0).

## Native rows for the controller

- b4-quicklook rows this change affects:
  - code: quicklook.code-*
  - text: text-txt, text-log, text-empty, text-large, text-utf16, text-hidden, text-csv, text-tsv, text-md,
    text-markdown
  - pdf and office: pdf-pdf, office-docx, office-xlsx, office-odt, office-odp
  - models: model-stl, model-obj, model-3mf
  - media: video-mp4, video-mkv, video-webm, video-mov; audio-mp3, audio-mp3-cover, audio-ogg, audio-opus,
    audio-flac, audio-wav, audio-m4a
  - refusal: unsupported-blob, unsupported-dump, unsupported-desktop
- b4-quicklook rows that should not change: image-* and raw-dng.
- b10-previews rows this change affects: parity.ql-text, ql-code, ql-pdf, ql-video, ql-audio, ql-archive, ql-font,
  ql-expanded. Rows that should not change: ql-image and ql-symlink.
- Rows to watch:
  - text-csv and text-tsv now draw the table view, while sweep/eyes-ctl2/scene-board.tsv maps them to ql-text.png.
  - text-markdown (.markdown) now draws the Markdown view, while the scene board maps it to ql-text.
  - model-gltf, model-glb and model-mtl still show the refusal. The backend parses only stl, obj, ply, amf, 3mf and
    G-code (src/backend/model3d/mod.rs:68), and the column's list (selection_preview.rs:74) leaves them out too.

## Unconfirmed, with where I looked

- No native capture was taken, so fidelity to each board (sizes, insets, the 27 px bar, colours) is unproven. The
  recipe comes from the g2/pv1/look.rs constants: quick_look_docs.rs uses BAR 27, PAD 14, TEXT_LEFT 31,
  TEXT_LINE 20, BODY 14 and CAPTION 12.
- ql-at-match: no host seam gives Quick Look a search match line. I checked window/quick_look*.rs and the search
  family. The card opens at line 1.
- Media facts: kind_name ("Ogg audio") and size_text are empty, because RowFacts carries no kind name. The facts
  line falls back to the suffix. The rate and duration come from the meta answer (quick_look_mount.rs:301).
- The media test proves that the decoder starts and that p, m and the arrows keep the card open. It does not
  assert which player commands those keys produce.
- Column `expand` on a PDF has no test. The card reads surface.page from the column's PdfView and turns its own
  PDF by that many pages once the pages are known (doc_keys.rs:222).
- The Markdown variants, office -locked, -converting and -sheet, and ql-error are wired but not driven one by one.
- built-check.py exits 128 (`git log --no-walk --stdin` in /home/gm/Documents/claude/flea-release), so I read
  BUILT.tsv by hand (see PLAN.md).
- To fold after the previewtypes join: quick_look_wire.rs repeats the shape of gm/0400-m9-s-previewtypes
  selection_preview_wire.rs, and Look in quick_look_docs.rs repeats that branch's selection_preview_docs.rs recipe.
  After both land, they should share one wire module.
- Body::Text has no producer left, since plain text now mounts CodeView. It stays because readers and tests name
  it. A follow-up could drop it.
- Formatting: default rustfmt touched only the new files. Each modified file keeps base's own wider formatting
  outside my hunks, rebuilt by applying my changes onto the unformatted base (.superpowers/unfmt/).

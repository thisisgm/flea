# m9-fam-qlbody PLAN

Base ce153f093 (train 66dd10254 + FAM-preview). Write only in /home/gm/flea-work/fam-qlbody. No commits.

## BUILT CHECK

- `tracker/built-check.py` fails here: `git log --no-walk --stdin` exits 128 in
  /home/gm/Documents/claude/flea-release (a SHA in BUILT.tsv the repo does not hold). Read BUILT.tsv by hand.
- BUILT.tsv rows for gm/0400-g2-pv1..pv4 and their snapshots: EVOLVED (on the train). One PARTIAL: snapshot
  t3-2eac44d2 (pv2) holds `crates/flea-gui/tests/g2_pv2` 1 file and `src/ui_logic_pv2` 1 file not on the train.
  Reported, not rebuilt.
- REPORT TO CONTROLLER: the column host the brief names, `window/selection_preview_docs.rs:104` (`Doc::mount` for
  CodeView, MarkdownView, CsvView, HtmlView, ArchiveView, OfficeView), is NOT on this base. It lives on
  `gm/0400-m9-s-previewtypes` / `t3/m9-fam-previewtypes` (91d41946e + c8b634e77, landing per STATE 19:49). Its
  `selection_preview_wire.rs` (Wire impls and code/markdown/html reply parsers) is generic. This unit does not
  rebuild the column host. The card needs the same wire, so `window/quick_look_wire.rs` carries the card's own copy
  of those host impls and parsers (slot "look"). After both join, fold the two wires into one module.
- Views on the train, reused as is: pv2 CodeView, MarkdownView, CsvView, HtmlView, ArchiveView, OfficeView,
  FontView, PdfView (single page, `continuous=false`); pv3 MediaPlayer + `pv3::surface::QuickLook`; pv4
  Model3dView (`Surface::QuickLook`). Each already exposes its Quick Look bar data (`bar()`), keys (`action`/`act`)
  and refusal sentence.

## Approach (could this be simpler? yes: no new views, no new layout code per kind)

pv1's `Body` gains `Doc(Doc)`: an enum of the mounted view entities. The card draws one generic chrome bar from each
view's bar data (mark, name, count, Rendered/Source or sheet or plate words, the PDF strip, close) and the view's
own element under it. QuickLook routes keys and bar clicks to the view itself. The views' backend asks go into one
outbox the Shell drains whenever the card notifies; replies route by the view's token and slot "look".

## Boards

board -> view (file:line) -> mounted in card today -> what this unit does. Strings quoted from each board .html.

- ql-text -> pv1 Body::Text (g2/pv1/card.rs:156) -> yes -> CodeView unnumbered at TEXT_LEFT, so `w` wraps it.
  "notes.txt" "4 lines".
- ql-code -> pv2 CodeView (g2/pv2/code/mod.rs:99) -> no (plain Body::Text) -> mount CodeView numbered, styled.
  "main.rs" "7 lines".
- ql-code-wrap -> CodeView.set_wrap (code/mod.rs:187) -> no -> `w` toggleWrap restored. "long.rs" "164 lines".
- ql-at-match -> CodeView scroll to the match line -> no -> mount; open at a search match line when the host names one
  (unconfirmed seam, see RESULT). "long.rs" "164 lines".
- ql-markdown, -badges, -bar, -closefocus, -crlf, -figures, -html, -nesting, -tables -> pv2 MarkdownView
  (markdown/mod.rs:214) -> no -> mount `column=false`, bar "Rendered" "Source", r flips. "guide.md" "25 lines"
  "Remote image not loaded · cdn.example.com".
- ql-markdown-source -> MarkdownView.set_view(Source) -> no -> r / click Source.
- ql-html -> pv2 HtmlView (html/mod.rs:77) -> no -> mount; bar "61 lines" "Rendered" "Source".
- ql-html-source -> HtmlView.set_view -> no -> r / click Source.
- ql-office-doc, ql-office-slides -> pv2 OfficeView look=true (office/mod.rs:60) -> no -> mount; PDF strip "1 / 3".
- ql-office-sheet -> OfficeView sheet table -> no -> mount; bar "13 rows" "Monthly" "Savings".
- ql-office-converting -> OfficeView idle strip -> no -> mount; "talk.odp".
- ql-office-locked -> OfficeView.failure -> no -> recipe "This document is password protected." "Open With…".
- ql-pdf, ql-pdf3 -> pv2 PdfView single page (view.rs:100) -> no (cannot preview) -> mount; strip "1 / 1", "1 / 3",
  chevrons, minus, plus, maximize, close; expand-at-page from the column.
- ql-error -> PdfView.fail_sentence -> partly (FAM-preview recipe) -> failed read keeps bar: "This file could not be
  read." "Open With…".
- ql-video -> pv3 MediaPlayer + surface::QuickLook (pv3/surface.rs:92) -> no (row DEAD) -> mount, autoplay, p, m,
  seek on Left/Right/h. "0:02 / 0:02".
- ql-audio -> same -> no (row DEAD) -> mount; "tone.ogg" "Ogg audio · 0:02 · 44.1 kHz · 6.9 kB".
- ql-stl, ql-stl-orbit, ql-3mf, ql-gcode -> pv4 Model3dView Surface::QuickLook (pv4/view.rs:67) -> no -> mount; bar
  fact "60.0 × 40.0 × 40.0 mm", plates "Plate 1" "Plate 2", "Layer 128 of 240 · 25.60 mm".
- ql-font -> pv2 FontView (font/mod.rs:70) -> no -> mount; "Adwaita Sans · Regular · 879.8 kB · 2,937 glyphs".
- ql-archive -> pv2 ArchiveView (archive/mod.rs:71) -> no -> mount `column=false`; "1 entry · 156 B unpacked".
- ql-image -> pv1 Body::Image -> yes -> unchanged.
- ql-raw -> pv1 Body::Raw -> yes -> unchanged. "Embedded preview · 1620 × 1080".
- ql-symlink -> pv1 Body::Link -> yes -> unchanged. "→ photo.png".
- ql-unsupported -> pv1 CannotShow -> yes -> unchanged (negative control). "This file cannot be previewed."

Also: `w` toggleWrap reaches the mounted CodeView/HtmlView source; the card closes on a tab switch.

## Files

- g2/pv1/doc.rs (new): `Doc` enum, its chrome, keys, readiness, failure, close.
- g2/pv1/chrome.rs (new): the generic bar drawn from a `Chrome`.
- g2/pv1/body.rs, card.rs, mod.rs, glyph.rs: the Doc arm, hits for words and strip, chevron glyphs.
- window/quick_look_docs.rs (new): mounts each kind with theme roles; routes backend lines.
- window/quick_look_wire.rs (new): the card's wire and reply parsers.
- window/quick_look.rs, quick_look_replies.rs, quick_look_readers.rs, quick_look_keys.rs: routing, drain, close.
- window/dispatch/quick_look_body_tests.rs (new): one mount test per kind plus the cannot-preview negative control.

## Gates

check (m9-qlb-check), test -p flea-gui (m9-qlb-test), tools/flea-file-budget, git diff --check.

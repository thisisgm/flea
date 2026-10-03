# Unreleased

- A `list` scans on its own thread, so a slow MTP or network mount no longer freezes transfer progress while the existing reader-thread Cancel stops the copy (#144, @mfilm77).

# Flea 0.3.7

- Tight row density fits more rows, while Compact stays the default (#154, @muellan).
- Grid gains Huge and Largest thumbnail sizes, and grid names follow Omarchy text size (#147, @calebhat; #81, @aholbreich).
- Copied and cut rows are marked in the listing (#169, @MISTERNEGATIVE21).
- Today's dates can be tinted, off by default (#132, @PowerSerg10).
- Hidden files can sort last, off by default, and each folder remembers its sort (#70, @TyRichards; #179, @zm14).
- Columns view counts its columns from the window width (#167, @wanghailei; #69, @TyRichards).
- Large archive extracts and compression jobs no longer stop at the 30 s CPU cap (#211, @Auxxed; #218, @itsmunzir).
- Flea starts on OpenGL when all Vulkan GPUs need Mesa's hasvk driver, with a diagnostic (#160, @davidhbigelow).
- Folder sizes walk only where a size is drawn (#66, @jesedv).
- dd trashes the focused row after clicking another row and moving with the keyboard (#222, @mubshrx).
- Columns at / no longer duplicate the root, and Left stays at / (#221, @mubshrx).
- Picker filter chips take room before the path, so the path cannot squeeze them out (#224, @mubshrx).
- A double click on a file in the picker accepts it like Enter (requested on X).
- Arrow keys regain listing focus after using the sidebar (requested on X).
- Drives with no filesystem no longer leave an unusable sidebar row (requested on X).
- A second Space closes PDF Quick Look, and clicking the folder already shown no longer reloads it (requested on X).
- Columns view builds ancestor panes only when they are shown (requested on X).
- Neighbour columns open their background menu, and Locked tiles explain why no menu can open.
- New Folder works from a side column, and popup keys and provider navigation work again.
- Grid captions stay inside the tile, and hidden Grid and List views release viewport work and restore the cursor.
- Inline rename errors retain the visible draft and caret through Grid reflow; scrolling restores listing focus without abandoning a pending write.
- Context menus reserve no scrollbar gap, and row highlights reach the edge of the listing.
- Scroll lanes stay invisible until used, and the footer keeps its complete dismissal hint.
- Counts, fonts and layouts stay consistent across listings, dialogs and menus.
- Startup removes Flea's dead Quickshell entries and stale links, reducing registry work after repeated launches.
- Moves confirm once per batch, and USB, network and phone copies are still confirmed on the drive; final 1000-file USB synced move: 9.62 s.
- This build, AUR 0.3.7-2, draws the copy and cut mark after the file name in List view, where the first 0.3.7 build clipped it at the row's left edge.

Known issues

- The 5 GB memory growth and crash in #151 were not reproduced in a one-hour soak; the report remains open.
- The NFS problem reported on 0.3.5 remains unexplained.
- Mac Return and keypad Enter support in the picker is deferred to 0.3.8 (#225).
- In List view, the GUI process uses 529-788 KiB more PSS, 512-606 KiB more anonymous memory and 526-774 KiB more private memory than 0.3.6 at 900 px; at 2540 px, anonymous memory is 496-540 KiB higher.
- USB folders rank third for GUI PSS and fifth for anonymous memory among seven managers; memory optimization is deferred to 0.3.8.
- The NAS backend uses about 120 KiB more file-backed executable PSS than 0.3.6, with no measured increase in anonymous memory; investigation continues in 0.3.8.
- The existing automatic-hide sidebar focus problem and a 2-3 ms cached image-folder revisit cost remain for 0.3.8.
- The driven navigation workload uses 52-84 ms more CPU on minipc and 29-37 ms more on the VPS than 0.3.6; backend PSS is 61-103 KiB higher after that workload. Investigation is deferred to 0.3.8.

Thanks to @muellan, @calebhat, @aholbreich, @MISTERNEGATIVE21, @PowerSerg10, @TyRichards, @zm14, @wanghailei, @Auxxed, @itsmunzir, @davidhbigelow, @jesedv and @mubshrx.

Source revision: ae9419c8de789ac4f8b25a5d3c7a3fa727ae4196 (v0.3.7).

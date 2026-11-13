# Flea 0.4.3

- A touchscreen works again on Wayland: a tap clicks, a double tap opens, and a moving finger scrolls every list, the preview, menus and Settings (#479, @nicolasfalesy).
- A window left open and idle no longer climbs in CPU and memory (#478, @Imacx-maria).
- A window whose graphics device was lost, by a driver reset or a GPU fault, draws again on its own instead of staying blank until it is closed (#478, @Imacx-maria).
- The first file or folder drag of a session shows its drag image at once instead of freezing the window, and a folder under a drag no longer says "copy here" for a moment before "move here" (#463, @nicolasfalesy).
- Thumbnails no longer go missing in Grid under a filter or a search, and a switch from a filtered Grid to List or Columns lands with the cursor row fully in view (#477, @muellan).
- After a delete the cursor stays on the place of the first deleted file in List, Grid and Columns, instead of following a row far away in a long folder (#476, @muellan).
- After a drop, a paste or a move the listing keeps its scroll position and cursor instead of jumping to the top.
- Folder sizes stay on screen while a folder is read again after a move, a paste or a change inside it, instead of blinking to the pending dot.
- In Settings > Keys the Keyboard Shortcuts hint no longer draws over the key rows at a fractional display scale (#482, @atomicol).
- A BitLocker partition shows as a locked drive and opens the unlock card, as a LUKS one does (#480, #481, @JonathanPitre).
- HDR video previews (HLG and PQ) keep their highlights instead of drawing overexposed (#474, @sixers).
- A 3MF project with many parts previews instead of ending the preview worker (#467, @ids444).
- Delete Permanently works on rclone mounts that renumber a file when it is renamed, for files and folders, and a refused delete no longer renames every object on the mount (#464, @lzm3, @jekitf).
- Folder sizes no longer cross into, or mount, autofs and network mounts below the folder being measured (#458, @hachre).
- A favourite whose folder comes back opens again without restarting Flea (#462, @nicolasfalesy).
- A phone folder that stops answering shows Reload in its not-responding state (#144, @mfilm77).
- A file dropped on a symlink that points at a folder lands in that folder, as on a folder row.
- Settings > View offers Wrap at row ends, off by default: in Grid, Right on a row's last tile moves to the next row's first and Left on a row's first tile to the previous row's last (#434, #433, @letargopausado-byte; #447, @manish0467; #439, @reverb256).
- Shift+Left and Shift+Right extend the selection in Grid. Both are new default keys in the listing, so a custom binding of either key no longer acts in List and Columns; the terminal interface and the sidebar are unchanged.
- With a filter on, Home, End, Page Up, Page Down, Left and Right follow the files being shown, whether Wrap at row ends is on or off.
- Under a filter that shows close matches, Shift with the arrows, Select All, and reveal after a click or a rename follow the order on screen.
- A filename too long for its row or tile shows whole in a tip while the pointer rests on it, in List, Columns and Grid (#352, @dyedfox; #214, @AksharP5).
- Ctrl+Left and Ctrl+Right move by word, Ctrl+Home and Ctrl+End reach the ends, and Shift extends the selection, in the address bar, Search, Filter, the rename field and the file chooser's name field (#466, @jotapesse).
- The file chooser goes Back and Forward with the mouse side buttons (#455, @chrizbee).
- The file chooser shows an application's own button label without its mnemonic underscore (#457, @derekroper).
- Folder size answers that repeat the size already shown no longer redraw their rows, and a small scroll keeps the size walks of rows still on screen (#400, @iccodes, @Loafer19).
- Leaving a folder stops a video thumbnail that is still decoding (#403, @k-kostin).
- Restore all in Trash finishes sooner on large selections, with the same recovery guarantees.
- A rename refused on the last visible row scrolls its message into view, in List, Columns and Grid.
- A click that ends a rename selects the row under the pointer even when the rename lands first.
- A click in Grid no longer pulls the cursor back to where List was scrolled.
- Print, Convert and Combine handle a path that holds a percent sign on ImageMagick 7.1.1.
- About > Updates names the package manager on every packaged install, and a keyboard rename that is refused says so in the rename sentence.
- The source builds on Rust 1.99 and later and is clean under Clippy on Rust 1.100.0. A source build through rustup selects Rust 1.100.0 from `rust-toolchain.toml`; a distribution cargo keeps its own compiler.
- The GUI library tests pass under default test parallelism, and the tools run on Python 3.15 (#456, #413, @reeseroyster).

Thanks to @AksharP5, @atomicol, @chrizbee, @derekroper, @dyedfox, @hachre, @iccodes, @ids444, @Imacx-maria, @jekitf, @JonathanPitre, @jotapesse, @k-kostin, @letargopausado-byte, @Loafer19, @lzm3, @manish0467, @mfilm77, @muellan, @nicolasfalesy, @reeseroyster, @reverb256 and @sixers.

# Flea 0.4.2

- A new search starts in the current folder (#451, @muellan).
- Large folders open at Tight row density instead of failing to load (#454, @austrasien).
- A second file paste queues behind a running transfer, with each job keeping its own progress, Cancel, receipt and Undo (#443, #444, @AksharP5).
- Copy, move, rename, Trash and delete act on the selected file when its name is not valid UTF-8, even beside a file with the same displayed name.
- A cross-drive move keeps its source intact if the new copy cannot be verified or a read-only parent prevents removal.
- Copies to removable drives confirm the written data before reporting completion, including encrypted USB drives, SD cards and Thunderbolt storage.
- Trash works on btrfs subvolume and bind mounts that GIO refuses, with listing, restore and Empty Trash supported (#436, @AkitaOnRails).
- Empty Trash from the sidebar keeps the current folder behind the confirmation card (#419, @k-kostin).
- Extract is available for several selected archives, with a separate job for each (#397, @MISTERNEGATIVE21).
- Extract, Compress, Convert and Combine publish complete outputs on rclone and WebDAV mounts without reporting a successful copy as failed (#370, @vitonique).
- Files with no known extension preview as text or code when their contents identify them, including executable scripts with a shebang (#377, @mtp13; #393, @filip-spaldon).
- A click on a playing video pauses it, and another click resumes it, in Quick Look and the preview pane (#424, @timothy-ecc).
- Videos with rotation metadata play in the right orientation without crashing the media helper on libmpv 0.41 (#450, @kevinpardo).
- A copy button beside the address bar copies the folder path, and a single click on empty address space opens the path editor (#327, @dpan).
- F3 toggles Dual and returns to the previous view, and Dual is remembered across relaunch.
- A folder open in both Dual panes keeps the same remembered sort in both (#179, @zm14).
- Settings > View offers Show tabs as icons, off by default (#308, @jlahijani).
- Settings > View can hide Omi in empty folders or stop the empty-folder messages rotating.
- Highlight on hover also controls sidebar, breadcrumb and share-row highlights, and stays off by default (#230, @alextakitani).
- Tag colours stay distinct from one another in listings and menus on the bundled themes.
- Columns thumbnails appear on every visible row at the chosen Row density.
- Dragging a selection band over a filtered listing marks only the files being shown.
- Up and Down in a filtered Grid move by the rows being drawn.
- Search keeps marked files, the cursor and the scroll position when files in its scope change.
- Search results still load when row density, text size or window height changes while the search is running.
- Filter and Sort stay disabled while Search is open, including while its query is being typed.
- One Enter in Search, Find or Filter opens the highlighted file or enters the highlighted folder.
- Fuzzy search highlights only the letters that matched, and long no-match headlines fit inside the pane.
- A double-click also selects a word in the card, Settings and Save-name fields.
- The address, rename, search, filter, Settings and Save-name fields offer a right-click menu with Undo, Cut, Copy, Paste and Select All.
- Dragging in the Save chooser name field selects text.
- Inline rename keeps the typed name and refuses the change if another file has replaced the selected one, whether rename started from the keyboard or the menu.
- Escape keeps an inline rename open while its change is pending, and renaming to a hidden name says why the row disappears.
- Carried files land in List, Columns, Grid and another Flea window; Escape in either window cancels the carry.
- The footer's Undo hint works on click, and completed undo and redo refresh the folder even when no file-watch event arrives.
- Undo and redo work inside the Trash view and show their receipts there.
- Restore recreates a missing original folder, and damaged Trash records no longer hide healthy entries.
- Empty Trash also removes leftover files and records omitted from the listing, and reports entries it cannot remove.
- Large Trash selections undo and restore faster.
- Tab and Shift+Tab reach Empty Trash, and the Trash view updates from the first real change after it opens.
- A single Permissions change and a Convert output can be undone, and Convert uses a free copy name when its output name is taken.
- Compress and Extract choose a free copy name when the destination name is taken.
- Archives keep names beginning with @ or a dash literally; ZIP skips pipes and devices and explains names it cannot preserve.
- Paste and Extract catch names that collide on a case-insensitive drive, and undo and redo preserve the original spelling, including on exFAT.
- Copy, folder-size walks and delete handle deeply nested folders without a stack overflow or running out of open files.
- Sparse files keep their holes when copied where the filesystem supports it, and Duplicate and Keep both work with names near the filename-length limit.
- A copy reports failure if its source is truncated before it finishes.
- Copies that exceed a shell file-size limit report an error without interrupting the rest of Flea.
- Convert and Combine keep percent signs in output paths literal.
- Eject and unmount refuse while a transfer uses the drive, name the running job, and keep the folder and transfer card in place.
- An open folder that disappears returns to its nearest surviving parent.
- Revisiting a folder recomputes its size when files have hard links outside it or the filesystem cannot report remote changes.
- Text and Markdown previews read UTF-16 and Windows-1252 text and omit the byte-order mark.
- CSV columns widen for longer values found while scrolling, and a spreadsheet's reading mark clears when the new sheet appears.
- Font previews use characters the font contains.
- Quick Look moves one file at a time over Grid, and opening it stops media in the Columns preview beneath it.
- End and Ctrl+End reach the final page of a code preview.
- Properties shows photo Camera, Lens, Exposure and Taken details.
- Empty files open in the application for their filename type.
- Permissions explains a read-only mode, keeps invalid Octal text for correction, retains the form after a failed change, and scrolls in a small window.
- Space toggles Always in Open With, and a failed launch keeps its reason visible.
- AirDrop shares one Receiving state across windows, stays responsive during changes, and shuts down cleanly on quit.
- AirDrop reports received files, links and failures in the footer and names the receiving device on send jobs.
- Shelf offers Copy to Clipboard for files (#361, @Ewerton).
- Plugin sidebar places keep their section titles and can draw the plugin's own mark (#418, @rms1982).
- Installed plugin executables start without an intermittent Text file busy refusal.
- Plugin menu actions reach the selected provider, and a refused destination keeps its reason.
- Settings opens at a wider size, long labels and hints fit, and keyboard-shortcut columns keep their chords whole.
- Job headings, plain-text file kinds and file-operation refusals read in the selected language, and right-to-left hints keep Latin key chords in order.
- Filenames in rows and breadcrumbs omit bidirectional override characters from their displayed text.
- A held settings lock opens a read-only session instead of hanging startup, and invalid saved settings start on defaults while preserving the original file.
- Settings and Favorites keep read and navigation errors visible instead of silently discarding them.
- Scrolling Favorites retries a stalled status check.
- Footer errors clear on their own, a new notice replaces an old error, and Retry appears only when another attempt can help.
- Failed-job reasons wrap inside the card, and network folders use their saved sidebar label without showing unknown free space as zero.
- Returning focus to Trash keeps keys on Trash, hidden controls leave the Tab order, and Tab and Shift+Tab move a menu's highlight.
- Tab-number and tab-move keys work from the sidebar, and a right-clicked Columns row keeps a visible menu target.
- Menus can open beside a running Jobs card, and Trash is refused before deletion is armed on a phone or mount without Trash.
- Ctrl+P reaches Print and appears in the menu's key hint.
- About identifies a flea-bin installation as a Release build.
- The TUI honours NO_COLOR, keeps menu and card focus visible, and draws readable image and 3D previews in monochrome and 16-colour terminals.
- The TUI opens symlinked folders under the link's path and explains a broken link.
- TUI Find and Filter open the highlighted result with one Enter, and Right or l chooses a file in chooser mode.
- Ctrl+click in the TUI keeps the previously clicked row in the selection, and F5 reloads.
- The TUI preview, key sheet and Properties answer paging and first/last keys, and p plays or pauses TUI media.
- TUI Go to folder cleans up dots and trailing slashes without resolving links, and r copies a relative path.
- TUI editing and bulk rename use VISUAL before EDITOR and name a missing editor or helper program.
- TUI receipts remain visible during Find, modified Return and Space do not accidentally accept fields or toggle options, and the key sheet fits a short terminal.
- flea --help, -h and -V answer directly, and unknown options are refused.
- Arch packages declare their conflict with mutt so pacman explains the shared /usr/bin/flea path (#437, @synacktic).
- The command interface distinguishes a closed window from a window that dropped a request without replying.

Thanks to @AkitaOnRails, @AksharP5, @alextakitani, @austrasien, @dpan, @Ewerton, @filip-spaldon, @jlahijani, @k-kostin, @kevinpardo, @MISTERNEGATIVE21, @mtp13, @muellan, @rms1982, @synacktic, @timothy-ecc, @vitonique and @zm14.

# Flea 0.4.1

- The AirDrop plugin, off by default, sends files to nearby Apple devices and accepts incoming files from a notification; it is built on [owly](https://github.com/t4t5/owly/releases/tag/v0.4.0) by @t4t5 and needs a supported Wi-Fi card and the owly package (#369, @netmojo).
- The file menu shows Properties and Permissions, Show Hidden Files and Hide Code Icons move to the background menu, a customized menu list is kept, and Settings > Menus brings either row back.
- Relative Path in the Copy As flyout copies paths relative to the listed folder (#296, @root1945).
- Network's + lists nearby hosts when Settings > Places > List nearby hosts when adding a share is on, off by default (#343, @gdeyoung; #32, @nixfred).
- Dragging while renaming selects filename text, and a double-click selects a word in every text field (#396, @k-kostin).
- The > and < keys open or close every marked folder in place, and a folder toggles from its mark without moving the cursor or a flicker (#412, @muellan).
- The Save chooser names a new folder before it is made (#417, @MrVoicer; #304, @OldJobobo).
- Save is enabled for the caller's file name when the Save chooser starts in an empty folder (#428, @vitonique).
- The first window opens faster (#381, @chrispository).
- 4K HDR video previews play at full frame rate.
- An idle NVIDIA GPU on a hybrid laptop stays suspended while it drives no display, unless FLEA_VK_KEEP_NVIDIA=1 is set (#376, @neuromante).
- Trash rows stay drawn while scrolling, on a refresh and when Trash changes (#422, #423, @Loafer19).
- Columns rows draw at the chosen Row density (#388, @brianblakely).
- TUI folder sizes survive tab switches, and a folder is walked again only after it changes (#390, @brianblakely).
- Tab and clicks stay off an empty TUI preview, so q still quits (#389, @brianblakely).
- Git marks show on rows inside folders opened in place (#411, @muellan).
- Highlight today's dates takes effect and refreshes at midnight (#427, @muellan; #408, @filip-spaldon).
- The Flash image row is reachable from the keyboard when Flash opens from a drive, and the image chooser says why it failed (#416, #415, @simonhull).
- Updating under a Lua-configured Hyprland session closes 0.3.12 windows (#414, @simonhull).
- One Enter in the Find and Filter lines acts on the highlighted row.
- A copied file or folder takes its final name only once it is whole, so quitting mid-copy leaves no partial file.
- LocalSend finds peers, sends and cancels reliably, including large sends and many files.
- Office document previews finish, large spreadsheets included, by converting the first 30 pages for the preview.
- The extraction card keeps the item count and bytes in step and shows a time left only when it can be estimated.
- A Cancel click closes the Compress and Extract password cards and the Settings > Plugins Add and Remove cards.
- The Compress format chips, the Encrypt checkbox and the Compress and Extract fields answer clicks.
- Quick Look decodes large PNGs faster and keeps the shown item until the next one is ready instead of fading.
- A file drag survives a workspace switch in List, Columns and Grid.
- Switching back to a tab keeps its remembered sort direction.
- Inactive windows draw a grey list cursor.
- Each tag keeps its own hue on every theme, so red, orange and yellow tags no longer draw green on Hackerman.
- A live language switch translates sidebar place names, the Projects row included.
- When no graphics driver can open a window, Flea names vulkan-swrast in the user's language and lists each refused adapter on stderr.
- The copy card's spinner is back, held still and dimmed while a job waits.
- The sign-in fields open when an SMB share lists nothing without credentials.
- The Grid cursor lands in the same place every time a filter clears.
- A small Copy As menu shows the half-row cue, and the HTML Source header has its word spacing back.
- The key sheet and the TUI use one Russian word for Select.
- The field benchmark reads NVIDIA GPU memory from the driver (#380, @nchudleigh).

Thanks to @brianblakely, @chrispository, @filip-spaldon, @gdeyoung, @k-kostin, @Loafer19, @MrVoicer, @muellan, @nchudleigh, @netmojo, @neuromante, @nixfred, @OldJobobo, @root1945, @simonhull, @t4t5 and @vitonique.

# Flea 0.4.0

- Flea's window is drawn by a new GPU renderer built on GPUI, with every view, card, menu, preview and dialog of 0.3.12 carried over. Machines without a working Vulkan driver fall back to software Vulkan and then OpenGL, and when no renderer can open a window Flea says so and points to flea --tui.
- Flea follows the system language and includes the 50 languages Omarchy offers, with missing strings falling back to English (#43, @eachann1024).
- On non-Latin keyboard layouts, digits and letter keys reach their bindings by their position on a US keyboard, while Latin layouts keep the printed key and typed text is unchanged.
- Theme, font and Window Transparency changes reach an open window at once, and window and card corners follow Hyprland's rounding.
- Ctrl+G searches the contents of files in the open folder (#252, @passport0819).
- Name search tolerates typos without listing a near match beside a real hit.
- Folders expand in place in List view (#113, @muellan).
- A tab can be dragged into its own window or onto another window, and files drag between panes and windows.
- A tab change closes the filter, search and Quick Look, and the tab strip exposes its controls to accessibility tools.
- With Start in set to Last folder, Flea restores tab folders and their order (#353, @dyedfox).
- X carries selected files to another application without moving the pointer (#57, @jesedv).
- Open agent here starts Omarchy's agent in the folder being shown (#61, @jesedv).
- Dragging the window's top bar moves the window, and a user@host:/path location opens as a network share (#32, @nixfred).
- Text in Markdown, Source and code previews can be selected and copied (#332, @axelfontaine; #358, @avillagran).
- Clicks used to select text in a JSON preview no longer pass through to the listing (#331, @axelfontaine).
- Image previews replace the cached thumbnail with an original decoded for the preview frame (#371, @letsfkccp).
- Quick Look images use the display scale when they are decoded (#336, @gjouret).
- PDF previews reopen without the image-reader crash reported on 0.3.12 (#347, @FortyTwoFifty).
- Moving to another video in Quick Look starts a fresh player that plays at once (#364, @jessekalil; #365, @aholbreich).
- 3D models and toolpaths, including STL, OBJ, PLY, 3MF, STEP and G-code, preview in the preview column and Quick Look.
- Markdown tables size their columns to their contents.
- Properties on several items shows their combined size, kinds and counts (#316, @muellan).
- The filter matches every name in a large folder, and clearing a deep filter restores the listing (#315, @muellan; #318, @itsmunzir; #362, @hegjon).
- List rows update their hover after a re-read, and each pane keeps its own row details.
- Folder sizes and size sorting no longer walk into a network or rclone mount (#368, @group6digital).
- Properties waits for a folder's full size and says unknown when none could be read (#172, @nerdislb).
- Emptying a large Trash starts as a cancellable job while the window stays responsive (#321, @muellan).
- Delete on a location without Trash offers permanent deletion (#325, @tko76).
- Copy and move jobs queue while another transfer is running (#363, @jakovius).
- Send with rclone uploads a file or folder as a checked job with progress and cancellation (#176, @nerdislb).
- The extract card shows progress while 7z unpacks (#264, @MISTERNEGATIVE21).
- Extracting onto an rclone mount falls back to a checked copy when no-replace rename is unsupported (#370, @vitonique).
- Navigation stays responsive after a rename (#265, @mubshrx).
- Enter and Open open every selected file with its default application (#288, @murilocb; #350, @OldJobobo).
- Opening a program on a noexec volume or without its run bit says why it could not run (#330, @jesusalcaladev).
- Open in terminal uses the selected row's folder and opens Ghostty in a fresh instance (#373, @chrispository).
- F and B page text, Markdown, PDF and office documents in Quick Look and the preview column (#320, @muellan).
- Bulk rename buttons take a click and Edit in Terminal is reachable from the keyboard (#305, @muellan).
- Unlock and bulk rename cards grow with the text size (#310, @OldJobobo).
- Copy and cut marks line up in List, Columns and Grid (#242, @muellan).
- The Kind header and cells stay on the right while their divider is dragged (#348 and #349, @felixzsh).
- A narrow List pane drops optional columns in order, and hidden columns reserve no width (#333, @razmikb; #366, @aholbreich).
- F4 fits enabled columns even when the window is too narrow to draw them (#375, @aholbreich).
- A resting pointer does not leave a hover highlight while the keyboard drives the listing (#322, @AyushKr2003).
- A FUSE folder is read again when the window returns to it (#324, @gjouret).
- CSV and TSV previews keep quoted headers aligned after a UTF-8 byte-order mark (#355 and #356, @AksharP5).
- A match late in a long name no longer paints over the next column (#326, @joebaus).
- The text size keys work with Ctrl alone as well as Ctrl+Shift (#302, @root1945).
- An .otf font that the system types as an office file previews as a font (#314, @avillagran).
- The file picker rail lists the same places as the main rail (#205, @akitaonrails).
- The file picker filters names in Browse and Recent (#297, @NjengaFelix; #312, @pvpmartins; #319, @itsmunzir).
- A Save dialog can create and enter a folder without leaving the dialog (#304, @OldJobobo).
- Clicking an existing file in a Save dialog fills Filename and selects its stem (#337, @vitonique; #367, @aholbreich).
- The file picker accepts path presses and Ctrl+wheel Grid zoom and keeps the requested hidden-file setting (#338, @OldJobobo).
- The file picker follows the Single Click setting (#372, @jotapesse).
- A long menu label ends in an ellipsis inside its row (#214, @AksharP5).
- An internal NTFS volume stays listed with Show unmounted drives off, and an idle HFS+ drive unmounts cleanly (#232, @MiguelDebruyne).
- The network rail collapses automatic CIFS child mounts of the same share (#374, @EmilsValdmanis).
- Thumbnailer field codes expand inside options such as --output=%o (#339, @k-kostin).
- F3D thumbnailers receive expanded options and a bounded 3 GiB address-space allowance (#357, @not-that-nda).
- Previews release images when selection changes, and a crashed window is not relaunched in a loop (#151, @marvreichmann).
- Third-party plugins run as separate processes, with settings, menu actions, key bindings and previews drawn by Flea.
- Plugin startup failures identify the plugin that failed, and PLUGINS.md provides the protocol and working examples.
- Flea checks for and installs updates through the Omarchy Package Repository.
- Convert processes selected images as a batch (#269, @muellan).
- The terminal interface previews office documents and 3D models, searches names and contents, and gains Print and Convert cards.
- The terminal interface can clear its selection without clearing the filter (#164, @muellan; #317, @itsmunzir).
- Architecture documentation is split into indexed files so AGENTS.md fits within 32 KiB (#299, @wmfeht).
- The architecture notes describe the terminal interface that now ships (#198, @johnkattenhorn).
- The fetch_update warning is suppressed without requiring a newer Rust API (#334 and #335, @paulnunezm).
- Test and benchmark drivers report missing optional tools before starting (#344 and #345, @gdeyoung).

Thanks to @aholbreich, @akitaonrails, @AksharP5, @avillagran, @axelfontaine, @AyushKr2003, @chrispository, @dyedfox, @eachann1024, @EmilsValdmanis, @felixzsh, @FortyTwoFifty, @gdeyoung, @gjouret, @group6digital, @hegjon, @itsmunzir, @jakovius, @jesedv, @jessekalil, @jesusalcaladev, @joebaus, @johnkattenhorn, @jotapesse, @k-kostin, @letsfkccp, @marvreichmann, @MiguelDebruyne, @MISTERNEGATIVE21, @mubshrx, @muellan, @murilocb, @nerdislb, @nixfred, @NjengaFelix, @not-that-nda, @OldJobobo, @passport0819, @paulnunezm, @pvpmartins, @razmikb, @root1945, @tko76, @vitonique and @wmfeht.

# Flea 0.3.10

- Tags in Omarchy's colours, shared with Dolphin. T tags the selection, 1 to 7 pick a colour, the menus gain a Tags row and flyout, a TAGS section can show in the sidebar (off by default), and search finds files by tag, such as the red tag.
- Pasting a folder onto one with the same name can merge the two, and undo puts both back as they were.
- Hovering a drag over a folder can open it (off by default), lists scroll while you drag near their edges, names that a stick's exFAT cannot hold are fixed before the copy, and an image on the clipboard pastes as a file.
- A run card shows what a script, program or AppImage will run and asks before starting it (#174, @dedeanth; #71, @nate8199).
- Bulk rename edits every selected name at once in your terminal editor, with swaps, bad lines and undo handled (#183, @muellan).
- Archives take a password for 7z, refuse to extract onto a drive without room, and open ISO images and package files.
- Encrypted drives unlock and relock in Flea, and flea --unlock does it from a terminal.
- A new image flasher writes an ISO or disk image to a USB stick and verifies every byte.
- An unlocked encrypted volume appears in the sidebar with its eject mark (#200, @akitaonrails).
- Back and forward return the cursor to the row it was on (#235, @t1nk333r).
- Listing a phone or network share no longer holds up the rest of Flea (#144, @mfilm77; #247, @itsmunzir).
- A WebDAV share's breadcrumb shows the name you saved it under (#237, @bthome; #246, @aholbreich).
- A refused WebDAV sign-in says the server did not accept the password, and a failure is no longer guessed as a TLS handshake failure from the address (#238, @bthome; #241, @GreyforgeLabs).
- Unmount is offered on every mounted volume, so a mounted NTFS drive can be unmounted from the sidebar (#232, @MiguelDebruyne).
- NFS and SMB shares that the system mounts appear under NETWORK in the sidebar, beside the shares Flea mounts (#257, @citizen-why).
- The file picker takes a typed path (#191, @dyedfox; #206, @akitaonrails; #256, @karacas).
- A drop on the Shelf's edge rail lands even after the rail has opened the card (#255, @liri2006).
- Flea skips Vulkan drivers for graphics cards the machine does not have (#248, @t1nk333r).
- Properties shows a folder's size and item count (#172, @nerdislb).
- A thumbnailer installed outside /usr, such as one under ~/.local, now runs inside Flea's thumbnail sandbox (#171, @matirossi93).
- Quick Look on an image that cannot decode and has no thumbnail yet waits for the row thumbnail and draws it, instead of showing "This image could not be read." (#263, @kevinbsr).
- Quick Look works when Flea was started with a relative path (#290, @fishandsheep).
- A copy or move onto a FAT or exFAT drive goes on past links, special files and names it cannot hold, and names what it skipped (#271, @jotapesse).
- Moving a folder onto an rclone mount that refuses no-replace renames no longer fails, and the move copies across with progress, cancel and the journal (#282, @protoavatar).
- Replacing a file on phone storage, where there is no Trash, works and Undo restores the old file (#261, @kosmicus).
- Copy, move or link follows the keys held at the drop, not at the start of the drag (#259, @muellan).
- The "/" filter narrows the rows in Columns view too (#260, @demostenex).
- A Shift range made after a scroll starts at the lone selected row, not at the cursor (#286, @dyedfox).
- Ctrl+V and Shift+Insert paste the first line of the clipboard into the filter and search lines (#287, @dyedfox).
- A bare digit switches to that tab (#284, @joebaus).
- With hidden files sorted last, a click on a file picker column header keeps them last, as every view does (#291, @KelpyShades).
- Grid view's selection band starts from the margin between tiles (#292, @KelpyShades).
- Flea can remember the view mode of each folder (#293, @carldx).
- The file picker remembers its sort (#281, @pedrosatin).
- The window can fade its background alone, leaving text and icons fully opaque (#274, @erikrjohansson).
- The system disk's other partitions, such as a Windows or second Linux install, show in the sidebar and follow Show unmounted drives, while boot and recovery partitions and the one holding / stay hidden (#215, @AyushKr2003; #217, @fsrodrig).
- Dual view's divider can be dragged and remembered, Tab reaches the rail, and Right on a rail row ejects it.
- Redo is also Ctrl+Shift+Z, the Tags flyout stays open while you pick, and a multi-selection's menu row reads "Rename N items…".
- Muted text meets 4.5 to 1 contrast, and a row that errored shows its state in words.
- A newer navigation cancels a listing that has stalled, so a slow folder no longer holds up the one you went to next.
- Search shows ranked results as you type: f opens it, Esc returns to the folder and the row you were on, and Enter keeps the results.
- Quick Look zooms an image with + and -, 0 returns to fit, and the media strip gains a volume track (#208, @iccodes).
- Properties shows a video's codecs and bit rate, and a photo's camera, lens, exposure and capture time (#208, @iccodes).
- A damaged volume that will not mount says why, with the system's reason as the detail (@maeandrew).
- In a pane without focus, selected rows keep a muted fill, and the sidebar lights the place that holds the current folder.
- Dialogs, panels and menus fade in and out the same way, and a long dialog keeps its title and buttons in view while its body scrolls.
- Window corners follow a Hyprland config reload without restarting Flea.

Known issues

- Deselect all is not in 0.3.10. There is no menu row or key for it, and Escape clears the selection instead (#164).
- Dragging a file onto Trash in the sidebar is not in 0.3.10. The Trash row has no drop target, so it moves to a later release (#235).
- Mounted ISO images do not appear in the sidebar yet. Unlocked volumes do, and image rows move to a later release (#200).
- The thumbnail stall reported in #268 is not fixed in 0.3.10. PR #268 changes only the thumbnail test harness.
- HFS+ eject could not be reproduced, so it is not claimed for 0.3.10 (#232).
- The memory growth and crash in #151 have not been reproduced; the report stays open.
- On a folder whose first row is a video the field run drew Flea's window about 28 ms after 0.3.9's, 530 against 502 ms on 2,000 mixed files and 541 against 513 on 1,700 (medians of 3 runs, with 0.3.10 launched first every time). Rerun with each version launched first in half of 10 runs, the medians are 502.5 against 502.5 ms on 2,000 files and 507.5 against 503.5 on 1,700. On 100,000 text files the field run had them level, 505 against 510.
- PCManFM's window appears first on all three folders (448, 420 and 416 ms), and in the field run Thunar's also appeared before Flea's on the two media folders (513 and 507 ms against 530 and 541).
- On the two media folders Flea's window uses 93.5 and 93.6 MiB, fifth for window PSS after Thunar, PCManFM, Nemo and Strata (34.2 to 87.5 MiB). On 100,000 files it uses the least, 80.7 MiB.
- On the media folders Thunar, and on 1,700 files Nemo, settle sooner and use less CPU than Flea, but the bench measured no thumbnail work from either, so those columns are not ranked.

Thanks to @dedeanth, @nate8199, @muellan, @akitaonrails, @t1nk333r, @mfilm77, @itsmunzir, @bthome, @aholbreich, @GreyforgeLabs, @MiguelDebruyne, @citizen-why, @dyedfox, @karacas, @liri2006, @nerdislb, @matirossi93, @kevinbsr, @fishandsheep, @jotapesse, @protoavatar, @kosmicus, @demostenex, @joebaus, @KelpyShades, @carldx, @pedrosatin, @erikrjohansson, @AyushKr2003, @fsrodrig, @iccodes and @maeandrew.

Source revision: f1c46dbf758dc93338f0d6e4bcbdcc829c4edc1b (v0.3.10).

# Flea 0.3.9

- Theme colours draw again on Qt 6.12, where QtQuick's own Color type hid the Omarchy palette and left every view black (#276, @jfg96; #278, @AndreasDellrud; #285, @zigmoo; #289, @playGitboy).

# Flea 0.3.8

- Copy, cut and paste reach the system clipboard, so they work between two Flea windows and with other file managers (#203, @akitaonrails).
- A drag between windows moves within a drive, copies across drives, and says the verb it will perform; a drop from another app follows that app's action (#212, @shawnyeager).
- Drop modifiers held at lift: Shift moves, Ctrl copies, Ctrl+Shift links.
- Trackpad scrolling matches Finder's speed, coasts after the fingers lift, and stretches at a list's ends; the mouse wheel is unchanged.
- [ and ] switch tabs, and tabs move by key or by drag (#163, @muellan).
- A middle click on a folder opens it in a new tab, and Ctrl+Return does the same for the folder under the cursor (#219, @matt-shearing; #229, @alextakitani).
- F5 and Ctrl+R reload the folder and say how many rows changed (#197, @johnkattenhorn; #80, @avillagran).
- A slow second click renames, a single click can open, and the list keeps context rows around the cursor.
- Escape can go up a folder, off by default (#29, @herr-brandt).
- Copy as copies the path, name or URI of the whole selection, and Ctrl+Shift+C copies its paths (#189, @AksharP5; #188, @AksharP5).
- Invert selection on V (#164, @muellan).
- Permissions takes several rows at once and shows mixed values (#220, @muellan).
- Paste as links and Show original in the menus.
- Make executable for a script with a shebang, and Open in terminal from the background menu.
- Recent in the sidebar, off by default, and favourites reorder by drag (#99, @morgoth).
- The keymap sheet finds menu rows, places and recent files, and ? opens it in Trash too (#227, @bcosta19; #228, @bcosta19).
- Kind sort breaks ties by extension (#190, @markallisongit).
- A Shift range keeps the rows already marked (#209, @muellan).
- Rename starts on the first try (#170, @MISTERNEGATIVE21).
- Return and keypad Enter confirm the file picker under the Mac preset (#225, @pomartel; #240, @GreyforgeLabs).
- The file picker gains a grid view with thumbnails and remembers the view last used (#191, @dyedfox).
- Markdown renders in Quick Look and the preview column, with Source one key away (#204, @fsrodrig).
- Markdown draws aligned tables (one too wide for its pane scrolls sideways and never splits a word), task lists, footnotes, nested lists and quotes, math and Mermaid diagrams, and the HTML a README uses: centred headers, pictures and badge rows, details, kbd, sub and sup.
- Previews appear about ten times sooner on a single move, and Quick Look shows the cached thumbnail while the full image decodes.
- The mouse forward button retraces a back, and does nothing while Trash is open (#236, @t1nk333r).
- Columns view's side columns follow files created, renamed or deleted by other programs (#244, @muellan).
- Every open tab can come back with Last folder.
- A revealed auto-hide sidebar takes its click, and a click-away rename commit keeps the clicked row.
- A key pressed right after clicking a selected name runs alone, with no rename editor opening behind it.
- Menus and flyouts widen to their widest row, so a long label is no longer cut beside its key hint (#213, @AksharP5).
- Menus, flyouts and the sidebar draw no scrollbar and keep no room for one; in a menu the wheel and trackpad move the highlight.
- A hung NFS, SMB or other network share no longer freezes Flea: listings, previews and file operations on it answer within seconds, and a slow rename or new folder says it is still running and finishes on its own.
- Moving 1,000 small files to a USB stick confirms every file on the stick before its source is removed, in about 4.3 s against 6.6 s for 0.3.7 measured the same way on the same stick (medians of 18 runs each; the 9.62 s in 0.3.7's notes came from a different harness).
- Undo and redo stay quick after a long session: one operation reads and writes the shared undo history once.
- Partitions with no filesystem, and EFI and other system partitions, no longer appear in the sidebar (#232, @MiguelDebruyne).
- A folder rename on an NFS server that refuses no-replace renames works in place (#245, @akitaonrails).
- Kernel NFS and CIFS mounts, and FUSE mounts outside home, appear with the network places.
- Tested on ext4, btrfs, xfs, f2fs, exfat, vfat, NTFS and NFS: renaming to a different case works on FAT, exFAT and NTFS, a copy over 4 GiB to FAT refuses before writing, mount and permission errors say why, and the status bar names every filesystem.
- One button style everywhere: every dialog, strip and card button draws the same frame, hover, press and focus ring.
- A focused field draws its own frame in the theme accent, and so does a focused protocol chip in the Network dialog.
- Rename uses the list view's editor in every view.
- The terminal version's rename editor no longer paints terminal control characters from a file name (#239, @GreyforgeLabs).
- flea-git now updates on every release with flea and flea-bin.
- The test suites run headless on an Omarchy box with no display, on Qt's generic theme (#253, @nixfred).

Known issues

- A new window maps about 35 ms later than 0.3.7 on a local folder and about 50 ms later on a USB folder, so PCManFM's window appears first on both (532 and 550 ms against 464 and 524, medians of 5 runs); the startup cost is the first item for 0.3.9.
- USB folders rank fourth for GUI PSS among the file managers measured (third in 0.3.7; 88.9 MB against 86.4 MB for 0.3.7 in the same run) and NAS folders third; PCManFM skips thumbnails on removable and network media, and Flea's floor is the Quickshell and Qt base.
- On the NAS, Strata settles with slightly less CPU (1.38 s against 1.44 s), as in 0.3.7.
- Quick Look's first Space on a Markdown document whose first screen is a table draws about 37 ms later than other documents (60 ms against about 23 ms), still ahead of Strata's 104 ms.
- The 5 GB memory growth and crash in #151 have not been reproduced; the report stays open.
- The NFS problem reported on 0.3.5 remains unexplained.
- The memory and CPU items deferred from 0.3.7 (List view memory, USB anonymous memory, the NAS backend's executable PSS, the driven navigation CPU cost and a 2-3 ms cached image-folder revisit) were not measured again for this release and move to 0.3.9.

Thanks to @akitaonrails, @shawnyeager, @muellan, @matt-shearing, @alextakitani, @johnkattenhorn, @avillagran, @herr-brandt, @AksharP5, @morgoth, @bcosta19, @markallisongit, @MISTERNEGATIVE21, @pomartel, @GreyforgeLabs, @dyedfox, @fsrodrig, @t1nk333r, @MiguelDebruyne and @nixfred.

Source revision: cafb37056e268f7fde86dc66f4b42107467759c1 (v0.3.8).

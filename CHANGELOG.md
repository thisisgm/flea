# Unreleased

- Directory-selection dialogs move their filter to a separate row when New Folder and the answer buttons would otherwise overlap.

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

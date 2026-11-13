# Flea 0.4.3

- Use a touchscreen again on Wayland: tap to click, double tap to open, and scroll every list, the preview, menus and Settings with a moving finger (#479, @nicolasfalesy).
- Leave a window open and idle without its CPU and memory climbing (#478, @Imacx-maria).
- Keep working after the graphics device is lost to a driver reset or a GPU fault: the window draws again on its own instead of staying blank until it is closed (#478, @Imacx-maria).
- See the drag image at once on the first file or folder drag of a session instead of a frozen window, with no "copy here" shown for a moment before "move here" over a folder (#463, @nicolasfalesy).
- Keep thumbnails in Grid under a filter or a search, and land with the cursor row fully in view after switching from a filtered Grid to List or Columns (#477, @muellan).
- Stay at the place of the first deleted file after a delete in List, Grid and Columns, instead of following a row far away in a long folder (#476, @muellan).
- Keep the scroll position and cursor after a drop, a paste or a move instead of jumping to the top.
- Keep folder sizes on screen while a folder is read again after a move, a paste or a change inside it, instead of a blink to the pending dot.
- Read the Keyboard Shortcuts hint in Settings > Keys below the key rows at a fractional display scale, where it drew over them (#482, @atomicol).
- Unlock a BitLocker partition from its locked drive row and unlock card, as with a LUKS one (#480, #481, @JonathanPitre).
- Preview HDR video (HLG and PQ) with its highlights kept instead of overexposed (#474, @sixers).
- Preview a 3MF project with many parts instead of ending the preview worker (#467, @ids444).
- Delete Permanently on rclone mounts that renumber a file when it is renamed, for files and folders, without a refused delete renaming every object on the mount (#464, @lzm3, @jekitf).
- Measure folder sizes without crossing into, or mounting, autofs and network mounts below the folder (#458, @hachre).
- Open a favourite again when its folder comes back, without restarting Flea (#462, @nicolasfalesy).
- Reload a phone folder that stopped answering from its not-responding state (#144, @mfilm77).
- Drop a file on a symlink that points at a folder and have it land in that folder, as on a folder row.
- Turn on Wrap at row ends in Settings > View, off by default: in Grid, Right on a row's last tile moves to the next row's first and Left on a row's first tile to the previous row's last (#434, #433, @letargopausado-byte; #447, @manish0467; #439, @reverb256).
- Extend the selection in Grid with Shift+Left and Shift+Right. Both are new default keys in the listing, so a custom binding of either key no longer acts in List and Columns; the terminal interface and the sidebar are unchanged.
- Move with Home, End, Page Up, Page Down, Left and Right through the files being shown while a filter is on, whether Wrap at row ends is on or off.
- Follow the order on screen with Shift and the arrows, Select All, and reveal after a click or a rename, under a filter that shows close matches.
- Read a filename too long for its row or tile whole in a tip while the pointer rests on it, in List, Columns and Grid (#352, @dyedfox; #214, @AksharP5).
- Move by word with Ctrl+Left and Ctrl+Right, reach the ends with Ctrl+Home and Ctrl+End, and extend the selection with Shift, in the address bar, Search, Filter, the rename field and the file chooser's name field (#466, @jotapesse).
- Go Back and Forward in the file chooser with the mouse side buttons (#455, @chrizbee).
- See an application's own button label in the file chooser without its mnemonic underscore (#457, @derekroper).
- Scroll with fewer redraws: folder size answers that repeat the size already shown no longer redraw their rows, and a small scroll keeps the size walks of rows still on screen (#400, @iccodes, @Loafer19).
- Leave a folder and have a video thumbnail that is still decoding stop (#403, @k-kostin).
- Finish Restore all in Trash sooner on large selections, with the same recovery guarantees.
- See the message of a rename refused on the last visible row scrolled into view, in List, Columns and Grid.
- End a rename with a click and have the row under the pointer selected even when the rename lands first.
- Click in Grid without the cursor being pulled back to where List was scrolled.
- Print, Convert and Combine a path that holds a percent sign on ImageMagick 7.1.1.
- Read the package manager's name in About > Updates on every packaged install, and see a refused keyboard rename say so in the rename sentence.
- Build the source on Rust 1.99 and later, clean under Clippy on Rust 1.100.0. A source build through rustup selects Rust 1.100.0 from `rust-toolchain.toml`; a distribution cargo keeps its own compiler.
- Run the GUI library tests under default test parallelism, and the tools on Python 3.15 (#456, #413, @reeseroyster).

Thanks to @AksharP5, @atomicol, @chrizbee, @derekroper, @dyedfox, @hachre, @iccodes, @ids444, @Imacx-maria, @jekitf, @JonathanPitre, @jotapesse, @k-kostin, @letargopausado-byte, @Loafer19, @lzm3, @manish0467, @mfilm77, @muellan, @nicolasfalesy, @reeseroyster, @reverb256 and @sixers.

# Font preview, installation and defaults

Open a font with **Enter** or **Space** to view it inside Flea. TrueType, OpenType
and TTC collections initially use Flea's existing Quick Look: three fixed samples
rendered by ImageMagick in an isolated helper, with cached images and font facts.
Use **Tab**, then **Enter**, or click **Manage font…** to replace that sample with
the interactive viewer and installation controls. ImageMagick is optional: if the quick sample cannot be
rendered, **Manage font…** remains available. Other supported containers open the
interactive viewer directly. Closing and reopening Quick Look restores the initial sample.

The columns view also shows an interactive, compact font preview; **Expand** opens
Quick Look. Font loading and inspection run only for the selected file, outside
directory listing and thumbnail work. Interactive specimens use Qt's `FontLoader`
inside the GUI; the initial Quick Look retains the existing isolated renderer.

The interactive viewer includes:

- A sample of characters actually present in the selected face.
- Uppercase and lowercase alphabets, numbers, symbols and accented characters.
- Lorem ipsum, a pangram and an editable text sample.
- Sample size controls, a collection face selector and variable font axis controls.

Missing characters in the alphabet or text samples may use the system's fallback
fonts. The glyph sample shows the selected face's own character coverage. Variable
axis changes affect the specimen; choosing a default applies the family, not the
specimen's axis settings or sample size. **Escape** closes Quick Look, including
when the sample editor has focus.

## Supported files

Flea validates file contents with Fontconfig and FreeType. A filename extension
opens the viewer but does not prove that the file is a usable font.

| Container | Handling |
|---|---|
| TrueType and OpenType (`.ttf`, `.otf`) | Preview and install when Fontconfig accepts the contents. |
| Collections (`.ttc`, `.otc`) | Select faces for preview; install or remove the complete collection as one file. |
| Web fonts (`.woff`, `.woff2`) | Preserve the original container; install only if the local Fontconfig configuration exposes it to applications. |
| Variable fonts | Preserve all variation data; show the available axes and their limits for the selected face. |
| Legacy fonts | Accept Type 1/42, CFF/CID, PFR, Windows FON/FNT, BDF/PCF and resource-font containers recognized by the local stack, including supported compressed bitmap fonts. |
| Metrics and other files | AFM/PFM metrics alone, unsupported EOT containers and Office documents are not installable fonts. |

Inspection accepts regular files up to 64 MiB. A format recognized by Fontconfig
may still be unsupported by an individual application or by Qt's preview loader.
There is no format conversion. The font backend is implemented in Rust, with native
FreeType FFI for variable axes and Fontconfig/coreutils commands for discovery and
cache refresh. It adds no Cargo dependencies and does not invoke Python. The Arch
packages include the required runtime dependency closure.

## Install and uninstall

Choose **Install for me** to install for the current user. No administrator access
is needed. Flea writes a content-derived filename under:

```text
$XDG_DATA_HOME/fonts/flea/
# Default: ~/.local/share/fonts/flea/
```

The validated snapshot is installed without overwriting another file. Collections
and variable fonts retain their original bytes. Flea refreshes the font cache and
requires the installed file to appear in Fontconfig's `fc-list`. If a new copy is
not discoverable, Flea removes that copy and refreshes the cache again. An existing
user font is preserved when an installation check fails.

**Uninstall** removes the matching Flea-managed copy and keeps the downloaded
source. When browsing a font directly inside `$XDG_DATA_HOME/fonts` or `~/.fonts`,
uninstall can remove that installed user file itself. System fonts and source
symlinks are never removed. Removing a collection removes all of its faces.

Reopen applications that keep their own font lists. Uninstalling a font does not
restore preferences that name its family; use **Restore previous** for those
preferences separately.

## Use a font as a default

Expand **Use as default…**, then choose **Apply** beside a destination. The font
must already be discoverable by Fontconfig, either at its source location or as
an identical installed Flea copy. Fonts already installed by the system can be
used without first copying them into Flea's directory.

| Destination | User preferences changed |
|---|---|
| Desktop interface | Fontconfig `sans-serif`, GTK 3/4 `gtk-font-name`, and writable `org.gnome.desktop.interface font-name` when available. |
| Documents | Fontconfig `serif`. |
| System monospace / Omarchy | Fontconfig `monospace`, which the Omarchy shell also resolves. |
| Alacritty | Families in the normal, bold, italic and bold-italic font settings. |
| Kitty | Primary family, with automatic bold and italic variants. |
| Ghostty | Primary family and configured style families; additional fallback entries remain. |
| Foot | Primary font and configured style fonts, retaining their size and fallback suffixes. |

Terminal destinations appear when the terminal is installed or configured.
Terminal and system monospace destinations require a face Fontconfig identifies
as monospaced, dual-width or character-cell. Terminal sizes, unrelated settings
and dotfile symlinks are preserved. An unsupported Alacritty configuration layout
is refused before any preference files are changed.

Fontconfig preferences live in separate `99-flea-*.conf` files under
`$XDG_CONFIG_HOME/fontconfig/conf.d` (default `~/.config/fontconfig/conf.d`). Flea
leaves the existing `fonts.conf` in place, including Omarchy's earlier preference.
These changes affect the current user. Applications with explicit font overrides
may keep those overrides. Reopen applications or terminals as needed; the Omarchy
shell may need restarting to display a new monospace family.

## Restore a previous default

Choose **Restore previous** beside the destination. Flea retains the preference
that existed before its first apply, even when another family is applied later.
The backup journal is private and stored under:

```text
$XDG_STATE_HOME/flea/font-defaults/
# Default: ~/.local/state/flea/font-defaults/
```

Restore reinstates the previous files or removes files created by that apply.
It refuses to overwrite preferences edited outside Flea after the apply. Keep the
journal until you have restored the destination or no longer need its backup.
Installed user fonts, applied preferences and these journals are user data; removing
the Flea package does not remove them. Restore defaults and uninstall unwanted
fonts before removing Flea if you want those changes undone.

## Developer validation

```bash
cargo build
bash tests/fonts.sh
```

The suite uses isolated user directories and a private GSettings keyfile backend.
Its real quick-sample integration leg requires ImageMagick and bubblewrap.
It covers real font installation, discovery and removal; invalid inputs and
rollback; collection faces and variable axes; all seven preference destinations;
restore conflicts, dotfile symlinks and failure of the desktop settings service.
Native Rust tests also cover configuration editing and compatibility with backup
journals written by earlier versions. When installed, Kitty, Ghostty and Foot parse
the generated preferences themselves. A blocked-interpreter integration check
verifies that font inspection, installation, defaults and variable axes work without
invoking Python. Python remains a test harness and fixture generator.
`bash tests/fontpane.sh` covers the existing quick sample's lifecycle, theme changes
and stale replies. Rust tests cover the `.otf` MIME collision with OpenDocument
formula templates without treating an actual office document as a font.
The offscreen UI tests exercise the sample-to-management transition, pane/backend, editable sample,
install controls and default apply/restore controls. They do not change the
operator's font preferences.

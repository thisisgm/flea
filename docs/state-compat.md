# State compatibility with 0.3.12

The source of this inventory is tag `v0.3.12` (`f21789e56`). CONFIG, STATE, DATA and CACHE
mean the XDG homes, falling back to `~/.config`, `~/.local/state`, `~/.local/share` and `~/.cache`.
Empty variables use the fallback. No migration step or alternate 0.4.0 location is introduced.

`tests/state-compat.sh` replays the frozen files in `tests/compat/state-v0312.json`. The files
were produced by the source-built old executable in a marked sandbox using the listed writers.
The fixture records exact bytes and reader output, with only its sandbox root substituted.
Reads must leave all captured files byte-for-byte unchanged. It also reads malformed,
non-object, structurally foreign, unparsable and future-version documents. Each shelf state file, ui.json, picker reply and installed plugin manifest survives a real GPUI-child launch and the first writer attempt byte for byte, except a ui.json that is not a JSON object, which the first write keeps as ui.json.broken and replaces as 0.3.12 did. An empty or blank shelf.json is an empty pile, as 0.3.12 read it. A cache entry that is empty, cut short inside its writer's own format line, or damaged after that line is a miss that the next write replaces, as 0.3.12 did. Unsupported state reads as defaults in memory; writers refuse it with the path and a reason.

| Path | Format/version | Owner and access | Coverage or GUI responsibility |
| --- | --- | --- | --- |
| STATE/flea/ui.json | JSON, stateVersion 3 | Rust Store persists; QML and TUI consume | Fixture: settings, keys preset, favourites, lastPath, lastTabs; GPUI browser/settings units must consume the same document |
| STATE/flea/ui.json.lock | flock, no schema | Rust synchronization | Captured and retained |
| STATE/flea/ui.json.broken | legacy verbatim backup, no schema | Rust Store writes it as 0.3.12 did: a ui.json that is not a JSON object is copied here, synced, then replaced by the defaults plus the patch | uistate suite and Store tests pin the backup bytes; foreign or newer-version objects are still refused unchanged |
| CONFIG/flea/view.json | legacy hiddenCols/uiScale JSON | Rust read-only fallback while ui.json is absent | Existing uistate migration suite; GPUI must use Store rather than a second migration |
| user keys.toml | absent | No user-file reader | Shipped keys.toml is embedded; overrides are ui.json.keys: default/vim/mac/windows |
| separate tab/history files | absent | Tab folders/index persist inside ui.json; navigation stacks are in memory | GPUI browser/tabs unit owns restoration |
| HOME/.config/gtk-3.0/bookmarks | GTK URI and optional label, unversioned | QML NetworkPlaces reads/writes; Rust shelf places reads CONFIG path | GPUI rail/places unit must preserve QML's HOME path behavior or explicitly reconcile its XDG difference |
| DATA/recently-used.xbel | desktop XBEL | Desktop writes; QML reads only | GPUI browser/picker Recent units must read it; Flea has no writer |
| file user.xdg.tags xattr | comma-separated names, unversioned | Rust backend reads/writes | Fixture drives the old settags writer and new tagstates reader; no XDG tag database |
| persisted search index | absent | Rust session memory only, 200,000-entry cap | GPUI search unit reuses backend |
| STATE/omarchy/flea-shelf/shelf.json | unversioned JSON items | Rust CLI writes; plugin/QML reads | Fixture add/pin/clear/restore; GPUI shelf rail consumes the same rows |
| same directory/shelf.json.lock | flock | Rust | Captured |
| same directory/drags.json, drags.json.lock | unversioned JSON drags with tokens/identities; flock | Rust drag writer/redeemer | Fixture drag-begin; live token expiry/identity replay still needs native integration |
| same directory/piles.json | unversioned JSON, newest five piles | Rust clear/restore/piles | Fixture writer and read |
| same directory/summon.json, summon.lock | unversioned JSON counter; flock | Rust toggle writes; plugin reads | Fixture toggle; GPUI shelf unit consumes counter |
| same directory/undo.json, undo.lock | unversioned move identities; flock | Rust move/undo | Fixture shelf move writer; native same-inode undo must still be exercised |
| same directory/dests.json | unversioned JSON, newest five destinations | Rust shelf transfer writes and places reads | Fixture shelf move writer; destination reader remains in existing shelf tests |
| same directory/reply.json | transient picker response JSON | QML writes; Rust shelf/flash chooser reads/removes | GPUI picker unit must retain reply publication before child exit |
| same directory/archives/shelf-date[-n].zip | ZIP user archives, unversioned | Rust shelf zip publishes and adds to pile | Fixture shelf zip writer; persistent user content, never disposable cache |
| same directory/cancel | cancel plus newline | Rust CLI writes; transfer polls | Fixture cancel |
| STATE/flea/recovery/*.review | framed JSON records, header version 1 | Rust durable Trash recovery | Existing recovery tests reject foreign versions |
| DATA/Trash/files and info | freedesktop payload and .trashinfo | Rust backend reads/writes | Existing Trash suites; no format change |
| CACHE/thumbnails/large/*.png | freedesktop PNG Thumb metadata | Rust thumbnail cache; QML consumes path | Existing thumbs suite; GPUI media unit consumes returned path |
| CACHE/thumbnails/fail/flea/*.png | PNG failure metadata, X-Flea-Table | Rust | Existing thumbs suite |
| CACHE/thumbnails/flea-html/*.png and .deps | html-v3 key; dependency JSON version 1 | Rust HTML preview | Existing html-preview suite |
| CACHE/flea/raw/*.jpg | source-mtime-stamped JPEG, unversioned | Rust extraction | Existing raw-fixture suite |
| CACHE/flea/fonts/*.png | sample-keyed PNG, unversioned | Rust fontlook | Existing fontpane suite |
| CACHE/flea/pdf/pid/* | session-private PDF copies | Rust pdfcopy | Existing preview-pdf-flow suite |
| CACHE/flea/office/*.pdf and fail/flea/*.txt | source-mtime-stamped PDF/failure text | Rust officepdf | Existing media/preview suites |
| CACHE/flea/figures/key/manifest, math.bc, mermaid.bc | flea-figures 1 manifest, engine-specific bytecode | Rust figure compile | Existing figure-warm tests; stale engine keys remain misses |
| same directory/key.lock, key.failed, temporary generations | build coordination/failure markers | Rust | Existing figure-warm tests |
| CACHE/flea/figures/svg/*.svg | flea-figure-svg 4 framed key and SVG | Rust figure-store get/put/known | Format-1 entries miss and can be replaced by a new render; document source and captured input bytes remain unchanged |
| CACHE/flea/prefetch | flea-prefetch 2, byte ranges | Rust launcher reads; backend records | Existing prefetch tests; production prewarm remains disabled |
| CACHE/flea/vulkan-probe | Legacy 0.3.12 flea-vulkan-probe 1, fingerprint/device rows | Retained legacy cache file | The GPUI launcher does not read or write a Vulkan probe cache |
| Atomic-write scratch and decode work directories | target.pid.tmp, .flea-work-*, .flea-*.png, RAW/office/prefetch/Vulkan scratch | Rust writers; interrupted writers may retain them | Existing subsystem cleanup tests; never reinterpret as saved preferences |
| CONFIG/mimeapps.list | freedesktop INI | Rust default claim/release | Existing modes/filemanager1 suites |
| DATA/dbus-1/services/org.freedesktop.FileManager1.service | D-Bus INI, ownership comment | Rust default claim/release | Existing filemanager1 suite |
| CONFIG/xdg-desktop-portal/portals.conf and desktop-portals.conf | INI preferred FileChooser key | Rust chooser claim/release | Existing portal suite |
| CONFIG/hypr/bindings.lua | Lua with marked blocks | Rust default/picker/shelf registration | Existing defaults/chooser tests |
| CONFIG/omarchy/plugins/io.github.thisisgm.flea-shelf/** | plugin manifest schemaVersion 1 and payload | Rust settings toggle installs; bar reads | Existing shelfplugin tests |
| CONFIG/user-dirs.dirs, CONFIG/flea/scripts/* | XDG assignments; script files | Shared read-only inputs | GPUI rail, scripts and capture units preserve existing sources |
| DATA/applications, icons, thumbnailers | desktop entries, icon assets, thumbnail specs | Shared read-only inputs | Backend and GPUI icon/handler units |
| Omarchy current theme and toggle files | Omarchy-owned formats | QML reads, never Flea-owned persistence | GPUI theme unit owns matching reads |
| CONFIG/DATA Vulkan manifests/settings | Vulkan JSON | Rust absent-vendor filter reads only | Existing ICD filter tests; GPUI launch adds no display pin |

There is no separately versioned QML settings format to translate: `ui.json` is shared Rust-owned
persistence. GUI responsibilities above are grouped by their owning surface. Each implementation must
consume the existing backend persistence rather than introduce a second migration.

The initial replay fixture is deliberately bounded. It does not prove every cache generator,
durable recovery writer, live drag identity, or external desktop registration. Those remain
explicit native upgrade validation requirements, together with a real 0.3.12 home. The unknown-file
write guards also cover shared undo, Vulkan and page-prefetch records, GVFS runtime publications,
figure generations and SVG entries, HTML dependency sidecars, and render cache payloads. Cache
misses still run the live path but do not authorize replacing an unsupported document. Eviction
keeps those documents as well. Known older schemas retain their existing readers and writers;
unversioned shelf formats accept their historical shape and refuse a newly stamped schema.

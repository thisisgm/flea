# Opening a selection

Enter and context-menu Open send a deliberate multi-selection to the desktop's default applications. Files with the same default application are passed together to `gio launch`, so an image viewer such as imv receives selected PNGs and JPEGs in one launch. This is not image-specific: each default application receives its own group. The desktop entry remains responsible for multiple-file support, terminal handling and D-Bus activation; Flea does not parse its Exec line or hardcode an application.

Keyboard Open resolves selected indices through the backend, including indices outside the held viewport. Menu Open uses the validated snapshot's paths. Neither route scans a directory or loads offscreen icons, thumbnails or MIME metadata in the UI.

`flea --open-many <path> <path> ...` canonicalizes and validates the entire selection before launching any group. Folders, unavailable files and programs requiring a run decision refuse the batch: open those individually. Missing defaults also refuse the batch. A launch failure reports failure without retrying successful groups. Single-file Open retains navigation, archive preview, run-card and Open-with behavior. The existing single-flight opener guard covers the whole batch.

## Checks

- `tests/js.sh nav`: selection resolution, offscreen indices, captured menu paths, pending-claim protection and the unchanged single-file entrance.
- `cargo test --locked open_many::`: grouping across MIME types, selection order, argv boundaries and mode arity. Isolated subprocess tests cover the complete path-to-launch route with a stub GIO, canonical symlink targets, null stdout, batch refusal before any launch, missing defaults and launcher failures. No real desktop application is started.
- Manual acceptance after building: select several images (including a name with spaces), press Enter or context-menu Open, and confirm the viewer's next/previous navigation contains exactly that selection. A single image must still open normally.

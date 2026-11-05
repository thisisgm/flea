# Replacing a running 0.3.12 window

The launcher runs this handoff only on its GPUI path. It does not modify the frozen QML frontend.

An instance is identified by the `quickshell/by-shell/flea/<instance>` link, its exact `by-id`
link target, the matching `by-pid/<pid>` link, and the live process holding the POSIX write lock
on `instance.lock`. The runtime directory must be owner-only. Registry directories must belong to the caller and must not be symbolic links; their creation umask does not weaken the private runtime root. The lock is opened nonblocking with `O_NOFOLLOW`. The process
must have the caller's uid, the exact `-p <FLEA_UI>/boot/shell.qml` argument pair (the packaged
path when `FLEA_UI` is absent), and a nonempty `FLEA_BIN`. Its `/proc/<pid>/stat` birth time is
checked again before IPC and closure. No process is selected by its name.

Each IPC command executes the verified process through `/proc/<pid>/exe`, preserving any interpreter prefix before `-p`, then uses `ipc --pid <pid> call flea <function>`. This also reaches a native Quickshell executable unlinked during the upgrade. Output is limited to 64 KiB
and 750 ms. The launcher reads `statusActivityState`, `renameEditorLive`, and `providerState`.
A running operation, rename editor, invalid response, unheld selected row, or timeout refuses
the new launch with status 2. All old windows pass preflight before any is closed.

For an idle instance, Hyprland's client list must name its pid and a valid hexadecimal window
address. `hyprctl dispatch closewindow address:<address>` invokes the normal last-window-close
path. A Lua session refuses that shape with a syntax error, so the launcher then sends
`hl.dsp.window.close({ window = "address:<address>" })`, which takes the same path. In v0.3.12 `ui/boot/shell.qml` calls `WindowBody.quitBackends`, which sends `quit` to both
backends and waits for their drain before terminating the shell. The launcher waits up to 30 s
for each original process identity to disappear. It never force-kills a window or backend.
Any failed close or remaining old instance prevents the new launch. If a later close fails, earlier windows may already have closed; they are not force-restarted.

All instances supply path, cursor and selection snapshots in `FLEA_UPGRADE_WINDOWS`, for the GPUI browser/tabs unit to restore. The last instance in registry-id order supplies its open path, first selected path (otherwise
cursor path), and selected-path array. Explicit caller path and `--select` take precedence.
The path and cursor travel as `flea-gui` arguments (`--select <cursor> -- <path>`), never through the environment. Full marks travel in
`FLEA_UPGRADE_SELECTION`; the GPUI browser unit must consume that array. Multiple old windows
cannot all become one initial GPUI window; saved tabs and dual paths remain in `ui.json`.

The portal and FileManager1 daemons are left alive. They load their Python code before the
upgrade and resolve the new `flea` executable for each child. Their public argv and reply-file
protocols are preserved. An already open QML picker is not a browser instance and is not killed:
it must finish its reply before its old runtime assets are removed.

Other compositors have no verified graceful-close route here and refuse a live browser handoff.
The GPUI unlock dialog and full selection restore must be supplied by their owning GUI units;
the GPUI picker takes its reply file as `flea-gui --pick <reply>`. A hung old window stays untouched and blocks replacement rather than risking its writes.

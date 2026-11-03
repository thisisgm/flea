# Plugin examples

These five examples use Flea plugin protocol version 1. See
[PLUGINS.md](../../PLUGINS.md) for the manifest reference, complete request and
response exchanges, runtime limits and plugin lifecycle. The `shelf` kind is
described there; this directory has no shelf example.

All examples use Python 3 and the standard library. `checksums` requires Python
3.11 or newer for `hashlib.file_digest`. Flea also needs `bwrap` and `prlimit` to
run `torrent-preview` in its preview sandbox.

| Folder | Plugin id | Kind | Executable |
| --- | --- | --- | --- |
| [checksums](checksums) | `example.checksums` | `menu-action` | `plugin.py` |
| [send-target](send-target) | `example.send-target` | `send-target` | `plugin.py` |
| [syncthing](syncthing) | `example.syncthing` | `rail-section` | `bin/run` |
| [virtual-place](virtual-place) | `example.virtual-place` | `place` | `bin/run` |
| [torrent-preview](torrent-preview) | `example.torrent-preview` | `previewer` | `preview.py` |

## Try an example

Run these commands from the repository root, substituting the folder and plugin
id from the table to try a different example:

```sh
flea plugin validate "$PWD/examples/plugins/checksums"
flea plugin add "$PWD/examples/plugins/checksums"
flea plugin enable example.checksums
flea plugin list
```

Validate checks the manifest and executable path without running the plugin.
Add installs a copy of the folder, initially disabled. Enable checks the startup
exchange. Neither validate nor add installs Python or other dependencies.
Contributions are available only while the plugin is enabled and healthy.

To disable or remove the installed copy:

```sh
flea plugin disable example.checksums
flea plugin remove example.checksums
```

Remove also deletes the plugin's saved settings. It leaves the source folder in
this repository unchanged.

## checksums

Select one or more files and choose **Verify Checksum…** in the file menu. Flea's
input card asks for the expected SHA-256 checksum. The example compares every
selected file with that same checksum and reports progress. All files must match
for success. A mismatch returns an operation error and leaves the plugin
available for another verification.

## send-target

The **Selection Receiver** provider offers **Local Receiver**, which is ready,
and **Offline Receiver**, which is unavailable. Sending to Local Receiver
acknowledges the selected paths and reports progress. This fixture performs no
network transfer or file copy.

## syncthing

The **Syncthing** rail section contains **Photos**, **Notes** and **Music**. Photos
and Notes report `on`; Music reports `off`. The rows point to folders under
`/tmp/flea-example-syncthing`, or under `FLEA_EXAMPLE_FOLDERS` when that environment
variable is set in Flea's environment. Use an absolute path for that variable.
The example returns fixed rows. It does not create the folders, contact
Syncthing or synchronize files.

## virtual-place

The **Example Files** rail row opens an in-memory place containing a **Photos**
folder and **notes.txt**. Photos initially contains **lake.png**. The example
handles listing, rename, delete, and copying or moving files between folders
inside its own place. It refuses transfers to or from local folders. Operations
change this process's fixture rather than real files. The fixture resets when
Flea starts a new plugin process, including after the 30-second idle stop.

## torrent-preview

The previewer handles `application/x-bittorrent` and returns plain text with the
torrent's name, total file count and size, and up to 200 file entries. It includes
the announce address when present. Flea uses it in the preview column and Quick
Look when no built-in preview handles the file. Each request runs in the preview
sandbox with the selected file available read-only at `/in/file`. The example
reads torrent metadata; it does not download the torrent's contents.

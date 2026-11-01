# Plugin examples

`checksums` declares Verify Checksum. Flea asks for the expected SHA-256 in its
native input card, then verifies the selected files against it. A mismatch is an
operation error; the plugin remains available for another verification.
`send-target` declares a local receiver fixture. It acknowledges selected paths and
performs no network transfer or file copy. Both use protocol version 1, require
Python 3.11 or newer, and keep reading requests until Flea closes stdin.

Each folder contains its own `manifest.json` and executable `plugin.py`. Add a
folder through Settings > Plugins > Add > From Folder, then enable it. The menu
contribution appears only while the plugin is enabled and healthy. Disable the
plugin to remove its contribution. The receiver's offline target stays unavailable.

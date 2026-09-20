# rclone upload monitoring

Flea detects `fuse.rclone` mounts without walking their contents. The GUI shows a
separate cloud status row for the active pane's **whole mount**, not the selected
folder. A directory's size is not upload progress: an incomplete size scan that
has counted no bytes reads **Unknown**, not `>0 B`.

With an explicitly configured private rclone RC socket the row distinguishes:

- **Waiting to upload**: closed files in rclone's write-back queue.
- **Uploading**: active upload count; bytes, active-file total and rate only when
  queue entries match unambiguous transfer samples from a single VFS endpoint.
- **Retrying**: a queued job has already failed an attempt.
- **Upload cache error**: rclone reports errored cache files or an out-of-space cache.
- **No pending uploads**: the sampled queue is empty. This is **not** a claim that
  files still open for writing are uploaded, or a checksum verification of remote data.
- **Unavailable**: missing configuration, disconnected endpoint, timeout, unsupported
  response or insufficient VFS cache telemetry. This never becomes an idle verdict.

Flea's normal copy progress measures the copy into the mount. The cloud row
measures rclone's subsequent write-back. Byte totals cover currently active files,
not the entire historical copy batch. Leaving the mount hides its status row;
Flea is not a background, all-mount transfer dashboard. No cancel/retry controls
are sent to rclone. The TUI does not have a live cloud row.

## Opt-in setup

Use an existing rclone mount with VFS disk caching (`writes` or `full`). Add RC
only when safe to restart the mount: first close writers and verify cached writes
have drained. **Do not clear the VFS cache or force an unmount to enable this.**
For a user service, create a private runtime directory and restrict the socket:

```ini
[Service]
RuntimeDirectory=rclone-private
RuntimeDirectoryMode=0700
UMask=0077
```

Preserve your existing mount command and flags, adding:

```text
--rc --rc-addr unix:///run/user/1000/rclone-private/rc.sock
```

Replace `1000` with your UID. There must be no group/other permissions on the
socket or its immediate directory. RC is a powerful control interface; do not
expose it over TCP for this feature. `--rc-no-auth` is **not** required by the
read-only methods Flea uses (tested with rclone 1.75.1). Flea does not read rclone
credentials or discover control endpoints by scanning processes/config files.

Create `$XDG_CONFIG_HOME/flea/rclone-status.json` (normally
`~/.config/flea/rclone-status.json`) as a regular, user-owned file that is not
writable by other users; merge with existing mappings:

```json
{
  "mounts": [
    {"mount": "/home/alex/Cloud", "socket": "/run/user/1000/rclone-private/rc.sock"}
  ]
}
```

Paths must be absolute. Mount mappings are exact, not path prefixes. Navigate
through the actual mount path; symlink aliases are not resolved during polling
because resolving them could block on remote filesystem I/O. Do not use rclone
`--devname`: its custom mountinfo source prevents reliable remote matching. Bind mounts and
separate rclone processes mounting the same remote require careful endpoint
selection: the operator owns the mapping; matching `fs` alone cannot prove a
unique mount instance. Unsupported configurations fail visibly, not as synced.

The probe verifies the endpoint's `fs` against Linux mountinfo, reads only
`vfs/stats`, `vfs/queue`, `vfs/list` and optionally `core/stats`, caps responses at
2 MiB and bounds its lifetime to four seconds. Requests run outside the GUI
thread, one query at a time; old results are discarded across path changes.
A completed Flea copy triggers a fresh sample. Navigation inside a known mount
keeps the row height stable but replaces the old verdict with Checking.
Active polling is one second after a result, idle/unavailable polling five
seconds, local re-detection thirty seconds. Polls never recursively enumerate or
stat files under the mount. A local read-only diagnostic is available as:

```sh
flea --cloud-status /home/alex/Cloud
```

See [rclone RC documentation](https://rclone.org/rc/) and
[VFS write-back caching](https://rclone.org/commands/rclone_mount/#vfs-file-caching).

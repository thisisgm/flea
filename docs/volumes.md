# Volume helper

The installed `flea-udisks` helper exposes filesystem volumes through UDisks2.
Call it in a child process, since authorization and device operations can wait.
Its existing `drives`, `open` and `eject` commands retain the image flasher's
whole-disk checks. These volume commands do not open block devices for writing.

`flea-udisks volumes` returns `{"volumes":[...]}`. Mounted volumes and removable
media appear by default. `volumes --unmounted` also includes eligible unmounted
internal filesystems. Neither command mounts anything. Ignore and system hints,
firmware and recovery partitions, partition-table containers and blocks without
a filesystem are excluded. Unlocked filesystems inherit their backing drive.
Hidden mount aliases do not hide a volume with an eligible mount; its row and
an already mounted response use the eligible path. System mounts remain guarded.

Each row carries `kind`, `device`, `path`, `label`, `mounted`, `removable`,
`mediaRemovable`, `readOnly`, `size`, `fstype`, `glyph`, `release` and
`releaseGlyph`. `kind` is `volume`, `glyph` is the named `drive` icon, and
`releaseGlyph` is the named `eject` icon on a mounted volume. `release` is
`unmount` for fixed storage, `eject` for removable storage, or empty when
unmounted. The view draws named icons rather than interpreting font code points.

`flea-udisks mount /dev/sdb2` mounts an eligible filesystem and returns
`{"ok":true,"path":"/run/media/user/Data"}`. An already mounted filesystem
returns its current path without mounting it again.

`flea-udisks release /mnt/data/subdir` finds the deepest containing mount,
independently of the unmounted-volume setting. A fixed volume is unmounted.
Each Unmount removes one alias. The helper reads MountPoints again after every
call and drains all aliases, with at most 32 attempts per filesystem. A call
that removes no mount point stops with `busy`; a read failure returns `error`.
A removable drive's mounted siblings are unmounted, its unlocked containers
are locked, and the drive is powered off when supported, or its media ejected
when supported. A busy filesystem stops the sequence without forcing it.
A drive carrying a system mount is never powered off. Lock and PowerOff run only after
one final refreshed drive listing shows no mounted filesystem. A mounted sibling there
returns `busy`. No command schedules
an automatic remount.

Actions return `{"ok":true}` on success. Refusals return
`{"ok":false,"err":"notListed"}`, `busy`, `denied` or `error` as machine
codes for the caller's existing localized messages. A daemon failure returns
`error`, including during enumeration, so a failed poll can preserve old rows.
Malformed command arguments exit 2. Operation outcomes are JSON with exit 0.

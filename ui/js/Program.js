.pragma library

.import "Format.js" as Format

// Which rows Flea starts itself rather than handing to the desktop; ui/Opener.qml open() asks this
// before flea --open, and src/program.rs asks the same question of the same mode before it spawns.

// A regular file with any of the three execute bits: what the kernel reads as permission to exec.
// The kind is asked first, because a directory's execute bit is the right to enter it and a device
// or a socket carrying one is not a program either.
function isRunnable(mode) {
    return (mode & Format.S_IFMT) === Format.S_IFREG && Format.isExecutable(mode)
}

// The execute bit is the operator's own statement that a file is a program, and the desktop
// database has nothing to say about one: an AppImage resolves to application-x-executable in
// generic-icons, which no handler claims, so gio open could only come back refused. A .desktop
// entry is the exception the desktop itself owns, because only it can read the Exec line inside.
function startsItself(row) {
    return !!row && !row.d && isRunnable(Number(row.p) || 0) && !/\.desktop$/i.test(String(row.n))
}

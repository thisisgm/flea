.import "../../ui/js/Program.js" as Program

function run(check) {
    // What src/program.rs stats before it spawns: the same three bits on the same mode, on a file
    // and on nothing else.
    check("a regular file with an owner execute bit is runnable", Program.isRunnable(0o100755), true)
    check("a group or other bit alone is one too, because that is what the kernel reads",
          Program.isRunnable(0o100111), true)
    check("a regular file with no execute bit is not", Program.isRunnable(0o100644), false)
    // A directory's execute bit is the right to enter it, which is why the kind is asked first.
    check("a directory is not runnable however it is moded", Program.isRunnable(0o040755), false)
    check("nor is a symlink, which canonicalize resolves before anything is started",
          Program.isRunnable(0o120777), false)
    check("nor is a character device with every bit set", Program.isRunnable(0o020777), false)
    check("a vanished row is not runnable", Program.isRunnable(0), false)

    // What ui/Opener.qml open() reads off the row ui/js/Nav.js hands it with the path.
    check("an executable AppImage is started rather than handed to the desktop",
          Program.startsItself({ n: "pcsx2.AppImage", i: "application-x-executable", p: 0o100755 }), true)
    check("and the same file without the bit is still the desktop's to open",
          Program.startsItself({ n: "pcsx2.AppImage", i: "application-x-executable", p: 0o100644 }), false)
    check("an executable script is started too",
          Program.startsItself({ n: "backup.sh", i: "text-x-script", p: 0o100755 }), true)
    // Only the desktop can read the Exec line inside a .desktop entry, however it is moded.
    check("an executable desktop entry stays with the desktop",
          Program.startsItself({ n: "steam.desktop", i: "application-x-desktop", p: 0o100755 }), false)
    check("a directory row is never started", Program.startsItself({ n: "Work", d: true, p: 0o040755 }), false)
    // Every caller that opens a path with no row, the menu's Open among them, keeps the desktop's route.
    check("no row, no run", Program.startsItself(undefined), false)
}

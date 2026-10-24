## Where the backend binary comes from

`ui/Backend.qml` spawns the child as `[Quickshell.env("FLEA_BIN") || "flea", "--backend"]`.
A relative path cannot be right, because `Process` resolves it against the working
directory of the `qs` process rather than against the config directory, and `qs` is started
from several different directories here, including a non-interactive ssh whose cwd is
`$HOME`. Measured with `command: ["./target/release/flea", "--backend"]` and `qs` started
from `$HOME`: the child never ran, and the log read `Process failed to start, likely
because the binary could not be found. Command: QList("./target/release/flea",
"--backend")`.

`workingDirectory` does not rescue it. It can only relocate a relative path that is wrong
in the packaged layout anyway, where the binary installs to `/usr/bin/flea` and the UI to
`/usr/share/flea/ui/`, so no single working directory makes `./target/release/flea`
correct in both layouts.

The default is therefore the bare name `flea`, which is exactly what a packaged install
puts on PATH, and `FLEA_BIN` is the development seam that points at `target/release/flea`
in this tree. Every script that starts `qs` for a test exports it. It is a deployment seam
and not a tuning knob: a user never sets it.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

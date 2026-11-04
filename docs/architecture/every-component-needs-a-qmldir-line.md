## Every component needs a qmldir line

`ui/qmldir` is the type authority for that directory, and **the qualifier decides whether a
reference goes through it**. An unqualified name resolves only through the qmldir, so a bare
`Backend` is a type only because a line declares one, however correct `Backend.qml` itself is.
A namespace-qualified name does not: under `import "." as Flea` an undeclared sibling still
resolves by file name, so `Flea.StatusBar` loads `StatusBar.qml` with no qmldir line at all.
`WindowBody.qml` carries both `import "."` and `import "." as Flea`, so both spellings are in scope
and only the qualified one falls back to the file on disk. Deleting the `Header`, `Row` or
`StatusBar` line therefore does nothing whatever with the caches cleared, not an error and not
a warning, while deleting the `Backend` line breaks the load outright: `StatusBar` and
`Backend` are both used from `WindowBody.qml`, and the qualifier is the only thing separating them.
Adding `ui/Backend.qml` alone left the config refusing to load, and the entire message
was `ERROR: Failed to load configuration` followed by `caused by @shell.qml[25:13]: Backend
is not a type`, which names the line that uses the type and says nothing about the qmldir.
Deleting that one line from a working tree reproduces it exactly and putting it back clears
it.

**Every component added to `ui/` still needs its own line**, `<Name> 1.0 <Name>.qml`,
alongside `singleton Theme 1.0 Theme.qml`. The qualified form loading undeclared is not a
licence to skip it: a singleton has no unqualified fallback and `Theme` is referenced bare
everywhere, and the first bare reference anyone writes to an undeclared component fails the
whole config load with a message that points at the use and never at the qmldir.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.

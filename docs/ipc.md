# Window IPC

The native window exposes the same typed function signatures as the QML `flea` and
`fleapicker` targets. The backend's JSON-lines protocol remains separate.

```
flea ipc [--pid N|--any] call <target> <function> [arguments...]
flea ipc [--pid N|--any] show
```

The `ipc` verb owns its arguments, including strings such as `--version` or
`--backend`; they do not dispatch another Flea mode.
Selectors precede `call` or `show`. `--pid` selects exactly that process and never
falls back to another window. With no selector, or with `--any`, the oldest live
endpoint is tried first. `-i N` and `--id N` accept the native instance ID, its PID.
Strings print bare, integers print in decimal, and booleans print `true` or `false`.
A returned string gets one trailing newline, including an empty string. A void
return writes no bytes. `show` prints `target NAME` followed by indented
`function NAME(argument: type): type` lines. Function order is not contractual.

The current QML oracle contains 302 browser functions and 22 picker functions,
324 target/function pairs and 315 distinct names. All are registered with their
argument names, types, return types and arities. There are also `version(): string`
and `quit(): void` lifecycle functions on both targets. Registry tests compare the
complete oracle signatures, rather than a historical function count.

Implemented browser reads are `flea.ready`, `flea.path`, `flea.total`, `flea.cursor`,
`flea.selectionCount`, and `flea.selectedIndices`. They read the current committed
shell model, including an empty selection, rather than a pending listing. `ready`
means the shell entity is available, not that a listing has completed.
`flea.version` and `fleapicker.version` return the package version. `flea.quit` and
`fleapicker.quit` stop the addressed process after its void reply is flushed.
The native picker model does not exist yet, so its oracle functions are refused.

## Exit statuses and streams

| Status | Meaning | Output |
| --- | --- | --- |
| 0 | Successful call or `show` | The returned bytes on stdout |
| 0 | Unknown target or function | `Target not found.` or `Function not found.` on stdout |
| 0 | Missing call target or function | `Target required to send message.` or `Function required to send message.` on stdout |
| 0 | Wrong arity or unparseable typed argument | Quickshell's diagnostic and function definition on stdout |
| 2 | Invalid client syntax or malformed wire request | A diagnostic on stderr |
| 3 | Registered function not ported | `FLEA_IPC_NOT_PORTED target.function` on stderr; stdout is empty |
| 255 | Missing, unavailable, unsafe or unresponsive endpoint | A diagnostic; a missing explicit PID prints `No instance found for pid N` on stdout |

Every diagnostic ends with one newline. A registered refusal is distinct from
lookup failure and a successful empty string. Integer arguments accept signed
32-bit decimal values with surrounding whitespace. Boolean arguments accept
`true`, `false`, `1`, or `0`. Strings are passed without interpretation.

## Socket and wire format

Each process binds `$XDG_RUNTIME_DIR/flea-ipc/PID.sock`. The runtime directory and
`flea-ipc` directory must be owned by the effective user with no group or other
permissions; the socket is mode 0600 inside a mode 0700 directory. Directory and
socket symlinks are refused. Clean shutdown removes only the socket inode this
process bound. Startup removes a stale socket only if its PID is dead and a
nonblocking connect probe reports refusal, with identity and liveness rechecked.
The probe never waits for queue space. An unrelated file, a live PID, or a
connectable, pending or backlogged socket is retained.

Each connection carries one UTF-8 JSON request line and one JSON reply line:

```json
{"call":["flea","path"]}
{"code":0,"stdout":"/home/user\n","stderr":""}
```

`{"show":true}` requests the registry. JSON escaping preserves spaces, newlines,
quotes, Unicode and empty arguments. Requests are bounded to 64 KiB, replies to
16 MiB on the client. Incomplete, invalid UTF-8 and oversized lines are refused.
The server admits at most 32 concurrent clients, bounds request reception to two
seconds, and applies two-second reply and model-response timeouts. The client
uses a nonblocking connect with a five-second deadline, then five-second read
and write timeouts. A connection deadline returns status 255 with a timeout
diagnostic. Stalled readers and disconnected
clients run on separate workers and cannot block the UI or other calls.

Sockets are bound before the GPUI event loop starts. Registry formatting and all
socket I/O run on workers. Model reads pass through a bounded channel into a GPUI
entity task. The task reads one current model and sends its answer without waiting
for the client to consume it. No filesystem or socket operation runs in that task.

## Harness selection

Shell harnesses source `tools/flea-ipc`, then call `flea_ipc_init "$binary"` once
after choosing the launched binary. `FLEA_FRONTEND=gpui`, or a direct `flea-gui`
launch, selects the native client. A direct `qs` launch explicitly initializes
with `flea_ipc_init qs`. The exported `FLEA_IPC_KIND`, `FLEA_IPC_BIN`, and
`FLEA_IPC_HELPER` keep that decision fixed through subprocesses and timeouts.

Use `flea_ipc_call` for the `qs ipc` grammar, `flea_ipc_drive` for existing
`omarchy-drive ipc` arguments, and `flea_ipc_wait` for existing IPC waits. Commands
under `timeout` or `env` invoke `"$FLEA_IPC_HELPER"` with the same arguments, or
its `drive` and `wait` verbs. QML calls pass through their original clients with
unchanged arguments, streams and status. Native `-p` selectors resolve an exact
configuration to one PID and refuse ambiguous windows. Prefer explicit PIDs.
`flea_ipc_list` supplies the selected frontend's instance list. Native IDs are
decimal PIDs. Python harnesses use `tools/flea_ipc.py`'s `Client` and pass its
`environment()` to subprocesses. The helper never probes a function to guess
which frontend answered.

`tests/gui-ipc.sh` exercises the real CLI and production socket server using the
`flea-gui` `ipc-fixture` example as a headless surface. Its marked private runtime
is mandatory. `tests/run-all.sh` builds the fixture before running the suite.
The same contract runs during `cargo test --locked --workspace`; GPUI tests also
exercise the shell entity's actual executor task. The headless fixture proves
transport and dispatch, while compositor rendering remains a separate check.

## Owned test windows

Window-driving harnesses source `tests/lib/flea-transport.sh`. Set
`FLEA_TEST_FRONTEND=qml|gpui` once, defaulting to `qml`, and use
`flea_test_launch PATH LOG`, `flea_test_call call TARGET FUNCTION`, and
`flea_test_quit` for normal shutdown or `flea_test_stop` for failure cleanup. Python harnesses use `Client.start_owned(PATH)` or
`tests/lib/flea_transport.py`'s `Window`. The helper chooses binaries only from
this worktree's `target/debug` or `target/release` build, or its explicit
`CARGO_TARGET_DIR`. It never finds a Flea binary on PATH.

Each launch gets a marked `mktemp` root, a private runtime and a dedicated process
group. A supervisor holds the launch directory open, providing a short runtime path
through `/proc/PID/fd/N/runtime` so Unix sockets fit even in long worktree paths. Calls
validate the launched PID's start time, environment marker, group and socket inode,
then verify the connected peer's PID. A failed or incomplete read cannot become
a successful empty reply. `Window.quit()` checks the native acknowledgement before
waiting for group drain. Native status 3 preserves `FLEA_IPC_NOT_PORTED` verbatim.
The current stage has no native picker launch; the helper reports
`FLEA_IPC_NOT_PORTED fleapicker.launch` rather than opening a browser as a picker.

Set `FLEA_TEST_WINDOWS_ROOT` to a private marked root to own several windows.
The existing IPC helper, PID lookup and instance listing then resolve only those
windows. Ambiguous configuration selectors and foreign PIDs fail. Cleanup signals
only verified members of a launched process group, waits for backend drain, and
keeps evidence roots. Unowned QML client forwarding keeps its existing arguments,
streams and statuses. `tests/native-transport.sh` covers controls and QML launches;
`tests/native-transport-gpui.sh` covers the stage native window where a compositor
or Xvfb is available.

Cleanup uses one bounded TERM/KILL drain, including failed startup. The supervisor
acts as a subreaper and retains the launch leader until its owned descendants have
exited and been reaped. Historical startup diagnostics remain available after a
successful cleanup retry. Native quit waits for natural exit and a zero process
status; cleanup that required a signal cannot satisfy its acknowledgement.
QML listings retain Quickshell's complete instance metadata and original instance
IDs. An ID selector resolves that metadata to an owned PID before forwarding.
The Xvfb launcher records start time and group and signals through a pidfd only
after checking the same identity. The transport suites run with no controlling
tty, a private PID namespace, and an explicit environment built with `env -i`.

"""Shell adapter for the owned Flea transport."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import stat
import subprocess
import sys

from flea_transport import REPO, TransportError, Window, private, qml_listing, qml_metadata
from flea_transport_peers import PeerWindow, peers


def append_to_log(fd):
    if stat.S_ISREG(os.fstat(fd).st_mode):
        fcntl.fcntl(fd, fcntl.F_SETFL, fcntl.fcntl(fd, fcntl.F_GETFL) | os.O_APPEND)


def windows(parent, pending=False):
    parent = private(parent)
    for path in sorted(parent.glob("w.*")):
        if (path / ("options.json" if pending else "window.json")).is_file() and not (path / "stopped").exists():
            window = Window(path)
            yield window
            if not pending:
                yield from peers(window)


def records(parent):
    for window in windows(parent):
        try:
            record = window.record()
        except (OSError, TransportError):
            if not isinstance(window, PeerWindow):
                raise
            continue
        yield window, record


def select(arguments):
    arguments = list(arguments)
    if arguments[:1] == ["--"]:
        arguments.pop(0)
    parent = os.environ.get("FLEA_TEST_WINDOWS_ROOT")
    if parent:
        candidates = records(parent)
    else:
        window = Window(os.environ["FLEA_TEST_WINDOW"])
        candidates = [(window, window.record())]
    pid = config = instance = None
    while arguments and arguments[0] not in ("call", "show"):
        flag = arguments.pop(0)
        if flag not in ("--pid", "-i", "--id", "-p", "--path") or not arguments:
            raise TransportError("owned IPC requires an exact PID or configuration")
        value = arguments.pop(0)
        if flag in ("-p", "--path"):
            config = Path(value).resolve()
        elif flag == "--pid":
            pid = value
        else:
            instance = (flag, value)
    matching = []
    for window, record in candidates:
        if pid is not None and pid != str(record["pid"]):
            continue
        if instance is not None:
            actual = qml_metadata(record)["id"] if record["frontend"] == "qml" else str(record["pid"])
            if instance[1] != actual:
                continue
            if record["frontend"] == "qml":
                window.selector = instance
        if config is not None and config not in (Path(record["ui"]).resolve(), Path(record["ui"]).resolve() / "boot", Path(record["ui"]).resolve() / "boot/shell.qml"):
            continue
        matching.append(window)
    if len(matching) != 1:
        raise TransportError(f"owned IPC selector matched {len(matching)} windows")
    return matching[0], arguments


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="verb", required=True)
    start = commands.add_parser("start")
    start.add_argument("path")
    start.add_argument("--log")
    start.add_argument("--ui")
    start.add_argument("--binary")
    start.add_argument("--backend")
    start.add_argument("--wrap", action="append", default=[])
    start.add_argument("--picker", action="store_true")
    start.add_argument("--select")
    start.add_argument("--share-runtime")
    start.add_argument("--fixture", choices=("html-lifecycle",))
    start.add_argument("--timeout", type=float, default=15)
    execute = commands.add_parser("exec")
    execute.add_argument("command", nargs=argparse.REMAINDER)
    for name in ("pid", "stop", "quit", "stop-all", "pids"):
        commands.add_parser(name).add_argument("root")
    lookup = commands.add_parser("lookup")
    lookup.add_argument("root")
    lookup.add_argument("ui")
    call = commands.add_parser("call")
    call.add_argument("root")
    call.add_argument("arguments", nargs=argparse.REMAINDER)
    for name in ("route", "drive"):
        commands.add_parser(name).add_argument("arguments", nargs=argparse.REMAINDER)
    listing = commands.add_parser("list")
    listing.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.verb == "start":
        parent = os.environ.get("FLEA_TEST_WINDOWS_ROOT", str(REPO / ".superpowers/g010t"))
        print(Window.start(args.path, parent=parent, binary=args.binary, backend=args.backend, picker=args.picker,
                           log=args.log, ui=args.ui, timeout=args.timeout, wrap=args.wrap,
                           select=args.select, share=args.share_runtime, fixture=args.fixture).root)
    elif args.verb == "exec":
        command = args.command
        if command[:1] == ["--"]:
            command.pop(0)
        if not command:
            raise TransportError("missing launch command")
        # The window has its own append handle, so this handle must append too to preserve its diagnostics.
        for fd in (1, 2):
            append_to_log(fd)
        Window.start(os.environ.get("FLEA_PATH", str(REPO)), parent=os.environ["FLEA_TEST_WINDOWS_ROOT"],
                     binary=None if Path(command[0]).name in ("qs", "quickshell") else command[0],
                     command=command, ui=os.environ.get("FLEA_UI"),
                     log=f"/proc/{os.getpid()}/fd/1", timeout=15)
    elif args.verb == "pid":
        print(Window(args.root).pid)
    elif args.verb == "quit":
        Window(args.root).quit()
    elif args.verb == "stop":
        Window(args.root).stop()
    elif args.verb == "stop-all":
        for window in windows(args.root, pending=True):
            window.stop()
    elif args.verb == "pids":
        for _, record in records(args.root):
            print(record["pid"])
    elif args.verb == "list":
        flags = args.arguments[1:] if args.arguments[:1] == ["--"] else args.arguments
        active = list(records(os.environ["FLEA_TEST_WINDOWS_ROOT"]))
        if len(active) == 1 and active[0][1]["frontend"] == "qml":
            record = active[0][1]
            qml_metadata(record)
            result = qml_listing(record, flags)
            sys.stdout.buffer.write(result.stdout)
            sys.stderr.buffer.write(result.stderr)
            return result.returncode
        rows = []
        text = []
        for window, record in active:
            if record["frontend"] == "qml":
                rows.append(qml_metadata(record))
                if "--json" not in flags:
                    result = qml_listing(record, flags)
                    if result.returncode:
                        raise TransportError(result.stderr.decode().strip())
                    text.append(result.stdout.decode())
            else:
                rows.append(dict(id=str(record["pid"]), pid=record["pid"], shell_id="flea",
                                 config_path=str(Path(record["ui"]) / "boot/shell.qml")))
                row = rows[-1]
                text.append(f'Instance {row["id"]}:\n  Process ID: {row["pid"]}\n  Shell ID: flea\n  Config path: {row["config_path"]}\n')
        if "--json" in flags:
            print(json.dumps(rows))
        else:
            sys.stdout.write("".join(text))
    elif args.verb == "lookup":
        for _, record in records(args.root):
            if Path(record["ui"]).resolve() == Path(args.ui).resolve():
                print(record["pid"])

    else:
        if args.verb == "call":
            window, arguments = Window(args.root), args.arguments
        else:
            incoming = list(args.arguments)
            if incoming[:1] == ["--"]:
                incoming.pop(0)
            if args.verb == "drive":
                index = 0
                while index < len(incoming) and incoming[index] in ("--pid", "-i", "--id", "-p", "--path"):
                    index += 2
                incoming.insert(index, "call")
            window, arguments = select(incoming)
        drive = args.verb == "drive"
        result = window.call(*arguments, drive=drive, selector=getattr(window, "selector", None))
        sys.stdout.buffer.write(result.stdout)
        sys.stderr.buffer.write(result.stderr)
        return result.returncode
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, TransportError, subprocess.SubprocessError) as error:
        print(f"flea-test-transport: {error}", file=sys.stderr)
        sys.exit(3 if str(error).startswith("FLEA_IPC_NOT_PORTED ") else 255)

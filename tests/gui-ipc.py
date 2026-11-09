"""End-to-end IPC contract, including unchanged QML forwarding and fixed selection."""
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
import tomllib

root, binary, fixture = map(lambda s: Path(s).resolve(), sys.argv[1:])
assert root.is_absolute() and (root / ".flea-test-sandbox").is_file()
repo = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(repo / "tools"))
from flea_ipc import Client

# The fixture answers flea-gui's CARGO_PKG_VERSION, so the expectation follows the crate a bump edits.
version = tomllib.loads((repo / "crates/flea-gui/Cargo.toml").read_text())["package"]["version"]

runtime = root / "r"
runtime.mkdir(mode=0o700)
(runtime / ".flea-test-sandbox").write_text("private IPC fixture runtime\n")
environment = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), FLEA_BIN=str(binary), FLEA_UI=str(repo / "ui"))
client = Client(binary, "gpui")
environment = client.environment(environment)
windows = []
checks = 0
roundtrips = []


def check(condition, detail):
    global checks
    assert condition, detail
    checks += 1


def call(pid, function, *arguments, target="flea"):
    started = time.monotonic_ns()
    result = subprocess.run([str(binary), "ipc", "--pid", str(pid), "call", target,
                           function, *map(str, arguments)], env=environment,
                          capture_output=True, timeout=6)
    roundtrips.append((time.monotonic_ns() - started) / 1_000_000)
    return result


def value(pid, function, expected, *arguments, target="flea"):
    result = call(pid, function, *arguments, target=target)
    check((result.returncode, result.stdout, result.stderr) == (0, expected, b""),
          (function, result.returncode, result.stdout, result.stderr))


def registry_inventory():
    # Sample declaration: function!("flea", "show", "void", &[]).
    declaration = re.compile(
        r'function!\(\s*"([^"]+)",\s*"([^"]+)",\s*"([^"]+)",\s*&\[(.*?)\]\s*,?\s*\)',
        re.DOTALL,
    )
    parameter = re.compile(r'\(\s*"([^"]+)",\s*"([^"]+)"\s*\)')
    functions = []
    for path in ("src/ipc/functions.rs", "src/ipc/registry.rs"):
        source = (repo / path).read_text()
        entries = list(declaration.finditer(source))
        check(len(entries) == source.count("function!("), (path, "unparsed registry declaration"))
        for entry in entries:
            target, name, result, arguments = entry.groups()
            check(target in ("flea", "fleapicker", "fleatab"), (path, target))
            check(not parameter.sub("", arguments).strip(" \t\r\n,"),
                  (path, name, "unparsed registry parameters"))
            arguments = ", ".join(f"{name}: {kind}" for name, kind in parameter.findall(arguments))
            functions.append((target, name, f"  function {name}({arguments}): {result}\n"))
    check(len({(target, name) for target, name, _ in functions}) == len(functions),
          "duplicate registry function")
    return "".join(
        f"target {target}\n" + "".join(signature for owner, _, signature in functions if owner == target)
        for target in ("flea", "fleapicker", "fleatab")
    ).encode()


def helper_forwarding():
    stub = root / "stub"
    stub.mkdir()
    for name in ("qs", "omarchy-drive"):
        path = stub / name
        path.write_text('#!/bin/bash\nprintf "<%s>\\n" "$@"\nprintf "stderr stays\\n" >&2\nexit "${STUB_STATUS:-0}"\n')
        path.chmod(0o700)
    env = dict(environment, PATH=str(stub) + ":" + os.environ["PATH"], FLEA_IPC_KIND="qml")
    helper = str(repo / "tools/flea-ipc")
    cases = [(["qs", "ipc"], [], ["--pid", "17", "call", "flea", "path", " a\nb "]),
             (["qs", "ipc"], [], ["-i", "abc", "show"]),
             (["omarchy-drive", "ipc"], ["drive"], ["-p", "/x", "flea", "total"]),
             (["omarchy-drive", "wait", "ipc"], ["wait"], ["-p", "/x", "flea", "ready", "true", "--timeout", "0"]),
             (["qs"], [], ["list", "--all", "--json"])]
    for status in (0, 1, 3, 255):
        for command, prefix, arguments in cases:
            direct = subprocess.run(command + arguments, env=dict(env, STUB_STATUS=str(status)), capture_output=True)
            forwarded = subprocess.run([helper] + prefix + arguments, env=dict(env, STUB_STATUS=str(status)), capture_output=True)
            check((direct.returncode, direct.stdout, direct.stderr) ==
                  (forwarded.returncode, forwarded.stdout, forwarded.stderr), (command, arguments, forwarded))
    script = '. "$1"; flea_ipc_init qs; export FLEA_FRONTEND=gpui; "$FLEA_IPC_HELPER" call flea ready'
    result = subprocess.run(["bash", "-c", script, "bash", helper], env=env, capture_output=True)
    check(result.stdout == b"<ipc>\n<call>\n<flea>\n<ready>\n", result)


try:
    helper_forwarding()
    for label in ("/first path\nnext", "/second"):
        log = (root / f"window-{len(windows)}.log").open("wb")
        window = subprocess.Popen([str(fixture), label], env=environment, stdout=log, stderr=log)
        log.close()
        windows.append(window)
        deadline = time.monotonic() + 10
        while not (runtime / "flea-ipc" / f"{window.pid}.sock").exists():
            check(window.poll() is None, f"fixture exited {window.returncode}")
            assert time.monotonic() < deadline, "fixture did not publish its endpoint"
            time.sleep(0.02)
    first, second = windows
    check((runtime / "flea-ipc").stat().st_mode & 0o777 == 0o700, "directory permissions")
    endpoint = runtime / "flea-ipc" / f"{first.pid}.sock"
    check(endpoint.stat().st_mode & 0o777 == 0o600, "socket permissions")
    value(first.pid, "path", b"/first path\nnext\n")
    value(second.pid, "path", b"/second\n")
    value(first.pid, "version", f"{version}\n".encode())
    value(first.pid, "ready", b"true\n")
    value(first.pid, "total", b"3\n")
    value(first.pid, "cursor", b"1\n")
    value(first.pid, "selectedIndices", b"0,2\n")
    value(first.pid, "selectionCount", b"2\n")
    value(first.pid, "missing", b"Function not found.\n")
    value(first.pid, "path", b"Target not found.\n", target="missing")
    for target, function in [("flea", "palette"), ("fleapicker", "path")]:
        refused = call(first.pid, function, target=target)
        check((refused.returncode, refused.stdout, refused.stderr) ==
              (3, b"", f"FLEA_IPC_NOT_PORTED {target}.{function}\n".encode()), refused)
    show = subprocess.run([str(binary), "ipc", "--pid", str(first.pid), "show"], env=environment, capture_output=True, timeout=6)
    check(show.returncode == 0 and b"target flea\n" in show.stdout and b"target fleapicker\n" in show.stdout
          and b"target fleatab\n  function taken(token: string): bool\n" in show.stdout, show)
    expected_registry = registry_inventory()
    check(show.stdout == expected_registry, "exact registry targets, signatures and declaration order")
    for name in (b"scene(", b"nativeGeometry(", b"pendingRequests(", b"previewReadiness(", b"sceneState(", b"presentedFrame("):
        check(b"function " + name in show.stdout, "native visual registry signature")
    idle = socket.socket(socket.AF_UNIX)
    idle.connect(str(endpoint))
    try:
        value(first.pid, "path", b"/first path\nnext\n")
    finally:
        idle.close()
    disconnected = socket.socket(socket.AF_UNIX)
    disconnected.connect(str(endpoint))
    disconnected.sendall(b'{"call":["flea","path"]}\n')
    disconnected.close()
    value(first.pid, "path", b"/first path\nnext\n")
    forwarded = subprocess.run(client.command("--pid", first.pid, "call", "flea", "path"), env=environment, capture_output=True, timeout=6)
    check(forwarded.stdout == b"/first path\nnext\n" and forwarded.returncode == 0, forwarded)
    ambiguous = subprocess.run(client.command("-p", repo / "ui/boot", "call", "flea", "path"), env=environment, capture_output=True, timeout=6)
    check(ambiguous.returncode == 255 and b"2 native windows" in ambiguous.stderr, ambiguous)
    any_window = subprocess.run([str(binary), "ipc", "--any", "call", "flea", "total"], env=environment, capture_output=True, timeout=6)
    check(any_window.stdout == b"3\n" and any_window.returncode == 0, any_window)
    check(call(2147483647, "path").returncode == 255, "missing pid fell back to a live window")
    for window in windows:
        value(window.pid, "quit", b"")
        check(window.wait(timeout=5) == 0, "quit did not stop the window")
        check(not (runtime / "flea-ipc" / f"{window.pid}.sock").exists(), "endpoint survived clean shutdown")
    print(f"GUI_IPC checks={checks} failed=0 roundtrip-ms={min(roundtrips):.3f}..{max(roundtrips):.3f}")
finally:
    for window in windows:
        if window.poll() is None:
            window.terminate()
        try:
            window.wait(timeout=3)
        except subprocess.TimeoutExpired:
            window.kill()
            window.wait(timeout=3)

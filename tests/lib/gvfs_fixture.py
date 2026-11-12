"""Exercise the public fixture through the real GMount and FUSE contract."""
import os
import http.client
from contextlib import closing
from pathlib import Path
import pwd
import selectors
import signal
import socket
import stat
import subprocess
import sys


REPO = Path(__file__).resolve().parents[2]
GIO = "/usr/bin/gio"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def run(argv, *, check=True, environment=None):
    result = subprocess.run(argv, env=environment, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=20)
    if check and result.returncode:
        raise RuntimeError(f"{argv[0]} exit {result.returncode}: {result.stderr.strip()}")
    return result


def isolated(root):
    account = pwd.getpwuid(os.getuid())
    require(account.pw_uid != 0, "run the fixture as its unprivileged owner")
    require(root.is_absolute() and not root.is_symlink(), "fixture root must be absolute and real")
    require(root.stat().st_uid == account.pw_uid and stat.S_IMODE(root.stat().st_mode) == 0o700,
            "fixture root must be owned by the current uid with mode 0700")
    require((root / ".flea-test-sandbox").is_file(), "fixture root has no ownership marker")
    home = Path(account.pw_dir)
    require(not root.is_relative_to(home), "fixture root must be outside the caller's home")
    for name in ("home", "tmp", "runtime", "config", "cache", "data", "state"):
        (root / name).mkdir(mode=0o700)
    environment = dict(PATH="/usr/bin:/bin", HOME=str(root / "home"), USER=account.pw_name,
                       LOGNAME=account.pw_name, LC_ALL="C.UTF-8", LANG="C.UTF-8", LANGUAGE="",
                       XDG_RUNTIME_DIR=str(root / "runtime"), TMPDIR=str(root / "tmp"))
    for name in ("CONFIG", "CACHE", "DATA", "STATE"):
        environment[f"XDG_{name}_HOME"] = str(root / name.lower())
    if "FLEA_FIXTURE_ROOT" in os.environ:
        environment["FLEA_FIXTURE_ROOT"] = os.environ["FLEA_FIXTURE_ROOT"]
    argv = ["/usr/bin/dbus-run-session", "--", "/usr/bin/python3", "-B", str(Path(__file__).resolve()),
            "--inside", str(root)]
    process = None
    termination = None
    signals = (signal.SIGTERM, signal.SIGHUP, signal.SIGINT)

    def interrupted(number, _frame):
        nonlocal termination
        if termination is None:
            termination = number

    reader, writer = os.pipe2(os.O_NONBLOCK | os.O_CLOEXEC)
    previous = {number: signal.signal(number, interrupted) for number in signals}
    previous_wakeup = signal.set_wakeup_fd(writer)
    descriptor = None
    try:
        if termination is not None:
            raise SystemExit(128 + termination)
        process = subprocess.Popen(argv, env=environment, start_new_session=True)
        if termination is not None:
            raise SystemExit(128 + termination)
        descriptor = os.pidfd_open(process.pid)
        with selectors.DefaultSelector() as ready:
            ready.register(descriptor, selectors.EVENT_READ)
            ready.register(reader, selectors.EVENT_READ)
            require(ready.select(timeout=120), "GVfs session timed out")
        if termination is not None:
            raise SystemExit(128 + termination)
    finally:
        # The unreaped leader pins its process group ID until every owned descendant is stopped.
        try:
            if process is not None:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                if descriptor is not None:
                    os.close(descriptor)
                process.wait(timeout=3)
        finally:
            signal.set_wakeup_fd(previous_wakeup)
            for number, handler in previous.items():
                signal.signal(number, handler)
            os.close(reader)
            os.close(writer)
    return 128 + termination if termination is not None else process.returncode


# Sample input: Mount(0): share.zip on 127.0.0.1 -> dav://127.0.0.1:2222/share.zip
def mount_lines():
    return [line.split(" -> ", 1)[1] for line in run([GIO, "mount", "-l"]).stdout.splitlines()
            if line.startswith("Mount(") and " -> " in line]


def boundaries(uri, mount_uri, unmount):
    mounts = mount_lines()
    require(mount_uri in mounts, f"GMount regression: {mount_uri} absent from gio mount -l: {mounts}")
    require(sorted(run([GIO, "list", uri]).stdout.splitlines()) == ["alpha.txt", "beta.txt"],
            "GMount regression: unexpected directory listing")
    require(run([GIO, "cat", uri + "alpha.txt"]).stdout == "alpha\n",
            "GMount regression: remote content differs")
    info = run([GIO, "info", uri]).stdout
    # Sample input: local path: /fixture/runtime/gvfs/dav:host=127.0.0.1,port=2222,prefix=%2Fshare.zip
    paths = [line.removeprefix("local path: ") for line in info.splitlines()
             if line.startswith("local path: ")]
    require(len(paths) == 1 and paths[0].startswith(os.environ["XDG_RUNTIME_DIR"] + "/gvfs/"),
            "GMount regression: no GVfs FUSE path")
    local = Path(paths[0]) / "alpha.txt"
    require(local.read_text() == "alpha\n", "GMount regression: FUSE content differs")
    require(run([GIO, "cat", uri + "missing.txt"], check=False).returncode != 0,
            "GMount regression: missing-file negative control unexpectedly succeeded")
    unmount()
    require(mount_uri not in mount_lines(), "GMount regression: mount survived unmount")
    require(not local.exists(), "GMount regression: FUSE file survived unmount")


# Sample input: dav://127.0.0.1:2222/share.zip/
def port_of(uri):
    return int(uri.split(":")[2].split("/")[0])


def endpoint_stopped(uri):
    port = port_of(uri)
    with socket.socket() as client:
        client.settimeout(2)
        require(client.connect_ex(("127.0.0.1", port)) != 0, "DAV server survived cleanup")


def fixture(root):
    tool = str(REPO / "tools/flea-gvfs-fixture")
    environment = dict(os.environ, DIR=str(root / "dav"))
    other = dict(os.environ, DIR=str(root / "other"))
    dead = dict(os.environ, DIR=str(root / "dead"))
    uri = run([tool, "make"], environment=environment).stdout.strip()
    require(uri.startswith("dav://127.0.0.1:"), "fixture must use the shared DAV backend")
    require(run([tool, "uri"], environment=environment).stdout.strip() == uri, "fixture URI changed")
    try:
        port = port_of(uri)
        (root / "private.txt").write_text("private fixture control")
        (root / "dav/src/escape.txt").symlink_to(root / "private.txt")
        for method, path, expected in (("GET", "/share.zip/%2e%2e/private.txt", 404),
                                       ("GET", "/share.zip/escape.txt", 404),
                                       ("PUT", "/share.zip/alpha.txt", 501)):
            with closing(http.client.HTTPConnection("127.0.0.1", port, timeout=5)) as client:
                client.request(method, path, body=b"changed" if method == "PUT" else None)
                response = client.getresponse()
                require(response.status == expected, f"DAV boundary accepted {method} {path}")
                response.read()
        run([tool, "mount"], environment=environment)
        boundaries(uri, uri.rstrip("/"), lambda: run([tool, "unmount"], environment=environment))
        run([tool, "unmount"], environment=environment)
        run([tool, "mount"], environment=environment)
        second = run([tool, "make"], environment=other).stdout.strip()
        require(second != uri, "independent fixtures reused one endpoint")
        run([tool, "mount"], environment=other)
        run([tool, "clean"], environment=environment)
        require(uri.rstrip("/") not in mount_lines(), "clean left its mount behind")
        require(second.rstrip("/") in mount_lines(), "clean unmounted another fixture")
        endpoint_stopped(uri)
        require(not (root / "dav").exists(), "clean left its sandbox behind")
        run([tool, "clean"], environment=environment)
        run([tool, "unmount"], environment=environment)
        require(run([tool, "uri"], check=False, environment=environment).returncode != 0,
                "missing fixture URI unexpectedly succeeded")
        replacement = run([tool, "make"], environment=other).stdout.strip()
        require(second.rstrip("/") not in mount_lines(), "remake left the previous mount behind")
        if replacement != second:
            endpoint_stopped(second)
        run([tool, "make"], environment=dead)
        run([sys.executable, "-B", str(REPO / "tools/flea-gvfs-dav.py"), "stop", str(root / "dead")])
        # A bound socket nobody listens on is what a killed server leaves behind.
        with socket.socket(socket.AF_UNIX) as orphan:
            orphan.bind(str(root / "dead/control.sock"))
        run([tool, "clean"], environment=dead)
        require(not (root / "dead").exists(), "clean refused a fixture whose server had died")
        print("gvfs: fixture=dav mount=ok list=ok read=ok fuse=ok missing-refusal=ok unmount=ok", flush=True)
        print("gvfs: concurrent-fixtures=ok clean=ok idempotent-clean=ok remake=ok", flush=True)
        print("gvfs: traversal-refusal=ok symlink-refusal=ok write-refusal=ok dead-server-clean=ok", flush=True)
    finally:
        failed = [(env["DIR"], result.stderr.strip()) for env in (environment, other, dead)
                  if (result := run([tool, "clean"], check=False, environment=env)).returncode]
        require(not failed, f"fixture clean failed: {failed}")


def main():
    if len(sys.argv) == 2:
        return isolated(Path(sys.argv[1]))
    require(len(sys.argv) == 3 and sys.argv[1] == "--inside", "invalid fixture arguments")
    root = Path(sys.argv[2])
    run(["/usr/bin/gdbus", "call", "--session", "--dest", "org.freedesktop.DBus", "--object-path",
         "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.ListNames"])
    try:
        fixture(root)
    finally:
        fuse = Path(os.environ["XDG_RUNTIME_DIR"]) / "gvfs"
        if os.path.ismount(fuse):
            run(["/usr/bin/fusermount3", "-u", str(fuse)])
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"gvfs: FAIL {error}", file=sys.stderr)
        sys.exit(1)

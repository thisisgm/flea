"""Check archive capability separately from the real GMount and FUSE contract."""
import os
from pathlib import Path
import pwd
import selectors
import shlex
import signal
import socket
import stat
import subprocess
import sys
import zipfile


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


def stop(process):
    if process.poll() is None:
        process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=3)


def isolated(root):
    account = pwd.getpwuid(os.getuid())
    require(account.pw_uid != 0, "run the fixture as its unprivileged owner")
    require(root.is_absolute() and not root.is_symlink(), "fixture root must be absolute and real")
    require(root.stat().st_uid == account.pw_uid and stat.S_IMODE(root.stat().st_mode) == 0o700,
            "fixture root must be owned by the current uid with mode 0700")
    require((root / ".flea-test-sandbox").is_file(), "fixture root has no ownership marker")
    home = Path(account.pw_dir)
    require(not root.is_relative_to(home), "fixture root must be outside the caller's home")
    for name in ("home", "tmp", "runtime", "config", "cache", "data", "state", "bin"):
        (root / name).mkdir(mode=0o700)
    client = root / "home/.ssh/config"
    launcher = root / "bin/ssh"
    options = ["/usr/bin/ssh", "-F", str(client), "-oNoHostAuthenticationForLocalhost=no",
               "-oStrictHostKeyChecking=yes", "-oBatchMode=yes", "-oIdentitiesOnly=yes",
               "-oControlMaster=no", "-oControlPath=none"]
    launcher.write_text("#!/bin/sh\nexec " + shlex.join(options) + ' "$@"\n')
    launcher.chmod(0o700)
    environment = dict(PATH=f"{root / 'bin'}:/usr/bin:/bin", HOME=str(root / "home"), USER=account.pw_name,
                       LOGNAME=account.pw_name, LC_ALL="C.UTF-8", LANG="C.UTF-8", LANGUAGE="",
                       XDG_RUNTIME_DIR=str(root / "runtime"), TMPDIR=str(root / "tmp"))
    for name in ("CONFIG", "CACHE", "DATA", "STATE"):
        environment[f"XDG_{name}_HOME"] = str(root / name.lower())
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


# Sample input: Mount(0): gm on 127.0.0.1 -> sftp://gm@127.0.0.1:2222/
def mount_lines():
    return [line.split(" -> ", 1)[1] for line in run([GIO, "mount", "-l"]).stdout.splitlines()
            if line.startswith("Mount(") and " -> " in line]


def boundaries(uri, mount_uri, unmount):
    require(mount_uri in mount_lines(), f"GMount regression: {mount_uri} absent from gio mount -l")
    require(sorted(run([GIO, "list", uri]).stdout.splitlines()) == ["alpha.txt", "beta.txt"],
            "GMount regression: unexpected directory listing")
    require(run([GIO, "cat", uri + "alpha.txt"]).stdout == "alpha\n",
            "GMount regression: remote content differs")
    info = run([GIO, "info", uri]).stdout
    # Sample input: local path: /fixture/runtime/gvfs/sftp:host=127.0.0.1,port=2222,user=gm/share
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


# Sample stderr: gio: archive://file%253A%252F%252F%252Ffixture.zip/: Location is not mountable
def unsupported(result, uri):
    messages = ("Location is not mountable", "volume doesn't implement mount")
    return result.returncode != 0 and any(f"gio: {uri}: {message}" in result.stderr
                                         for message in messages)


def archive(root):
    fixture = REPO / "tools/flea-gvfs-fixture"
    environment = dict(os.environ, DIR=str(root / "archive"))
    uri = run([str(fixture), "make"], environment=environment).stdout.strip()
    with zipfile.ZipFile(root / "archive/share.zip") as package:
        require(package.testzip() is None and sorted(package.namelist()) == ["alpha.txt", "beta.txt"],
                "archive integrity control failed")
        require(package.read("alpha.txt") == b"alpha\n" and package.read("beta.txt") == b"beta\n",
                "archive contents control failed")
    descriptor = any(Path(prefix, "share/gvfs/mounts/archive.mount").exists()
                     for prefix in ("/usr", "/usr/local"))
    daemon = any(Path(prefix, directory, "gvfsd-archive").exists()
                 for prefix in ("/usr", "/usr/local") for directory in ("lib", "libexec", "lib/gvfs"))
    result = run([str(fixture), "mount"], check=False, environment=environment)
    if result.returncode == 0:
        try:
            boundaries(uri, uri, lambda: run([str(fixture), "unmount"], environment=environment))
        finally:
            run([GIO, "mount", "-u", uri], check=False)
        print("gvfs: archive=supported integrity=ok mount=ok list=ok read=ok fuse=ok unmount=ok", flush=True)
        return True
    require(not descriptor and not daemon and unsupported(result, uri),
            f"archive regression: {result.stderr.strip()}")
    missing = "archive://file%253A%252F%252F%252Fctl2-missing-archive.zip/"
    require(unsupported(run([GIO, "mount", missing], check=False), missing),
            "archive absence control did not refuse a nonexistent archive")
    unknown = "flea-unsupported:///"
    require(unsupported(run([GIO, "mount", unknown], check=False), unknown),
            "unknown-backend refusal control failed")
    require(uri not in mount_lines(), "unsupported archive unexpectedly registered a mount")
    print("gvfs: archive=unsupported-expected integrity=ok refusal=ok absent-backend=confirmed", flush=True)
    return False


def sftp(root):
    account = pwd.getpwuid(os.getuid())
    share = root / "share"
    share.mkdir()
    for name in ("alpha", "beta"):
        (share / f"{name}.txt").write_text(name + "\n")
    for name in ("host", "client"):
        run(["/usr/bin/ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(root / name)])
        require((root / name).stat().st_uid == account.pw_uid
                and stat.S_IMODE((root / name).stat().st_mode) == 0o600, "unsafe ephemeral key ownership")
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    known = root / "known_hosts"
    known.write_text(f"[127.0.0.1]:{port} " + (root / "host.pub").read_text())
    ssh = Path.home() / ".ssh"
    ssh.mkdir(mode=0o700)
    client = ssh / "config"
    client.write_text(f"Host 127.0.0.1\n    User {account.pw_name}\n    Port {port}\n"
                      f"    IdentityFile {root / 'client'}\n    IdentityAgent none\n    IdentitiesOnly yes\n"
                      "    BatchMode yes\n    PasswordAuthentication no\n    KbdInteractiveAuthentication no\n"
                      f"    UserKnownHostsFile {known}\n    GlobalKnownHostsFile /dev/null\n"
                      "    StrictHostKeyChecking yes\n    ForwardAgent no\n    ControlMaster no\n    ControlPath none\n"
                      "    ProxyCommand none\n    ProxyJump none\n    PermitLocalCommand no\n    ClearAllForwardings yes\n")
    client.chmod(0o600)
    config = root / "sshd_config"
    config.write_text(f"ListenAddress 127.0.0.1\nPort {port}\nHostKey {root / 'host'}\n"
                      f"PidFile {root / 'sshd.pid'}\nAuthorizedKeysFile {root / 'client.pub'}\n"
                      "StrictModes yes\nUsePAM no\nPubkeyAuthentication yes\nPasswordAuthentication no\n"
                      f"KbdInteractiveAuthentication no\nPermitRootLogin no\nAllowUsers {account.pw_name}\n"
                      "Subsystem sftp internal-sftp\nLogLevel INFO\n")
    run(["/usr/bin/sshd", "-t", "-f", str(config)])
    server = subprocess.Popen(["/usr/bin/sshd", "-D", "-e", "-f", str(config)], stderr=subprocess.PIPE)
    mount_uri = f"sftp://{account.pw_name}@127.0.0.1:{port}/"
    try:
        with selectors.DefaultSelector() as ready:
            ready.register(server.stderr, selectors.EVENT_READ)
            require(ready.select(timeout=5), "SFTP server readiness timed out")
        # Sample input: Server listening on 127.0.0.1 port 2222.
        line = server.stderr.readline().decode()
        require(f"Server listening on 127.0.0.1 port {port}" in line, f"SFTP server failed: {line.strip()}")
        correct_host = known.read_text()
        known.write_text(f"[127.0.0.1]:{port} " + (root / "client.pub").read_text())
        rejected = run([str(root / "bin/ssh"), "127.0.0.1", "printf sftp-positive"], check=False)
        require(rejected.returncode != 0 and "Host key verification failed" in rejected.stderr,
                "strict host-key negative control unexpectedly succeeded")
        print(rejected.stderr, file=sys.stderr, end="")
        known.write_text(correct_host)
        print("gvfs: strict-host-key-refusal=ok", flush=True)
        require(run([str(root / "bin/ssh"), "127.0.0.1", "printf sftp-positive"]).stdout
                == "sftp-positive", "SFTP authentication positive control failed")
        run([GIO, "mount", mount_uri])
        boundaries(mount_uri + str(share).lstrip("/") + "/", mount_uri,
                   lambda: run([GIO, "mount", "-u", mount_uri]))
        print("gvfs: gmount=sftp auth=ok mount=ok list=ok read=ok fuse=ok missing-refusal=ok unmount=ok", flush=True)
    finally:
        try:
            run([GIO, "mount", "-u", mount_uri], check=False)
        finally:
            stop(server)
            print(server.stderr.read().decode(), file=sys.stderr, end="")


def main():
    if len(sys.argv) == 2:
        return isolated(Path(sys.argv[1]))
    require(len(sys.argv) == 3 and sys.argv[1] == "--inside", "invalid fixture arguments")
    root = Path(sys.argv[2])
    run(["/usr/bin/gdbus", "call", "--session", "--dest", "org.freedesktop.DBus", "--object-path",
         "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.ListNames"])
    try:
        if not archive(root):
            sftp(root)
    finally:
        fuse = Path(os.environ["XDG_RUNTIME_DIR"]) / "gvfs"
        if os.path.ismount(fuse):
            run(["/usr/bin/fusermount3", "-u", str(fuse)])
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.TimeoutExpired, zipfile.BadZipFile) as error:
        print(f"gvfs: FAIL {error}", file=sys.stderr)
        sys.exit(1)

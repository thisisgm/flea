"""Owned-window control in the operator's serialized Hyprland benchmark session."""

import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import time

NATIVE_RUNTIME = Path("/run/user/1000")
TOOLS = ("bwrap", "hyprctl", "grim", "magick", "perf", "dbus-run-session", "wtype")
ADDRESS = re.compile(r"0x[0-9a-fA-F]+")
# Only these modifiers: wtype with the logo key reads as a Super tap and opens the Omarchy menu.
WTYPE_MODIFIERS = {"ctrl": "ctrl", "alt": "alt", "shift": "shift"}


def descends_from(pid, root):
    """True when pid is root or one of its descendants, read from the host /proc parent links."""
    for _ in range(64):
        if pid == root:
            return True
        if pid <= 1:
            return False
        try:
            # Sample input: '123 (flea worker) S 100'; fields[1] is the host parent PID.
            fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        except (OSError, IndexError):
            return False
        pid = int(fields[1])
    return False


def session_environment():
    names = ("XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE")
    result = {name: os.environ[name] for name in names if os.environ.get(name)}
    if len(result) != len(names):
        output = subprocess.check_output(["omarchy-drive", "env"], text=True, timeout=10)
        for line in output.splitlines():
            match = re.fullmatch(r"export ([A-Z_]+)=([A-Za-z0-9_./-]+)", line)
            if not match:
                raise ValueError("unexpected omarchy-drive session environment format")
            if match[1] in names:
                result[match[1]] = match[2]
    if set(result) != set(names) or Path(result["XDG_RUNTIME_DIR"]) != NATIVE_RUNTIME:
        raise ValueError("benchmark needs the /run/user/1000 Hyprland session")
    for name in ("WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE"):
        if not re.fullmatch(r"[A-Za-z0-9_.-]+", result[name]):
            raise ValueError(f"invalid session field: {name}")
    return {**os.environ, **result}


class Display:
    def __init__(self, root):
        self.root = root
        self.lock = None
        self.env = None
        self.roots = {}

    def command(self, arguments):
        result = subprocess.run(arguments, env=self.env, cwd=self.root, capture_output=True, text=True, timeout=10)
        with (self.root / "controls.jsonl").open("a") as stream:
            stream.write(json.dumps(dict(argv=arguments, rc=result.returncode, out=result.stdout.strip()[:300], err=result.stderr.strip()[:300])) + "\n")
        result.check_returncode()
        return result.stdout.strip()

    def dispatch(self, expression):
        """One Hyprland 0.56 Lua dispatch. A miss prints a warning and still exits 0, so only the exact answer ok counts."""
        answer = self.command(["hyprctl", "dispatch", expression])
        if answer != "ok":
            raise ValueError(f"Hyprland did not accept {expression}: {answer!r}")

    def clients(self):
        clients = json.loads(self.command(["hyprctl", "-j", "clients"]))
        if not isinstance(clients, list):
            raise ValueError("hyprctl clients did not return a list")
        return clients

    def __enter__(self):
        for program in TOOLS:
            if not shutil.which(program):
                raise ValueError(f"missing executable: {program}")
        self.env = session_environment()
        self.socket = NATIVE_RUNTIME / self.env["WAYLAND_DISPLAY"]
        if not self.socket.is_socket():
            raise ValueError("the selected Wayland socket is absent")
        descriptor = os.open(NATIVE_RUNTIME / "flea-display.lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        self.lock = os.fdopen(descriptor, "r+")
        try:
            info = os.fstat(descriptor)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid():
                raise ValueError("display lock is not an owned regular file")
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.previous = json.loads(self.command(["hyprctl", "-j", "activewindow"]))
            self.clients()
            return self
        except BaseException:
            self.lock.close()
            self.lock = None
            raise

    def __exit__(self, *_):
        try:
            previous = self.previous.get("address", "")
            if ADDRESS.fullmatch(previous) and any(c.get("address") == previous and c.get("pid") == self.previous.get("pid") for c in self.clients()):
                self.dispatch(f'hl.dsp.focus({{ window = "address:{previous}" }})')
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            (self.root / "focus-restore-error.txt").write_text(str(error) + "\n")
        finally:
            self.lock.close()

    def owned(self, client, token):
        pid = client.get("pid")
        if type(pid) is not int or pid <= 0 or client.get("xwayland") is not False or not ADDRESS.fullmatch(client.get("address", "")):
            return False
        root = self.roots.get(token)
        if root is not None and not descends_from(pid, root):
            return False
        try:
            return ("FLEA_B7_BENCH=" + token).encode() in Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
        except OSError:
            return False

    def observed(self, token):
        """The compositor's client list as one line, with each client's ownership verdict, for a failed sample."""
        try:
            rows = [f"{c.get('address')}/pid={c.get('pid')}/class={c.get('class')}/owned={self.owned(c, token)}" for c in self.clients()[:12]]
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            return f"client list unreadable: {error}"
        return "observed clients: " + (", ".join(rows) if rows else "none")

    def wait_window(self, token, process, timeout):
        self.roots[token] = process.pid
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline and process.poll() is None:
            clients = [client for client in self.clients() if self.owned(client, token)]
            if len(clients) == 1:
                return clients[0]
            if len(clients) > 1:
                raise ValueError("entrant mapped more than one window before input; " + self.observed(token))
            time.sleep(0.05)
        raise ValueError(f"no owned entrant window mapped (launcher exit {process.poll()}); " + self.observed(token))

    def listed(self, window, token):
        return any(c.get("address") == window.get("address") and c.get("pid") == window.get("pid") and self.owned(c, token) for c in self.clients())

    def verified(self, window, token):
        """The window's address, only when the compositor still lists it as this sample's own native client."""
        matches = [c for c in self.clients() if c.get("address") == window.get("address") and c.get("pid") == window.get("pid") and self.owned(c, token)]
        if len(matches) != 1:
            raise ValueError("owned window is not listed by the compositor; " + self.observed(token))
        return matches[0]["address"]

    def focus(self, window, token):
        address = self.verified(window, token)
        self.dispatch(f'hl.dsp.focus({{ window = "address:{address}" }})')
        self.require_focus(window, token, wait=2.0)

    def close(self, window, token):
        """The compositor's close request for the verified owned window: the entrant's own close handler runs, as for its close button."""
        address = self.verified(window, token)
        self.dispatch(f'hl.dsp.window.close({{ window = "address:{address}" }})')

    def require_focus(self, window, token, wait=0.0):
        deadline = time.monotonic() + wait
        while True:
            active = json.loads(self.command(["hyprctl", "-j", "activewindow"]))
            if active.get("address") == window["address"] and active.get("pid") == window.get("pid") and self.owned(active, token):
                return
            if time.monotonic() >= deadline:
                raise ValueError(f"focus is not on the owned window (active {active.get('address')}/pid={active.get('pid')}); refusing keyboard input")
            time.sleep(0.05)

    def key(self, window, token, *keys):
        """Each chord goes to the focused window through wtype, because send_shortcut cannot resolve letters or digits on this box.

        The owned window must be the active one immediately before and after each chord; a change between the two
        fails the sample, since the chord may then have reached another window.
        """
        modifiers = WTYPE_MODIFIERS
        for chord in keys:
            parts = chord.split("+")
            if not re.fullmatch(r"[A-Za-z0-9_]+", parts[-1]) or parts[-1] in modifiers or any(m not in modifiers for m in parts[:-1]):
                raise ValueError("unsupported benchmark keyboard chord")
            argv = ["wtype"]
            for name in parts[:-1]:
                argv += ["-M", modifiers[name]]
            argv += ["-k", parts[-1]]
            for name in reversed(parts[:-1]):
                argv += ["-m", modifiers[name]]
            self.require_focus(window, token)
            self.command(argv)
            try:
                self.require_focus(window, token)
            except ValueError as error:
                if self.listed(window, token):
                    raise ValueError(f"focus moved while {chord} was delivered; the chord may have reached another window: {error}") from None
                # The owned window itself is gone, which is what a quit chord is for.

    def screenshot(self, root, window, token, label):
        self.require_focus(window, token)
        current = next((c for c in self.clients() if c.get("address") == window["address"] and self.owned(c, token)), None)
        if not current:
            raise ValueError("owned window disappeared before capture")
        x, y = current["at"]
        width, height = current["size"]
        if any(type(v) is not int for v in (x, y, width, height)) or width <= 0 or height <= 0:
            raise ValueError("invalid owned-window capture geometry")
        target = root / f"{label}.png"
        self.command(["grim", "-g", f"{x},{y} {width}x{height}", str(target)])
        with target.open("rb") as stream:
            if stream.read(8) != b"\x89PNG\r\n\x1a\n":
                raise ValueError("grim did not produce a PNG")

    def metadata(self):
        return dict(transport="Hyprland/Wayland", display_lock=str(NATIVE_RUNTIME / "flea-display.lock"),
                    wayland_display=self.env["WAYLAND_DISPLAY"], monitors=json.loads(self.command(["hyprctl", "-j", "monitors"])))

#!/usr/bin/env python3
"""Fail closed unless every discovered wallpaper is a still image or solid color."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import stat
import struct
import subprocess
import sys

# Omarchy v4.5.0-rc1, 03ff1eff3812933f7088484533784d2f63eaf19a, complete shell tree.
# Omarchy v4.5.0-rc3, 8d19ad75ace59af8b6ded52f8b2bec1b15adda7c, complete shell tree.
# Omarchy 4.5.0-3 (stable), complete shell tree; against rc1 its background changes touch only video wallpapers.
SYSTEM_BIN = Path("/usr/bin")
OMARCHY_SHELL = Path("/usr/share/omarchy/shell")
OMARCHY_TREES = {"87955e9cef088cb1c966e85c582e166e57ed19a5", "ba6c88d1b2db05cd4044d5b7398eb638785b9182",
                 "2d21146caa73cc035bbd26769e14f95eaf1df63e"}


def read_json(raw):
    def members(pairs):
        result = {}
        for key, value in pairs:
            if key in result: raise ValueError("duplicate JSON member: " + key)
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=members)


def file_identity(path):
    info = path.stat()
    if not stat.S_ISREG(info.st_mode): raise RuntimeError("not a regular wallpaper input: " + str(path))
    return [info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns]


def shell_tree(path, started_ns=None):
    """Hash the exact upstream tree, including executable modes, without running Git."""
    remaining = [1024, 8 * 1024 * 1024]
    def digest(directory):
        entries = []
        for child in sorted(directory.iterdir(), key=lambda p: os.fsencode(p.name) + (b"/" if p.is_dir() else b"")):
            remaining[0] -= 1
            if remaining[0] < 0: raise RuntimeError("Omarchy shell source exceeds verification bound")
            info = child.lstat()
            if started_ns is not None and info.st_ctime_ns > started_ns:
                raise RuntimeError("Omarchy shell source changed since renderer start: " + str(child))
            if stat.S_ISDIR(info.st_mode):
                mode, value = b"40000", digest(child)
            elif stat.S_ISREG(info.st_mode):
                remaining[1] -= info.st_size
                if remaining[1] < 0: raise RuntimeError("Omarchy shell source exceeds verification bound")
                fd = os.open(child, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
                with os.fdopen(fd, "rb") as source:
                    if not stat.S_ISREG(os.fstat(source.fileno()).st_mode): raise RuntimeError("nonregular shell source")
                    data = source.read(remaining[1] + info.st_size + 1)
                if len(data) != info.st_size: raise RuntimeError("Omarchy shell source changed during read")
                mode = b"100755" if info.st_mode & 0o111 else b"100644"
                value = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).digest()
            else:
                raise RuntimeError("unsupported Omarchy shell source: " + str(child))
            entries.append(mode + b" " + os.fsencode(child.name) + b"\0" + value)
        data = b"".join(entries)
        return hashlib.sha1(b"tree " + str(len(data)).encode() + b"\0" + data).digest()
    return digest(path).hex()


def process_identity(proc, executable):
    if proc.stat().st_uid != os.getuid(): raise RuntimeError("foreign wallpaper process owner")
    if (proc / "exe").resolve(strict=True) != (SYSTEM_BIN / executable).resolve(strict=True):
        raise RuntimeError("forged wallpaper executable: " + str(proc))
    fields = (proc / "stat").read_text().rsplit(")", 1)[1].split()
    if len(fields) < 20 or not fields[19].isdigit(): raise RuntimeError("invalid wallpaper process stat")
    environment = {}
    for word in (proc / "environ").read_bytes().decode().split("\0"):
        if "=" not in word: continue
        key, value = word.split("=", 1)
        if key in environment: raise RuntimeError("duplicate wallpaper process environment key")
        environment[key] = value
    return dict(pid=int(proc.name), start=fields[19], executable=str((proc / "exe").resolve()), environment=environment)


def omarchy_shell(words):
    paths = [words[i+1] for i, word in enumerate(words[:-1]) if word in ("-p", "--path")]
    paths += [word.split("=", 1)[1] for word in words if word.startswith("--path=")]
    return paths == [str(OMARCHY_SHELL)]


# A bar widget or a menu draws in the bar or a popup, never on the background layer; any other kind is unverified.
PLUGIN_KINDS = {"bar-widget", "menu"}
MANIFEST_MAX_BYTES = 65536


def user_plugins(root):
    """Sample manifest.json: {"id": "io.github.thisisgm.slack", "kinds": ["bar-widget"], ...}."""
    admitted = {}
    if not root.exists(): return admitted
    for entry in sorted(root.iterdir()):
        manifest = entry / "manifest.json"
        try: raw = manifest.read_bytes()
        except OSError: raise RuntimeError("unverified Omarchy shell user plugins")
        if len(raw) > MANIFEST_MAX_BYTES: raise RuntimeError("unverified Omarchy shell user plugins")
        data = read_json(raw)
        kinds = data.get("kinds") if isinstance(data, dict) else None
        if (not isinstance(kinds, list) or not kinds or not all(isinstance(kind, str) for kind in kinds)
                or not set(kinds) <= PLUGIN_KINDS or not isinstance(data.get("id"), str)):
            raise RuntimeError("unverified Omarchy shell user plugins")
        admitted[data["id"]] = dict(kinds=sorted(kinds), sha256=hashlib.sha256(raw).hexdigest())
    return admitted


def shell_identity(proc, home):
    identity = process_identity(proc, "quickshell")
    environment = identity.pop("environment")
    if environment.get("HOME") != str(home) or Path(environment.get("OMARCHY_PATH", "/nonexistent")).resolve(strict=True) != OMARCHY_SHELL.parent.resolve(strict=True):
        raise RuntimeError("unverified Omarchy shell HOME or OMARCHY_PATH")
    if any(environment.get(key) for key in ("QML_IMPORT_PATH", "QML2_IMPORT_PATH", "QT_QUICK_CONTROLS_CONF")):
        raise RuntimeError("unverified Omarchy shell import override")
    boot = [line.split()[1] for line in (proc.parent / "stat").read_text().splitlines() if line.startswith("btime ")]
    if len(boot) != 1: raise RuntimeError("missing wallpaper process boot time")
    started_ns = int(boot[0]) * 1_000_000_000 + int(identity["start"]) * 1_000_000_000 // os.sysconf("SC_CLK_TCK")
    tree = shell_tree(OMARCHY_SHELL, started_ns)
    if tree not in OMARCHY_TREES: raise RuntimeError("unverified Omarchy shell implementation: " + tree)
    plugins = user_plugins(home / ".config/omarchy/plugins")
    configs = {}
    for path in (OMARCHY_SHELL.parent / "config/omarchy/shell.json", home / ".config/omarchy/shell.json"):
        if path.exists() or path.is_symlink():
            before = file_identity(path)
            fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
            with os.fdopen(fd, "rb") as source:
                if not stat.S_ISREG(os.fstat(source.fileno()).st_mode): raise RuntimeError("nonregular shell configuration")
                raw = source.read(262145)
            if len(raw) > 262144: raise RuntimeError("Omarchy shell configuration exceeds verification bound")
            config = read_json(raw)
            if not isinstance(config, dict) or type(config.get("version")) is not int or config["version"] != 1:
                raise RuntimeError("unverified Omarchy shell configuration: " + str(path))
            disabled = config.get("disabledPlugins", [])
            if not isinstance(disabled, list) or not all(isinstance(value, str) for value in disabled):
                raise RuntimeError("invalid Omarchy disabledPlugins configuration")
            if "omarchy.background" in disabled:
                raise RuntimeError("Omarchy shell background plugin is disabled")
            configs[str(path)] = dict(identity=before, sha256=hashlib.sha256(raw).hexdigest())
            if before != file_identity(path): raise RuntimeError("Omarchy shell configuration changed")
    if not configs: raise RuntimeError("missing Omarchy shell configuration")
    identity.update(tree=tree, configs=configs, plugins=plugins)
    return identity


def owed_status(proc):
    """owe 0.2.12 status is read-only; authenticate the reply to this daemon PID."""
    identity = process_identity(proc, "owed")
    environment = identity.pop("environment")
    words = (proc / "cmdline").read_bytes().decode().strip("\0").split("\0")
    paths = [words[i+1] for i, word in enumerate(words[:-1]) if word == "--socket"]
    paths += [word.split("=", 1)[1] for word in words if word.startswith("--socket=")]
    if len(paths) > 1: raise RuntimeError("ambiguous owed socket")
    path = paths[0] if paths else str(Path(environment.get("XDG_RUNTIME_DIR", "/nonexistent")) / "owe/owed.sock")
    if not Path(path).is_absolute(): raise RuntimeError("relative owed socket")
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as peer:
        peer.settimeout(5)
        peer.connect(path)
        pid, uid, _ = struct.unpack("3i", peer.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if pid != int(proc.name) or uid != os.getuid(): raise RuntimeError("foreign owed status peer")
        peer.sendall(b'{"cmd":"status"}\n')
        raw = b""
        while b"\n" not in raw and len(raw) <= 16384:
            chunk = peer.recv(4096)
            if not chunk: break
            raw += chunk
    if not raw.endswith(b"\n") or len(raw) > 16384: raise RuntimeError("incomplete owed status")
    answer = read_json(raw)
    if not isinstance(answer, dict): raise RuntimeError("invalid owed status")
    if answer.get("intro") is not False: raise RuntimeError("owed intro is active or unverified")
    if (answer.get("status") != "ok" or answer.get("engine") != "shell" or answer.get("source_kind") != "still"
            or answer.get("job_running") is not False):
        raise RuntimeError("owed background is not a settled shell still")
    source = answer.get("source_path")
    if not isinstance(source, str) or not Path(source).is_absolute(): raise RuntimeError("missing owed source path")
    identity.update(source=source, intro=False, engine="shell")
    return identity


def discover(proc_root=Path("/proc"), home=Path.home()):
    images = set()
    renderers = []
    renderer_pids = set()
    shell_pids = set()
    identities = []
    daemons = []
    for proc in proc_root.glob('[0-9]*'):
        try:
            if proc.stat().st_uid != os.getuid(): continue
            name = (proc / 'comm').read_text().strip()
            if re.search(r'mpvpaper|wallpaperengine|linux-wallpaper', name):
                raise RuntimeError('video/animated wallpaper process: ' + name)
            if name == 'swaybg':
                words = (proc / 'cmdline').read_bytes().decode().strip('\0').split('\0')
                renderers.append(name)
                renderer_pids.add(int(proc.name))
                found = False
                for i, word in enumerate(words):
                    if word in ('-i', '--image') and i + 1 < len(words): images.add(words[i+1]); found = True
                    elif word.startswith('--image='): images.add(word.split('=',1)[1]); found = True
                if not found and not any(w in ('-c','--color') or w.startswith('--color=') for w in words):
                    raise RuntimeError('swaybg wallpaper source is unreadable')
            if name == 'owed': daemons.append(owed_status(proc))
            if name == 'quickshell':
                words = (proc / 'cmdline').read_bytes().decode().strip('\0').split('\0')
                if omarchy_shell(words):
                    identities.append(shell_identity(proc, home))
                    shell_pids.add(int(proc.name))
                    # The same read-only query omarchy-theme-bg-current makes; every Omarchy setter repoints this link.
                    link = home / '.local/state/omarchy/current/background'
                    try: images.add(str(link.resolve(strict=True)))
                    except OSError: raise RuntimeError('omarchy-shell background link is unreadable: ' + str(link))
                    renderers.append('omarchy-shell')
                    renderer_pids.add(int(proc.name))
            if name in ('awww-daemon','swww-daemon'):
                client = name.split('-',1)[0]
                if not shutil.which(client): raise RuntimeError('missing wallpaper query client: ' + client)
                answer = subprocess.check_output([client,'query'],text=True,timeout=5)
                values = re.findall(r'\bimage:\s*(.+)',answer)
                if not values: raise RuntimeError('wallpaper query returned no image paths: ' + client)
                images.update(value.strip() for value in values)
                renderers.append(name)
                renderer_pids.add(int(proc.name))
        except (FileNotFoundError, ProcessLookupError):
            continue
    layers = read_json(subprocess.check_output(['hyprctl','-j','layers'],text=True,timeout=5))
    if not isinstance(layers, dict) or not layers: raise RuntimeError("missing compositor layer observation")
    active = set()
    relevant_layers = []
    for monitor in layers.values():
        if not isinstance(monitor, dict) or not isinstance(monitor.get("levels"), dict):
            raise RuntimeError("invalid compositor layer observation")
        for level, entries in monitor["levels"].items():
            if not isinstance(entries, list): raise RuntimeError("invalid compositor layer list")
            for layer in entries:
                if not isinstance(layer, dict) or type(layer.get("pid")) is not int:
                    raise RuntimeError("invalid compositor layer identity")
                namespace = layer.get("namespace")
                if namespace == "owe-render": raise RuntimeError("active owe-render layer")
                if namespace == "omarchy-background" and level != "0":
                    raise RuntimeError("active Omarchy intro or transition cover")
                if level != "0": continue
                pid = layer["pid"]
                if pid not in renderer_pids or (pid in shell_pids and namespace != "omarchy-background"):
                    raise RuntimeError("unsupported or unidentified active background layer: " + str(layer))
                active.add(pid)
                relevant_layers.append([level, layer])
    if not active or not shell_pids.issubset(active): raise RuntimeError("no identified active wallpaper layer")
    for daemon in daemons:
        if daemon["source"] not in images: raise RuntimeError("owed source disagrees with wallpaper link")
    sources = {str(Path(value).resolve(strict=True)): file_identity(Path(value).resolve(strict=True)) for value in images}
    return images, renderers, dict(processes=identities, daemons=daemons, sources=sources, layers=relevant_layers,
                                  link=home.joinpath(".local/state/omarchy/current/background").lstat().st_ctime_ns if shell_pids else None)

def inspect(proc_root=Path("/proc"), home=Path.home()):
    images, renderers, before = discover(proc_root, home)
    if not renderers: raise RuntimeError('no supported active wallpaper renderer; inspect the session and add its exact read-only source query')
    checked=[]
    for value in sorted(images):
        path=Path(value).resolve(strict=True)
        mime=subprocess.check_output(['file','--brief','--mime-type','--',str(path)],text=True,timeout=5).strip()
        if not mime.startswith('image/'): raise RuntimeError('wallpaper is not a still image: ' + str(path) + ' (' + mime + ')')
        frames=subprocess.check_output(['magick','identify','-ping','-format','%n\n',str(path)],text=True,timeout=10).split()
        if not frames or any(int(n)!=1 for n in frames): raise RuntimeError('animated wallpaper: ' + str(path))
        checked.append(dict(path=str(path),mime=mime,frames=1))
    _, _, after = discover(proc_root, home)
    if before != after: raise RuntimeError("wallpaper source, process or configuration changed during inspection")
    return dict(still=True,renderers=renderers,images=checked,identity=before)

def main(proc_root=Path("/proc"), home=Path.home()):
    try:
        print(json.dumps(inspect(proc_root, home)))
        return 0
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print("REFUSED: still-wallpaper preflight: " + str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())

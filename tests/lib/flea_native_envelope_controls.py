"""Still-wallpaper and diagnostic guards reject deliberately failing twins."""
from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
from unittest.mock import patch

import flea_native_wallpaper as wallpaper
from flea_native_log import check, unexpected


# A retained-input miniature, not an assertion that the live shell matches upstream.
FIXTURE_QML = b'import QtQuick\nImage { source: "wallpaper.png" }\n'
FIXTURE_TREE = "9f80af0ebbaf1c95d9ced3457bdcf0bb3a58edd8"


def check_envelope(parent):
    assert parent.is_absolute() and (parent / ".flea-test-sandbox").is_file()
    root = parent / "native-envelope"
    root.mkdir()
    (root / ".flea-test-sandbox").touch()
    proc = root / "proc"
    process = proc / "101"
    process.mkdir(parents=True)
    (proc / "stat").write_text("btime 0\n")
    start_ticks = str(int((time.time() + 60) * os.sysconf("SC_CLK_TCK")))
    home = root / "home"
    link = home / ".local/state/omarchy/current/background"
    link.parent.mkdir(parents=True)
    image = root / "wallpaper.png"
    image.write_bytes(b"fixture image")
    second_image = root / "second.png"
    second_image.write_bytes(b"different fixture image")
    link.symlink_to(image)
    shell = root / "omarchy/shell"
    shell.mkdir(parents=True)
    source = shell / "Background.qml"
    source.write_bytes(FIXTURE_QML)
    config = shell.parent / "config/omarchy/shell.json"
    config.parent.mkdir(parents=True)
    config.write_text('{"version":1,"plugins":[]}\n')
    binary = root / "bin"
    binary.mkdir()
    for name in ("quickshell", "owed", "foreign"):
        (binary / name).touch()
    (process / "exe").symlink_to(binary / "quickshell")
    (process / "stat").write_text("101 (quickshell) " + " ".join(["S"] + ["0"] * 18 + [start_ticks]) + "\n")
    (process / "environ").write_bytes(f"HOME={home}\0OMARCHY_PATH={shell.parent}\0".encode())
    state = dict(mime="image/png", frames="1\n", layers={"monitor": {"levels": {"0": [{"pid": 101}]}}})
    counts = dict(passing=0, refused=0)

    def output(command, **_):
        if command[0] == "hyprctl": return state.get("raw_layers", json.dumps(state["layers"]))
        if command[0] == "file": return state["mime"]
        if command[:2] == ["magick", "identify"]:
            mutation = state.pop("mutate", None)
            if mutation: mutation()
            return state["frames"]
        raise AssertionError(command)

    def layer(namespace="omarchy-background", pid=101, level="0"):
        state["layers"] = {"monitor": {"levels": {level: [{"pid": pid, "namespace": namespace}]}}}

    def replace_link(target):
        assert link.is_absolute() and str(link) and link.is_relative_to(root)
        assert (root / ".flea-test-sandbox").is_file()
        link.unlink()
        link.symlink_to(target)

    def verify(name, refused=False, diagnostic=""):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            status = wallpaper.main(proc, home)
        assert status == (2 if refused else 0), (name, status, out.getvalue(), err.getvalue())
        if refused:
            assert diagnostic in err.getvalue(), (name, err.getvalue())
            assert not out.getvalue(), (name, out.getvalue())
            counts["refused"] += 1
        else:
            answer = json.loads(out.getvalue())
            assert answer["still"] and answer["images"][0]["path"] == str(image), answer
            counts["passing"] += 1
        print(f"PASS native-admission {name} exit={status}")

    with patch.object(wallpaper, "OMARCHY_SHELL", shell), patch.object(wallpaper, "OMARCHY_TREES", {FIXTURE_TREE}), \
            patch.object(wallpaper, "SYSTEM_BIN", binary), patch.object(wallpaper.subprocess, "check_output", output):
        (process / "comm").write_text("swaybg\n")
        (process / "cmdline").write_bytes(b"swaybg\0-i\0" + str(image).encode() + b"\0")
        verify("swaybg-still")
        for name, key, value, diagnostic in (
                ("animated", "frames", "2\n2\n", "animated wallpaper"),
                ("video", "mime", "video/mp4", "not a still image"),
                ("bad-frame-parser", "frames", "invalid", "invalid literal"),
                ("zero-frames", "frames", "", "animated wallpaper"),
                ("bad-layer-json", "raw_layers", "{", "Expecting"),
                ("duplicate-layer-json", "raw_layers", '{"monitor":{},"monitor":{}}', "duplicate JSON")):
            original = state.get(key)
            state[key] = value
            verify(name, True, diagnostic)
            if original is None: del state[key]
            else: state[key] = original
        layer(pid=102)
        verify("missing-process", True, "unidentified active background layer")
        (process / "comm").write_text("mpvpaper\n")
        verify("animated-renderer", True, "video/animated")
        (process / "comm").write_text("quickshell\n")
        shell_words = ["quickshell", "-n", "-p", str(shell)]
        (process / "cmdline").write_bytes(("\0".join(shell_words) + "\0").encode())
        layer()
        verify("omarchy-still")
        state["mime"] = "video/mp4"
        verify("omarchy-video", True, "not a still image")
        state["mime"] = "image/png"
        state["frames"] = "2\n"
        verify("omarchy-animation", True, "animated wallpaper")
        state["frames"] = "1\n"
        for namespace in ("foreign-background", "owe-render"):
            layer(namespace)
            verify(namespace, True, "active owe-render" if namespace == "owe-render" else "unidentified")
        for level in ("1", "3"):
            layer(level=level)
            verify("intro-cover-" + level, True, "intro or transition cover")
        layer(pid=202)
        verify("forged-namespace", True, "unidentified")
        layer()
        original_exe = process / "exe"
        assert original_exe.is_absolute() and str(original_exe) and original_exe.is_relative_to(root)
        original_exe.unlink()
        original_exe.symlink_to(binary / "foreign")
        verify("forged-executable", True, "forged wallpaper executable")
        assert original_exe.is_absolute() and str(original_exe) and original_exe.is_relative_to(root)
        original_exe.unlink()
        original_exe.symlink_to(binary / "quickshell")
        (process / "cmdline").write_bytes(b"quickshell\0-p\0/home/gm/other/omarchy/shell\0")
        verify("foreign-shell-suffix", True, "unidentified")
        (process / "cmdline").write_bytes(("\0".join(shell_words) + "\0").encode())
        replace_link(root / "absent")
        verify("missing-background", True, "background link is unreadable")
        replace_link(image)
        source.write_bytes(FIXTURE_QML + b"// different renderer\n")
        verify("changed-implementation", True, "unverified Omarchy shell implementation")
        source.write_bytes(FIXTURE_QML)
        source.rename(shell / "missing.qml")
        verify("missing-implementation", True, "unverified Omarchy shell implementation")
        (shell / "missing.qml").rename(source)
        config.write_text("{")
        verify("config-parser-error", True, "Expecting")
        config.write_text('{"version":1,"disabledPlugins":["omarchy.background"]}')
        verify("disabled-background", True, "plugin is disabled")
        config.rename(config.with_suffix(".absent"))
        verify("missing-config", True, "missing Omarchy shell configuration")
        config.with_suffix(".absent").rename(config)
        config.write_text('{"version":1,"plugins":[]}\n')
        config.write_text('{"version":1,"disabledPlugins":null}')
        verify("invalid-config-type", True, "invalid Omarchy disabledPlugins")
        config.write_text('{"version":1,"plugins":[]}\n')
        (process / "stat").write_text("101 (quickshell) " + " ".join(["S"] + ["0"] * 18 + ["1"]) + "\n")
        verify("source-newer-than-renderer", True, "source changed since renderer start")
        (process / "stat").write_text("101 (quickshell) " + " ".join(["S"] + ["0"] * 18 + [start_ticks]) + "\n")
        for name, action in (
                ("link-change-during-inspect", lambda: replace_link(second_image)),
                ("same-path-image-change", lambda: image.write_bytes(b"changed image")),
                ("config-change-during-inspect", lambda: config.write_text('{"version":1,"plugins":[],"bar":{}}')),
                ("process-reuse", lambda: (process / "stat").write_text("101 (quickshell) " + " ".join(["S"] + ["0"] * 18 + [str(int(start_ticks) + 10)]) + "\n"))):
            state["mutate"] = action
            verify(name, True, "changed during inspection")
            replace_link(image)
        config.write_text('{"version":1,"plugins":[]}\n')
        state["layers"] = {}
        verify("no-layers", True, "missing compositor layer")
        state["layers"] = {"monitor": {"levels": {"0": {}}}}
        verify("layer-parser-type", True, "invalid compositor layer list")
        layer()
        plugins = home / ".config/omarchy/plugins/custom"
        plugins.mkdir(parents=True)
        verify("unverified-user-plugin", True, "unverified Omarchy shell user plugins")
        plugins.rename(home / ".config/omarchy/retained-plugin")
        daemon = proc / "303"
        daemon.mkdir()
        (daemon / "comm").write_text("owed\n")
        settled = dict(pid=303, start="321", executable=str(binary / "owed"), source=str(image), intro=False, engine="shell")
        with patch.object(wallpaper, "owed_status", return_value=settled):
            verify("owed-settled-still")
            layer("owe-render", 404)
            verify("owed-video-layer", True, "active owe-render")
            layer()
        with patch.object(wallpaper, "owed_status", side_effect=[settled, RuntimeError("owed intro is active or unverified")]):
            verify("intro-starts-during-inspection", True, "owed intro is active or unverified")
        for reason in ("owed intro is active or unverified", "invalid owed status"):
            with patch.object(wallpaper, "owed_status", side_effect=RuntimeError(reason)):
                verify(reason, True, reason)
        (daemon / "comm").write_text("sleep\n")
        # Exercise the production entry point in a fresh process with only external observations doubled.
        runner = root / "cli.py"
        runner.write_text('''import json, sys
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0, sys.argv[1])
import flea_native_wallpaper as w
root = Path(sys.argv[2])
def output(command, **kwargs):
    if command[0] == "hyprctl": return json.dumps({"monitor":{"levels":{"0":[{"pid":101,"namespace":sys.argv[3]}]}}})
    if command[0] == "file": return "image/png"
    if command[0] == "magick": return "1\\n"
    raise AssertionError(command)
with patch.object(w, "SYSTEM_BIN", root / "bin"), patch.object(w, "OMARCHY_SHELL", root / "omarchy/shell"), patch.object(w, "OMARCHY_TREES", {sys.argv[4]}), patch.object(w.subprocess, "check_output", output):
    sys.exit(w.main(root / "proc", root / "home"))
''')
        for namespace, expected in (("omarchy-background", 0), ("forged-background", 2)):
            result = subprocess.run([sys.executable, "-B", str(runner), str(Path(__file__).parent), str(root), namespace, FIXTURE_TREE], capture_output=True, text=True, timeout=5)
            assert result.returncode == expected, (result.returncode, result.stdout, result.stderr)
            counts["refused" if expected else "passing"] += 1
            print(f"PASS native-admission cli-{namespace} exit={result.returncode}")
    check_owed_socket(root, binary, image, counts)
    assert unexpected("INFO window mapped\nWARN deliberate\n", ["WARN deliberate"]) == []
    assert check("flea-gui: WARN deliberate\n", ["WARN deliberate"]) == []
    assert check("WARN deliberate\nWARN deliberate\n", ["WARN deliberate"])
    assert check("INFO mapped\n", ["WARN deliberate"])
    assert check("ERROR backend panicked while handling WARN deliberate", ["WARN deliberate"])
    for line in ("WARN unexpected", "ERROR VAAPITextureConverter: No rhi", "flea-gui: backend failed",
                 "FLEA_IPC_NOT_PORTED flea.tokens", "thread panicked", "flea-gui: backend disconnected"):
        assert unexpected(line, ["WARN deliberate"]) == [line], line
    expected = root / "native-expected-log"
    log = root / "native-twin.log"
    expected.write_text("WARN deliberate\n")
    for content, status in (("flea-gui: WARN deliberate\n", 0),
                            ("ERROR backend panicked while handling WARN deliberate\n", 1)):
        log.write_text(content)
        result = subprocess.run([sys.executable, "-B", str(Path(__file__).with_name("flea_native_log.py")),
                                 str(expected), str(log)], capture_output=True, timeout=3)
        assert result.returncode == status, result
    print(f"TRANSPORT_ENVELOPE still=verified animation=refused video=refused foreign-layer=refused omarchy-shell=verified native-log=verified twin_exit={result.returncode}")
    print(f"PASS native-admission controls={sum(counts.values())} passing={counts['passing']} refused={counts['refused']} failed=0 capability=retained-inputs native=unproven")


def check_owed_socket(root, binary, image, counts):
    # /proc/self/fd keeps the Unix address short while the socket remains inside the marked root.
    directory = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    try:
        base = root / "socket-proc"
        process = base / str(os.getpid())
        process.mkdir(parents=True)
        (process / "exe").symlink_to(binary / "owed")
        (process / "stat").write_text(f"{os.getpid()} (owed) " + " ".join(["S"] + ["0"] * 18 + ["789"]) + "\n")
        (process / "environ").write_bytes(b"XDG_RUNTIME_DIR=/unused\0")
        settled = dict(status="ok", intro=False, engine="shell", source_kind="still", job_running=False, source_path=str(image))
        answers = [settled, dict(status="ok", intro=True), dict(status="ok"), dict(status="error", intro=False), "{", [],
                   dict(settled, intro="false"), dict(settled, engine="renderer"), dict(settled, source_kind="gif"),
                   dict(settled, job_running=True), dict(settled, source_path=""), settled]
        foreign = base / str(os.getpid() + 1)
        foreign.mkdir()
        (foreign / "exe").symlink_to(binary / "owed")
        (foreign / "stat").write_text(f"{foreign.name} (owed) " + " ".join(["S"] + ["0"] * 18 + ["789"]) + "\n")
        (foreign / "environ").write_bytes(b"XDG_RUNTIME_DIR=/unused\0")
        for index, answer in enumerate(answers):
            address = f"/proc/self/fd/{directory}/s{index}"
            selected = foreign if index == len(answers) - 1 else process
            (selected / "cmdline").write_bytes(f"owed\0--socket\0{address}\0".encode())
            requests = []
            errors = []
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
                server.bind(address)
                server.listen()
                server.settimeout(5)
                def reply():
                    try:
                        peer, _ = server.accept()
                        with peer:
                            peer.settimeout(5)
                            requests.append(peer.recv(1024))
                            if requests[-1]: peer.sendall(((answer if isinstance(answer, str) else json.dumps(answer)) + "\n").encode())
                    except Exception as error:
                        errors.append(error)
                thread = threading.Thread(target=reply)
                thread.start()
                try:
                    with patch.object(wallpaper, "SYSTEM_BIN", binary):
                        try:
                            observed = wallpaper.owed_status(selected)
                        except (RuntimeError, ValueError):
                            assert index > 0
                            counts["refused"] += 1
                        else:
                            assert index == 0 and observed["source"] == str(image), observed
                            counts["passing"] += 1
                finally:
                    thread.join(timeout=6)
                assert not thread.is_alive() and not errors, errors
                assert requests == ([b""] if selected == foreign else [b'{"cmd":"status"}\n']), requests
            print(f"PASS native-admission owed-socket-{index} exit={0 if index == 0 else 2} query=status peer={'foreign-refused' if selected == foreign else 'owned'}")
    finally:
        os.close(directory)


if __name__ == "__main__":
    scratch = Path(__file__).resolve().parents[2] / ".superpowers/m9-uish-24"
    scratch.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="controls.", dir=scratch))
    (root / ".flea-test-sandbox").touch()
    check_envelope(root)

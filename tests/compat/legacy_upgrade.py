#!/usr/bin/env python3
"""Drive the real launcher against private locked legacy processes."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import select
import sys
import time

import sandbox

repo = Path(__file__).resolve().parents[2]
binary = Path(sys.argv[1]).resolve()
root = sandbox.make("upgrade-")
for name in ("home", "config", "state", "data", "cache", "runtime", "bin", "files"):
    (root / name).mkdir(mode=0o700)
shutil.copy2(binary, root / "bin/flea")
binary = root / "bin/flea"
for name in ("qs", "hyprctl"):
    shutil.copy(repo / "tests/compat/legacy_fixture.py", root / "bin" / name)
    (root / "bin" / name).chmod(0o700)
(root / "bin/flea-gui").write_text('''#!/usr/bin/python3
import json, os
from pathlib import Path
import sys
runtime=Path(os.environ["XDG_RUNTIME_DIR"])
special=os.environ.get("FLEA_PICKER_REPLY") or os.environ.get("FLEA_UNLOCK") or "--pick" in sys.argv
assert special or all((p.parent/"drained").exists() for p in runtime.glob("quickshell/by-id/*/backend.pid")), "GUI started before old backends drained"
args=sys.argv[1:]
assert os.environ.get("FLEA_PATH") is None and os.environ.get("FLEA_SELECT") is None, "start or selection leaked through the environment"
state={"path":args[args.index("--")+1] if "--" in args else None,"select":args[args.index("--select")+1] if "--select" in args else None,"FLEA_UPGRADE_SELECTION":os.environ.get("FLEA_UPGRADE_SELECTION")}
root=Path(os.environ["UPGRADE_FIXTURE"]).resolve()
assert root.is_absolute() and (root/".flea-test-sandbox").is_file()
with (root/"gui-records.jsonl").open("a") as log: log.write(json.dumps(state)+"\\n")
# The GPUI chooser takes its reply file as flea-gui --pick <reply>.
reply=os.environ.get("FLEA_PICKER_REPLY") or (args[args.index("--pick")+1] if "--pick" in args else None)
if reply:
    target=Path(reply).resolve()
    assert target.is_absolute() and target.is_relative_to(root)
    target.write_text(json.dumps({"response":0,"uris":[(root/"files/picked.txt").as_uri()]}))
print(json.dumps(state))
''')
(root / "bin/flea-gui").chmod(0o700)
env = {"PATH": str(root / "bin") + ":/usr/bin:/bin", "HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "config"), "XDG_STATE_HOME": str(root / "state"), "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"), "XDG_RUNTIME_DIR": str(root / "runtime"), "FLEA_FRONTEND": "gpui", "FLEA_UI": str(repo / "ui"), "FLEA_BIN": str(binary), "WAYLAND_DISPLAY": "fixture", "QSG_RHI_BACKEND": "vulkan", "VK_LOADER_DRIVERS_DISABLE": "fixture", "UPGRADE_FIXTURE": str(root)}
processes = []
clients = []
(root / "clients.json").write_text("[]")


def start(identity, **state):
    (root / (identity + ".json")).write_text(json.dumps({"path": str(root / "files"), **state}))
    child = subprocess.Popen([str(root / "bin/qs"), "-p", str(repo / "ui/boot/shell.qml")], env=env | {"OLD_ID": identity}, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    processes.append(child)
    deadline = time.monotonic() + 5
    while not (root / "runtime/quickshell/by-id" / identity / "ready").exists():
        assert child.poll() is None and time.monotonic() < deadline, "fixture did not start"
        time.sleep(.01)
    clients.append({"pid": child.pid, "address": hex(0x1000 + len(clients))})
    (root / "clients.json").write_text(json.dumps(clients))
    return child


def launch(*args):
    begin = time.monotonic()
    result = subprocess.run([str(binary), "--gui", *args], env=env, capture_output=True, timeout=35)
    durations.append(time.monotonic() - begin)
    return result


# A chooser or unlock request is a separate window beside any 0.3.12 browser window, so it never hands over.
def specials_leave(*windows):
    for name, args, extra in (("pick", ["--pick", str(root / "reply.json")], {"FLEA_PICKER": '{"kind":"open","multiple":false}'}),
                              ("unlock", ["--unlock", "/dev/contract"], {})):
        result = subprocess.run([str(binary), *args], env=env | extra, capture_output=True, timeout=35)
        assert result.returncode == 0, (name, result.returncode, result.stderr)
        (root / "reply.json").unlink(missing_ok=True)
    time.sleep(.5)
    assert all(window.poll() is None for window in windows), "a chooser or unlock request closed an old window"


def bus_call(destination, path, method, *arguments):
    result = subprocess.run(["gdbus", "call", "--session", "--dest", destination, "--object-path", path, "--method", method, *arguments], env=env, capture_output=True, timeout=5)
    assert result.returncode == 0, (method, result.stderr)
    return result.stdout


def helpers():
    runtime_fd = os.open(root / "runtime", os.O_RDONLY | os.O_DIRECTORY)
    config = root / "bus.conf"
    config.write_text(f'<busconfig><type>session</type><listen>unix:path=/proc/{os.getpid()}/fd/{runtime_fd}/bus</listen><auth>EXTERNAL</auth><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
    bus = subprocess.Popen(["dbus-daemon", "--nofork", "--config-file=" + str(config), "--print-address=1"], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    processes.append(bus)
    assert select.select([bus.stdout], [], [], 5)[0], "private bus did not start"
    env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().decode().strip()
    owned = []
    for script, name in (("flea-filemanager1", "org.freedesktop.FileManager1"), ("flea-portal", "org.freedesktop.impl.portal.desktop.flea")):
        child = subprocess.Popen(["/usr/bin/python3", str(repo / "tools" / script)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        processes.append(child)
        owned.append(child)
        deadline = time.monotonic() + 5
        while b"true" not in bus_call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus.NameHasOwner", name):
            assert child.poll() is None, child.stderr.read().decode()
            assert time.monotonic() < deadline, script + " did not own its private bus name"
            time.sleep(.02)
    return owned, runtime_fd


def helper_requests():
    picked = root / "files/picked.txt"
    picked.write_text("picked through a surviving helper\n")
    bus_call("org.freedesktop.FileManager1", "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowFolders", str([(root / "files").as_uri()]), "")
    bus_call("org.freedesktop.FileManager1", "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowItems", str([picked.as_uri()]), "")
    answer = bus_call("org.freedesktop.impl.portal.desktop.flea", "/org/freedesktop/portal/desktop", "org.freedesktop.impl.portal.FileChooser.OpenFile", "/org/freedesktop/portal/desktop/request/contract/one", "contract", "", "Choose", "{}")
    assert b"uint32 0" in answer and picked.as_uri().encode() in answer, answer
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        rows = [json.loads(row) for row in (root / "gui-records.jsonl").read_text().splitlines()]
        if any(row["select"] == str(picked) for row in rows):
            return
        time.sleep(.01)
    raise AssertionError("surviving FileManager1 did not reveal its selected file")


durations = []
try:
    result = launch()
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["path"] is None
    if "--helpers" in sys.argv:
        old_helpers, runtime_fd = helpers()
        assert launch().returncode == 0 and all(helper.poll() is None for helper in old_helpers), "helper-only launch disturbed a service"
        helper_requests()
    else:
        helper = subprocess.Popen(["/usr/bin/sleep", "30"], env=env)
        processes.append(helper)
        old_helpers = [helper]
        assert launch().returncode == 0 and helper.poll() is None, "launch disturbed an unrelated process"
    old = start("a-busy", busy=True)
    result = launch()
    assert result.returncode == 2 and b"busy" in result.stderr and not result.stdout and old.poll() is None, (result.returncode, result.stdout, result.stderr)
    specials_leave(old)
    state_file = root / "a-busy.json"
    state_file.write_text(json.dumps({"path": str(root / "files"), "editor": True}))
    result = launch()
    assert result.returncode == 2 and b"busy" in result.stderr and old.poll() is None, (result.returncode, result.stdout, result.stderr)
    state_file.write_text(json.dumps({"path": str(root / "files"), "busy": True}))
    neighbour = start("b-idle")
    result = launch()
    assert result.returncode == 2 and old.poll() is None and neighbour.poll() is None, "preflight partially closed old windows"
    state_file = root / "a-busy.json"
    state_file.write_text(json.dumps({"path": str(root / "files"), "hung": True}))
    result = launch()
    assert result.returncode == 2 and b"750 ms" in result.stderr and durations[-1] < 2 and old.poll() is None and neighbour.poll() is None
    time.sleep(1.4)
    state_file.write_text(json.dumps({"path": str(root / "files")}))
    selected = str(root / "files/selected")
    (root / "b-idle.json").write_text(json.dumps({"path": str(root / "files"), "cursor": selected, "selection": [selected]}))
    specials_leave(old, neighbour)
    result = launch()
    assert result.returncode == 0, result.stderr
    carried = json.loads(result.stdout)
    assert carried["path"] == str(root / "files") and carried["select"] == selected
    assert json.loads(carried["FLEA_UPGRADE_SELECTION"]) == [selected]
    old.wait(timeout=2)
    neighbour.wait(timeout=2)
    if "--helpers" in sys.argv:
        helper_requests()
    assert all(helper.poll() is None for helper in old_helpers), "handoff replaced a surviving helper"
    # A session started before the Lua dispatch keeps the classic close, so an update alone cannot strand it.
    (root / "classic-compositor").touch()
    old = start("b-classic")
    result = launch()
    assert result.returncode == 0, result.stderr
    old.wait(timeout=2)
    (root / "classic-compositor").unlink()
    old = start("c-explicit", cursor=selected, selection=[selected])
    gui_binary = root / "bin/flea-gui"
    parked_binary = root / "bin/flea-gui.parked"
    gui_binary.rename(parked_binary)
    result = launch()
    assert result.returncode == 1 and old.poll() is None, "missing candidate closed the old window"
    parked_binary.rename(gui_binary)
    bad_ipc_state = root / "c-explicit.json"
    bad_ipc_state.write_text(json.dumps({"path": str(root / "files"), "ipcError": True}))
    result = launch()
    assert result.returncode == 2 and b"IPC command failed" in result.stderr and old.poll() is None, result.stderr
    bad_ipc_state.write_text(json.dumps({"path": str(root / "files"), "cursor": selected, "selection": [selected]}))
    result = launch(str(root / "elsewhere"))
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["path"] == str(root / "elsewhere")
    assert json.loads(result.stdout)["select"] is None
    old.wait(timeout=2)
    print(f"legacy-upgrade: launch duration {min(durations):.3f}..{max(durations):.3f} s")
    print("legacy-helpers: all cases passed" if "--helpers" in sys.argv else "legacy-upgrade: all cases passed")
finally:
    for child in reversed(processes):
        if child.poll() is None:
            child.terminate()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            # One stuck child must not leave the rest running.
            child.kill()
            child.wait()

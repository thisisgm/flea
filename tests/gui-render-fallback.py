"""Measure Mesa renderer fallbacks using real Flea binaries and Wayland buffers."""

import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import threading
import time

MESSAGE = "flea: no graphics driver could open a window; install or repair vulkan-swrast and mesa, or run flea --tui"
NOTICE = ["--app-name=Flea", "--urgency=critical", "--icon=com.thisisgm.flea", "No graphics driver could open Flea",
          "Install or repair vulkan-swrast and mesa, or run flea --tui in a terminal."]
# What an Intel Gen6 (Sandy Bridge) GPU's GL offers: OpenGL 3.3, OpenGL ES 3.0 and no storage buffers.
GEN6_GL = {"MESA_GL_VERSION_OVERRIDE": "3.3", "MESA_GLSL_VERSION_OVERRIDE": "330", "MESA_GLES_VERSION_OVERRIDE": "3.0",
           "MESA_EXTENSION_OVERRIDE": "-GL_ARB_shader_storage_buffer_object"}
CASES = ("vulkan", "no-vulkan-icd", "device-create-failure", "gl-only", "neither", "gen6-lavapipe", "gen6-only")
# GPUI's reason per failing case, then each refused adapter: with no EGL no adapter is tried; Gen6 GL lacks storage buffers.
FAILING = {"neither": ("Failed to create surface:", []),
           "gen6-only": ("No GPU adapter found that can configure the display surface",
                         ["(Gl) was refused: Failed to create wgpu device: Limit 'max_storage_buffer_binding_size'"])}
ROOT = Path(sys.argv[1])
assert ROOT.is_absolute() and (ROOT / ".flea-test-sandbox").is_file()
REPO = Path.cwd()
# Sample input: pub const APP_ID: &str = "com.thisisgm.flea";
APP_IDS = re.findall(r'(?m)^pub const APP_ID: &str = "([^"\n]+)";$',
                     (REPO / "crates/flea-gui/src/window/mod.rs").read_text())
assert len(APP_IDS) == 1, "window source must define exactly one APP_ID"
APP_ID = APP_IDS[0]
RUNS = int(os.environ.get("FLEA_RENDER_RUNS", "3"))
assert 2 <= RUNS <= 10
SESSION = {}
ACTIVE_CASE = None


def fail(message):
    if ACTIVE_CASE is not None:
        path = ACTIVE_CASE / "stderr.log"
        if path.is_file():
            print(path.read_bytes()[-12000:].decode(errors="replace"), flush=True)
    raise RuntimeError(message)


def stop(process):
    if process.poll() is None:
        assert process.pid > 1
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)


def environment(home):
    env = dict(os.environ)
    for key in list(env):
        if key.startswith(("VK_", "FLEA_VK_", "QSG_", "QT_")) or key in (
            "DISPLAY", "WAYLAND_DISPLAY", "FLEA_RENDERER_AUTOMATIC", "FLEA_SELECT",
            "FLEA_PREFETCH", "FLEA_PREFETCH_SHELL", "LD_PRELOAD", "LIBGL_DRIVERS_PATH",
            "__EGL_VENDOR_LIBRARY_FILENAMES", "MESA_LOADER_DRIVER_OVERRIDE", "GALLIUM_DRIVER",
        ):
            env.pop(key)
    for name, relative in (("HOME", "home"), ("XDG_CACHE_HOME", "cache"),
                           ("XDG_CONFIG_HOME", "config"), ("XDG_STATE_HOME", "state"),
                           ("XDG_DATA_HOME", "data")):
        directory = home / relative
        directory.mkdir(parents=True, exist_ok=True)
        env[name] = str(directory)
    env.update(XDG_CONFIG_DIRS=str(ROOT / "empty"), XDG_DATA_DIRS=str(ROOT / "empty"),
               XDG_RUNTIME_DIR=SESSION.get("XDG_RUNTIME_DIR", str(ROOT / "runtime")), LIBGL_ALWAYS_SOFTWARE="1",
               GALLIUM_DRIVER="llvmpipe", MESA_SHADER_CACHE_DISABLE="true")
    env.update(SESSION)
    return env


def manifest():
    for directory in (Path("/usr/share/vulkan/icd.d"), Path("/etc/vulkan/icd.d")):
        for path in sorted(directory.glob("*.json")):
            document = json.loads(path.read_text())
            library = document.get("ICD", {}).get("library_path", "")
            if Path(library).name == "libvulkan_lvp.so":
                return document, library, path
    fail("vulkan-swrast (Lavapipe) is missing; no packages were changed")



def mapped_surface(server, surface_id):
    if surface_id is None:
        return False
    surface = rf"wl_surface[#@]{surface_id}"
    # Sample input: get_xdg_surface(new id xdg_surface#29, wl_surface#23)
    xdg = re.search(r"get_xdg_surface\(new id xdg_surface[#@](\d+), " + surface + r"\)", server)
    if xdg is None:
        return False
    top = re.search(rf"xdg_surface[#@]{xdg[1]}\.get_toplevel\(new id xdg_toplevel[#@](\d+)\)", server)
    if top is None or not re.search(rf'xdg_toplevel[#@]{top[1]}\.set_app_id\("{re.escape(APP_ID)}"\)', server):
        return False
    attached = False
    # Sample input: wl_surface#23.attach(wl_buffer#8, 0, 0), then wl_surface#23.commit().
    for operation, arguments in re.findall(surface + r"\.(attach|commit)\(([^)]*)\)", server):
        if operation == "attach":
            attached = arguments.lstrip().startswith("wl_buffer")
        elif attached:
            return True
    return False


def mapped_surface_controls():
    role = ("get_xdg_surface(new id xdg_surface#29, wl_surface#23)\n"
            "xdg_surface#29.get_toplevel(new id xdg_toplevel#30)\n"
            f'xdg_toplevel#30.set_app_id("{APP_ID}")\n')
    attach = "wl_surface#23.attach(wl_buffer#8, 0, 0)\n"
    commit = "wl_surface#23.commit()\n"
    cases = (("committed buffer", role + attach + commit, True),
             ("attachment only", role + attach, False),
             ("commit before attachment", role + commit + attach, False),
             ("other surface commit", role + attach + "wl_surface#24.commit()\n", False),
             ("detached before commit", role + attach + "wl_surface#23.attach(nil, 0, 0)\n" + commit, False),
             ("other app", role.replace(APP_ID, "other.app") + attach + commit, False))
    for name, trace, expected in cases:
        assert mapped_surface(trace, 23) is expected, name
    assert mapped_surface((role + attach + commit).replace("#", "@"), 23)


def journal_packets(journal):
    packets = []
    journal.setblocking(False)
    try:
        while True:
            packets.append(journal.recv(8192).decode())
    except BlockingIOError:
        return packets
    finally:
        journal.settimeout(3)


def run_case(case, run, document, journal, entry, lavapipe, egl_vendor):
    global ACTIVE_CASE
    journal_packets(journal)
    directory = ROOT / f"{entry}-{case}-{run}"
    directory.mkdir()
    ACTIVE_CASE = directory
    env = environment(directory)
    env.update(WAYLAND_DISPLAY=SESSION["WAYLAND_DISPLAY"], WAYLAND_DEBUG="client", FLEA_FRONTEND="gpui",
               FLEA_BIN=str(REPO / "target/debug/flea"), FLEA_GUI_RENDER_DIAGNOSTICS="1",
               QSG_RHI_BACKEND="opengl", FLEA_RENDERER_AUTOMATIC="1",
               __EGL_VENDOR_LIBRARY_FILENAMES=str(egl_vendor))
    # Window IPC requires the real owner-only directory; only the compositor needs the short descriptor path.
    env["XDG_RUNTIME_DIR"] = str(ROOT / "runtime")
    # A stand-in notify-send records the desktop notice, so no real notification is shown.
    notice = directory / "notice.log"
    env.update(PATH=f"{ROOT / 'bin'}:{env.get('PATH', '/usr/bin')}", FLEA_TEST_NOTICE=str(notice))
    selected_icd = ROOT / "empty"
    if case.startswith("gen6"):
        env.update(GEN6_GL)
    if case in ("vulkan", "gen6-lavapipe"):
        selected_icd = lavapipe
    elif case == "device-create-failure":
        faulty = json.loads(json.dumps(document))
        faulty["ICD"]["library_path"] = str(ROOT / "fail-device.so")
        selected_icd = directory / "fault.json"
        selected_icd.write_text(json.dumps(faulty))
    elif case in ("no-vulkan-icd", "gen6-only"):
        env["QSG_RHI_BACKEND"] = ""
    elif case == "gl-only":
        env["MESA_LOADER_DRIVER_OVERRIDE"] = "swrast"
    elif case == "neither":
        env.update(__EGL_VENDOR_LIBRARY_FILENAMES=str(ROOT / "empty/no-egl.json"),
                   LIBGL_DRIVERS_PATH=str(ROOT / "empty"), QSG_RHI_BACKEND="")
    env.update(VK_DRIVER_FILES=str(selected_icd), VK_ICD_FILENAMES=str(selected_icd))
    command = ["bwrap", "--die-with-parent", "--ro-bind", "/", "/", "--bind", str(ROOT), str(ROOT),
               "--bind", str(ROOT / "tmp"), "/tmp", "--dev", "/dev",
               "--bind", str(ROOT / "journal.socket"), "/dev/log"]
    if entry == "fork":
        command += [str(REPO / "target/debug/flea-gui"), "--", str(directory)]
    else:
        command += [str(REPO / "target/debug/flea"), "--gui", str(directory)]
    rows = []
    frame = threading.Event()
    timing = []
    surface_id = [None]
    started = time.monotonic_ns()
    server_start = (ROOT / "compositor.log").stat().st_size
    with (directory / "stdout.log").open("w") as stdout:
        process = subprocess.Popen(command, env=env, cwd=directory, stdout=stdout, stderr=subprocess.PIPE,
                                   text=True, start_new_session=True)

        def read_output():
            attached = False
            with (directory / "stderr.log").open("w") as output:
                for row in process.stderr:
                    rows.append(row)
                    output.write(row)
                    toplevel = re.search(r"get_xdg_surface\(.*wl_surface[#@](\d+)\)", row)
                    if toplevel and surface_id[0] is None:
                        surface_id[0] = toplevel[1]
                    if surface_id[0] is None:
                        continue
                    surface = rf"wl_surface[#@]{surface_id[0]}"
                    if re.search(surface + r"\.attach\(wl_buffer", row):
                        attached = True
                    if attached and re.search(surface + r"\.commit\(", row) and not frame.is_set():
                        timing.append((time.monotonic_ns() - started) / 1_000_000)
                        frame.set()

        reader = threading.Thread(target=read_output)
        reader.start()
        try:
            deadline = time.monotonic() + 30
            while not frame.wait(0.02) and process.poll() is None and time.monotonic() < deadline:
                pass
            if frame.is_set():
                time.sleep(0.1)
            elif process.poll() is None:
                fail(f"{case}: no frame and no exit within 30 seconds")
        finally:
            pre_stop_exit = process.poll()
            stop(process)
            reader.join(timeout=5)
            process.stderr.close()
        text = "".join(rows)
    server = (ROOT / "compositor.log").read_bytes()[server_start:].decode(errors="replace")
    mapped = mapped_surface(server, surface_id[0])
    adapter = next((row.strip() for row in rows if row.startswith("flea-render:")), "none")
    if "so the shell starts on OpenGL" in text:
        fail(f"{case}: Qt downgrade ran on the GPUI path")
    notices = notice.read_text().split("\n")[:-1] if notice.is_file() else []
    # The line report() writes after the sentence is GPUI's own reason for the failure.
    cause = next((row.strip() for before, row in zip(rows, rows[1:]) if before.strip() == MESSAGE), "")
    refusals = [row.strip() for row in rows if " was refused: " in row]
    if case in FAILING:
        if mapped or frame.is_set() or process.returncode != 1 or text.count(MESSAGE) != 1:
            fail(f"{case}: mapped={mapped}, exit={process.returncode}, sentences={text.count(MESSAGE)}")
        reason, refused = FAILING[case]
        if not cause.startswith(reason):
            fail(f"{case}: GPUI's reason was {cause!r}, not one starting {reason!r}")
        if len(refusals) != len(refused) or not all(want in got for want, got in zip(refused, refusals)):
            fail(f"{case}: the refused adapters were {refusals}, not {refused}")
        packets = journal_packets(journal)
        if sum(packet.count(MESSAGE) for packet in packets) != 1:
            fail(f"{case}: journal did not receive the driver sentence exactly once")
        if notices != NOTICE:
            fail(f"{case}: the desktop notice was {notices}, not {NOTICE}")
    else:
        if not mapped or not frame.is_set() or adapter == "none" or pre_stop_exit is not None:
            fail(f"{case}: mapped={mapped}, exit={process.returncode}, adapter={adapter}")
        if notices:
            fail(f"{case}: a window that opened sent a desktop notice: {notices}")
        if refusals:
            fail(f"{case}: a window that opened reported refused adapters: {refusals}")
        driver = re.search(r'driver="([^"]*)"', adapter)
        lavapipe_case = case in ("vulkan", "gen6-lavapipe")
        if lavapipe_case and (not driver or "llvmpipe" not in driver[1].lower()):
            fail(f"{case}: Lavapipe was not selected: {adapter}")
        if not lavapipe_case and driver and driver[1]:
            fail(f"{case}: GL was not selected: {adapter}")
        if case == "device-create-failure" and "render-test: vkCreateDevice refused" not in text:
            fail("device-create-failure: the injected device failure was never reached")
    milliseconds = f"{timing[0]:.2f}" if timing else "none"
    detail = f" cause={cause!r} refused={refusals!r}" if case in FAILING else ""
    print(f"CASE {case} entry={entry} run={run} adapter={adapter} first_buffer_ms={milliseconds} mapped={int(mapped)} exit={process.returncode}{detail}", flush=True)
    return timing


def main():
    mapped_surface_controls()
    for directory in ("empty", "runtime", "tmp"):
        (ROOT / directory).mkdir(mode=0o700)
    (ROOT / "empty/.flea-test-sandbox").touch()
    (ROOT / "bin").mkdir(mode=0o700)
    stand_in = ROOT / "bin/notify-send"
    stand_in.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$FLEA_TEST_NOTICE\"\n")
    stand_in.chmod(0o700)
    document, library, lavapipe = manifest()
    egl_vendor = Path("/usr/share/glvnd/egl_vendor.d/50_mesa.json")
    if not egl_vendor.is_file():
        fail("Mesa's EGL vendor manifest is missing; no packages were changed")
    subprocess.run(["cc", "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror",
                    f'-DREAL_ICD_PATH="{library}"', "tests/gui-render-fallback.c", "-ldl",
                    "-o", str(ROOT / "fail-device.so")], check=True)
    journal = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
    journal.bind(str(ROOT / "journal.socket"))
    journal.settimeout(3)
    runtime_fd = os.open(ROOT / "runtime", os.O_RDONLY | os.O_DIRECTORY)
    SESSION["XDG_RUNTIME_DIR"] = f"/proc/{os.getpid()}/fd/{runtime_fd}"
    env = environment(ROOT / "compositor")
    env.update(WAYLAND_DEBUG="server", LIBSEAT_BACKEND="noop", HYPRLAND_NO_SD_TARGET="1",
               HYPRLAND_NO_SD_NOTIFY="1", HYPRLAND_NO_RT="1")
    config = ROOT / "hyprland.lua"
    config.touch()
    with (ROOT / "compositor.log").open("w") as log:
        compositor = subprocess.Popen(["start-hyprland", "--", "--config", str(config)],
                                      env=env, cwd=ROOT, stdout=log, stderr=log, start_new_session=True)
        try:
            deadline = time.monotonic() + 10
            while True:
                displays = [path.name for path in (ROOT / "runtime").glob("wayland-*") if path.is_socket()]
                instances = [path.parent.name for path in (ROOT / "runtime/hypr").glob("*/.socket.sock")]
                if displays and instances:
                    SESSION.update(WAYLAND_DISPLAY=displays[0], HYPRLAND_INSTANCE_SIGNATURE=instances[0])
                    break
                if compositor.poll() is not None or time.monotonic() > deadline:
                    print((ROOT / "compositor.log").read_text(errors="replace")[-12000:], flush=True)
                    for detail in (ROOT / "runtime/hypr").glob("*/hyprland.log"):
                        print(detail.read_bytes()[-16000:].decode(errors="replace"), flush=True)
                    render_nodes = list(Path("/dev/dri").glob("*"))
                    print(f"gui-render-fallback: compositor render devices={render_nodes}", flush=True)
                    fail("Hyprland did not create its private Wayland and control sockets within 10 seconds")
                time.sleep(0.02)
            ready = environment(ROOT / "control")
            subprocess.run(["hyprctl", "output", "create", "headless", "FLEA"], env=ready,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=10)
            for entry in ("fork", "launcher"):
                for case in CASES:
                    timings = []
                    for run in range(1, RUNS + 1):
                        timings += run_case(case, run, document, journal, entry, lavapipe, egl_vendor)
                    bounds = f"{min(timings):.2f}..{max(timings):.2f}" if timings else "none"
                    print(f"RANGE {case} entry={entry} first_buffer_ms={bounds}", flush=True)
        finally:
            stop(compositor)
            journal.close()
            os.close(runtime_fd)


try:
    main()
except (RuntimeError, OSError, subprocess.SubprocessError) as error:
    print(f"FAIL gui-render-fallback: {error}", flush=True)
    sys.exit(1)

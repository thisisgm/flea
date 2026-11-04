"""Hidden-file preference proof using picker-native's private portal, native keys and captures."""
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import sys
import tomllib
import traceback

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
from flea_ipc import Client
IPC = Client("qs")
IPC.export()


def backend_request(native, name, folder, hidden):
    # The public frontend drops show_hidden, so explicit overrides use the real implementation interface.
    class BackendRequest(native["Request"]):
        def __init__(self):
            self.name, self.title = name, f"Flea picker {os.getpid()} {name}"
            self.result, self.responses, self.pid = None, 0, None
            self.handle = f"{native['OBJECT']}/request/issue313/{name.replace('-', '_')}"
            self.subscription = native["bus"].signal_subscribe(native["BACKEND"], "org.freedesktop.impl.portal.Request",
                "Response", self.handle, None, native["Gio"].DBusSignalFlags.NONE, self.response)
            options = {"current_folder": native["GLib"].Variant("ay", os.fsencode(folder) + b"\0"),
                       "show_hidden": native["GLib"].Variant("b", hidden)}
            native["write"](native["root"] / f"request-{name}.json", json.dumps({"route": "backend",
                "method": "OpenFile", "options": {key: value.unpack() for key, value in options.items()}}))
            native["current"] = self
            native["bus"].call(native["BACKEND"], native["OBJECT"], "org.freedesktop.impl.portal.FileChooser", "OpenFile",
                native["GLib"].Variant("(osssa{sv})", (self.handle, "issue313", "", self.title, options)),
                None, native["Gio"].DBusCallFlags.NONE, 120000, None, self.finished, None)

        def finished(self, connection, result, _data=None):
            try:
                answer = connection.call_finish(result)
                self.response(connection, native["BACKEND"], self.handle, "", "", answer)
            except Exception as error:
                self.result = {"response": 2, "results": {}, "error": str(error)}
                native["write"](native["root"] / f"response-{name}.json", json.dumps(self.result))

    return BackendRequest()


def restore_display(native, workspace, active, helper=None):
    """Restore both saved identities, collecting failures so one cannot prevent the other attempt."""
    helper = helper or native["REPO"] / "tests/lib/hypr-dispatch.sh"
    failures = []
    try:
        if type(workspace) is not int:
            raise AssertionError("saved workspace is not an integer")
        if native["run"](["bash", helper, "workspace_focus", workspace], native["drive_env"]) != "ok":
            raise AssertionError("workspace focus was refused")
        # Sample input: {"id": 1, "name": "1", "monitor": "DP-2", "windows": 0}.
        restored = json.loads(native["run"](["hyprctl", "-j", "activeworkspace"], native["drive_env"]))
        if type(restored.get("id")) is not int or restored["id"] != workspace:
            raise AssertionError(f"workspace did not restore {workspace}: {restored!r}")
    except Exception as error:
        failures.append("workspace: " + str(error))
    try:
        if active:
            address, pid = active.get("address", ""), active.get("pid")
            if not isinstance(address, str) or not re.fullmatch(r"0x[0-9a-fA-F]+", address) or type(pid) is not int or pid <= 0:
                raise AssertionError("saved window has no valid address and PID")
            matches = [window for window in native["windows"]() if window.get("address") == address
                       and type(window.get("pid")) is int and window["pid"] == pid]
            if len(matches) != 1:
                raise AssertionError(f"saved window {address} PID {pid} is missing or ambiguous")
            if native["run"](["bash", helper, "window_focus", address], native["drive_env"]) != "ok":
                raise AssertionError("window focus was refused")
            # Sample input: {"address": "0xabc", "pid": 71, "mapped": true, "hidden": false}.
            restored = json.loads(native["run"](["hyprctl", "-j", "activewindow"], native["drive_env"]))
            if restored.get("address") != address or type(restored.get("pid")) is not int or restored["pid"] != pid:
                raise AssertionError(f"window did not restore {address} PID {pid}: {restored!r}")
            # Sample input: {"id": 1, "name": "1", "monitor": "DP-2", "windows": 0}.
            restored = json.loads(native["run"](["hyprctl", "-j", "activeworkspace"], native["drive_env"]))
            if type(restored.get("id")) is not int or restored["id"] != workspace:
                raise AssertionError(f"window focus left saved workspace {workspace}: {restored!r}")
        elif active != {}:
            raise AssertionError("saved active-window observation is not an object")
    except Exception as error:
        failures.append("window: " + str(error))
    return failures


def test_issue313(native):
    root, check, write = native["root"], native["check"], native["write"]
    drive_env, run = native["drive_env"], native["run"]
    workspace = json.loads(run(["hyprctl", "-j", "activeworkspace"], drive_env))["id"]
    active = json.loads(run(["hyprctl", "-j", "activewindow"], drive_env))
    theme_path = Path.home() / ".local/state/omarchy/current/theme/colors.toml"
    theme_bytes = theme_path.read_bytes()
    foreground = tomllib.loads(theme_bytes.decode())["foreground"].lower()
    folder = native["guard"](root / "issue313-files")
    folder.mkdir()
    write(folder / "visible.txt", "issue313 visible fixture\n")
    write(folder / ".dotfile", "issue313 hidden fixture\n")
    write(root / "issue313-theme.json", json.dumps({"path": str(theme_path), "resolved": str(theme_path.resolve()),
        "sha256": hashlib.sha256(theme_bytes).hexdigest(), "foreground": foreground, "font": 14}, indent=2))
    request, captures = None, []

    def observe(request, stage, expected, saved, override):
        state = request.until(stage + " palette loaded", lambda state: state["themeLoaded"])
        expected_names = [".dotfile", "visible.txt"] if expected else ["visible.txt"]
        check(f"{request.name}: {stage} exact visible rows", state["state"] == "ready" and state["held"] == 0
              and state["total"] == len(expected_names) and native["names"](state) == expected_names, state)
        check(f"{request.name}: {stage} saved menu preference retained", native["ui_json"]()["hidden"] is saved)
        check(f"{request.name}: {stage} font 14 and Windows Ctrl+H preset", state["body"] == 14
              and state["bodySmall"] == 13 and state["preset"] == "windows", state)
        check(f"{request.name}: {stage} current theme", state["themeLoaded"]
              and state["themeForeground"].lower() == foreground and theme_path.read_bytes() == theme_bytes, state)
        # Sample environ entry: FLEA_PICKER={"hidden":true}\0
        environment = dict(item.split(b"=", 1) for item in Path(f"/proc/{request.pid}/environ").read_bytes().split(b"\0") if b"=" in item)
        payload = json.loads(environment[b"FLEA_PICKER"])
        check(f"{request.name}: {stage} exact portal override presence", ("hidden" in payload) == (override is not None)
              and (override is None or payload["hidden"] is override), payload)
        centres = [run([*IPC.command(), "--pid", request.pid, "call", "fleapicker", "rowCentre", index],
                       native["picker_env"]) for index in range(len(expected_names))]
        check(f"{request.name}: {stage} rows have live delegates", all(re.fullmatch(r"[0-9]+ [0-9]+", value)
              and 0 < int(value.split()[0]) < state["width"] and 0 < int(value.split()[1]) < state["height"] for value in centres), centres)
        request.capture(stage)
        picture = root / f"{request.name}-{stage}.png"
        data = picture.read_bytes()
        check(f"{request.name}: {stage} real PNG", data[:8] == b"\x89PNG\r\n\x1a\n" and len(data) > 100
              and all(size > 0 for size in struct.unpack(">II", data[16:24])))
        identity = {"pid": request.pid, "window": request.window(), "request": payload,
                    # Sample stat suffix after pid/comm: R 1 123 123 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 1 0 456789
                    "processStartTicks": Path(f"/proc/{request.pid}/stat").read_text().rsplit(") ", 1)[1].split()[19],
                    "savedHidden": saved, "expectedVisibleHidden": expected, "rowCentres": centres,
                    "png": picture.name, "pngSha256": hashlib.sha256(data).hexdigest(), "snapshot": state}
        write(root / f"{request.name}-{stage}-identity.json", json.dumps(identity, indent=2))
        captures.append(identity)

    try:
        for saved in (False, True):
            for override in (None, True, False):
                label = f"issue313-saved-{str(saved).lower()}-override-{str(override).lower()}"
                patch = {"hidden": saved, "pickerView": "list", "keys": "windows", "display": {"textSize": {"mode": 14}}}
                run([native["BIN"], "--ui-state", json.dumps(patch)], native["picker_env"])
                request = (native["Request"](label, folder=folder) if override is None
                           else backend_request(native, label, folder, override))
                request.opened()
                expected = saved if override is None else override
                observe(request, "first-listing", expected, saved, override)
                request.key("-M", "ctrl", "-k", "h", "-m", "ctrl")
                wanted = [".dotfile", "visible.txt"] if not expected else ["visible.txt"]
                request.until("Ctrl+H locally toggles the real listing", lambda state: state["state"] == "ready"
                              and native["names"](state) == wanted and state["total"] == len(wanted))
                observe(request, "ctrl-h", not expected, saved, override)
                request.cancel()
                check(label + " cancelled picker kept saved hidden preference", native["ui_json"]()["hidden"] is saved)
                request = native["Request"](label + "-reopen", folder=folder).opened()
                observe(request, "fresh-preference", saved, saved, None)
                request.cancel()
                check(label + " reopened picker kept saved hidden preference", native["ui_json"]()["hidden"] is saved)
        write(root / "issue313-captures.json", json.dumps(captures, indent=2))
        check("issue313 six cases produced 18 native captures", len(captures) == 18, len(captures))
    except BaseException:
        write(root / "issue313-failure.txt", traceback.format_exc())
        current = native.get("current")
        if current is not None and current.pid is not None:
            try:
                current.window()
                write(root / "issue313-failed-state.json", json.dumps(current.state(), indent=2))
                native["drive"]("shot", native["guard"](root / "issue313-failed.png"), current.title)
            except Exception as error:
                write(root / "issue313-failed-capture.txt", str(error))
        raise
    finally:
        primary = sys.exc_info()[1]
        failures = []
        current = native.get("current")
        if current is not None and current.result is None:
            try:
                direct = current.name.endswith(("override-true", "override-false"))
                native["bus"].call_sync(native["BACKEND"] if direct else native["FRONTEND"], current.handle,
                    "org.freedesktop.impl.portal.Request" if direct else "org.freedesktop.portal.Request", "Close",
                    None, None, native["Gio"].DBusCallFlags.NONE, 5000, None)
            except Exception as error:
                failures.append(str(error))
        failures.extend(restore_display(native, workspace, active))
        write(root / "issue313-cleanup.json", json.dumps({"primary": str(primary), "failures": failures,
              "workspace": workspace, "originalWindow": active}, indent=2))
        if failures:
            if primary is not None:
                primary.add_note("issue313 cleanup: " + "; ".join(failures))
            else:
                raise AssertionError("issue313 cleanup: " + "; ".join(failures))

#!/usr/bin/env python3
"""Load the complete exporter against small stubs and exercise its retained tag rule."""

from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile


def main():
    worktree = Path(__file__).resolve().parents[2]
    parent = worktree / ".superpowers/g05"
    parent.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="qml-check.", dir=parent))
    assert root.is_absolute()
    (root / ".g05-test-root").write_text("g05 exporter load check\n")

    def write(relative, text):
        path = root / relative
        assert path.is_absolute() and path.is_relative_to(root)
        assert ".." not in path.parts and (root / ".g05-test-root").is_file()
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def module(name, entries):
        directory = "imports/" + name.replace(".", "/")
        lines = ["module " + name]
        for kind, singleton, body in entries:
            lines.append(("singleton " if singleton else "") + f"{kind} 1.0 {kind}.qml")
            write(f"{directory}/{kind}.qml", ("pragma Singleton\n" if singleton else "") + "import QtQuick\n" + body + "\n")
        write(f"{directory}/qmldir", "\n".join(lines) + "\n")

    module("Quickshell", [
        ("ShellRoot", False, "Item {}"),
        ("Quickshell", True, 'QtObject { function env(key) { return "" } function execDetached(args) { Qt.quit() } }'),
    ])
    module("Quickshell.Io", [
        ("FileView", False, 'QtObject { property string path; property bool blockLoading; property bool printErrors; function text() { return "[]" } }'),
    ])
    module("qs.Commons", [(kind, True, "QtObject {}") for kind in ("Color", "Style")])
    write("Oracle/qmldir", "singleton Theme 1.0 Theme.qml\nsingleton ViewState 1.0 ViewState.qml\nsingleton Glass 1.0 Glass.qml\n")
    write("Oracle/Theme.qml", "pragma Singleton\nimport QtQuick\nQtObject { property var tag: ({ colors: {}, menuColors: {} }) }\n")
    for kind in ("ViewState", "Glass"):
        write(f"Oracle/{kind}.qml", "pragma Singleton\nimport QtQuick\nQtObject {}\n")
    harness = r'''
    Component.onCompleted: {
        if (root.color(Qt.rgba(1, 0.5, 0, 1)) !== "#ffff8000") throw new Error("channel conversion")
        var base = {red: "#c8b0b0", orange: "#ff8000", yellow: "#ffff00", green: "#00ff00",
            blue: "#0000ff", magenta: "#ff00ff", dark_foreground: "#777777"}
        var variants = [base, Object.assign({}, base, {red: "#c8b1b1"}), Object.assign({}, base, {red: "#000000"}),
            Object.assign({}, base, {red: ""}), Object.assign({}, base, {red: base.green})]
        var names = ["boundary", "grey", "black", "missing", "duplicate"]
        for (var n = 0; n < variants.length; n++) {
            var found = variants[n], colors = {}, labels = ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"]
            var roles = ["red", "orange", "yellow", "green", "blue", "magenta", "dark_foreground"]
            for (var i = 0; i < labels.length; i++) colors[labels[i]] = found[roles[i]]
            Flea.Theme.tag = {colors: colors, menuColors: colors}
            root.palettes = [{name: names[n]}]
            root.dumpTags(found, "#000000", "#000000")
        }
        console.log("THEME_EXPORTER_CHECK_DONE")
        Qt.quit()
    }
'''
    source_path = Path(sys.argv[1]) if len(sys.argv) == 2 else worktree / "tools/theme-golden/dump.qml"
    source = source_path.read_text()
    before, after = source.rsplit("}", 1)
    write("probe.qml", before + harness + "}" + after)
    for name in ("home", "cache", "config", "state", "runtime"):
        (root / name).mkdir(mode=0o700)
    env = dict(os.environ, HOME=str(root / "home"), XDG_CONFIG_HOME=str(root / "config"),
               XDG_STATE_HOME=str(root / "state"), XDG_CACHE_HOME=str(root / "cache"),
               XDG_RUNTIME_DIR=str(root / "runtime"), QT_QPA_PLATFORM="offscreen", QSG_RHI_BACKEND="opengl",
               QML_IMPORT_PATH=str(root / "imports"), QML2_IMPORT_PATH=str(root / "imports"))
    qml = shutil.which("qml6") or shutil.which("qml")
    if qml is None:
        raise SystemExit("qml6 or qml is required for the exporter load check")
    result = subprocess.run([qml, str(root / "probe.qml")], env=env, capture_output=True, text=True, timeout=15)
    log = result.stdout + result.stderr
    write("qml.log", log)
    print(f"evidence: {root}")
    if result.returncode != 0:
        print(log)
        raise SystemExit(result.returncode)
    assert "THEME_EXPORTER_CHECK_DONE" in log, log
    for name, fallback in (("boundary", "false"), ("grey", "true"), ("black", "true"),
                           ("missing", "true"), ("duplicate", "true")):
        assert f"THEME_GOLDEN\t{name}\t12\ttag.fallback\t{fallback}" in log, log
        assert f"THEME_GOLDEN\t{name}\t12\ttag.swatchSize\t14" in log, log
    assert not any(error in log for error in ("TypeError", "ReferenceError", "SyntaxError")), log
    print("exporter parsed and loaded; channel conversion and five tag cases passed")


if __name__ == "__main__":
    main()

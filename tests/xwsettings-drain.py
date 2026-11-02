"""Delay the frozen QML startup oracle's quitReady observer while its real backend drains."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

repo = Path.cwd()
# Long enough that a SIGTERM launched inside quitReady lands while the observer still runs.
OBSERVER_DELAY_MS = 500
# Bounds one probe launch; a healthy drain exits in a few seconds.
PROBE_TIMEOUT_S = 35
expected = 1
scratch = Path(os.environ.get('TMPDIR', repo / '.superpowers/tmp'))
scratch.mkdir(parents=True, exist_ok=True)
run = Path(tempfile.mkdtemp(prefix='xwsdrain-', dir=scratch))
assert run.is_absolute() and len(run.parts) > 2
(run / '.flea-test-sandbox').touch()
(run / 'files').mkdir()
for name in ('a.txt', 'b.txt', 'c.txt'):
    (run / 'files' / name).touch()
config = run / 'ui' / 'boot'
config.mkdir(parents=True)
(config / 'Ui').symlink_to((repo / 'ui' / 'boot' / 'Ui').resolve())
subprocess.run(['bash', '-c', '. tools/flea-sandbox-guard; . tests/headless-commons.sh; headless_commons "$1" "$2"',
                'headless_commons', str(run), str(config / 'Commons')], check=True)
source = (repo / 'tests/xwsettings-start.qml').read_text()
needle = 'function onQuitReady() { console.log("PROBE " + Quickshell.env("PROBE_ROLE") + " backend drained") }'
assert source.count(needle) == 1
source = source.replace(needle, """function onQuitReady() {
            console.log("PROBE observer entered target=" + drainObserver.target)
            var until = Date.now() + """ + str(OBSERVER_DELAY_MS) + """
            while (Date.now() < until) {}
            console.log("PROBE observer returned")
            console.log("PROBE " + Quickshell.env("PROBE_ROLE") + " backend drained")
        }""")
(config / 'shell.qml').write_text(source)
env = os.environ.copy()
for name in ('DISPLAY', 'WAYLAND_DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE'):
    env.pop(name, None)
env.update(QT_QPA_PLATFORM='offscreen', QT_FORCE_STDERR_LOGGING='1',
           QSG_RHI_BACKEND='opengl', XDG_STATE_HOME=str(run / 'state'), FLEA_UI=str(run / 'ui'),
           FLEA_BIN=sys.argv[1], FLEA_PATH=str(run / 'files'), PROBE_BODY=str(repo / 'ui' / 'WindowBody.qml'),
           PROBE_STATE_HELPER=str(repo / 'tests/xwsettings-start-state.qml'), PROBE_ROLE='B',
           PROBE_READY=str(run / 'ready'), PROBE_DONE=str(run / 'done'), PROBE_POLL_S='0.05')
subprocess.run([sys.argv[1], '--ui-state', '{"hidden":false,"view":"list","density":"compact","updates":{"autoCheck":false}}'],
               env=env, check=True, stdout=subprocess.DEVNULL)
result = subprocess.run(['timeout', str(PROBE_TIMEOUT_S), 'qs', '-p', str(config)], env=env,
                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
(run / 'probe.log').write_text(result.stdout)
count = result.stdout.count('PROBE B backend drained')
status = result.returncode if result.returncode >= 0 else 128 - result.returncode
print(result.stdout)
print(f'xwsdrain: scope=frozen-qml-oracle entry=qs expected={expected} count={count} exit={status} observer_entered={result.stdout.count("PROBE observer entered")} observer_returned={result.stdout.count("PROBE observer returned")}')
assert status == 143
assert result.stdout.count('PROBE observer entered') == 1
assert count == expected
assert result.stdout.count('PROBE observer returned') == 1
assert result.stdout.count('PROBE B drained path=') == 1
assert 'PROBE FAIL' not in result.stdout

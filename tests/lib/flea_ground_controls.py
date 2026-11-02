"""Real product material and live-toggle proof using the coherent RC2 transport."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from flea_transport import Window, built_binary, private

root = private(sys.argv[1])
colors = 'background = "#1e2030"\ndark_background = "#2c3045"\nforeground = "#d0d4e0"\naccent = "#7aa2f7"\n'
saved = dict(os.environ)
failed = 0

def read(window, mode):
    deadline = time.monotonic() + 12
    while True:
        reply = window.call('call', 'flea', 'glass')
        assert reply.returncode == 0 and not reply.stderr, reply
        # Sample input: {"mode":"glassy","groundReady":true,"alpha":0.7529412,"grounds":{"chrome":192}}
        value = json.loads(reply.stdout)
        if value.get('mode') == mode and value.get('groundReady'):
            return value
        if time.monotonic() >= deadline:
            raise RuntimeError(f'actual ground producers did not settle: {value}')
        time.sleep(.05)

for name, mode, override, expected in [('opaque', 'opaque', None, 255), ('transparent', 'transparent', None, 255),
        ('user', 'glassy', 'user', 87), ('theme', 'glassy', 'theme', 87), ('glassy', 'glassy', None, 192)]:
    case = root / name
    case.mkdir(mode=0o700)
    (case / '.flea-test-sandbox').touch()
    for key, sub in [('HOME','home'), ('XDG_CONFIG_HOME','config'), ('XDG_STATE_HOME','state'), ('XDG_DATA_HOME','data'), ('XDG_CACHE_HOME','cache')]:
        folder = case / sub
        folder.mkdir()
        os.environ[key] = str(folder)
    theme = case / 'home/.local/state/omarchy/current/theme'
    theme.mkdir(parents=True)
    (theme / 'colors.toml').write_text(colors)
    toggles = case / 'state/omarchy/toggles/hypr'
    toggles.mkdir(parents=True)
    if mode != 'transparent': (toggles / f'window-{mode}.lua').write_text('return\n')
    if override:
        path = theme / 'shell.toml' if override == 'theme' else case / 'home/.config/omarchy/shell.toml'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('[flea]\nbackground-alpha=0.34\n')
    files = case / 'files'
    files.mkdir()
    (files / 'a.txt').touch()
    subprocess.run([built_binary('flea'), '--ui-state', '{"updates":{"autoCheck":false}}'], check=True, capture_output=True)
    window = None
    try:
        window = Window.start(files, parent=case, frontend='gpui', timeout=15)
        value = read(window, mode)
        (case / 'glass.json').write_text(json.dumps(value) + '\n')
        assert round(value['alpha'] * 255) == expected, value
        assert value['grounds']['chrome'] == expected and value['grounds']['status'] == expected, value
        if name == 'glassy':
            assert value['material'] and value['grounds']['body'] == 0
            assert round(value['listAlpha'] * 255) == round(value['requestedListAlpha'] * 255), value
            assert value['grounds']['header'] == value['grounds']['listing'], value
            (toggles / 'window-glassy.lua').unlink()
            (toggles / 'window-opaque.lua').write_text('return\n')
            opaque = read(window, 'opaque')
            assert opaque['alpha'] == 1 and opaque['grounds']['chrome'] == 255, opaque
            (toggles / 'window-opaque.lua').unlink()
            (toggles / 'window-glassy.lua').write_text('return\n')
            again = read(window, 'glassy')
            assert round(again['alpha'] * 255) == 192 and again['grounds']['chrome'] == 192, again
            print('PRODUCT_GROUND live-toggle=glassy-opaque-glassy PASS', flush=True)
        window.quit()
        print(f'PRODUCT_GROUND {name} PASS actual={value}', flush=True)
    except Exception as error:
        failed += 1
        print(f'PRODUCT_GROUND {name} FAIL {error}', flush=True)
    finally:
        if window is not None: window.stop()
        os.environ.clear()
        os.environ.update(saved)
print(f'PRODUCT_GROUND failed={failed}', flush=True)
sys.exit(bool(failed))

"""Drive registered scenes through owned IPC and validate their real native frames."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

REPO = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(REPO / 'tests/lib'), str(REPO / 'ci/visual')]
from flea_transport import Window, load, endpoint, client_run
import scenes
import blackgate
import omi_fit
import codeicons_native

spec = importlib.util.spec_from_file_location('capture_check', REPO / 'tests/gpui-capture-check.py')
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)
root, logs = map(Path, sys.argv[1:])
windows = []

# viewMode answers the mode the focused pane drew; grid and columns change to their own name when their GPUI views mount.
DRAWN = {'list': 'list', 'dual': 'list', 'grid': 'grid', 'columns': 'columns'}


def require_scene_views(status, mode, view):
    scenes.require(status.get('view') == view, f'scene view {status.get("view")!r} is not {view}')
    scenes.require(mode == DRAWN[view], f'drawn view {mode!r} is not {DRAWN[view]}')


def run(*command):
    result = subprocess.run(command, capture_output=True, timeout=15)
    if result.returncode:
        raise RuntimeError(f'{command[0]} exited {result.returncode}: {result.stderr.decode().strip()}')
    return result.stdout.decode().strip()


def call(window, function, *arguments, fail=False):
    record = load(window.root)
    with endpoint(record):
        pass
    result = client_run(record, [record['binary'], 'ipc', '--pid', str(window.pid),
                                 'call', 'flea', function, *map(str, arguments)], 5)
    if fail:
        scenes.require(result.returncode == 2, f'control accepted {function}: {result}')
        return result
    scenes.require(result.returncode == 0 and not result.stderr, f'IPC {function}: {result}')
    return result.stdout.decode().strip()


def wait(window, function, expected):
    for attempt in range(100):
        if call(window, function) == expected:
            return
        time.sleep(.05)
    raise ValueError(f'{function} did not reach {expected}')


def require_frame(window, name, what, expected, actual):
    if actual != expected:
        raise ValueError(f'{name}: {what} {expected}->{actual}: {call(window, "sceneState")}')


def grab(window, out):
    clients = json.loads(run('hyprctl', 'clients', '-j'))
    item = capture.client(clients, window.pid)
    monitors = json.loads(run('hyprctl', 'monitors', '-j'))
    capture.output(monitors, item)
    scenes.require(item['size'] == [1100, 700], 'incorrect native client rect')
    x, y = item['at']
    w, h = item['size']
    geometry = f'{x},{y} {w}x{h}'
    run('grim', '-g', geometry, str(out))
    capture.frame(out, item)
    image = blackgate.read_image(out)
    scenes.require(blackgate.black_share(image, (0, 0, w, h)) <= blackgate.BLACK_THRESHOLD, 'blackgate rejected native scene')
    return item


def control(window, name, scene, settings):
    result = call(window, 'scene', window.pid if name != 'wrong-window' else 0, scene, json.dumps(settings), fail=True)
    expected = {'unknown-scene': 'not registered', 'wrong-window': 'another window', 'never-presented': 'not presented'}[name]
    scenes.require(expected in result.stderr.decode(), f'incorrect {name} refusal: {result.stderr!r}')
    print(f'GPUI_SCENE_CONTROL name={name} rejected=true', flush=True)


try:
    registered = scenes.manifest(REPO / 'ci/visual/scenes/g1-01.json')
    scenes.require({scene['view'] for scene in registered} <= set(DRAWN), 'scene view without a drawn row')
    control_manifest = root / 'manifest.json'
    shutil.copyfile(REPO / 'ci/visual/scenes/g1-01.json', control_manifest)
    scenes.controls(control_manifest)
    os.environ['FLEA_TEST_WINDOWS_ROOT'] = str(root)
    codeicons_native.run(root, logs, Window, call, wait, grab, run, windows)
    for scene in registered:
        name = scene['id']
        state = json.dumps(dict(view=scene['view'], density='compact', display=dict(textSize=dict(mode=14))))
        run(os.environ['FLEA_BIN'], '--ui-state', state)
        run('hyprctl', 'eval', f'hl.config({{ decoration = {{ rounding = {scene["corner_radius"]} }} }})')
        option = json.loads(run('hyprctl', 'getoption', 'decoration:rounding', '-j'))
        scenes.require(option['int'] == scene['corner_radius'], 'compositor corner mismatch')
        run('bash', str(REPO / 'tests/lib/hypr-dispatch.sh'), 'cursor_move', '2559', '1439')
        window = Window.start(root / 'fixture', parent=root, frontend='gpui', log=logs / f'{name}-app.log', timeout=15)
        windows.append(window)
        wait(window, 'total', '2')
        wait(window, 'pendingRequests', '0')
        scenes.wait_quiet(lambda function: call(window, function))
        settings = dict(path=str(root / 'fixture'), width=1100, height=700)
        receipt = json.loads(call(window, 'scene', window.pid, name, json.dumps(settings)))
        scenes.require(receipt['window'] == window.pid and receipt['scene'] == name and receipt['frame'] > 0, 'missing scene receipt')
        scenes.require(call(window, 'path') == settings['path'] and call(window, 'cursor') == '0', 'stale native rows')
        scenes.require(call(window, 'previewReadiness') == 'absent', 'unexpected preview claim')
        status = json.loads(call(window, 'sceneState'))
        scenes.require(status['receipt'] == receipt and status['pending'] == 0, 'incomplete native requests')
        require_scene_views(status, call(window, 'viewMode'), scene['view'])
        if name == registered[0]['id']:
            other = 'grid' if scene['view'] != 'grid' else 'list'
            try:
                require_scene_views(dict(status, view=other), DRAWN[scene['view']], scene['view'])
            except ValueError:
                print('GPUI_SCENE_CONTROL name=scene-view rejected=true', flush=True)
            else:
                raise ValueError('a sceneState view that differs from the scene was accepted')
        frame = int(call(window, 'presentedFrame'))
        require_frame(window, name, 'stale presented receipt', receipt['frame'], frame)
        sidecar = json.loads(call(window, 'nativeGeometry', window.pid, name, frame))
        before = logs / f'{name}-noise.png'
        image = logs / f'{name}.png'
        item = grab(window, before)
        for attempt in range(1, 21):
            time.sleep(1)
            final = grab(window, image)
            if before.read_bytes() == image.read_bytes():
                break
            shutil.copyfile(image, before)
        else:
            raise ValueError(f'{name} unstable after 20 pairs')
        scenes.require(item['at'] == final['at'] and item['size'] == final['size'], 'native rect moved')
        require_frame(window, name, 'frame changed during capture', frame, int(call(window, 'presentedFrame')))
        scenes.require(call(window, 'pendingRequests') == '0', 'backend request still pending after capture')
        digest = scenes.wrapper(sidecar, receipt, image, logs / f'{name}.html')
        (logs / f'{name}-receipt.json').write_text(json.dumps(receipt, indent=2))
        print(f'GPUI_SCENE PASS scene={name} frame={frame} rect=1100x700 scale=1 stable={attempt} blackgate=pass roles={len(sidecar["roles"])} sha256={digest}', flush=True)
        if name == registered[0]['id']:
            other = root / 'fixture-other'
            other.mkdir()
            (other / 'gamma.txt').write_text('three\n')
            os.utime(other / 'gamma.txt', (1750000000, 1750000000))
            moved = json.loads(call(window, 'scene', window.pid, name, json.dumps(dict(settings, path=str(other)))))
            scenes.require(moved['frame'] > frame and moved['path'] == str(other), 'path scene did not present')
            scenes.require(call(window, 'total') == '1' and call(window, 'visibleRowName', 0) == 'gamma.txt', 'stale rows after fixture change')
            scenes.require(call(window, 'pendingRequests') == '0', 'path scene requests incomplete')
            print('GPUI_SCENE_PATH PASS fresh_rows=true', flush=True)
            control(window, 'unknown-scene', 'unknown', settings)
            control(window, 'wrong-window', name, settings)
            control(window, 'never-presented', name, dict(settings, width=1099))
            try:
                scenes.validate(dict(sidecar, roles=[]), receipt, image)
            except ValueError as error:
                scenes.require(str(error) == 'empty sidecar', str(error))
                print('GPUI_SCENE_CONTROL name=empty-sidecar rejected=true', flush=True)
            else:
                raise ValueError('empty sidecar was accepted')
        window.quit()
    run(os.environ['FLEA_BIN'], '--ui-state', json.dumps(dict(
        view='list', density='compact', display=dict(textSize=dict(mode=14)))))
    empty = root / 'omi-empty'
    empty.mkdir()
    # Capture mode forces reduced motion, which stops the caption rotation this case measures.
    window = Window.start(empty, parent=root, frontend='gpui',
                          log=logs / 'omi-caption-app.log', timeout=15,
                          wrap=('/usr/bin/env', 'FLEA_GPUI_CAPTURE=0'))
    windows.append(window)
    wait(window, 'total', '0')
    omi_fit.caption_case(window, call, grab, logs)
    window.quit()
    print('GPUI_SCENES PASS scenes=9 controls=5', flush=True)
finally:
    for window in windows:
        window.stop()

"""Offline controls for tools/flea-b3-capture: fake launcher, window and compositor; each control names its refusal.

The fakes stand in for flea, flea-gui, hyprctl and wtype inside a marked mktemp root. They prove the
driver's admission and arithmetic only; no number here is a measurement.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
CAPTURE = HERE.parent / 'tools/flea-b3-capture/capture.py'

LAUNCHER = r'''#!/usr/bin/env python3
import json, os, signal, sys, time
from pathlib import Path
state = Path(os.environ['FAKE_STATE'])
argv = sys.argv[1:]
if argv == ['--version']:
    print('flea 0.4.0-fake'); sys.exit(0)
if argv[:1] == ['--gui']:
    os.execv(sys.executable, [sys.executable, str(Path(__file__).with_name('flea-gui')), argv[1]])
assert argv[:2] == ['ipc', '--pid'] and argv[3:5] == ['call', 'flea'], argv
pid, function = int(argv[2]), argv[5]
gui = json.loads((state / 'gui.json').read_text())
assert gui['pid'] == pid, 'ipc reached a foreign pid'
scenario = os.environ['FAKE_SCENARIO']
typed = json.loads((state / 'typed.json').read_text()) if (state / 'typed.json').exists() else None
if function == 'quit':
    os.kill(pid, signal.SIGUSR1); print('true'); sys.exit(0)
if function == 'ready':
    print('true'); sys.exit(0)
if function == 'keyDeliveryState':
    query = typed['text'][1:] if typed else ''
    print(json.dumps(dict(searchQuery='zz' if scenario == 'delivery' else query, searchMode='Typing'))); sys.exit(0)
assert function == 'searchStamps', function
if scenario == 'garbage':
    print('not json'); sys.exit(0)
stamps = dict(gen=0, keyAt=0, firstRowsAt=0, settledAt=0)
if scenario != 'base':
    stamps.update(query='', indexed=None, total=None, walking=None, cancelled=None, partial=None,
                  firstRow=None, firstName=None, presentedFirst=None, presentedSettled=None, coverage='none')
# The fake compositor reports a frame only once its presentation time has passed.
if typed and scenario not in ('base', 'stuck', 'crash') and time.monotonic_ns() >= typed['ns'] + 50_000_000:
    base = typed['ns']
    first = dict(frame=7, ns=base + 30_000_000, clock=2 if scenario == 'clock2' else 1)
    if scenario == 'early':
        first['ns'] = gui['start_ns'] - 1
    stamps.update(gen=2, keyAt=40, firstRowsAt=60, settledAt=90, query=typed['text'][1:],
                  indexed=7 if scenario == 'census' else 6, total=3, walking=False, cancelled=False,
                  partial='unreadable' if scenario == 'partial' else False,
                  coverage={'lost': 'lost', 'pending': 'pending'}.get(scenario, 'complete'),
                  firstRow=1 if scenario == 'row1' else 0,
                  firstName='zzz.txt' if scenario == 'nomatch' else 'Docs/needle.txt',
                  presentedFirst=None if scenario == 'paint' else first,
                  presentedSettled=dict(frame=8, ns=base + 50_000_000, clock=1))
print(json.dumps(stamps))
'''

GUI = r'''#!/usr/bin/env python3
import json, os, signal, subprocess, sys, time
from pathlib import Path
state = Path(os.environ['FAKE_STATE'])
start = time.monotonic_ns()
backend = subprocess.Popen([sys.executable, '-c', 'import signal, time\n'
    + ('signal.signal(signal.SIGTERM, signal.SIG_IGN)\n' if os.environ['FAKE_SCENARIO'] == 'survivor' else '')
    + 'time.sleep(120)'])
def quit(*_):
    if os.environ['FAKE_SCENARIO'] != 'survivor':
        backend.terminate(); backend.wait()
    os._exit(0)
signal.signal(signal.SIGUSR1, quit)
(state / 'gui.json').write_text(json.dumps(dict(pid=os.getpid(), start_ns=start, backend=backend.pid)))
if os.environ['FAKE_SCENARIO'] == 'crash':
    time.sleep(0.5)
    os._exit(1)
time.sleep(120)
'''

FAKE_COMPOSITORS = HERE / 'fixtures/hypr-dispatch.json'
FAKE_COMPOSITOR = 'bench-offline-compositor'
# The fake compositor matches the exact selector the source scan refuses, so it lives in that scan's refusal corpus.
fakes = [row['code'] for row in json.loads(FAKE_COMPOSITORS.read_text()) if row['name'] == FAKE_COMPOSITOR]
if len(fakes) != 1:
    raise ValueError(f'{FAKE_COMPOSITORS} holds {len(fakes)} {FAKE_COMPOSITOR} entries, expected 1')
HYPRCTL = fakes[0]

WTYPE = r'''#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
(Path(os.environ['FAKE_STATE']) / 'typed.json').write_text(json.dumps(dict(text=sys.argv[1], ns=time.monotonic_ns())))
'''


def fixture(root):
    corpus = root / 'corpus'
    (corpus / 'Docs').mkdir(parents=True)
    for name in ('Docs/needle.txt', 'Docs/other.txt', 'a.txt', 'b.txt'):
        (corpus / name).write_text('x')
    (corpus / 'Empty').mkdir()
    (corpus / '.hidden').write_text('never counted')
    return corpus


def run(root, scenario, query='needle', manifest=None, cgroup=False):
    bin_dir = root / 'bin'
    state = root / f'state-{scenario}-{query}'
    state.mkdir(exist_ok=True)
    env = dict(os.environ, PATH=f'{bin_dir}:{os.environ["PATH"]}', FAKE_STATE=str(state),
               FAKE_SCENARIO=scenario, XDG_RUNTIME_DIR=str(root / 'run'), WAYLAND_DISPLAY='wayland-9')
    out = root / f'out-{scenario}-{query}'
    argv = [sys.executable, '-B', str(CAPTURE), 'run', '--flea', str(bin_dir / 'flea'),
            '--corpus', str(manifest or root / 'corpus.json'), '--query', query, '--output', str(out),
            '--timeout', '8' if scenario in ('stuck', 'pending') else '90', '--tool-timeout', '30', '--poll', '0.05', '--offline', *(['--check-cgroup'] if cgroup else [])]
    done = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=60)
    # Every run prints its verdict; RESULT.json exists only once the output root was made.
    result = json.loads((out / 'RESULT.json').read_text()) if (out / 'RESULT.json').exists() else None
    verdict = json.loads(done.stdout.strip().splitlines()[-1]) if done.stdout.strip() else {}
    return done, result, verdict


def blocked(root, scenario, code, rc=3, **options):
    done, result, verdict = run(root, scenario, **options)
    got = (verdict.get('blocked') or {}).get('code')
    if code == 'output_exists':
        assert result['status'] == 'OFFLINE_CONTROL', 'a refused run never touches the existing RESULT.json'
        result = None
    assert result is None or result['blocked']['code'] == got, f'{scenario}: RESULT.json and stdout differ'
    assert done.returncode == rc and got == code, f'{scenario}: rc {done.returncode} code {got}\n{done.stdout}\n{done.stderr}'
    if result and 'window' in result:
        survivors = result['close']['survivors'] if 'close' in result else []
        assert scenario in ('survivor', 'crash') or not survivors, f'{scenario}: survivors {survivors}'
        assert result['close']['after_kill'] == [], f'{scenario}: members outlived the forced close {result["close"]}'
    state = root / f'state-{scenario}-{options.get("query", "needle")}'
    if (state / 'gui.json').exists():
        backend = json.loads((state / 'gui.json').read_text())['backend']
        assert not Path(f'/proc/{backend}').exists() or 'Z' in Path(f'/proc/{backend}/stat').read_text().rsplit(') ', 1)[1][:1], \
            f'{scenario}: backend {backend} outlived the run'
    print(f'... {scenario} blocked {code} ok')


def main():
    parent = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(tempfile.gettempdir())
    root = Path(tempfile.mkdtemp(prefix='bench-b3-capture.', dir=parent))
    (root / '.flea-test-sandbox').touch()
    bin_dir = root / 'bin'
    bin_dir.mkdir()
    for name, body in (('flea', LAUNCHER), ('flea-gui', GUI), ('hyprctl', HYPRCTL), ('wtype', WTYPE)):
        (bin_dir / name).write_text(body)
        (bin_dir / name).chmod(0o700)
    (root / 'run').mkdir()
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(str(root / 'run/wayland-9'))
    corpus = fixture(root)
    census = subprocess.run([sys.executable, '-B', str(CAPTURE), 'census', str(corpus), str(root / 'corpus.json')],
                            capture_output=True, text=True, timeout=30)
    manifest = json.loads((root / 'corpus.json').read_text())
    assert census.returncode == 0 and (manifest['files'], manifest['directories']) == (4, 2), census
    print('... census counts non-hidden entries below the root ok')

    done, result, _ = run(root, 'ok')
    assert done.returncode == 0 and result['status'] == 'OFFLINE_CONTROL', (done.stdout, done.stderr)
    first = result['cells']['B3:million-file-search:open_to_first_result_ms']
    clocks = result['clocks_ns']
    assert first == (clocks['presented_first'] - clocks['open']) / 1e6 and first > 30, result['cells']
    assert clocks['presented_first'] - clocks['key'] >= 30_000_000, clocks
    assert result['cells']['B3:million-file-search:indexed_pss_mib'] > 1, result['cells']
    assert len(result['memory']['pids']) == 2, 'launcher window plus its backend'
    assert result['close'] == dict(exit_code=0, survivors=[], after_kill=[]), result['close']
    print('... settled capture reports presented-first minus launch and the tree PSS ok')

    blocked(root, 'base', 'missing_seam')
    blocked(root, 'paint', 'missing_presented_clock')
    blocked(root, 'clock2', 'clock_domain')
    blocked(root, 'lost', 'feedback_lost')
    blocked(root, 'pending', 'timeout')
    blocked(root, 'partial', 'partial_index')
    blocked(root, 'census', 'index_census')
    blocked(root, 'row1', 'not_first_result')
    blocked(root, 'nomatch', 'not_a_match')
    blocked(root, 'delivery', 'query_delivery')
    blocked(root, 'early', 'clock_order')
    blocked(root, 'stuck', 'timeout')
    blocked(root, 'survivor', 'failed_close')
    blocked(root, 'crash', 'gui_exited')
    blocked(root, 'nofocus', 'focus_refused')
    blocked(root, 'twowin', 'foreign_client')
    blocked(root, 'garbage', 'malformed_reply')
    blocked(root, 'ok', 'foreign_process', query='needl', cgroup=True)
    blocked(root, 'ok', 'query_text', rc=2, query='Needle!')
    bad = root / 'bad.json'
    bad.write_text(json.dumps(dict(manifest, hidden=True)))
    blocked(root, 'ok', 'corpus_hidden', rc=2, query='needlx', manifest=bad)
    blocked(root, 'ok', 'corpus_fields', rc=2, query='needly', manifest=root / 'missing.json')
    blocked(root, 'ok', 'output_exists', rc=2)
    print('BENCH_B3_CAPTURE PASS controls=23')


if __name__ == '__main__':
    main()

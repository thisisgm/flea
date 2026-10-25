"""Real executor, matcher and walker seam verification on a marked private tree."""
import json
import os
from pathlib import Path
import subprocess


def run(binary, root, cpus):
    from engines import Engine
    from artifacts import candidate
    identity = candidate(binary)
    tree = root / 'smoke-tree'
    (tree / 'deep').mkdir(parents=True)
    (tree / 'deep/render_frame.rs').write_text('fixture\n')
    (tree / 'other.txt').write_text('fixture\n')
    os.environ['FLEA_SEARCH_BIN'] = str(binary)
    engine = Engine('flea', 'native', root, root, tree, [], cpus)
    try:
        engine.begin_open()
        ready, _ = engine.until(lambda row: row.get('t') == 'searchready', timeout=15)
        if ready['n'] != 3:
            raise ValueError('executor-ready census differs: ' + json.dumps(ready))
        result = engine.query('render_frame.rs')
        if not result['paths'] or result['paths'][0] != 'deep/render_frame.rs':
            raise ValueError('integrated executor answer differs')
    finally:
        closed = engine.close()
        if closed['exit_code']:
            raise ValueError('executor smoke exit differs')
    prefix = ['taskset', '--cpu-list', ','.join(map(str, cpus)), str(binary), '--search-stage']
    walked = subprocess.run(prefix + ['walk', str(tree)], check=True, capture_output=True, timeout=30)
    walk = json.loads(walked.stdout)
    if walk['files'] != 2 or walk['dirs'] != 1 or walk['errors']:
        raise ValueError('product walker smoke differs')
    corpus = root / 'smoke-paths.txt'
    corpus.write_text('deep\ndeep/render_frame.rs\nother.txt\n')
    env = dict(os.environ, FLEA_STAGE_ROOT=str(root))
    trace = f'O\t{corpus}\nQ\t1\t350\t0\tnative\trender_frame.rs\nA\t2\tdeep/render_frame.rs\trender_frame.rs\nX\n'
    matched = subprocess.run(prefix + ['match', str(len(cpus))], input=trace, env=env, text=True,
                             check=True, capture_output=True, timeout=30)
    lines = [json.loads(line) for line in matched.stdout.splitlines()]
    if [row['t'] for row in lines] != ['boot', 'ready', 'result', 'audit'] or lines[2]['paths'][0] != 'deep/render_frame.rs' or lines[3]['rank'] != 1:
        raise ValueError('product matcher contract differs')
    result = dict(status='PASS', executor_ready=ready, walk_entries=walk['files'] + walk['dirs'],
                  matcher_rank=lines[3]['rank'], candidate=identity)
    (root / 'smoke.json').write_text(json.dumps(result, indent=2) + '\n')
    print('fs-instrument executor/matcher/walker: PASS ' + json.dumps(result), flush=True)
    return result

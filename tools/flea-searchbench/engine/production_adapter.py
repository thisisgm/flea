#!/usr/bin/env python3
"""Reviewer adapter with valid controls and one isolated defect per mode."""
import json
import os
import pathlib
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from contracts import identities
from fixtures import TARGETS

if pathlib.Path(sys.argv[0]).name == 'zoxide':
    directory = pathlib.Path(os.environ['_ZO_DATA_DIR'])
    directory.mkdir()
    (directory / 'db.zo').write_text('control-provider-only\n')
    sys.exit(0)

if len(sys.argv) == 5 and sys.argv[3].startswith('seed-'):
    state = pathlib.Path(sys.argv[2])
    history = json.loads((state / 'history.json').read_text())
    repeats = 3 if sys.argv[3] == 'seed-rich' else 1
    events = [dict(path=path, at_sec=int(time.time())) for path in history for _ in range(repeats)]
    (state / 'frecency').mkdir()
    (state / 'frecency/data.mdb').write_text(json.dumps(events))
    print(json.dumps(dict(t='priorseed', replay_us=10, events=events, prior_replay_inside_open=False)))
    sys.exit(0)

flea = len(sys.argv) == 2
tree = None if flea else pathlib.Path(sys.argv[1])
mode = os.environ.get('FB0_REVIEW_MODE', 'valid')
cached = None
first_query = True


def emit(value):
    print(json.dumps(value), flush=True)


def answer(query, limit):
    global first_query
    paths = list(cached)
    if query.startswith('zz_'):
        paths = [os.fsdecode(path) for path, _ in identities(tree) if query in os.fsdecode(path)]
    elif query:
        target = next((target for _, text, target in TARGETS if text.startswith(query)), None)
        if target in paths:
            paths.remove(target)
            paths.insert(0, target)
    count = len(paths)
    if flea and mode == 'startup-miss' and first_query and query and not query.startswith('zz_'):
        paths.remove(target)
        paths.append(target)
    if flea and mode == 'startup-misrank' and first_query and query and not query.startswith('zz_'):
        paths[0], paths[1] = paths[1], paths[0]
    first_query = False
    if not flea and mode == 'short-head-no-total' and limit == 350:
        limit = 1
    if flea and mode == 'short-head' and limit == 350:
        limit = 1
    return paths[:limit], count


if not flea:
    emit(dict(t='boot', cpus=2))
for line in sys.stdin:
    if flea:
        command = json.loads(line)
        kind = command['c']
        if kind == 'list':
            tree = pathlib.Path(command['path'])
            emit(dict(t='listed', n=len(identities(tree))))
            emit(dict(t='rows', rows=[]))
        elif kind == 'window':
            emit(dict(t='rows', rows=[]))
        elif kind == 'searchopen':
            cached = [os.fsdecode(path) for path, _ in identities(tree)]
            emit(dict(t='searchready', n=len(identities(tree)), ms=1, capped=False))
        elif kind == 'benchinventory':
            emit(dict(t='inventory', identities=[(p.hex(), k) for p, k in identities(tree)]))
        elif kind == 'benchping':
            emit(dict(t='pong'))
        elif kind == 'benchaudit':
            paths, count = answer(command['query'], command['count'])
            target = command['target']
            rank = paths.index(target) + 1 if target in paths else 0
            emit(dict(t='rank', gen=command['gen'], rank=rank, n=count, complete=True))
        elif kind == 'searchquery':
            gen = command['gen']
            legacy_clock = mode == 'startup-clock' and first_query and not command['query'].startswith('zz_')
            paths, count = answer(command['query'], command['count'])
            if mode == 'short-final-head' and command['count'] > 350:
                paths = paths[:1]
            if legacy_clock:
                emit(dict(t='searchcore', gen=gen, core_ms=1234.56789))
            else:
                emit(dict(t='searchcore', gen=gen, matcher_ms=0.001, answer_ms=1))
            emit(dict(t='listed', gen=gen, n=count))
            emit(dict(t='rows', gen=gen, rows=[dict(n=path) for path in paths]))
            emit(dict(t='searched', gen=gen, n=count, walking=False))
        elif kind == 'searchclose':
            emit(dict(t='searchclosed'))
        elif kind != 'searchcancel':
            raise ValueError(kind)
    else:
        fields = line.rstrip('\n').split('\t', 5)
        if fields[0] == 'O':
            cached = [os.fsdecode(path) for path, _ in identities(tree)]
            if mode == 'prior-replay-inside-open':
                state = pathlib.Path(sys.argv[2])
                (state / 'frecency/data.mdb').write_text('deliberate late replay')
            emit(dict(t='ready', n=len(identities(tree)), build_ms=1, has_git=False,
                      prior_replay_inside_open=False))
        elif fields[0] == 'P':
            emit(dict(t='pong'))
        elif fields[0] == 'Q':
            gen, limit = int(fields[1]), int(fields[2])
            if fields[4].startswith('audit:'):
                paths, count = answer(fields[5], limit)
                target = fields[4][6:]
                rank = paths.index(target) + 1 if target in paths else 0
                emit(dict(t='rank', gen=gen, rank=rank, n=count, complete=True))
            else:
                paths, count = answer(fields[5], limit)
                emit(dict(t='result', gen=gen, paths=paths, n=None if mode in {'short-head-no-total', 'valid-no-total'} else count,
                          core_ms=1, comparable_core_ms=1))
        else:
            raise ValueError(fields[0])

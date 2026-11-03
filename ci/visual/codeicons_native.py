"""Language-mark cases in the native GPUI scene suite: all 35 logos in List and Grid, beside a generic page at rest."""
import json
import os
from pathlib import Path
import time

import codeicons
from blackgate import read_image

# One specimen per logo in ui/js/LangMarks.js; the extension and basename tables are unit-tested in row/language_tests.rs.
SPECIMENS = {
    'c': 'sample.c', 'cmake': 'cmakelists.txt', 'cplusplus': 'sample.cpp', 'css': 'sample.css',
    'docker': 'dockerfile', 'fishshell': 'sample.fish', 'git': '.gitignore', 'gnu': 'makefile',
    'gnubash': 'sample.sh', 'go': 'sample.go', 'html5': 'sample.html', 'javascript': 'sample.js',
    'json': 'sample.json', 'kotlin': 'sample.kt', 'lua': 'sample.lua', 'markdown': 'sample.md',
    'nixos': 'sample.nix', 'openjdk': 'sample.java', 'perl': 'sample.pl', 'php': 'sample.php',
    'python': 'sample.py', 'qt': 'sample.qml', 'react': 'sample.tsx', 'ruby': 'sample.rb',
    'rust': 'sample.rs', 'sass': 'sample.scss', 'sqlite': 'sample.sql', 'swift': 'sample.swift',
    'toml': 'sample.toml', 'typescript': 'sample.ts', 'vuedotjs': 'sample.vue', 'xml': 'sample.xml',
    'yaml': 'sample.yaml', 'zig': 'sample.zig', 'zsh': 'sample.zsh',
}
# Both sort before every specimen; the launch cursor takes the first row, so the control and every logo rest unmarked.
# The headless lane has no keyboard (send_shortcut: no keyboard), so selected and cut inks are proven natively.
CURSOR, CONTROL = '.0-cursor.ini', '.1-ink.ini'
# Columns has no row rectangle reader under GPUI yet (FLEA_IPC_NOT_PORTED flea.rowRect); its marks are proven natively.
VIEWS = ('list', 'grid')
# Grid answers empty row names until its first paint, so the rows are judged once, from that first painted frame.
FIRST_PAINT_POLLS, FIRST_PAINT_STEP = 100, .05


def mark_box(call, window, index, view):
    if view == 'grid':
        box = call(window, 'rowThumbRect', index)
        codeicons.require(box, 'grid mark slot must be exported by the drawn tile')
        return [int(round(float(n))) for n in box.split()]
    x, y, width, height = (int(round(float(n))) for n in call(window, 'rowRect', index).split())
    return [x + 14, y + (height - 23) // 2, 23, 23]


def run(root, logs, Window, call, wait, grab, command, windows):
    languages = sorted(SPECIMENS)
    codeicons.require(len(languages) == 35, 'all 35 language logos are required')
    dispatch = ['bash', str(Path(__file__).resolve().parents[2] / 'tests/lib/hypr-dispatch.sh')]
    receipts = []
    groups = [languages[offset:offset + 7] for offset in range(0, 35, 7)]
    for offset, group in zip(range(0, 35, 7), groups):
        directory = root / 'codeicons' / str(offset)
        directory.mkdir(parents=True)
        for name in (CURSOR, CONTROL):
            (directory / name).write_text('generic page control\n')
        for slug in group:
            (directory / SPECIMENS[slug]).write_text('codeicons fixture\n')
    # One state write per view: every window of a view shares it, and fewer writes race fewer window exits.
    for view in VIEWS:
        state = dict(view=view, density='compact', langMarks=True, hidden=True,
                     display=dict(textSize=dict(mode=14)), preview=dict(thumbnails='off', thumbSize='large', column=False))
        command(os.environ['FLEA_BIN'], '--ui-state', json.dumps(state))
        command(*dispatch, 'cursor_move', '2559', '1439')
        for offset, group in zip(range(0, 35, 7), groups):
            directory = root / 'codeicons' / str(offset)
            rows = len(group) + 2
            window = Window.start(directory, parent=root, frontend='gpui', log=logs / f'codeicons-{offset}-{view}.log', timeout=15)
            windows.append(window)
            wait(window, 'total', str(rows))
            wait(window, 'pendingRequests', '0')
            wait(window, 'viewMode', view)
            for attempt in range(FIRST_PAINT_POLLS):
                if call(window, 'visibleRowName', 0):
                    break
                time.sleep(FIRST_PAINT_STEP)
            else:
                raise ValueError(f'codeicons-{offset}-{view}: no row painted after {FIRST_PAINT_POLLS} polls')
            names = [call(window, 'visibleRowName', index) for index in range(rows)]
            codeicons.require(sorted(names) == sorted([CURSOR, CONTROL] + [SPECIMENS[s] for s in group]), f'unexpected rows {names}')
            codeicons.require(names[0] == CURSOR and call(window, 'cursor') == '0', f'generic cursor row must lead: {names}')
            image = logs / f'codeicons-{offset}-{view}.png'
            time.sleep(.2)
            grab(window, image)
            pixels = read_image(image)
            generic = codeicons.crop(pixels, mark_box(call, window, names.index(CONTROL), view))
            expected = codeicons.ink(generic)[1]
            for slug in group:
                box = mark_box(call, window, names.index(SPECIMENS[slug]), view)
                mark = codeicons.crop(pixels, box)
                score = codeicons.check(mark, generic, expected)
                if slug == 'rust':
                    codeicons.controls(mark, generic, expected)
                receipts.append(dict(language=slug, view=view, state='rest', png=image.name, rect=box, **score))
            print(f'CODEICONS_NATIVE PASS view={view} offset={offset} languages={len(group)}', flush=True)
            # A graceful quit can outlast the drain deadline on a loaded lane; these windows hold no state worth saving.
            window.stop()
            windows.remove(window)
    # Hand the later scenes the stock state these cases changed.
    command(os.environ['FLEA_BIN'], '--ui-state', json.dumps(dict(hidden=False, preview=dict(thumbnails='media', thumbSize='medium', column=True))))
    codeicons.distinct(receipts)
    (logs / 'codeicons-pixels.json').write_text(json.dumps([{k: v for k, v in r.items() if k != 'shape'} for r in receipts], indent=2) + '\n')
    codeicons.require(len(receipts) == 35 * len(VIEWS), 'missing language pixel receipts')
    print(f'CODEICONS_NATIVE PASS languages=35 views={len(VIEWS)} receipts={len(receipts)} distinct=true', flush=True)

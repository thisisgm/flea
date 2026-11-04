"""Behavioral PTY evidence for detail modes, calendar facts, sorting and clean terminal restoration."""
import json
import os
from pathlib import Path
import pwd
import sys
import time
from tui_pty import Session, fixture

binary, root = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
work, env = fixture(root)
# The Properties card's title and Modified label, from the English catalog the TUI draws them with.
catalog = json.loads((Path(__file__).resolve().parent.parent / 'i18n/en/flea.json').read_text())
card_title, modified = catalog['properties.title'], catalog['properties.modified']

with Session(binary, root, work, env) as terminal:
    wait, send, grid = terminal.wait, terminal.send, terminal.grid
    wait(lambda rows: '4 items' in rows[30] and 'A' in rows[2])
    start = time.monotonic()
    send(b'I')
    rows = wait(lambda rows: 'Detail column · Modified' in rows[30])
    assert all(row[61:77] == '2026-09-23 10:14' for row in rows[2:6]), rows[2:6]
    assert 'Name ▴' in rows[0]
    actual_data = terminal.data
    terminal.data += b'\x1b[H\x1b[1;1Hincomplete'
    assert 'Detail column' in grid()[30], 'a partial frame erased a proven receipt'
    terminal.data = actual_data
    wait(lambda rows: 'Detail column' not in rows[30])
    elapsed = time.monotonic() - start
    assert 3.7 <= elapsed <= 8, elapsed
    print(f'ok calendar rows and four-second receipt, observed {elapsed:.3f}s')
    send(b'I')
    rows = wait(lambda rows: 'Detail column · Mode' in rows[30])
    assert rows[2][73:77] == '0755' and rows[4][73:77] == '0644', rows[2:6]
    send(b'I')
    expected_owner = pwd.getpwuid(os.getuid()).pw_name
    wait(lambda rows: 'Detail column · Owner' in rows[30] and all(expected_owner in row[60:77] for row in rows[2:6]))
    send(b'I')
    wait(lambda rows: 'Detail column · Size' in rows[30] and '5 B' in rows[5])
    send(b's')
    rows = wait(lambda rows: 'Size ▴' in rows[0] and 'small.txt' in rows[4])
    assert rows[2][31:32] == 'A' and rows[3][31:32] == 'Z', rows[2:6]
    assert 'small.txt' in rows[4] and 'large.txt' in rows[5]
    send(b'S')
    rows = wait(lambda rows: 'Size ▾' in rows[0] and 'large.txt' in rows[4])
    assert rows[2][31:32] == 'A' and rows[3][31:32] == 'Z', rows[2:6]
    send(b'i')
    # The card's title row, then its Modified row: the label, the label column's padding, the local date.
    card_row = lambda row, text: row.split('│')[-2].strip().startswith(text) if row.count('│') >= 3 else False
    wait(lambda rows: any(card_row(row, card_title) for row in rows)
         and any(card_row(row, modified) and row.split('│')[-2].split()[1:3] == ['2026-09-23', '10:14'] for row in rows))
    send(b'\x1b')
    wait(lambda rows: not any(card_row(row, card_title) for row in rows))
    send(b'q')
    terminal.finish()
    print('PASS tui-detail: four modes, local dates, properties, folder-name size sorting, clean TTY quit')

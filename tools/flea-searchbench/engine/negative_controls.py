"""Small executable adapter controls, run before expensive collection."""
import argparse
import json
import pathlib
import subprocess
import sys
import xml.etree.ElementTree as ET

from contracts import checked_result, classify_file_rank, classify_rank, identities, inventory_report, quality_reason
from fixtures import marked_root
from measure import Measurements
from priors import RICH_OPENS, replay


def adapter(root, mode):
    observed = [(path.hex(), kind) for path, kind in identities(root)]
    if mode == 'drop':
        observed.pop()
    paths = ['target.rs', 'other.rs', 'third.rs']
    if mode == 'misrank':
        paths.reverse()
    return dict(t='result', gen=6 if mode == 'stale' else 7, paths=paths,
                identities=observed, n=3, capped=mode == 'cap', core_ms=0.001)


def run_controls(parent):
    root = marked_root(parent, 'negative-controls-')
    tree = root / 'tree'
    tree.mkdir()
    for name in ['target.rs', 'other.rs', 'third.rs']:
        (tree / name).touch(exist_ok=False)
    expected = identities(tree)
    report = {}
    for mode in ['valid', 'drop', 'cap', 'stale', 'misrank']:
        reply = subprocess.run([sys.executable, '-B', __file__, '--adapter', mode, '--tree', str(tree)],
                               check=True, capture_output=True, text=True, timeout=5)
        result = json.loads(reply.stdout)
        observed = [(bytes.fromhex(path), kind) for path, kind in result['identities']]
        inventory = inventory_report(expected, observed, result)
        reasons = list(inventory['reasons'])
        try:
            checked_result(result, 7, 350)
        except ValueError as error:
            reasons.append(str(error))
        rank = quality_reason('exact', result['paths'], 'target.rs')
        if rank:
            reasons.append(rank)
        accepted = not reasons
        if accepted != (mode == 'valid'):
            raise AssertionError(f'negative control {mode} incorrectly accepted={accepted}')
        output = marked_root(root, mode + '-')
        rows = Measurements(output)
        context = dict(phase='measured', run=0, fixture='control', engine='invalid-adapter', profile=mode)
        rows.row(context, 'native_core_ms', 0.001, 'ms')
        for reason in reasons:
            rows.invalidate(context, reason)
        rows.finish(True)
        import csv
        with (output / 'performance.tsv').open() as stream:
            included = list(csv.DictReader(stream, delimiter='\t'))
        if bool(included) != accepted:
            raise AssertionError('ineligible performance sample leaked')
        report[mode] = dict(accepted=accepted, reasons=reasons)
    head, folders = ['src', 'src/target.rs', 'other.rs'], frozenset(['src'])
    if classify_rank(head, 'src/target.rs') != (2, 'ok') or classify_file_rank(head, folders, 'src/target.rs') != (1, 'ok'):
        raise AssertionError('file rank control: a folder ahead of the target must count in the mixed rank only')
    if classify_file_rank(head, folders, 'gone.rs', [], complete=True) != (0, 'miss'):
        raise AssertionError('file rank control: an absent target must stay a miss')
    if classify_file_rank(['other.rs'], folders, 'src/target.rs', ['other.rs', 'src', 'src/target.rs']) != (-1, 'censored'):
        raise AssertionError('file rank control: a target past the head must stay censored whatever folders the audit holds')
    try:
        classify_file_rank(head, folders, 'src')
    except ValueError:
        pass
    else:
        raise AssertionError('file rank control: a folder target must fail loud, never rank as a miss')
    report['file-rank'] = dict(accepted=True, reasons=[])
    state = root / 'rich-replay'
    (state / 'data').mkdir(parents=True)
    rich = replay(state, tree, expected, ['target.rs'], 'rich', dict(XDG_DATA_HOME=str(state / 'data')))
    if rich['replay'] != ['target.rs'] or rich['held_out']:
        raise AssertionError(f"rich replay control: the rich profile must replay the targets and hold none out, got replay={rich['replay']} held_out={rich['held_out']}")
    xbel = state / 'data' / 'recently-used.xbel'
    application = '{http://www.freedesktop.org/standards/desktop-bookmarks}application'
    try:
        # <bookmark href="file:///.../target.rs"><info><metadata owner="..."><bookmark:applications><bookmark:application name="findbench-replay" count="3"/>
        opens = [(mark.get('href'), [app.get('count') for app in mark.iter(application)])
                 for mark in ET.parse(xbel).getroot().iter('bookmark')]
    except (OSError, ET.ParseError) as error:
        raise AssertionError(f'rich replay control: cannot read {xbel}: {error}') from error
    if opens != [((tree / 'target.rs').as_uri(), [str(RICH_OPENS)])]:
        raise AssertionError(f'rich replay control: {xbel} must hold one bookmark per target with {RICH_OPENS} opens, got {opens}')
    report['rich-replay'] = dict(accepted=True, reasons=[])
    from production_controls import run_production_controls
    report.update(run_production_controls(root))
    (root / 'controls.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--adapter', choices=['valid', 'drop', 'cap', 'stale', 'misrank'])
    parser.add_argument('--tree', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path)
    args = parser.parse_args()
    if args.adapter:
        print(json.dumps(adapter(args.tree, args.adapter)))
    else:
        if not args.output or not args.output.is_absolute():
            parser.error('absolute --output required')
        print(json.dumps(run_controls(args.output), indent=2))

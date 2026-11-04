"""Execute path and cold publication regressions without containers or cache drops."""
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from types import SimpleNamespace
from unittest import mock

REPO = Path(__file__).resolve().parents[1]
HERE = REPO / 'tools/flea-searchbench'
sys.path.insert(0, str(HERE))
import cold


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def remove_scratch(root):
    parent = (REPO / '.superpowers/m9-fs-pathcold-fixes').resolve(strict=True)
    path = Path(root)
    if not str(root) or not path.is_absolute() or path.is_symlink():
        raise ValueError('absolute nonsymlink scratch path required')
    if path != path.resolve(strict=True) or path.parent != parent or not path.name.startswith('case-') or len(path.name) <= 5:
        raise ValueError('owned case scratch root required')
    marker = path / '.findbench-root'
    if marker.is_symlink() or not marker.is_file() or marker.read_text() != 'instrument regression\n':
        raise ValueError('owned scratch marker required')
    shutil.rmtree(path)


def scratch(case):
    parent = REPO / '.superpowers/m9-fs-pathcold-fixes'
    parent.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='case-', dir=parent)).resolve()
    (root / '.findbench-root').write_text('instrument regression\n')
    case.addCleanup(remove_scratch, root)
    return root


GIT_STUB = '''#!/bin/bash
set -eu
tree=$2; shift 2
case $1 in
 archive) cat "$TEST_GIT_ARCHIVE" ;;
 rev-parse)
  value=${!#}
  case $value in
   *'^{commit}') printf '%s\\n' "${value:0:40}" ;;
   *'^{tree}') printf '%040d\\n' 0 ;;
   *)
    while IFS=$'\\t' read -r location sha; do
     if [[ $location = "$tree" ]]; then printf '%s\\n' "$sha"; exit 0; fi
    done < "$TEST_GIT_PINS_FILE"
    printf '%s\\n' "$TEST_GIT_HEAD" ;;
  esac ;;
 diff|status|ls-files) exit 0 ;;
 *) exit 2 ;;
esac
'''

PODMAN_STUB = '''#!/usr/bin/env python3
import json,os,pathlib,sys
a=sys.argv[1:]
if a[0]=='build': sys.exit(0)
if a[:2]==['image','inspect']: print('test-image'); sys.exit(0)
if a[0]!='run': sys.exit(2)
mounts=[]
for i,v in enumerate(a):
 if v=='--volume':
  src,dst,mode=a[i+1].rsplit(':',2); mounts.append((src,dst))
script=a[a.index('/bin/bash')+1]
matches=[(src,dst) for src,dst in mounts if script==dst or script.startswith(dst+'/')]
if not matches: sys.exit(3)
src,dst=max(matches,key=lambda pair:len(pair[1]))
mapped=pathlib.Path(src)/script[len(dst):].lstrip('/')
record=dict(script=script,mapped=str(mapped),exists=mapped.is_file())
with open(os.environ['TEST_PODMAN_LOG'],'a') as out: out.write(json.dumps(record)+'\\n')
if not mapped.is_file(): print('missing mapped script: '+str(mapped),file=sys.stderr); sys.exit(4)
'''


class EntryPaths(unittest.TestCase):
    def run_entry(self, name):
        root = scratch(self)
        unit = root / 'tools/flea-searchbench'
        shutil.copytree(HERE / 'engine', unit / 'engine', ignore=shutil.ignore_patterns('__pycache__'))
        bins = root / 'bin'
        bins.mkdir()
        for label, text in [('git', GIT_STUB), ('podman', PODMAN_STUB)]:
            path = bins / label
            path.write_text(text)
            path.chmod(0o700)
        sources, current = root / 'sources', root / 'current'
        historical = {'fff':'d978416f77aec3f9928d56e00c9e56ba39ca1fda',
                      'fsearch':'cb6cd67f9ef12fedcf6a160b6a38faf492f7c989',
                      'fsearch-nd':'88e63fdf850cf7749deae8198b187accd21f8ba5'}
        pins = {}
        for parent, values in [(sources, historical), (current, dict(line.split('\t') for line in (unit / 'engine/rival-pins.tsv').read_text().splitlines()))]:
            for label, commit in values.items():
                path = parent / label
                path.mkdir(parents=True)
                pins[str(path)] = commit
        (root / 'pins.tsv').write_text(''.join(path + '\t' + commit + '\n' for path, commit in pins.items()))
        with tarfile.open(root / 'empty.tar', 'w'):
            pass
        if name == 'run.sh':
            build = unit / 'build.test'
            build.mkdir()
            for label in ['.findbench-root', 'BUILD-COMPLETE', 'image-id']:
                (build / label).write_text('test\n')
            provenance = module('path_provenance', HERE / 'engine/provenance.py')
            (build / 'harness-sha256.json').write_text(json.dumps(provenance.hashes(unit / 'engine')))
        env = dict(os.environ, PATH=str(bins) + os.pathsep + os.environ.get('PATH', '/usr/bin:/bin'),
                   FINDBENCH_SOURCES=str(sources), FINDBENCH_CURRENT_SOURCES=str(current),
                   TEST_GIT_PINS_FILE=str(root / 'pins.tsv'), TEST_GIT_ARCHIVE=str(root / 'empty.tar'),
                   TEST_GIT_HEAD='0ebbcffae14609a3663c90db6d5f73c5e86c97a4',
                   TEST_PODMAN_LOG=str(root / 'podman.jsonl'), PYTHONDONTWRITEBYTECODE='1')
        done = subprocess.run(['/bin/bash', str(unit / 'engine' / name)], env=env, capture_output=True, text=True, timeout=10)
        output = done.stdout + done.stderr
        for log in [*unit.glob('build.*/build.log'), *unit.glob('run.*/run.log')]:
            output += '\n' + log.name + ':\n' + log.read_text()
        (root / 'entry.log').write_text(output)
        self.assertEqual(done.returncode, 0, output)
        records = [json.loads(line) for line in (root / 'podman.jsonl').read_text().splitlines()]
        self.assertEqual(len(records), 1)
        self.assertTrue(records[0]['exists'], records[0])
        self.assertEqual(Path(records[0]['mapped']), unit / 'engine' / name)

    def test_build_entry_maps_to_existing_script(self):
        self.run_entry('build.sh')

    def test_run_entry_maps_to_existing_script(self):
        self.run_entry('run.sh')


class PollStopped(Exception):
    pass


class MatcherMetadata(unittest.TestCase):
    def test_contract_and_roster_match_production_reference(self):
        import csv
        runner = module('metadata_matcher', REPO / 'tools/flea-matchbench/runner.py')
        contract = json.loads((REPO / 'tools/flea-matchbench/contract.json').read_text())
        self.assertEqual(contract['reference'], '/'.join(runner.REFERENCE))
        self.assertEqual(set(contract['entrants']), {'/'.join(value) for value in runner.ENTRANTS})
        with (REPO / 'tools/flea-matchbench/ROSTER.tsv').open() as stream:
            rows = {row['entrant']:row for row in csv.DictReader(stream, delimiter='\t')}
        adapted = {(name, profile) for name, row in rows.items() if row['status'].startswith('adapted')
                   for profile in row['profiles'].split(',')}
        self.assertEqual(adapted, set(runner.ENTRANTS))
        self.assertIn('reference', rows['flea-search']['status'])
        self.assertNotIn('reference', rows['flea-current']['status'])


class ColdPublication(unittest.TestCase):
    def test_request_is_invisible_during_json_write(self):
        root = scratch(self)
        helper = module('publication_helper', HERE / 'cold-controller.py')
        dump = json.dump
        def paused_write(payload, out, **kwargs):
            self.assertEqual(list(root.glob('cache-request-*.json')), [])
            with mock.patch.object(sys, 'argv', ['cold-controller.py', str(root)]), mock.patch.object(helper.time, 'sleep', side_effect=PollStopped):
                with self.assertRaises(PollStopped):
                    helper.main()
            dump(payload, out, **kwargs)
        with mock.patch.object(cold.clocks.json, 'dump', side_effect=paused_write):
            request = cold.clocks.cold_request(root, {'run':1})
        self.assertEqual(json.loads(request.read_text())['context'], {'run':1})

    def test_request_collision_never_replaces_existing_payload(self):
        root = scratch(self)
        identifier = SimpleNamespace(hex='a'*32)
        with mock.patch.object(cold.clocks.uuid, 'uuid4', return_value=identifier):
            request = cold.clocks.cold_request(root, {'run':1})
            original = request.read_bytes()
            with self.assertRaises(FileExistsError):
                cold.clocks.cold_request(root, {'run':2})
        self.assertEqual(request.read_bytes(), original)

    def test_request_collision_during_write_preserves_other_payload(self):
        root = scratch(self)
        identifier = SimpleNamespace(hex='b'*32)
        request = root / ('cache-request-' + identifier.hex + '.json')
        dump = json.dump
        def raced_write(payload, out, **kwargs):
            dump(payload, out, **kwargs)
            request.write_text('existing payload')
        with mock.patch.object(cold.clocks.uuid, 'uuid4', return_value=identifier), mock.patch.object(cold.clocks.json, 'dump', side_effect=raced_write):
            with self.assertRaises(FileExistsError):
                cold.clocks.cold_request(root, {'run':1})
        self.assertEqual(request.read_text(), 'existing payload')

    def test_request_collision_at_publication_never_overwrites(self):
        root = scratch(self)
        identifier = SimpleNamespace(hex='e'*32)
        request = root / ('cache-request-' + identifier.hex + '.json')
        replace, link = os.replace, os.link
        def raced(operation):
            def publish(source, destination):
                Path(destination).write_text('existing payload')
                return operation(source, destination)
            return publish
        with mock.patch.object(cold.clocks.uuid, 'uuid4', return_value=identifier), mock.patch.object(cold.clocks.os, 'replace', side_effect=raced(replace)), mock.patch.object(cold.clocks.os, 'link', side_effect=raced(link)):
            with self.assertRaises(FileExistsError):
                cold.clocks.cold_request(root, {'run':1})
        self.assertEqual(request.read_text(), 'existing payload')

    def retry_partial(self, legacy):
        root = scratch(self)
        helper = module('partial_helper_' + str(legacy), HERE / ('engine/cold-controller.py' if legacy else 'cold-controller.py'))
        host = 'minipc' if legacy else 'ultra-36'
        identifier = 'c'*32
        request = root / ('cache-request-' + identifier + '.json')
        request.write_text('{"action":')
        payload = dict(action='drop-page-cache', mode='cold', id=identifier, host=host,
                       context={'run':3}, requested_ns=time.monotonic_ns())
        polls = []
        def complete(_):
            if (root / 'completion.json').exists():
                return
            polls.append(1)
            if len(polls) == 2:
                request.write_text(json.dumps(payload))
            if len(polls) > 2:
                raise AssertionError('reader did not recover')
        def cache_clear(*args, **kwargs):
            (root / 'completion.json').write_text('{}')
            return SimpleNamespace(returncode=0, stderr='')
        stderr = io.StringIO()
        with mock.patch.object(sys, 'argv', ['cold-controller.py', str(root)]), mock.patch.object(helper.socket, 'gethostname', return_value=host), mock.patch.object(helper.time, 'sleep', side_effect=complete), mock.patch.object(helper.subprocess, 'run', side_effect=cache_clear) as run, mock.patch('sys.stdout', new=io.StringIO()), mock.patch('sys.stderr', new=stderr):
            self.assertEqual(helper.main(), None if legacy else 0)
        run.assert_called_once()
        self.assertTrue(request.with_name(request.name.replace('request', 'response')).is_file())
        self.assertEqual(len(polls), 2)
        self.assertEqual(stderr.getvalue().count(str(request)), 1)

    def test_current_controller_retries_partial_json(self):
        self.retry_partial(False)

    def test_historical_controller_retries_partial_json(self):
        self.retry_partial(True)

    def test_current_controller_still_refuses_wrong_host(self):
        root = scratch(self)
        helper = module('wrong_host_helper', HERE / 'cold-controller.py')
        request = root / ('cache-request-' + 'd'*32 + '.json')
        request.write_text(json.dumps(dict(action='drop-page-cache', mode='cold', id='d'*32,
                                           host='wrong', context={}, requested_ns=time.monotonic_ns())))
        with mock.patch.object(sys, 'argv', ['cold-controller.py', str(root)]), mock.patch.object(helper.subprocess, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'invalid cold request identity'):
                helper.main()
        run.assert_not_called()


if __name__ == '__main__':
    unittest.main(verbosity=2)

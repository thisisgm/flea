"""Executable fake peers exercise the whole collector; output is never benchmark evidence."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
from unittest import mock

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / 'tools/flea-searchbench'))
import campaign
if os.environ.get('FLEA_CONTROL_CAMPAIGN'):
    import importlib.util
    spec = importlib.util.spec_from_file_location('controlled_campaign_base', os.environ['FLEA_CONTROL_CAMPAIGN'])
    campaign = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(campaign)
    campaign.HERE = REPO / 'tools/flea-searchbench'
    campaign.REPO = REPO
from corpus import identity_digest

FAKE = r'''#!/usr/bin/env python3
import hashlib,json,os,sys

def say(row):
 print(json.dumps(row),flush=True)
raw=b'entry.rs';digest=hashlib.sha256(len(raw).to_bytes(8,'little')+raw+b'f').hexdigest()
args=sys.argv[1:]
if args[:2]==['--search-stage','walk']:
 say(dict(elapsed_ns=1000,files=1,dirs=0,errors=0,pss=1234,identities=[[raw.hex(),'f']]))
 sys.exit(0)
if args and args[0]=='adapter' and '--tree' in args:
 print('RESULT\t1000\t1\t0\t0\t'+digest+'\t1\t1\t0\t0')
 sys.exit(0)
flea=bool(args and args[0]=='--backend')
if not flea: say(dict(t='boot',cpus=2,capabilities={}))
for line in sys.stdin:
 if flea:
  row=json.loads(line);command=row['c'];gen=row.get('gen',0)
  if command in ['list','window']:say(dict(t='rows',rows=[]))
  elif command=='searchopen':say(dict(t='searchready',n=1,ms=.001,capped=False))
  elif command=='benchinventory':say(dict(t='inventory',identities=[[raw.hex(),'f']]))
  elif command=='benchaudit':say(dict(t='rank',gen=gen,rank=1,n=1,complete=True))
  elif command=='searchquery':
   say(dict(t='searchcore',gen=gen,matcher_ms=.001,answer_ms=.001))
   say(dict(t='listed',gen=gen))
   say(dict(t='rows',gen=gen,rows=[dict(n='entry.rs')]))
   say(dict(t='searched',gen=gen,n=1,walking=False))
 else:
  fields=line.rstrip('\n').split('\t');command=fields[0]
  if command=='O':say(dict(t='ready',n=1,build_ms=.001,build_cpu_ns=1000))
  elif command=='I':say(dict(t='inventory',identities=[[raw.hex(),'f']]))
  elif command=='Q':
   gen=int(fields[1])
   if fields[4].startswith('audit:'):say(dict(t='rank',gen=gen,rank=1,n=1,complete=True))
   else:
    bad=os.path.exists(os.path.join(os.path.dirname(__file__),'BAD-HEAD'))
    say(dict(t='result',gen=gen,n=500 if bad else 1,paths=['entry.rs'],core_ms=.001,comparable_core_ms=.001,cpu_ns=1000))
  elif command=='A':say(dict(t='audit',gen=int(fields[1]),n=1,rank=1))
  elif command=='P':say(dict(t='pong'))
  elif command=='X':break
'''


def main():
    parent = REPO / '.superpowers/m9-fs-instrument'
    parent.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='executable-control-', dir=parent)).resolve()
    (root / '.findbench-root').write_text('test\n')
    (root / 'CONTROL-ONLY').write_text('Synthetic peers, no benchmark claims.\n')
    tree = root / 'tree'
    tree.mkdir()
    (tree / 'entry.rs').write_text('test\n')
    paths = root / 'paths.txt'
    paths.write_text('entry.rs\n')
    manifest = [['entry.rs', 'f']]
    (root / 'manifest.json').write_text(json.dumps(manifest))
    fixture = dict(tree=str(tree), fixture=str(root), paths=str(paths), queries=[['exact','entry.rs','entry.rs']],
                   info={'entries':1}, identity_sha256=identity_digest(manifest), paths_sha256=hashlib.sha256(paths.read_bytes()).hexdigest())
    (root / 'registry.json').write_text(json.dumps({'flea-source':fixture}))
    binary = root / 'fake'
    binary.write_text(FAKE)
    binary.chmod(0o755)
    target = root / 'target/release'
    target.mkdir(parents=True)
    (target / 'fsearch-driver').write_text(FAKE)
    (target / 'fsearch-driver').chmod(0o755)
    args = SimpleNamespace(engine_build=root, matcher_bin=binary, walker_bin=binary, flea_bin=binary,
                           corpus_root=root, fixtures='flea-source', runs=1, stages='engine,match,walk',
                           cache='warm', fzf=None, fd=None, rg=None)
    original_import = campaign.imported
    def imported(name, path):
        module = original_import(name, path)
        if name == 'matchbench_runner':
            module.ENTRANTS = [('flea-search','native'),('nucleo','native')]
        return module
    with mock.patch.object(campaign, 'ENGINE_ROSTER', ['flea/native','fsearch/native']), \
         mock.patch.object(campaign, 'WALK_ROSTER', ['flea-search','std-serial']), \
         mock.patch.object(campaign, 'imported', side_effect=imported), \
         mock.patch('priors.prepare', return_value=root):
        code = campaign.run(args, root)
        if code:
            raise AssertionError('normal executable control failed')
    records = [json.loads(line) for line in (root / 'runs.jsonl').read_text().splitlines()]
    if len(records) != 66 or any(r['status'] != 'ok' or not r['eligible'] for r in records):
        raise AssertionError('collector did not complete all fake peers: ' + str(root))
    for stage in ['engine','match','walk']:
        orders = {tuple(r['batch']['order']) for r in records if r['stage'] == stage}
        if len(orders) != 2:
            raise AssertionError('A/B alternation missing: ' + stage)
    summary = json.loads((root / 'summary.json').read_text())
    if not summary or any(row['winner'] != 'unproven' for row in summary):
        raise AssertionError('control samples must never establish an accepted winner')
    (root / 'BAD-HEAD').touch()
    failed = Path(tempfile.mkdtemp(prefix='failed-control-', dir=parent)).resolve()
    (failed / '.findbench-root').write_text('test\n')
    with mock.patch.object(campaign, 'ENGINE_ROSTER', ['flea/native','fsearch/native']), \
         mock.patch.object(campaign, 'WALK_ROSTER', ['flea-search','std-serial']), \
         mock.patch.object(campaign, 'imported', side_effect=imported), \
         mock.patch('priors.prepare', return_value=root):
        code = campaign.run(args, failed)
    if code != 2 or json.loads((failed / 'completion.json').read_text())['complete']:
        raise AssertionError('blocked collection was certified as complete')
    cold_failed = Path(tempfile.mkdtemp(prefix='cold-failed-control-', dir=parent)).resolve()
    (cold_failed / '.findbench-root').write_text('test\n')
    from cold import clocks
    with mock.patch.object(campaign, 'ENGINE_ROSTER', ['flea/native','fsearch/native']), \
         mock.patch.object(campaign, 'WALK_ROSTER', ['flea-search','std-serial']), \
         mock.patch.object(campaign, 'imported', side_effect=imported), \
         mock.patch('priors.prepare', return_value=root), \
         mock.patch('metrics.cache_gate', side_effect=clocks.ColdGateError('controlled denial')) as gate:
        try:
            campaign.run(args, cold_failed)
            raise AssertionError('cold failure was swallowed')
        except clocks.ColdGateError:
            pass
        if gate.call_count != 1:
            raise AssertionError('cold failure started another pair')
    external = Path(tempfile.mkdtemp(prefix='external-cold-control-', dir=parent)).resolve()
    (external / '.findbench-root').write_text('test\n')
    args.cache = 'cold'
    args.external_cold_helper = True
    original_popen = campaign.subprocess.Popen
    def no_internal_helper(argv, *positional, **keywords):
        if any('cold-controller.py' in str(arg) for arg in argv):
            raise AssertionError('external mode started an internal cache controller')
        return original_popen(argv, *positional, **keywords)
    with mock.patch.object(campaign, 'ENGINE_ROSTER', ['flea/native','fsearch/native']), \
         mock.patch.object(campaign, 'WALK_ROSTER', ['flea-search','std-serial']), \
         mock.patch.object(campaign, 'imported', side_effect=imported), \
         mock.patch('priors.prepare', return_value=root), \
         mock.patch('metrics.cache_gate', side_effect=clocks.ColdGateError('controlled external denial')), \
         mock.patch.object(campaign.subprocess, 'Popen', side_effect=no_internal_helper):
        try:
            campaign.run(args, external)
            raise AssertionError('external cold failure was swallowed')
        except clocks.ColdGateError:
            pass
    print('fs-instrument executable campaign: PASS ' + str(root))


if __name__ == '__main__':
    main()

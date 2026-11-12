"""Plan and verify contract of tools/flea-i18n-native, run without a display or a candidate."""
import copy
import hashlib
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

REPO = Path(__file__).resolve().parents[2]
TOOL = REPO / 'tools/flea-i18n-native'
ROSTER = REPO / 'src/i18n/fixtures/locales.json'


def load_tool():
    loader = importlib.machinery.SourceFileLoader('flea_i18n_native', str(TOOL))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def sha(data):
    return hashlib.sha256(data).hexdigest()


class Scratch(unittest.TestCase):
    def setUp(self):
        parent = REPO / '.superpowers/m9-i50-27'
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix='native.', dir=parent))
        (self.root / '.flea-test-sandbox').write_text('i50-27 native driver test\n')
        self.addCleanup(self.remove_root)

    def remove_root(self):
        root = self.root
        if root.is_absolute() and root.name.startswith('native.') and (root / '.flea-test-sandbox').is_file():
            shutil.rmtree(root)

    def run_tool(self, *args):
        return subprocess.run([sys.executable, '-B', str(TOOL), *map(str, args)], capture_output=True, text=True, cwd=REPO)

    def plan(self, name='plan.json'):
        out = self.root / name
        done = self.run_tool('plan', '--languages', 'all', '--output', out)
        self.assertEqual(done.returncode, 0, done.stderr)
        return json.loads(out.read_text())


class Evidence(Scratch):
    """A synthetic complete plan and capture, so each refusal is the only defect in its input."""

    def setUp(self):
        super().setUp()
        self.tool = load_tool()
        self.catalogs = self.root / 'i18n'
        roster = json.loads(ROSTER.read_text())
        self.codes = sorted(roster)
        for code in self.codes:
            (self.catalogs / code).mkdir(parents=True)
            text = {'menu.open': 'Open' if code == 'en' else f'Open {code}', 'menu.cancel': 'Cancel'}
            (self.catalogs / code / 'flea.json').write_text(json.dumps(text, ensure_ascii=False))
        self.keymap = self.root / 'keys.toml'
        self.keymap.write_text('[keys]\n')
        self.index = self.root / 'source-index.json'
        self.index.write_text('{}\n')
        self.candidate = self.root / 'flea'
        self.candidate.write_bytes(b'candidate binary')
        self.plan_doc = {
            'schema': self.tool.PLAN_SCHEMA, 'scope': 'all', 'complete': True, 'owed': [],
            'roster': {'codes': self.codes}, 'source_commit': 'a' * 40,
            'source_index': {'sha256': sha(self.index.read_bytes())},
            'cases': [dict(id=f'catalog.{code}.gui', kind='catalog', frontend='gui', language=code,
                           status='planned', required_keys=['menu.open'])
                      for code in self.codes],
        }
        records = []
        for frame, code in enumerate(self.codes, 1):
            value = json.loads((self.catalogs / code / 'flea.json').read_text())['menu.open']
            records.append(dict(case=f'catalog.{code}.gui', language=code, frontend='gui',
                                catalog_sha256=sha((self.catalogs / code / 'flea.json').read_bytes()),
                                receipt=dict(case=f'catalog.{code}.gui', window=7, frame=frame, presented=True),
                                texts=[dict(key='menu.open', text=value)]))
        self.capture = {
            'schema': self.tool.CAPTURE_SCHEMA,
            'candidate': dict(path=str(self.candidate), sha256=sha(self.candidate.read_bytes())),
            'source_commit': 'a' * 40, 'keymap_sha256': sha(self.keymap.read_bytes()),
            'source_index_sha256': sha(self.index.read_bytes()), 'records': records,
        }

    def errors(self, plan=None, capture=None, candidate=None):
        return self.tool.check_evidence(plan or self.plan_doc, capture or self.capture, catalog_root=self.catalogs,
                                        keymap=self.keymap, source_index=self.index,
                                        candidate=self.candidate if candidate is None else candidate)

    def assertRefused(self, needle, **kwargs):
        errors = self.errors(**kwargs)
        self.assertTrue(any(needle in error for error in errors), (needle, errors))


class Plan(Scratch):
    def test_i50_native_all_codes_plan(self):
        plan = self.plan()
        roster = json.loads(ROSTER.read_text())
        self.assertEqual(plan['scope'], 'all')
        self.assertEqual(sorted(plan['roster']['codes']), sorted(roster))
        self.assertEqual(len(plan['roster']['codes']), 50)
        self.assertEqual(plan['roster']['sha256'], sha(ROSTER.read_bytes()))
        self.assertEqual(plan['targets'], 49)
        catalog = {(case['language'], case['frontend']) for case in plan['cases'] if case['kind'] == 'catalog'}
        self.assertEqual(catalog, {(code, end) for code in roster for end in ('gui', 'tui')})
        self.assertEqual({case['kind'] for case in plan['cases']}, {'catalog', 'fit', 'ime', 'fonts', 'rtl', 'live'})
        rtl = {case['language'] for case in plan['cases'] if case['kind'] == 'rtl'}
        self.assertEqual(rtl, {code for code, entry in roster.items() if entry.get('direction') == 'rtl'})
        # Nothing whose producer has not handed off may read as planned work.
        self.assertFalse(plan['complete'])
        owed = {row['dependency'] for row in plan['owed']}
        # The source index, RTL policy, entry inventory, catalogs, TUI-16 callable, its private preparation and SCENE-LANG's language scenes are integrated; the rest stays owed by name.
        self.assertTrue({'TUI-RECEIPT', 'SCENES', 'I50-09', 'I50-28'} <= owed, owed)
        self.assertFalse({'I50-02', 'I50-04', 'I50-07', 'TUI-16', 'TUI-PREPARE', 'CATALOGS', 'SCENE-LANG'} & owed, owed)
        self.assertTrue(all(row['missing'] for row in plan['owed']))
        # Only a gui fit case whose spelled scene is registered is planned; every other case stays owed by name.
        registered = load_tool().registered_scenes()
        for case in plan['cases']:
            if case.get('scene') in registered:
                self.assertEqual((case['status'], case['missing']), ('planned', []), case['id'])
                continue
            self.assertEqual(case['status'], 'OWED', case['id'])
            self.assertTrue(set(case['missing']) <= owed and case['missing'], case['id'])
        self.assertEqual(len([case for case in plan['cases'] if case['status'] == 'planned']), 24)
        # The engines come only from I50-07's accepted inventory, never from a list in the contract.
        self.assertNotIn('ime_engines', json.loads((REPO / 'ci/visual/i50/contract.json').read_text()))
        # Font values and the live switch sequence are I50-09's and I50-28's to publish: one owed slot each, no stand-in list.
        for kind, ident, owner in (('fonts', 'fonts.font-inventory', 'I50-09'), ('live', 'live.switch-sequence', 'I50-28')):
            rows = [case for case in plan['cases'] if case['kind'] == kind]
            self.assertEqual([(case['id'], case['missing'][0], case['language']) for case in rows], [(ident, owner, None)])
            self.assertNotIn('sequence', rows[0])
        contract = json.loads((REPO / 'ci/visual/i50/contract.json').read_text())
        self.assertFalse({'font_languages', 'live_sequence'} & set(contract))

    def test_i50_native_plan_is_deterministic_and_refuses_unknown_inputs(self):
        first = self.plan()
        self.assertEqual(first, self.plan('again.json'))
        for args, needle in [(('plan', '--languages', 'de,xx', '--output', self.root / 'x.json'), 'unknown language xx'),
                             (('plan', '--languages', 'all', '--output', self.root / 'y.json', '--entry-inventory', ROSTER), 'I50-07'),
                             (('plan', '--languages', 'all', '--output', self.root / 'z.json', '--rtl-policy', ROSTER), 'I50-04'),
                             (('plan', '--languages', 'all', '--output', self.root / 'w.json', '--tui-candidate', ROSTER), 'TUI-16')]:
            done = self.run_tool(*args)
            self.assertNotEqual(done.returncode, 0, args)
            self.assertIn(needle, done.stderr)
            self.assertFalse(Path(args[4]).exists(), args)

    def test_i50_native_absent_english_catalog_is_owed(self):
        empty = self.root / 'no-catalogs'
        empty.mkdir()
        out = self.root / 'bare-plan.json'
        done = self.run_tool('plan', '--languages', 'en,de', '--output', out, '--catalog-root', empty)
        self.assertEqual(done.returncode, 0, done.stderr)
        plan = json.loads(out.read_text())
        self.assertIn('2 catalogs absent: en, de', [row['missing'] for row in plan['owed'] if row['dependency'] == 'CATALOGS'])
        self.assertIn('CATALOGS', next(c for c in plan['cases'] if c['id'] == 'catalog.en.gui')['missing'])
        # A registered scene never plans a fit case whose language has no catalog.
        tool = load_tool()
        contract, roster = tool.load_contract(tool.CONTRACT), tool.load_roster(ROSTER)
        inputs = tool.load_inputs(contract, roster)
        bare = tool.build_plan(contract, roster, ['en', 'de'], catalog_root=empty, inputs=inputs)
        fit = [case for case in bare['cases'] if case['kind'] == 'fit']
        self.assertTrue(any(case.get('scene') in inputs['scenes'] for case in fit))
        self.assertTrue(fit and all(case['status'] == 'OWED' and 'CATALOGS' in case['missing'] for case in fit))

    def test_i50_native_gui_cases_owe_registered_scenes(self):
        tool = load_tool()
        contract, roster = tool.load_contract(tool.CONTRACT), tool.load_roster(ROSTER)
        inputs = tool.load_inputs(contract, roster)
        plan = tool.build_plan(contract, roster, 'all', inputs=inputs)
        fit = [case for case in plan['cases'] if case['kind'] == 'fit' and case['frontend'] == 'gui']
        # The driver points at the spelled ids the case id already carries, the ids SCENE-LANG registers.
        self.assertTrue(all(case['scene'] == 'i50.' + case['id'] for case in fit))
        self.assertIn('i50.fit.de.list.rest.gm.r0', {case['scene'] for case in fit})
        registered = {case['scene'] for case in fit if case['scene'] in inputs['scenes']}
        self.assertEqual({scene for scene in inputs['scenes'] if scene.startswith('i50.fit.')}, registered)
        sceneless = [case for case in plan['cases'] if case['frontend'] == 'gui' and case['kind'] != 'fit']
        self.assertTrue(sceneless and all('SCENES' in case['missing'] for case in sceneless))
        owed = {row['dependency']: row['missing'] for row in plan['owed']}
        self.assertEqual(owed['SCENES'], f'{len(fit) - len(registered)} of {len(fit)} gui fit cases name a scene that '
                         f'ci/visual/scenes does not register; {len(sceneless)} catalog, ime, fonts, rtl and live gui cases '
                         'have no scene id yet')
        # A registry with every fit scene leaves only the sceneless cases on SCENES, still counted.
        every = dict(inputs, scenes={case['scene'] for case in fit})
        full = tool.build_plan(contract, roster, 'all', inputs=every)
        self.assertTrue(all(case['status'] == 'planned' for case in full['cases'] if case['kind'] == 'fit' and case['frontend'] == 'gui'))
        self.assertIn(f'0 of {len(fit)} gui fit cases', {row['dependency']: row['missing'] for row in full['owed']}['SCENES'])
        # No registry, or one without an i50 scene, keeps SCENE-LANG owed on every gui case and never names SCENES.
        for scenes in (None, {'g1-01-list-r0'}):
            bare = tool.build_plan(contract, roster, 'all', inputs=dict(inputs, scenes=scenes))
            deps = {row['dependency'] for row in bare['owed']}
            self.assertIn('SCENE-LANG', deps)
            self.assertNotIn('SCENES', deps)
            for case in bare['cases']:
                self.assertEqual('SCENE-LANG' in case['missing'], case['frontend'] == 'gui', case['id'])

    def test_i50_native_plan_output_needs_a_marked_root(self):
        bare = Path(tempfile.mkdtemp(prefix='native-unmarked.', dir=self.root.parent))
        self.addCleanup(lambda: bare.is_absolute() and bare.name.startswith('native-unmarked.') and shutil.rmtree(bare))
        done = self.run_tool('plan', '--languages', 'all', '--output', bare / 'plan.json')
        self.assertNotEqual(done.returncode, 0)
        self.assertIn('marker', done.stderr)
        self.assertFalse((bare / 'plan.json').exists())


class MissingLanguage(Evidence):
    def test_i50_native_missing_language_fails(self):
        capture = copy.deepcopy(self.capture)
        capture['records'] = [r for r in capture['records'] if r['language'] != 'th']
        self.assertRefused('catalog.th.gui: no evidence', capture=capture)
        plan = copy.deepcopy(self.plan_doc)
        plan['roster']['codes'] = [code for code in self.codes if code != 'th']
        plan['cases'] = [case for case in plan['cases'] if case['language'] != 'th']
        self.assertRefused('roster language th has no case', plan=plan)
        subset = dict(copy.deepcopy(self.plan_doc), scope='subset')
        self.assertRefused('subset plan is not all-language proof', plan=subset)

    def test_i50_native_complete_synthetic_evidence_has_no_refusal(self):
        self.assertEqual(self.errors(), [])

    def test_i50_native_owed_plan_and_tui_records_fail(self):
        plan = copy.deepcopy(self.plan_doc)
        plan['complete'] = False
        plan['owed'] = [dict(dependency='TUI-16', provides='catalog-case callable', missing='accepted RESULT')]
        self.assertRefused('plan owes TUI-16', plan=plan)
        plan = copy.deepcopy(self.plan_doc)
        plan['cases'][0]['status'] = 'OWED'
        self.assertRefused('case catalog.ar.gui is OWED', plan=plan)
        plan = copy.deepcopy(self.plan_doc)
        plan['cases'][0]['frontend'] = 'tui'
        capture = copy.deepcopy(self.capture)
        capture['records'][0]['frontend'] = 'tui'
        self.assertRefused('TUI receipt adapter OWED', plan=plan, capture=capture)

    def test_i50_native_uncovered_or_english_text_fails(self):
        capture = copy.deepcopy(self.capture)
        de = next(r for r in capture['records'] if r['language'] == 'de')
        de['texts'] = [dict(key='menu.open', text='Open')]
        self.assertRefused('catalog.de.gui: menu.open shows', capture=capture)
        de['texts'] = []
        self.assertRefused('catalog.de.gui: required key menu.open not displayed', capture=capture)
        de['texts'] = [dict(key='menu.open', text='Open de'), dict(key='menu.gone', text='x')]
        self.assertRefused('catalog.de.gui: menu.gone is not in the de catalog', capture=capture)

    def test_i50_native_catalog_case_needs_required_keys_and_other_kinds_wait(self):
        plan = copy.deepcopy(self.plan_doc)
        plan['cases'][0]['required_keys'] = []
        self.assertRefused('catalog.ar.gui: case names no required keys', plan=plan)
        plan = copy.deepcopy(self.plan_doc)
        plan['cases'][0]['kind'] = 'fit'
        self.assertRefused('catalog.ar.gui: fit evidence adapter OWED', plan=plan)

    def test_i50_native_placeholder_and_plural_forms(self):
        self.assertTrue(self.tool.matches('3 Elemente', {'one': '%1 Element', 'other': '%1 Elemente'}))
        self.assertFalse(self.tool.matches('3 items', {'one': '%1 Element', 'other': '%1 Elemente'}))
        self.assertFalse(self.tool.matches('Open', '%1'))
        self.assertTrue(self.tool.matches('notes.txt öffnen', '%1 öffnen'))
        # A screen that shows the raw form never took its arguments.
        self.assertFalse(self.tool.matches('%1 öffnen', '%1 öffnen'))
        self.assertFalse(self.tool.matches('%1 x öffnen', '%1 %2 öffnen'))
        self.assertFalse(self.tool.matches('%1 Elemente', {'one': '%1 Element', 'other': '%1 Elemente'}))
        self.assertTrue(self.tool.matches('my%20file.txt öffnen', '%1 öffnen'))
        self.assertTrue(self.tool.matches('%5 erledigt', '%1 erledigt'))
        self.assertTrue(self.tool.matches('%10 erledigt', '%1 erledigt'))
        self.assertTrue(self.tool.matches('a%20b von c', '%2 von %1'))
        self.assertFalse(self.tool.matches('Datei %1%2', 'Datei %1%2'))
        self.assertFalse(self.tool.matches('Datei %2x', 'Datei %2%1'))
        self.assertFalse(self.tool.matches('Datei a%1', 'Datei %2%1'))
        self.assertTrue(self.tool.matches('Datei ab', 'Datei %2%1'))
        self.assertTrue(self.tool.matches('Datei report%1.txt', 'Datei %1'))
        self.assertTrue(self.tool.matches('Datei a%2 von b', 'Datei %1 von %2'))
        self.assertFalse(self.tool.matches('Datei %25', 'Datei %2%1'))
        self.assertFalse(self.tool.matches('%2 von %1', '%2 von %1'))
        self.assertFalse(self.tool.placeholder_only({}))
        self.assertFalse(self.tool.placeholder_only(None))

    def test_i50_native_placeholder_only_key_is_not_required(self):
        for code in self.codes:
            path = self.catalogs / code / 'flea.json'
            data = json.loads(path.read_text())
            data['send.reason'] = '%1'
            path.write_text(json.dumps(data, ensure_ascii=False))
        capture = copy.deepcopy(self.capture)
        for record in capture['records']:
            record['catalog_sha256'] = sha((self.catalogs / record['language'] / 'flea.json').read_bytes())
        plan = copy.deepcopy(self.plan_doc)
        for case in plan['cases']:
            case['required_keys'] = ['menu.open', 'send.reason']
        self.assertEqual(self.errors(plan=plan, capture=capture), [])
        for case in plan['cases']:
            case['required_keys'] = ['menu.open', 'menu.cancel']
        self.assertRefused('required key menu.cancel not displayed', plan=plan, capture=capture)

    def test_i50_native_each_evidence_refusal_names_its_defect(self):
        plan, capture = copy.deepcopy(self.plan_doc), copy.deepcopy(self.capture)
        plan['schema'] = 'other'
        self.assertRefused('unknown plan or capture schema', plan=plan)
        capture['schema'] = 'other'
        self.assertRefused('unknown plan or capture schema', capture=capture)
        capture = copy.deepcopy(self.capture)
        plan = copy.deepcopy(self.plan_doc)
        plan['cases'].append(copy.deepcopy(plan['cases'][0]))
        self.assertRefused('duplicate case catalog.ar.gui', plan=plan)
        plan = copy.deepcopy(self.plan_doc)
        plan['cases'][0]['language'] = None
        self.assertRefused('malformed case', plan=plan)
        plan = copy.deepcopy(self.plan_doc)
        plan['complete'] = False
        self.assertRefused('plan is not complete', plan=plan)
        capture['records'].append(dict(capture['records'][0], case='catalog.xx.gui'))
        self.assertRefused('catalog.xx.gui: evidence for an unplanned case', capture=capture)
        capture = copy.deepcopy(self.capture)
        capture['records'][0]['frontend'] = 'tui'
        self.assertRefused('catalog.ar.gui: evidence names another language or frontend', capture=capture)
        capture = copy.deepcopy(self.capture)
        capture['records'][0]['language'] = 'de'
        self.assertRefused('catalog.ar.gui: evidence names another language or frontend', capture=capture)
        capture = copy.deepcopy(self.capture)
        capture['records'][0]['texts'] = [dict(text='Open ar')]
        self.assertRefused("displayed text 'Open ar' has no catalog key", capture=capture)
        plan = copy.deepcopy(self.plan_doc)
        plan['source_index'] = {'sha256': '0' * 64}
        self.assertRefused('source index differs from the plan', plan=plan)

    def test_i50_native_malformed_evidence_is_refused_not_crashed(self):
        capture = copy.deepcopy(self.capture)
        capture['records'].append('x')
        capture['records'][0]['texts'] = ['menu.open']
        capture['records'][1]['receipt'] = 'presented'
        self.assertRefused('malformed evidence record', capture=capture)
        self.assertRefused('malformed displayed text', capture=capture)
        self.assertRefused(f'{capture["records"][1]["case"]}: no presented receipt', capture=capture)
        self.assertEqual(self.tool.check_evidence([], {}, self.catalogs, self.keymap, self.index, self.candidate),
                         ['plan or capture is not a JSON object'])
        # Dict-shaped defects are refused by name, never raised.
        plan = copy.deepcopy(self.plan_doc)
        plan['cases'][0] = {'id': 'catalog.ar.gui'}
        self.assertRefused('malformed case', plan=plan)
        plan = copy.deepcopy(self.plan_doc)
        plan['owed'] = [{'missing': 'x'}, 'y']
        self.assertRefused('malformed owed row', plan=plan)
        # The producer always writes owed, so its absence is refused by name rather than read as nothing owed.
        plan.pop('owed')
        errors = self.errors(plan=plan)
        self.assertIn('plan has no owed list', errors)
        self.assertFalse(any('malformed owed row' in error for error in errors), errors)
        capture = copy.deepcopy(self.capture)
        capture['records'][0]['case'] = ['x']
        self.assertRefused('malformed evidence record', capture=capture)
        capture = copy.deepcopy(self.capture)
        capture['records'][0]['texts'] = [dict(key='menu.open', text=5)]
        self.assertRefused('catalog.ar.gui: menu.open shows 5', capture=capture)
        capture['records'][0]['texts'] = [dict(key=['menu.open'], text='x')]
        self.assertRefused("catalog.ar.gui: ['menu.open'] is not in the ar catalog", capture=capture)
        self.assertRefused('evidence records are not a list', capture=dict(self.capture, records=5))
        self.assertRefused('malformed plan cases', plan=dict(self.plan_doc, cases=5))

    def test_i50_native_never_approves_a_baseline(self):
        report = self.tool.verdict(self.plan_doc, [])
        self.assertEqual(report['visual_approval'], 'controller-inspection-owed')
        capture = copy.deepcopy(self.capture)
        capture['records'][0]['approved'] = True
        self.assertRefused('evidence may not carry approval', capture=capture)


class Identity(Evidence):
    def test_i50_native_candidate_identity_fails(self):
        self.assertRefused('candidate identity absent', candidate=False)
        plan = copy.deepcopy(self.plan_doc)
        plan.pop('source_index')
        self.assertRefused('source index differs from the plan', plan=plan)
        other = self.root / 'other-flea'
        other.write_bytes(b'another build')
        self.assertRefused('candidate sha256 does not match', candidate=other)
        for field, needle in [('source_commit', 'source commit'), ('keymap_sha256', 'keymap'),
                              ('source_index_sha256', 'source index')]:
            capture = copy.deepcopy(self.capture)
            capture[field] = 'b' * 64
            self.assertRefused(needle, capture=capture)
            capture.pop(field)
            self.assertRefused(needle, capture=capture)
        capture = copy.deepcopy(self.capture)
        capture['source_commit'] = 'c' * 40
        self.assertRefused('source commit absent or mismatched', capture=capture)
        capture = copy.deepcopy(self.capture)
        capture['records'][3]['receipt']['presented'] = False
        self.assertRefused('no presented receipt', capture=capture)
        capture = copy.deepcopy(self.capture)
        capture['records'][3]['receipt']['case'] = capture['records'][4]['case']
        self.assertRefused('receipt names another case', capture=capture)
        capture = copy.deepcopy(self.capture)
        capture['records'].append(copy.deepcopy(capture['records'][0]))
        self.assertRefused('duplicate evidence', capture=capture)


class StaleCatalog(Evidence):
    def test_i50_native_stale_catalog_fails(self):
        path = self.catalogs / 'ja' / 'flea.json'
        path.write_text(path.read_text().replace('Open ja', 'Open ja v2'))
        self.assertRefused('catalog.ja.gui: catalog changed since capture', capture=self.capture)
        path.unlink()
        self.assertRefused('catalog.ja.gui: catalog missing', capture=self.capture)


class Lease(Scratch):
    def test_i50_native_shared_tui_lease(self):
        plan = self.plan()
        protocol = plan['capture_protocol']
        self.assertEqual(protocol['queue'], '/home/gm/flea-work/ctl-gates/minipc-q.sh')
        self.assertFalse(protocol['own_lock'])
        self.assertEqual(protocol['lease'], 'TUI-16')
        self.assertEqual(protocol['tui_callable']['callable'], 'tui_battery.candidate.catalog_case')
        out = self.root / 'capture'
        before = sorted(p.name for p in self.root.iterdir())
        done = self.run_tool('capture', '--candidate', ROSTER, '--languages', 'all', '--cases', 'catalog', '--output', out)
        self.assertEqual(done.returncode, 3, done.stderr)
        self.assertIn('BLOCKED', done.stderr)
        self.assertIn('BLOCKED SCENES', done.stderr)
        self.assertNotIn('BLOCKED SCENE-LANG', done.stderr)
        self.assertIn('BLOCKED TUI-RECEIPT', done.stderr)
        self.assertNotIn('TUI-PREPARE', done.stderr)
        self.assertIn('no capture adapter exists yet', done.stderr)
        self.assertEqual(before, sorted(p.name for p in self.root.iterdir()))
        # The driver joins the existing queue; it never opens a lock of its own.
        text = TOOL.read_text()
        for word in ('flock', 'fcntl', 'lockf', '.lock'):
            self.assertNotIn(word, text)

    def test_i50_native_verify_refuses_owed_plan_end_to_end(self):
        plan = self.plan()
        (self.root / 'capture.json').write_text(json.dumps({'schema': load_tool().CAPTURE_SCHEMA, 'records': []}))
        done = self.run_tool('verify', '--input', self.root, '--source-index', INDEX)
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertIn('I50_NATIVE_VERIFY REFUSED', done.stdout)
        self.assertIn('plan owes SCENES', done.stdout)
        self.assertFalse(plan['complete'])

    def test_i50_native_verify_refuses_a_forged_plan(self):
        plan = self.plan()
        plan.update(complete=True, owed=[])
        for case in plan['cases']:
            case.update(status='planned', missing=[])
        (self.root / 'plan.json').write_text(json.dumps(plan))
        (self.root / 'capture.json').write_text(json.dumps({'schema': load_tool().CAPTURE_SCHEMA, 'records': ['x']}))
        done = self.run_tool('verify', '--input', self.root, '--source-index', INDEX)
        self.assertEqual(done.returncode, 1, done.stderr)
        self.assertIn('plan differs from the plan this tree computes', done.stdout)
        self.assertIn('malformed evidence record', done.stdout)
        self.assertNotIn('plan owes', done.stdout)



INDEX = REPO / 'i18n/source-index.json'
POLICY = REPO / 'i18n/rtl-policy.json'
INVENTORY = REPO / 'ci/visual/i50/ime-cases.tsv'
TUI_CANDIDATE = REPO / 'tests/tui_battery/candidate.py'


class Adapters(Scratch):
    """Accepted I50-01/02 source index, I50-04 RTL policy and I50-07 entry inventory feed the plan; a bad input is refused."""

    def copy(self, source, name, mutate):
        target = self.root / name
        if source.suffix == '.json':
            data = json.loads(source.read_text())
            mutate(data)
            target.write_text(json.dumps(data, ensure_ascii=False))
        else:
            target.write_text(mutate(source.read_text()))
        return target

    def refused(self, flag, path, needle):
        out = self.root / f'refused-{flag}.json'
        done = self.run_tool('plan', '--languages', 'all', '--output', out, flag, path)
        self.assertEqual(done.returncode, 2, done.stdout)
        self.assertIn(needle, done.stderr)
        self.assertFalse(out.exists())

    def test_i50_native_source_index_adapter(self):
        plan = self.plan()
        index = json.loads(INDEX.read_text())
        self.assertEqual(plan['source_index']['sha256'], sha(INDEX.read_bytes()))
        owed = {row['dependency'] for row in plan['owed']}
        self.assertNotIn('I50-02', owed)
        for end in ('gui', 'tui'):
            want = sorted(key for key, record in index['keys'].items()
                          if any(site['surface'] == end and site['usage'] == 'translated-call' for site in record['call_sites']))
            self.assertTrue(want, end)
            for code in ('de', 'ar', 'en'):
                case = next(c for c in plan['cases'] if c['id'] == f'catalog.{code}.{end}')
                self.assertEqual(case['required_keys'], want, (code, end))
            # Unverified occurrences stay visible as source-owner work, never as required native coverage.
            self.assertEqual(plan['source_index']['unverified'][end],
                             sorted(key for key, record in index['keys'].items()
                                    if any(site['surface'] == end for site in record['call_sites']) and key not in want))
        self.refused('--source-index', self.copy(INDEX, 'schema.json', lambda d: d.update(schema=2)), 'source index schema')
        self.refused('--source-index', self.copy(INDEX, 'roster.json', lambda d: d['roster'].update(file_sha256='0' * 64)),
                     'source index roster differs')

    def test_i50_native_source_index_requires_accepted_handoff(self):
        self.refused('--source-index', self.copy(INDEX, 'empty-index.json', lambda d: d.update(keys={})),
                     'I50-01 source index differs from the accepted handoff')
        self.refused('--source-index', self.copy(INDEX, 'changed-index.json', lambda d: d['counts'].update(extra=1)),
                     'I50-01 source index differs from the accepted handoff')

    def test_i50_native_missing_index_guidance_uses_the_authoritative_pin(self):
        tool = load_tool()
        contract = tool.load_contract(tool.CONTRACT)
        roster = tool.load_roster(ROSTER)
        for pin in (contract['source_index']['sha256'], '0' * 64):
            contract['source_index']['sha256'] = pin
            with patch.object(tool, 'INDEX', self.root / 'absent-index.json'):
                inputs = tool.load_inputs(contract, roster)
            self.assertIsNone(inputs['source_index'])
            plan = tool.build_plan(contract, roster, 'all', inputs=inputs)
            row = next(row for row in plan['owed'] if row['dependency'] == 'I50-02')
            self.assertIn('source_index.sha256', row['missing'])
            self.assertNotIn('cbf351a8', row['missing'])
            self.assertFalse(plan['complete'])
            self.assertTrue(any('I50-02' in case['missing'] for case in plan['cases']))
        with self.assertRaisesRegex(ValueError, 'differs from the accepted handoff'):
            tool.load_source_index(INDEX, roster, '0' * 64)

    def test_i50_native_verify_binds_the_tree_head(self):
        plan = self.plan()
        plan['source_commit'] = '0' * 40
        (self.root / 'plan.json').write_text(json.dumps(plan))
        (self.root / 'capture.json').write_text(json.dumps({'schema': load_tool().CAPTURE_SCHEMA, 'records': []}))
        done = self.run_tool('verify', '--input', self.root, '--source-index', INDEX)
        self.assertEqual(done.returncode, 1, done.stderr)
        self.assertIn(f'plan source commit {"0" * 40} is not this tree\'s HEAD', done.stdout)
        self.assertNotIn('plan differs from the plan this tree computes', done.stdout)

    def test_i50_native_unparsable_tui_candidate_is_refused(self):
        broken = self.copy(TUI_CANDIDATE, 'broken.py', lambda s: s + '\ndef (\n')
        self.refused('--tui-candidate', broken, f'TUI-16 candidate {broken} does not parse')
        self.refused('--tui-candidate', broken, f'({broken.name}, line')

    def test_i50_native_verify_binds_the_plan_source_index(self):
        self.plan()
        other = self.copy(INDEX, 'other-index.json', lambda d: d.update(counts=dict(d['counts'], extra=1)))
        (self.root / 'capture.json').write_text(json.dumps({'schema': load_tool().CAPTURE_SCHEMA, 'records': []}))
        done = self.run_tool('verify', '--input', self.root, '--source-index', other)
        self.assertEqual(done.returncode, 2, done.stderr)
        self.assertIn('source index differs from the accepted handoff', done.stderr)

    def test_i50_native_rtl_policy_adapter(self):
        plan = self.plan()
        policy = json.loads(POLICY.read_text())
        self.assertEqual(plan['rtl_policy']['sha256'], sha(POLICY.read_bytes()))
        self.assertEqual(plan['rtl_policy']['status'], policy['status'])
        self.assertNotIn('I50-04', {row['dependency'] for row in plan['owed']})
        rtl = [c for c in plan['cases'] if c['kind'] == 'rtl']
        self.assertEqual({c['language'] for c in rtl}, set(policy['rtl_languages']))
        for case in rtl:
            self.assertEqual(case['roles'], {name: role['action'] for name, role in sorted(policy['roles'].items())})
        self.refused('--rtl-policy', self.copy(POLICY, 'langs.json', lambda d: d.update(rtl_languages=['ar'])),
                     'rtl policy languages differ from the roster')
        self.refused('--rtl-policy', self.copy(POLICY, 'action.json', lambda d: d['roles']['rail'].update(action='flip')),
                     'rtl policy role rail')

    def test_i50_native_rtl_policy_requires_accepted_handoff(self):
        self.refused('--rtl-policy', self.copy(POLICY, 'partial-policy.json',
                                             lambda d: d.update(roles={'rail': d['roles']['rail']})),
                     'I50-04 rtl policy differs from the accepted handoff')
        self.refused('--rtl-policy', self.copy(POLICY, 'changed-policy.json',
                                             lambda d: d['roles']['rail'].update(action='keep')),
                     'I50-04 rtl policy differs from the accepted handoff')

    def test_i50_native_verify_reloads_explicit_plan_inputs(self):
        overrides = {}
        for flag, source in (('--source-index', INDEX), ('--rtl-policy', POLICY), ('--entry-inventory', INVENTORY)):
            target = self.root / source.name
            shutil.copyfile(source, target)
            overrides[flag] = target
        for flags in (('--rtl-policy',), ('--entry-inventory',), tuple(overrides)):
            with self.subTest(flags=flags):
                output = self.root / f'verify-{len(flags)}-{flags[0][2:]}'
                output.mkdir()
                (output / '.flea-test-sandbox').write_text('i50-27 override test\n')
                inputs = [arg for flag in flags for arg in (flag, overrides[flag])]
                done = self.run_tool('plan', '--languages', 'all', '--output', output / 'plan.json', *inputs)
                self.assertEqual(done.returncode, 0, done.stderr)
                plan = json.loads((output / 'plan.json').read_text())
                for flag in flags:
                    field = {'--source-index': 'source_index', '--rtl-policy': 'rtl_policy',
                             '--entry-inventory': 'ime_inventory'}[flag]
                    self.assertEqual(plan[field]['sha256'], sha(overrides[flag].read_bytes()))
                    self.assertEqual(plan[field]['path'], load_tool().shown(overrides[flag]))
                (output / 'capture.json').write_text(json.dumps({'schema': load_tool().CAPTURE_SCHEMA, 'records': []}))
                source_args = [] if '--source-index' in flags else ['--source-index', INDEX]
                done = self.run_tool('verify', '--input', output, *source_args, *inputs)
                self.assertEqual(done.returncode, 1, done.stderr)
                self.assertNotIn('plan differs from the plan this tree computes', done.stdout)
                self.assertIn('plan owes SCENES', done.stdout)

    def test_i50_native_ime_inventory_adapter(self):
        plan = self.plan()
        rows = [line.split('\t') for line in INVENTORY.read_text().splitlines()[1:]]
        ime = [c for c in plan['cases'] if c['kind'] == 'ime']
        self.assertEqual(len(ime), 280)
        self.assertEqual({c['engine'] for c in ime}, {'mozc', 'hangul', 'pinyin', 'chewing', 'cangjie5'})
        self.assertEqual(len({c['entry'] for c in ime}), 56)
        self.assertEqual([c['id'] for c in ime], ['ime.' + row[0].replace('/', '.') for row in rows])
        owed = {row['dependency']: row['missing'] for row in plan['owed']}
        self.assertNotIn('I50-07', owed)
        hosted = {c['entry'] for c in ime if c['host'] == 'mounted'}
        for case in ime:
            self.assertEqual('IME-HOSTS' in case['missing'], case['host'] != 'mounted', case['id'])
            self.assertIn('SCENES', case['missing'])
        self.assertIn(f'{56 - len(hosted)} entries', owed['IME-HOSTS'])
        self.assertEqual(plan['ime_inventory']['sha256'], sha(INVENTORY.read_bytes()))
        lines = INVENTORY.read_text().splitlines(keepends=True)
        self.refused('--entry-inventory', self.copy(INVENTORY, 'short.tsv', lambda s: ''.join(lines[:-1])),
                     'I50-07 inventory does not run every entry on every engine')
        self.refused('--entry-inventory', self.copy(INVENTORY, 'dup.tsv', lambda s: s + lines[1]),
                     'I50-07 inventory repeats a case')
        self.refused('--entry-inventory', ROSTER, 'I50-07 inventory')

    def test_i50_native_ime_inventory_requires_accepted_handoff(self):
        def changed_field(text):
            lines = text.splitlines(keepends=True)
            fields = lines[1].rstrip('\n').split('\t')
            fields[4] += ' changed'
            lines[1] = '\t'.join(fields) + '\n'
            return ''.join(lines)
        self.refused('--entry-inventory', self.copy(INVENTORY, 'changed-inventory.tsv', changed_field),
                     'I50-07 inventory differs from the accepted handoff')

    def test_i50_native_tui_callable_adapter(self):
        plan = self.plan()
        callable_ = plan['capture_protocol']['tui_callable']
        self.assertEqual(callable_['sha256'], sha(TUI_CANDIDATE.read_bytes()))
        self.assertEqual(tuple(callable_['keywords']), load_tool().TUI_SIGNATURE)
        owed = {row['dependency'] for row in plan['owed']}
        self.assertNotIn('TUI-PREPARE', owed)
        self.assertNotIn('TUI-16', owed)
        self.assertIn('TUI-RECEIPT', owed)
        for case in plan['cases']:
            # The callable and its private preparation are in the tree; only the verify reader is still owed.
            if case['frontend'] == 'tui':
                self.assertEqual((case['status'], case['missing']), ('OWED', ['TUI-RECEIPT']), case['id'])
        text = TUI_CANDIDATE.read_text()
        head = 'def catalog_case(*, args,'
        self.assertIn(head, text)
        for name, mutate in (('dropped', lambda s: s.replace(', prepare):', '):', 1)),
                             ('positional', lambda s: s.replace(head, 'def catalog_case(args, *,', 1)),
                             ('default', lambda s: s.replace(', prepare):', ', prepare=None):', 1))):
            with self.subTest(name):
                changed = mutate(text)
                self.assertNotEqual(changed, text)
                self.refused('--tui-candidate', self.copy(TUI_CANDIDATE, f'{name}.py', lambda s: changed),
                             'TUI-16 catalog_case signature differs from the frozen handoff')
        self.refused('--tui-candidate', self.copy(TUI_CANDIDATE, 'none.py', lambda s: s.replace('def catalog_case', 'def other_case')),
                     'TUI-16 candidate has no single catalog_case')


if __name__ == '__main__':
    unittest.main()

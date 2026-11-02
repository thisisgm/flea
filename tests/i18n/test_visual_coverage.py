"""Language fit coverage: every required language, surface and control state is planned and accounted for."""
import copy
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
TOOL = REPO / 'tools/flea-i18n-native'
CONTRACT = REPO / 'ci/visual/i50/contract.json'
UNDRIVEN = REPO / 'ci/visual/undriven.txt'
FIT = ['de', 'ar', 'ja', 'zh-CN', 'th', 'hi']
STATES = {'rest', 'hover', 'focus', 'pressed', 'disabled'}


def load_tool():
    loader = importlib.machinery.SourceFileLoader('flea_i18n_native', str(TOOL))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class Coverage(unittest.TestCase):
    def setUp(self):
        self.tool = load_tool()
        self.contract = json.loads(CONTRACT.read_text())
        self.plan = self.tool.build_plan(self.tool.load_contract(CONTRACT), self.tool.load_roster(self.tool.ROSTER), 'all')

    def test_i50_visual_six_languages_covered(self):
        self.assertEqual(self.contract['fit_languages'], FIT)
        surfaces = [s['id'] for s in self.contract['surfaces'] if s['frontend'] == 'gui']
        boards = {b for s in self.contract['surfaces'] for b in s['boards']}
        for board in ('lang-de-menu', 'lang-de-dialog', 'lang-de-settings', 'lang-ar-list', 'lang-ja-list',
                      'lang-ja-rename', 'lang-caps', 'lang-about', 'tui-lang-ja', 'tui-lang-ar', 'tui-lang-ru-keys'):
            self.assertIn(board, boards)
        fit = {(c['language'], c['surface']) for c in self.plan['cases'] if c['kind'] == 'fit' and c['frontend'] == 'gui'}
        self.assertEqual(fit, {(code, surface) for code in FIT for surface in surfaces})
        self.assertEqual(self.tool.coverage_errors(self.plan, UNDRIVEN.read_text()), [])

    def test_i50_visual_coverage_refuses_gaps_and_stale_lines(self):
        text = UNDRIVEN.read_text()
        scenes = self.tool.fit_scene_ids(self.plan)
        self.assertTrue(scenes)
        menu = [scene for scene in scenes if scene.startswith('i50.fit.ar.menu.')]
        self.assertEqual(len(menu), 20)
        dropped = '\n'.join(line for line in text.splitlines() if not line.startswith('i50.fit.ar.menu '))
        errors = self.tool.coverage_errors(self.plan, dropped)
        for scene in menu:
            self.assertIn(f'{scene}: no registered scene and no undriven reason', errors)
        bare = text.replace('i50.fit.ar.caps BLOCKED ', 'i50.fit.ar.caps OK ', 1)
        self.assertIn('i50.fit.ar.caps.rest.gm.r0: no registered scene and no undriven reason', self.tool.coverage_errors(self.plan, bare))
        # A group ends at a dot: a cut-off name or a spelled field that matches nothing names no planned scene.
        for stale in ('i50.fit.xx.menu', 'i50.fit.ar.men', 'i50.fit.ar.menu.rest.gm.r4'):
            self.assertIn(f'{stale}: undriven line names no planned scene',
                          self.tool.coverage_errors(self.plan, text + f'{stale} BLOCKED I50-27: gone\n'))
        twice = text + 'i50.fit.ar.menu BLOCKED I50-27: again\n'
        self.assertIn('i50.fit.ar.menu: duplicate undriven line', self.tool.coverage_errors(self.plan, twice))
        nested = text + 'i50.fit.ar.menu.hover.gm.r0 BLOCKED I50-27: again\n'
        self.assertIn('i50.fit.ar.menu.hover.gm.r0: more than one undriven line (i50.fit.ar.menu, i50.fit.ar.menu.hover.gm.r0)',
                      self.tool.coverage_errors(self.plan, nested))
        caps = [scene for scene in scenes if scene.startswith('i50.fit.ar.caps.')]
        self.assertIn('i50.fit.ar.caps: registered scene still has an undriven line',
                      self.tool.coverage_errors(self.plan, text, registered=set(caps)))
        # A group with one scene still unregistered keeps its reason line.
        self.assertNotIn('i50.fit.ar.caps: registered scene still has an undriven line',
                         self.tool.coverage_errors(self.plan, text, registered=set(caps[1:])))

    def test_i50_visual_list_rest_is_registered_and_other_states_have_reasons(self):
        text = UNDRIVEN.read_text()
        registered = self.tool.registered_scenes()
        scenes = self.tool.fit_scene_ids(self.plan)
        self.assertEqual(sorted(scene for scene in registered if scene.startswith('i50.fit.')),
                         [scene for scene in scenes if '.list.rest.' in scene])
        for code in FIT:
            self.assertNotIn(f'i50.fit.{code}.list BLOCKED', text)
            for state in ('hover', 'focus'):
                self.assertIn(f'i50.fit.{code}.list.{state} BLOCKED SCENE-LANG: list registers rest only; '
                              f'{state} needs a drawn-state readback\n', text)
        # The removed list group line overlaps the hover and focus lines, so restoring it is a double cover.
        restored = text + 'i50.fit.de.list BLOCKED I50-27: old\n'
        self.assertIn('i50.fit.de.list.hover.gm.r0: more than one undriven line (i50.fit.de.list.hover, i50.fit.de.list)',
                      self.tool.coverage_errors(self.plan, restored))
        # A rest group line or a rest scene line covers only registered scenes.
        old = text + 'i50.fit.de.list.rest BLOCKED I50-27: old\n'
        self.assertIn('i50.fit.de.list.rest: registered scene still has an undriven line', self.tool.coverage_errors(self.plan, old))
        exact = text + 'i50.fit.de.list.rest.gm.r0 BLOCKED I50-27: old\n'
        self.assertIn('i50.fit.de.list.rest.gm.r0: registered scene still has an undriven line',
                      self.tool.coverage_errors(self.plan, exact))
        dropped = text.replace('i50.fit.de.list.hover BLOCKED', 'x BLOCKED', 1)
        self.assertIn('i50.fit.de.list.hover.gm.r0: no registered scene and no undriven reason',
                      self.tool.coverage_errors(self.plan, dropped))
        contract = copy.deepcopy(self.tool.load_contract(CONTRACT))
        contract['fit_languages'] = FIT[:-1]
        with self.assertRaisesRegex(ValueError, 'fit languages must include hi'):
            self.tool.check_contract(contract)

    def test_i50_visual_registry_reads_scene_manifests(self):
        scenes = self.tool.fit_scene_ids(self.plan)
        with tempfile.TemporaryDirectory() as scratch:
            registry = Path(scratch) / 'scenes'
            registry.mkdir()
            (registry / 'a.json').write_text(json.dumps({'version': 1, 'scenes': [{'id': 'g1-01-list-r0'}]}))
            (registry / 'b.json').write_text(json.dumps({'version': 1, 'scenes': [{'id': scenes[0]}]}))
            registered = self.tool.registered_scenes(registry)
            self.assertEqual(registered, {'g1-01-list-r0', scenes[0]})
            undriven = ''.join(f'{scene} BLOCKED I50-27: fixture\n' for scene in scenes)
            needle = f'{scenes[0]}: registered scene still has an undriven line'
            self.assertIn(needle, self.tool.coverage_errors(self.plan, undriven, registered))
            self.assertNotIn(needle, self.tool.coverage_errors(self.plan, undriven, set()))
            self.assertNotIn('g1-01-list-r0: registered scene still has an undriven line',
                             self.tool.coverage_errors(self.plan, undriven, registered))
            (registry / 'c.json').write_text('{')
            with self.assertRaisesRegex(ValueError, 'c.json: unreadable scene manifest'):
                self.tool.registered_scenes(registry)
            with self.assertRaisesRegex(ValueError, 'no scene registry directory'):
                self.tool.registered_scenes(Path(scratch) / 'absent')

    def test_i50_visual_each_control_state(self):
        self.assertEqual(set(self.contract['states']), STATES)
        self.assertEqual(self.contract['viewport'], dict(width=1100, height=700, text_size=14))
        self.assertEqual(sorted(self.contract['rounding']), [0, 8])
        self.assertEqual(len(self.contract['themes']), 2)
        for surface in self.contract['surfaces']:
            self.assertTrue(surface['states'] and set(surface['states']) <= STATES, surface['id'])
            self.assertIn('rest', surface['states'], surface['id'])
            if surface['frontend'] != 'gui':
                continue
            for code in FIT:
                got = {(c['state'], c['theme'], c['rounding']) for c in self.plan['cases']
                       if c['kind'] == 'fit' and c['surface'] == surface['id'] and c['language'] == code}
                want = {(state, theme, rounding) for state in surface['states']
                        for theme in self.contract['themes'] for rounding in self.contract['rounding']}
                self.assertEqual(got, want, (surface['id'], code))
        states = set().union(*(s['states'] for s in self.contract['surfaces'] if s['frontend'] == 'gui'))
        self.assertEqual(states, STATES)
        contract = copy.deepcopy(self.tool.load_contract(CONTRACT))
        contract['surfaces'][0]['states'] = ['hover']
        with self.assertRaisesRegex(ValueError, 'rest'):
            self.tool.check_contract(contract)
        contract['surfaces'][0]['states'] = ['rest', 'blink']
        with self.assertRaisesRegex(ValueError, 'unknown state blink'):
            self.tool.check_contract(contract)


if __name__ == '__main__':
    unittest.main()

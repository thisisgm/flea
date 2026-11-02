"""The TUI language preparation in tools/flea-i18n-native: omarchy-language run on a marked fixture, never on the host."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

REPO = Path(__file__).resolve().parents[2]
TOOL = REPO / 'tools/flea-i18n-native'
# The pinned Omarchy 4.5.0 rc1 source; the real-helper test runs only where it exists.
OMARCHY_SOURCE = Path(os.environ.get('FLEA_OMARCHY_SOURCE', '/home/gm/flea-work/omarchy-450rc1/src'))
# A stand-in that calls what the real helper calls, and also tries to write SENTINEL at its host path.
FAKE_LANGUAGE = r'''#!/bin/bash
set -euo pipefail
file=${OMARCHY_LANGUAGE_FILE:-$HOME/.config/omarchy/language}
if [[ -z ${1:-} ]]; then cat "$file"; exit 0; fi
[[ $1 == fail ]] && { echo refused >&2; exit 4; }
env | cut -d= -f1 | sort >/fx/omarchy/seen-env
{ mkdir -p "$(dirname SENTINEL)" && echo leaked >SENTINEL; } 2>/dev/null || true
sudo -n -l -l x "$1" || true
pkexec true
[[ -z ${HYPRLAND_INSTANCE_SIGNATURE:-} ]] || hyprctl eval x
mkdir -p "$(dirname "$file")"
echo "$1" >"$file"
echo "$1_X.UTF-8 UTF-8" >>"$OMARCHY_LOCALE_GEN"
[[ $1 == wrong ]] && echo other >"$file"
omarchy-notification-send done
'''


def load_tool():
    loader = importlib.machinery.SourceFileLoader('flea_i18n_native', str(TOOL))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


@unittest.skipUnless(shutil.which('bwrap'), 'the preparation namespace needs bwrap')
class Prepare(unittest.TestCase):
    def setUp(self):
        self.tool = load_tool()
        self.root = Path(tempfile.mkdtemp(prefix='native-prepare.'))
        (self.root / '.flea-test-sandbox').write_text('tui-prepare test\n')
        self.addCleanup(self.remove, self.root)
        self.sentinel = self.root / 'host/sentinel'
        self.omarchy = self.root / 'omarchy'
        (self.omarchy / 'bin').mkdir(parents=True)
        (self.omarchy / 'default/i18n').mkdir(parents=True)
        (self.omarchy / 'default/i18n/locales.json').write_text(json.dumps(
            {code: {'locale': f'{code}_X.UTF-8'} for code in ('ja', 'ar', 'ru', 'fail', 'wrong')}))
        language = self.omarchy / 'bin/omarchy-language'
        language.write_text(FAKE_LANGUAGE.replace('SENTINEL', str(self.sentinel)))
        language.chmod(0o755)
        self.fx = self.root / 'fx'
        self.fx.mkdir()

    def remove(self, root):
        if root.name.startswith('native-prepare.') and (root / '.flea-test-sandbox').is_file():
            shutil.rmtree(root)

    def prepare(self, language='ja', fx=None, omarchy=None, group='us'):
        return self.tool.tui_prepare(language, group, {'files': {'keys.toml': 'bound'}}, omarchy or self.omarchy)(fx or self.fx)

    def run_tool(self, tmp, *args):
        tmp.mkdir()
        return subprocess.run([sys.executable, '-B', str(TOOL), 'prepare', '--keyboard-group', 'us', '--omarchy',
                               self.omarchy, *args], capture_output=True, text=True, cwd=REPO,
                              env={**os.environ, 'TMPDIR': str(tmp)})

    def test_tui_prepare_writes_only_inside_the_fixture(self):
        (self.fx / 'v/work').mkdir(parents=True)
        (self.fx / 'v/work/a.txt').write_text('battery fixture\n')
        (self.fx / 'config/flea').mkdir(parents=True)
        (self.fx / 'config/flea/keep.json').write_text('{}\n')
        hashed, real_sha = [], self.tool.sha
        host = self.tool.host_state()
        with patch.dict(os.environ, {'HYPRLAND_INSTANCE_SIGNATURE': 'live-session'}), \
                patch.object(self.tool, 'sha', side_effect=lambda path: hashed.append(Path(path)) or real_sha(path)):
            receipt = self.prepare()
        self.assertEqual(self.tool.host_state(), host)
        self.assertEqual(receipt['host_files'], host)
        # The helper tried the sentinel's host path; inside the namespace that path is private, so the host copy never appears.
        self.assertFalse(self.sentinel.exists())
        self.assertEqual((receipt['language'], receipt['keyboard_group'], receipt['locale']), ('ja', 'us', 'ja_X.UTF-8'))
        self.assertEqual(receipt['candidate_inputs'], {'files': {'keys.toml': 'bound'}})
        self.assertEqual((self.fx / 'config/omarchy/language').read_text(), 'ja\n')
        self.assertTrue((self.fx / 'omarchy/locale.gen').read_text().endswith('ja_X.UTF-8 UTF-8\n'))
        for name in ('config/omarchy/language', 'omarchy/locale.gen', 'omarchy/locales.json', 'omarchy/calls.log'):
            self.assertEqual(receipt['artifacts'][name], digest(self.fx / name), name)
        for name, sha in receipt['artifacts'].items():
            self.assertEqual(digest(self.fx / name), sha, name)
        # An unchanged file is not an artifact, and the battery tree under v/ is never hashed.
        self.assertNotIn('config/flea/keep.json', receipt['artifacts'])
        self.assertIn(self.fx / 'config/flea/keep.json', hashed)
        self.assertFalse([path for path in hashed if path.is_relative_to(self.fx / 'v')])
        # The live session never reaches the namespace: no Hyprland signature, so no hyprctl call either.
        seen = (self.fx / 'omarchy/seen-env').read_text().split()
        self.assertNotIn('HYPRLAND_INSTANCE_SIGNATURE', seen)
        self.assertTrue({'OMARCHY_LOCALES_JSON', 'OMARCHY_LANGUAGE_FILE', 'OMARCHY_LOCALE_GEN'} <= set(seen))
        self.assertEqual(receipt['calls'], ['sudo -n -l -l x ja', 'pkexec true', 'omarchy-notification-send done'])
        self.assertTrue(all(value.startswith('/fx/') for value in receipt['environment'].values()))

    def test_tui_prepare_receipt_satisfies_catalog_case(self):
        sys.path.insert(0, str(REPO / 'tests'))
        self.addCleanup(sys.path.remove, str(REPO / 'tests'))
        from tui_battery import candidate, driver
        from tui_battery.evidence import Blocked
        identity = {'candidate_inputs': {'files': {'keys.toml': 'bound'}}}

        def run(prepared_language, prepared_group, fx):
            fx.mkdir()
            prepare = self.tool.tui_prepare(prepared_language, prepared_group, identity['candidate_inputs'], self.omarchy)
            with patch.object(driver, 'execute_pair', side_effect=lambda *args, fixture_setup, **kwargs: fixture_setup(fx)):
                return candidate.catalog_case(args=None, root=None, display=None, terminal='foot', state={}, variant={},
                                              colors={}, identity=identity, profiles={}, reviews={}, overrides={}, seams={},
                                              language='ja', keyboard_group='us', prepare=prepare)
        receipt = run('ja', 'us', self.root / 'fx-ja')
        self.assertEqual((receipt['language'], receipt['keyboard_group']), ('ja', 'us'))
        self.assertIn('config/omarchy/language', receipt['artifacts'])
        for language, group, name in (('ar', 'us', 'fx-ar'), ('ja', 'ru', 'fx-ru')):
            with self.subTest(language=language, group=group), self.assertRaisesRegex(Blocked, 'identity mismatch'):
                run(language, group, self.root / name)

    def test_tui_prepare_refuses_an_unmarked_fixture(self):
        bare = Path(tempfile.mkdtemp(prefix='native-prepare.'))
        self.addCleanup(shutil.rmtree, bare)
        (bare / 'fx').mkdir()
        with self.assertRaisesRegex(ValueError, 'no nonempty .flea-test-sandbox marker'):
            self.prepare(fx=bare / 'fx')
        self.assertEqual(list((bare / 'fx').iterdir()), [])

    def test_tui_prepare_refuses_a_changed_host(self):
        # A rewritten file, a file mapped to None, and a key gone from the map all count as a change.
        for after in ({'/etc/locale.gen': 'b'}, {'/etc/locale.gen': None}, {}):
            with self.subTest(after=after):
                fx = self.root / f'fx-{len(after)}-{after.get("/etc/locale.gen")}'
                fx.mkdir()
                states = iter([{'/etc/locale.gen': 'a'}, after])
                with patch.object(self.tool, 'host_state', side_effect=lambda: next(states)), self.assertRaisesRegex(
                        ValueError, 'omarchy-language ja changed host locale or language files: /etc/locale.gen'):
                    self.prepare(fx=fx)

    def test_tui_prepare_refuses_a_failed_or_wrong_language(self):
        with self.assertRaisesRegex(ValueError, 'omarchy-language fail exit 4: refused'):
            self.prepare('fail')
        fx = self.root / 'fx2'
        fx.mkdir()
        with self.assertRaisesRegex(ValueError, "omarchy-language reads 'other', not wrong"):
            self.prepare('wrong', fx=fx)

    def test_tui_prepare_refuses_a_root_without_omarchy_language(self):
        (self.omarchy / 'bin/omarchy-language').unlink()
        with self.assertRaisesRegex(ValueError, 'no bin/omarchy-language'):
            self.prepare()

    def test_tui_language_cases_follow_the_battery(self):
        cases = self.tool.tui_language_cases('all')
        self.assertEqual(cases, [('tui-lang-ja', 'ja'), ('tui-lang-ja-menu', 'ja'), ('tui-lang-ar', 'ar'),
                                 ('tui-lang-ru-keys', 'ru')])
        self.assertEqual(self.tool.tui_language_cases('tui-lang-ar'), [('tui-lang-ar', 'ar')])
        for text in ('tui-list-text', 'tui-lang-ar,tui-lang-ar', ''):
            with self.assertRaises(ValueError):
                self.tool.tui_language_cases(text)

    def test_tui_prepare_cli_dry_run(self):
        out = self.root / 'dry-run.json'
        done = self.run_tool(self.root / 'tmp', '--cases', 'tui-lang-ar,tui-lang-ru-keys', '--output', out)
        self.assertEqual(done.returncode, 0, done.stderr)
        report = json.loads(out.read_text())
        self.assertEqual([(row['case'], row['receipt']['language']) for row in report['cases']],
                         [('tui-lang-ar', 'ar'), ('tui-lang-ru-keys', 'ru')])
        for row in report['cases']:
            root = Path(row['root'])
            self.assertEqual(root.parent, self.root / 'tmp')
            self.assertIn(f'I50_TUI_PREPARE {row["case"]} language={row["receipt"]["language"]} root={root}', done.stdout)
            self.assertTrue((root / '.flea-test-sandbox').read_text().strip())
            self.assertEqual((root / 'fx/config/omarchy/language').read_text().strip(), row['receipt']['language'])
        # An unmarked output is refused before any case root is made.
        done = self.run_tool(self.root / 'tmp2', '--output', self.root / 'unmarked/x.json')
        self.assertEqual(done.returncode, 2)
        self.assertIn('no nonempty .flea-test-sandbox marker', done.stderr)
        self.assertEqual(list((self.root / 'tmp2').iterdir()), [])

    @unittest.skipUnless((OMARCHY_SOURCE / 'bin/omarchy-language').is_file(), 'pinned Omarchy source absent')
    def test_tui_prepare_runs_the_real_omarchy_language(self):
        host = self.tool.host_state()
        for language, locale in (('ru', 'ru_RU.UTF-8'), ('ja', 'ja_JP.UTF-8')):
            with self.subTest(language=language):
                fx = self.root / f'fx-{language}'
                fx.mkdir()
                receipt = self.prepare(language, fx=fx, omarchy=OMARCHY_SOURCE)
                self.assertEqual(self.tool.host_state(), host)
                self.assertEqual((receipt['locale'], [run['exit'] for run in receipt['runs']]), (locale, [0, 0]))
                self.assertEqual((fx / 'config/uwsm/env.d/50-omarchy-language').read_text().splitlines()[-1],
                                 f'export LANG={locale}')
                self.assertIn('config/uwsm/env.d/50-omarchy-language', receipt['artifacts'])
        # Only Japanese seeds CJK glyph forms, and it lands in the fixture's own fontconfig.
        self.assertTrue((self.root / 'fx-ja/config/fontconfig/conf.d/50-omarchy-input-method.conf').is_file())


if __name__ == '__main__':
    unittest.main()

"""F6 regressions; each test writes only inside its own marked mktemp root."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

WORKTREE = Path(__file__).resolve().parents[2]
TOOL = WORKTREE / 'tools/flea-i18n-manifest'


class ManifestHistory(unittest.TestCase):
    def setUp(self):
        scratch = WORKTREE / '.superpowers/l1'
        scratch.mkdir(parents=True, exist_ok=True)
        out = subprocess.run(['mktemp', '-d', str(scratch / 'adv-c-manifest.XXXXXX')], capture_output=True, text=True, check=True)
        self.root = Path(out.stdout.strip())
        assert self.root.is_absolute() and self.root.parent == WORKTREE / '.superpowers/l1'
        (self.root / '.i18n-test-root').write_text('manifest history regression\n')
        self.addCleanup(self.cleanup_root)
        self.english = {'label': 'Open file', 'count': {'one': '%1 file', 'other': '%1 files'}}
        self.translation = {'label': 'Datei öffnen', 'count': {'one': '%1 Datei', 'other': '%1 Dateien'}}
        self.write('en/flea.json', self.english)
        self.write('de/flea.json', self.translation)
        self.run_tool()
        self.initial = self.read_manifest()

    def cleanup_root(self):
        root = self.root
        assert root.is_absolute() and str(root) and root.parent == WORKTREE / '.superpowers/l1'
        assert root.name.startswith('adv-c-manifest.') and not root.is_symlink()
        assert (root / '.i18n-test-root').is_file()
        shutil.rmtree(root)

    def write(self, name, value):
        path = self.root / name
        assert self.root.is_absolute() and (self.root / '.i18n-test-root').is_file()
        assert path.is_relative_to(self.root) and '..' not in path.parts
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, ensure_ascii=False), encoding='utf-8')

    def read_manifest(self):
        return json.loads((self.root / 'translation-manifest.json').read_text())

    def run_tool(self, *args, status=0):
        result = subprocess.run([sys.executable, '-B', str(TOOL), '--i18n-dir', str(self.root), *args], capture_output=True, text=True)
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return result

    def change_english(self):
        self.english['label'] = 'Open folder'
        self.write('en/flea.json', self.english)

    def test_regeneration_preserves_translation_source_hash(self):
        self.change_english()
        self.run_tool()
        current = self.read_manifest()
        self.assertNotEqual(current['en']['flea:label'], self.initial['en']['flea:label'])
        self.assertEqual(current['de'], self.initial['de'])
        before = (self.root / 'translation-manifest.json').read_bytes()
        result = self.run_tool('--check')
        self.assertIn('stale translation de:flea:label', result.stderr)
        self.assertEqual(before, (self.root / 'translation-manifest.json').read_bytes())

    def test_stale_translation_warns_by_name_without_failing_check(self):
        self.change_english()
        recorded = self.initial
        recorded['en']['flea:label'] = hashlib.sha256(b'Open folder').hexdigest()[:12]
        self.write('translation-manifest.json', recorded)
        before = (self.root / 'translation-manifest.json').read_bytes()
        result = self.run_tool('--check')
        self.assertIn('warning: stale translation de:flea:label', result.stderr)
        self.assertEqual(before, (self.root / 'translation-manifest.json').read_bytes())

    def test_mark_updates_only_selected_translation_keys(self):
        self.change_english()
        self.english['count']['other'] = '%1 total files'
        self.write('en/flea.json', self.english)
        self.run_tool()
        self.run_tool('--mark', 'de', 'label')
        current = self.read_manifest()
        self.assertEqual(current['de']['flea:label'], current['en']['flea:label'])
        self.assertEqual(current['de']['flea:count'], self.initial['de']['flea:count'])
        result = self.run_tool('--check')
        self.assertIn('stale translation de:flea:count', result.stderr)
        self.assertNotIn('stale translation de:flea:label', result.stderr)
        self.run_tool('--mark', 'de', 'flea:count')
        self.assertNotIn('stale translation', self.run_tool('--check').stderr)

    def test_new_keys_and_languages_get_initial_hash_without_refreshing_old_keys(self):
        self.change_english()
        self.english['new'] = 'New folder'
        self.translation['new'] = 'Neuer Ordner'
        self.write('en/flea.json', self.english)
        self.write('de/flea.json', self.translation)
        self.write('fr/flea.json', {'label': 'Ouvrir', 'count': {'one': '%1 fichier', 'other': '%1 fichiers'}, 'new': 'Nouveau dossier'})
        self.run_tool()
        current = self.read_manifest()
        self.assertEqual(current['de']['flea:label'], self.initial['de']['flea:label'])
        self.assertEqual(current['de']['flea:new'], current['en']['flea:new'])
        self.assertEqual(current['fr'], current['en'])

    def test_bad_marks_leave_manifest_unchanged(self):
        before = (self.root / 'translation-manifest.json').read_bytes()
        for args, status in [(('--mark', 'zz', 'label'), 1), (('--mark', 'de', 'absent'), 1), (('--mark', 'en', 'label'), 1), (('--mark', 'de'), 2), (('--mark', 'de', 'label', '--check'), 2)]:
            with self.subTest(args=args):
                self.run_tool(*args, status=status)
                self.assertEqual(before, (self.root / 'translation-manifest.json').read_bytes())

    def delete(self, name):
        path = self.root / name
        assert path.is_absolute() and str(path) and path.is_relative_to(self.root) and '..' not in path.parts
        assert (self.root / '.i18n-test-root').is_file()
        path.unlink()

    def drop_label(self):
        self.english.pop('label')
        self.translation.pop('label')
        self.write('en/flea.json', self.english)
        self.write('de/flea.json', self.translation)

    def test_regeneration_prunes_deleted_keys_from_all_sections(self):
        self.drop_label()
        self.run_tool()
        current = self.read_manifest()
        self.assertNotIn('flea:label', current['en'])
        self.assertNotIn('flea:label', current['de'])
        self.assertEqual(current['de']['flea:count'], self.initial['de']['flea:count'])

    def test_deleted_keys_warn_as_orphans_without_mutating_manifest(self):
        self.drop_label()
        before = (self.root / 'translation-manifest.json').read_bytes()
        result = self.run_tool('--check')
        self.assertIn('warning: orphaned entry en:flea:label', result.stderr)
        self.assertIn('warning: orphaned entry de:flea:label', result.stderr)
        self.assertEqual(before, (self.root / 'translation-manifest.json').read_bytes())

    def test_regeneration_prunes_deleted_language_and_reinitializes_its_return(self):
        self.delete('de/flea.json')
        self.run_tool()
        self.assertNotIn('de', self.read_manifest())
        self.change_english()
        self.write('de/flea.json', self.translation)
        self.run_tool()
        current = self.read_manifest()
        self.assertEqual(current['de'], current['en'])
        self.assertNotIn('stale translation', self.run_tool('--check').stderr)

    def test_deleted_language_warns_as_orphan_without_failing_check(self):
        self.delete('de/flea.json')
        before = (self.root / 'translation-manifest.json').read_bytes()
        result = self.run_tool('--check')
        self.assertIn('warning: orphaned language de', result.stderr)
        self.assertIn('warning: orphaned entry de:flea:label', result.stderr)
        self.assertEqual(before, (self.root / 'translation-manifest.json').read_bytes())

    def test_readded_key_records_current_english_for_its_new_translation(self):
        self.drop_label()
        self.run_tool()
        self.english['label'] = 'Open a new folder'
        self.translation['label'] = 'Neuen Ordner öffnen'
        self.write('en/flea.json', self.english)
        self.write('de/flea.json', self.translation)
        self.run_tool()
        current = self.read_manifest()
        self.assertNotEqual(current['en']['flea:label'], self.initial['en']['flea:label'])
        self.assertEqual(current['de']['flea:label'], current['en']['flea:label'])
        self.assertNotIn('stale translation', self.run_tool('--check').stderr)

    def test_emptied_translation_loses_its_hash_and_refill_records_current_english(self):
        self.translation['label'] = ''
        self.write('de/flea.json', self.translation)
        self.run_tool()
        self.assertNotIn('flea:label', self.read_manifest()['de'])
        self.run_tool('--check', status=1)
        self.change_english()
        self.translation['label'] = 'Ordner öffnen'
        self.write('de/flea.json', self.translation)
        self.run_tool()
        current = self.read_manifest()
        self.assertEqual(current['de']['flea:label'], current['en']['flea:label'])
        self.assertNotIn('stale translation', self.run_tool('--check').stderr)

    def test_incomplete_plural_loses_its_hash_until_all_forms_are_refilled(self):
        self.translation['count']['other'] = ''
        self.write('de/flea.json', self.translation)
        self.run_tool()
        self.assertNotIn('flea:count', self.read_manifest()['de'])
        self.english['count']['other'] = '%1 total files'
        self.translation['count']['other'] = '%1 Dateien insgesamt'
        self.write('en/flea.json', self.english)
        self.write('de/flea.json', self.translation)
        self.run_tool()
        current = self.read_manifest()
        self.assertEqual(current['de']['flea:count'], current['en']['flea:count'])
        self.assertNotIn('stale translation', self.run_tool('--check').stderr)

    def test_kind_lang_mime_override_is_optional_only_outside_its_owners(self):
        self.english['mime.markdown'] = 'Markdown document'
        self.write('en/flea.json', self.english)
        self.run_tool()
        self.run_tool('--check')
        self.assertNotIn('flea:mime.markdown', self.read_manifest()['de'])
        self.write('ja/flea.json', self.translation)
        result = self.run_tool(status=1)
        self.assertIn('mime.markdown', result.stderr)
        japanese = dict(self.translation, **{'mime.markdown': 'Markdown 文書'})
        self.write('ja/flea.json', japanese)
        self.run_tool()
        self.run_tool('--check')
        self.assertIn('flea:mime.markdown', self.read_manifest()['ja'])
        del self.translation['label']
        self.write('de/flea.json', self.translation)
        self.assertIn('label', self.run_tool(status=1).stderr)


if __name__ == '__main__':
    unittest.main(verbosity=2)

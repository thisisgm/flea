"""Regression contracts for the headless search instrument."""
import importlib.util
import json
import pathlib
import sys
import unittest
from unittest import mock

REPO = pathlib.Path(__file__).resolve().parents[1]
HERE = REPO / 'tools/flea-searchbench'
sys.path.insert(0, str(HERE))


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


class Instrument(unittest.TestCase):
    def test_integrated_executor_ready(self):
        patch = (HERE / 'engine/flea-metrics.patch').read_text()
        self.assertIn('+++ b/src/backend/searchjobs.rs', patch)
        self.assertIn('done.landed', patch)

    def test_matcher_uses_integrated_candidate(self):
        runner = module('matcher', REPO / 'tools/flea-matchbench/runner.py')
        self.assertIn(('flea-search', 'native'), runner.ENTRANTS)
        self.assertEqual(runner.REFERENCE, ('flea-search', 'native'))
        seam = (HERE / 'stage.rs').read_text()
        self.assertRegex(seam, r'session\s*\.query\(')
        self.assertIn('index.push(', seam)

    def test_walker_uses_product_walk(self):
        source = (REPO / 'tools/flea-walkbench/src/adapters.rs').read_text()
        self.assertIn('"flea-search"', source)
        seam = (HERE / 'stage.rs').read_text()
        self.assertIn('searchwalk::walk(', seam)

    def test_source_manifest_is_pinned_and_shared(self):
        import corpus
        manifest = corpus.source_manifest(REPO)
        self.assertEqual(len(manifest), 4902)
        self.assertEqual(corpus.identity_digest(manifest), '13e5fe5a7904f9a5474d5fa915968d42070060d8741bc26a01423c05b3cefaa9')
        paths = {p for p, _ in manifest}
        for _, _, target in corpus.SOURCE_QUERIES:
            self.assertIn(target, paths)
        self.assertEqual(str(corpus.DEFAULT_ROOT), '/home/gm/flea-work/bench-corpora')

    def archive_manifest(self, text):
        import corpus
        with mock.patch.object(pathlib.Path, 'exists', return_value=False), \
                mock.patch.object(pathlib.Path, 'read_text', return_value=text), \
                mock.patch.object(corpus.subprocess, 'run', side_effect=AssertionError('archive ran git')):
            return corpus.source_manifest(pathlib.Path('/source-archive'))

    def test_source_archive_manifest_accepts_the_packaged_inventory(self):
        import corpus
        packaged = corpus.SOURCE_MANIFEST.read_text()
        self.assertEqual(self.archive_manifest(packaged), [tuple(item) for item in json.loads(packaged)])

    def test_source_archive_manifest_rejects_a_short_inventory(self):
        with self.assertRaisesRegex(ValueError, r'source-manifest\.json lists 0 entries, expected 4902'):
            self.archive_manifest('[]')

    def test_source_archive_manifest_rejects_a_renamed_path_at_full_count(self):
        import corpus
        items = json.loads(corpus.SOURCE_MANIFEST.read_text())
        items[0][0] += '.renamed'
        with self.assertRaisesRegex(ValueError, r'source-manifest\.json digest [0-9a-f]{64}, expected ' + corpus.SOURCE_DIGEST):
            self.archive_manifest(json.dumps(items))

    def test_git_checkout_reads_its_own_history(self):
        import corpus
        items = json.loads(corpus.SOURCE_MANIFEST.read_text())
        tree = {'d': b'040000 tree 0', 'f': b'100644 blob 0'}
        stdout = b'\0'.join(tree[kind] + b'\t' + name.encode() for name, kind in items) + b'\0'
        reply = mock.Mock(stdout=stdout)
        with mock.patch.object(pathlib.Path, 'exists', return_value=True), \
                mock.patch.object(pathlib.Path, 'read_text', side_effect=AssertionError('checkout read the package')), \
                mock.patch.object(corpus.subprocess, 'run', return_value=reply) as run:
            self.assertEqual(corpus.source_manifest(pathlib.Path('/checkout')), [tuple(item) for item in items])
        self.assertEqual(run.call_args.args[0], ['git', '-C', '/checkout', 'ls-tree', '-r', '-t', '-z', corpus.COMMIT])

    def test_alternating_pairs(self):
        import campaign
        for run in range(4):
            pairs = list(campaign.pairs(['a', 'b', 'c'], 'a', run))
            self.assertEqual(len(pairs), 2)
            for rival, order in pairs:
                self.assertEqual(order, ['a', rival] if run % 2 == 0 else [rival, 'a'])

    def test_load_is_sampled_and_flagged(self):
        import campaign
        with mock.patch('os.getloadavg', return_value=(4.01, 3.0, 2.0)):
            self.assertTrue(campaign.host_load()['above_4'])
        with mock.patch('os.getloadavg', return_value=(4.0, 3.0, 2.0)):
            self.assertFalse(campaign.host_load()['above_4'])

    def test_unmeasurable_columns_explain_boundary(self):
        import campaign
        for stage in ['engine', 'match', 'walk']:
            caps = campaign.capabilities(stage)
            self.assertTrue(caps['cancel_to_idle']['reason'])
            self.assertFalse(caps['cancel_to_idle']['supported'])
        for stage in ['match', 'walk']:
            self.assertFalse(campaign.capabilities(stage)['watch_freshness']['supported'])

    def test_collector_uses_same_host_cold_validation(self):
        import cold
        with mock.patch.object(cold.clocks, 'cache_gate') as gate:
            cold.cache_gate(pathlib.Path('/marked'), 'cold', {'run': 1})
        gate.assert_called_once_with(pathlib.Path('/marked'), 'cold', {'run': 1})


class Evidence(unittest.TestCase):
    def scratch(self):
        import tempfile
        parent = REPO / '.superpowers/m9-fs-instrument'
        parent.mkdir(parents=True, exist_ok=True)
        root = pathlib.Path(tempfile.mkdtemp(prefix='test-', dir=parent)).resolve()
        (root / '.findbench-root').write_text('test\n')
        return root

    def test_same_host_receipt_and_rejection(self):
        import cold
        import hashlib
        root = self.scratch()
        for host in ['ultra-36', 'ultra-27']:
            with mock.patch.object(cold.clocks.socket, 'gethostname', return_value=host):
                request = cold.clocks.cold_request(root, {'run': 3})
            payload = json.loads(request.read_text())
            response = request.with_name(request.name.replace('request', 'response'))
            receipt = dict(id=payload['id'], host=host, context=payload['context'], exit_code=0,
                           request_sha256=hashlib.sha256(request.read_bytes()).hexdigest(),
                           completed_ns=payload['requested_ns'])
            response.write_text(json.dumps(receipt))
            cold.clocks.validate_cold_response(request, response)
            receipt['host'] = 'wrong-host'
            response.write_text(json.dumps(receipt))
            with self.assertRaises(cold.clocks.ColdGateError):
                cold.clocks.validate_cold_response(request, response)

    def test_summary_requires_matched_run_ids(self):
        import metrics
        root = self.scratch()
        records = []
        for name, number in [('flea/native', 1), ('rival', 2)]:
            records.append(dict(stage='engine', fixture='flea-source', cache='warm', rival='rival',
                                phase='measured', run=number, entrant=name, index_ms=1.0,
                                status='ok', eligible=True, batch={'above_4': False}))
        result = metrics.summarize(root, records, 30)
        self.assertIsNone(result[0]['difference_ci'])
        self.assertEqual(result[0]['winner'], 'unproven')

    def test_headless_archive_skips_external_symlinks(self):
        builder = module('build_stage_test', HERE / 'build-stage.py')
        import io
        import tarfile
        root = self.scratch()
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode='w') as archive:
            member = tarfile.TarInfo('ui/Commons')
            member.type = tarfile.SYMTYPE
            member.linkname = '/usr/share/omarchy/Commons'
            archive.addfile(member)
            member = tarfile.TarInfo('src/entry.rs')
            member.size = 4
            archive.addfile(member, io.BytesIO(b'test'))
        builder.extract_source(data.getvalue(), root)
        self.assertFalse((root / 'ui/Commons').exists())
        self.assertEqual((root / 'src/entry.rs').read_bytes(), b'test')

    def test_versions_are_exact_tokens(self):
        import campaign
        self.assertTrue(campaign.pinned_version('fzf', '0.74.5 (hash)'))
        self.assertFalse(campaign.pinned_version('fzf', '0.74.50 (hash)'))
        self.assertFalse(campaign.pinned_version('fd', 'fd 10.5.0-dev'))
        self.assertTrue(campaign.pinned_version('rg', 'ripgrep 15.2.0 (hash)\nfeatures:default'))

    def test_preflight_requires_external_roster_tools(self):
        import campaign
        from types import SimpleNamespace
        root = self.scratch()
        args = SimpleNamespace(corpus_root=root, engine_build=None, matcher_bin=None,
                               walker_bin=None, flea_bin=None, fzf=None, fd=None, rg=None, cache='warm')
        with mock.patch.object(campaign.socket, 'gethostname', return_value='ultra-36'):
            result = campaign.preflight(args, root)
        for name in ['fzf', 'fd', 'rg']:
            self.assertTrue(any(name in reason for reason in result['blockers']), name)

    def test_preflight_accepts_a_build_directory(self):
        import campaign
        from types import SimpleNamespace
        root = self.scratch()
        build = root / 'build'
        build.mkdir()
        binary = root / 'adapter'
        binary.touch()
        args = SimpleNamespace(corpus_root=root, engine_build=build, matcher_bin=binary,
                               walker_bin=binary, flea_bin=binary, cache='warm')
        with mock.patch.object(campaign.socket, 'gethostname', return_value='ultra-36'):
            result = campaign.preflight(args, root)
        self.assertNotIn('missing build input: ' + str(build), result['blockers'])

    def test_stage_memory_counts_descendants(self):
        import metrics
        with mock.patch.object(metrics, 'descendants', return_value=[11, 22]), \
             mock.patch('engines.process_memory', side_effect=[dict(pss=100, rss=200), dict(pss=300, rss=400)]):
            self.assertEqual(metrics.process_tree_memory(11), dict(pss=400, rss=600))

    def test_head_validation_rejects_truncation(self):
        import metrics
        with self.assertRaises(ValueError):
            metrics.validate_match_head(dict(gen=1, n=500, paths=['entry.rs']), 350)

    def test_production_summary_requires_thirty_pairs(self):
        import metrics
        root = self.scratch()
        records = [dict(stage='walk', fixture='source', cache='warm', rival='rival', phase='measured', run=0,
                        entrant=name, index_ms=value, status='ok', eligible=True, batch={'above_4':False})
                   for name, value in [('flea-search',1),('rival',100)]]
        rows = metrics.summarize(root, records, 1)
        self.assertEqual(rows[0]['winner'], 'unproven')

    def test_registry_staging_preserves_symlink_target(self):
        import corpus
        root = self.scratch()
        target = root / 'owned-sentinel'
        target.write_text('keep')
        (root / '.registry.pending').symlink_to(target)
        fixture = root / 'fixture'
        fixture.mkdir()
        (fixture / '.findbench-root').write_text('test\n')
        tree = fixture / 'tree'
        tree.mkdir()
        (tree / 'entry').touch()
        with mock.patch.object(corpus, 'synthetic', return_value=(fixture, tree, [], {'entries':1})):
            corpus.prepare(root, REPO, ['100000'])
        self.assertEqual(target.read_text(), 'keep')

    def test_files_only_cannot_win_full_index_comparison(self):
        import metrics
        root = self.scratch()
        records = []
        for number in range(30):
            for name, value, comparable in [('flea-search',100,True),('ripgrep',1,False)]:
                records.append(dict(stage='walk', fixture='source', cache='warm', rival='ripgrep', phase='measured',
                                    run=number, entrant=name, index_ms=value, status='ok', eligible=True,
                                    comparable=comparable, batch={'above_4':False}))
        rows = metrics.summarize(root, records, 30)
        self.assertEqual(rows[0]['winner'], 'unproven')
        self.assertIsNone(rows[0]['difference_ci'])

    def test_bootstrap_uses_process_reductions(self):
        import metrics
        self.assertEqual(metrics.p95(list(range(1, 101))), 95)
        ci = metrics.interval([1, 1, 1, 1])
        self.assertEqual(ci, dict(n=4, median=1.0, low=1.0, high=1.0))

    def test_ineligible_samples_cannot_win(self):
        import metrics
        root = self.scratch()
        samples = []
        for number in range(30):
            for entrant in ['flea/native', 'rival']:
                samples.append(dict(stage='engine', fixture='flea-source', cache='warm', rival='rival',
                                    phase='measured', run=number, entrant=entrant, index_ms=0 if entrant == 'flea/native' else 10,
                                    eligible=entrant != 'flea/native', status='ok', batch={'above_4': True}))
        rows = metrics.summarize(root, samples, 30)
        self.assertEqual(rows[0]['winner'], 'unproven')
        self.assertEqual(rows[0]['above_4_pairs'], 30)

    def test_corpus_reuse_rejects_mutated_tree(self):
        import corpus
        import hashlib
        root = self.scratch()
        fixture = root / 'fixture'
        fixture.mkdir()
        (fixture / '.findbench-root').write_text('test\n')
        tree = fixture / 'tree'
        tree.mkdir()
        (tree / 'entry').write_text('data')
        manifest = [['entry', 'f']]
        paths = fixture / 'paths.txt'
        paths.write_text('entry\n')
        (fixture / 'manifest.json').write_text(json.dumps(manifest))
        value = dict(fixture=str(fixture), tree=str(tree), paths=str(paths), info={'entries':1}, identity_sha256=corpus.identity_digest(manifest),
                     paths_sha256=hashlib.sha256(paths.read_bytes()).hexdigest())
        (root / 'registry.json').write_text(json.dumps({'test': value}))
        corpus.load(root, 'test')
        value['info']['entries'] = 2
        (root / 'registry.json').write_text(json.dumps({'test': value}))
        with self.assertRaises(ValueError):
            corpus.load(root, 'test')
        value['info']['entries'] = 1
        (root / 'registry.json').write_text(json.dumps({'test': value}))
        (tree / 'other').write_text('data')
        with self.assertRaises(ValueError):
            corpus.load(root, 'test')


if __name__ == '__main__':
    unittest.main(verbosity=2)

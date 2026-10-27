#!/usr/bin/env python3
"""Adversarial receipt tests with synthetic artifacts, never native acceptance."""

import contextlib
import copy
import base64
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import types
import unittest
import zlib

ROOT = Path(__file__).resolve().parents[1]
SANDBOX = Path(sys.argv.pop(1)).resolve(strict=True)
assert SANDBOX.is_absolute() and (SANDBOX / '.flea-test-sandbox').is_file()
assert SANDBOX.is_relative_to(ROOT / '.superpowers/m9-bench-14')
TOOL = ROOT / 'tools/flea-bench-markdown'
loader = importlib.machinery.SourceFileLoader('markdown_bench', str(TOOL))
spec = importlib.util.spec_from_loader(loader.name, loader)
BENCH = importlib.util.module_from_spec(spec)
loader.exec_module(BENCH)
SYNTHETIC = []


def artifact(name, data):
    path = SANDBOX / name
    assert path.is_absolute() and path.is_relative_to(SANDBOX)
    path.write_bytes(data)
    return dict(path=name, sha256=BENCH.digest(data))


def receipt(name, value):
    return artifact(name, json.dumps(value, allow_nan=False).encode())


def png():
    def chunk(tag, body):
        return struct.pack('>I', len(body)) + tag + body + struct.pack('>I', zlib.crc32(tag + body) & 0xffffffff)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 1, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(b'\x00\xff\x00\x00\x00\x00\xff')) + chunk(b'IEND', b''))


def synthetic_campaign():
    original, _ = BENCH.load_catalog()
    catalog = copy.deepcopy(original)
    catalog['entrants'] = catalog['entrants'][:4]
    wanted = {'source-toggle', 'remote-images', *BENCH.CONTROLS}
    catalog['features'] = [row for row in catalog['features'] if row['id'] in wanted]
    for row in catalog['features']:
        row['fixtures'] = ['table-control'] if row['id'] in BENCH.CONTROLS else ['notes']
        row['scenes'] = {'quicklook': next(iter(row['scenes'].values()))}
    catalog['fixtures'] = {key: catalog['fixtures'][key] for key in ('notes', 'table-control')}
    catalog_hash = '1' * 64
    campaign = BENCH.plan(catalog, catalog_hash)
    proof = artifact('native-output.txt', b'Synthetic test evidence only.\n')
    capture = dict(image=artifact('capture.png', png()), reviewer='synthetic-test', useful=True, matches=True)
    campaign['manifest'].update(host='test-host', kernel='test-kernel', recorded_at='2026-10-24',
                                session='test-session', theme='test-theme', candidate_sha='2' * 40)
    campaign['manifest']['cache_policy'] = {mode: proof for mode in BENCH.MODES}
    campaign['manifest']['calibration'] = receipt('calibration.json', dict(scope='presented-calibration',
        valid=True, clock='CLOCK_MONOTONIC', host='test-host', session='test-session',
        method='synthetic test only', ranges_ms=[0, 1]))
    for key in catalog['questions']:
        campaign['resolutions'][key] = receipt('resolve-' + key + '.json',
                                              dict(conclusion='Synthetic test only', sources=[proof]))
    for entrant in catalog['entrants']:
        key = entrant['id']
        campaign['binaries'][key] = receipt('binary-' + key + '.json',
            dict(scope='installed-product', entrant=key, installed=True, path='/installed/' + key,
                 version=entrant['version'], binary_sha256=BENCH.digest(key.encode()),
                 source_sha=catalog['reference_ref'] if key == 'flea-anchor' else '2' * 40,
                 launch_argv=['/installed/' + key, '--gui'], dependencies=['test'], recorded_at='2026-10-24'))
    for key, fixture in catalog['fixtures'].items():
        outputs = {}
        for name in fixture['files']:
            data = BENCH.control_texts(catalog)[name] if key == 'table-control' else b'# Notes\n'
            outputs[name] = artifact('fixture-' + name.replace('/', '-') , data)
        if key != 'table-control':
            fixture['outputs'] = {name: value['sha256'] for name, value in outputs.items()}
        campaign['fixtures'][key] = receipt('fixtures-' + key + '.json',
            dict(source_sha256=catalog['sources'][fixture['source']]['sha256'],
                 source_blob=catalog['sources'][fixture['source']].get('blob'), selector=fixture['selector'], files=outputs))
    for row in campaign['applicability']:
        row['status'] = 'supported'
        identity = {key: row[key] for key in ('entrant', 'feature', 'surface', 'status')}
        identity.update(scope='product-capability', limitation='product', reason='Synthetic support',
                        observed_at='2026-10-24', capture=capture,
                        binary_sha256=BENCH.digest(row['entrant'].encode()))
        row['receipt'] = receipt('cap-' + '-'.join(identity[key] for key in ('entrant', 'feature', 'surface')) + '.json', identity)
    for opponent in ['strata-stable', 'strata-nightly', 'flea-anchor']:
        for feature in catalog['features']:
            for mode in BENCH.MODES:
                for pair in range(1, 11):
                    first = 'flea' if pair % 2 else opponent
                    for arm in ('flea', opponent):
                        sample = dict(entrant=arm, opponent=opponent, feature=feature['id'], surface='quicklook',
                                      mode=mode, pair=pair, position=0 if arm == first else 1)
                        start = pair * 1_000_000_000 + sample['position'] * 100_000_000
                        observed = dict(sample, kind='gui-presented-useful', clock='CLOCK_MONOTONIC',
                            frame1_useful=True, blankFrames=0, loadingFrames=0, blinkBacks=0,
                            binary_sha256=BENCH.digest(arm.encode()),
                            cache_state='cold-first-use' if mode == 'cold' else 'warm-reuse',
                            process_instance=arm + '-' + opponent + '-' + feature['id'] + ('-' + str(pair) if mode == 'cold' else '-warm'),
                            host='test-host', session='test-session', viewport_px=[1100, 700], theme='test-theme',
                            input_ns=start, presented_ns=start + (2 if arm == 'flea' else 4) * 1_000_000,
                            useful_content='Synthetic feature pixels',
                            fixture_receipts={key: campaign['fixtures'][key]['sha256'] for key in feature['fixtures']},
                            captures={key: capture for key in feature['checks']})
                        if feature['id'] == 'remote-images':
                            observed['network'] = receipt('net-' + '-'.join(map(str, sample.values())) + '.json',
                                dict(entrant=arm, input_ns=start, presented_ns=observed['presented_ns'], remote_requests=[]))
                        sample['observation'] = receipt('sample-' + '-'.join(map(str, sample.values())) + '.json', observed)
                        campaign['samples'].append(sample)
    component_output = artifact('components.txt', ''.join('FIGBENCH%s %-9s n=10 median=5 min=1 max=10 ms\n' %
        (' warm' if mode == 'warm' else '', case) for mode in BENCH.MODES for case in catalog['component_cases']).encode())
    campaign['components'] = [dict(entrant=arm, case=case, mode=mode, kind='helper-response', output=component_output)
                              for arm in ('flea', 'flea-anchor') for case in catalog['component_cases'] for mode in BENCH.MODES]
    return campaign, catalog, catalog_hash


def mutant(replacements):
    source = TOOL.read_text()
    for old, new in replacements:
        assert source.count(old) == 1, 'mutant anchor must match once: ' + old
        source = source.replace(old, new)
    module = types.ModuleType('markdown_bench_mutant')
    module.__file__ = str(TOOL)
    exec(compile(source, str(TOOL), 'exec'), module.__dict__)
    return module


class MatrixTests(unittest.TestCase):
    bench = BENCH

    @classmethod
    def setUpClass(cls):
        if not SYNTHETIC:
            SYNTHETIC.append(synthetic_campaign())
        cls.original, cls.catalog, cls.catalog_hash = SYNTHETIC[0]

    def setUp(self):
        self.campaign = copy.deepcopy(self.original)
        self.catalog = copy.deepcopy(self.catalog)

    def verify(self, catalog=None):
        return self.bench.verify(self.campaign, catalog or self.catalog, self.catalog_hash, SANDBOX)

    def retime(self, sample, elapsed_ms=None, start=None):
        row = json.loads(BENCH.Evidence(SANDBOX).read(sample['observation']))
        begin = row['input_ns'] if start is None else start
        width = row['presented_ns'] - row['input_ns'] if elapsed_ms is None else elapsed_ms * BENCH.NS_PER_MS
        row.update(input_ns=begin, presented_ns=begin + width)
        sample['observation'] = receipt('retimed-' + BENCH.digest(json.dumps(row).encode()) + '.json', row)

    def flea_cell(self, feature, status):
        for row in self.campaign['applicability']:
            if row['entrant'] == 'flea' and row['feature'] == feature:
                row['status'] = status
                self.change_receipt(row, 'receipt', 'status', status)

    def change_receipt(self, owner, key, field, value):
        current = json.loads(BENCH.Evidence(SANDBOX).read(owner[key]))
        current[field] = value
        suffix = BENCH.digest(json.dumps(current).encode())
        owner[key] = receipt('changed-' + self.id() + '-' + suffix + '.json', current)

    def rejected(self, text):
        return self.assertRaisesRegex((ValueError, KeyError), text)

    def test_complete_synthetic_receipts_report_ranges(self):
        result = self.verify()
        self.assertEqual((result['status'], result['superiority']), ('COMPLETE', 'FIRST'))
        self.assertEqual(result['gui'][0]['n'], 10)
        self.assertEqual(result['gui'][0]['median_ms'], 2)
        self.assertEqual(len(result['components']), 12)

    def test_real_catalog_retains_full_scope_and_scene_ids(self):
        catalog, _ = BENCH.load_catalog()
        self.assertEqual(len(catalog['features']), 64)
        self.assertEqual(len(catalog['scenes']), 13)
        self.assertEqual(len(catalog['entrants']), 18)
        families = {row['id'].removeprefix('mermaid-') for row in catalog['features'] if row['id'].startswith('mermaid-')}
        self.assertEqual(families, set('agentflow architecture block c4 class cynefin er eventmodeling flowchart gantt git info ishikawa journey kanban mindmap packet pie quadrant radar railroad requirement sankey sequence state swimlane timeline treemap treeview usecase venn wardley xychart'.split()))
        self.assertTrue(all(row['status'] == 'unresolved' for row in BENCH.plan(catalog, '1' * 64)['applicability']))

    def test_original_generator_writes_exact_controls_and_receipt(self):
        root = SANDBOX / 'original-generator'
        root.mkdir()
        (root / '.flea-test-sandbox').touch()
        process = subprocess.run([sys.executable, '-B', str(ROOT / 'tools/flea-bench-markdown'), 'fixtures', str(root)],
                                 capture_output=True, text=True, check=True)
        result = json.loads(process.stdout)
        self.assertEqual(result['status'], 'GENERATED')
        generated = json.loads((root / 'table-control.json').read_text())
        self.assertEqual(generated['source_blob'], '12ddaf8bd73a9245d8035862ada66efe30cae684')
        rows = {key: (root / value['path']).read_bytes() for key, value in generated['files'].items()}
        self.assertEqual({len(value) for value in rows.values()}, {4096})
        self.assertEqual(rows['ql_md_plain'], rows['ql_md_plain_b'])
        self.assertEqual(sum(line.startswith(b'| Row ') for line in rows['ql_md_table_first'].splitlines()), 40)
        self.assertEqual(rows, BENCH.control_texts(self.catalog))
        print('CONTROL-GENERATOR-EVIDENCE ' + json.dumps(dict(receipt=generated,
            files_base64={key: base64.b64encode(value).decode() for key, value in rows.items()})))
        process = subprocess.run([sys.executable, '-B', str(ROOT / 'tools/flea-bench-markdown'), 'fixtures', str(root)],
                                 capture_output=True, text=True)
        self.assertEqual(process.returncode, 2)
        self.assertEqual(rows, {key: (root / value['path']).read_bytes() for key, value in generated['files'].items()})

    def test_equal_size_twin_preserving_drift_from_original_refused(self):
        descriptor = self.campaign['fixtures']['table-control']
        value = json.loads(BENCH.Evidence(SANDBOX).read(descriptor))
        data = b'drift\n'.ljust(4096, b' ')
        changed = artifact('drifted-control.md', data)
        value['files']['ql_md_plain'] = changed
        value['files']['ql_md_plain_b'] = changed
        self.campaign['fixtures']['table-control'] = receipt('drifted-fixtures.json', value)
        with self.rejected('drift from original generator'):
            self.verify()

    def test_three_helpers_do_not_close_matrix(self):
        self.campaign['samples'] = []
        with self.rejected('missing cold/warm'):
            self.verify()

    def test_missing_rival_refused(self):
        del self.campaign['binaries']['strata-nightly']
        with self.rejected('missing artifact'):
            self.verify()

    def test_missing_capability_refused(self):
        self.campaign['applicability'].pop()
        with self.rejected('missing rival/capability'):
            self.verify()

    def test_unresolved_is_not_unsupported(self):
        self.campaign['applicability'][0]['status'] = 'unresolved'
        with self.rejected('unresolved capability'):
            self.verify()

    def test_observer_limit_is_not_product_unsupported(self):
        row = self.campaign['applicability'][0]
        self.change_receipt(row, 'receipt', 'limitation', 'observer')
        with self.rejected('limitation'):
            self.verify()

    def test_product_unsupported_receipt_is_explicit_and_keeps_other_rivals(self):
        for row in self.campaign['applicability']:
            if row['entrant'] == 'strata-nightly':
                row['status'] = 'unsupported'
                self.change_receipt(row, 'receipt', 'status', 'unsupported')
        self.campaign['samples'] = [row for row in self.campaign['samples'] if row['opponent'] != 'strata-nightly']
        result = self.verify()
        self.assertEqual(result['status'], 'COMPLETE')
        self.assertFalse(any(row['opponent'] == 'strata-nightly' for row in result['gui']))
        self.assertTrue(any(row['opponent'] == 'strata-stable' for row in result['gui']))

    def test_candidate_only_feature_still_requires_twenty_presented_samples(self):
        catalog = copy.deepcopy(self.catalog)
        catalog['features'] = [row for row in catalog['features'] if row['id'] == 'source-toggle']
        capabilities = {key: 'supported' if key[0] == 'flea' else 'unsupported' for key in BENCH.cells(catalog)}
        samples = [row for row in self.campaign['samples'] if row['feature'] == 'source-toggle'
                   and row['opponent'] == 'strata-stable' and row['entrant'] == 'flea']
        for sample in samples:
            sample['opponent'] = 'none'
            sample['position'] = 0
            observed = json.loads(BENCH.Evidence(SANDBOX).read(sample['observation']))
            observed.update(opponent='none', position=0)
            sample['observation'] = receipt('solo-' + sample['mode'] + '-' + str(sample['pair']) + '.json', observed)
        self.campaign['samples'] = samples
        binaries = self.bench.binary_receipts(self.campaign, catalog, self.bench.Evidence(SANDBOX))
        groups = self.bench.sample_groups(self.campaign, catalog, self.bench.Evidence(SANDBOX), binaries, capabilities)
        self.assertEqual(sorted(len(values) for values in groups.values()), [10, 10])
        self.campaign['samples'].pop()
        with self.rejected('missing cold/warm'):
            self.bench.sample_groups(self.campaign, catalog, self.bench.Evidence(SANDBOX), binaries, capabilities)

    def test_strata_channels_cannot_alias(self):
        self.change_receipt(self.campaign['binaries'], 'strata-nightly', 'binary_sha256', BENCH.digest(b'strata-stable'))
        with self.rejected('alias'):
            self.verify()

    def test_wrong_installed_version_refused(self):
        self.change_receipt(self.campaign['binaries'], 'flea-anchor', 'version', '0.3.10')
        with self.rejected('wrong installed version'):
            self.verify()

    def test_rebuilt_anchor_refused(self):
        self.change_receipt(self.campaign['binaries'], 'flea-anchor', 'installed', False)
        with self.rejected('installed'):
            self.verify()

    def test_dev_launch_refused(self):
        self.change_receipt(self.campaign['binaries'], 'flea', 'launch_argv', ['/installed/flea'])
        with self.rejected('--gui'):
            self.verify()

    def test_wrong_catalog_identity_refused(self):
        self.campaign['matrix_sha256'] = 'f' * 64
        with self.rejected('scope/catalog'):
            self.verify()

    def test_wrong_flea_source_identity_refused(self):
        for key in ('flea', 'flea-anchor'):
            with self.subTest(key):
                self.campaign = copy.deepcopy(self.original)
                self.change_receipt(self.campaign['binaries'], key, 'source_sha', '3' * 40)
                with self.rejected('wrong Flea source identity: changed-'):
                    self.verify()

    def test_wrong_instrument_identity_refused(self):
        self.campaign['instrument_sha256'] = '0' * 64
        with self.rejected('instrument identity changed'):
            self.verify()

    def test_invalid_or_foreign_calibration_refused(self):
        for field, value in (('valid', False), ('host', 'other-host'), ('session', 'other-session')):
            with self.subTest(field):
                self.campaign = copy.deepcopy(self.original)
                self.change_receipt(self.campaign['manifest'], 'calibration', field, value)
                with self.rejected('receipt identity mismatch: ' + field + ' in changed-'):
                    self.verify()

    def test_missing_cache_policy_receipt_refused(self):
        for mode in BENCH.MODES:
            with self.subTest(mode):
                self.campaign = copy.deepcopy(self.original)
                self.campaign['manifest']['cache_policy'][mode] = None
                with self.rejected('missing artifact/hash'):
                    self.verify()

    def test_unanswered_question_refused(self):
        key = next(iter(self.catalog['questions']))
        self.change_receipt(self.campaign['resolutions'], key, 'conclusion', '')
        with self.rejected('unresolved source/native question: ' + key):
            self.verify()

    def test_slower_flea_cell_is_not_first(self):
        result = self.verify()
        self.assertEqual(result['superiority'], 'FIRST')
        for sample in self.campaign['samples']:
            if (sample['entrant'], sample['opponent'], sample['feature'], sample['mode']) == ('flea', 'strata-stable', 'source-toggle', 'cold'):
                self.retime(sample, 5)
        result = self.verify()
        self.assertEqual((result['status'], result['superiority']), ('COMPLETE', 'NOT_FIRST'))

    def test_table_control_hold_turns_first_into_not_first(self):
        for sample in self.campaign['samples']:
            if sample['entrant'] == 'flea' and sample['feature'] == 'ql_md_table_first':
                self.retime(sample, 3)
        result = self.verify()
        self.assertTrue(all(row['median_ms'] < 4 for row in result['gui'] if row['entrant'] == 'flea'))
        self.assertIn('HOLD', {row['verdict'] for row in result['table_controls']})
        self.assertEqual((result['status'], result['superiority']), ('COMPLETE', 'NOT_FIRST'))

    def run_main(self, campaign, *argv):
        path = SANDBOX / 'campaign.json'
        path.write_text(json.dumps(campaign))
        original, sys.argv = sys.argv, ['flea-bench-markdown', *argv, str(path)]
        self.bench.load_catalog, saved = (lambda: (self.catalog, self.catalog_hash)), self.bench.load_catalog
        stdout = io.StringIO()
        try:
            with contextlib.redirect_stdout(stdout):
                code = self.bench.main()
        finally:
            sys.argv, self.bench.load_catalog = original, saved
        return code, json.loads(stdout.getvalue())

    def test_cli_exit_codes_separate_not_first_from_blocked(self):
        code, value = self.run_main(self.campaign, 'verify')
        self.assertEqual((code, value['superiority']), (0, 'FIRST'))
        slow = copy.deepcopy(self.campaign)
        for sample in slow['samples']:
            if sample['entrant'] == 'flea' and sample['feature'] == 'source-toggle':
                self.retime(sample, 9)
        code, value = self.run_main(slow, 'verify')
        self.assertEqual((code, value['status'], value['superiority']), (1, 'COMPLETE', 'NOT_FIRST'))

    def test_malformed_receipts_block_with_exit_two(self):
        def binary_path_null(campaign):
            self.change_receipt(campaign['binaries'], 'flea', 'path', None)
        def capture_null(campaign):
            self.change_receipt(campaign['applicability'][0], 'receipt', 'capture', None)
        def captures_list(campaign):
            sample = next(row for row in campaign['samples'] if row['feature'] == 'source-toggle')
            self.change_receipt(sample, 'observation', 'captures', ['rest', 'source', 'toggle-r', 'line-count'])
        def manifest_null(campaign):
            campaign['manifest'] = None
        for breaker in (binary_path_null, capture_null, captures_list, manifest_null):
            with self.subTest(breaker.__name__):
                campaign = copy.deepcopy(self.original)
                breaker(campaign)
                code, value = self.run_main(campaign, 'verify')
                self.assertEqual((code, value['status']), (2, 'BLOCKED'))
        code, value = self.run_main([], 'verify')
        self.assertEqual((code, value['status']), (2, 'BLOCKED'))

    def test_presented_pair_order_must_match_timestamps(self):
        first = next(row for row in self.campaign['samples'] if row['position'] == 0 and row['feature'] == 'source-toggle')
        peer = next(row for row in self.campaign['samples'] if row['position'] == 1 and all(
            row[key] == first[key] for key in ('opponent', 'feature', 'surface', 'mode', 'pair')))
        later = json.loads(BENCH.Evidence(SANDBOX).read(peer['observation']))['input_ns'] + 1
        self.retime(first, start=later)
        with self.rejected('presented pair ordering disagrees with capture timestamps: '):
            self.verify()

    def test_warm_group_must_share_one_process(self):
        sample = next(row for row in self.campaign['samples'] if row['mode'] == 'warm' and row['feature'] == 'source-toggle')
        self.change_receipt(sample, 'observation', 'process_instance', 'a-second-warm-process')
        with self.rejected('cold/warm process reuse disagrees: flea/'):
            self.verify()

    def test_candidate_capability_guards(self):
        cases = (('required', True, 'unsupported', 'required candidate feature unsupported: source-toggle/quicklook'),
                 ('anchor', False, 'supported', 'candidate lost reference capability: source-toggle/quicklook'),
                 ('rival', False, 'unsupported', 'candidate missing applicable feature: source-toggle/quicklook'))
        for name, required, anchor, message in cases:
            with self.subTest(name):
                self.campaign = copy.deepcopy(self.original)
                catalog = copy.deepcopy(self.catalog)
                next(row for row in catalog['features'] if row['id'] == 'source-toggle')['candidate_required'] = required
                self.flea_cell('source-toggle', 'unsupported')
                for row in self.campaign['applicability']:
                    if row['entrant'] == 'flea-anchor' and row['feature'] == 'source-toggle':
                        row['status'] = anchor
                        self.change_receipt(row, 'receipt', 'status', anchor)
                with self.rejected(message):
                    self.verify(catalog)

    def test_source_identity_check_refuses_drift(self):
        source = SANDBOX / 'source-identity.txt'
        source.write_bytes(b'pinned source\n')
        catalog = dict(sources=dict(probe=dict(kind='file', path=str(source), sha256=BENCH.digest(b'pinned source\n'))))
        self.assertEqual(self.bench.check_sources(catalog, ROOT)['status'], 'SOURCE_IDENTITIES_MATCH')
        source.write_bytes(b'drifted source\n')
        with self.rejected('source identity changed: probe'):
            self.bench.check_sources(catalog, ROOT)

    def test_recipe_fixture_must_equal_pinned_output(self):
        value = json.loads(BENCH.Evidence(SANDBOX).read(self.campaign['fixtures']['notes']))
        value['files']['notes.md'] = artifact('one-line-notes.md', b'# One line\n')
        self.campaign['fixtures']['notes'] = receipt('one-line-fixtures.json', value)
        with self.rejected('fixture output differs from pinned recipe: notes/notes.md'):
            self.verify()
        del self.catalog['fixtures']['notes']['outputs']
        with self.rejected('fixture output unpinned; pin original bytes before timing: notes'):
            self.verify()

    def test_real_catalog_pins_every_recipe_output(self):
        catalog, _ = BENCH.load_catalog()
        for key, fixture in catalog['fixtures'].items():
            with self.subTest(key):
                if fixture['selector'] == 'original-board-fixture':
                    self.assertEqual(fixture['question'], 'board-specimens')
                    self.assertNotIn('outputs', fixture)
                elif fixture['selector'] not in ('whole-file', 'ql_texts'):
                    self.assertEqual(list(fixture['outputs']), fixture['files'])
                    self.assertTrue(all(BENCH.hex_hash(value) for value in fixture['outputs'].values()))

    def test_rival_without_table_support_keeps_plain_twin_controls(self):
        for row in self.campaign['applicability']:
            if row['entrant'] == 'strata-nightly' and row['feature'] == 'ql_md_table_first':
                row['status'] = 'unsupported'
                self.change_receipt(row, 'receipt', 'status', 'unsupported')
        self.campaign['samples'] = [row for row in self.campaign['samples']
                                    if (row['opponent'], row['feature']) != ('strata-nightly', 'ql_md_table_first')]
        result = self.verify()
        self.assertEqual(result['status'], 'COMPLETE')
        partial = [row for row in result['table_controls'] if row['opponent'] == 'strata-nightly']
        self.assertEqual({row['verdict'] for row in partial}, {'PARTIAL'})
        self.assertTrue(all('plain_p95_ms' in row for row in partial))

    def test_measure_commands_cover_resolved_cells_and_solo_candidate(self):
        driver = SANDBOX / 'stub-driver.py'
        driver.write_text('')
        catalog = self.catalog
        campaign = self.bench.plan(catalog, self.catalog_hash)
        for row in campaign['applicability']:
            solo = row['feature'] == 'source-toggle' and row['entrant'] != 'flea'
            dropped = row['feature'] == 'remote-images' and row['entrant'] == 'strata-nightly'
            row['status'] = 'unsupported' if solo or dropped else 'supported'
        result = self.bench.planned_commands(campaign, catalog, SANDBOX, 'measure', driver)
        got = {(row['entrant'], row['feature'], row['surface']) for row in result['commands']}
        entrants = [row['id'] for row in catalog['entrants']]
        want = {('none', 'source-toggle', 'quicklook')}
        for feature in catalog['features']:
            if feature['id'] == 'source-toggle':
                continue
            for surface in feature['scenes']:
                want.update((entrant, feature['id'], surface) for entrant in entrants if entrant != 'flea'
                            and not (feature['id'] == 'remote-images' and entrant == 'strata-nightly'))
        self.assertEqual(got, want)
        self.assertTrue(all(row['argv'][1] == str(driver) and row['argv'][4] == 'measure' for row in result['commands']))
        with self.rejected('native driver must be an existing absolute file'):
            self.bench.planned_commands(campaign, catalog, SANDBOX, 'measure', SANDBOX / 'absent-driver.py')

    def test_fixture_root_needs_marker(self):
        root = Path(tempfile.mkdtemp(prefix='unmarked-fixture-root.', dir=SANDBOX))
        with self.rejected('fixture root must hold a .flea-test-sandbox marker'):
            self.bench.generate_controls(root, self.catalog)
        self.assertEqual(list(root.iterdir()), [])

    def test_expand_roots_refuses_unset_root_and_substitutes_set_root(self):
        names = ('FLEA_VISUAL_DESIGN', 'FLEA_RELEASE_ROOT')
        saved = {name: os.environ.pop(name, None) for name in names}
        try:
            for name in names:
                path = '${' + name + '}/plan.md'
                with self.rejected(name + ' is unset'):
                    self.bench.expand_roots(path)
                os.environ[name] = '/srv/' + name.lower()
                self.assertEqual(self.bench.expand_roots(path), '/srv/' + name.lower() + '/plan.md')
                del os.environ[name]
        finally:
            for name, value in saved.items():
                os.environ.pop(name, None)
                if value is not None:
                    os.environ[name] = value

    def test_missing_in_root_artifact_named(self):
        self.campaign['samples'][0]['observation']['path'] = 'absent-observation.json'
        with self.rejected('missing artifact file: absent-observation.json'):
            self.verify()

    def test_blank_frame_refused(self):
        self.change_receipt(self.campaign['samples'][0], 'observation', 'frame1_useful', False)
        with self.rejected('frame1_useful'):
            self.verify()

    def test_loading_frame_refused(self):
        self.change_receipt(self.campaign['samples'][0], 'observation', 'loadingFrames', 1)
        with self.rejected('blank/loading'):
            self.verify()

    def test_foreign_session_refused(self):
        self.change_receipt(self.campaign['samples'][0], 'observation', 'session', 'old-session')
        with self.rejected('receipt identity mismatch: session in changed-'):
            self.verify()

    def test_cold_process_reuse_refused(self):
        sample = self.campaign['samples'][2]
        first = json.loads(BENCH.Evidence(SANDBOX).read(self.campaign['samples'][0]['observation']))
        self.change_receipt(sample, 'observation', 'process_instance', first['process_instance'])
        with self.rejected('cold first-use process reused'):
            self.verify()

    def test_helper_event_is_not_gui_completion(self):
        self.change_receipt(self.campaign['samples'][0], 'observation', 'kind', 'helper-response')
        with self.rejected('kind'):
            self.verify()

    def test_missing_source_toggle_capture_refused(self):
        sample = next(row for row in self.campaign['samples'] if row['feature'] == 'source-toggle')
        self.change_receipt(sample, 'observation', 'captures', {})
        with self.rejected('missing source toggle'):
            self.verify()

    def test_remote_fetch_refused(self):
        sample = next(row for row in self.campaign['samples'] if row['feature'] == 'remote-images')
        row = json.loads(BENCH.Evidence(SANDBOX).read(sample['observation']))
        network = json.loads(BENCH.Evidence(SANDBOX).read(row['network']))
        network['remote_requests'] = ['https://cdn.example.com/shot.png']
        row['network'] = receipt('network-fetch.json', network)
        sample['observation'] = receipt('observed-network-fetch.json', row)
        with self.rejected('remote_requests'):
            self.verify()

    def test_missing_warm_sample_refused(self):
        self.campaign['samples'] = [row for row in self.campaign['samples'] if row['mode'] != 'warm']
        with self.rejected('missing cold/warm sample or matched pair: [0-9]+, first .*/warm/'):
            self.verify()

    def test_missing_cold_sample_refused(self):
        self.campaign['samples'] = [row for row in self.campaign['samples'] if row['mode'] != 'cold']
        with self.rejected('missing cold/warm'):
            self.verify()

    def test_duplicate_sample_cannot_fill_gap(self):
        self.campaign['samples'][-1] = copy.deepcopy(self.campaign['samples'][0])
        with self.rejected('duplicate'):
            self.verify()

    def test_warm_cold_label_swap_refused(self):
        self.change_receipt(self.campaign['samples'][0], 'observation', 'cache_state', 'warm-reuse')
        with self.rejected('cache_state'):
            self.verify()

    def test_pair_order_refused(self):
        self.campaign['samples'][0]['position'] = 1
        with self.rejected('pair order'):
            self.verify()

    def test_hash_tamper_refused(self):
        self.campaign['samples'][0]['observation']['sha256'] = '0' * 64
        with self.rejected('hash mismatch'):
            self.verify()

    def test_artifact_escape_refused(self):
        path = '../../../tools/flea-bench-markdown'
        self.assertTrue((SANDBOX / path).resolve().is_file())
        self.campaign['samples'][0]['observation']['path'] = path
        with self.rejected('artifact escapes campaign root: ' + path):
            self.verify()

    def test_uninspected_capture_refused(self):
        sample = self.campaign['samples'][0]
        row = json.loads(BENCH.Evidence(SANDBOX).read(sample['observation']))
        next(iter(row['captures'].values()))['reviewer'] = ''
        sample['observation'] = receipt('uninspected.json', row)
        with self.rejected('inspected useful'):
            self.verify()

    def test_missing_component_refused(self):
        self.campaign['components'].pop()
        with self.rejected('missing component'):
            self.verify()

    def test_twin_noise_and_table_regression(self):
        times = {('flea', 'flea-anchor', feature, 'quicklook', 'cold'): [value] * 10
                 for feature, value in zip(BENCH.CONTROLS, [30, 10, 10])}
        self.assertEqual(self.bench.twin_controls(times, 10)[0]['verdict'], 'HOLD')
        times['flea', 'flea-anchor', BENCH.CONTROLS[2], 'quicklook', 'cold'] = [20] * 10
        with self.rejected('INSTRUMENT-NOISE'):
            self.bench.twin_controls(times, 10)

    def test_actual_cli_plan_is_unmeasured_and_refuses_acceptance(self):
        process = subprocess.run([sys.executable, '-B', str(ROOT / 'tools/flea-bench-markdown'), 'plan'],
                                 capture_output=True, text=True, check=True)
        value = json.loads(process.stdout)
        self.assertEqual(value['status'], 'NOT_RUN')
        self.assertGreater(len(value['applicability']), 2000)
        plan_file = artifact('plan.json', process.stdout.encode())
        process = subprocess.run([sys.executable, '-B', str(ROOT / 'tools/flea-bench-markdown'),
                                  'verify', str(SANDBOX / plan_file['path'])], capture_output=True, text=True)
        self.assertEqual(process.returncode, 2)
        self.assertEqual(json.loads(process.stdout)['status'], 'BLOCKED')

    def test_offline_command_generation_keeps_every_rival_and_refuses_unresolved_timing(self):
        catalog, catalog_hash = BENCH.load_catalog()
        campaign = BENCH.plan(catalog, catalog_hash)
        output = artifact('campaign.json', json.dumps(campaign).encode())
        argv = [sys.executable, '-B', str(ROOT / 'tools/flea-bench-markdown'), 'commands', str(SANDBOX / output['path'])]
        process = subprocess.run(argv, capture_output=True, text=True)
        self.assertEqual(process.returncode, 2)
        self.assertIn('native driver must be an existing absolute file', json.loads(process.stdout)['reason'])
        driver = SANDBOX / 'cli-stub-driver.py'
        driver.write_text('')
        argv += ['--driver', str(driver)]
        process = subprocess.run(argv, capture_output=True, text=True, check=True)
        commands = json.loads(process.stdout)
        self.assertEqual(commands['status'], 'PLAN_ONLY')
        self.assertEqual(len(commands['commands']), 2196)
        self.assertEqual(commands['commands'][0]['argv'][-4:], ['capability', 'strata-stable', 'rendered-default', 'quicklook'])
        order = list(dict.fromkeys(row['entrant'] for row in commands['commands']))
        self.assertEqual(order, [row['id'] for row in catalog['entrants']])
        process = subprocess.run(argv + ['--stage', 'measure'], capture_output=True, text=True)
        self.assertEqual(process.returncode, 2)
        self.assertIn('awaits native applicability', json.loads(process.stdout)['reason'])

    def test_no_tty_and_minimal_environment(self):
        self.assertFalse(os.isatty(0))
        self.assertNotIn('HOME', os.environ)


# Each guard a review named, with the edit that deletes it (or restores the code before the fix); its test must go red.
MUTANTS = {
    'test_wrong_flea_source_identity_refused': [("            require(row.get('source_sha') == expected, 'wrong Flea source identity: ' + where)\n", '')],
    'test_wrong_instrument_identity_refused': [("    require(campaign.get('instrument_sha256') == digest(Path(__file__).read_bytes()), 'instrument identity changed')\n", '')],
    'test_invalid_or_foreign_calibration_refused': [("valid=True, clock='CLOCK_MONOTONIC',\n                                      host=manifest['host'], session=manifest['session']", "clock='CLOCK_MONOTONIC'")],
    'test_missing_cache_policy_receipt_refused': [("        evidence.read(policy.get(mode))", "        pass")],
    'test_unanswered_question_refused': [("        require(answer.get('conclusion') and answer.get('sources'), 'unresolved source/native question: ' + question)\n", '')],
    'test_slower_flea_cell_is_not_first': [("if wins and all(wins) and not holds", "if wins and any(wins) and not holds")],
    'test_table_control_hold_turns_first_into_not_first': [("if wins and all(wins) and not holds", "if wins and all(wins)")],
    'test_cli_exit_codes_separate_not_first_from_blocked': [("return 1 if result.get('superiority') == 'NOT_FIRST' else 0", "return 0")],
    'test_malformed_receipts_block_with_exit_two': [
        ("    except Exception as error:", "    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:"),
        ("require(isinstance(row.get('path'), str) and row['path'].startswith('/')", "require(row.get('path', '').startswith('/')")],
    'test_presented_pair_order_must_match_timestamps': [("require(len(pair) == 1 or pair[0] < pair[1],", "require(True,")],
    'test_warm_group_must_share_one_process': [("(len(pairs) if group[-1] == 'cold' else 1)", "(len(pairs) if group[-1] == 'cold' else len(set(instances)))")],
    'test_candidate_capability_guards': [("require(not feature['candidate_required'] or current == 'supported',", "require(True,")],
    'test_source_identity_check_refuses_drift': [("        require(digest(data) == source['sha256'], 'source identity changed: ' + key)\n", '')],
    'test_recipe_fixture_must_equal_pinned_output': [("        elif key != 'table-control':", "        elif False:")],
    'test_rival_without_table_support_keeps_plain_twin_controls': [("require(all(row is None or len(row) == pairs", "require(all(row is not None and len(row) == pairs")],
    'test_measure_commands_cover_resolved_cells_and_solo_candidate': [("                entrant = 'none'\n", '')],
    'test_fixture_root_needs_marker': [("    require((root / '.flea-test-sandbox').is_file(), 'fixture root must hold a .flea-test-sandbox marker: ' + str(root))\n", '')],
    'test_expand_roots_refuses_unset_root_and_substitutes_set_root': [("            path = path.replace(token, os.environ[name])\n", '')],
    'test_artifact_escape_refused': [("        require(target.is_relative_to(self.root), 'artifact escapes campaign root: ' + path)\n", '')],
    'test_missing_in_root_artifact_named': [("        require(target.is_relative_to(self.root), 'artifact escapes campaign root: ' + path)\n        require(target.is_file(), 'missing artifact file: ' + path)",
                                            "        require(target.is_relative_to(self.root) and target.is_file(), 'artifact escapes campaign root')")],
}


class MutantTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        MatrixTests.setUpClass()

    def test_every_named_guard_has_a_red_mutant(self):
        for name, replacements in MUTANTS.items():
            with self.subTest(name):
                self.assertTrue(callable(getattr(MatrixTests, name, None)), 'unknown test: ' + name)
                case = type('Mutant', (MatrixTests,), dict(bench=mutant(replacements)))(name)
                result = unittest.TestResult()
                with contextlib.redirect_stdout(io.StringIO()):
                    case.run(result)
                self.assertFalse(result.wasSuccessful(), 'mutant stayed green: ' + name)
                print('MUTANT-RED', name, len(result.failures), 'failures', len(result.errors), 'errors')


if __name__ == '__main__':
    unittest.main(verbosity=2)

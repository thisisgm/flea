#!/usr/bin/env python3
"""Adversarial receipt tests with synthetic artifacts, never native acceptance."""

import copy
import base64
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import unittest
import zlib

ROOT = Path(__file__).resolve().parents[1]
SANDBOX = Path(sys.argv.pop(1)).resolve(strict=True)
assert SANDBOX.is_absolute() and (SANDBOX / '.flea-test-sandbox').is_file()
assert SANDBOX.is_relative_to(ROOT / '.superpowers/m9-bench-14')
loader = importlib.machinery.SourceFileLoader('markdown_bench', str(ROOT / 'tools/flea-bench-markdown'))
spec = importlib.util.spec_from_loader(loader.name, loader)
BENCH = importlib.util.module_from_spec(spec)
loader.exec_module(BENCH)


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


class MatrixTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original, cls.catalog, cls.catalog_hash = synthetic_campaign()

    def setUp(self):
        self.campaign = copy.deepcopy(self.original)

    def verify(self):
        return BENCH.verify(self.campaign, self.catalog, self.catalog_hash, SANDBOX)

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
        binaries = BENCH.binary_receipts(self.campaign, catalog, BENCH.Evidence(SANDBOX))
        groups = BENCH.sample_groups(self.campaign, catalog, BENCH.Evidence(SANDBOX), binaries, capabilities)
        self.assertEqual(sorted(len(values) for values in groups.values()), [10, 10])
        self.campaign['samples'].pop()
        with self.rejected('missing cold/warm'):
            BENCH.sample_groups(self.campaign, catalog, BENCH.Evidence(SANDBOX), binaries, capabilities)

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

    def test_wrong_source_identity_refused(self):
        self.campaign['matrix_sha256'] = 'f' * 64
        with self.rejected('scope/catalog'):
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
        with self.rejected('session'):
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
        with self.rejected('missing cold/warm'):
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
        self.campaign['samples'][0]['observation']['path'] = '../../tools/flea-bench-markdown'
        with self.rejected('escapes'):
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
        self.assertEqual(BENCH.twin_controls(times)[0]['verdict'], 'HOLD')
        times['flea', 'flea-anchor', BENCH.CONTROLS[2], 'quicklook', 'cold'] = [20] * 10
        with self.rejected('INSTRUMENT-NOISE'):
            BENCH.twin_controls(times)

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


if __name__ == '__main__':
    unittest.main(verbosity=2)

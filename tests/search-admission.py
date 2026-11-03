"""GAP80 counterfactuals through each public search ranking entry point."""
import csv
import importlib.util
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parents[1] / 'tools/flea-searchbench'
sys.path.insert(0, str(HERE))
from fair.eligibility import compare as original_compare
from metrics import summarize

spec = importlib.util.spec_from_file_location('secondary_compare', HERE / 'engine/compare.py')
secondary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(secondary)


def rows(pilots=5, metric='query_p95_ms', unit='ms', resolution=None):
    result = []
    for engine, value in [('candidate', 10), ('rival', 20)]:
        for phase, count in [('noise-a', pilots), ('noise-b', pilots), ('measured', 30)]:
            for run in range(count):
                result.append(dict(engine=engine, phase=phase, run=run, fixture='fixture',
                                   profile='native', query_kind='trace', prefix='', query='',
                                   metric=metric, unit=unit, value=value, resolution=resolution,
                                   status='ok', eligible='yes', work='common', quality='ok'))
    return result


def primary(data, minimum=30):
    samples = []
    for row in data:
        samples.append(dict(stage='engine', fixture=row['fixture'], cache='warm', rival='rival',
                            reference='candidate', entrant=row['engine'],
                            phase={'noise-a': 'pilot-a', 'noise-b': 'pilot-b'}.get(row['phase'], row['phase']),
                            run=row['run'], status=row['status'], eligible=row['eligible'] == 'yes',
                            batch={'above_4': False}, resolutions={row['metric']: row['resolution']},
                            **{row['metric']: row['value']}))
    with tempfile.TemporaryDirectory(prefix='search-admission-') as directory:
        reports = summarize(pathlib.Path(directory), samples, minimum)
    report = next(r for r in reports if r['metric'] == data[0]['metric'])
    return dict(report, verdict={'candidate': 'win', 'rival': 'loss', 'overlap': 'tie-or-unresolved'}.get(
        report['winner'], report.get('admission', 'unproven')))


class Admission(unittest.TestCase):
    def callers(self):
        return [('summary', primary), ('secondary', lambda data, **kw: secondary.compare(data, 'candidate', 'rival', **kw)[0]),
                ('retained', lambda data, **kw: original_compare(data, 'candidate', 'rival', **kw)[0])]

    def test_l_search_gap80_one_observation_batch_is_refused(self):
        for name, compare in self.callers():
            with self.subTest(path=name):
                self.assertEqual(compare(rows(1))['verdict'], 'missing-metric-pilot')

    def test_l_search_gap80_secondary_cli_refuses_one_observation(self):
        with tempfile.TemporaryDirectory(prefix='search-admission-') as directory:
            root = pathlib.Path(directory)
            data = rows(1)
            with (root / 'input.tsv').open('w') as stream:
                writer = csv.DictWriter(stream, fieldnames=list(data[0]), delimiter='\t')
                writer.writeheader()
                writer.writerows(data)
            subprocess.run([sys.executable, '-B', str(HERE / 'engine/compare.py'),
                            '--input', str(root / 'input.tsv'), '--output', str(root / 'output.json'),
                            '--candidate', 'candidate', '--reference', 'rival'], check=True)
            self.assertEqual(json.loads((root / 'output.json').read_text())[0]['verdict'], 'missing-metric-pilot')

    def test_l_search_gap80_repeated_observations_are_not_five_runs(self):
        data = rows()
        for row in data:
            if row['phase'] != 'measured':
                row['run'] = 0
        for name, compare in self.callers():
            with self.subTest(path=name):
                self.assertEqual(compare(data)['verdict'], 'missing-metric-pilot')

    def test_l_search_gap80_missing_or_nonfive_pilots_cannot_rank(self):
        for count in (0, 4, 6):
            for name, compare in self.callers():
                with self.subTest(path=name, count=count):
                    self.assertEqual(compare(rows(count))['verdict'], 'missing-metric-pilot')

    def test_l_search_gap80_five_plus_five_admits_both_directions(self):
        for name, compare in self.callers():
            data = rows()
            with self.subTest(path=name):
                self.assertEqual(compare(data)['verdict'], 'win')
                for row in data:
                    row['value'] = 30 - row['value']
                self.assertEqual(compare(data)['verdict'], 'loss')

    def test_l_search_gap80_each_entrant_and_metric_needs_its_own_pilots(self):
        data = rows() + rows(metric='index_ms')
        data = [r for r in data if not (r['metric'] == 'query_p95_ms' and r['engine'] == 'rival'
                                      and r['phase'] == 'noise-b' and r['run'] > 0)]
        for name, compare in self.callers():
            with self.subTest(path=name):
                target = [r for r in data if r['metric'] == 'query_p95_ms']
                self.assertEqual(compare(target)['verdict'], 'missing-metric-pilot')
                self.assertEqual(compare([r for r in data if r['metric'] == 'index_ms'])['verdict'], 'win')
        self.assertEqual(primary(data)['verdict'], 'missing-metric-pilot')
        for compare in (secondary.compare, original_compare):
            reports = {r['metric']: r for r in compare(data, 'candidate', 'rival')}
            self.assertEqual(reports['query_p95_ms']['verdict'], 'missing-metric-pilot')
            self.assertEqual(reports['index_ms']['verdict'], 'win')

    def test_l_search_gap80_noise_and_resolution_block_apparent_gain(self):
        for name, compare in self.callers():
            for blocker in ('noise', 'resolution'):
                data = rows(resolution=11 if blocker == 'resolution' else None)
                if blocker == 'noise':
                    for row in data:
                        if row['engine'] == 'rival' and row['phase'] == 'noise-b':
                            row['value'] += 6
                with self.subTest(path=name, blocker=blocker):
                    self.assertEqual(compare(data)['verdict'], 'tie-or-unresolved')

    def test_l_search_gap80_metric_specific_resolution(self):
        for name, compare in self.callers():
            for metric, unit, gain in [('query_p95_ms', 'ms', .0000005),
                                       ('memory_bytes', 'bytes', .5), ('cpu_ticks_ms', 'ms', .5)]:
                if name == 'summary' and metric == 'cpu_ticks_ms':
                    continue
                data = rows(metric=metric, unit=unit)
                for row in data:
                    row['value'] = 10 + (gain if row['engine'] == 'rival' else 0)
                with self.subTest(path=name, metric=metric):
                    self.assertEqual(compare(data)['verdict'], 'tie-or-unresolved')
        self.assertEqual(primary(rows(metric='memory_bytes', unit='bytes'))['verdict'], 'tie-or-unresolved')

    def test_l_search_gap80_thirty_paired_runs_cannot_be_lowered(self):
        data = [r for r in rows() if r['phase'] != 'measured' or r['run'] == 0]
        for name, compare in self.callers():
            with self.subTest(path=name):
                self.assertEqual(compare(data, minimum=1)['verdict'], 'insufficient-pairs')

    def test_l_search_gap80_quality_is_descriptive_and_cannot_bypass_admission(self):
        for count in (0, 1, 5):
            data = rows(count, metric='quality_mrr', unit='ratio')
            for row in data:
                row['value'] /= 20
            self.assertEqual(primary(data)['winner'], 'unproven')

    def test_l_search_gap80_invalid_resolution_is_refused(self):
        for name, compare in self.callers():
            for resolution in (0, -1, float('nan'), float('inf')):
                with self.subTest(path=name, resolution=resolution):
                    with self.assertRaises(ValueError) as raised:
                        compare(rows(resolution=resolution))
                    for context in ('searchbench', 'candidate', 'query_p95_ms', repr(resolution)):
                        self.assertIn(context, str(raised.exception))


if __name__ == '__main__':
    unittest.main(verbosity=2)

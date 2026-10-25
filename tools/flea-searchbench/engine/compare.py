"""Paired run-clustered comparisons, with metric-specific pilot noise."""
import argparse
import collections
import csv
import json
import os
import pathlib
import random
import statistics


GROUP = ['fixture', 'profile', 'query_kind', 'prefix', 'query', 'metric', 'unit', 'work', 'quality']


def compare(samples, candidate, reference, minimum=30, iterations=2000):
    resolutions = collections.defaultdict(list)
    groups = collections.defaultdict(lambda: collections.defaultdict(lambda: collections.defaultdict(list)))
    for row in samples:
        if row['engine'] not in {candidate, reference} or row['eligible'] != 'yes':
            continue
        if row['status'] not in {'ok', 'coarse'} or row['value'] in ('', None):
            continue
        if row.get('quality') not in {'', None, 'ok'}:
            continue
        if row['unit'] not in {'us', 'ms', 'bytes', 'bytes/entry'}:
            continue
        key = tuple(row.get(field, '') for field in GROUP)
        if row.get('resolution') not in {'', None}:
            resolutions[key].append(float(row['resolution']))
        groups[key][(row['engine'], row['phase'])][str(row['run'])].append(float(row['value']))
    result = []
    for key, populations in sorted(groups.items()):
        left, right = populations[(candidate, 'measured')], populations[(reference, 'measured')]
        paired = sorted(set(left) & set(right))
        report = dict(zip(GROUP, key), candidate=candidate, reference=reference, paired_runs=len(paired))
        if len(paired) < minimum or set(left) != set(right):
            result.append(dict(report, verdict='insufficient-pairs'))
            continue
        delta = [statistics.median(right[run]) - statistics.median(left[run]) for run in paired]
        shifts = []
        for engine in [candidate, reference]:
            batches = []
            for phase in ['noise-a', 'noise-b']:
                values = [statistics.median(values) for values in populations[(engine, phase)].values()]
                batches.append(statistics.median(values) if values else None)
            if None not in batches:
                shifts.append(abs(batches[0] - batches[1]))
        if len(shifts) != 2:
            result.append(dict(report, verdict='missing-metric-pilot'))
            continue
        generator = random.Random(0)
        boot = sorted(statistics.median(generator.choices(delta, k=len(delta))) for _ in range(iterations))
        lo, hi = boot[int(iterations * 0.025)], boot[min(iterations - 1, int(iterations * 0.975))]
        unit = report['unit']
        resolution = 0.001 if unit == 'us' else 0.000001 if unit == 'ms' else 1
        if 'ticks' in report['metric']:
            resolution = 1000 / os.sysconf('SC_CLK_TCK') if unit == 'ms' else 1000000 / os.sysconf('SC_CLK_TCK')
        threshold = max(2 * max(shifts), resolution, *resolutions[key])
        gain = statistics.median(delta)
        verdict = 'win' if lo > threshold else 'loss' if hi < -threshold else 'tie-or-unresolved'
        result.append(dict(report, verdict=verdict, gain=gain, ci95=[lo, hi], noise_threshold=threshold,
                           candidate_p50=statistics.median(statistics.median(v) for v in left.values()),
                           reference_p50=statistics.median(statistics.median(v) for v in right.values())))
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--input', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--candidate', default='flea')
    parser.add_argument('--reference', required=True)
    args = parser.parse_args()
    with args.input.open() as stream:
        result = compare(list(csv.DictReader(stream, delimiter='\t')), args.candidate, args.reference)
    with args.output.open('x') as out:
        json.dump(result, out, indent=2)
        out.write('\n')

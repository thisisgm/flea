"""Paired run-clustered comparisons, with metric-specific pilot noise."""
import argparse
import csv
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from fair.eligibility import GROUP, compare


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

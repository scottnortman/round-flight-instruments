"""Red/green zero-rate check for a capture independently confirmed stationary."""
import argparse
import json
import statistics

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('capture')
args = parser.parse_args()
with open(args.capture, encoding='utf-8') as stream:
    samples = json.load(stream)['samples']
if len(samples) < 20:
    raise SystemExit('Insufficient stationary observations')
spans = [max(float(s[f'a{a}_g']) for s in samples)-min(float(s[f'a{a}_g']) for s in samples) for a in 'xyz']
if max(spans) > 0.02:
    raise SystemExit('Capture not sufficiently stable for this zero-rate check')
means = [statistics.mean(float(s[f'g{a}_dps']) for s in samples) for a in 'xyz']
passed = all(abs(v) <= 10 for v in means)
print(f'{"PASS" if passed else "FAIL"}: stationary gyro mean dps={means}; limit=+/-10 dps; no bias correction')
raise SystemExit(0 if passed else 1)

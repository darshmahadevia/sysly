#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import subprocess
import time

from PIL import Image, ImageChops

parser = argparse.ArgumentParser()
parser.add_argument('pid', type=int)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--expect', choices=['continuous', 'stepped', 'frozen'], required=True)
parser.add_argument('--sample-seconds', type=float, default=2.0)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
args.output.mkdir(parents=True, exist_ok=True)
frames = []
started = time.monotonic()
for index in range(20):
    path = args.output / f'{index:02}.png'
    subprocess.run([str(root / 'target/sysly-ui'), str(args.pid), 'screenshot', str(path)], check=True)
    with Image.open(path) as source:
        width, height = source.size
        box = tuple(round(value) for value in (
            width * 262 / 1320, height * 200 / 901,
            width * 731 / 1320, height * 590 / 901,
        ))
        image = source.convert('RGB').crop(box)
        mask = Image.new('1', image.size)
        mask.putdata([g > 90 and b > 100 and g > r * 1.25 and b > r * 1.25
                      for r, g, b in image.get_flattened_data()])
        frames.append((time.monotonic() - started, mask))
    time.sleep(0.06)
changes = [ImageChops.difference(a, b).getbbox() is not None
           for (_, a), (_, b) in zip(frames, frames[1:])]
assert any(frame.getbbox() for _, frame in frames), 'Processor trace was not visible in the captured graph region'
changed = sum(changes)
inter_sample_changes = [change for (a, _), (b, _), change in zip(frames, frames[1:], changes)
                        if b - a < args.sample_seconds]
report = {'expectation': args.expect, 'changed_pairs': changed, 'total_pairs': len(changes),
          'inter_sample_pairs': len(inter_sample_changes),
          'inter_sample_changed': sum(inter_sample_changes),
          'frames': [{'time': round(t, 3), 'trace_changed': change}
                     for (t, _), change in zip(frames[1:], changes)]}
(args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
if args.expect == 'continuous':
    assert len(inter_sample_changes) >= 10, 'Too few captures were fast enough to check movement between samples'
    assert sum(inter_sample_changes) >= len(inter_sample_changes) * .8, 'Trace still holds between samples'
elif args.expect == 'stepped':
    assert 0 < changed < len(changes) * .5, 'Expected a sample-driven hold-and-jump baseline'
else:
    assert changed == 0, 'Paused trace moved'

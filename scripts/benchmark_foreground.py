#!/usr/bin/env python3
"""Record absolute resource use in five fresh native foreground sessions."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--runs', type=int, default=5)
args = parser.parse_args()
assert args.runs >= 5
root = Path(__file__).resolve().parents[1]
binary = root / 'target/release/sysly'
ui = root / 'target/sysly-ui'
args.output.mkdir(parents=True, exist_ok=False)
report = {'method': 'Five fresh visible Overview sessions with isolated default settings at 1320 by 900 points. 20-second warmup including measure.py settling, then five 5-second windows. Native CPU time checked against ps, visibility and foreground checked throughout. Absolute resource use, no speedup comparison.', 'host': platform.platform(), 'hardware': subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip(), 'memory_bytes': int(subprocess.check_output(['sysctl', '-n', 'hw.memsize'], text=True)), 'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'trials': []}
caffeinate = subprocess.Popen(['/usr/bin/caffeinate', '-di', '-w', str(os.getpid())])
try:
    for trial in range(args.runs):
        directory = args.output / f'trial-{trial + 1}'
        directory.mkdir()
        with (directory / 'app.log').open('w') as log:
            child = subprocess.Popen([str(binary)], env={**os.environ, 'SYSLY_CONFIG_DIR': str((directory / 'config').resolve())}, stdout=log, stderr=log)
            try:
                time.sleep(10)
                assert child.poll() is None
                assert subprocess.check_output([str(ui), str(child.pid), 'visible'], text=True).strip() == 'true'
                subprocess.run([str(ui), str(child.pid), 'key', '1', 'cmd'], check=True, stdout=subprocess.DEVNULL)
                subprocess.run([str(ui), str(child.pid), 'resize', '1320', '900'], check=True, stdout=subprocess.DEVNULL)
                time.sleep(5)
                bounds = subprocess.check_output([str(ui), str(child.pid), 'bounds'], text=True).strip()
                if trial == 0:
                    subprocess.run([str(ui), str(child.pid), 'screenshot', str(directory / 'overview.png')], check=True)
                load = subprocess.check_output(['uptime'], text=True).strip()
                subprocess.run([sys.executable, str(root / 'scripts/measure.py'), str(child.pid), '--mode', 'visible', '--verify-ui', '--seconds', '5', '--output', str(directory / 'measurement.json')], check=True, stdout=subprocess.DEVNULL)
                measured = json.loads((directory / 'measurement.json').read_text())
                report['trials'].append({'trial': trial + 1, 'load': load, 'window_bounds': bounds, **measured})
                (args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
                print(f"trial {trial + 1}: CPU {measured['cpu_time_weighted_percent_one_core']:.3f}% of one core", flush=True)
            finally:
                if child.poll() is None:
                    subprocess.run([str(ui), str(child.pid), 'key', 'q', 'cmd'], stdout=subprocess.DEVNULL, check=True)
                    try:
                        child.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        child.terminate()
                        child.wait(timeout=10)
                assert child.returncode == 0, child.returncode
finally:
    caffeinate.terminate()
report['summary'] = {}
for key, values in {
    'cpu_percent_one_core': [row['cpu_time_weighted_percent_one_core'] for row in report['trials']],
    'rss_bytes': [statistics.median(window['rss_bytes'] for window in row['runs']) for row in report['trials']],
    'physical_footprint_bytes': [statistics.median(window['physical_footprint_bytes'] for window in row['runs']) for row in report['trials']],
}.items():
    report['summary'][key] = {'median': statistics.median(values), 'range': [min(values), max(values)]}
report['successful_sessions'] = len(report['trials'])
report['rejected_windows'] = sum(row['rejected_windows'] for row in report['trials'])
(args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report['summary'], indent=2))

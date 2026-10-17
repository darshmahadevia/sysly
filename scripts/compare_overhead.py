#!/usr/bin/env python3
"""Compare two release binaries in alternating native GUI sessions on macOS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import time

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("baseline", type=Path)
parser.add_argument("candidate", type=Path)
parser.add_argument("--runs", type=int, default=5)
parser.add_argument("--warmup", type=int, default=15)
parser.add_argument("--seconds", type=int, default=5, help="Seconds per observation window, five windows per session")
parser.add_argument("--output", type=Path, default=root / ".audit/optimization/comparison")
args = parser.parse_args()
if args.runs < 1 or args.warmup < 10 or args.seconds < 5:
    parser.error("Use at least one run, ten warmup seconds, and five seconds per window")
args.output.mkdir(parents=True, exist_ok=True)
ui = root / "target/sysly-ui"
subprocess.Popen(["/usr/bin/caffeinate", "-di", "-w", str(os.getpid())], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
records = []
binaries = {"baseline": args.baseline.resolve(), "candidate": args.candidate.resolve()}
report = {
    "method": f"Alternating fresh visible Overview sessions. Default settings, 1320x900 window, {args.warmup + 5} second warmup followed by five {args.seconds} second observation windows. CPU is percent of one core. No other Sysly session should run. No build or profiler should run during observation.",
    "comparison_repeated_five_times": args.runs >= 5,
    "binaries": {name: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for name, path in binaries.items()},
    "host": subprocess.check_output(["uname", "-a"], text=True).strip(),
    "cores": int(subprocess.check_output(["sysctl", "-n", "hw.ncpu"], text=True)),
    "runs": records,
}

def drive(pid, *command):
    return subprocess.check_output([str(ui), str(pid), *map(str, command)], text=True).strip()

for run in range(args.runs):
    for name, binary in binaries.items():
        directory = args.output / f"{run + 1}-{name}"
        directory.mkdir(exist_ok=True)
        config = directory / "config"
        if config.exists():
            raise RuntimeError(f"Use a fresh output directory. Settings already exist at {config}")
        with (directory / "app.log").open("w") as log:
            process = subprocess.Popen([str(binary)], env={**os.environ, "SYSLY_CONFIG_DIR": str(config.resolve())}, stdout=log, stderr=log)
            try:
                time.sleep(10)
                assert process.poll() is None, "Sysly exited during warmup"
                assert drive(process.pid, "visible") == "true", "Overview window is not visible"
                drive(process.pid, "key", "1", "cmd")
                time.sleep(args.warmup - 10)
                if run == 0:
                    drive(process.pid, "screenshot", directory / "overview.png")
                load = subprocess.check_output(["uptime"], text=True).strip()
                subprocess.run([sys.executable, str(root / "scripts/measure.py"), str(process.pid), "--mode", "visible", "--verify-ui", "--seconds", str(args.seconds), "--output", str(directory / "measurement.json")], check=True, stdout=subprocess.DEVNULL)
                observation = json.loads((directory / "measurement.json").read_text())
                records.append({"binary": name, "run": run + 1, "load": load, **observation})
                print(f"{name} run {run + 1}: CPU {observation['cpu_median_percent_one_core']:.3f}%", flush=True)
            finally:
                if process.poll() is None:
                    try:
                        drive(process.pid, "key", "q", "cmd")
                        process.wait(timeout=10)
                    finally:
                        if process.poll() is None:
                            process.terminate()
                            process.wait(timeout=10)
            assert process.returncode == 0, f"Sysly exited with {process.returncode}"
        (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")

report["summary"] = {}
for name in binaries:
    sessions = [r for r in records if r["binary"] == name]
    summary = {}
    for key, values in {
        "cpu_percent_one_core": [r["cpu_time_weighted_percent_one_core"] for r in sessions],
        "rss_bytes": [statistics.median(w["rss_bytes"] for w in r["runs"]) for r in sessions],
        "physical_footprint_bytes": [statistics.median(w["physical_footprint_bytes"] for w in r["runs"]) for r in sessions],
    }.items():
        summary[key] = {"median": statistics.median(values), "range": [min(values), max(values)]}
    report["summary"][name] = summary
(args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report["summary"], indent=2))

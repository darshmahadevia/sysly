#!/usr/bin/env python3
import argparse
import ctypes
import json
import pathlib
import statistics
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument("pid", type=int)
parser.add_argument("--mode", choices=["visible", "hidden", "paused"], required=True)
parser.add_argument("--windows", type=int, default=5)
parser.add_argument("--seconds", type=int, default=10)
args = parser.parse_args()
if args.windows < 5 or args.seconds < 5:
    parser.error("Use at least five windows of at least five seconds")


class Usage(ctypes.Structure):
    _fields_ = [("uuid", ctypes.c_uint8 * 16)] + [(name, ctypes.c_uint64) for name in [
        "user", "system", "package_wakeups", "interrupt_wakeups", "pageins", "wired",
        "resident", "footprint", "started", "exited", "child_user", "child_system",
        "child_package_wakeups", "child_interrupt_wakeups", "child_pageins",
        "child_elapsed", "disk_read", "disk_written",
    ]]


libproc = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
libproc.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]


class Timebase(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


timebase = Timebase()
libsystem = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
if libsystem.mach_timebase_info(ctypes.byref(timebase)) != 0:
    raise RuntimeError("Cannot convert Mach CPU ticks")


def observe():
    output = subprocess.check_output(["ps", "-p", str(args.pid), "-o", "time=,rss=,comm="], text=True).strip()
    cpu, rss, command = output.split(maxsplit=2)
    assert pathlib.Path(command).name in ("sysly", "Sysly"), "PID does not belong to Sysly"
    usage = Usage()
    if libproc.proc_pid_rusage(args.pid, 2, ctypes.byref(usage)) != 0:
        raise OSError(ctypes.get_errno(), "Cannot read process resource counters")
    cpu_seconds = (usage.user + usage.system) * timebase.numer / timebase.denom / 1e9
    ps_seconds = sum(float(value) * 60 ** index for index, value in enumerate(reversed(cpu.split(":"))))
    assert abs(cpu_seconds - ps_seconds) < 0.1, "native CPU counter disagrees with ps"
    return time.monotonic(), cpu_seconds, int(rss) * 1024, usage.footprint


time.sleep(5)
runs = []
for _ in range(args.windows):
    before = observe()
    time.sleep(args.seconds)
    after = observe()
    runs.append({"seconds": after[0] - before[0], "cpu_percent_one_core": (after[1] - before[1]) / (after[0] - before[0]) * 100, "rss_bytes": after[2], "physical_footprint_bytes": after[3]})
report = {
    "mode": args.mode,
    "pid": args.pid,
    "runs": runs,
    "cpu_median_percent_one_core": statistics.median(r["cpu_percent_one_core"] for r in runs),
    "cpu_range_percent_one_core": [min(r["cpu_percent_one_core"] for r in runs), max(r["cpu_percent_one_core"] for r in runs)],
    "rss_range_bytes": [min(r["rss_bytes"] for r in runs), max(r["rss_bytes"] for r in runs)],
    "physical_footprint_range_bytes": [min(r["physical_footprint_bytes"] for r in runs), max(r["physical_footprint_bytes"] for r in runs)],
    "interpretation": "Absolute steady-state process observation. No speedup comparison. Includes collector and UI. RSS includes shared resident pages and is not physical footprint. Mode must be established through the real UI before running.",
}
path = pathlib.Path(__file__).resolve().parents[1] / ".audit" / f"overhead-{args.mode}.json"
path.parent.mkdir(exist_ok=True)
path.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))

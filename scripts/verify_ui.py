#!/usr/bin/env python3
import json
import pathlib
import re
import signal
import subprocess
import sys
import time

root = pathlib.Path(__file__).resolve().parents[1]
pid = int(sys.argv[1])
helper = root / "target/sysly-ui"
subprocess.run(["swiftc", str(root / "scripts/ui.swift"), "-o", str(helper)], check=True)
evidence = root / ".audit"
evidence.mkdir(exist_ok=True)
events = []


def drive(operation, *args):
    return subprocess.check_output([str(helper), str(pid), operation, *map(str, args)], text=True)


def wait_for(predicate, label, timeout=8):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        state = drive("dump")
        if predicate(state):
            events.append(label)
            return state
        time.sleep(0.15)
    raise AssertionError(f"Timed out: {label}")


child = subprocess.Popen(["/bin/sleep", "120"])
try:
    wait_for(lambda s: "AXWindow Sysly" in s, "native window ready")
    drive("press", "Apps & processes")
    wait_for(lambda s: "AXTextField" in s, "process search reachable")
    drive("search", str(child.pid))
    wait_for(lambda s: "AXButton sleep" in s and f"AXTextField {child.pid}" in s, "PID search finds owned child")
    current = drive("dump")
    memory_header = re.search(r"AXButton (Memory[^\n]*)", current).group(1)
    drive("press", memory_header)
    wait_for(lambda s: "AXButton Memory" in s and ("Memory ↓" in s or "Memory ↑" in s), "memory sorting changes header")
    drive("press", "Tree")
    wait_for(lambda s: "AXButton sleep" in s, "tree preserves searched process")
    drive("press", "sleep")
    wait_for(lambda s: "AXButton Quit process" in s, "process inspector reachable")
    drive("screenshot", root / ".impeccable/review/inspector.png")
    drive("press", "Pause process")
    wait_for(lambda s: "Pause process sleep?" in s, "stop asks for confirmation")
    assert child.poll() is None, "unconfirmed signal changed process"
    drive("confirm", "Pause process")

    def stopped():
        return "T" in subprocess.check_output(["ps", "-p", str(child.pid), "-o", "state="], text=True)

    deadline = time.monotonic() + 2
    while not stopped() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert stopped(), "stop was not delivered within two seconds"
    events.append("confirmed STOP changes owned child OS state")
    drive("press", "Resume process")
    wait_for(lambda s: "Resume process sleep?" in s, "continue asks for confirmation")
    drive("confirm", "Resume process")
    deadline = time.monotonic() + 2
    while stopped() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert not stopped(), "continue did not resume process"
    events.append("confirmed CONT resumes owned child")
    drive("press", "Quit process")
    wait_for(lambda s: "Quit process sleep?" in s, "termination asks for confirmation")
    drive("confirm", "Quit process")
    assert child.wait(timeout=2) < 0, "owned child did not exit from signal"
    events.append("confirmed TERM exits owned child")
    drive("press", "Pause")
    wait_for(lambda s: "AXStaticText Paused" in s, "pause visible")
    drive("close")
    wait_for(lambda s: "AXWindow Sysly" not in s, "close hides desktop window")
    subprocess.run(["kill", "-0", str(pid)], check=True)
    drive("press", "Open Sysly")
    wait_for(lambda s: "AXWindow Sysly" in s, "menu bar reopens desktop window")
    drive("press", "Resume")
    wait_for(lambda s: "AXStaticText Live" in s, "sampling resumes")
    report = {"result": "VERIFIED", "events": events}
    (evidence / "ui-verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
finally:
    if child.poll() is None:
        child.send_signal(signal.SIGCONT)
        child.terminate()
        child.wait(timeout=5)

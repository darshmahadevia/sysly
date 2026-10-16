#!/usr/bin/env python3
import argparse
import json
import pathlib
import signal
import subprocess
import time


root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("pid", type=int)
parser.add_argument("--output", type=pathlib.Path, default=root / "artifacts/redesign/native-check")
parser.add_argument("--resume-after-reopen", action="store_true")
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
ui = root / "target/sysly-ui"
ocr = root / "target/sysly-ocr"
for source, binary in [("ui.swift", ui), ("ocr.swift", ocr)]:
    subprocess.run(["swiftc", str(root / "scripts" / source), "-o", str(binary)], check=True)
class EventLog(list):
    def append(self, value):
        super().append(value)
        print(value, flush=True)


events = EventLog()
if args.resume_after_reopen:
    previous = json.loads((args.output / "report.json").read_text())
    assert previous["events"][-1] == "menu bar reopens the paused dashboard"
    events.extend(previous["events"])
last = None
capture_number = 0


def drive(operation, *values):
    return subprocess.check_output([str(ui), str(args.pid), operation, *map(str, values)], text=True)


def capture():
    global last, capture_number
    capture_number += 1
    path = args.output / "current.png"
    drive("screenshot", path)
    last = json.loads(subprocess.check_output([str(ocr), str(path)], text=True, timeout=90))
    (args.output / "current.json").write_text(json.dumps(last, indent=2) + "\n")
    return "\n".join(line["text"] for line in last["lines"])


def wait_for(text, label, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        content = capture()
        if text.casefold() in content.casefold():
            events.append(label)
            return content
        time.sleep(0.25)
    raise AssertionError(f"Timed out waiting for {label}. Visible text follows.\n{content}")


def click(text, *, below=0, right_of=0, top=1):
    capture()
    def matches_text(value):
        if value.casefold() == text.casefold():
            return True
        return text in {"Pause", "Resume"} and value.startswith(text) and len(value[len(text):].strip()) <= 2

    matches = [line for line in last["lines"] if matches_text(line["text"])
               and below < line["y"] < top and line["x"] > right_of]
    assert matches, f"No visible control named {text!r}"
    line = matches[-1]
    bounds = json.loads(drive("bounds"))
    drive("point", line["x"] * bounds["Width"], line["y"] * bounds["Height"])
    time.sleep(0.15)


def search(query):
    drive("key", "f", "cmd")
    drive("key", "a", "cmd")
    drive("key", "delete")
    drive("type", query)


def stopped(pid):
    return "T" in subprocess.check_output(["ps", "-p", str(pid), "-o", "state="], text=True)


def wait_state(predicate, label):
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if predicate():
            events.append(label)
            return
        time.sleep(0.05)
    raise AssertionError(label)


child = None if args.resume_after_reopen else subprocess.Popen(["/bin/sleep", "600"])
try:
    if not args.resume_after_reopen:
        drive("key", "1", "cmd")
        wait_for("View details", "overview renders live resource sections")
        drive("key", "2", "cmd")
        wait_for("Disk I/O", "process table opens by keyboard")
        search("sysly-no-such-process-493817")
        wait_for("No processes match", "search displays an empty result")
        search(str(child.pid))
        wait_for("sleep", "PID search finds the verification child")
        click("Memory", right_of=0.4, top=0.4)
        click("Tree view")
        wait_for("sleep", "tree search retains the child")
        click("sleep", right_of=0.2)
        wait_for("Quit process", "process inspector opens")
        for action, confirm, predicate, label in [
            ("Pause process", "Pause", lambda: stopped(child.pid), "confirmed STOP changes the child OS state"),
            ("Resume process", "Resume", lambda: not stopped(child.pid), "confirmed CONT resumes the child"),
        ]:
            click(action, right_of=0.5)
            wait_for(f"{confirm} sleep?", f"{confirm} asks for confirmation")
            assert child.poll() is None, "Unconfirmed action exited the child"
            if confirm == "Pause":
                assert not stopped(child.pid), "STOP was sent before confirmation"
            else:
                assert stopped(child.pid), "CONT was sent before confirmation"
            click(confirm, below=0.2, right_of=0.3)
            wait_state(predicate, label)
        click("Quit process", right_of=0.5)
        wait_for("Quit sleep?", "TERM asks for confirmation")
        assert child.poll() is None, "TERM was sent before confirmation"
        click("Quit", below=0.2, right_of=0.3)
        assert child.wait(timeout=3) < 0, "Child did not exit from TERM"
        events.append("confirmed TERM exits the owned child")
        drive("key", "escape")
        drive("key", "1", "cmd")
        click("Pause", right_of=0.5, top=0.2)
        wait_for("Resume", "sampling pauses")
        drive("close")
        subprocess.run(["kill", "-0", str(args.pid)], check=True)
        wait_state(lambda: drive("visible").strip() == "false", "close hides the actual window")
        events.append("closing the window keeps the menu bar process alive")
        drive("press", "Open Sysly")
        wait_state(lambda: drive("visible").strip() == "true", "menu bar makes the window visible")
        wait_for("Resume", "menu bar reopens the paused dashboard")
    click("Resume", right_of=0.5, top=0.2)
    wait_for("Pause", "sampling resumes")
    drive("key", ",", "cmd")
    wait_for("Make it yours", "appearance opens by keyboard")
    wait_for("Layout & text", "customization tabs are visible")
    drive("key", "1", "cmd")
    report = {"result": "VERIFIED", "events": events}
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
except BaseException as error:
    report = {"result": "FAILED", "events": events, "error": str(error)}
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    raise
finally:
    if child is not None and child.poll() is None:
        child.send_signal(signal.SIGCONT)
        child.terminate()
        child.wait(timeout=5)

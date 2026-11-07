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
        return False

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


def wait_sheet(title, label, timeout=6):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        tree = drive("dump")
        if title in tree:
            events.append(label)
            return
        time.sleep(0.15)
    raise AssertionError(f"Native sheet did not show {title!r}\n{tree}")


child = None if args.resume_after_reopen else subprocess.Popen(["/bin/sleep", "600"])
try:
    if not args.resume_after_reopen:
        drive("key", "1", "cmd")
        wait_for("Details", "overview renders live resource sections")
        bounds = json.loads(drive("bounds"))
        drive("hover", bounds["Width"] * 0.7, 260)
        wait_for("ago", "hovering a chart scrubs the recorder to that moment")
        drive("key", "3", "cmd")
        wait_for("Each core", "processor page opens by keyboard")
        drive("key", "2", "cmd")
        wait_for("Disk", "process table opens by keyboard")
        search("sysly-no-such-process-493817")
        wait_for("Nothing matches", "search displays an empty result")
        search(str(child.pid))
        wait_for("sleep", "PID search finds the verification child")
        # Vision OCR misreads the sparse numeric columns, so these two controls are
        # driven by layout position. Results are still checked by reading the screen.
        drive("point", bounds["Width"] - 134, 117)
        drive("point", 664, 80)
        wait_for("sleep", "tree search retains the child")
        click("sleep", right_of=0.15)
        wait_for("Command", "process inspector opens beside the table")
        for action, confirm, predicate, label in [
            ("Pause", "Pause", lambda: stopped(child.pid), "confirmed STOP changes the child OS state"),
            ("Resume", "Resume", lambda: not stopped(child.pid), "confirmed CONT resumes the child"),
        ]:
            drive("point", bounds["Width"] - 243, bounds["Height"] - 30)
            wait_sheet(f"{confirm} sleep?", f"{confirm} asks for confirmation in a native sheet")
            assert child.poll() is None, "Unconfirmed action exited the child"
            if confirm == "Pause":
                assert not stopped(child.pid), "STOP was sent before confirmation"
            else:
                assert stopped(child.pid), "CONT was sent before confirmation"
            drive("confirm", confirm)
            wait_state(predicate, label)
            time.sleep(0.5)
        drive("point", bounds["Width"] - 243, bounds["Height"] - 64)
        wait_sheet("Quit sleep?", "TERM asks for confirmation in a native sheet")
        assert child.poll() is None, "TERM was sent before confirmation"
        drive("confirm", "Quit")
        assert child.wait(timeout=3) < 0, "Child did not exit from TERM"
        events.append("confirmed TERM exits the owned child")
        drive("key", "escape")
        drive("key", "1", "cmd")
        drive("key", "p", "cmd")
        wait_for("Paused", "sampling pauses")
        drive("close")
        subprocess.run(["kill", "-0", str(args.pid)], check=True)
        wait_state(lambda: drive("visible").strip() == "false", "close hides the actual window")
        events.append("closing the window keeps the menu bar process alive")
        drive("press", "Open Sysly")
        wait_state(lambda: drive("visible").strip() == "true", "menu bar makes the window visible")
        wait_for("Paused", "menu bar reopens the paused dashboard")
    drive("key", "p", "cmd")
    wait_for("Live", "sampling resumes")
    drive("key", ",", "cmd")
    wait_for("Sampling", "settings open by keyboard")
    wait_for("Appearance", "settings tabs are visible")
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

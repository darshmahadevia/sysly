#!/usr/bin/env python3
import json
import pathlib
import re
import subprocess
import sys
import time

root = pathlib.Path(__file__).resolve().parents[1]
pid = int(sys.argv[1])
helper = root / 'target/sysly-ui'
output = root / 'artifacts/tray'
output.mkdir(parents=True, exist_ok=True)
events = []


def drive(command, *args):
    return subprocess.check_output([str(helper), str(pid), command, *map(str, args)], text=True, timeout=15)


def wait_for(predicate, label, timeout=18):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = drive('menu-dump')
        if predicate(value):
            events.append(label)
            return value
        time.sleep(0.25)
    raise AssertionError(f'{label}\n{value}')


def timestamp(value):
    match = re.search(r'updated\s+(\d\d:\d\d:\d\d)', value)
    return match.group(1) if match else None


def open_menu():
    drive('tray')
    return wait_for(lambda s: all(label in s for label in ['CPU', 'MEM', 'GPU', 'NET', 'Open Sysly', 'Quit Sysly']), 'logo opens four-metric dropdown')


state = open_menu()
assert 'Pause sampling' in state, state
assert re.search(r'AXMenuBarItem\s+\| Sysly system monitor', state), 'Menu bar must contain only the logo'
(output / 'menu-live.txt').write_text(state)
drive('menu-screenshot', output / 'menu-live.png')
first = timestamp(state)
assert first, f'No sample timestamp found\n{state}'
wait_for(lambda s: timestamp(s) is not None and timestamp(s) != first, 'readings update while dropdown remains open')
drive('menu-press', 'Pause sampling')
time.sleep(0.5)
state = open_menu()
assert 'Resume sampling' in state and 'Paused' in state, state
(output / 'menu-paused.txt').write_text(state)
drive('menu-screenshot', output / 'menu-paused.png')
frozen = state
time.sleep(3)
assert drive('menu-dump') == frozen, 'Paused readings changed'
events.append('pause freezes readings and offers resume')
drive('menu-press', 'Resume sampling')
time.sleep(0.5)
state = open_menu()
assert 'Pause sampling' in state and 'Live' in state, state
events.append('resume restores live state')
drive('key', 'escape')
time.sleep(0.3)
assert drive('menu-visible').strip() == 'false', 'Escape left dropdown open'
events.append('Escape dismisses dropdown')
drive('close')
time.sleep(0.5)
assert drive('visible').strip() == 'false', 'Dashboard did not hide'
subprocess.run(['kill', '-0', str(pid)], check=True)
open_menu()
drive('menu-press', 'Open Sysly')
deadline = time.monotonic() + 5
while drive('visible').strip() != 'true' and time.monotonic() < deadline:
    time.sleep(0.2)
assert drive('visible').strip() == 'true', 'Open Sysly did not restore dashboard'
events.append('logo remains available after closing dashboard and Open Sysly restores it')
report = {'result': 'VERIFIED', 'pid': pid, 'events': events}
(output / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))

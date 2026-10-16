---
name: verify-sysly
description: Verify Sysly's native macOS dashboard, process controls, layout editor, themes, persistence, and sampling after changes.
---

# Verify Sysly

## Launch

Run `scripts/bundle.sh` from the project root. Start `dist/Sysly.app/Contents/MacOS/Sysly` with `SYSLY_CONFIG_DIR` pointing at a new directory under `.audit/`. Retain the PID and own only that instance. Two instances have distinct PIDs but duplicate menu bar entries; drive one PID explicitly.

## Doctor

Compile `swiftc scripts/ui.swift -o target/sysly-ui`. Run `target/sysly-ui PID doctor`, then `target/sysly-ui PID dump`. A usable instance has the Sysly window and live sampling. The first Accessibility query can activate the tree asynchronously; query again before diagnosing missing controls. Missing macOS Accessibility, input, or screen permissions is a failed prerequisite, not a pass.

## Drive

Read [the feature map](features/README.md) for the affected user path. Use accessible control names from a fresh dump. `press` operates native accessible actions; `click`, `drag`, and `drag-resize` derive pointer coordinates from the named accessible element. Never use internal setters as interaction evidence.

Run `python3 scripts/verify_ui.py PID` for process controls and lifecycle. It owns the disposable process it signals. Run `python3 scripts/verify_metrics.py` against the release binary for independent OS-counter and workload checks.

## Evidence

Keep action/result logs under `.audit/` and native screenshots under `.impeccable/review/`. Verify layout order, sizes, and appearance both in the window and in the saved preferences, then quit and relaunch the same isolated configuration. Theme import must pass through its native file picker. Invalid imports must leave the active theme unchanged.

## Cleanup

Use the owned instance's **Quit Sysly** menu action and verify its PID exits. Signal only disposable children created by the verification script. Keep screenshots, logs, and the isolated configuration as evidence. Never kill by executable name or delete the user's real settings.

## Helpers

`target/sysly-ui PID screenshot PATH` captures the real native window. `resize WIDTH HEIGHT` resizes the window. `drag SOURCE_LABEL TARGET_LABEL` moves a section. `drag-resize RESIZE_LABEL DELTA_Y` drags a resize handle.

`python3 scripts/measure.py PID --mode visible` observes five windows of CPU, RSS, and physical footprint. Establish visible, hidden, or paused mode through the UI first. CPU time is converted from Mach ticks and cross-checked against `ps`. Do not report obsolete measurements after changing the app.

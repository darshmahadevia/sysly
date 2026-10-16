# Dashboard and process controls

## Sub-features

Current readings, technical details, search, sorting, process tree, inspector, and confirmed signals.

## How to get to it (user POV)

Open the dashboard. Expand a section's Details. Open Apps & processes or press Command-F to find a process.

## Driving it with native Accessibility

Run `python3 scripts/verify_ui.py PID`. Its disposable sleep process must appear in search, open an inspector, stop, continue, and exit only after the respective confirmation. Compare process state with the OS, not just an in-app notice.

## Gotchas

CPU for a process is per core and can exceed 100%. OS permissions can prevent actions on other users' processes. Unknown sensors remain unavailable.

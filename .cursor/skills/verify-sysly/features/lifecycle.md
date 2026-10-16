# Menu bar and sampling

## Sub-features

Live/paused state, refresh interval, retained history, menu bar choices, close-to-menu-bar, reopen, and quit.

## How to get to it (user POV)

Use Pause/Resume in the dashboard or menu bar. Close the window and reopen it from Open Sysly in the menu bar.

## Driving it with native Accessibility

Record the visible sample number, pause, wait longer than the sampling interval, and verify it stays fixed. Close the native window, verify the owned PID remains alive, then use Open Sysly and verify the window returns. Change summary choices and verify the actual menu bar title. Quit Sysly and verify PID exit.

## Gotchas

An active hardware sampling wait cannot be interrupted, but paused snapshots are discarded before publication. Hidden sampling slows to at least five seconds. Process signals have a separate worker and must not wait for that interval.

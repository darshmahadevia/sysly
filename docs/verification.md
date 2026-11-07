# Verification

## Recorder redesign, 2026-11-06

Verified on an Apple M4 Mac with 16 GiB RAM and macOS 27.2. Sections below this one describe the earlier interface.

`cargo test --locked` passes all 51 tests, and `cargo clippy --locked --all-targets -- -D warnings` is clean. New tests cover verdict rules, app grouping, settings migration, chart scaling, scrubbing, and interface filtering.

`scripts/verify_gpui.py` passes against the real window. It checks scrubbing, keyboard navigation, search, tree view, the inspector, and native sheets for STOP, CONT, and TERM, comparing the outcome with the disposable child's actual OS state. It also covers pause, close to the menu bar, and reopening. `scripts/verify_tray.py` passes for the menu bar panel. `scripts/verify_metrics.py` passes for the collectors.

Other checks were run by hand against the native window. A real 10-process CPU load changed the verdict to "Your Mac is working hard" and named the process. Real pointer drags reordered and resized sections, and the saved file and a relaunch confirmed both. Paper, 18-point text at the minimum window size, half-width sections, Tab focus, and Space activation were reviewed from captures.

Not verified: a Mac without a battery, Intel Macs (unsupported), Reduce Motion enabled, thermal and memory-pressure verdicts on real hardware events (covered by unit tests only), the native font and file pickers, and very long sessions.

Vision OCR misreads sparse numeric columns, so a few table controls in `verify_gpui.py` are driven by layout position.

## Earlier interface

The GPUI redesign was verified locally on 2026-10-16 on an Apple M4 Mac with 10 logical cores, 16 GiB RAM, and macOS 27.0.1.

## Build and behavior

`cargo build`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings` pass. All 16 Rust tests pass. They cover settings storage and import/export, normalization and recovery, named layouts, process identity and hierarchy, sampling and signal scheduling, chart gaps, and layout reordering.

The installed optimized release passed the native workflow in `scripts/verify_gpui.py`. Keyboard navigation opened Overview, Processes, and Appearance. Search produced an empty state and found a disposable child by PID. Sorting, tree mode, and the process inspector remained usable. STOP, CONT, and TERM required confirmation; the harness checked the child's actual OS state and exit status. It never confirmed a signal for an unexpected process name.

Pause, close-to-menu-bar, reopening, and resume passed against the real window. Window visibility is checked through Core Graphics. A hidden AppKit window can remain in the Accessibility tree, so its presence there is not a visibility test.

GPUI 0.2.2 exposes native window and menu accessibility but does not expose the dashboard's full content tree. The native verifier uses Swift input and screen capture with Vision OCR. The helper activates the target process through System Events before sending keyboard input. The verifier records completed checks in `artifacts/redesign/native-check/report.json`.

## Measurements

`scripts/verify_metrics.py` passed against the new release executable. Six real snapshots matched physical RAM and logical core counts from `sysctl`, detected a CPU workload, and measured a known 16 MiB loopback transfer. GPU, CPU temperature, battery, and disk I/O were available on this Mac. No fan readings were returned.

The preserved sampling CLI also produced two advancing real snapshots. Evidence is in `artifacts/redesign/cli-after.jsonl` and `artifacts/redesign/metrics-check.json`. Snapshot files include local process information and are not bundled in the app.

The earlier egui resource observations in `.audit` do not describe this GPUI build. No comparative performance claim is made for the redesign.

## Native appearance

Final captures under `artifacts/redesign` cover the 1320 × 900 desktop and 1000 × 700 minimum window, the process table, Appearance, and the process inspector. `before.png` records the prior interface. The final visual review is recorded beside the captures.

The source retains the version-one preferences schema and existing import/export formats. Rust tests verify their round trips. The redesign's native workflow does not claim to have exercised every font file, imported theme, or hardware variant.

Jimit's final code review independently reran all 16 tests and found stale editor values after applying a layout preset. The callback now refreshes its input entities. A native check started with custom section text, applied Balanced, and confirmed that the editor and saved configuration both reset to the preset. Evidence is in `artifacts/redesign/preset-verification.json` and its before/after captures.

The full visual review requested a scrolling compact inspector and explicit storage-count units. Qurvo's scoped verdict scored both fixes resolved. Reports are `artifacts/redesign/final-review.md` and `final-verdict.md`. Qurvo also checked documentation, tokens, local links, and verification claims in `qurvo-handoff-review.md`.

## Package

`scripts/bundle.sh` builds the optimized native app and ZIP. The app in `~/Applications/Sysly.app` has the same executable SHA-256 as `dist/Sysly.app`. Its local signature passes `codesign --verify --deep --strict`. The ZIP was extracted outside the document-sync workspace and its signature verified there as well.

`artifacts/redesign/previous-installed.app` retains the previous installed bundle. Preferences remain separate in Application Support and were preserved. GUI verification uses an isolated `SYSLY_CONFIG_DIR`.

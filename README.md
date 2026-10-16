# Sysly

A native Rust GPUI system monitor for Apple Silicon Macs. A graphite dashboard shows live resource history, memory composition, and process activity. Resource pages and process inspection provide more detail.

## Run

Build the macOS app with Rust 1.88 or newer and the Xcode command-line tools.

```sh
scripts/bundle.sh
open dist/Sysly.app
```

For development, run `cargo run --locked`. The app uses GPUI 0.2.2 and gpui-component 0.5.1. The `runtime_shaders` feature supports builds with Command Line Tools.

Closing the window keeps the menu bar summary running when the menu bar is available. Use its menu to reopen the dashboard, pause sampling, or quit.

The bundle is ad-hoc signed for local use. Distribution to other Macs requires your own Developer ID signing and notarization.

## Make it yours

Open **Appearance** with Command-Comma. Choose **Layout & text** to drag sections into a new order, change their width, resize charts, and edit dashboard copy. The other tabs control colors, typography, density, history, refresh rate, and menu bar contents. Save named layouts or import and export layouts and themes.

Use Command-1 for Overview, Command-2 for Apps & processes, and Command-F for process search. See the [dashboard guide](docs/dashboard.md) for navigation and customization.

Theme files carry colors, font family, and text size. Layout files carry section order, visibility, width, chart height, labels, and descriptions. A locally selected font file stays local. Import a theme from [themes](themes/) to try the file format.

Settings live in `~/Library/Application Support/Sysly/preferences.json`. Malformed settings fall back to defaults and are backed up before the next save. Set `SYSLY_CONFIG_DIR` to use an isolated configuration directory for verification.

## Read the measurements

- Processor activity is the percentage of the Mac's total CPU capacity in use. Process activity is per core and can exceed 100%.
- Graphics activity is GPU active residency, not a benchmark or a measure of rendering speed.
- Memory use and available memory follow macOS counters. Available memory includes reclaimable memory and does not complement the used value. Memory use alone does not indicate memory pressure.
- Network rates use actual elapsed time between samples. The first sample establishes a baseline. Interface totals are OS counters, not totals since Sysly opened.
- APFS volumes can share physical disk I/O counters. Do not add their throughput together.
- Temperatures, frequencies, fans, and power depend on the Mac and OS APIs. Missing readings remain unavailable. Power readings are estimates from the hardware source.
- Process signals use your account's existing permissions. Sysly rechecks the process identity before signalling it and rejects its own process and PID 0/1.

The app targets Apple Silicon and macOS 14 or later. Hardware APIs come from [macmon](https://github.com/vladkens/macmon), including private Apple interfaces. Compatibility with every chip and macOS version requires separate verification.

## Verify

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 scripts/verify_metrics.py
```

The metric check compares memory and core counts with macOS, measures a real CPU workload, and checks a known loopback transfer. `target/release/sysly --sample 5 1` prints five real snapshots at one-second intervals. Snapshot output includes process commands, so treat it as local diagnostic data.

Run `python3 scripts/verify_gpui.py PID` against an existing instance for native interaction checks. The script compiles the Swift window helper and Vision OCR helper. It requires macOS Accessibility and screen capture permissions. GPUI 0.2.2 does not expose content labels through Accessibility, so the check reads captured window text. Process signals target a disposable child created by the script.

Native screenshots and review evidence live in [artifacts/redesign](artifacts/redesign/). `scripts/verify_ui.py` and `.impeccable/review` captures belong to the previous interface. `scripts/measure.py PID --mode visible` records resource observations after you establish the mode through the real UI.

See [architecture](docs/architecture.md) for ownership, scheduling, and data sources, and [verification](docs/verification.md) for verification results and limits.

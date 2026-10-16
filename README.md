<p align="center">
  <img src="assets/brand/sysly-icon.png" alt="Sysly icon" width="96">
</p>

# Sysly

A native system monitor for Apple Silicon Macs, built with Rust and GPUI.

See what is using your Mac's resources, inspect individual processes, and arrange the dashboard around the readings you care about. Sysly keeps CPU and memory summaries in the menu bar when you close the window.

![Sysly dashboard with memory, graphics, and network activity](artifacts/logo/dashboard.png)

## What you can do

- Track CPU and per-core activity, GPU activity, memory, swap, disk I/O, network traffic, and battery status.
- Read temperatures, frequencies, power estimates, and fan speeds where your hardware exposes them.
- Search processes by name, PID, or user. Sort the list, browse a process tree, and inspect commands and resource use.
- Terminate, force quit, pause, or resume a process after confirmation, within your account's permissions.
- Drag dashboard sections into place, change their width, resize charts, and edit labels and descriptions.
- Choose colors and fonts, save named layouts, and import or export themes and layouts.
- Adjust the history window, refresh rate, display density, and menu bar readings.

Missing hardware readings appear as unavailable.

## Build and run

You need an Apple Silicon Mac with macOS 14 or later, Rust 1.88 or newer, and the Xcode Command Line Tools.

If you have not installed the Command Line Tools, run:

```sh
xcode-select --install
```

Clone the repository and build the app:

```sh
git clone https://github.com/darshmahadevia/sysly.git
cd sysly
scripts/bundle.sh
open dist/Sysly.app
```

The script creates `dist/Sysly.app` and `dist/Sysly.zip`. It signs the app with an ad-hoc signature for local use. Distribution to other Macs requires your own Developer ID signing and notarization.

For development, launch the app directly:

```sh
cargo run --locked
```

Sysly uses GPUI 0.2.2 and gpui-component 0.5.1. GPUI's `runtime_shaders` feature lets the app build with Command Line Tools.

## Customize the dashboard

Press `Command-,` to open Appearance. In **Layout & text**, drag sections into a new order, adjust their size, and edit dashboard copy. The other tabs control colors, fonts, chart history, refresh rate, and the menu bar.

Use `Command-1` for Overview, `Command-2` for Apps & processes, and `Command-F` for process search. The [dashboard guide](docs/dashboard.md) covers navigation and customization.

Try the [included themes](themes/), or export your own. Themes store colors, font family, and text size. Layouts store section order, visibility, widths, chart heights, labels, and descriptions. A selected local font file stays on your Mac.

Settings live at `~/Library/Application Support/Sysly/preferences.json`. If settings are malformed, Sysly uses defaults and backs up the original file before the next save. Set `SYSLY_CONFIG_DIR` to use a different settings directory.

## Understand the readings

CPU activity shows the percentage of the Mac's total CPU capacity in use. Individual process CPU activity is per core and can exceed 100%. GPU activity measures active residency, not rendering speed.

Memory readings follow macOS counters. Available memory includes reclaimable memory, so used and available memory do not add up to total memory. Memory use alone does not indicate memory pressure.

Network rates use the actual time between samples. The first sample establishes a baseline. Interface totals come from macOS and do not start at zero when you open Sysly. APFS volumes can share physical disk I/O counters, so do not add their throughput together.

Hardware readings depend on the Mac and macOS version. Power readings are estimates. Sysly uses [macmon](https://github.com/vladkens/macmon) for Apple Silicon hardware data, including private Apple interfaces. Support for every chip and macOS version needs separate verification.

## Develop and verify

Run the tests and lint checks:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Check the collectors against real macOS activity:

```sh
python3 scripts/verify_metrics.py
```

The metric check compares memory and core counts with macOS, creates a CPU workload, and checks a known loopback transfer.

To print five JSON snapshots at one-second intervals, build the release binary and run:

```sh
cargo build --release --locked
target/release/sysly --sample 5 1
```

Snapshots include process commands. Treat them as local diagnostic data.

For native interaction checks, run `python3 scripts/verify_gpui.py PID` against a running Sysly instance. The script needs macOS Accessibility and screen capture permissions. It compiles Swift helpers and reads captured window text with Vision OCR because GPUI 0.2.2 does not expose content labels through Accessibility. Process signal checks use a disposable child process.

Read the [architecture guide](docs/architecture.md) for code ownership and data sources, or the [verification notes](docs/verification.md) for recorded results and limits.

## License

Sysly uses the [MIT license](LICENSE). The vendored macmon code carries its [own MIT license](vendor/macmon/LICENSE).

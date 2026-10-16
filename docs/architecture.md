# Architecture

Sysly uses Rust GPUI 0.2.2 with gpui-component 0.5.1 and a native AppKit-backed menu bar. GPUI renders the window. There is no webview or server. The collector reads OS APIs directly.

## Interface ownership

`main.rs` handles the sampling CLI before GUI startup. It initializes GPUI, component assets, the dark control theme, key bindings, and the root window.

`app.rs` has one root `App` entity. The root owns the monitor, current `Arc<Snapshot>`, bounded history, preferences, store, tray, selected page, and typed process overlay. Stateless render functions compose cards and charts. Dedicated `InputState` entities own text editing and focus. The process list uses GPUI's virtualized `uniform_list`.

The root polls monitor and tray events every 200 milliseconds. A new snapshot updates history and process ordering. History retains at most 901 samples and also respects the configured time span. Pauses mark a break before the next point. Missing values split traces rather than becoming zero. Navigation has a 180-millisecond opacity transition.

The section vector is the source of order, visibility, width, chart height, and custom copy. Stable section IDs support drag ordering. Named layouts snapshot that vector. Appearance settings update the component theme and schedule a save after 400 milliseconds. The store also saves on root drop.

## Sampling and process actions

`model.rs` defines snapshots without UI or OS handle types. `collector.rs` owns OS handles and cumulative counter baselines. `monitor.rs` owns a sampler worker and a separate process-action worker. The sampler replaces one latest snapshot rather than queuing a history backlog. The action channel has a capacity of eight.

Pausing prevents publication of an in-flight sample and puts the sampler to sleep. A hidden window increases the sampling interval to at least five seconds. Process actions remain independent of hardware sampling waits. `tray.rs` sends menu events to the same UI root.

Process identity includes PID and start time. The signal boundary refreshes the process before sending TERM, KILL, STOP, or CONT. macOS has no atomic PID-plus-start-time signal API. A narrow exit and PID-reuse race remains between the final check and `kill`.

## Data and persistence

Data sources are sysinfo for CPU, memory, processes, networks, and disks, macmon for Apple Silicon hardware metrics, and native IOKit for battery properties. Battery, user, and disk topology refresh less often than activity counters. Rates use monotonic observation time and counter identity. A reset establishes a fresh baseline.

`Snapshot.collected_ms` measures post-hardware OS collection. It excludes macmon processing and its sampling wait. It does not measure total application overhead.

`preferences.rs` retains settings format version 1, normalization, bounded imports, named layouts, and atomic writes. Invalid settings fall back to defaults. Before overwriting an invalid file, the store preserves a backup. Reads and exports have a 1 MiB limit. Theme files carry palette and typography. Layout files carry section configuration. A custom font path stays local.

## Build and verification boundaries

The GPUI dependency enables `runtime_shaders` so this Command Line Tools installation can compile without the offline Metal toolchain from full Xcode. The vendored macmon 0.10.0 manifest changes its Core Foundation dependency to 0.10 for compatibility with the application. No sampling implementation is changed in that vendored copy.

Release builds disable stripping for build-time procedural macro libraries to avoid the Rust macOS LINKEDIT alignment issue. The application executable retains the release profile's optimization and stripping settings.

`scripts/verify_gpui.py PID` drives the real window through a Swift helper and Vision OCR. GPUI 0.2.2 does not expose content labels through macOS Accessibility. Native captures and review evidence live in `artifacts/redesign`. See [verification](verification.md) for results and limits.

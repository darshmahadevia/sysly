# Architecture

Sysly uses Rust GPUI 0.2.2 with gpui-component 0.5.1 and a native AppKit-backed menu bar. GPUI renders the window. There is no webview or server. The collector reads OS APIs directly.

## Interface ownership

`main.rs` handles the sampling CLI before GUI startup. It initializes GPUI, component assets, the dark control theme, key bindings, and the root window.

`src/app/mod.rs` has one root `App` entity. It owns the monitor, current `Arc<Snapshot>`, bounded history, preferences, store, tray, selected page, and process selection. Screens are `impl App` blocks in sibling modules: `chrome.rs` (sidebar, toolbar, toasts), `overview.rs`, `detail.rs`, `processes.rs`, and `settings.rs`. `chart.rs` paints traces, and `style.rs` holds design tokens and primitives. The cheap `Ui` value carries the palette and type scale into closures.

`src/insight.rs` is pure. It folds helpers into apps and turns readings into the verdict, so its wording and thresholds are unit tested. `src/icons.rs` loads real app icons on a background thread and reads the Reduce Motion setting.

The root polls monitor and tray events every 200 milliseconds. `App::record` computes everything derived from a sample once: history, app grouping, top consumers, and the verdict. Rendering then only reads. History retains at most 902 samples and 15 minutes, regardless of the range on screen. Render closures share history through an `Arc`. Pauses mark a break before the next point, and missing values split traces rather than becoming zero. Each history entry also records the busiest app, so the past can be questioned.

Charts register their painted bounds in a shared list. The root's mouse handler maps a pointer position to the nearest sample and re-renders only when that sample changes.

Charts update on samples by default. The optional `smooth_charts` setting interpolates the viewport and requests animation frames between samples. GPUI 0.2.2 propagates view invalidation to ancestors, so continuous chart scrolling also rebuilds the dashboard. Sample-driven charts avoid that work.

The section vector is the source of order, visibility, width, chart height, and custom copy. Consecutive full-width chart sections render as recorder lanes. Named layouts snapshot that vector. Settings update the component theme and schedule a save after 400 milliseconds. The store also saves on root drop.

## Sampling and process actions

`model.rs` defines snapshots without UI or OS handle types. `collector.rs` owns OS handles and cumulative counter baselines. `monitor.rs` owns a sampler worker and a separate process-action worker. The sampler replaces one latest snapshot rather than queuing a history backlog. The action channel has a capacity of eight.

Pausing prevents publication of an in-flight sample and puts the sampler to sleep. A hidden window increases the sampling interval to at least five seconds. The worker waits on a condition variable between samples so policy changes interrupt the wait. `Collector::sample_now` uses macmon's retained IOReport baseline across those waits. The CLI retains its blocking `sample(interval)` interface. Process actions remain independent of hardware sampling waits. `tray.rs` sends menu events to the same UI root.

Process identity includes PID and start time. The signal boundary refreshes the process before sending TERM, KILL, STOP, or CONT. macOS has no atomic PID-plus-start-time signal API. A narrow exit and PID-reuse race remains between the final check and `kill`.

## Data and persistence

Data sources are sysinfo for CPU, memory, processes, networks, and disks, macmon for Apple Silicon hardware metrics, and native IOKit for battery properties. Mach VM counters supply memory composition, a sysctl supplies the kernel's memory pressure level, and `NSProcessInfo` supplies thermal state. Battery and disk topology refresh every five seconds. User names are indexed by UID and refreshed every 60 seconds. Rates use monotonic observation time and counter identity. A reset establishes a fresh baseline.

`Snapshot.collected_ms` measures post-hardware OS collection. It excludes macmon processing and its sampling wait. It does not measure total application overhead.

`preferences.rs` retains settings format version 1, with new fields defaulted so older files still load. Stock themes saved by earlier versions move to the current presets, and customized themes are left alone. It also keeps normalization, bounded imports, named layouts, and atomic writes. Invalid settings fall back to defaults. Before overwriting an invalid file, the store preserves a backup. Reads and exports have a 1 MiB limit. Theme files carry palette and typography. Layout files carry section configuration. A custom font path stays local.

## Build and verification boundaries

The GPUI dependency enables `runtime_shaders` so this Command Line Tools installation can compile without the offline Metal toolchain from full Xcode. The vendored macmon 0.10.0 manifest changes its Core Foundation dependency to 0.10 for compatibility with the application. No sampling implementation is changed in that vendored copy.

Release builds disable stripping for build-time procedural macro libraries to avoid the Rust macOS LINKEDIT alignment issue. The application executable retains the release profile's optimization and stripping settings.

`scripts/verify_gpui.py PID` drives the real window through a Swift helper and Vision OCR. GPUI 0.2.2 does not expose content labels through macOS Accessibility. See [verification](verification.md) for results and limits.

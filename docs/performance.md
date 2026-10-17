# Overhead measurements

Sysly defaults to sample-driven charts. Continuous chart scrolling remains available in Appearance. Both modes collect the same readings at the configured interval.

## Recorded results

On the 10-core Apple Silicon test Mac, median foreground CPU use fell from 25.52% to 2.58% of one core, about 90% lower. The five session averages ranged from 25.02% to 26.34% before and 2.42% to 3.47% after. Repeated dashboard rendering was the main cost removed.

Whole-app memory changed much less. Median physical footprint fell from 267.1 MiB to 264.5 MiB. Median RSS fell from 134.1 MiB to 133.2 MiB. These are modest changes, not a large reduction in resident memory. The removed chart allocations reduce allocation traffic. Native rendering still carries its own memory cost.

The [recorded session results](performance-results.json) include executable hashes, medians, ranges, and rejected-window counts. Each build has five sessions, with five accepted observation windows per session. Four pairs completed before a conversation interruption. Both builds in the fifth pair ran again after resumption, and the interrupted unpaired baseline is excluded.

## What changed and why

Continuous scrolling requested a new frame from the root view. GPUI 0.2.2 then reconstructed the dashboard even when the collector had no new data. Making that animation optional removes repeated rendering from the default monitoring workload.

Charts now share bounded history and per-core arrays. They stream clipped points into their paths rather than allocate temporary vectors. The overview computes its busiest processes once per sample, and the process page builds its row index only while open. The collector caches user names for 60 seconds and removes temporary command and counter-cleanup collections.

The sampling worker uses interruptible interval waits. Reopening the window or selecting a shorter interval wakes it without creating another collector thread. macmon retains its hardware counter baseline between calls.

## Verification

The final implementation passes 32 release tests and release clippy with warnings denied. The real collector matches macOS memory and core counts, detects a CPU workload, and measures a verified 16 MiB loopback transfer.

Native interaction checks verify process search, tree view, inspection, confirmed STOP, CONT, and TERM signals, pause and resume, closing to the menu bar, reopening, and Appearance navigation. Frame captures verify that sample-driven charts hold between samples, smooth charts move between samples, and paused charts remain frozen. The smoothing preference survives storage and older version-1 settings default to sample-driven updates.

The refreshed `dist/Sysly.zip` passes archive integrity checks and strict signature verification after extraction outside the synced workspace. Its executable's machine code matches the measured release build. A native launch and clean quit also pass. macOS File Provider reattaches Finder metadata to `dist/Sysly.app` in this workspace, so strict signature checks on that local directory fail. Extract the ZIP outside the synced folder when checking its signature.

## Reproduce the comparison

Build both revisions with the same locked dependencies, Rust compiler, and release profile. Preserve each executable at a different path. Compile the native helper with `swiftc scripts/ui.swift -o target/sysly-ui`.

Run this on an idle Mac with no other Sysly instance or build running:

```sh
python3 scripts/compare_overhead.py /path/to/baseline/sysly target/release/sysly
```

The script alternates five fresh sessions of each build. It uses isolated default settings and a visible 1320 by 900 Overview window. Each session warms up for 20 seconds, then records five five-second windows. It verifies process identity, liveness, native window visibility, foreground focus, and CPU-counter agreement with `ps`. It prevents display sleep, rejects windows that lose foreground focus, and reacquires focus before retrying. Retries are bounded and their count appears in the report. First-session screenshots provide a separate check that the dashboard rendered.

CPU is time-weighted across observation windows and expressed as a percentage of one core. RSS includes shared resident pages. Physical footprint is a separate macOS accounting measure. Neither is peak allocated Rust heap memory. The renderer, native window buffers, fonts, and process count affect the results.

The default comparison covers short sessions with partially filled history. It is not a long-running leak test. Removing the chart allocations alone did not produce a measurable whole-app improvement in the first five-session comparison. The sample-driven default addresses the repeated rendering that remained.

To include a full default history and periodic user-name refreshes, use a longer run:

```sh
python3 scripts/compare_overhead.py /path/to/baseline/sysly target/release/sysly --runs 1 --warmup 245 --seconds 24 --output .audit/optimization/mature-comparison
```

One run per build is a supplemental observation. It does not establish repeatability by itself. The script records executable hashes, host information, load averages, per-window readings, and session ranges in its JSON report.

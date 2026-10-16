# Dashboard

Sysly is a native macOS GPUI workspace for current activity, resource history, and process inspection. The visual direction is recorded in [DESIGN.md](../DESIGN.md). The accepted redesign brief is in [direction.md](../artifacts/redesign/direction.md).

The left sidebar opens Overview, Apps & processes, Appearance, and individual resources. Resource shortcuts include live readings. The header holds the current page title and sampling controls. The bottom status line reports sampling state and uptime.

Overview presents configurable sections for processor, memory, graphics, network, storage, battery, and processes. Adjacent half-width sections share a row. Full-width sections occupy their own row. The workspace scrolls vertically, including at the minimum window size of 1000 by 700 logical pixels. View details opens a resource page with additional measurements.

Apps & processes provides search by name, PID, or user, sortable columns, and a tree view that retains matching processes' ancestry. Select a process to inspect its identity, resource use, and command. Each signal requires a separate confirmation.

Appearance has three tabs. Appearance controls palettes, individual colors, font selection, and text size. Layout & text controls dashboard copy, section visibility, ordering, width, chart height, and named layouts. Charts & menu bar controls history, refresh rate, density, and menu bar readings. Layout editors support drag ordering and chart resize handles alongside button controls. Themes and layouts can be imported and exported separately.

Keyboard shortcuts are Command-1 for Overview, Command-2 for Apps & processes, Command-Comma for Appearance, and Command-F for process search. Escape dismisses the active overlay. Command-Q quits.

The menu bar keeps CPU and memory summaries available when the window is closed. Its menu reopens Sysly, pauses or resumes sampling, and quits. If menu bar initialization fails, closing the window quits the app.

Native screenshots and review evidence belong in [artifacts/redesign](../artifacts/redesign/). The older `.impeccable/review` screenshots show the previous interface and are not evidence for this GPUI build.

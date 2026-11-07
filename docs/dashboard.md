# Dashboard

Sysly is a native macOS GPUI app. It says how your Mac is doing in one sentence, then shows the evidence. The visual direction is recorded in [DESIGN.md](../DESIGN.md).

The sidebar opens Overview, Apps & processes, each resource, and Settings. Each resource row has a live sparkline and reading. The toolbar shows the page title, sampling state, and Pause. On Overview it also holds Customize.

## Overview

The verdict at the top is a plain sentence. It changes for heat, memory pressure, a nearly full startup disk, a nearly empty battery, sustained processor load, and single busy apps. CPU uses a 30-second average, so one spike cannot flip it. When the sentence names an app, a button opens it. High memory use alone is not reported as a problem, because memory pressure is what predicts slowdowns.

Below it, full-width sections join into the recorder: one panel with one time axis. Choose 1, 5, or 15 minutes. Sysly keeps 15 minutes of samples, so changing the range never loses history. Hover over any chart to scrub. Every lane shows its reading from that moment, and the heading says what was busiest then. Select a lane to open that resource.

## Resource pages

Each resource page has a large chart and the question that follows it. Processor shows a per-core strip that keeps a load hopping between cores visible. Memory breaks down what is in memory, shows pressure, and ranks what uses the most. Network lists real connections and lets you choose the one to chart. Storage shows volumes and ranks disk activity. Battery & power shows charge, health, power, and cooling.

## Apps & processes

By app folds helpers into their app and uses the app's real icon. All lists every process. Tree shows the hierarchy and keeps the ancestors of search matches. Search by name, PID, or user.

Select a row to open the inspector beside the table. It stays open while you browse, and tracks CPU and memory from the moment you opened it. Quit, Force Quit, and Pause each ask in a native sheet first. For an app, signals go to its main process.

## Customize

Press Customize on the Overview, or Command-E. Drag a section by its handle, or to the drop zone at the end. Drag a bottom edge to change its height. Rename it, switch between Half and Full, or hide it. Hidden sections wait at the bottom. Presets offer Balanced, Focus, and Compact, with Undo.

## Settings

General covers sampling, density, and the menu bar reading. Appearance covers themes, text, and every color. Black is the default. Automatic follows the system's light or dark setting. Dashboard covers the title, saved layouts, and layout files. Themes and layouts import and export separately.

## Keyboard

| Shortcut | Action |
| --- | --- |
| Command-1 | Overview |
| Command-2 | Apps & processes |
| Command-3 to Command-8 | Processor, Memory, Graphics, Network, Storage, Battery & power |
| Command-Comma | Settings |
| Command-F | Search processes |
| Command-P | Pause or resume sampling |
| Command-E | Customize Overview |
| Up and Down | Move through the process table |
| Tab and Space | Move between and press controls |
| Escape | Leave Customize, close the inspector, or clear search |
| Command-W | Close to the menu bar |
| Command-Q | Quit |

The native menu bar lists these commands too.

## Menu bar

Closing the window keeps Sysly in the menu bar. Its panel shows the same verdict, with processor, memory, graphics, and network readings. It reopens Sysly, pauses or resumes sampling, and quits. Settings can add a live processor or memory reading next to the icon. If menu bar initialization fails, closing the window quits the app.

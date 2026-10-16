---
name: Sysly
description: A native macOS workspace for real system activity.
colors:
  background: "#13171c"
  surface: "#1a1f26"
  text: "#eff0f4"
  muted: "#9ca9b5"
  divider: "#2b333d"
  accent: "#83abf8"
  cpu: "#64cdd7"
  memory: "#77d6b7"
  gpu: "#e9b670"
  network: "#70b0f2"
  danger: "#f17784"
typography:
  headline:
    fontFamily: ".AppleSystemUIFont"
    fontSize: "28px"
    fontWeight: 600
  title:
    fontFamily: ".AppleSystemUIFont"
    fontSize: "14px"
    fontWeight: 600
  body:
    fontFamily: ".AppleSystemUIFont"
    fontSize: "13px"
  label:
    fontFamily: ".AppleSystemUIFont"
    fontSize: "11px"
  data:
    fontFamily: "Menlo"
    fontSize: "11px"
  reading:
    fontFamily: ".AppleSystemUIFont"
    fontSize: "36px"
    fontWeight: 500
rounded:
  control: "7px"
  navigation: "8px"
  editor: "10px"
  card: "14px"
spacing:
  control-gap: "8px"
  row-gap: "12px"
  section-gap: "16px"
  compact-inset: "17px"
  card-inset: "23px"
  content-inset: "34px"
components:
  button:
    backgroundColor: "rgb(43 51 61 / 45%)"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    height: "32px"
    padding: "0 12px"
  button-hover:
    backgroundColor: "{colors.divider}"
  navigation-active:
    backgroundColor: "rgb(43 51 61 / 60%)"
    textColor: "{colors.text}"
    rounded: "{rounded.navigation}"
    height: "41px"
  section:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.card}"
    padding: "{spacing.card-inset}"
---

# Design System: Sysly

## Overview

**Creative North Star: "Your Mac, at a glance"**

Sysly presents system activity in a native Rust GPUI window. Graphite backgrounds, aligned readings, and resource colors give charts priority. The catalog-inspired direction uses spacing and typography to separate navigation, measurements, and actions.

These tokens record the default Graphite theme in `src/preferences.rs` and its application in `src/app.rs`. Saved themes, custom fonts, and layouts override defaults. Measurements use plain-language captions and explicit units.

**Key Characteristics:**

- A persistent sidebar with live resource readings.
- Colored history traces and a segmented memory composition.
- Tonal cards, rounded controls, and fine separators.
- User-controlled appearance, copy, and section layout.

## Colors

### Primary

The cool blue accent identifies focused controls and selected settings. CPU cyan, memory sea glass, graphics amber, and network blue distinguish measurements consistently across cards and resource navigation. The danger color identifies process actions with consequences.

### Neutral

The graphite background frames lighter section surfaces. Near-white text carries primary readings. Muted text carries descriptions, chart labels, and metadata. Dividers separate navigation and table rows without enclosing every measurement.

## Typography

The default native font is `.AppleSystemUIFont`, the macOS system UI family. Menlo aligns process table values and compact sidebar readings. Prominent resource readings retain the selected UI font.

The page heading uses the headline role. Section headings use the title role. Body text defaults to 13 logical pixels, with a saved range of 11 to 20. Many component labels retain explicit sizes. Users can select Menlo for the UI or load a local font file.

## Layout

The window opens at 1320 by 900 logical pixels and supports a minimum of 1000 by 700. A fixed 204-pixel sidebar sits beside the content area. Content has 34-pixel top and horizontal insets, a page header, a scrollable workspace, and a persistent 30-pixel status line.

Overview pairs adjacent half-width sections with a 16-pixel gap. Full-width sections interrupt the pair. The current implementation preserves these pairs at the minimum window size and scrolls vertically. Resource pages show one expanded section. Process rows use fixed metric columns and a flexible name column.

Cards use the comfortable inset by default and the compact inset when requested. Layout editing lives in Appearance, under Layout & text. Stable section identities preserve order, visibility, width, chart height, and custom copy.

## Elevation & Depth

Tonal contrast supplies depth. Cards have no decorative shadows. A translucent black scrim separates process inspection and confirmation from the workspace. Navigation changes fade the page from 86% to full opacity over 180 milliseconds with cubic-out easing.

## Shapes

Cards and confirmation panels use the card radius. Action buttons and resource rows use the control radius. Primary navigation uses the navigation radius. Layout editors use the editor radius. Small circular markers associate resources with their chart colors.

## Logo

Sysly's symbol is a continuous, rounded S trace with a detached sampling dot. The trace connects the name to live system monitoring. It transitions from the interface's blue through CPU cyan to memory sea glass; the dot uses sea glass. A graphite rounded tile frames the macOS app icon and sidebar logo. Keep the standalone symbol transparent, and use the monochrome variant where color is unavailable.

`scripts/icon.swift` owns the geometry and generates the SVG assets in `assets/brand`, a 1024-pixel PNG, and the complete macOS iconset. `scripts/bundle.sh` packages the icon into the app. The sidebar embeds the generated PNG so it also works outside the app bundle.

## Components

### Buttons

Actions use a translucent divider fill, medium-weight text, and a solid divider fill on hover. A one-pixel accent border marks keyboard focus. Confirmation buttons use a translucent danger fill and danger-colored text.

### Inputs

`gpui-component` input entities handle editing, selection, and focus. Search includes a search icon and the placeholder "Search name, PID or user". The control theme follows saved background, text, divider, and accent colors.

### Navigation

The sidebar lists Overview, Apps & processes, Appearance, and resource shortcuts. Selected rows use a stronger tonal fill. Muted labels become primary text on selection. Resource rows pair a colored marker with a live Menlo reading.

### Resource cards

A section heading and View details action sit above the reading. Charts use fine horizontal guides, scale labels, elapsed history, and semantic resource colors. Memory adds a segmented composition bar. Missing readings and paused intervals break history traces.

### Processes and inspection

The process list has a search field, a tree toggle, sortable columns, and virtualized rows. Inspection opens a panel with identity, measurements, and command text. A separate confirmation explains each signal before the user sends it.

## Do's and Don'ts

### Do:

- Do pair measurements with units and plain-language captions.
- Do preserve resource color roles across charts and navigation.
- Do retain saved themes, custom fonts, copy, and section arrangements.
- Do show gaps when sampling pauses or a reading is unavailable.

### Don't:

- Don't present missing telemetry as zero.
- Don't add decorative shadows or continuous decorative animation to the monitoring workspace.
- Don't imply that memory use alone measures memory pressure.
- Don't hide the consequence of a process action behind an unlabeled icon.

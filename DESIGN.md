---
name: Sysly
description: A native macOS instrument that tells you how your Mac is doing, then shows why.
colors:
  background: "#1b1c1f"
  surface: "#212226"
  text: "#ecedf0"
  muted: "#a4a6ae"
  divider: "#36383e"
  accent: "#40c4be"
  cpu: "#409fe3"
  memory: "#21b283"
  gpu: "#d2952a"
  network: "#9f80ee"
  storage: "#e2628e"
  battery: "#86b13a"
  danger: "#ff6b62"
typography:
  verdict:
    fontFamily: "New York"
    fontSize: "30px"
    fontWeight: 500
  title:
    fontFamily: ".AppleSystemUIFont"
    fontSize: "14px"
    fontWeight: 600
  body:
    fontFamily: ".AppleSystemUIFont"
    fontSize: "13px"
  label:
    fontFamily: ".AppleSystemUIFont"
    fontSize: "12px"
  smallest:
    fontFamily: ".AppleSystemUIFont"
    fontSize: "11px"
  figure:
    fontFamily: ".AppleSystemUIFont"
    fontFeature: "tnum"
rounded:
  segment: "5px"
  control: "7px"
  panel: "12px"
spacing:
  panel-gap: "14px"
  panel-inset: "16px"
  compact-inset: "12px"
  gutter: "24px"
---

# Design System: Sysly

## Concept

**Creative North Star: "The recorder"**

Sysly's mark is a trace with a sampling dot. The product follows it. Sysly is one instrument, not a wall of cards. It speaks one plain sentence about how the Mac is doing, then shows every resource on one shared time axis so that causes line up.

Two things make Sysly distinct, and everything else stays quiet so they can land.

- **The verdict.** A sentence in New York, the only serif in the app, answers "is my Mac OK?" and names the culprit when there is one. `src/insight.rs` owns the wording and thresholds.
- **The scrub.** Hovering any chart moves one cursor across all of them. Each lane shows its reading from that moment, and the heading says what was busiest then.

## Colors

Resource colors identify a resource everywhere: lanes, sparklines, meters, and the menu bar panel. They were checked with a categorical palette validator in both Graphite and Paper for color-blind separation, normal-vision separation, and contrast. The dark set sits slightly above the validator's lightness band on purpose, because it draws thin traces on graphite. Every lane also carries a text label, so identity never depends on color alone.

Values and labels use text colors, never a resource color. The accent marks selection and focus. Danger is reserved for destructive actions and errors. Graphics amber doubles as the warning color for verdicts and paused states, always with an icon or a word.

`Automatic` follows the system's light or dark setting with Paper and Graphite. Editing any color leaves Automatic and keeps the customized theme.

## Typography

System font throughout, with tabular digits on every figure so changing values never shift the layout. New York appears only for the verdict. Choosing Monospace or a custom font replaces it too.

The smallest text is 11 points. The text size setting scales every type size, plus the sidebar, lane labels, table columns, and inspector.

## Layout

The window opens at 1320 by 900 points, restores its last position, and has a minimum of 960 by 640. A 224-point sidebar sits beside a 52-point toolbar. The toolbar is also the title bar. There is no status bar.

Overview is the verdict, then sections. Consecutive full-width chart sections join into one recorder panel. Half-width sections pair up as panels. Order, width, height, visibility, and labels come from the saved section list.

Resource pages share a shape: a large chart, then "Using the most" and the facts behind it. The process table keeps its columns in priority order and drops the CPU bar, PID, Disk, and Memory as space runs out. The inspector sits beside the table and never exceeds 45% of the content area.

## Charts

- Lines are 1.5 points with a 14% area fill. A dot marks the latest sample.
- Vertical time divisions align across lanes, so the panel reads like one sheet of recorder paper.
- Paired measures share one scale, mirrored around a center line, with a label saying which side is which. Never use two y-axes.
- Scale labels round up to clean values in display units.
- While history is short, charts use the shortest range that holds it, so a new window is never a sliver at the right edge.
- Gaps stay gaps. Pauses and missing readings break the trace. A first sample's rates are baselines, so they are not charted.

## Motion

Motion answers an action and nothing else: a 160-millisecond page fade, a 150-millisecond inspector fade, and a 180-millisecond toast rise. All three stop when macOS Reduce Motion is on. Nothing moves on its own, and continuous chart scrolling is opt-in because it costs CPU.

## Components

**Buttons** come in four weights: primary for the one main action, secondary, quiet, and danger. Focus rings appear for keyboard users only. Clicking hands focus back to the window, because GPUI 0.2.2 has no focus-visible state.

**Segmented controls** carry every either-or choice and always show the selection. **Switches** carry on-off settings.

**Confirmations** for process signals are native sheets. The action keeps its name through the flow: Quit, "Quit Safari?", Quit, "Asked Safari to quit".

**Toasts** report results at the bottom of the window and offer Undo for layout changes. Errors stay until dismissed.

**Customize** happens on the Overview itself. Sections gain a handle, label fields, a width control, a hide button, and a height grip. Hidden sections wait in a tray at the bottom.

## Copy

Sentence case, plain verbs, no jargon. Say "Paused", not "SIGSTOP". Say "Efficiency cores", not "E clusters". Storage uses Finder's decimal units and memory uses binary units, so each matches what macOS shows elsewhere. Errors say what happened and what to do. Empty states offer the next step.

## Do's and Don'ts

### Do:

- Do lead with an answer, then the evidence.
- Do use memory pressure, not memory use, to decide whether memory is a problem.
- Do show "Reading…" until there is a reading, and "Not available" when there will not be one.
- Do retain saved themes, fonts, copy, and layouts.

### Don't:

- Don't present missing telemetry as zero or as fact, including before the first sample.
- Don't name a culprit unless one stands out.
- Don't split the Overview back into identical cards.
- Don't add decorative shadows or motion. The toast is the only floating element.
- Don't hide the consequence of a process action behind an unlabeled icon.

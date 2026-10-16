# Product

<!-- impeccable:product-schema 1 -->

## Platform
Native macOS desktop, Apple Silicon, macOS 14 or later.

## Stack
Rust with GPUI. The user requires GPUI for the complete native interface.

## Users
MacBook users inspecting system activity and identifying resource-heavy processes.

## Product purpose
Present the information and process controls available in btop in a polished desktop dashboard with a compact menu bar summary.

## Capabilities and constraints
CPU, per-core activity, memory, swap, storage, disk I/O, network interfaces and throughput, battery, process search, sorting, tree, details and signals. Include GPU, temperatures, frequencies, power and fans where hardware exposes them. Unavailable measurements must be explicit. Keep resource use low and measure observer overhead. Do not fabricate values.

## Brand commitments
Dark, clean native Mac interface. The user requested a complete visual redesign on 2026-10-16, with inspiration from https://vibrant.design/. Sysly is the working name inferred from the project folder.

The default experience must be modern, minimal, and understandable to nontechnical users. Technical measurements remain available through details. The user explicitly requires drag-and-drop layout customization, resizable sections, importable themes, editable text, fonts, and colors. Layouts and themes must persist and offer recovery to useful defaults.

## Evidence on hand
An existing Rust system monitor with working collectors, versioned preferences, and native automation. The pre-migration screenshot is artifacts/redesign/before.png. A local Apple Silicon Mac is available for verification.

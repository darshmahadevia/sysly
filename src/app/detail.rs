//! A page per resource: the recorder up close, then what is causing it.

use super::{
    App, TraceViewport, bytes, chart, decimal_bytes, duration, optional, rate, ratio, style::*,
};
use crate::{
    model::{Network, Pressure, Thermal},
    preferences::SectionKind,
};
use gpui::{AnyElement, Context, Div, FontWeight, Hsla, div, prelude::*, px, relative};

fn pressure_label(pressure: Pressure) -> &'static str {
    match pressure {
        Pressure::Normal => "Normal",
        Pressure::Warning => "Elevated",
        Pressure::Critical => "High",
        Pressure::Unknown => "Not available",
    }
}

/// Apple Silicon names its core groups with a letter.
fn tier_label(name: &str) -> String {
    match name.chars().next() {
        Some('E') => "Efficiency cores".into(),
        Some('P') => "Performance cores".into(),
        _ => format!("{name} cores"),
    }
}

/// Virtual interfaces with only a link-local address are plumbing, not connections.
fn worth_listing(network: &Network, selected: &str) -> bool {
    let routable = network.addresses.split(", ").any(|address| {
        !address.is_empty()
            && !address.starts_with("fe80")
            && !address.starts_with("127.")
            && address != "::1"
    });
    network.name == selected
        || routable
        || (!network.name.starts_with("lo") && network.received_per_sec + network.sent_per_sec > 0.)
}

/// The address someone would recognize: IPv4 first when there is one.
fn readable_address(addresses: &str) -> String {
    addresses
        .split(", ")
        .find(|address| address.contains('.'))
        .or_else(|| addresses.split(", ").next())
        .unwrap_or_default()
        .to_string()
}

fn thermal_label(thermal: Thermal) -> &'static str {
    match thermal {
        Thermal::Nominal => "Normal",
        Thermal::Fair => "Warm",
        Thermal::Serious => "Hot, performance limited",
        Thermal::Critical => "Very hot, performance limited",
        Thermal::Unknown => "Not available",
    }
}

impl App {
    fn hero(&self, kind: SectionKind, cx: &mut Context<Self>) -> Div {
        let ui = self.ui();
        let p = ui.p;
        let reading = self.reading(kind);
        let scrubbing = self.cursor.is_some();
        ui.panel()
            .p(px(ui.inset()))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap_2()
                                    .child(
                                        ui.figure(30., p.text, reading.primary)
                                            .font_weight(FontWeight::MEDIUM),
                                    )
                                    .child(ui.figure(13., p.muted, reading.caption)),
                            )
                            .child(div().h(px(22.)).flex().items_center().child(if scrubbing {
                                self.recorder_heading()
                            } else {
                                div().child(ui.muted(super::overview::range_label(
                                    self.preferences.history_secs,
                                )))
                            })),
                    )
                    .child(self.range_control(cx)),
            )
            .child(div().mt_2().child(self.scaled_chart(kind, 190.)))
            .child(div().mt(px(5.)).child(self.time_axis()))
    }

    fn facts(&self, title: &'static str, rows: Vec<(String, String)>) -> Div {
        let ui = self.ui();
        let mut panel = ui
            .panel()
            .flex_1()
            .min_w_0()
            .p(px(ui.inset()))
            .child(ui.strong(title).mb(px(6.)));
        for (name, value) in rows {
            panel = panel.child(ui.fact(name, value));
        }
        panel
    }

    fn ranked(&self, title: &'static str, list: AnyElement, note: Option<&'static str>) -> Div {
        let ui = self.ui();
        ui.panel()
            .flex_1()
            .min_w_0()
            .p(px(ui.inset()))
            .child(ui.strong(title).mb(px(6.)))
            .child(list)
            .children(note.map(|note| {
                ui.text(11., ui.p.muted, note)
                    .mt(px(8.))
                    .line_height(relative(1.35))
            }))
    }

    fn row_pair(&self, left: Div, right: Div) -> Div {
        div()
            .flex()
            .items_start()
            .gap(px(14.))
            .child(left)
            .child(right)
    }

    fn cores(&self) -> Div {
        let ui = self.ui();
        let p = ui.p;
        let cores = self.snapshot.cores.len();
        let row = 13.;
        let viewport = TraceViewport {
            end: self.graph_viewport.end,
            span: self.graph_viewport.span,
        };
        let mut names = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .w(px(56.))
            .flex_shrink_0();
        let mut values = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .w(px(44.))
            .flex_shrink_0();
        for (index, load) in self.snapshot.cores.iter().enumerate() {
            names = names.child(
                ui.figure(11., p.muted, format!("Core {}", index + 1))
                    .h(px(row))
                    .line_height(px(row)),
            );
            values = values.child(
                ui.figure(11., p.text, format!("{load:.0}%"))
                    .h(px(row))
                    .line_height(px(row))
                    .text_right(),
            );
        }
        ui.panel()
            .p(px(ui.inset()))
            .child(ui.strong("Each core").mb(px(8.)))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(names)
                    .child(div().flex_1().min_w_0().child(chart::core_strip(
                        self.history.clone(),
                        viewport,
                        cores,
                        p.cpu,
                        p.text.opacity(0.05),
                        row,
                    )))
                    .child(values),
            )
            .child(
                div()
                    .mt(px(5.))
                    .ml(px(64.))
                    .mr(px(52.))
                    .child(self.time_axis()),
            )
    }

    fn composition(&self) -> Option<Div> {
        let ui = self.ui();
        let p = ui.p;
        let m = &self.snapshot.memory;
        let c = m.composition?;
        let parts: [(&str, &str, u64, Hsla); 5] = [
            ("App memory", "Used by your apps", c.app, p.memory),
            (
                "Wired",
                "Reserved by macOS",
                c.wired,
                p.memory.opacity(0.68),
            ),
            (
                "Compressed",
                "Squeezed to save room",
                c.compressed,
                p.memory.opacity(0.42),
            ),
            (
                "Cached files",
                "Reclaimed when needed",
                c.cached,
                p.text.opacity(0.28),
            ),
            ("Free", "Not in use", c.free, p.text.opacity(0.09)),
        ];
        let total: u64 = parts.iter().map(|part| part.2).sum();
        let mut bar = div().flex().gap(px(2.)).h(px(14.)).w_full();
        let mut legend = div().flex().justify_between().gap_3().mt(px(10.));
        for (name, hint, value, tint) in parts {
            bar = bar.child(
                div()
                    .h_full()
                    .flex_basis(relative(value as f32 / total.max(1) as f32))
                    .flex_grow()
                    .min_w(px(2.))
                    .rounded(px(3.))
                    .bg(tint),
            );
            legend = legend.child(
                div()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(div().size(px(8.)).rounded(px(2.)).bg(tint))
                            .child(ui.text(12., p.muted, name)),
                    )
                    .child(ui.figure(15., p.text, bytes(value)).mt(px(2.)))
                    .child(ui.text(11., p.muted, hint).truncate()),
            );
        }
        Some(
            ui.panel()
                .p(px(ui.inset()))
                .child(ui.strong("What is in memory").mb(px(10.)))
                .child(bar)
                .child(legend),
        )
    }

    fn interfaces(&self, cx: &mut Context<Self>) -> Div {
        let ui = self.ui();
        let p = ui.p;
        let mut panel = ui
            .panel()
            .p(px(ui.inset()))
            .child(ui.strong("Connections").mb(px(6.)));
        for network in self
            .snapshot
            .networks
            .iter()
            .filter(|n| worth_listing(n, &self.interface))
        {
            let name = network.name.clone();
            let selected = name == self.interface;
            panel = panel.child(
                div()
                    .id(gpui::SharedString::from(format!("interface-{name}")))
                    .tab_index(0)
                    .flex()
                    .items_center()
                    .gap_3()
                    .min_h(px(ui.row_height()))
                    .mx(px(-6.))
                    .px(px(6.))
                    .py(px(3.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .when(selected, |s| s.bg(p.network.opacity(0.14)))
                    .when(!selected, |s| s.hover(move |s| s.bg(p.hover)))
                    .child(
                        ui.text(12., p.text, name.clone())
                            .w(px(70.))
                            .flex_shrink_0(),
                    )
                    .child(
                        ui.figure(11., p.muted, readable_address(&network.addresses))
                            .flex_1()
                            .min_w_0()
                            .truncate(),
                    )
                    .child(
                        ui.figure(
                            12.,
                            p.text,
                            format!(
                                "↓ {}  ↑ {}",
                                rate(network.received_per_sec),
                                rate(network.sent_per_sec)
                            ),
                        )
                        .flex_shrink_0(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.interface != name {
                            this.interface = name.clone();
                            this.update_tray();
                            for history in std::sync::Arc::make_mut(&mut this.history) {
                                history.received = None;
                                history.sent = None;
                            }
                            cx.notify();
                        }
                    })),
            );
        }
        panel
    }

    fn volumes(&self) -> Div {
        let ui = self.ui();
        let p = ui.p;
        let mut panel = ui
            .panel()
            .flex_1()
            .min_w_0()
            .p(px(ui.inset()))
            .child(ui.strong("Volumes").mb(px(8.)));
        if self.snapshot.disks.is_empty() {
            panel = panel.child(ui.muted("Reading drives…"));
        }
        // An APFS system and data pair reports the same name and space twice.
        let mut seen = std::collections::HashSet::new();
        for disk in self
            .snapshot
            .disks
            .iter()
            .filter(|d| seen.insert((d.name.as_str(), d.total)))
        {
            let used = disk.total.saturating_sub(disk.available);
            panel = panel.child(
                div()
                    .mb(px(12.))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap_3()
                            .mb(px(5.))
                            .child(
                                ui.text(
                                    12.,
                                    p.text,
                                    if disk.name.is_empty() {
                                        disk.mount.clone()
                                    } else {
                                        disk.name.clone()
                                    },
                                )
                                .truncate(),
                            )
                            .child(
                                ui.figure(
                                    12.,
                                    p.muted,
                                    format!(
                                        "{} of {} free",
                                        decimal_bytes(disk.available),
                                        decimal_bytes(disk.total)
                                    ),
                                )
                                .flex_shrink_0(),
                            ),
                    )
                    .child(ui.meter(ratio(used, disk.total) / 100., p.storage, 6.)),
            );
        }
        panel
    }

    pub(super) fn resource(&self, kind: SectionKind, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let s = self.snapshot.clone();
        let h = &s.hardware;
        let mut page = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(GUTTER))
            .pt(px(18.));
        match kind {
            SectionKind::Processor => {
                let mut rows = vec![
                    (
                        "Load, last 1, 5 and 15 minutes".to_string(),
                        format!("{:.2}, {:.2}, {:.2}", s.load[0], s.load[1], s.load[2]),
                    ),
                    ("Temperature".into(), optional(h.cpu_temp, " °C")),
                    ("Thermal state".into(), thermal_label(s.thermal).into()),
                    ("Power".into(), optional(h.cpu_watts, " W")),
                ];
                for (name, mhz) in &h.tiers {
                    rows.push((tier_label(name), format!("{mhz} MHz")));
                }
                page = page
                    .child(self.hero(kind, cx))
                    .child(self.cores())
                    .child(self.row_pair(
                        self.ranked(
                            "Using the most",
                            self.consumer_list(
                                &self.top_cpu,
                                6,
                                p.cpu,
                                |c| c.cpu / 100.,
                                |c| format!("{:.1}%", c.cpu),
                                cx,
                            ),
                            Some("100% equals one fully busy core, so an app can exceed 100%."),
                        ),
                        self.facts("Details", rows),
                    ));
            }
            SectionKind::Memory => {
                let m = &s.memory;
                let total = m.total.max(1) as f32;
                page = page
                    .child(self.hero(kind, cx))
                    .children(self.composition())
                    .child(self.row_pair(
                        self.ranked(
                            "Using the most",
                            self.consumer_list(
                                &self.top_memory,
                                6,
                                p.memory,
                                |c| c.memory as f32 / total,
                                |c| bytes(c.memory),
                                cx,
                            ),
                            None,
                        ),
                        self.facts(
                            "Details",
                            vec![
                                ("Memory pressure".into(), pressure_label(m.pressure).into()),
                                ("Available".into(), bytes(m.available)),
                                (
                                    "Swap".into(),
                                    format!("{} of {}", bytes(m.swap_used), bytes(m.swap_total)),
                                ),
                            ],
                        )
                        .child(
                            ui.text(
                                11.,
                                p.muted,
                                "Pressure is what affects speed. High memory use with normal pressure is healthy: macOS keeps memory busy on purpose.",
                            )
                            .mt(px(8.))
                            .line_height(relative(1.35)),
                        ),
                    ));
            }
            SectionKind::Graphics => {
                page = page.child(self.hero(kind, cx)).child(self.row_pair(
                    self.facts(
                        "Details",
                        vec![
                            ("Temperature".into(), optional(h.gpu_temp, " °C")),
                            (
                                "Frequency".into(),
                                h.gpu_mhz
                                    .map_or("Not available".into(), |v| format!("{v} MHz")),
                            ),
                            ("Power".into(), optional(h.gpu_watts, " W")),
                        ],
                    ),
                    ui.panel()
                        .flex_1()
                        .min_w_0()
                        .p(px(ui.inset()))
                        .child(ui.strong("About this reading").mb(px(6.)))
                        .child(
                            ui.muted(
                                "Activity is the share of time the graphics processor was doing work, not how fast it is. macOS does not report graphics use per app.",
                            )
                            .line_height(relative(1.4)),
                        ),
                ));
            }
            SectionKind::Network => {
                let selected = s.networks.iter().find(|n| n.name == self.interface);
                page = page
                    .child(self.hero(kind, cx))
                    .child(self.interfaces(cx))
                    .children(selected.map(|n| {
                        self.facts(
                            "Since this Mac started",
                            vec![
                                ("Downloaded".into(), bytes(n.total_received)),
                                ("Uploaded".into(), bytes(n.total_sent)),
                                ("Errors".into(), n.errors.to_string()),
                            ],
                        )
                    }));
            }
            SectionKind::Storage => {
                let active_io: Vec<usize> = self
                    .top_io
                    .iter()
                    .copied()
                    .filter(|&i| self.consumers[i].io > 0.)
                    .collect();
                page = page.child(self.hero(kind, cx)).child(self.row_pair(
                    self.volumes(),
                    self.ranked(
                        "Reading and writing the most",
                        if active_io.is_empty() {
                            ui.muted("Nothing is reading or writing right now.")
                                .into_any_element()
                        } else {
                            self.consumer_list(
                                &active_io,
                                6,
                                p.storage,
                                |c| c.io as f32,
                                |c| rate(c.io),
                                cx,
                            )
                        },
                        None,
                    ),
                ));
            }
            SectionKind::Battery => {
                let mut rows = vec![];
                if let Some(b) = &s.battery {
                    rows.push(("Charge".to_string(), format!("{:.0}%", b.percent)));
                    rows.push(("Status".to_string(), b.state.clone()));
                    rows.push((
                        "Time remaining".to_string(),
                        b.remaining_secs
                            .map_or("Not available".to_string(), duration),
                    ));
                    rows.push((
                        "Battery health".to_string(),
                        format!("{:.0}%", b.health_percent),
                    ));
                    rows.push((
                        "Charge cycles".to_string(),
                        b.cycles.map_or("Not available".into(), |v| v.to_string()),
                    ));
                    rows.push(("Battery power".to_string(), format!("{:.1} W", b.watts)));
                }
                let mut power = vec![
                    ("System".to_string(), optional(h.system_watts, " W")),
                    ("Processor".to_string(), optional(h.cpu_watts, " W")),
                    ("Graphics".to_string(), optional(h.gpu_watts, " W")),
                ];
                for (name, rpm) in &h.fans {
                    power.push((name.clone(), format!("{rpm} RPM")));
                }
                if h.fans.is_empty() {
                    power.push(("Fans".to_string(), "None reported".to_string()));
                }
                page = page.child(self.hero(kind, cx)).child(self.row_pair(
                    if rows.is_empty() {
                        ui.panel()
                            .flex_1()
                            .min_w_0()
                            .p(px(ui.inset()))
                            .child(ui.strong("Battery").mb(px(6.)))
                            .child(ui.muted("This Mac has no battery."))
                    } else {
                        self.facts("Battery", rows)
                    },
                    self.facts("Power and cooling", power),
                ));
            }
            SectionKind::Processes => {}
        }
        page.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(name: &str, addresses: &str, traffic: f64) -> Network {
        Network {
            name: name.into(),
            addresses: addresses.into(),
            received_per_sec: traffic,
            ..Network::default()
        }
    }

    #[test]
    fn connections_hide_link_local_plumbing() {
        assert!(worth_listing(
            &network("en0", "fe80::1, 192.168.4.1", 0.),
            "x"
        ));
        assert!(!worth_listing(&network("utun3", "fe80::9", 0.), "x"));
        assert!(!worth_listing(&network("lo0", "127.0.0.1, ::1", 900.), "x"));
        assert!(worth_listing(&network("utun3", "fe80::9", 80.), "x"));
        assert!(worth_listing(&network("utun3", "fe80::9", 0.), "utun3"));
    }

    #[test]
    fn prefers_the_address_people_recognize() {
        assert_eq!(readable_address("fe80::1, 192.168.4.1"), "192.168.4.1");
        assert_eq!(readable_address("fe80::1"), "fe80::1");
        assert_eq!(readable_address(""), "");
    }
}

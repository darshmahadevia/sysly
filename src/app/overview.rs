//! The overview: one sentence about your Mac, then the recorder.

use super::{
    App, DragSection, Edit, Page, Resize, TraceSeries, TraceViewport, bytes,
    chart::Plot,
    chart::{nice_ceiling, nice_rate},
    clock_time, decimal_bytes, duration, optional, rate, series_maximum, startup_disk,
    style::*,
};
use crate::{
    insight::Level,
    preferences::{HISTORY_RANGES, SectionConfig, SectionKind, Span},
};
use gpui::{
    AnyElement, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent, SharedString, div,
    prelude::*, px, relative,
};
use gpui_component::{Icon, IconName, tooltip::Tooltip};

/// What a section says at a glance: its main reading and one line of context.
pub(super) struct Reading {
    pub primary: String,
    pub caption: String,
}

pub(super) fn is_lane(kind: SectionKind) -> bool {
    matches!(
        kind,
        SectionKind::Processor
            | SectionKind::Memory
            | SectionKind::Graphics
            | SectionKind::Network
            | SectionKind::Storage
    )
}

pub(super) fn plural(count: usize, one: &str) -> String {
    let suffix = match (count, one.ends_with('s')) {
        (1, _) => "",
        (_, true) => "es",
        (_, false) => "s",
    };
    format!("{count} {one}{suffix}")
}

pub(super) fn range_label(seconds: u32) -> String {
    if seconds < 120 {
        "Last minute".to_string()
    } else {
        format!("Last {} minutes", seconds / 60)
    }
}

/// Number of equal time divisions shown for a range: every 15 seconds, one
/// minute or three minutes.
pub(super) fn divisions(seconds: u32) -> u32 {
    if seconds <= 60 { 4 } else { 5 }
}

impl App {
    /// The reading at the scrubbed moment, or the current one.
    pub(super) fn reading(&self, kind: SectionKind) -> Reading {
        let s = &self.snapshot;
        if s.sequence == 0 {
            return Reading {
                primary: "–".into(),
                caption: "Reading…".into(),
            };
        }
        let at = self.sample_at_cursor();
        let m = &s.memory;
        let selected = s.networks.iter().find(|n| n.name == self.interface);
        match kind {
            SectionKind::Processor => Reading {
                primary: format!("{:.0}%", at.map_or(s.cpu_percent, |h| h.cpu)),
                caption: if s.cores.is_empty() {
                    "Reading…".into()
                } else {
                    format!("of {}", plural(s.cores.len(), "core"))
                },
            },
            SectionKind::Memory => {
                let used = at
                    .map(|h| (h.memory as f64 / 100. * m.total as f64) as u64)
                    .unwrap_or(m.used);
                Reading {
                    primary: bytes(used),
                    caption: format!("of {}", bytes(m.total)),
                }
            }
            SectionKind::Graphics => {
                let value = at.map_or(s.hardware.gpu_percent, |h| h.gpu);
                Reading {
                    primary: value
                        .map(|v| format!("{v:.0}%"))
                        .unwrap_or_else(|| "Not available".into()),
                    caption: match s.hardware.gpu_mhz {
                        Some(mhz) if value.is_some() => format!("active, at {mhz} MHz"),
                        _ => "active".into(),
                    },
                }
            }
            SectionKind::Network => {
                let (down, up) = match at {
                    Some(h) => (h.received, h.sent),
                    None => (
                        selected.map(|n| n.received_per_sec),
                        selected.map(|n| n.sent_per_sec),
                    ),
                };
                Reading {
                    primary: format!("↓ {}", down.map_or("–".into(), rate)),
                    caption: format!("↑ {}", up.map_or("–".into(), rate)),
                }
            }
            SectionKind::Storage => {
                let disk = startup_disk(s);
                let (read, write) = match at {
                    Some(h) => (h.disk_read, h.disk_write),
                    None => (
                        disk.and_then(|d| d.read_per_sec),
                        disk.and_then(|d| d.write_per_sec),
                    ),
                };
                Reading {
                    primary: disk
                        .map(|d| format!("{} free", decimal_bytes(d.available)))
                        .unwrap_or_else(|| "Reading…".into()),
                    caption: match (read, write) {
                        (Some(r), Some(w)) => format!("↓ {}  ↑ {}", rate(r), rate(w)),
                        _ => "No activity reading".into(),
                    },
                }
            }
            SectionKind::Battery => Reading {
                primary: s
                    .battery
                    .as_ref()
                    .map(|b| format!("{:.0}%", b.percent))
                    .unwrap_or_else(|| optional(s.hardware.system_watts, " W")),
                caption: s
                    .battery
                    .as_ref()
                    .map(|b| b.state.clone())
                    .unwrap_or_else(|| "System power".into()),
            },
            SectionKind::Processes => Reading {
                primary: s.processes.len().to_string(),
                caption: "running".into(),
            },
        }
    }

    /// The plot for a section, plus the label for the top of its scale.
    pub(super) fn plot(
        &self,
        kind: SectionKind,
        scrubbable: bool,
        viewport: TraceViewport,
    ) -> (Plot, String) {
        let (series, mirror, maximum, label) = match kind {
            SectionKind::Memory => (TraceSeries::Memory, None, 100., "100%".to_string()),
            SectionKind::Graphics => (TraceSeries::Gpu, None, 100., "100%".to_string()),
            SectionKind::Network => {
                let top = nice_rate(series_maximum(
                    &self.history,
                    viewport,
                    &[TraceSeries::Received, TraceSeries::Sent],
                    1024.,
                ));
                (
                    TraceSeries::Received,
                    Some(TraceSeries::Sent),
                    top as f32,
                    format!("{}   ↓ above the line, ↑ below", rate(top)),
                )
            }
            SectionKind::Storage => {
                let top = nice_rate(series_maximum(
                    &self.history,
                    viewport,
                    &[TraceSeries::DiskRead, TraceSeries::DiskWrite],
                    1024. * 1024.,
                ));
                (
                    TraceSeries::DiskRead,
                    Some(TraceSeries::DiskWrite),
                    top as f32,
                    format!("{}   Reads above the line, writes below", rate(top)),
                )
            }
            SectionKind::Battery => {
                let top = nice_ceiling(series_maximum(
                    &self.history,
                    viewport,
                    &[TraceSeries::Power],
                    5.,
                ));
                (TraceSeries::Power, None, top as f32, format!("{top:.0} W"))
            }
            _ => (TraceSeries::Cpu, None, 100., "100%".to_string()),
        };
        (
            Plot {
                history: self.history.clone(),
                viewport,
                series,
                mirror,
                maximum,
                color: self.section_color(kind),
                divisions: divisions(self.graph_viewport.span as u32),
                cursor: self.cursor,
                registry: scrubbable.then(|| self.charts.clone()),
                stroke: 1.5,
                decorated: true,
            },
            label,
        )
    }

    /// A chart with its scale label laid over the top corner.
    pub(super) fn scaled_chart(&self, kind: SectionKind, height: f32) -> AnyElement {
        let ui = self.ui();
        let (plot, label) = self.plot(kind, true, self.graph_viewport);
        div()
            .relative()
            .w_full()
            .child(plot.element(&ui, height))
            .child(
                ui.figure(11., ui.p.muted, label)
                    .absolute()
                    .top(px(2.))
                    .left(px(6.)),
            )
            .into_any_element()
    }

    pub(super) fn time_axis(&self) -> Div {
        let ui = self.ui();
        let span = self.graph_viewport.span as u32;
        let steps = divisions(span);
        let mut axis = div().flex().justify_between();
        for step in 0..=steps {
            let left = span - span * step / steps;
            let text = if left == 0 {
                "Now".to_string()
            } else if span <= 60 {
                format!("{left}s")
            } else {
                format!("{} min", left / 60)
            };
            axis = axis.child(ui.figure(11., ui.p.muted, text));
        }
        axis
    }

    pub(super) fn range_control(&self, cx: &mut Context<Self>) -> Div {
        let ui = self.ui();
        let mut control = ui.segments();
        for range in HISTORY_RANGES {
            let label = if range < 120 {
                "1 min".to_string()
            } else {
                format!("{} min", range / 60)
            };
            control = control.child(
                ui.segment(
                    ("range", range as usize),
                    label,
                    self.preferences.history_secs == range,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.preferences.history_secs = range;
                    this.save(cx);
                })),
            );
        }
        control
    }

    /// Says which moment the recorder is showing, and what was busiest then.
    pub(super) fn recorder_heading(&self) -> Div {
        let ui = self.ui();
        let p = ui.p;
        let latest = self.history.back();
        if let (Some(at), Some(latest)) = (self.sample_at_cursor(), latest) {
            let age = (latest.time - at.time).round() as u64;
            let when = if age == 0 {
                "Just now".to_string()
            } else {
                format!("{} ago", duration(age))
            };
            return div()
                .flex()
                .items_center()
                .gap(px(10.))
                .min_w_0()
                .child(ui.strong(when).flex_shrink_0())
                .child(
                    ui.figure(12., p.muted, clock_time(at.clock))
                        .flex_shrink_0(),
                )
                .children(at.busiest.as_ref().map(|(name, cpu)| {
                    ui.text(12., p.text, format!("{name} was busiest, at {cpu:.0}%"))
                        .truncate()
                }));
        }
        let mut heading =
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(ui.strong(if self.paused {
                    latest.map_or("Paused".to_string(), |h| {
                        format!("Paused at {}", clock_time(h.clock))
                    })
                } else {
                    range_label(self.preferences.history_secs)
                }));
        let recorded = self.recorded_secs();
        if !self.paused && recorded < self.preferences.history_secs as f64 - 2. {
            heading = heading.child(ui.muted(if recorded < 2. {
                "Recording started".to_string()
            } else {
                format!("Recording for {}", duration(recorded as u64))
            }));
        }
        heading
    }

    fn verdict_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let verdict = &self.verdict;
        let culprit = verdict.culprit.clone();
        let tint = match verdict.level {
            Level::Calm => None,
            Level::Busy => Some(p.warning),
            Level::Strained => Some(p.danger),
        };
        div()
            .flex()
            .items_end()
            .justify_between()
            .gap_6()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .max_w(px(680.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .children(tint.map(|tint| {
                                Icon::new(IconName::TriangleAlert)
                                    .size(px(20.))
                                    .text_color(tint)
                            }))
                            .child(ui.verdict(verdict.headline.clone())),
                    )
                    .child(
                        ui.text(14., p.muted, verdict.detail.clone())
                            .mt(px(6.))
                            .line_height(relative(1.4)),
                    ),
            )
            .children(culprit.map(|(key, name)| {
                ui.button("verdict-culprit", format!("Show {name}"))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_process(key, cx)))
            }))
            .into_any_element()
    }

    fn lane(&self, section: &SectionConfig, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let kind = section.kind;
        let reading = self.reading(kind);
        let tint = self.section_color(kind);
        let description: SharedString = section.description.clone().into();
        let heading = div()
            .id(("lane", kind as usize))
            .tab_index(0)
            .w(px(ui.lane_label_width()))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .justify_center()
            .pl(px(ui.inset()))
            .pr_2()
            .py(px(8.))
            .border_1()
            .border_color(gpui::transparent_black())
            .focus(move |s| s.border_color(p.accent))
            .cursor_pointer()
            .group("lane")
            .hover(move |s| s.bg(p.hover))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(div().w(px(8.)).h(px(2.)).rounded_full().bg(tint))
                    .child(ui.text(12., p.muted, section.title.clone()).truncate()),
            )
            .map(|s| {
                if kind == SectionKind::Network {
                    s.child(
                        ui.figure(14., p.text, reading.primary)
                            .font_weight(FontWeight::MEDIUM)
                            .mt(px(3.)),
                    )
                    .child(
                        ui.figure(14., p.text, reading.caption)
                            .font_weight(FontWeight::MEDIUM),
                    )
                } else {
                    s.child(
                        ui.figure(20., p.text, reading.primary)
                            .font_weight(FontWeight::MEDIUM)
                            .mt(px(2.))
                            .truncate(),
                    )
                    .child(ui.figure(12., p.muted, reading.caption).truncate())
                }
            })
            .tooltip(move |window, cx| Tooltip::new(description.clone()).build(window, cx))
            .on_click(cx.listener(move |this, _, _, cx| this.navigate(Page::Resource(kind), cx)));
        self.editable(
            section,
            div()
                .flex()
                .border_t_1()
                .border_color(p.line.opacity(0.7))
                .child(heading)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pr(px(ui.inset()))
                        .py(px(8.))
                        .child(self.scaled_chart(kind, section.graph_height)),
                ),
            cx,
        )
    }

    fn lanes(&self, sections: &[SectionConfig], cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let mut panel = ui.panel().overflow_hidden().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .h(px(44.))
                .px(px(ui.inset()))
                .child(self.recorder_heading().flex_1().min_w_0())
                .child(self.range_control(cx)),
        );
        for section in sections {
            panel = panel.child(self.lane(section, cx));
        }
        panel
            .child(
                div()
                    .flex()
                    .border_t_1()
                    .border_color(p.line.opacity(0.7))
                    .py(px(5.))
                    .child(div().w(px(ui.lane_label_width())).flex_shrink_0())
                    .child(self.time_axis().flex_1().min_w_0().pr(px(ui.inset()))),
            )
            .into_any_element()
    }

    pub(super) fn panel_heading(
        &self,
        title: String,
        kind: SectionKind,
        cx: &mut Context<Self>,
    ) -> Div {
        let ui = self.ui();
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(28.))
            .mb(px(6.))
            .child(ui.strong(title))
            .child(
                ui.quiet(("detail", kind as usize), "Details")
                    .mr(px(-8.))
                    .child(Icon::new(IconName::ChevronRight).size(px(12.)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.navigate(
                            if kind == SectionKind::Processes {
                                Page::Processes
                            } else {
                                Page::Resource(kind)
                            },
                            cx,
                        )
                    })),
            )
    }

    /// Fills a slot in a row or a full-width row with a section's panel.
    fn section_panel(&self, section: &SectionConfig, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let kind = section.kind;
        let mut panel = ui
            .panel()
            .flex()
            .flex_col()
            .h_full()
            .p(px(ui.inset()))
            .pt(px(ui.inset() - 6.))
            .child(self.panel_heading(section.title.clone(), kind, cx));
        match kind {
            SectionKind::Processes => {
                panel = panel.child(self.consumer_list(
                    &self.top_cpu.clone(),
                    5,
                    p.cpu,
                    |c| c.cpu / 100.,
                    |c| format!("{:.1}%", c.cpu),
                    cx,
                ));
            }
            SectionKind::Battery => {
                panel = panel.child(self.battery_summary());
            }
            _ => {
                let reading = self.reading(kind);
                panel = panel
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_2()
                            .child(
                                ui.figure(20., p.text, reading.primary)
                                    .font_weight(FontWeight::MEDIUM),
                            )
                            .child(ui.figure(12., p.muted, reading.caption)),
                    )
                    .child(
                        div()
                            .mt_2()
                            .child(self.scaled_chart(kind, section.graph_height)),
                    );
            }
        }
        self.editable(section, panel, cx)
    }

    fn battery_summary(&self) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let s = &self.snapshot;
        let mut body = div().flex().flex_col();
        if s.sequence == 0 {
            return ui.muted("Reading…").into_any_element();
        }
        if let Some(b) = &s.battery {
            let time = b.remaining_secs.map(duration);
            body = body
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_2()
                        .child(
                            ui.figure(20., p.text, format!("{:.0}%", b.percent))
                                .font_weight(FontWeight::MEDIUM),
                        )
                        .child(ui.muted(match time {
                            Some(time) => format!("{}, {time} left", b.state),
                            None => b.state.clone(),
                        })),
                )
                .child(
                    div()
                        .mt(px(8.))
                        .mb(px(6.))
                        .child(ui.meter(b.percent / 100., p.battery, 6.)),
                )
                .child(ui.fact("Battery health", format!("{:.0}%", b.health_percent)));
        } else {
            body = body.child(ui.muted("This Mac has no battery."));
        }
        body.child(ui.fact("System power", optional(s.hardware.system_watts, " W")))
            .into_any_element()
    }

    /// Adds the handles of Customize mode around a section.
    fn editable(
        &self,
        section: &SectionConfig,
        content: Div,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.editing {
            return content.into_any_element();
        }
        let ui = self.ui();
        let p = ui.p;
        let kind = section.kind;
        let drag = DragSection {
            kind,
            title: section.title.clone().into(),
            ui: ui.clone(),
        };
        let height = section.graph_height;
        let span = section.span;
        let charted = kind != SectionKind::Processes && kind != SectionKind::Battery;
        div()
            .id(("edit", kind as usize))
            .flex()
            .flex_col()
            .border_t_2()
            .border_color(gpui::transparent_black())
            .drag_over::<DragSection>(move |s, _, _, _| s.border_color(p.accent))
            .on_drop(cx.listener(move |this, drag: &DragSection, _, cx| {
                if drag.kind != kind {
                    this.preferences.remember_layout();
                    super::move_section(&mut this.preferences, drag.kind, Some(kind));
                    this.save(cx);
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px(px(ui.inset() - 6.))
                    .py(px(6.))
                    .bg(p.accent.opacity(0.08))
                    .child(
                        div()
                            .id(("drag", kind as usize))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(24.))
                            .rounded(px(5.))
                            .cursor_grab()
                            .hover(move |s| s.bg(p.hover))
                            .child(Icon::new(IconName::Menu).size(px(14.)).text_color(p.muted))
                            .tooltip(|window, cx| Tooltip::new("Drag to move").build(window, cx))
                            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone())),
                    )
                    .child(
                        div()
                            .w(px(170.))
                            .child(self.input(Edit::SectionTitle(kind))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.input(Edit::SectionDescription(kind))),
                    )
                    .child(
                        ui.segments()
                            .child(
                                ui.segment(("half", kind as usize), "Half", span == Span::Half)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_span(kind, Span::Half, cx)
                                    })),
                            )
                            .child(
                                ui.segment(("full", kind as usize), "Full", span == Span::Full)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_span(kind, Span::Full, cx)
                                    })),
                            ),
                    )
                    .child(
                        ui.icon_button(("hide", kind as usize))
                            .child(Icon::new(IconName::EyeOff).size(px(14.)))
                            .tooltip(|window, cx| Tooltip::new("Hide").build(window, cx))
                            .on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.set_visible(kind, false, cx)
                                }),
                            ),
                    ),
            )
            .child(content)
            .when(charted, |s| {
                s.child(
                    div()
                        .id(("resize", kind as usize))
                        .h(px(11.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_row_resize()
                        .bg(p.accent.opacity(0.08))
                        .hover(move |s| s.bg(p.accent.opacity(0.18)))
                        .child(div().w(px(36.)).h(px(3.)).rounded_full().bg(p.muted))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                this.preferences.remember_layout();
                                this.resize = Some(Resize {
                                    kind,
                                    y: event.position.y,
                                    height,
                                });
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
            .into_any_element()
    }

    fn set_span(&mut self, kind: SectionKind, span: Span, cx: &mut Context<Self>) {
        self.preferences.remember_layout();
        if let Some(section) = self
            .preferences
            .sections
            .iter_mut()
            .find(|s| s.kind == kind)
        {
            section.span = span;
        }
        self.save(cx);
    }

    fn set_visible(&mut self, kind: SectionKind, visible: bool, cx: &mut Context<Self>) {
        self.preferences.remember_layout();
        if let Some(section) = self
            .preferences
            .sections
            .iter_mut()
            .find(|s| s.kind == kind)
        {
            section.visible = visible;
        }
        self.save(cx);
    }

    fn customize_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let mut presets = div().flex().items_center().gap_2();
        for name in ["Balanced", "Focus", "Compact"] {
            presets = presets.child(ui.button(name, name).on_click(cx.listener(
                move |this, _, window, cx| {
                    this.preferences.apply_preset(name);
                    this.create_inputs(window, cx);
                    this.show_toast(format!("Applied {name}"), Some(super::Undo::Layout));
                    this.save(cx);
                },
            )));
        }
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div().child(ui.verdict("Arrange your overview")).child(
                    ui.text(
                        14.,
                        ui.p.muted,
                        "Drag a section by its handle. Drag a bottom edge to change its height.",
                    )
                    .mt(px(6.)),
                ),
            )
            .child(presets)
            .into_any_element()
    }

    fn hidden_tray(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ui = self.ui();
        let hidden: Vec<_> = self
            .preferences
            .sections
            .iter()
            .filter(|s| !s.visible)
            .collect();
        if hidden.is_empty() {
            return None;
        }
        let mut tray = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(ui.muted("Hidden"));
        for section in hidden {
            let kind = section.kind;
            tray = tray.child(
                ui.button(("show", kind as usize), section.title.clone())
                    .child(Icon::new(IconName::Plus).size(px(12.)))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_visible(kind, true, cx))),
            );
        }
        Some(tray.into_any_element())
    }

    pub(super) fn overview(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let sections: Vec<SectionConfig> = self
            .preferences
            .sections
            .iter()
            .filter(|s| s.visible)
            .cloned()
            .collect();
        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(GUTTER))
            .pt(px(22.))
            .pb(px(GUTTER))
            .child(if self.editing {
                self.customize_bar(cx)
            } else {
                self.verdict_view(cx)
            })
            .child(div().h(px(2.)));
        if sections.is_empty() {
            content = content.child(
                ui.panel()
                    .p_6()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_3()
                    .child(ui.strong("Nothing to show"))
                    .child(ui.muted("Every section is hidden. Add some back to see them here."))
                    .when(!self.editing, |s| {
                        s.child(
                            ui.button("empty-customize", "Customize")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.editing = true;
                                    cx.notify();
                                })),
                        )
                    }),
            );
        }
        let mut index = 0;
        while index < sections.len() {
            let section = &sections[index];
            if section.span == Span::Full && is_lane(section.kind) {
                let start = index;
                while index < sections.len()
                    && sections[index].span == Span::Full
                    && is_lane(sections[index].kind)
                {
                    index += 1;
                }
                content = content.child(self.lanes(&sections[start..index], cx));
            } else if section.span == Span::Full {
                content = content.child(self.section_panel(section, cx));
                index += 1;
            } else {
                let next = sections.get(index + 1).filter(|s| s.span == Span::Half);
                content = content.child(
                    div()
                        .flex()
                        .gap(px(14.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(self.section_panel(section, cx)),
                        )
                        .when_some(next, |row, next| {
                            row.child(div().flex_1().min_w_0().child(self.section_panel(next, cx)))
                        }),
                );
                index += if next.is_some() { 2 } else { 1 };
            }
        }
        if self.editing {
            let p = ui.p;
            content = content
                .child(
                    div()
                        .id("drop-at-end")
                        .flex()
                        .items_center()
                        .justify_center()
                        .h(px(36.))
                        .rounded(px(RADIUS_PANEL))
                        .border_1()
                        .border_color(p.line)
                        .child(ui.muted("Drop here to move a section to the end"))
                        .drag_over::<DragSection>(move |s, _, _, _| {
                            s.border_color(p.accent).bg(p.accent.opacity(0.08))
                        })
                        .on_drop(cx.listener(|this, drag: &DragSection, _, cx| {
                            this.preferences.remember_layout();
                            super::move_section(&mut this.preferences, drag.kind, None);
                            this.save(cx);
                        })),
                )
                .children(self.hidden_tray(cx));
        }
        content.into_any_element()
    }

    /// Ranks what is using a resource. The same list appears wherever the
    /// question "what is causing this?" comes up.
    pub(super) fn consumer_list(
        &self,
        indices: &[usize],
        limit: usize,
        tint: Hsla,
        fraction: impl Fn(&crate::insight::Consumer) -> f32,
        value: impl Fn(&crate::insight::Consumer) -> String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        if indices.is_empty() {
            return ui.muted("Reading processes…").into_any_element();
        }
        // Bars rank the leaders against each other; the figures carry the scale.
        let top = indices
            .first()
            .map_or(1., |&i| fraction(&self.consumers[i]))
            .max(f32::EPSILON);
        let mut list = div().flex().flex_col();
        for &index in indices.iter().take(limit) {
            let consumer = &self.consumers[index];
            let Some(process) = self.snapshot.processes.get(consumer.lead) else {
                continue;
            };
            let key = process.key;
            list = list.child(
                div()
                    .id(("consumer", key.pid as usize))
                    .tab_index(0)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .h(px(ui.row_height()))
                    .mx(px(-6.))
                    .px(px(6.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.hover))
                    .child(self.icon(consumer.bundle.as_deref(), 18.))
                    .child(
                        ui.text(12., p.text, consumer.name.clone())
                            .flex_1()
                            .min_w_0()
                            .truncate(),
                    )
                    .child(
                        div()
                            .w(px(48.))
                            .flex_shrink_0()
                            .child(ui.bar(fraction(consumer) / top, tint)),
                    )
                    .child(
                        ui.figure(12., p.text, value(consumer))
                            .w(px(64.))
                            .flex_shrink_0()
                            .text_right(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open_process(key, cx))),
            );
        }
        list.into_any_element()
    }

    pub(super) fn icon(&self, bundle: Option<&str>, size: f32) -> AnyElement {
        let p = self.ui().p;
        if let Some(icon) = bundle.and_then(|bundle| self.icons.peek(bundle)) {
            return gpui::img(icon)
                .w(px(size))
                .h(px(size))
                .flex_shrink_0()
                .into_any_element();
        }
        div()
            .size(px(size))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(size * 0.25))
            .bg(p.hover)
            .child(
                Icon::new(IconName::SquareTerminal)
                    .size(px(size * 0.66))
                    .text_color(p.muted),
            )
            .into_any_element()
    }
}

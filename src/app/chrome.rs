//! The window frame: sidebar, toolbar and toasts.

use super::{
    App, Page, TraceSeries, TraceViewport, Undo, bytes, chart::Plot, chart::nice_ceiling, duration,
    rate, ratio, series_maximum, startup_disk, style::*,
};
use crate::preferences::SectionKind;
use gpui::{
    Animation, AnimationExt, AnyElement, Context, FontWeight, Hsla, MouseButton, MouseDownEvent,
    PathBuilder, Window, canvas, div, point, prelude::*, px,
};
use gpui_component::{Icon, IconName, tooltip::Tooltip};
use std::time::Duration;

pub(super) const SIDEBAR_RESOURCES: [SectionKind; 6] = [
    SectionKind::Processor,
    SectionKind::Memory,
    SectionKind::Graphics,
    SectionKind::Network,
    SectionKind::Storage,
    SectionKind::Battery,
];

fn short_title(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Processor => "Processor",
        SectionKind::Memory => "Memory",
        SectionKind::Graphics => "Graphics",
        SectionKind::Network => "Network",
        SectionKind::Storage => "Storage",
        SectionKind::Battery => "Battery",
        SectionKind::Processes => "Processes",
    }
}

fn mark() -> AnyElement {
    static LOGO: std::sync::OnceLock<std::sync::Arc<gpui::Image>> = std::sync::OnceLock::new();
    let image = LOGO.get_or_init(|| {
        std::sync::Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            include_bytes!("../../assets/brand/sysly-icon.png").to_vec(),
        ))
    });
    gpui::img(image.clone())
        .w(px(26.))
        .h(px(26.))
        .into_any_element()
}

/// Lets empty toolbar space move the window, as a native title bar does.
fn drag_window(event: &MouseDownEvent, window: &mut Window, _: &mut gpui::App) {
    if event.click_count == 2 {
        window.titlebar_double_click();
    } else {
        window.start_window_move();
    }
}

impl App {
    pub(super) fn section_color(&self, kind: SectionKind) -> Hsla {
        let p = self.ui().p;
        match kind {
            SectionKind::Processor => p.cpu,
            SectionKind::Memory => p.memory,
            SectionKind::Graphics => p.gpu,
            SectionKind::Network => p.network,
            SectionKind::Storage => p.storage,
            SectionKind::Battery => p.battery,
            SectionKind::Processes => p.accent,
        }
    }

    fn sidebar_reading(&self, kind: SectionKind) -> String {
        let s = &self.snapshot;
        if s.sequence == 0 {
            return String::new();
        }
        match kind {
            SectionKind::Processor => format!("{:.0}%", s.cpu_percent),
            SectionKind::Memory => format!("{:.0}%", ratio(s.memory.used, s.memory.total)),
            SectionKind::Graphics => s
                .hardware
                .gpu_percent
                .map(|v| format!("{v:.0}%"))
                .unwrap_or_else(|| "–".into()),
            SectionKind::Network => s
                .networks
                .iter()
                .find(|n| n.name == self.interface)
                .map(|n| rate(n.received_per_sec + n.sent_per_sec))
                .unwrap_or_else(|| "–".into()),
            SectionKind::Storage => startup_disk(s)
                .map(|d| {
                    format!(
                        "{:.0}%",
                        ratio(d.total.saturating_sub(d.available), d.total)
                    )
                })
                .unwrap_or_else(|| "–".into()),
            SectionKind::Battery => s
                .battery
                .as_ref()
                .map(|b| format!("{:.0}%", b.percent))
                .unwrap_or_else(|| "–".into()),
            SectionKind::Processes => s.processes.len().to_string(),
        }
    }

    fn sparkline(&self, kind: SectionKind) -> Option<AnyElement> {
        let viewport = TraceViewport {
            end: self.graph_viewport.end,
            span: self.graph_viewport.span.min(120.),
        };
        let (series, maximum) = match kind {
            SectionKind::Processor => (TraceSeries::Cpu, 100.),
            SectionKind::Memory => (TraceSeries::Memory, 100.),
            SectionKind::Graphics => (TraceSeries::Gpu, 100.),
            SectionKind::Network => (
                TraceSeries::Received,
                nice_ceiling(series_maximum(
                    &self.history,
                    viewport,
                    &[TraceSeries::Received],
                    1024.,
                )) as f32,
            ),
            SectionKind::Storage => (
                TraceSeries::DiskWrite,
                nice_ceiling(series_maximum(
                    &self.history,
                    viewport,
                    &[TraceSeries::DiskWrite],
                    1024. * 1024.,
                )) as f32,
            ),
            SectionKind::Battery => (
                TraceSeries::Power,
                nice_ceiling(series_maximum(
                    &self.history,
                    viewport,
                    &[TraceSeries::Power],
                    5.,
                )) as f32,
            ),
            SectionKind::Processes => return None,
        };
        let ui = self.ui();
        Some(
            Plot {
                history: self.history.clone(),
                viewport,
                series,
                mirror: None,
                maximum,
                color: self.section_color(kind),
                divisions: 1,
                cursor: None,
                registry: None,
                stroke: 1.25,
                decorated: false,
            }
            .element(&ui, 16.),
        )
    }

    fn nav_item(
        &self,
        page: Page,
        title: &'static str,
        icon: IconName,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ui = self.ui();
        let p = ui.p;
        let selected = self.page == page;
        div()
            .id(title)
            .tab_index(0)
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(30.))
            .px(px(8.))
            .rounded(px(6.))
            .cursor_pointer()
            .border_1()
            .border_color(gpui::transparent_black())
            .focus(move |s| s.border_color(p.accent))
            .text_size(ui.fs(13.))
            .text_color(if selected { p.text } else { p.muted })
            .when(selected, |s| s.bg(p.active).font_weight(FontWeight::MEDIUM))
            .when(!selected, |s| {
                s.hover(move |s| s.bg(p.hover).text_color(p.text))
            })
            .child(Icon::new(icon).size(px(15.)))
            .child(title)
            .on_click(cx.listener(move |this, _, _, cx| this.navigate(page, cx)))
    }

    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let mut resources = div().flex().flex_col().gap(px(1.));
        for kind in SIDEBAR_RESOURCES {
            let selected = self.page == Page::Resource(kind);
            resources = resources.child(
                div()
                    .id(("resource", kind as usize))
                    .tab_index(0)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .h(px(30.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .focus(move |s| s.border_color(p.accent))
                    .text_color(if selected { p.text } else { p.muted })
                    .when(selected, |s| s.bg(p.active))
                    .when(!selected, |s| {
                        s.hover(move |s| s.bg(p.hover).text_color(p.text))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(ui.fs(12.))
                            .child(short_title(kind)),
                    )
                    .when(ui.roomy_sidebar(), |s| {
                        s.child(
                            div()
                                .w(px(38.))
                                .flex_shrink_0()
                                .children(self.sparkline(kind)),
                        )
                    })
                    .child(
                        ui.figure(11., p.text, self.sidebar_reading(kind))
                            .w(px(ui.scaled(52.)))
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .text_right(),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.navigate(Page::Resource(kind), cx)),
                    ),
            );
        }
        let s = &self.snapshot;
        div()
            .w(px(ui.sidebar_width()))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(p.sidebar.opacity(0.72))
            .border_r_1()
            .border_color(p.line)
            .child(
                div()
                    .h(px(TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .on_mouse_down(MouseButton::Left, drag_window),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px(px(10.))
                    .gap(px(1.))
                    .child(self.nav_item(Page::Overview, "Overview", IconName::LayoutDashboard, cx))
                    .child(self.nav_item(
                        Page::Processes,
                        "Apps & processes",
                        IconName::SquareTerminal,
                        cx,
                    ))
                    .child(
                        ui.text(11., p.muted, "Resources")
                            .font_weight(FontWeight::MEDIUM)
                            .px(px(9.))
                            .pt(px(18.))
                            .pb(px(5.)),
                    )
                    .child(resources)
                    .child(div().flex_1())
                    .child(self.nav_item(Page::Appearance, "Settings", IconName::Settings, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .mx(px(10.))
                    .mt(px(8.))
                    .px(px(8.))
                    .py(px(11.))
                    .border_t_1()
                    .border_color(p.line)
                    .child(mark())
                    .child(
                        div()
                            .min_w_0()
                            .child(ui.text(
                                12.,
                                p.text,
                                if s.cpu_name.is_empty() {
                                    "Reading your Mac…".to_string()
                                } else {
                                    s.cpu_name.clone()
                                },
                            ))
                            .when(s.sequence > 0, |d| {
                                d.child(
                                    ui.text(
                                        11.,
                                        p.muted,
                                        format!(
                                            "{}, up {}",
                                            bytes(s.memory.total),
                                            duration(s.uptime_secs)
                                        ),
                                    )
                                    .truncate(),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn page_title(&self) -> (String, Option<String>) {
        match self.page {
            Page::Overview => (self.preferences.dashboard_title.clone(), None),
            Page::Processes => ("Apps & processes".into(), None),
            Page::Appearance => ("Settings".into(), None),
            Page::Resource(kind) => {
                let section = self.preferences.sections.iter().find(|s| s.kind == kind);
                (
                    section
                        .map(|s| s.title.clone())
                        .unwrap_or_else(|| kind.title().into()),
                    section.map(|s| s.description.clone()),
                )
            }
        }
    }

    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let (title, subtitle) = self.page_title();
        let ready = self.snapshot.sequence > 0;
        let status = if self.paused {
            "Paused".to_string()
        } else if ready {
            format!("Live, every {} s", self.preferences.refresh_secs)
        } else {
            "Connecting".to_string()
        };
        let paused = self.paused;
        div()
            .h(px(TOOLBAR_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_3()
            .px(px(GUTTER))
            .border_b_1()
            .border_color(p.line)
            .on_mouse_down(MouseButton::Left, drag_window)
            .child(
                ui.text(14., p.text, title)
                    .font_weight(FontWeight::SEMIBOLD)
                    .flex_shrink_0(),
            )
            .children(subtitle.map(|s| ui.muted(s).truncate().min_w_0()))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(6.))
                    .child(div().size(px(6.)).rounded_full().bg(if paused {
                        p.warning
                    } else if ready {
                        p.memory
                    } else {
                        p.muted
                    }))
                    .child(ui.muted(status)),
            )
            .child(
                ui.icon_button("pause")
                    .child(playback_icon(paused, p.muted))
                    .tooltip(move |window, cx| {
                        Tooltip::new(if paused {
                            "Resume sampling  ⌘P"
                        } else {
                            "Pause sampling  ⌘P"
                        })
                        .build(window, cx)
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_pause(cx))),
            )
            .when(self.page == Page::Overview, |s| {
                s.child(
                    if self.editing {
                        ui.primary("customize", "Done")
                    } else {
                        ui.button("customize", "Customize")
                    }
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.editing = !this.editing;
                        this.cursor = None;
                        cx.notify();
                    })),
                )
            })
            .into_any_element()
    }

    pub(super) fn toast_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let Some(toast) = &self.toast else {
            return div().into_any_element();
        };
        let undo = toast.undo;
        let error = toast.error;
        let frame = div()
            .absolute()
            .bottom(px(20.))
            .left(px(ui.sidebar_width()))
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("toast")
                    .occlude()
                    .flex()
                    .items_center()
                    .gap_3()
                    .max_w(px(560.))
                    .pl(px(14.))
                    .pr(px(6.))
                    .py(px(6.))
                    .min_h(px(38.))
                    .rounded(px(10.))
                    .bg(p.raised)
                    .border_1()
                    .border_color(if error { p.danger } else { p.line })
                    .shadow_lg()
                    .when(error, |s| {
                        s.child(
                            Icon::new(IconName::TriangleAlert)
                                .size(px(14.))
                                .text_color(p.danger),
                        )
                    })
                    .child(ui.text(12., p.text, toast.message.clone()).flex_1())
                    .children(undo.map(|undo: Undo| {
                        ui.quiet("toast-undo", "Undo")
                            .text_color(p.accent)
                            .on_click(
                                cx.listener(move |this, _, window, cx| this.undo(undo, window, cx)),
                            )
                    }))
                    .child(
                        ui.icon_button("dismiss-toast")
                            .child(Icon::new(IconName::Close).size(px(13.)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toast = None;
                                cx.notify();
                            })),
                    ),
            );
        if self.reduce_motion {
            return frame.into_any_element();
        }
        frame
            .with_animation(
                "toast-in",
                Animation::new(Duration::from_millis(180))
                    .with_easing(|value| 1. - (1. - value).powi(3)),
                |element, progress| element.opacity(progress).bottom(px(12. + 8. * progress)),
            )
            .into_any_element()
    }
}

fn playback_icon(paused: bool, c: Hsla) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let x = bounds.left();
            let y = bounds.top();
            if paused {
                let mut path = PathBuilder::fill();
                path.move_to(point(x + px(2.5), y + px(1.)));
                path.line_to(point(x + px(10.5), y + px(6.)));
                path.line_to(point(x + px(2.5), y + px(11.)));
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, c);
                }
            } else {
                let mut path = PathBuilder::stroke(px(2.));
                for offset in [3.5, 8.5] {
                    path.move_to(point(x + px(offset), y + px(1.5)));
                    path.line_to(point(x + px(offset), y + px(10.5)));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, c);
                }
            }
        },
    )
    .size(px(12.))
    .into_any_element()
}

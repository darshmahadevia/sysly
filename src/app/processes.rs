//! Apps & processes: a dense table with a live inspector beside it.

use super::{
    App, Edit, ProcessView, Row, Sort, bytes, chart::sparkline, duration, overview::plural, rate,
    style::*,
};
use crate::{
    insight,
    model::{Process, ProcessAction, ProcessKey},
};
use gpui::{
    Animation, AnimationExt, AnyElement, ClipboardItem, Context, Div, FontWeight, SharedString,
    div, prelude::*, px, uniform_list,
};
use gpui_component::{Icon, IconName, tooltip::Tooltip};
use std::{path::PathBuf, time::Duration};

const COLUMN_CPU: f32 = 116.;
const COLUMN_MEMORY: f32 = 84.;
const COLUMN_DISK: f32 = 88.;
const COLUMN_PID: f32 = 62.;
const INSPECTOR_WIDTH: f32 = 332.;
/// The space the name column keeps before other columns start to give way.
const NAME_MINIMUM: f32 = 210.;

/// Which columns fit. They drop in reverse order of how much they help answer
/// "what is using my Mac?"
#[derive(Clone, Copy)]
struct Columns {
    pid: bool,
    disk: bool,
    memory: bool,
    bar: bool,
}

fn is_stopped(process: &Process) -> bool {
    process.status.to_lowercase().starts_with("stop")
}

/// macOS scheduler states, in words a person would use.
fn status_label(status: &str, stopped: bool) -> String {
    if stopped {
        return "Paused".into();
    }
    match status {
        "Runnable" | "Run" => "Running".into(),
        "Sleeping" | "Sleep" | "Idle" => "Idle".into(),
        "Zombie" => "Ended, awaiting cleanup".into(),
        other => other.into(),
    }
}

impl App {
    fn process_stopped(&self, process: &Process) -> bool {
        match self.signaled_stop {
            Some((key, stopped, until))
                if key == process.key && std::time::Instant::now() < until =>
            {
                stopped
            }
            _ => is_stopped(process),
        }
    }

    /// The inspector never takes more than about half of the content area, so the
    /// table stays usable beside it in a small window.
    fn inspector_width(&self, window: &gpui::Window) -> f32 {
        let ui = self.ui();
        let content = f32::from(window.viewport_size().width) - ui.sidebar_width();
        ui.scaled(INSPECTOR_WIDTH).min(content * 0.45)
    }

    fn columns(&self, window: &gpui::Window) -> Columns {
        let ui = self.ui();
        let width = f32::from(window.viewport_size().width);
        let inspector = if self.selection.is_some() {
            self.inspector_width(window)
        } else {
            0.
        };
        let mut room = width - ui.sidebar_width() - inspector - GUTTER * 2. - NAME_MINIMUM;
        room -= ui.scaled(COLUMN_CPU - 52.);
        let mut fits = |column: f32| {
            let column = ui.scaled(column);
            let fits = room >= column;
            if fits {
                room -= column;
            }
            fits
        };
        let memory = fits(COLUMN_MEMORY);
        let disk = fits(COLUMN_DISK);
        let pid = self.view != ProcessView::Apps && fits(COLUMN_PID);
        let bar = fits(52.);
        Columns {
            pid,
            disk,
            memory,
            bar,
        }
    }

    fn column(
        &self,
        sort: Sort,
        name: &'static str,
        width: Option<f32>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ui = self.ui();
        let p = ui.p;
        let active = self.sort == sort;
        div()
            .id(name)
            .tab_index(0)
            .flex()
            .items_center()
            .gap_1()
            .h_full()
            .cursor_pointer()
            .text_size(ui.fs(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(if active { p.text } else { p.muted })
            .hover(move |s| s.text_color(p.text))
            .map(|s| match width {
                Some(width) => s.w(px(ui.scaled(width))).flex_shrink_0().justify_end(),
                None => s.flex_1().min_w_0(),
            })
            .child(name)
            .when(active, |s| {
                s.child(
                    Icon::new(if self.descending {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronUp
                    })
                    .size(px(12.)),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.sort == sort {
                    this.descending = !this.descending;
                } else {
                    this.sort = sort;
                    this.descending = sort != Sort::Name;
                }
                this.update_process_rows(cx);
                cx.notify();
            }))
    }

    fn view_control(&self, cx: &mut Context<Self>) -> Div {
        let ui = self.ui();
        let mut control = ui.segments();
        for (view, name) in [
            (ProcessView::Apps, "By app"),
            (ProcessView::All, "All"),
            (ProcessView::Tree, "Tree"),
        ] {
            control = control.child(ui.segment(name, name, self.view == view).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.view = view;
                    this.update_process_rows(cx);
                    cx.notify();
                }),
            ));
        }
        control
    }

    fn row(&self, index: usize, row: Row, columns: Columns, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let process = &self.snapshot.processes[row.process];
        let consumer = row.consumer.map(|c| &self.consumers[c]);
        let selected = self.selected_row() == Some(index);
        let (name, cpu, memory, io) = match consumer {
            Some(c) => (c.name.clone(), c.cpu, c.memory, c.io),
            None => (
                process.name.clone(),
                process.cpu_percent,
                process.memory,
                process.read_per_sec + process.write_per_sec,
            ),
        };
        let bundle = consumer
            .and_then(|c| c.bundle.as_deref())
            .or_else(|| insight::app_bundle(&process.command));
        let helpers = consumer.map_or(1, |c| c.count);
        div()
            .id(("process", index))
            .w_full()
            .flex()
            .items_center()
            .h(px(ui.row_height()))
            .px(px(10.))
            .rounded(px(6.))
            .cursor_pointer()
            .when(selected, |s| s.bg(p.accent.opacity(0.18)))
            .when(!selected, |s| s.hover(move |s| s.bg(p.hover)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pl(px(row.depth as f32 * 14.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(self.icon(bundle, 18.))
                    .child(ui.text(13., p.text, name).truncate())
                    .when(helpers > 1, |s| {
                        s.child(ui.figure(11., p.muted, helpers.to_string()).flex_shrink_0())
                    })
                    .when(self.process_stopped(process), |s| {
                        s.child(ui.text(11., p.warning, "Paused").flex_shrink_0())
                    }),
            )
            .when(columns.pid, |s| {
                s.child(
                    ui.figure(12., p.muted, process.key.pid.to_string())
                        .w(px(ui.scaled(COLUMN_PID)))
                        .flex_shrink_0()
                        .text_right(),
                )
            })
            .child(
                div()
                    .w(px(ui.scaled(if columns.bar {
                        COLUMN_CPU
                    } else {
                        COLUMN_CPU - 52.
                    })))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .when(columns.bar, |s| {
                        s.child(div().w(px(44.)).child(ui.bar(cpu / 100., p.cpu)))
                    })
                    .child(
                        ui.figure(12., p.text, format!("{cpu:.1}%"))
                            .whitespace_nowrap()
                            .text_right(),
                    ),
            )
            .when(columns.memory, |s| {
                s.child(
                    ui.figure(12., p.text, bytes(memory))
                        .w(px(ui.scaled(COLUMN_MEMORY)))
                        .flex_shrink_0()
                        .text_right(),
                )
            })
            .when(columns.disk, |s| {
                s.child(
                    ui.figure(12., p.muted, rate(io))
                        .w(px(ui.scaled(COLUMN_DISK)))
                        .flex_shrink_0()
                        .text_right(),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(&row) = this.process_rows.get(index) {
                    this.select_row(row, cx);
                }
            }))
            .into_any_element()
    }

    pub(super) fn processes(&self, window: &gpui::Window, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let query = self.query(cx);
        let columns = self.columns(window);
        let wide = columns.memory && columns.disk;
        let table = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px(px(GUTTER))
                    .pt(px(14.))
                    .pb(px(10.))
                    .child(
                        div().w(px(280.)).min_w(px(96.)).flex_shrink().child(
                            self.input(Edit::Search)
                                .prefix(Icon::new(IconName::Search).size(px(14.)))
                                .cleanable(true),
                        ),
                    )
                    .child(self.view_control(cx))
                    .child(div().flex_1())
                    .when(wide, |s| {
                        s.child(
                            ui.figure(
                                12.,
                                p.muted,
                                match self.view {
                                    ProcessView::Apps => {
                                        format!("{} apps and processes", self.process_rows.len())
                                    }
                                    _ => plural(self.process_rows.len(), "process"),
                                },
                            )
                            .flex_shrink_0(),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .h(px(28.))
                    .mx(px(GUTTER - 10.))
                    .px(px(10.))
                    .border_b_1()
                    .border_color(p.line)
                    .child(self.column(Sort::Name, "Name", None, cx))
                    .when(columns.pid, |s| {
                        s.child(self.column(Sort::Pid, "PID", Some(COLUMN_PID), cx))
                    })
                    .child(self.column(
                        Sort::Cpu,
                        "CPU",
                        Some(if columns.bar {
                            COLUMN_CPU
                        } else {
                            COLUMN_CPU - 52.
                        }),
                        cx,
                    ))
                    .when(columns.memory, |s| {
                        s.child(self.column(Sort::Memory, "Memory", Some(COLUMN_MEMORY), cx))
                    })
                    .when(columns.disk, |s| {
                        s.child(self.column(Sort::Io, "Disk", Some(COLUMN_DISK), cx))
                    }),
            )
            .when(self.process_rows.is_empty(), |s| {
                s.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .pt(px(72.))
                        .child(ui.muted(if self.snapshot.sequence == 0 {
                            "Reading processes…".to_string()
                        } else {
                            format!("Nothing matches “{query}”")
                        }))
                        .when(!query.is_empty(), |s| {
                            s.child(ui.button("clear-search", "Clear search").on_click(
                                cx.listener(|this, _, window, cx| {
                                    let input = this.inputs[&Edit::Search].clone();
                                    input.update(cx, |input, cx| input.set_value("", window, cx));
                                    this.update_process_rows(cx);
                                    cx.notify();
                                }),
                            ))
                        }),
                )
            })
            .child(
                uniform_list(
                    "process-list",
                    self.process_rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| this.row(index, this.process_rows[index], columns, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.list_scroll.clone())
                .flex_1()
                .min_h_0()
                .mx(px(GUTTER - 10.))
                .pt(px(4.)),
            );
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(table)
            .when(self.selection.is_some(), |s| {
                let pane = self.inspector(window, cx);
                s.child(if self.reduce_motion {
                    pane
                } else {
                    div()
                        .h_full()
                        .flex_shrink_0()
                        .child(pane)
                        .with_animation(
                            "inspector-open",
                            Animation::new(Duration::from_millis(150))
                                .with_easing(|value| 1. - (1. - value).powi(3)),
                            |element, progress| element.opacity(0.4 + 0.6 * progress),
                        )
                        .into_any_element()
                })
            })
            .into_any_element()
    }

    fn inspector(&self, window: &gpui::Window, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.ui();
        let p = ui.p;
        let Some(selection) = self.selection.clone() else {
            return div().into_any_element();
        };
        let process = self
            .snapshot
            .processes
            .iter()
            .find(|p| p.key == selection.key);
        let close = ui
            .icon_button("close-inspector")
            .child(Icon::new(IconName::Close).size(px(14.)))
            .tooltip(|window, cx| Tooltip::new("Close  esc").build(window, cx))
            .on_click(cx.listener(|this, _, _, cx| {
                this.selection = None;
                this.needs_focus = true;
                cx.notify();
            }));
        let pane = div()
            .w(px(self.inspector_width(window)))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(p.surface)
            .border_l_1()
            .border_color(p.line);
        let Some(process) = process else {
            return pane
                .p(px(16.))
                .gap_3()
                .child(div().flex().justify_end().child(close))
                .child(ui.strong("This process has ended"))
                .child(ui.muted("Its process ID may now belong to something else."))
                .into_any_element();
        };
        let consumer = self.selected_consumer();
        let name = consumer.map_or(process.name.clone(), |c| c.name.clone());
        let (cpu, memory, io) = match consumer {
            Some(c) => (c.cpu, c.memory, c.io),
            None => (
                process.cpu_percent,
                process.memory,
                process.read_per_sec + process.write_per_sec,
            ),
        };
        let helpers = consumer.map_or(1, |c| c.count);
        let bundle = selection
            .bundle
            .clone()
            .or_else(|| insight::app_bundle(&process.command).map(str::to_string));
        let cpu_trace: Vec<f32> = self.selection_history.iter().map(|h| h.0).collect();
        let memory_trace: Vec<f32> = self.selection_history.iter().map(|h| h.1).collect();
        let cpu_top = cpu_trace.iter().copied().fold(100f32, f32::max);
        let memory_top = memory_trace.iter().copied().fold(1f32, f32::max) * 1.2;
        let mut details = div()
            .flex()
            .flex_col()
            .px(px(16.))
            .pb(px(16.))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(self.reading_tile(
                        "CPU",
                        format!("{cpu:.1}%"),
                        sparkline(cpu_trace, cpu_top, p.cpu),
                    ))
                    .child(self.reading_tile(
                        "Memory",
                        bytes(memory),
                        sparkline(memory_trace, memory_top, p.memory),
                    )),
            )
            .child(div().h(px(8.)));
        if helpers > 1 {
            details = details.child(ui.fact("Processes", helpers.to_string()));
        }
        details = details
            .child(ui.fact("Disk activity", rate(io)))
            .child(ui.fact(
                "Status",
                status_label(&process.status, self.process_stopped(process)),
            ))
            .child(ui.fact("Running for", duration(process.run_secs)))
            .child(ui.fact("User", process.user.clone()))
            .child(ui.fact("Process ID", process.key.pid.to_string()))
            .child(ui.fact(
                "Parent process",
                process.parent.map_or("None".into(), |pid| pid.to_string()),
            ))
            .child(ui.fact("Virtual memory", bytes(process.virtual_memory)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mt(px(12.))
                    .mb(px(4.))
                    .child(ui.muted("Command"))
                    .child(
                        ui.icon_button("copy-command")
                            .child(Icon::new(IconName::Copy).size(px(13.)))
                            .tooltip(|window, cx| Tooltip::new("Copy command").build(window, cx))
                            .on_click(cx.listener({
                                let key = selection.key;
                                move |this, _, _, cx| {
                                    if let Some(p) =
                                        this.snapshot.processes.iter().find(|p| p.key == key)
                                    {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            p.command.clone(),
                                        ));
                                        this.show_toast("Copied command", None);
                                        cx.notify();
                                    }
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .p(px(10.))
                    .rounded(px(RADIUS_CONTROL))
                    .bg(p.bg)
                    .font_family("Menlo")
                    .text_size(ui.fs(11.))
                    .text_color(p.muted)
                    .child(if process.command.is_empty() {
                        SharedString::from("Not available for this process")
                    } else {
                        process.command.clone().into()
                    }),
            );
        let subtitle = if helpers > 1 {
            format!("App with {}", plural(helpers - 1, "helper"))
        } else {
            format!("Process {}", process.key.pid)
        };
        let key = selection.key;
        let stopped = self.process_stopped(process);
        pane.child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(16.))
                .pt(px(14.))
                .pb(px(12.))
                .child(self.icon(bundle.as_deref(), 34.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            ui.text(15., p.text, name.clone())
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate(),
                        )
                        .child(ui.text(12., p.muted, subtitle)),
                )
                .child(close),
        )
        .child(
            div()
                .id("inspector-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(details),
        )
        .child(self.inspector_actions(key, name, stopped, bundle, cx))
        .into_any_element()
    }

    fn reading_tile(&self, name: &'static str, value: String, trace: AnyElement) -> Div {
        let ui = self.ui();
        let p = ui.p;
        div()
            .flex_1()
            .min_w_0()
            .child(ui.muted(name))
            .child(
                ui.figure(17., p.text, value)
                    .font_weight(FontWeight::MEDIUM)
                    .truncate(),
            )
            .child(div().mt(px(4.)).child(trace))
    }

    fn inspector_actions(
        &self,
        key: ProcessKey,
        name: String,
        stopped: bool,
        bundle: Option<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let ui = self.ui();
        let p = ui.p;
        let signal = move |this: &mut Self,
                           action: ProcessAction,
                           name: String,
                           window: &mut gpui::Window,
                           cx: &mut Context<Self>| {
            this.confirm_signal(key, name, action, window, cx);
        };
        let (quit, kill, pause) = (name.clone(), name.clone(), name);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_shrink_0()
            .p(px(16.))
            .border_t_1()
            .border_color(p.line)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        ui.button("quit-process", "Quit")
                            .flex_1()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                signal(this, ProcessAction::Terminate, quit.clone(), window, cx)
                            })),
                    )
                    .child(
                        ui.danger("force-quit-process", "Force Quit")
                            .flex_1()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                signal(this, ProcessAction::Kill, kill.clone(), window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        ui.quiet("pause-process", if stopped { "Resume" } else { "Pause" })
                            .flex_1()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                signal(
                                    this,
                                    if stopped {
                                        ProcessAction::Continue
                                    } else {
                                        ProcessAction::Stop
                                    },
                                    pause.clone(),
                                    window,
                                    cx,
                                )
                            })),
                    )
                    .when_some(bundle, |s, bundle| {
                        s.child(ui.quiet("reveal-app", "Show in Finder").flex_1().on_click(
                            move |_, _, cx| {
                                cx.reveal_path(&PathBuf::from(&bundle));
                            },
                        ))
                    }),
            )
    }
}

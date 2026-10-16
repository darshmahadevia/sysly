use crate::{
    model::{Process, ProcessAction, ProcessKey, Snapshot},
    monitor::Monitor,
    preferences::{
        self, Density, FontChoice, Preferences, SectionConfig, SectionKind, Span, Store, Theme,
    },
    tray::{Tray, TrayAction},
};
use gpui::{
    Animation, AnimationExt, AnyElement, Bounds, ClipboardItem, Context, Div, ElementId, Entity,
    FocusHandle, FontWeight, Hsla, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    PathBuilder, Pixels, SharedString, Stateful, Subscription, Timer, Window, actions, black,
    canvas, div, point, prelude::*, px, relative, rgb, uniform_list,
};
use gpui_component::{
    Icon, IconName, Sizable,
    input::{Input, InputEvent, InputState},
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

actions!(
    sysly,
    [Overview, Processes, Appearance, Search, Escape, Quit]
);
pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        KeyBinding::new("cmd-1", Overview, None),
        KeyBinding::new("cmd-2", Processes, None),
        KeyBinding::new("cmd-,", Appearance, None),
        KeyBinding::new("cmd-f", Search, None),
        KeyBinding::new("escape", Escape, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Overview,
    Processes,
    Appearance,
    Resource(SectionKind),
}
#[derive(Clone)]
enum Overlay {
    Inspect(ProcessKey),
    Confirm(ProcessKey, ProcessAction),
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Edit {
    Search,
    Title,
    Subtitle,
    LayoutName,
    ThemeName,
    Color(usize),
    SectionTitle(SectionKind),
    SectionDescription(SectionKind),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Cpu,
    Memory,
    Name,
    Pid,
    Io,
}
#[derive(Clone)]
struct History {
    time: f64,
    cpu: f32,
    memory: f32,
    gpu: Option<f32>,
    received: Option<f64>,
    sent: Option<f64>,
    cores: Vec<f32>,
    swap: f32,
    break_before: bool,
}
#[derive(Clone, Copy, Debug)]
struct TraceViewport {
    end: f64,
    span: f64,
}

struct GraphTimeline {
    from: f64,
    to: f64,
    started: Instant,
    duration: Duration,
}
impl GraphTimeline {
    fn new(now: Instant) -> Self {
        Self {
            from: 0.,
            to: 0.,
            started: now,
            duration: Duration::ZERO,
        }
    }
    fn end_at(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            return self.to;
        }
        let progress = (now.saturating_duration_since(self.started).as_secs_f64()
            / self.duration.as_secs_f64())
        .min(1.);
        self.from + (self.to - self.from) * progress
    }
    fn advance_to(&mut self, time: f64, interval: f64, now: Instant) {
        self.from = self.end_at(now);
        self.to = time;
        self.started = now;
        self.duration = Duration::from_secs_f64(interval.max(0.001));
    }
    fn snap(&mut self, time: f64, now: Instant) {
        self.from = time;
        self.to = time;
        self.started = now;
        self.duration = Duration::ZERO;
    }
    fn freeze(&mut self, now: Instant) {
        self.snap(self.end_at(now), now);
    }
    fn is_moving(&self, now: Instant) -> bool {
        self.from != self.to && now.saturating_duration_since(self.started) < self.duration
    }
}

#[derive(Clone)]
struct DragSection {
    kind: SectionKind,
}
impl Render for DragSection {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_4()
            .py_3()
            .rounded_lg()
            .bg(rgb(0x303b49))
            .text_color(rgb(0xffffff))
            .child(self.kind.title())
    }
}
struct Resize {
    kind: SectionKind,
    y: Pixels,
    height: f32,
}
pub struct App {
    monitor: Monitor,
    tray: Option<Tray>,
    snapshot: Arc<Snapshot>,
    history: VecDeque<History>,
    graph_timeline: GraphTimeline,
    graph_viewport: TraceViewport,
    preferences: Preferences,
    store: Store,
    page: Page,
    overlay: Option<Overlay>,
    inputs: HashMap<Edit, Entity<InputState>>,
    subscriptions: Vec<Subscription>,
    paused: bool,
    hidden: bool,
    history_gap: bool,
    notice: Option<String>,
    sort: Sort,
    descending: bool,
    tree: bool,
    process_rows: Vec<(usize, usize)>,
    focus: FocusHandle,
    resize: Option<Resize>,
    font_family: SharedString,
    customize_tab: usize,
    needs_focus: bool,
    transition: u64,
    interface: String,
    save_due: Option<Instant>,
}
impl App {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = Store::new();
        let (preferences, notice) = store.load();
        let monitor = Monitor::start();
        monitor.set_interval(preferences.refresh_secs);
        let tray = Tray::new(monitor.snapshot_reader()).ok();
        let mut this = Self {
            monitor,
            tray,
            snapshot: Arc::default(),
            history: VecDeque::with_capacity(902),
            graph_timeline: GraphTimeline::new(Instant::now()),
            graph_viewport: TraceViewport {
                end: 0.,
                span: preferences.history_secs as f64,
            },
            preferences,
            store,
            page: Page::Overview,
            overlay: None,
            inputs: HashMap::new(),
            subscriptions: vec![],
            paused: false,
            hidden: false,
            history_gap: false,
            notice,
            sort: Sort::Cpu,
            descending: true,
            tree: false,
            process_rows: vec![],
            focus: cx.focus_handle(),
            resize: None,
            font_family: ".AppleSystemUIFont".into(),
            customize_tab: 0,
            needs_focus: true,
            transition: 0,
            interface: String::new(),
            save_due: None,
        };
        this.create_inputs(window, cx);
        this.load_font(cx);
        this.sync_control_theme(cx);
        window.focus(&this.focus);
        let entity = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            entity
                .update(cx, |this, cx| {
                    if this.tray.is_some() {
                        this.hidden = true;
                        this.monitor.set_visible(false);
                        cx.hide();
                        false
                    } else {
                        cx.quit();
                        true
                    }
                })
                .unwrap_or(true)
        });
        cx.spawn_in(window, async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(200)).await;
                if this
                    .update_in(cx, |this, window, cx| this.poll(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        this
    }
    fn create_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inputs.clear();
        self.subscriptions.clear();
        let mut fields = vec![
            (Edit::Search, String::new(), "Search name, PID or user"),
            (
                Edit::Title,
                self.preferences.dashboard_title.clone(),
                "Dashboard title",
            ),
            (
                Edit::Subtitle,
                self.preferences.dashboard_subtitle.clone(),
                "Dashboard description",
            ),
            (Edit::LayoutName, String::new(), "Name this layout"),
            (
                Edit::ThemeName,
                self.preferences.theme.name.clone(),
                "Theme name",
            ),
        ];
        for (i, (_, value)) in theme_colors(&self.preferences.theme).iter().enumerate() {
            fields.push((
                Edit::Color(i),
                format!("#{:02X}{:02X}{:02X}", value[0], value[1], value[2]),
                "#RRGGBB",
            ));
        }
        for section in &self.preferences.sections {
            fields.push((
                Edit::SectionTitle(section.kind),
                section.title.clone(),
                "Section title",
            ));
            fields.push((
                Edit::SectionDescription(section.kind),
                section.description.clone(),
                "Description",
            ));
        }
        for (key, value, placeholder) in fields {
            let state = cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder(placeholder);
                state.set_value(value, window, cx);
                state
            });
            self.subscriptions.push(cx.subscribe_in(
                &state,
                window,
                move |this, input, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.edit(key, input.read(cx).value().to_string(), cx);
                    }
                },
            ));
            self.inputs.insert(key, state);
        }
    }
    fn edit(&mut self, key: Edit, value: String, cx: &mut Context<Self>) {
        match key {
            Edit::Search => {
                self.update_process_rows(cx);
                cx.notify();
                return;
            }
            Edit::LayoutName => return,
            Edit::Title => self.preferences.dashboard_title = value,
            Edit::Subtitle => self.preferences.dashboard_subtitle = value,
            Edit::ThemeName => self.preferences.theme.name = value,
            Edit::Color(index) => {
                if let Some(color) = parse_color(&value) {
                    set_theme_color(&mut self.preferences.theme, index, color);
                } else {
                    return;
                }
            }
            Edit::SectionTitle(kind) => {
                if let Some(section) = self
                    .preferences
                    .sections
                    .iter_mut()
                    .find(|s| s.kind == kind)
                {
                    section.title = value;
                }
            }
            Edit::SectionDescription(kind) => {
                if let Some(section) = self
                    .preferences
                    .sections
                    .iter_mut()
                    .find(|s| s.kind == kind)
                {
                    section.description = value;
                }
            }
        }
        self.save(cx);
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        self.save_due = Some(Instant::now() + Duration::from_millis(400));
        self.monitor.set_interval(self.preferences.refresh_secs);
        self.trim_history();
        self.sync_control_theme(cx);
        self.update_tray();
        cx.notify();
    }
    fn sync_control_theme(&self, cx: &mut Context<Self>) {
        let theme = &self.preferences.theme;
        let ui = gpui_component::Theme::global_mut(cx);
        ui.colors.background = color(theme.surface);
        ui.colors.foreground = color(theme.text);
        ui.colors.input = color(theme.divider);
        ui.colors.border = color(theme.divider);
        ui.colors.muted = color(theme.background);
        ui.colors.muted_foreground = color(theme.muted);
        ui.colors.accent = color(theme.accent);
        ui.colors.accent_foreground = color(theme.text);
        ui.colors.primary = color(theme.accent);
        ui.colors.ring = color(theme.accent);
        ui.font_family = self.font_family.clone();
        ui.font_size = px(theme.font_size);
    }
    fn load_font(&mut self, cx: &mut Context<Self>) {
        self.font_family = if self.preferences.theme.font == FontChoice::Monospace {
            "Menlo".into()
        } else {
            ".AppleSystemUIFont".into()
        };
        if let Some(path) = &self.preferences.custom_font_path {
            let result = std::fs::metadata(path)
                .map_err(|e| e.to_string())
                .and_then(|metadata| {
                    if metadata.len() > 10 * 1024 * 1024 {
                        Err("Font must be smaller than 10 MiB".into())
                    } else {
                        std::fs::read(path).map_err(|e| e.to_string())
                    }
                })
                .and_then(|bytes| {
                    let face = ttf_parser::Face::parse(&bytes, 0)
                        .map_err(|_| "Choose a TrueType or OpenType font".to_string())?;
                    let family = face
                        .names()
                        .into_iter()
                        .find(|name| name.name_id == ttf_parser::name_id::FAMILY)
                        .and_then(|name| name.to_string())
                        .ok_or("Font has no family name")?;
                    cx.text_system()
                        .add_fonts(vec![bytes.into()])
                        .map_err(|e| e.to_string())?;
                    Ok(family)
                });
            match result {
                Ok(family) => self.font_family = family.into(),
                Err(error) => self.notice = Some(format!("Font unavailable. {error}")),
            }
        }
    }
    fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        if self.save_due.is_some_and(|due| Instant::now() >= due) {
            self.save_due = None;
            if let Err(error) = self.store.save(&self.preferences) {
                self.notice = Some(error);
                changed = true;
            }
        }
        while let Some(action) = self.tray.as_ref().and_then(Tray::poll) {
            match action {
                TrayAction::Open => {
                    self.hidden = false;
                    if !self.paused {
                        self.graph_timeline
                            .snap(self.snapshot.elapsed_secs, Instant::now());
                    }
                    self.monitor.set_visible(true);
                    cx.activate(true);
                    window.activate_window();
                }
                TrayAction::Pause => self.toggle_pause(cx),
                TrayAction::Quit => cx.quit(),
            }
            changed = true;
        }
        if self.hidden && window.is_window_active() {
            self.hidden = false;
            if !self.paused {
                self.graph_timeline
                    .snap(self.snapshot.elapsed_secs, Instant::now());
            }
            self.monitor.set_visible(true);
            changed = true;
        }
        let next = self.monitor.latest();
        if next.sequence != self.snapshot.sequence {
            if self.interface.is_empty() {
                self.interface = next
                    .networks
                    .iter()
                    .find(|n| n.name == "en0")
                    .or(next.networks.first())
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
            }
            let network = next.networks.iter().find(|n| n.name == self.interface);
            let received = network.map(|n| n.received_per_sec);
            let sent = network.map(|n| n.sent_per_sec);
            let now = Instant::now();
            let gap = std::mem::take(&mut self.history_gap);
            if !self.paused {
                if self.history.is_empty() || self.hidden || gap {
                    self.graph_timeline.snap(next.elapsed_secs, now);
                } else {
                    self.graph_timeline.advance_to(
                        next.elapsed_secs,
                        next.elapsed_secs - self.snapshot.elapsed_secs,
                        now,
                    );
                }
            }
            self.history.push_back(History {
                time: next.elapsed_secs,
                cpu: next.cpu_percent,
                memory: ratio(next.memory.used, next.memory.total),
                gpu: next.hardware.gpu_percent,
                received,
                sent,
                cores: next.cores.clone(),
                swap: ratio(next.memory.swap_used, next.memory.swap_total),
                break_before: gap,
            });
            self.snapshot = next;
            self.trim_history();
            self.update_process_rows(cx);
            changed = true;
        }
        if let Some(result) = self.monitor.take_action_result() {
            self.notice = Some(match result {
                Ok(()) => "Process signal sent.".into(),
                Err(error) => error,
            });
            changed = true;
        }
        if changed {
            self.update_tray();
            cx.notify();
        }
    }
    fn update_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.update(&self.snapshot, &self.interface, self.paused);
        }
    }
    fn trim_history(&mut self) {
        let cutoff =
            self.graph_timeline.end_at(Instant::now()) - self.preferences.history_secs as f64;
        while self.history.len() > 902 || (self.history.len() > 2 && self.history[1].time < cutoff)
        {
            self.history.pop_front();
        }
    }
    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        self.paused = !self.paused;
        self.graph_timeline.freeze(Instant::now());
        self.history_gap = true;
        self.monitor.set_paused(self.paused);
        self.update_tray();
        cx.notify();
    }
    fn update_process_rows(&mut self, cx: &gpui::App) {
        let query = self
            .inputs
            .get(&Edit::Search)
            .map(|input| input.read(cx).value())
            .unwrap_or_default();
        self.process_rows = process_rows(
            &self.snapshot.processes,
            query.as_ref(),
            self.sort,
            self.descending,
            self.tree,
        );
    }
    fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page != page {
            self.transition += 1;
        }
        self.page = page;
        self.needs_focus = true;
        self.overlay = None;
        cx.notify();
    }
    fn input(&self, key: Edit) -> Input {
        Input::new(&self.inputs[&key]).small()
    }
    fn button(&self, id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
        div()
            .id(id)
            .tab_index(0)
            .border_1()
            .border_color(color(self.preferences.theme.divider).opacity(0.0))
            .focus(|style| style.border_color(color(self.preferences.theme.accent)))
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .h(px(32.))
            .px_3()
            .rounded(px(7.))
            .bg(color(self.preferences.theme.divider).opacity(0.45))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|s| s.bg(color(self.preferences.theme.divider)))
            .child(text.into())
    }
    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.preferences.theme;
        let mut sidebar = div()
            .w(px(204.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(color(t.background))
            .border_r_1()
            .border_color(color(t.divider))
            .px_3()
            .pt(px(27.))
            .pb_4();
        sidebar = sidebar.child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .mb(px(36.))
                .child(mark())
                .child(
                    div()
                        .text_size(px(21.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Sysly"),
                ),
        );
        for (page, title, icon, shortcut) in [
            (Page::Overview, "Overview", IconName::LayoutDashboard, "⌘1"),
            (
                Page::Processes,
                "Apps & processes",
                IconName::SquareTerminal,
                "⌘2",
            ),
            (Page::Appearance, "Appearance", IconName::Palette, "⌘,"),
        ] {
            let selected = self.page == page;
            sidebar = sidebar.child(
                div()
                    .id(title)
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .h(px(41.))
                    .mb_1()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(if selected {
                        color(t.text)
                    } else {
                        color(t.muted)
                    })
                    .when(selected, |s| s.bg(color(t.divider).opacity(0.60)))
                    .hover(|s| s.bg(color(t.divider).opacity(0.4)))
                    .child(Icon::new(icon).size(px(16.)))
                    .child(title)
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(color(t.muted))
                            .child(shortcut),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.navigate(page, cx))),
            );
        }
        sidebar = sidebar.child(
            div()
                .mt(px(32.))
                .mb_3()
                .px_3()
                .text_size(px(11.))
                .text_color(color(t.muted))
                .child("RESOURCES"),
        );
        for kind in SectionKind::ALL
            .into_iter()
            .filter(|k| *k != SectionKind::Processes)
        {
            let reading = self.resource_reading(kind);
            let selected = self.page == Page::Resource(kind);
            sidebar = sidebar.child(
                div()
                    .id(("resource", kind as usize))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(37.))
                    .px_3()
                    .rounded(px(7.))
                    .cursor_pointer()
                    .text_color(if selected {
                        color(t.text)
                    } else {
                        color(t.muted)
                    })
                    .when(selected, |s| s.bg(color(t.divider).opacity(0.5)))
                    .hover(|s| s.bg(color(t.divider).opacity(0.35)))
                    .child(
                        div()
                            .size(px(5.))
                            .rounded_full()
                            .bg(self.section_color(kind)),
                    )
                    .child(div().flex_1().text_size(px(12.)).child(short_title(kind)))
                    .child(div().text_size(px(10.)).font_family("Menlo").child(reading))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.navigate(Page::Resource(kind), cx)),
                    ),
            );
        }
        sidebar
            .child(div().flex_1())
            .child(
                div()
                    .px_3()
                    .py_3()
                    .border_t_1()
                    .border_color(color(t.divider))
                    .child(
                        div()
                            .text_size(px(11.))
                            .child(if self.snapshot.cpu_name.is_empty() {
                                "Connecting to your Mac".into()
                            } else {
                                self.snapshot.cpu_name.clone()
                            }),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(10.))
                            .text_color(color(t.muted))
                            .child(format!(
                                "{} cores · {} memory",
                                self.snapshot.cores.len(),
                                bytes(self.snapshot.memory.total)
                            )),
                    ),
            )
            .into_any_element()
    }
    fn resource_reading(&self, kind: SectionKind) -> String {
        let s = &self.snapshot;
        match kind {
            SectionKind::Processor => format!("{:.0}%", s.cpu_percent),
            SectionKind::Memory => format!("{:.0}%", ratio(s.memory.used, s.memory.total)),
            SectionKind::Graphics => optional(s.hardware.gpu_percent, "%"),
            SectionKind::Network => format!(
                "{}/s",
                bytes(s.networks.iter().map(|n| n.received_per_sec as u64).sum())
            ),
            SectionKind::Storage => format!(
                "{} {}",
                s.disks.len(),
                if s.disks.len() == 1 {
                    "volume"
                } else {
                    "volumes"
                }
            ),
            SectionKind::Battery => s
                .battery
                .as_ref()
                .map(|b| format!("{:.0}%", b.percent))
                .unwrap_or_else(|| "N/A".into()),
            SectionKind::Processes => s.processes.len().to_string(),
        }
    }
    fn section_color(&self, kind: SectionKind) -> Hsla {
        let t = &self.preferences.theme;
        color(match kind {
            SectionKind::Processor => t.cpu,
            SectionKind::Memory => t.memory,
            SectionKind::Graphics | SectionKind::Battery => t.gpu,
            SectionKind::Network => t.network,
            _ => t.accent,
        })
    }
    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let (title, subtitle) = match self.page {
            Page::Overview => (
                self.preferences.dashboard_title.clone(),
                self.preferences.dashboard_subtitle.clone(),
            ),
            Page::Processes => (
                "Apps & processes".into(),
                "A closer look at the work happening on your Mac.".into(),
            ),
            Page::Appearance => (
                "Make it yours".into(),
                "A workspace that feels like your own. Changes save automatically.".into(),
            ),
            Page::Resource(kind) => (kind.title().into(), kind.description().into()),
        };
        div()
            .flex()
            .items_start()
            .justify_between()
            .gap_4()
            .mb(px(30.))
            .child(
                div()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(28.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt(px(8.))
                            .text_size(px(12.5))
                            .text_color(color(self.preferences.theme.muted))
                            .child(subtitle),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .pt_1()
                    .child(
                        self.button("pause", if self.paused { "Resume" } else { "Pause" })
                            .child(playback_icon(self.paused))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_pause(cx))),
                    )
                    .child(
                        self.button("customize", "Customize")
                            .child(Icon::new(IconName::Settings2).size(px(13.)))
                            .on_click(
                                cx.listener(|this, _, _, cx| this.navigate(Page::Appearance, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn overview(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut content = div().flex().flex_col().gap_4();
        let sections: Vec<_> = self
            .preferences
            .sections
            .iter()
            .filter(|s| s.visible)
            .cloned()
            .collect();
        let mut pending: Option<SectionConfig> = None;
        for section in sections {
            if section.span == Span::Full {
                if let Some(previous) = pending.take() {
                    content = content.child(self.section(&previous, false, cx));
                }
                content = content.child(self.section(&section, false, cx));
            } else if let Some(previous) = pending.take() {
                content = content.child(
                    div()
                        .flex()
                        .gap_4()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(self.section(&previous, false, cx)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(self.section(&section, false, cx)),
                        ),
                );
            } else {
                pending = Some(section);
            }
        }
        if let Some(section) = pending {
            content = content.child(self.section(&section, false, cx));
        }
        if !self.preferences.sections.iter().any(|s| s.visible) {
            content = content.child(
                div()
                    .p_8()
                    .text_color(color(self.preferences.theme.muted))
                    .child("Your dashboard is clear. Choose Customize to show a resource."),
            );
        }
        content.into_any_element()
    }
    fn section(
        &self,
        section: &SectionConfig,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.preferences.theme;
        let kind = section.kind;
        let accent = self.section_color(kind);
        let mut card = div()
            .id(("section", kind as usize))
            .flex()
            .flex_col()
            .h_full()
            .p(px(if self.preferences.density == Density::Compact {
                17.
            } else {
                23.
            }))
            .rounded(px(14.))
            .bg(color(t.surface));
        card = card.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .mb(px(19.))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(section.title.clone()),
                )
                .child(
                    div()
                        .id(("detail", kind as usize))
                        .flex()
                        .items_center()
                        .gap_1()
                        .cursor_pointer()
                        .text_size(px(11.))
                        .text_color(color(t.muted))
                        .hover(|s| s.text_color(color(t.text)))
                        .child(if expanded { "Overview" } else { "View details" })
                        .child(Icon::new(IconName::ChevronRight).size(px(12.)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.navigate(
                                if expanded {
                                    Page::Overview
                                } else if kind == SectionKind::Processes {
                                    Page::Processes
                                } else {
                                    Page::Resource(kind)
                                },
                                cx,
                            )
                        })),
                ),
        );
        let detailed = expanded || section.details;
        match kind {
            SectionKind::Processor => {
                card = card.child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_2()
                        .child(reading(
                            format!("{:.1}", self.snapshot.cpu_percent),
                            accent,
                            36.,
                        ))
                        .child(label("% in use", color(t.muted)))
                        .child(div().flex_1())
                        .child(label(
                            format!("{} cores", self.snapshot.cores.len()),
                            color(t.muted),
                        )),
                );
                card = card.child(self.chart(
                    kind,
                    section.graph_height + if expanded { 100. } else { 35. },
                ));
                let mut cores = div().flex().flex_wrap().gap_x_3().gap_y_2().mt_3();
                for (i, value) in self.snapshot.cores.iter().enumerate() {
                    cores = cores.child(
                        div()
                            .w(relative(if expanded { 0.115 } else { 0.22 }))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(label(format!("{:02}", i + 1), color(t.muted)))
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(4.))
                                    .rounded_full()
                                    .bg(color(t.divider))
                                    .child(
                                        div()
                                            .w(relative((*value / 100.).clamp(0., 1.)))
                                            .h_full()
                                            .rounded_full()
                                            .bg(accent),
                                    ),
                            ),
                    );
                }
                card = card.child(cores);
                if detailed {
                    let mut core_history = div().flex().flex_wrap().gap_3().mt_5();
                    for (index, value) in self.snapshot.cores.iter().enumerate() {
                        let values = self
                            .history
                            .iter()
                            .map(|h| (h.time, h.cores.get(index).copied(), h.break_before))
                            .collect();
                        core_history = core_history.child(
                            div()
                                .w(relative(0.23))
                                .child(
                                    div()
                                        .flex()
                                        .justify_between()
                                        .child(label(format!("Core {}", index + 1), color(t.muted)))
                                        .child(label(format!("{value:.1}%"), accent)),
                                )
                                .child(trace(
                                    values,
                                    100.,
                                    self.graph_viewport,
                                    accent,
                                    color(t.divider),
                                    45.,
                                )),
                        );
                    }
                    card = card.child(core_history);
                }
                card = card.child(self.stat_line(
                    "Load average",
                    format!(
                        "{:.2}  ·  {:.2}  ·  {:.2}",
                        self.snapshot.load[0], self.snapshot.load[1], self.snapshot.load[2]
                    ),
                ));
                if detailed {
                    card = card
                        .child(self.stat_line("Processor", self.snapshot.cpu_name.clone()))
                        .child(self.stat_line(
                            "Temperature",
                            optional(self.snapshot.hardware.cpu_temp, "°C"),
                        ))
                        .child(self.stat_line(
                            "CPU power",
                            optional(self.snapshot.hardware.cpu_watts, " W"),
                        ));
                    for (name, mhz) in &self.snapshot.hardware.tiers {
                        card = card.child(self.stat_line(name.clone(), format!("{mhz} MHz")));
                    }
                }
            }
            SectionKind::Memory => {
                let m = &self.snapshot.memory;
                let used = ratio(m.used, m.total);
                card = card.child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_2()
                        .child(reading(bytes(m.used), accent, 36.))
                        .child(label(format!("of {}", bytes(m.total)), color(t.muted))),
                );
                card = card.child(
                    div()
                        .mt(px(25.))
                        .mb(px(14.))
                        .h(px(22.))
                        .rounded(px(5.))
                        .overflow_hidden()
                        .bg(color(t.divider))
                        .child(
                            div()
                                .w(relative((used / 100.).clamp(0., 1.)))
                                .h_full()
                                .bg(accent),
                        ),
                );
                card = card.child(
                    div()
                        .flex()
                        .justify_between()
                        .child(legend("Used", bytes(m.used), accent))
                        .child(legend(
                            "Not in use",
                            bytes(m.total.saturating_sub(m.used)),
                            color(t.muted),
                        )),
                );
                card = card
                    .child(self.chart(kind, section.graph_height.max(65.)))
                    .child(self.stat_line(
                        "Swap",
                        format!("{} of {}", bytes(m.swap_used), bytes(m.swap_total)),
                    ));
                if detailed {
                    card = card
                        .child(trace(
                            self.history
                                .iter()
                                .map(|h| (h.time, Some(h.swap), h.break_before))
                                .collect(),
                            100.,
                            self.graph_viewport,
                            color(t.accent),
                            color(t.divider),
                            70.,
                        ))
                        .child(self.stat_line("Memory in use", format!("{used:.1}%")))
                        .child(self.stat_line("Available incl. caches", bytes(m.available)))
                        .child(label(
                            "macOS manages cached and compressed memory automatically.",
                            color(t.muted),
                        ));
                }
            }
            SectionKind::Graphics => {
                let h = &self.snapshot.hardware;
                card = card.child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_2()
                        .child(reading(
                            h.gpu_percent
                                .map(|v| format!("{v:.1}"))
                                .unwrap_or_else(|| "Unavailable".into()),
                            accent,
                            if h.gpu_percent.is_some() { 32. } else { 23. },
                        ))
                        .when(h.gpu_percent.is_some(), |s| {
                            s.child(label("% active time", color(t.muted)))
                        }),
                );
                card = card.child(self.chart(kind, section.graph_height)).child(
                    div()
                        .flex()
                        .justify_between()
                        .mt_2()
                        .child(legend(
                            "Temperature",
                            optional(h.gpu_temp, "°C"),
                            color(t.muted),
                        ))
                        .child(legend(
                            "Frequency",
                            h.gpu_mhz
                                .map(|v| format!("{v} MHz"))
                                .unwrap_or_else(|| "Unavailable".into()),
                            color(t.muted),
                        ))
                        .child(legend("Power", optional(h.gpu_watts, " W"), color(t.muted))),
                );
                if h.gpu_percent.is_none() || detailed {
                    card = card.child(
                        div()
                            .mt_3()
                            .text_size(px(11.))
                            .text_color(color(t.muted))
                            .child(if h.status.is_empty() {
                                "Hardware telemetry is connecting.".into()
                            } else {
                                h.status.clone()
                            }),
                    );
                }
            }
            SectionKind::Network => {
                let selected = self
                    .snapshot
                    .networks
                    .iter()
                    .find(|n| n.name == self.interface);
                let received = selected.map(|n| n.received_per_sec).unwrap_or(0.);
                let sent = selected.map(|n| n.sent_per_sec).unwrap_or(0.);
                card = card
                    .child(
                        div()
                            .flex()
                            .gap_8()
                            .child(legend(
                                "Download",
                                format!("{}/s", bytes(received as u64)),
                                accent,
                            ))
                            .child(legend(
                                "Upload",
                                format!("{}/s", bytes(sent as u64)),
                                color(t.accent),
                            )),
                    )
                    .child(self.chart(kind, section.graph_height));
                if detailed {
                    card = card.child(div().flex().flex_wrap().gap_2().mt_3().children(
                        self.snapshot.networks.iter().map(|network| {
                            let name = network.name.clone();
                            self.button(
                                SharedString::from(format!("interface-{}", name)),
                                name.clone(),
                            )
                            .when(name == self.interface, |s| {
                                s.text_color(accent).bg(accent.opacity(0.12))
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.interface = name.clone();
                                    this.update_tray();
                                    for history in &mut this.history {
                                        history.received = None;
                                        history.sent = None;
                                    }
                                    cx.notify();
                                },
                            ))
                        }),
                    ));
                }
                let active: Vec<_> = self
                    .snapshot
                    .networks
                    .iter()
                    .filter(|n| !n.addresses.is_empty() || n.received_per_sec + n.sent_per_sec > 0.)
                    .collect();
                for n in active.iter().take(if detailed { usize::MAX } else { 2 }) {
                    card = card.child(self.stat_line(
                        n.name.clone(),
                        if detailed {
                            n.addresses.clone()
                        } else {
                            format!("↓ {}   ↑ {}", bytes(n.total_received), bytes(n.total_sent))
                        },
                    ));
                    if detailed {
                        card = card.child(self.stat_line(
                            "Transferred",
                            format!(
                                "↓ {}   ↑ {} · {} errors",
                                bytes(n.total_received),
                                bytes(n.total_sent),
                                n.errors
                            ),
                        ));
                    }
                }
            }
            SectionKind::Storage => {
                if self.snapshot.disks.is_empty() {
                    card = card.child(label("Waiting for drive information…", color(t.muted)));
                }
                for disk in self
                    .snapshot
                    .disks
                    .iter()
                    .take(if detailed { usize::MAX } else { 2 })
                {
                    let used = disk.total.saturating_sub(disk.available);
                    let percent = ratio(used, disk.total);
                    card = card.child(
                        div()
                            .mb_4()
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .mb_2()
                                    .child(label(
                                        if disk.name.is_empty() {
                                            disk.mount.clone()
                                        } else {
                                            disk.name.clone()
                                        },
                                        color(t.text),
                                    ))
                                    .child(label(
                                        format!("{} free", bytes(disk.available)),
                                        color(t.muted),
                                    )),
                            )
                            .child(
                                div().h(px(7.)).rounded_full().bg(color(t.divider)).child(
                                    div()
                                        .w(relative((percent / 100.).clamp(0., 1.)))
                                        .h_full()
                                        .rounded_full()
                                        .bg(accent),
                                ),
                            )
                            .child(
                                div()
                                    .mt_2()
                                    .text_size(px(11.))
                                    .text_color(color(t.muted))
                                    .child(format!(
                                        "{} of {} used",
                                        bytes(used),
                                        bytes(disk.total)
                                    )),
                            ),
                    );
                    if detailed {
                        card = card
                            .child(self.stat_line("Mount", disk.mount.clone()))
                            .child(self.stat_line(
                                "Read / write",
                                match (disk.read_per_sec, disk.write_per_sec) {
                                    (Some(r), Some(w)) => {
                                        format!("{}/s  /  {}/s", bytes(r as u64), bytes(w as u64))
                                    }
                                    _ => "Per-drive I/O unavailable".into(),
                                },
                            ));
                    }
                }
            }
            SectionKind::Battery => {
                if let Some(b) = &self.snapshot.battery {
                    card = card
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap_3()
                                .child(reading(format!("{:.0}%", b.percent), accent, 32.))
                                .child(label(b.state.clone(), color(t.muted))),
                        )
                        .child(
                            self.stat_line("Battery health", format!("{:.0}%", b.health_percent)),
                        )
                        .child(self.stat_line("Power draw", format!("{:.1} W", b.watts)));
                    if detailed {
                        card = card
                            .child(
                                self.stat_line(
                                    "Time remaining",
                                    b.remaining_secs
                                        .map(duration)
                                        .unwrap_or_else(|| "Calculating / on power".into()),
                                ),
                            )
                            .child(
                                self.stat_line(
                                    "Cycle count",
                                    b.cycles
                                        .map(|v| v.to_string())
                                        .unwrap_or_else(|| "Unavailable".into()),
                                ),
                            );
                    }
                } else {
                    card = card.child(reading("No battery data", accent, 24.)).child(
                        div()
                            .mt_3()
                            .child(label("Battery telemetry is unavailable on this Mac. See Charts & menu bar for collection notices.", color(t.muted))),
                    );
                }
                card = card.child(self.stat_line(
                    "System power",
                    optional(self.snapshot.hardware.system_watts, " W"),
                ));
                for (name, rpm) in &self.snapshot.hardware.fans {
                    card = card.child(self.stat_line(name.clone(), format!("{rpm} RPM")));
                }
            }
            SectionKind::Processes => {
                let mut processes: Vec<_> = self.snapshot.processes.iter().collect();
                processes.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent));
                for process in processes.iter().take(5) {
                    let key = process.key;
                    card = card.child(
                        div()
                            .id(("top-process", key.pid as usize))
                            .flex()
                            .items_center()
                            .h(px(37.))
                            .border_b_1()
                            .border_color(color(t.divider).opacity(0.55))
                            .cursor_pointer()
                            .hover(|s| s.bg(color(t.divider).opacity(0.4)))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.))
                                    .child(process.name.clone()),
                            )
                            .child(
                                div()
                                    .w(px(90.))
                                    .font_family("Menlo")
                                    .text_size(px(11.))
                                    .child(format!("{:.1}%", process.cpu_percent)),
                            )
                            .child(
                                div()
                                    .w(px(90.))
                                    .text_right()
                                    .text_size(px(11.))
                                    .text_color(color(t.muted))
                                    .child(bytes(process.memory)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.overlay = Some(Overlay::Inspect(key));
                                cx.notify();
                            })),
                    );
                }
                card = card.child(
                    div()
                        .mt_3()
                        .text_size(px(11.))
                        .text_color(color(t.muted))
                        .child(format!(
                            "{} apps and background processes",
                            self.snapshot.processes.len()
                        )),
                );
            }
        }
        if detailed {
            card = card.child(
                div()
                    .mt_4()
                    .text_size(px(11.))
                    .text_color(color(t.muted))
                    .child(section.description.clone()),
            );
        }
        card.into_any_element()
    }
    fn stat_line(
        &self,
        name: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> AnyElement {
        div()
            .flex()
            .justify_between()
            .gap_4()
            .pt_3()
            .text_size(px(11.))
            .child(
                div()
                    .text_color(color(self.preferences.theme.muted))
                    .child(name.into()),
            )
            .child(div().text_right().child(value.into()))
            .into_any_element()
    }
    fn chart(&self, kind: SectionKind, height: f32) -> AnyElement {
        let viewport = self.graph_viewport;
        let max = if kind == SectionKind::Network {
            network_maximum(&self.history, viewport)
        } else {
            100.
        };
        let history = self.history.clone();
        let accent = self.section_color(kind);
        let secondary = color(self.preferences.theme.accent);
        let grid = color(self.preferences.theme.divider);
        let span = viewport.span;
        div()
            .mt_4()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .mb_1()
                    .text_size(px(9.))
                    .text_color(color(self.preferences.theme.muted))
                    .child(if kind == SectionKind::Network {
                        format!("{}/s", bytes(max as u64))
                    } else {
                        "100%".into()
                    }),
            )
            .child(
                canvas(
                    move |_, _, _| (),
                    move |bounds, _, window, _| {
                        let x0 = bounds.left();
                        let y0 = bounds.top();
                        let width = bounds.size.width;
                        let height = bounds.size.height;
                        for i in 0..=4 {
                            let y = y0 + height * (i as f32 / 4.);
                            let mut path = PathBuilder::stroke(px(1.));
                            path.move_to(point(x0, y));
                            path.line_to(point(x0 + width, y));
                            if let Ok(path) = path.build() {
                                window.paint_path(path, grid.opacity(0.65));
                            }
                        }
                        let series = if kind == SectionKind::Network { 2 } else { 1 };
                        for series_index in 0..series {
                            let values: Vec<_> = history
                                .iter()
                                .map(|h| {
                                    let value = match kind {
                                        SectionKind::Processor => Some(h.cpu),
                                        SectionKind::Memory => Some(h.memory),
                                        SectionKind::Graphics => h.gpu,
                                        SectionKind::Network => {
                                            if series_index == 0 {
                                                h.received.map(|v| v as f32)
                                            } else {
                                                h.sent.map(|v| v as f32)
                                            }
                                        }
                                        _ => None,
                                    };
                                    (h.time, value, h.break_before)
                                })
                                .collect();
                            paint_trace(
                                window,
                                bounds,
                                &values,
                                max as f32,
                                viewport,
                                if series_index == 0 { accent } else { secondary },
                                1.8,
                            );
                        }
                    },
                )
                .w_full()
                .h(px(height)),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .mt_2()
                    .text_size(px(9.))
                    .text_color(color(self.preferences.theme.muted))
                    .child(if span >= 60. {
                        format!("{:.1} min ago", span / 60.)
                    } else {
                        format!("{:.0}s ago", span)
                    })
                    .child(if self.history.len() < 2 {
                        "Collecting samples"
                    } else {
                        "Now"
                    }),
            )
            .into_any_element()
    }
    fn processes(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.preferences.theme;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .mb_5()
                    .child(
                        div().w(px(330.)).child(
                            self.input(Edit::Search)
                                .prefix(Icon::new(IconName::Search).size(px(14.)))
                                .cleanable(true),
                        ),
                    )
                    .child(
                        self.button(
                            "tree",
                            if self.tree {
                                "Tree view · on"
                            } else {
                                "Tree view"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tree = !this.tree;
                            this.update_process_rows(cx);
                            cx.notify();
                        })),
                    )
                    .child(div().flex_1())
                    .child(label(
                        format!("{} processes", self.process_rows.len()),
                        color(t.muted),
                    )),
            )
            .child(
                div()
                    .flex()
                    .h(px(35.))
                    .px_3()
                    .border_b_1()
                    .border_color(color(t.divider))
                    .children(
                        [
                            (Sort::Name, "Process"),
                            (Sort::Pid, "PID"),
                            (Sort::Cpu, "CPU"),
                            (Sort::Memory, "Memory"),
                            (Sort::Io, "Disk I/O"),
                        ]
                        .into_iter()
                        .map(|(sort, name)| {
                            div()
                                .id(name)
                                .flex()
                                .items_center()
                                .gap_1()
                                .cursor_pointer()
                                .text_size(px(10.))
                                .text_color(if self.sort == sort {
                                    color(t.text)
                                } else {
                                    color(t.muted)
                                })
                                .when(sort == Sort::Name, |s| s.flex_1())
                                .when(sort != Sort::Name, |s| {
                                    s.w(px(if sort == Sort::Pid { 80. } else { 115. }))
                                        .flex_shrink_0()
                                })
                                .child(name)
                                .when(self.sort == sort, |s| {
                                    s.child(
                                        Icon::new(if self.descending {
                                            IconName::ArrowDown
                                        } else {
                                            IconName::ArrowUp
                                        })
                                        .size(px(11.)),
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
                        }),
                    ),
            )
            .when(self.process_rows.is_empty(), |s| {
                s.child(div().p_8().text_color(color(t.muted)).child(
                    if self.snapshot.sequence == 0 {
                        "Collecting processes…"
                    } else {
                        "No processes match your search."
                    },
                ))
            })
            .child(
                uniform_list(
                    "process-list",
                    self.process_rows.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row| {
                                let (index, depth) = this.process_rows[row];
                                let process = &this.snapshot.processes[index];
                                let key = process.key;
                                let t = &this.preferences.theme;
                                div()
                                    .id(("process", key.pid as usize))
                                    .w_full()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .h(px(43.))
                                    .px_3()
                                    .border_b_1()
                                    .border_color(color(t.divider).opacity(0.45))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(color(t.surface)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .pl(px(depth as f32 * 13.))
                                            .flex()
                                            .items_center()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .size(px(25.))
                                                    .rounded(px(6.))
                                                    .bg(color(t.divider).opacity(0.7))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        Icon::new(if process.parent == Some(1) {
                                                            IconName::SquareTerminal
                                                        } else {
                                                            IconName::Frame
                                                        })
                                                        .size(px(13.))
                                                        .text_color(color(t.muted)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .text_size(px(12.))
                                                            .truncate()
                                                            .child(process.name.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(9.))
                                                            .text_color(color(t.muted))
                                                            .child(process.user.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(table_value(
                                        process.key.pid.to_string(),
                                        80.,
                                        color(t.muted),
                                    ))
                                    .child(table_value(
                                        format!("{:.1}%", process.cpu_percent),
                                        115.,
                                        if process.cpu_percent > 20. {
                                            color(t.cpu)
                                        } else {
                                            color(t.text)
                                        },
                                    ))
                                    .child(table_value(bytes(process.memory), 115., color(t.text)))
                                    .child(table_value(
                                        format!(
                                            "{}/s",
                                            bytes(
                                                (process.read_per_sec + process.write_per_sec)
                                                    as u64
                                            )
                                        ),
                                        115.,
                                        color(t.muted),
                                    ))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.overlay = Some(Overlay::Inspect(key));
                                        cx.notify();
                                    }))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .w_full()
                .flex_1()
                .min_h_0(),
            )
            .into_any_element()
    }
    fn appearance(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.preferences.theme;
        let mut page = div().flex().flex_col().gap_6();
        page = page.child(
            div().flex().gap_2().mb_1().children(
                ["Appearance", "Layout & text", "Charts & menu bar"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, title)| {
                        self.button(("settings-tab", index), title)
                            .when(self.customize_tab == index, |s| {
                                s.bg(color(t.accent).opacity(0.16))
                                    .text_color(color(t.accent))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.customize_tab = index;
                                cx.notify();
                            }))
                    }),
            ),
        );
        match self.customize_tab {
            0 => {
                let mut theme = div().flex().flex_col().gap_4().child(settings_heading(
                    "Color & character",
                    "Start with a palette, then tune every detail.",
                    t,
                ));
                theme = theme.child(
                    div().flex().gap_3().children(
                        [
                            (Theme::dark(), "Graphite"),
                            (Theme::light(), "Paper"),
                            (Theme::midnight(), "Midnight"),
                        ]
                        .into_iter()
                        .map(|(theme, name)| {
                            let preview = theme.clone();
                            div()
                                .id(name)
                                .w(px(158.))
                                .p_3()
                                .rounded(px(10.))
                                .bg(color(theme.surface))
                                .border_1()
                                .border_color(if t.name == name {
                                    color(t.accent)
                                } else {
                                    color(t.divider)
                                })
                                .cursor_pointer()
                                .hover(|s| s.border_color(color(t.accent)))
                                .child(
                                    div().flex().gap_1().mb_3().children(
                                        [theme.cpu, theme.memory, theme.gpu, theme.accent]
                                            .into_iter()
                                            .map(|c| {
                                                div()
                                                    .h(px(24.))
                                                    .flex_1()
                                                    .rounded(px(3.))
                                                    .bg(color(c))
                                            }),
                                    ),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(color(theme.text))
                                        .child(name),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.preferences.theme = preview.clone();
                                    this.create_inputs(window, cx);
                                    this.load_font(cx);
                                    this.save(cx);
                                }))
                        }),
                    ),
                );
                theme =
                    theme.child(
                        div()
                            .flex()
                            .gap_2()
                            .child(self.button("import-theme", "Import theme…").on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.file_action(FileAction::ImportTheme, window, cx)
                                }),
                            ))
                            .child(self.button("export-theme", "Export theme…").on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.file_action(FileAction::ExportTheme, window, cx)
                                }),
                            )),
                    );
                theme = theme.child(setting_row("Theme name", self.input(Edit::ThemeName), t));
                let mut palette = div().flex().flex_wrap().gap_x_6().gap_y_3();
                for (index, (name, value)) in theme_colors(t).iter().enumerate() {
                    palette = palette.child(
                        div()
                            .w(relative(0.45))
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(17.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(color(t.divider))
                                    .bg(color(*value)),
                            )
                            .child(div().flex_1().text_size(px(12.)).child(*name))
                            .child(div().w(px(112.)).child(self.input(Edit::Color(index)))),
                    );
                }
                theme = theme.child(div().mt_2().child(palette));
                page = page.child(theme);
                let mut typography = div()
                    .pt_5()
                    .border_t_1()
                    .border_color(color(t.divider))
                    .child(settings_heading(
                        "Typography",
                        "Comfortable to read, from headlines to the smallest reading.",
                        t,
                    ));
                typography = typography.child(
                    div()
                        .flex()
                        .gap_2()
                        .mt_4()
                        .child(
                            self.button("system-font", "System sans")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.preferences.theme.font = FontChoice::System;
                                    this.preferences.custom_font_path = None;
                                    this.load_font(cx);
                                    this.save(cx);
                                })),
                        )
                        .child(self.button("mono-font", "Monospace").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.preferences.theme.font = FontChoice::Monospace;
                                this.preferences.custom_font_path = None;
                                this.load_font(cx);
                                this.save(cx);
                            },
                        )))
                        .child(self.button("load-font", "Load font…").on_click(cx.listener(
                            |this, _, _, cx| {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Fonts", &["ttf", "otf", "ttc"])
                                    .pick_file()
                                {
                                    this.preferences.custom_font_path = Some(path);
                                    this.load_font(cx);
                                    this.save(cx);
                                }
                            },
                        ))),
                );
                typography = typography.child(
                    div().mt_3().child(label(
                        self.preferences
                            .custom_font_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| self.font_family.to_string()),
                        color(t.muted),
                    )),
                );
                typography = typography.child(setting_row(
                    "Text size",
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(self.button("font-minus", "−").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.preferences.theme.font_size =
                                    (this.preferences.theme.font_size - 1.).max(11.);
                                this.save(cx);
                            },
                        )))
                        .child(format!("{:.0} px", t.font_size))
                        .child(self.button("font-plus", "+").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.preferences.theme.font_size =
                                    (this.preferences.theme.font_size + 1.).min(20.);
                                this.save(cx);
                            },
                        ))),
                    t,
                ));
                page = page.child(typography);
            }
            1 => {
                page = page.child(
                    div()
                        .child(settings_heading(
                            "Your dashboard",
                            "Edit the words and arrange the resources that matter to you.",
                            t,
                        ))
                        .child(setting_row("Title", self.input(Edit::Title), t))
                        .child(setting_row("Description", self.input(Edit::Subtitle), t)),
                );
                let mut presets = div().flex().items_center().gap_2();
                for name in ["Balanced", "Focus", "Compact"] {
                    presets = presets.child(self.button(name, name).on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.preferences.apply_preset(name);
                            this.create_inputs(window, cx);
                            this.save(cx);
                        },
                    )));
                }
                presets = presets
                    .child(self.button("previous-layout", "Restore previous").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.preferences.restore_previous_layout();
                            this.create_inputs(window, cx);
                            this.save(cx);
                        }),
                    ))
                    .child(
                        self.button("reset-layout", "Reset layout")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.preferences.reset_layout();
                                this.create_inputs(window, cx);
                                this.save(cx);
                            })),
                    );
                page=page.child(div().child(settings_heading("Layout","Drag a section to reorder it. Drag its bottom edge to resize the chart.",t)).child(div().mt_4().child(presets)));
                for section in &self.preferences.sections {
                    page = page.child(self.section_editor(section, cx));
                }
                let mut saved = div()
                    .pt_5()
                    .border_t_1()
                    .border_color(color(t.divider))
                    .child(settings_heading(
                        "Saved layouts",
                        "Keep a few different ways to see your Mac.",
                        t,
                    ));
                saved = saved.child(
                    div()
                        .flex()
                        .gap_2()
                        .mt_4()
                        .child(div().w(px(260.)).child(self.input(Edit::LayoutName)))
                        .child(
                            self.button("save-layout", "Save current")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let name = this.inputs[&Edit::LayoutName].read(cx).value();
                                    this.notice = Some(
                                        match this.preferences.save_named_layout(name.as_ref()) {
                                            Ok(()) => "Layout saved.".into(),
                                            Err(error) => error,
                                        },
                                    );
                                    this.save(cx);
                                })),
                        )
                        .child(
                            self.button("import-layout", "Import…")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.file_action(FileAction::ImportLayout, window, cx)
                                })),
                        )
                        .child(
                            self.button("export-layout", "Export…")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.file_action(FileAction::ExportLayout, window, cx)
                                })),
                        ),
                );
                for (i, layout) in self.preferences.saved_layouts.iter().enumerate() {
                    saved = saved.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .mt_3()
                            .child(div().flex_1().text_size(px(12.)).child(layout.name.clone()))
                            .child(
                                self.button(("load-layout", i), "Load")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.preferences.load_named_layout(i);
                                        this.create_inputs(window, cx);
                                        this.save(cx);
                                    })),
                            )
                            .child(self.button(("delete-layout", i), "Delete").on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.preferences.saved_layouts.remove(i);
                                    this.save(cx);
                                }),
                            )),
                    )
                }
                page = page.child(saved);
            }
            _ => {
                page = page.child(settings_heading(
                    "Sampling & history",
                    "Keep enough context without collecting more than you need.",
                    t,
                ));
                for (title, values, current, mode) in [
                    (
                        "Sample every",
                        vec![1, 2, 3, 5, 10],
                        self.preferences.refresh_secs,
                        0,
                    ),
                    (
                        "History window",
                        vec![30, 60, 120, 240, 300, 600, 900],
                        self.preferences.history_secs as u64,
                        1,
                    ),
                ] {
                    page = page.child(setting_row(
                        title,
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .children(values.into_iter().map(|value| {
                                self.button(
                                    (title, value as usize),
                                    if mode == 0 {
                                        format!("{value}s")
                                    } else {
                                        format!("{} min", value as f32 / 60.)
                                    },
                                )
                                .when(value == current, |s| {
                                    s.bg(color(t.accent).opacity(0.2))
                                        .text_color(color(t.accent))
                                })
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if mode == 0 {
                                            this.preferences.refresh_secs = value;
                                        } else {
                                            this.preferences.history_secs = value as u32;
                                        }
                                        this.save(cx);
                                    },
                                ))
                            })),
                        t,
                    ));
                }
                page = page.child(setting_row(
                    "Density",
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button("comfortable", "Comfortable")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.preferences.density = Density::Comfortable;
                                    this.save(cx);
                                })),
                        )
                        .child(self.button("compact", "Compact").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.preferences.density = Density::Compact;
                                this.save(cx);
                            },
                        ))),
                    t,
                ));
                page = page.child(
                    div()
                        .pt_5()
                        .border_t_1()
                        .border_color(color(t.divider))
                        .child(settings_heading(
                            "Menu bar",
                            "Your essentials, even when the window is closed.",
                            t,
                        ))
                        .child(div().text_color(color(t.muted)).text_size(px(12.)).child(
                            "Click the Sysly logo for CPU, memory, graphics and network activity.",
                        )),
                );
                page=page.child(div().pt_5().border_t_1().border_color(color(t.divider)).child(settings_heading("About this session","Sampling pauses on request. A hidden window samples at most every five seconds.",t)).child(self.stat_line("Samples in memory",self.history.len().to_string())).child(self.stat_line("Last collection",format!("{:.0} ms",self.snapshot.collected_ms))).child(self.stat_line("Hardware telemetry",self.snapshot.hardware.status.clone())));
                for warning in &self.snapshot.warnings {
                    page = page.child(
                        div()
                            .text_size(px(11.))
                            .text_color(color(t.gpu))
                            .child(warning.clone()),
                    );
                }
            }
        }
        page.into_any_element()
    }
    fn toggle(&self, id: impl Into<ElementId>, enabled: bool) -> Stateful<Div> {
        self.button(id, if enabled { "On" } else { "Off" })
            .min_w(px(55.))
            .when(enabled, |s| {
                s.bg(color(self.preferences.theme.accent).opacity(0.18))
                    .text_color(color(self.preferences.theme.accent))
            })
    }
    fn section_editor(&self, section: &SectionConfig, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.preferences.theme;
        let kind = section.kind;
        let index = self
            .preferences
            .sections
            .iter()
            .position(|s| s.kind == kind)
            .unwrap();
        div()
            .id(("editor", kind as usize))
            .p_4()
            .rounded(px(10.))
            .bg(color(t.surface))
            .on_drop(cx.listener(move |this, drag: &DragSection, _, cx| {
                if drag.kind != kind {
                    this.preferences.remember_layout();
                    move_section(&mut this.preferences, drag.kind, Some(kind));
                    this.save(cx);
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .mb_3()
                    .child(
                        div()
                            .id(("drag", kind as usize))
                            .cursor_move()
                            .p_1()
                            .child(
                                Icon::new(IconName::Menu)
                                    .size(px(15.))
                                    .text_color(color(t.muted)),
                            )
                            .on_drag(DragSection { kind }, |drag, _, _, cx| {
                                cx.new(|_| drag.clone())
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(kind.title()),
                    )
                    .child(self.button(("up", index), "↑").on_click(cx.listener(
                        move |this, _, _, cx| {
                            if index > 0 {
                                this.preferences.remember_layout();
                                this.preferences.sections.swap(index, index - 1);
                                this.save(cx);
                            }
                        },
                    )))
                    .child(self.button(("down", index), "↓").on_click(cx.listener(
                        move |this, _, _, cx| {
                            if index + 1 < this.preferences.sections.len() {
                                this.preferences.remember_layout();
                                this.preferences.sections.swap(index, index + 1);
                                this.save(cx);
                            }
                        },
                    )))
                    .child(
                        self.toggle(("visible", index), section.visible)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.preferences.remember_layout();
                                this.preferences.sections[index].visible =
                                    !this.preferences.sections[index].visible;
                                this.save(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .mb_3()
                    .child(div().flex_1().child(self.input(Edit::SectionTitle(kind))))
                    .child(
                        div()
                            .flex_1()
                            .child(self.input(Edit::SectionDescription(kind))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(label("Width", color(t.muted)))
                    .child(
                        self.button(
                            ("span", index),
                            if section.span == Span::Full {
                                "Full"
                            } else {
                                "Half"
                            },
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.preferences.remember_layout();
                            let s = &mut this.preferences.sections[index];
                            s.span = if s.span == Span::Half {
                                Span::Full
                            } else {
                                Span::Half
                            };
                            this.save(cx);
                        })),
                    )
                    .child(div().w_3())
                    .child(label("Chart height", color(t.muted)))
                    .child(
                        self.button(("height-minus", index), "−")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.preferences.remember_layout();
                                this.preferences.sections[index].graph_height =
                                    (this.preferences.sections[index].graph_height - 10.).max(40.);
                                this.save(cx);
                            })),
                    )
                    .child(label(
                        format!("{:.0}px", section.graph_height),
                        color(t.text),
                    ))
                    .child(
                        self.button(("height-plus", index), "+")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.preferences.remember_layout();
                                this.preferences.sections[index].graph_height =
                                    (this.preferences.sections[index].graph_height + 10.).min(240.);
                                this.save(cx);
                            })),
                    )
                    .child(div().flex_1())
                    .child(label("Details", color(t.muted)))
                    .child(
                        self.toggle(("details", index), section.details)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.preferences.sections[index].details =
                                    !this.preferences.sections[index].details;
                                this.save(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .id(("resize", index))
                    .mt_3()
                    .h(px(9.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_row_resize()
                    .hover(|s| s.bg(color(t.divider).opacity(0.5)))
                    .child(
                        div()
                            .w(px(45.))
                            .h(px(2.))
                            .rounded_full()
                            .bg(color(t.divider)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.preferences.remember_layout();
                            this.resize = Some(Resize {
                                kind,
                                y: event.position.y,
                                height: this.preferences.sections[index].graph_height,
                            });
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }
    fn file_action(&mut self, action: FileAction, window: &mut Window, cx: &mut Context<Self>) {
        let path = match action {
            FileAction::ImportTheme | FileAction::ImportLayout => rfd::FileDialog::new()
                .add_filter("Sysly JSON", &["json"])
                .pick_file(),
            FileAction::ExportTheme => rfd::FileDialog::new()
                .set_file_name("sysly-theme.json")
                .save_file(),
            FileAction::ExportLayout => rfd::FileDialog::new()
                .set_file_name("sysly-layout.json")
                .save_file(),
        };
        let Some(path) = path else {
            return;
        };
        let result = match action {
            FileAction::ImportTheme => preferences::import_theme(&path).map(|theme| {
                self.preferences.theme = theme;
            }),
            FileAction::ImportLayout => preferences::import_layout(&path).map(|sections| {
                self.preferences.remember_layout();
                self.preferences.sections = sections;
            }),
            FileAction::ExportTheme => preferences::export_theme(&path, &self.preferences.theme),
            FileAction::ExportLayout => {
                preferences::export_layout(&path, &self.preferences.sections)
            }
        };
        self.notice = Some(match result {
            Ok(()) => "Saved successfully.".into(),
            Err(error) => error,
        });
        self.create_inputs(window, cx);
        self.load_font(cx);
        self.save(cx);
    }
    fn inspector(&self, key: ProcessKey, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.preferences.theme;
        let process = self.snapshot.processes.iter().find(|p| p.key == key);
        let mut panel = div()
            .w(px(400.))
            .h_full()
            .bg(color(t.surface))
            .border_l_1()
            .border_color(color(t.divider))
            .p_6()
            .flex()
            .flex_col()
            .gap_3();
        panel = panel.child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .mb_3()
                .flex_shrink_0()
                .child(
                    div()
                        .text_size(px(18.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Process details"),
                )
                .child(
                    self.button("close-inspector", "Close")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.overlay = None;
                            this.needs_focus = true;
                            cx.notify();
                        })),
                ),
        );
        if let Some(p) = process {
            let details = div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap_3()
                .child(
                    div()
                        .text_size(px(24.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(p.name.clone()),
                )
                .child(label(
                    format!("PID {} · {}", p.key.pid, p.user),
                    color(t.muted),
                ))
                .child(self.stat_line("CPU · per core", format!("{:.1}%", p.cpu_percent)))
                .child(self.stat_line("Physical memory", bytes(p.memory)))
                .child(self.stat_line("Virtual memory", bytes(p.virtual_memory)))
                .child(self.stat_line("Status", p.status.clone()))
                .child(self.stat_line("Running for", duration(p.run_secs)))
                .child(
                    self.stat_line(
                        "Parent PID",
                        p.parent
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "None".into()),
                    ),
                )
                .child(self.stat_line("Disk read", format!("{}/s", bytes(p.read_per_sec as u64))))
                .child(self.stat_line("Disk write", format!("{}/s", bytes(p.write_per_sec as u64))))
                .child(
                    div()
                        .mt_4()
                        .text_size(px(11.))
                        .text_color(color(t.muted))
                        .child("COMMAND"),
                )
                .child(
                    div()
                        .id("command-scroll")
                        .max_h(px(180.))
                        .flex_shrink_0()
                        .overflow_y_scroll()
                        .p_3()
                        .rounded(px(7.))
                        .bg(color(t.background))
                        .font_family("Menlo")
                        .text_size(px(10.))
                        .child(p.command.clone()),
                )
                .child(
                    self.button("copy-command", "Copy command")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.snapshot.processes.iter().find(|p| p.key == key) {
                                cx.write_to_clipboard(ClipboardItem::new_string(p.command.clone()));
                            }
                        })),
                );
            let mut actions = div().flex().flex_col().flex_shrink_0().gap_3().child(
                div()
                    .text_size(px(11.))
                    .text_color(color(t.muted))
                    .child("Signals affect this process and may interrupt work."),
            );
            for (action, title) in [
                (ProcessAction::Terminate, "Quit process"),
                (ProcessAction::Kill, "Force quit"),
                (ProcessAction::Stop, "Pause process"),
                (ProcessAction::Continue, "Resume process"),
            ] {
                actions = actions.child(
                    self.button(title, title)
                        .flex_shrink_0()
                        .when(matches!(action, ProcessAction::Kill), |s| {
                            s.text_color(color(t.danger))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.overlay = Some(Overlay::Confirm(key, action));
                            cx.notify();
                        })),
                );
            }
            panel = panel
                .child(
                    div()
                        .id("inspector-details-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(details),
                )
                .child(actions);
        } else {
            panel = panel.child(label(
                "This process has exited. Its PID may now belong to another process.",
                color(t.muted),
            ));
        }
        panel.into_any_element()
    }
    fn confirm(
        &self,
        key: ProcessKey,
        action: ProcessAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.preferences.theme;
        let name = self
            .snapshot
            .processes
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| format!("PID {}", key.pid));
        let title = match action {
            ProcessAction::Terminate => "Quit",
            ProcessAction::Kill => "Force quit",
            ProcessAction::Stop => "Pause",
            ProcessAction::Continue => "Resume",
        };
        div().absolute().inset_0().occlude().flex().items_center().justify_center().bg(black().opacity(0.6)).child(div().w(px(430.)).p_6().rounded(px(14.)).bg(color(t.surface)).child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child(format!("{title} {name}?"))).child(div().mt_3().text_size(px(12.)).text_color(color(t.muted)).child(match action{ProcessAction::Kill=>"This immediately ends the process. Unsaved work can be lost.",ProcessAction::Terminate=>"The process will receive a request to end. Unsaved work may be lost.",ProcessAction::Stop=>"The process will stop running until you resume it.",ProcessAction::Continue=>"The process will resume if it was paused."})).child(div().mt_6().flex().justify_end().gap_2().child(self.button("cancel-signal","Cancel").on_click(cx.listener(move |this,_,_,cx|{this.overlay=Some(Overlay::Inspect(key));this.needs_focus=true;cx.notify();}))).child(self.button("confirm-signal",title).bg(color(t.danger).opacity(0.17)).text_color(color(t.danger)).on_click(cx.listener(move |this,_,_,cx|{this.notice=match this.monitor.act(key,action){Ok(())=>Some("Sending signal…".into()),Err(error)=>Some(error)};this.overlay=Some(Overlay::Inspect(key));this.needs_focus=true;cx.notify();}))))).into_any_element()
    }
    fn status(&self) -> AnyElement {
        let t = &self.preferences.theme;
        div()
            .h(px(30.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_6()
            .border_t_1()
            .border_color(color(t.divider))
            .text_size(px(10.))
            .text_color(color(t.muted))
            .child(div().size(px(5.)).rounded_full().bg(if self.paused {
                color(t.gpu)
            } else {
                color(t.memory)
            }))
            .child(if self.paused {
                "Sampling paused"
            } else if self.snapshot.sequence == 0 {
                "Connecting"
            } else {
                "Live"
            })
            .child(format!("·  Every {}s", self.preferences.refresh_secs))
            .child(div().flex_1())
            .child(format!("Uptime {}", duration(self.snapshot.uptime_secs)))
            .child("·")
            .child(format!("{} processes", self.snapshot.processes.len()))
            .into_any_element()
    }
}
impl Render for App {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        self.graph_viewport = TraceViewport {
            end: self.graph_timeline.end_at(now),
            span: self.preferences.history_secs as f64,
        };
        let graph_page = match self.page {
            Page::Overview => self.preferences.sections.iter().any(|section| {
                section.visible
                    && matches!(
                        section.kind,
                        SectionKind::Processor
                            | SectionKind::Memory
                            | SectionKind::Graphics
                            | SectionKind::Network
                    )
            }),
            Page::Resource(kind) => matches!(
                kind,
                SectionKind::Processor
                    | SectionKind::Memory
                    | SectionKind::Graphics
                    | SectionKind::Network
            ),
            _ => false,
        };
        if !self.hidden && !self.paused && graph_page && self.graph_timeline.is_moving(now) {
            window.request_animation_frame();
        }
        if self.needs_focus {
            window.focus(&self.focus);
            self.needs_focus = false;
        }
        let t = &self.preferences.theme;
        let mut main = div().flex().flex_col().flex_1().min_w_0().h_full();
        let mut page = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(px(34.))
            .pt(px(34.))
            .pb_6()
            .child(self.header(cx));
        if let Some(notice) = &self.notice {
            page = page.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .mb_4()
                    .p_3()
                    .rounded(px(7.))
                    .bg(color(t.accent).opacity(0.1))
                    .text_size(px(11.))
                    .child(div().flex_1().child(notice.clone()))
                    .child(
                        div()
                            .id("dismiss-notice")
                            .cursor_pointer()
                            .child(Icon::new(IconName::Close).size(px(13.)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notice = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        page = match self.page {
            Page::Processes => page.child(self.processes(cx)),
            Page::Overview => page.child(
                div()
                    .id("overview-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.overview(cx)),
            ),
            Page::Appearance => page.child(
                div()
                    .id("appearance-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.appearance(cx)),
            ),
            Page::Resource(kind) => {
                let section = self
                    .preferences
                    .sections
                    .iter()
                    .find(|s| s.kind == kind)
                    .cloned()
                    .unwrap_or_else(|| SectionConfig::new(kind));
                page.child(
                    div()
                        .id("resource-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(self.section(&section, true, cx)),
                )
            }
        };
        main = main
            .child(
                page.with_animation(
                    ("page-transition", self.transition),
                    Animation::new(Duration::from_millis(180))
                        .with_easing(|value| 1. - (1. - value).powi(3)),
                    |element, progress| element.opacity(0.86 + 0.14 * progress),
                ),
            )
            .child(self.status());
        let mut root = div()
            .id("sysly")
            .key_context("Sysly")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .bg(color(t.background))
            .text_color(color(t.text))
            .font_family(self.font_family.clone())
            .text_size(px(t.font_size))
            .child(self.sidebar(cx))
            .child(main);
        if let Some(overlay) = self.overlay.clone() {
            let key = match overlay {
                Overlay::Inspect(key) | Overlay::Confirm(key, _) => key,
            };
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .flex()
                    .justify_end()
                    .bg(black().opacity(0.28))
                    .child(self.inspector(key, cx)),
            );
            if let Overlay::Confirm(key, action) = overlay {
                root = root.child(self.confirm(key, action, cx));
            }
        }
        root.on_action(cx.listener(|this, _: &Overview, _, cx| this.navigate(Page::Overview, cx)))
            .on_action(cx.listener(|this, _: &Processes, _, cx| this.navigate(Page::Processes, cx)))
            .on_action(
                cx.listener(|this, _: &Appearance, _, cx| this.navigate(Page::Appearance, cx)),
            )
            .on_action(cx.listener(|this, _: &Search, window, cx| {
                this.navigate(Page::Processes, cx);
                let input = this.inputs[&Edit::Search].clone();
                input.update(cx, |input, cx| input.focus(window, cx));
                this.needs_focus = false;
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.overlay = match this.overlay {
                    Some(Overlay::Confirm(key, _)) => Some(Overlay::Inspect(key)),
                    _ => None,
                };
                window.focus(&this.focus);
                cx.notify();
            }))
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some(resize) = &this.resize
                    && let Some(section) = this
                        .preferences
                        .sections
                        .iter_mut()
                        .find(|s| s.kind == resize.kind)
                {
                    section.graph_height =
                        (resize.height + f32::from(event.position.y - resize.y)).clamp(40., 240.);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.resize.take().is_some() {
                        this.save(cx);
                    }
                }),
            )
            .when(self.snapshot.sequence == 0, |s| s)
    }
}
#[derive(Clone, Copy)]
enum FileAction {
    ImportTheme,
    ExportTheme,
    ImportLayout,
    ExportLayout,
}
fn color(c: [u8; 3]) -> Hsla {
    rgb((c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).into()
}
fn parse_color(value: &str) -> Option<[u8; 3]> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}
fn theme_colors(t: &Theme) -> [(&'static str, [u8; 3]); 11] {
    [
        ("Background", t.background),
        ("Surface", t.surface),
        ("Text", t.text),
        ("Secondary text", t.muted),
        ("Dividers", t.divider),
        ("Accent", t.accent),
        ("Processor", t.cpu),
        ("Memory", t.memory),
        ("Graphics", t.gpu),
        ("Network", t.network),
        ("Destructive actions", t.danger),
    ]
}
fn set_theme_color(t: &mut Theme, i: usize, c: [u8; 3]) {
    match i {
        0 => t.background = c,
        1 => t.surface = c,
        2 => t.text = c,
        3 => t.muted = c,
        4 => t.divider = c,
        5 => t.accent = c,
        6 => t.cpu = c,
        7 => t.memory = c,
        8 => t.gpu = c,
        9 => t.network = c,
        10 => t.danger = c,
        _ => {}
    }
}
fn label(text: impl Into<SharedString>, c: Hsla) -> AnyElement {
    div()
        .text_size(px(11.))
        .text_color(c)
        .child(text.into())
        .into_any_element()
}
fn reading(text: impl Into<SharedString>, c: Hsla, size: f32) -> AnyElement {
    div()
        .text_size(px(size))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c)
        .child(text.into())
        .into_any_element()
}
fn legend(name: impl Into<SharedString>, value: impl Into<SharedString>, c: Hsla) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_size(px(10.)).text_color(c).child(name.into()))
        .child(
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::MEDIUM)
                .child(value.into()),
        )
        .into_any_element()
}
fn settings_heading(title: &str, description: &str, t: &Theme) -> AnyElement {
    div()
        .child(
            div()
                .text_size(px(17.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .mt_2()
                .text_size(px(12.))
                .text_color(color(t.muted))
                .child(description.to_string()),
        )
        .into_any_element()
}
fn setting_row(title: &str, control: impl IntoElement, t: &Theme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_6()
        .mt_4()
        .child(
            div()
                .w(px(160.))
                .text_size(px(12.))
                .text_color(color(t.muted))
                .child(title.to_string()),
        )
        .child(div().flex_1().max_w(px(650.)).child(control))
        .into_any_element()
}
fn table_value(value: String, width: f32, c: Hsla) -> AnyElement {
    div()
        .w(px(width))
        .flex_shrink_0()
        .font_family("Menlo")
        .text_size(px(11.))
        .text_color(c)
        .child(value)
        .into_any_element()
}
fn ratio(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.
    } else {
        used as f32 / total as f32 * 100.
    }
}
fn optional(value: Option<f32>, unit: &str) -> String {
    value
        .map(|v| format!("{v:.1}{unit}"))
        .unwrap_or_else(|| "Unavailable".into())
}
fn bytes(value: u64) -> String {
    let value = value as f64;
    if value >= 1073741824. {
        format!("{:.1} GB", value / 1073741824.)
    } else if value >= 1048576. {
        format!("{:.1} MB", value / 1048576.)
    } else if value >= 1024. {
        format!("{:.0} KB", value / 1024.)
    } else {
        format!("{value:.0} B")
    }
}
fn duration(seconds: u64) -> String {
    if seconds >= 86400 {
        format!("{}d {}h", seconds / 86400, seconds % 86400 / 3600)
    } else if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
    } else {
        format!("{}m", seconds / 60)
    }
}
fn short_title(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Processor => "Processor",
        SectionKind::Memory => "Memory",
        SectionKind::Graphics => "Graphics",
        SectionKind::Network => "Network",
        SectionKind::Storage => "Storage",
        SectionKind::Battery => "Battery & power",
        SectionKind::Processes => "Processes",
    }
}
fn mark() -> AnyElement {
    static LOGO: std::sync::OnceLock<Arc<gpui::Image>> = std::sync::OnceLock::new();
    let image = LOGO.get_or_init(|| {
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            include_bytes!("../assets/brand/sysly-icon.png").to_vec(),
        ))
    });
    gpui::img(image.clone())
        .w(px(34.))
        .h(px(34.))
        .into_any_element()
}

fn process_rows(
    processes: &[Process],
    query: &str,
    sort: Sort,
    descending: bool,
    tree: bool,
) -> Vec<(usize, usize)> {
    let query = query.to_lowercase();
    let matches = |p: &Process| {
        query.is_empty()
            || p.name.to_lowercase().contains(&query)
            || p.user.to_lowercase().contains(&query)
            || p.key.pid.to_string().contains(&query)
    };
    let compare = |a: &usize, b: &usize| {
        let (a, b) = (&processes[*a], &processes[*b]);
        let order = match sort {
            Sort::Cpu => a.cpu_percent.total_cmp(&b.cpu_percent),
            Sort::Memory => a.memory.cmp(&b.memory),
            Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            Sort::Pid => a.key.pid.cmp(&b.key.pid),
            Sort::Io => {
                (a.read_per_sec + a.write_per_sec).total_cmp(&(b.read_per_sec + b.write_per_sec))
            }
        };
        (if descending { order.reverse() } else { order }).then(a.key.pid.cmp(&b.key.pid))
    };
    if !tree {
        let mut indices: Vec<_> = (0..processes.len())
            .filter(|&i| matches(&processes[i]))
            .collect();
        indices.sort_by(compare);
        return indices.into_iter().map(|i| (i, 0)).collect();
    }
    let by_pid: HashMap<_, _> = processes
        .iter()
        .enumerate()
        .map(|(i, p)| (p.key.pid, i))
        .collect();
    let mut keep = HashSet::new();
    for (i, p) in processes.iter().enumerate().filter(|(_, p)| matches(p)) {
        keep.insert(i);
        let mut parent = p.parent;
        let mut seen = HashSet::new();
        while let Some(pid) = parent {
            if !seen.insert(pid) {
                break;
            }
            if let Some(&index) = by_pid.get(&pid) {
                keep.insert(index);
                parent = processes[index].parent;
            } else {
                break;
            }
        }
    }
    let mut children: HashMap<Option<u32>, Vec<usize>> = HashMap::new();
    for &i in &keep {
        let parent = processes[i]
            .parent
            .filter(|pid| by_pid.get(pid).is_some_and(|i| keep.contains(i)));
        children.entry(parent).or_default().push(i);
    }
    for siblings in children.values_mut() {
        siblings.sort_by(compare);
    }
    let mut rows = Vec::with_capacity(keep.len());
    let mut visited = HashSet::new();
    fn walk(
        index: usize,
        depth: usize,
        processes: &[Process],
        children: &HashMap<Option<u32>, Vec<usize>>,
        visited: &mut HashSet<usize>,
        rows: &mut Vec<(usize, usize)>,
    ) {
        if !visited.insert(index) {
            return;
        }
        rows.push((index, depth.min(12)));
        if let Some(items) = children.get(&Some(processes[index].key.pid)) {
            for &child in items {
                walk(child, depth + 1, processes, children, visited, rows);
            }
        }
    }
    if let Some(roots) = children.get(&None) {
        for &root in roots {
            walk(root, 0, processes, &children, &mut visited, &mut rows);
        }
    }
    let mut remainder: Vec<_> = keep.into_iter().filter(|i| !visited.contains(i)).collect();
    remainder.sort_by(compare);
    for i in remainder {
        walk(i, 0, processes, &children, &mut visited, &mut rows);
    }
    rows
}

impl Drop for App {
    fn drop(&mut self) {
        if self.save_due.is_some() {
            let _ = self.store.save(&self.preferences);
        }
    }
}
fn trace(
    values: Vec<(f64, Option<f32>, bool)>,
    maximum: f32,
    viewport: TraceViewport,
    accent: Hsla,
    grid: Hsla,
    height: f32,
) -> AnyElement {
    canvas(
        move |_, _, _| (),
        move |bounds, _, window, _| {
            let mut base = PathBuilder::stroke(px(1.));
            base.move_to(point(bounds.left(), bounds.bottom()));
            base.line_to(point(bounds.right(), bounds.bottom()));
            if let Ok(path) = base.build() {
                window.paint_path(path, grid);
            }
            paint_trace(window, bounds, &values, maximum, viewport, accent, 1.5);
        },
    )
    .w_full()
    .h(px(height))
    .into_any_element()
}

fn playback_icon(paused: bool) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let x = bounds.left();
            let y = bounds.top();
            let mut path = PathBuilder::stroke(px(1.6));
            if paused {
                path.move_to(point(x + px(2.), y + px(1.)));
                path.line_to(point(x + px(9.), y + px(6.)));
                path.line_to(point(x + px(2.), y + px(11.)));
                path.close();
            } else {
                for offset in [3., 8.] {
                    path.move_to(point(x + px(offset), y + px(1.)));
                    path.line_to(point(x + px(offset), y + px(11.)));
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(0xa5b3c1));
            }
        },
    )
    .size(px(12.))
    .into_any_element()
}

fn move_section(preferences: &mut Preferences, source: SectionKind, before: Option<SectionKind>) {
    if before == Some(source) {
        return;
    }
    if let Some(index) = preferences.sections.iter().position(|s| s.kind == source) {
        let section = preferences.sections.remove(index);
        let target = before
            .and_then(|kind| preferences.sections.iter().position(|s| s.kind == kind))
            .unwrap_or(preferences.sections.len());
        preferences.sections.insert(target, section);
    }
}
fn trace_segments(values: &[(f64, Option<f32>, bool)]) -> Vec<Vec<(f64, f32)>> {
    let mut segments: Vec<Vec<(f64, f32)>> = Vec::new();
    let mut current = Vec::new();
    for &(time, value, gap) in values {
        if (gap || value.is_none()) && !current.is_empty() {
            segments.push(std::mem::take(&mut current));
        }
        if let Some(value) = value {
            current.push((time, value));
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}
fn clipped_trace_segments(
    values: &[(f64, Option<f32>, bool)],
    viewport: TraceViewport,
) -> Vec<Vec<(f64, f32)>> {
    let left = viewport.end - viewport.span;
    let mut clipped = Vec::new();
    for segment in trace_segments(values) {
        let mut visible = Vec::new();
        if segment.len() == 1 {
            if segment[0].0 >= left && segment[0].0 <= viewport.end {
                visible.push(segment[0]);
            }
        } else {
            for pair in segment.windows(2) {
                let (start, end) = (pair[0], pair[1]);
                if end.0 < left || start.0 > viewport.end || end.0 <= start.0 {
                    continue;
                }
                let interpolate = |time: f64| {
                    (
                        time,
                        start.1 + (end.1 - start.1) * ((time - start.0) / (end.0 - start.0)) as f32,
                    )
                };
                let start = interpolate(start.0.max(left));
                let end = interpolate(end.0.min(viewport.end));
                if visible.last() != Some(&start) {
                    visible.push(start);
                }
                if visible.last() != Some(&end) {
                    visible.push(end);
                }
            }
        }
        if !visible.is_empty() {
            clipped.push(visible);
        }
    }
    clipped
}

fn network_maximum(history: &VecDeque<History>, viewport: TraceViewport) -> f64 {
    [true, false]
        .into_iter()
        .flat_map(|received| {
            let values: Vec<_> = history
                .iter()
                .map(|h| {
                    (
                        h.time,
                        if received { h.received } else { h.sent }.map(|v| v as f32),
                        h.break_before,
                    )
                })
                .collect();
            clipped_trace_segments(&values, viewport)
                .into_iter()
                .flatten()
                .map(|(_, value)| value as f64)
        })
        .fold(1024., f64::max)
}

fn paint_trace(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    values: &[(f64, Option<f32>, bool)],
    maximum: f32,
    viewport: TraceViewport,
    accent: Hsla,
    width: f32,
) {
    let mut line = PathBuilder::stroke(px(width));
    for segment in clipped_trace_segments(values, viewport) {
        for (index, (time, value)) in segment.into_iter().enumerate() {
            let point = point(
                bounds.left()
                    + bounds.size.width
                        * ((time - viewport.end + viewport.span) / viewport.span) as f32,
                bounds.bottom() - bounds.size.height * (value / maximum).clamp(0., 1.),
            );
            if index == 0 {
                line.move_to(point);
            } else {
                line.line_to(point);
            }
        }
    }
    if let Ok(path) = line.build() {
        window.paint_path(path, accent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn process(pid: u32, parent: Option<u32>, name: &str, cpu: f32) -> Process {
        Process {
            key: ProcessKey { pid, started_at: 1 },
            parent,
            name: name.into(),
            user: "tester".into(),
            command: name.into(),
            status: "Running".into(),
            cpu_percent: cpu,
            memory: 0,
            virtual_memory: 0,
            read_per_sec: 0.0,
            write_per_sec: 0.0,
            run_secs: 1,
        }
    }

    #[test]
    fn timeline_moves_linearly_and_accepts_samples_without_jumping() {
        let now = Instant::now();
        let mut timeline = GraphTimeline::new(now);
        timeline.snap(10., now);
        timeline.advance_to(12., 2., now);
        assert_eq!(timeline.end_at(now), 10.);
        assert_eq!(timeline.end_at(now + Duration::from_millis(500)), 10.5);
        let next = now + Duration::from_secs(1);
        assert_eq!(timeline.end_at(next), 11.);
        timeline.advance_to(13., 1., next);
        assert_eq!(timeline.end_at(next), 11.);
        assert_eq!(timeline.end_at(next + Duration::from_millis(500)), 12.);
        assert_eq!(timeline.end_at(next + Duration::from_secs(2)), 13.);
        assert!(!timeline.is_moving(next + Duration::from_secs(2)));
    }

    #[test]
    fn timeline_freezes_and_reanchors_after_a_pause() {
        let now = Instant::now();
        let mut timeline = GraphTimeline::new(now);
        timeline.snap(10., now);
        timeline.advance_to(12., 2., now);
        timeline.freeze(now + Duration::from_secs(1));
        assert_eq!(timeline.end_at(now + Duration::from_secs(30)), 11.);
        assert!(!timeline.is_moving(now + Duration::from_secs(30)));
        timeline.snap(40., now + Duration::from_secs(30));
        assert_eq!(timeline.end_at(now + Duration::from_secs(30)), 40.);
        timeline.advance_to(42., 2., now + Duration::from_secs(32));
        assert_eq!(timeline.end_at(now + Duration::from_secs(33)), 41.);
    }

    #[test]
    fn trace_interpolates_both_boundaries_and_excludes_outside_samples() {
        let values = [
            (0., Some(0.), false),
            (10., Some(100.), false),
            (20., Some(0.), false),
            (30., Some(100.), false),
        ];
        assert_eq!(
            clipped_trace_segments(
                &values,
                TraceViewport {
                    end: 15.,
                    span: 10.
                }
            ),
            vec![vec![(5., 50.), (10., 100.), (15., 50.)]],
        );
        assert!(
            clipped_trace_segments(
                &values,
                TraceViewport {
                    end: 50.,
                    span: 10.
                }
            )
            .is_empty()
        );
        assert!(
            clipped_trace_segments(
                &values,
                TraceViewport {
                    end: -1.,
                    span: 10.
                }
            )
            .is_empty()
        );
    }

    #[test]
    fn clipped_trace_does_not_interpolate_across_gaps() {
        let values = [
            (0., Some(0.), false),
            (10., Some(100.), true),
            (20., None, false),
            (30., Some(50.), false),
        ];
        assert!(clipped_trace_segments(&values, TraceViewport { end: 9., span: 8. }).is_empty());
        assert!(
            clipped_trace_segments(
                &values,
                TraceViewport {
                    end: 29.,
                    span: 18.
                }
            )
            .is_empty()
        );
        assert_eq!(
            clipped_trace_segments(
                &values,
                TraceViewport {
                    end: 30.,
                    span: 30.
                }
            ),
            vec![vec![(0., 0.)], vec![(10., 100.)], vec![(30., 50.)]],
        );
    }

    #[test]
    fn startup_keeps_the_configured_time_scale() {
        let viewport = TraceViewport { end: 1., span: 60. };
        let segments =
            clipped_trace_segments(&[(0., Some(20.), false), (1., Some(30.), false)], viewport);
        let positions: Vec<_> = segments[0]
            .iter()
            .map(|(time, _)| (time - viewport.end + viewport.span) / viewport.span)
            .collect();
        assert_eq!(positions, vec![59. / 60., 1.]);
    }

    #[test]
    fn network_scale_reveals_peaks_continuously_for_both_series() {
        let history: VecDeque<_> = [(0., 0., 0.), (10., 4096., 8192.), (20., 0., 0.)]
            .into_iter()
            .map(|(time, received, sent)| History {
                time,
                cpu: 0.,
                memory: 0.,
                gpu: None,
                received: Some(received),
                sent: Some(sent),
                cores: vec![],
                swap: 0.,
                break_before: false,
            })
            .collect();
        assert_eq!(
            network_maximum(&history, TraceViewport { end: 0., span: 60. }),
            1024.
        );
        assert_eq!(
            network_maximum(&history, TraceViewport { end: 5., span: 60. }),
            4096.
        );
        assert_eq!(
            network_maximum(
                &history,
                TraceViewport {
                    end: 10.,
                    span: 60.
                }
            ),
            8192.
        );
        assert_eq!(
            network_maximum(&history, TraceViewport { end: 20., span: 5. }),
            4096.
        );
    }

    #[test]
    fn tree_retains_ancestry_and_sorts_siblings() {
        let processes = vec![
            process(1, None, "root", 0.0),
            process(2, Some(1), "worker", 5.0),
            process(3, Some(1), "worker", 25.0),
            process(4, Some(2), "child", 50.0),
        ];
        assert_eq!(
            process_rows(&processes, "", Sort::Cpu, true, true),
            vec![(0, 0), (2, 1), (1, 1), (3, 2)]
        );
        assert_eq!(
            process_rows(&processes, "child", Sort::Cpu, true, true),
            vec![(0, 0), (1, 1), (3, 2)]
        );
        assert_eq!(
            process_rows(&processes, "4", Sort::Pid, true, false),
            vec![(3, 0)]
        );
    }

    #[test]
    fn cyclic_parents_remain_finite_and_visible() {
        let processes = vec![
            process(1, Some(2), "first", 0.0),
            process(2, Some(1), "second", 0.0),
        ];
        assert_eq!(
            process_rows(&processes, "second", Sort::Pid, false, true),
            vec![(0, 0), (1, 1)]
        );
    }
    #[test]
    fn trace_leaves_gaps_for_pauses_and_unavailable_telemetry() {
        assert_eq!(
            trace_segments(&[
                (0., Some(25.), false),
                (1., Some(50.), false),
                (2., Some(75.), true),
                (3., Some(90.), false),
                (4., None, false),
                (5., Some(10.), false)
            ]),
            vec![
                vec![(0., 25.), (1., 50.)],
                vec![(2., 75.), (3., 90.)],
                vec![(5., 10.)]
            ]
        );
    }
    #[test]
    fn drag_places_section_before_target_without_duplicates() {
        let mut preferences = Preferences::default();
        move_section(
            &mut preferences,
            SectionKind::Processor,
            Some(SectionKind::Storage),
        );
        assert_eq!(preferences.sections[3].kind, SectionKind::Processor);
        assert_eq!(preferences.sections[4].kind, SectionKind::Storage);
        assert_eq!(preferences.sections.len(), 7);
        move_section(
            &mut preferences,
            SectionKind::Processor,
            Some(SectionKind::Processor),
        );
        assert_eq!(preferences.sections[3].kind, SectionKind::Processor);
    }
}

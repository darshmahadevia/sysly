mod chart;
mod chrome;
mod detail;
mod overview;
mod processes;
mod settings;
mod style;

use crate::{
    icons::{self, Icons},
    insight::{self, Consumer, Signals, Verdict},
    model::{Process, ProcessAction, ProcessKey, Snapshot},
    monitor::Monitor,
    preferences::{
        self, FontChoice, HISTORY_RANGES, MenuBarReading, Preferences, SectionKind, Store, Theme,
    },
    tray::{Tray, TrayAction},
};
use chart::ChartRegistry;
use gpui::{
    Animation, AnimationExt, Context, Entity, FocusHandle, Focusable, KeyBinding, Menu, MenuItem,
    MouseButton, MouseMoveEvent, OsAction, Pixels, PromptButton, PromptLevel, ScrollStrategy,
    SharedString, Subscription, SystemMenuType, Timer, UniformListScrollHandle, Window,
    WindowBounds, actions, div, prelude::*, px,
};
use gpui_component::{
    Sizable,
    input::{Input, InputEvent, InputState},
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use style::{Ui, is_dark};

actions!(
    sysly,
    [
        Overview,
        Processes,
        Appearance,
        ShowProcessor,
        ShowMemory,
        ShowGraphics,
        ShowNetwork,
        ShowStorage,
        ShowBattery,
        Search,
        Escape,
        Quit,
        TogglePause,
        ToggleCustomize,
        SelectNext,
        SelectPrevious,
        CloseWindow,
        Minimize,
        Zoom,
        HideApp,
        HideOthers,
        ShowAll,
        About,
        OpenHelp,
    ]
);

pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        KeyBinding::new("cmd-1", Overview, None),
        KeyBinding::new("cmd-2", Processes, None),
        KeyBinding::new("cmd-3", ShowProcessor, None),
        KeyBinding::new("cmd-4", ShowMemory, None),
        KeyBinding::new("cmd-5", ShowGraphics, None),
        KeyBinding::new("cmd-6", ShowNetwork, None),
        KeyBinding::new("cmd-7", ShowStorage, None),
        KeyBinding::new("cmd-8", ShowBattery, None),
        KeyBinding::new("cmd-,", Appearance, None),
        KeyBinding::new("cmd-f", Search, None),
        KeyBinding::new("cmd-p", TogglePause, None),
        KeyBinding::new("cmd-e", ToggleCustomize, None),
        KeyBinding::new("escape", Escape, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("cmd-h", HideApp, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("down", SelectNext, Some("Sysly && !Input")),
        KeyBinding::new("up", SelectPrevious, Some("Sysly && !Input")),
    ]);
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &OpenHelp, cx| cx.open_url("https://github.com/darshmahadevia/sysly#readme"));
    cx.on_action(|_: &About, _| show_about_panel());
    use gpui_component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
    cx.set_menus(vec![
        Menu {
            name: "Sysly".into(),
            items: vec![
                MenuItem::action("About Sysly", About),
                MenuItem::separator(),
                MenuItem::action("Settings…", Appearance),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide Sysly", HideApp),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit Sysly", Quit),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", Undo, OsAction::Undo),
                MenuItem::os_action("Redo", Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
                MenuItem::separator(),
                MenuItem::action("Find Process", Search),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Overview", Overview),
                MenuItem::action("Apps & Processes", Processes),
                MenuItem::separator(),
                MenuItem::action("Processor", ShowProcessor),
                MenuItem::action("Memory", ShowMemory),
                MenuItem::action("Graphics", ShowGraphics),
                MenuItem::action("Network", ShowNetwork),
                MenuItem::action("Storage", ShowStorage),
                MenuItem::action("Battery & Power", ShowBattery),
                MenuItem::separator(),
                MenuItem::action("Pause or Resume Sampling", TogglePause),
                MenuItem::action("Customize Overview", ToggleCustomize),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::separator(),
                MenuItem::action("Close", CloseWindow),
            ],
        },
        Menu {
            name: "Help".into(),
            items: vec![MenuItem::action("Sysly Help", OpenHelp)],
        },
    ]);
}

fn show_about_panel() {
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    unsafe {
        let application: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
        let sender: Option<&AnyObject> = None;
        let _: () = msg_send![&*application, orderFrontStandardAboutPanel: sender];
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Overview,
    Processes,
    Appearance,
    Resource(SectionKind),
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Edit {
    Search,
    Title,
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProcessView {
    Apps,
    All,
    Tree,
}
/// One visible row in the process table.
#[derive(Clone, Copy)]
struct Row {
    process: usize,
    depth: usize,
    consumer: Option<usize>,
}
/// The row a person has open. A row is followed by process identity, so it stays
/// correct as sorting reorders the list.
#[derive(Clone, PartialEq)]
struct Selection {
    key: ProcessKey,
    bundle: Option<String>,
}
#[derive(Clone, Copy)]
enum Undo {
    Layout,
    SavedLayout,
}
struct Toast {
    message: String,
    error: bool,
    undo: Option<Undo>,
    until: Option<Instant>,
}
#[derive(Clone, Default)]
struct History {
    time: f64,
    cpu: f32,
    memory: f32,
    gpu: Option<f32>,
    received: Option<f64>,
    sent: Option<f64>,
    disk_read: Option<f64>,
    disk_write: Option<f64>,
    power: Option<f32>,
    cores: Arc<[f32]>,
    break_before: bool,
    /// Whatever was busiest at this moment, so the past can be questioned.
    busiest: Option<(Arc<str>, f32)>,
    /// Seconds since the Unix epoch, for showing a clock time.
    clock: f64,
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
    fn advance_to(&mut self, time: f64, interval: f64, smooth: bool, now: Instant) {
        if !smooth {
            self.snap(time, now);
            return;
        }
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
    title: SharedString,
    ui: Ui,
}
impl Render for DragSection {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.ui.p;
        div()
            .px_3()
            .py_2()
            .rounded(px(style::RADIUS_CONTROL))
            .bg(p.raised)
            .border_1()
            .border_color(p.accent)
            .text_color(p.text)
            .text_size(self.ui.fs(13.))
            .child(self.title.clone())
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
    icons: Icons,
    snapshot: Arc<Snapshot>,
    history: Arc<VecDeque<History>>,
    graph_timeline: GraphTimeline,
    graph_viewport: TraceViewport,
    preferences: Preferences,
    store: Store,
    page: Page,
    previous_page: Page,
    inputs: HashMap<Edit, Entity<InputState>>,
    subscriptions: Vec<Subscription>,
    paused: bool,
    hidden: bool,
    history_gap: bool,
    toast: Option<Toast>,
    sort: Sort,
    descending: bool,
    view: ProcessView,
    process_rows: Vec<Row>,
    consumers: Vec<Consumer>,
    top_cpu: Vec<usize>,
    top_memory: Vec<usize>,
    top_io: Vec<usize>,
    verdict: Verdict,
    selection: Option<Selection>,
    selection_history: VecDeque<(f32, f32)>,
    list_scroll: UniformListScrollHandle,
    pending_signal: Option<(String, ProcessKey, ProcessAction)>,
    /// A pause or resume that has been delivered but not yet seen in a sample, so
    /// the inspector's button changes at once rather than at the next reading.
    signaled_stop: Option<(ProcessKey, bool, Instant)>,
    charts: ChartRegistry,
    cursor: Option<f64>,
    editing: bool,
    deleted_layout: Option<preferences::NamedLayout>,
    focus: FocusHandle,
    resize: Option<Resize>,
    font_family: SharedString,
    settings_tab: usize,
    needs_focus: bool,
    transition: u64,
    interface: String,
    save_due: Option<Instant>,
    reduce_motion: bool,
    dark_controls: Option<bool>,
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
            icons: Icons::start(),
            snapshot: Arc::default(),
            history: Arc::new(VecDeque::new()),
            graph_timeline: GraphTimeline::new(Instant::now()),
            graph_viewport: TraceViewport {
                end: 0.,
                span: preferences.history_secs as f64,
            },
            preferences,
            store,
            page: Page::Overview,
            previous_page: Page::Overview,
            inputs: HashMap::new(),
            subscriptions: vec![],
            paused: false,
            hidden: false,
            history_gap: false,
            toast: notice.map(|message| Toast {
                message,
                error: true,
                undo: None,
                until: None,
            }),
            sort: Sort::Cpu,
            descending: true,
            view: ProcessView::Apps,
            process_rows: vec![],
            consumers: vec![],
            top_cpu: vec![],
            top_memory: vec![],
            top_io: vec![],
            verdict: insight::assess(&Signals::default()),
            selection: None,
            selection_history: VecDeque::new(),
            list_scroll: UniformListScrollHandle::new(),
            pending_signal: None,
            signaled_stop: None,
            charts: Rc::new(RefCell::new(Vec::new())),
            cursor: None,
            editing: false,
            deleted_layout: None,
            focus: cx.focus_handle(),
            resize: None,
            font_family: ".AppleSystemUIFont".into(),
            settings_tab: 0,
            needs_focus: true,
            transition: 0,
            interface: String::new(),
            save_due: None,
            reduce_motion: icons::reduce_motion(),
            dark_controls: None,
        };
        this.follow_system_appearance(window);
        this.create_inputs(window, cx);
        this.load_font(cx);
        this.sync_control_theme(cx);
        window.focus(&this.focus);
        cx.observe_window_appearance(window, |this, window, cx| {
            if this.preferences.follow_system {
                this.follow_system_appearance(window);
                this.create_inputs(window, cx);
                this.save(cx);
            }
        })
        .detach();
        let entity = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            entity
                .update(cx, |this, cx| {
                    if this.tray.is_some() {
                        this.hide_to_menu_bar(cx);
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
    fn hide_to_menu_bar(&mut self, cx: &mut Context<Self>) {
        self.hidden = true;
        self.monitor.set_visible(false);
        cx.hide();
    }
    fn follow_system_appearance(&mut self, window: &Window) {
        if !self.preferences.follow_system {
            return;
        }
        let dark = matches!(
            window.appearance(),
            gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
        );
        let theme = if dark { Theme::dark() } else { Theme::light() };
        let current = &self.preferences.theme;
        self.preferences.theme = Theme {
            font: current.font,
            font_size: current.font_size,
            ..theme
        };
    }
    fn ui(&self) -> Ui {
        Ui::new(
            &self.preferences.theme,
            self.preferences.density,
            self.font_family.clone(),
        )
    }
    fn create_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let search = self
            .inputs
            .get(&Edit::Search)
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default();
        self.inputs.clear();
        self.subscriptions.clear();
        let mut fields = vec![
            (Edit::Search, search, "Search by name, PID or user"),
            (
                Edit::Title,
                self.preferences.dashboard_title.clone(),
                "Dashboard title",
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
            Edit::ThemeName => {
                if value == self.preferences.theme.name {
                    return;
                }
                self.preferences.follow_system = false;
                self.preferences.theme.name = value;
            }
            Edit::Color(index) => {
                let Some(color) = parse_color(&value) else {
                    return;
                };
                if theme_colors(&self.preferences.theme)[index].1 == color {
                    return;
                }
                self.preferences.follow_system = false;
                set_theme_color(&mut self.preferences.theme, index, color);
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
        self.sync_control_theme(cx);
        self.update_tray();
        cx.notify();
    }
    fn sync_control_theme(&mut self, cx: &mut Context<Self>) {
        let theme = &self.preferences.theme;
        let dark = is_dark(theme);
        if self.dark_controls != Some(dark) {
            self.dark_controls = Some(dark);
            gpui_component::Theme::change(
                if dark {
                    gpui_component::ThemeMode::Dark
                } else {
                    gpui_component::ThemeMode::Light
                },
                None,
                cx,
            );
        }
        let palette = style::Palette::new(theme);
        let ui = gpui_component::Theme::global_mut(cx);
        ui.colors.background = palette.surface;
        ui.colors.foreground = palette.text;
        ui.colors.input = palette.line;
        ui.colors.border = palette.line;
        ui.colors.muted = palette.hover;
        ui.colors.muted_foreground = palette.muted;
        ui.colors.accent = palette.hover;
        ui.colors.accent_foreground = palette.text;
        ui.colors.primary = palette.accent;
        ui.colors.ring = palette.accent;
        ui.colors.caret = palette.accent;
        ui.colors.selection = palette.accent.opacity(0.3);
        ui.colors.popover = palette.raised;
        ui.colors.popover_foreground = palette.text;
        ui.colors.switch = palette.line;
        ui.colors.switch_thumb = gpui::white();
        ui.colors.scrollbar = gpui::transparent_black();
        ui.colors.scrollbar_thumb = palette.text.opacity(0.22);
        ui.colors.scrollbar_thumb_hover = palette.text.opacity(0.35);
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
                Err(error) => self.show_error(format!("Font unavailable. {error}")),
            }
        }
    }
    fn show_toast(&mut self, message: impl Into<String>, undo: Option<Undo>) {
        self.toast = Some(Toast {
            message: message.into(),
            error: false,
            undo,
            until: Some(Instant::now() + Duration::from_secs(if undo.is_some() { 8 } else { 4 })),
        });
    }
    fn show_error(&mut self, message: impl Into<String>) {
        self.toast = Some(Toast {
            message: message.into(),
            error: true,
            undo: None,
            until: None,
        });
    }
    fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        if self
            .toast
            .as_ref()
            .is_some_and(|toast| toast.until.is_some_and(|until| Instant::now() >= until))
        {
            self.toast = None;
            changed = true;
        }
        self.remember_window(window);
        if self.save_due.is_some_and(|due| Instant::now() >= due) {
            self.save_due = None;
            if let Err(error) = self.store.save(&self.preferences) {
                self.show_error(error);
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
        if self.cursor.is_some() && !self.pointer_on_chart(window.mouse_position()) {
            self.cursor = None;
            changed = true;
        }
        let next = self.monitor.latest();
        if next.sequence != self.snapshot.sequence {
            self.record(next, cx);
            changed = true;
        }
        if let Some(result) = self.monitor.take_action_result() {
            let pending = self.pending_signal.take();
            match (result, pending) {
                (Ok(()), Some((name, key, action))) => {
                    let lasts = Duration::from_secs(self.preferences.refresh_secs + 1);
                    if let ProcessAction::Stop | ProcessAction::Continue = action {
                        let stopped = matches!(action, ProcessAction::Stop);
                        self.signaled_stop = Some((key, stopped, Instant::now() + lasts));
                    }
                    self.show_toast(
                        match action {
                            ProcessAction::Terminate => format!("Asked {name} to quit"),
                            ProcessAction::Kill => format!("Force quit {name}"),
                            ProcessAction::Stop => format!("Paused {name}"),
                            ProcessAction::Continue => format!("Resumed {name}"),
                        },
                        None,
                    )
                }
                (Ok(()), None) => {}
                (Err(error), _) => self.show_error(error),
            }
            changed = true;
        }
        if self.icons.drain() {
            changed = true;
        }
        if changed {
            self.update_tray();
            cx.notify();
        }
    }
    /// Takes in a new sample: everything derived from it is computed here, once,
    /// so that rendering stays a pure read.
    fn record(&mut self, next: Arc<Snapshot>, cx: &mut Context<Self>) {
        if self.interface.is_empty() {
            self.interface = primary_interface(&next);
        }
        let network = next.networks.iter().find(|n| n.name == self.interface);
        // The first sample only establishes counter baselines, so its rates are not
        // measurements and must not be charted as zero.
        let measured = !self.history.is_empty();
        let received = network.filter(|_| measured).map(|n| n.received_per_sec);
        let sent = network.filter(|_| measured).map(|n| n.sent_per_sec);
        let startup = startup_disk(&next).filter(|_| measured);
        let now = Instant::now();
        let gap = std::mem::take(&mut self.history_gap);
        if !self.paused {
            if self.history.is_empty() || self.hidden || gap {
                self.graph_timeline.snap(next.elapsed_secs, now);
            } else {
                self.graph_timeline.advance_to(
                    next.elapsed_secs,
                    next.elapsed_secs - self.snapshot.elapsed_secs,
                    self.preferences.smooth_charts,
                    now,
                );
            }
        }
        self.consumers = insight::consumers(&next.processes);
        self.top_cpu = top_consumers(&self.consumers, |c| c.cpu as f64);
        self.top_memory = top_consumers(&self.consumers, |c| c.memory as f64);
        self.top_io = top_consumers(&self.consumers, |c| c.io);
        for consumer in &self.consumers {
            if let Some(bundle) = &consumer.bundle {
                self.icons.get(bundle);
            }
        }
        let busiest = self
            .top_cpu
            .first()
            .map(|&i| &self.consumers[i])
            .filter(|consumer| consumer.cpu >= 1.)
            .map(|consumer| (Arc::from(consumer.name.as_str()), consumer.cpu));
        Arc::make_mut(&mut self.history).push_back(History {
            time: next.elapsed_secs,
            cpu: next.cpu_percent,
            memory: ratio(next.memory.used, next.memory.total),
            gpu: next.hardware.gpu_percent,
            received,
            sent,
            disk_read: startup.and_then(|d| d.read_per_sec),
            disk_write: startup.and_then(|d| d.write_per_sec),
            power: next.hardware.system_watts,
            cores: next.cores.as_slice().into(),
            break_before: gap,
            busiest,
            clock: unix_now(),
        });
        self.snapshot = next;
        self.trim_history();
        self.verdict = insight::assess(&self.signals());
        if let Some(selection) = &self.selection
            && let Some(process) = self
                .snapshot
                .processes
                .iter()
                .find(|p| p.key == selection.key)
        {
            let (cpu, memory) = match self.selected_consumer() {
                Some(consumer) => (consumer.cpu, consumer.memory),
                None => (process.cpu_percent, process.memory),
            };
            self.selection_history.push_back((cpu, memory as f32));
            while self.selection_history.len() > 120 {
                self.selection_history.pop_front();
            }
        }
        self.update_process_rows(cx);
    }
    fn signals(&self) -> Signals {
        let s = &self.snapshot;
        let cutoff = s.elapsed_secs - 30.;
        let recent: Vec<f32> = self
            .history
            .iter()
            .rev()
            .take_while(|h| h.time >= cutoff)
            .map(|h| h.cpu)
            .collect();
        let startup = startup_disk(s).filter(|d| d.total > 0);
        let named = |index: Option<&usize>| {
            index.map(|&i| {
                let consumer = &self.consumers[i];
                (
                    s.processes[consumer.lead].key,
                    consumer.name.clone(),
                    consumer,
                )
            })
        };
        Signals {
            ready: s.sequence > 0,
            cpu_recent: recent.iter().sum::<f32>() / recent.len().max(1) as f32,
            pressure: s.memory.pressure,
            thermal: s.thermal,
            battery_percent: s.battery.as_ref().map(|b| b.percent),
            on_battery: s
                .battery
                .as_ref()
                .is_some_and(|b| b.state.eq_ignore_ascii_case("discharging")),
            startup_free_ratio: startup.map(|d| d.available as f32 / d.total as f32),
            startup_free: startup.map(|d| decimal_bytes(d.available)),
            top_cpu: named(self.top_cpu.first()).map(|(k, n, c)| (k, n, c.cpu)),
            top_memory: named(self.top_memory.first()).map(|(k, n, c)| (k, n, bytes(c.memory))),
        }
    }
    fn remember_window(&mut self, window: &Window) {
        if let WindowBounds::Windowed(bounds) = window.window_bounds() {
            let next = [
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            ];
            if self.preferences.window != Some(next) {
                self.preferences.window = Some(next);
                self.save_due = Some(Instant::now() + Duration::from_millis(600));
            }
        }
    }
    fn update_tray(&self) {
        if let Some(tray) = &self.tray {
            let reading = match self.preferences.menu_bar_reading {
                MenuBarReading::Icon => None,
                MenuBarReading::Cpu => Some(format!("{:.0}%", self.snapshot.cpu_percent)),
                MenuBarReading::Memory => Some(format!(
                    "{:.0}%",
                    ratio(self.snapshot.memory.used, self.snapshot.memory.total)
                )),
            };
            tray.update(
                &self.snapshot,
                &self.interface,
                self.paused,
                reading,
                (&self.verdict.headline, &self.verdict.detail),
            );
        }
    }
    fn trim_history(&mut self) {
        let longest = HISTORY_RANGES[HISTORY_RANGES.len() - 1] as f64;
        let cutoff = self.graph_timeline.end_at(Instant::now()) - longest;
        while self.history.len() > 902 || (self.history.len() > 2 && self.history[1].time < cutoff)
        {
            Arc::make_mut(&mut self.history).pop_front();
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
    fn query(&self, cx: &gpui::App) -> SharedString {
        self.inputs
            .get(&Edit::Search)
            .map(|input| input.read(cx).value())
            .unwrap_or_default()
    }
    fn update_process_rows(&mut self, cx: &gpui::App) {
        if self.page != Page::Processes {
            self.process_rows.clear();
            return;
        }
        let query = self.query(cx);
        self.process_rows = match self.view {
            ProcessView::Apps => consumer_rows(
                &self.snapshot.processes,
                &self.consumers,
                query.as_ref(),
                self.sort,
                self.descending,
            )
            .into_iter()
            .map(|consumer| Row {
                process: self.consumers[consumer].lead,
                depth: 0,
                consumer: Some(consumer),
            })
            .collect(),
            view => process_rows(
                &self.snapshot.processes,
                query.as_ref(),
                self.sort,
                self.descending,
                view == ProcessView::Tree,
            )
            .into_iter()
            .map(|(process, depth)| Row {
                process,
                depth,
                consumer: None,
            })
            .collect(),
        };
    }
    fn selected_consumer(&self) -> Option<&Consumer> {
        let selection = self.selection.as_ref()?;
        let bundle = selection.bundle.as_ref()?;
        self.consumers
            .iter()
            .find(|c| c.bundle.as_ref() == Some(bundle))
    }
    fn select_row(&mut self, row: Row, cx: &mut Context<Self>) {
        let process = &self.snapshot.processes[row.process];
        let selection = Selection {
            key: process.key,
            bundle: row
                .consumer
                .and_then(|consumer| self.consumers[consumer].bundle.clone()),
        };
        self.select(selection, cx);
    }
    fn select(&mut self, selection: Selection, cx: &mut Context<Self>) {
        if self.selection.as_ref() != Some(&selection) {
            self.selection_history.clear();
        }
        self.selection = Some(selection);
        cx.notify();
    }
    /// Opens a process from anywhere, by way of its row in the process table.
    fn open_process(&mut self, key: ProcessKey, cx: &mut Context<Self>) {
        let bundle = self
            .consumers
            .iter()
            .find(|c| self.snapshot.processes[c.lead].key == key)
            .and_then(|c| c.bundle.clone());
        self.navigate(Page::Processes, cx);
        self.select(Selection { key, bundle }, cx);
        self.reveal_selection();
    }
    fn selected_row(&self) -> Option<usize> {
        let selection = self.selection.as_ref()?;
        self.process_rows.iter().position(|row| {
            self.snapshot.processes[row.process].key == selection.key
                || (selection.bundle.is_some()
                    && row.consumer.and_then(|c| self.consumers[c].bundle.as_ref())
                        == selection.bundle.as_ref())
        })
    }
    fn reveal_selection(&self) {
        if let Some(row) = self.selected_row() {
            self.list_scroll.scroll_to_item(row, ScrollStrategy::Center);
        }
    }
    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.page != Page::Processes || self.process_rows.is_empty() {
            return;
        }
        let next = match self.selected_row() {
            Some(row) => row
                .saturating_add_signed(delta)
                .min(self.process_rows.len() - 1),
            None => 0,
        };
        self.select_row(self.process_rows[next], cx);
        self.list_scroll.scroll_to_item(next, ScrollStrategy::Top);
    }
    fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page != page {
            self.transition += 1;
            self.previous_page = self.page;
        }
        self.page = page;
        self.cursor = None;
        if page != Page::Overview {
            self.editing = false;
        }
        self.update_process_rows(cx);
        self.needs_focus = true;
        cx.notify();
    }
    /// A native sheet, because signals are the one consequential thing Sysly does.
    fn confirm_signal(
        &mut self,
        key: ProcessKey,
        name: String,
        action: ProcessAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (title, detail, confirm, level) = match action {
            ProcessAction::Terminate => (
                format!("Quit {name}?"),
                "It will be asked to close. Unsaved work may be lost.",
                "Quit",
                PromptLevel::Warning,
            ),
            ProcessAction::Kill => (
                format!("Force quit {name}?"),
                "It will stop immediately, without saving. Unsaved work will be lost.",
                "Force Quit",
                PromptLevel::Critical,
            ),
            ProcessAction::Stop => (
                format!("Pause {name}?"),
                "It will stop running until you resume it, and may appear frozen.",
                "Pause",
                PromptLevel::Info,
            ),
            ProcessAction::Continue => (
                format!("Resume {name}?"),
                "It will carry on from where it stopped.",
                "Resume",
                PromptLevel::Info,
            ),
        };
        let answer = window.prompt(
            level,
            &title,
            Some(detail),
            &[PromptButton::ok(confirm), PromptButton::cancel("Cancel")],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                let _ = this.update(cx, |this, cx| {
                    match this.monitor.act(key, action) {
                        Ok(()) => this.pending_signal = Some((name, key, action)),
                        Err(error) => this.show_error(error),
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn input(&self, key: Edit) -> Input {
        Input::new(&self.inputs[&key])
            .small()
            .h(px(28.))
            .rounded(px(style::RADIUS_CONTROL))
    }
    fn pointer_on_chart(&self, position: gpui::Point<Pixels>) -> bool {
        self.charts.borrow().iter().any(|b| b.contains(&position))
    }
    fn scrub(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let bounds = self
            .charts
            .borrow()
            .iter()
            .find(|b| b.contains(&position))
            .copied();
        let next = bounds.and_then(|bounds| {
            let fraction = (position.x - bounds.left()) / bounds.size.width;
            let v = self.graph_viewport;
            nearest_sample(&self.history, v.end - v.span * (1. - fraction as f64))
        });
        if next != self.cursor {
            self.cursor = next;
            cx.notify();
        }
    }
    fn recorded_secs(&self) -> f64 {
        self.history
            .front()
            .zip(self.history.back())
            .map_or(0., |(first, last)| last.time - first.time)
    }
    fn sample_at_cursor(&self) -> Option<&History> {
        let time = self.cursor?;
        self.history.iter().find(|h| h.time == time)
    }
    fn undo(&mut self, undo: Undo, window: &mut Window, cx: &mut Context<Self>) {
        match undo {
            Undo::Layout => self.preferences.restore_previous_layout(),
            Undo::SavedLayout => {
                if let Some(layout) = self.deleted_layout.take() {
                    self.preferences.saved_layouts.push(layout);
                }
            }
        }
        self.toast = None;
        self.create_inputs(window, cx);
        self.save(cx);
    }
}
impl Render for App {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        self.charts.borrow_mut().clear();
        self.graph_viewport = TraceViewport {
            end: self.graph_timeline.end_at(now),
            span: effective_span(self.preferences.history_secs, self.recorded_secs()) as f64,
        };
        let graph_page = matches!(self.page, Page::Overview | Page::Resource(_));
        if self.preferences.smooth_charts
            && !self.hidden
            && !self.paused
            && graph_page
            && self.graph_timeline.is_moving(now)
        {
            window.request_animation_frame();
        }
        if self.needs_focus {
            window.focus(&self.focus);
            self.needs_focus = false;
        }
        let ui = self.ui();
        let p = ui.p;
        let page = match self.page {
            Page::Processes => self.processes(window, cx),
            Page::Overview => div()
                .id("overview-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(self.overview(cx))
                .into_any_element(),
            Page::Appearance => div()
                .id("settings-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(self.settings(cx))
                .into_any_element(),
            Page::Resource(kind) => div()
                .id(("resource-scroll", kind as usize))
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(self.resource(kind, cx))
                .into_any_element(),
        };
        let body = div().flex().flex_col().flex_1().min_h_0().child(page);
        let body = if self.reduce_motion {
            body.into_any_element()
        } else {
            body.with_animation(
                ("page-transition", self.transition),
                Animation::new(Duration::from_millis(160))
                    .with_easing(|value| 1. - (1. - value).powi(3)),
                |element, progress| element.opacity(0.7 + 0.3 * progress),
            )
            .into_any_element()
        };
        let main = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(p.bg)
            .child(self.toolbar(cx))
            .child(body);
        let mut root = div()
            .id("sysly")
            .key_context("Sysly")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .text_color(p.text)
            .font_family(self.font_family.clone())
            .text_size(ui.fs(13.))
            .line_height(gpui::relative(1.3))
            .child(self.sidebar(cx))
            .child(main);
        if self.toast.is_some() {
            root = root.child(self.toast_view(cx));
        }
        root.on_action(cx.listener(|this, _: &Overview, _, cx| this.navigate(Page::Overview, cx)))
            .on_action(cx.listener(|this, _: &Processes, _, cx| this.navigate(Page::Processes, cx)))
            .on_action(
                cx.listener(|this, _: &Appearance, _, cx| this.navigate(Page::Appearance, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowProcessor, _, cx| {
                this.navigate(Page::Resource(SectionKind::Processor), cx)
            }))
            .on_action(cx.listener(|this, _: &ShowMemory, _, cx| {
                this.navigate(Page::Resource(SectionKind::Memory), cx)
            }))
            .on_action(cx.listener(|this, _: &ShowGraphics, _, cx| {
                this.navigate(Page::Resource(SectionKind::Graphics), cx)
            }))
            .on_action(cx.listener(|this, _: &ShowNetwork, _, cx| {
                this.navigate(Page::Resource(SectionKind::Network), cx)
            }))
            .on_action(cx.listener(|this, _: &ShowStorage, _, cx| {
                this.navigate(Page::Resource(SectionKind::Storage), cx)
            }))
            .on_action(cx.listener(|this, _: &ShowBattery, _, cx| {
                this.navigate(Page::Resource(SectionKind::Battery), cx)
            }))
            .on_action(cx.listener(|this, _: &Search, window, cx| {
                this.navigate(Page::Processes, cx);
                let input = this.inputs[&Edit::Search].clone();
                input.update(cx, |input, cx| input.focus(window, cx));
                this.needs_focus = false;
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                if this.editing {
                    this.editing = false;
                } else if this.selection.is_some() {
                    this.selection = None;
                } else if this.page == Page::Processes && !this.query(cx).is_empty() {
                    let input = this.inputs[&Edit::Search].clone();
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                    this.update_process_rows(cx);
                }
                window.focus(&this.focus);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &TogglePause, _, cx| this.toggle_pause(cx)))
            .on_action(cx.listener(|this, _: &ToggleCustomize, _, cx| {
                this.navigate(Page::Overview, cx);
                this.editing = !this.editing;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &CloseWindow, _, cx| {
                if this.tray.is_some() {
                    this.hide_to_menu_bar(cx);
                } else {
                    cx.quit();
                }
            }))
            .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
            .on_action(cx.listener(|_, _: &Zoom, window, _| window.zoom_window()))
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some(resize) = &this.resize {
                    if let Some(section) = this
                        .preferences
                        .sections
                        .iter_mut()
                        .find(|s| s.kind == resize.kind)
                    {
                        section.graph_height = (resize.height
                            + f32::from(event.position.y - resize.y))
                        .clamp(40., 240.);
                        cx.notify();
                    }
                } else if !this.editing {
                    this.scrub(event.position, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.resize.take().is_some() {
                        this.save(cx);
                    }
                    // GPUI has no focus-visible state: a control takes focus when
                    // clicked, so hand it back to keep rings for keyboard users.
                    let typing = this
                        .inputs
                        .values()
                        .any(|input| input.read(cx).focus_handle(cx).is_focused(window));
                    if !typing {
                        window.focus(&this.focus);
                    }
                }),
            )
    }
}
#[derive(Clone, Copy)]
enum FileAction {
    ImportTheme,
    ExportTheme,
    ImportLayout,
    ExportLayout,
}
fn parse_color(value: &str) -> Option<[u8; 3]> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}
fn theme_colors(t: &Theme) -> [(&'static str, [u8; 3]); 13] {
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
        ("Storage", t.storage),
        ("Battery", t.battery),
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
        10 => t.storage = c,
        11 => t.battery = c,
        12 => t.danger = c,
        _ => {}
    }
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
        .unwrap_or_else(|| "Not available".into())
}
/// Memory and rates, in the binary units macOS reports memory with.
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
/// Storage, in the decimal units Finder shows, so the two always agree.
fn decimal_bytes(value: u64) -> String {
    let value = value as f64;
    if value >= 1e12 {
        format!("{:.2} TB", value / 1e12)
    } else if value >= 1e9 {
        format!("{:.0} GB", value / 1e9)
    } else if value >= 1e6 {
        format!("{:.0} MB", value / 1e6)
    } else {
        format!("{:.0} KB", value / 1e3)
    }
}
fn rate(value: f64) -> String {
    format!("{}/s", bytes(value.max(0.) as u64))
}
fn duration(seconds: u64) -> String {
    if seconds >= 86400 {
        format!("{}d {}h", seconds / 86400, seconds % 86400 / 3600)
    } else if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}
fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
/// Formats a Unix time as the local time of day.
fn clock_time(seconds: f64) -> String {
    let time = seconds as libc::time_t;
    let mut parts: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&raw const time, &raw mut parts) };
    format!(
        "{:02}:{:02}:{:02}",
        parts.tm_hour, parts.tm_min, parts.tm_sec
    )
}
fn primary_interface(snapshot: &Snapshot) -> String {
    snapshot
        .networks
        .iter()
        .find(|n| n.name == "en0")
        .or(snapshot.networks.first())
        .map(|n| n.name.clone())
        .unwrap_or_default()
}
fn startup_disk(snapshot: &Snapshot) -> Option<&crate::model::Disk> {
    snapshot
        .disks
        .iter()
        .find(|d| d.mount == "/")
        .or(snapshot.disks.first())
}
/// The span charts show. Until there is enough history to fill the chosen range,
/// charts use the shortest range that holds what has been recorded, so a new
/// window shows a full chart instead of a sliver at the right edge.
fn effective_span(selected: u32, recorded: f64) -> u32 {
    HISTORY_RANGES
        .iter()
        .copied()
        .find(|range| *range as f64 >= recorded)
        .unwrap_or(selected)
        .min(selected)
}
fn nearest_sample(history: &VecDeque<History>, time: f64) -> Option<f64> {
    let after = history.partition_point(|h| h.time < time);
    let later = history.get(after);
    let earlier = after.checked_sub(1).and_then(|i| history.get(i));
    match (earlier, later) {
        (Some(a), Some(b)) => Some(if time - a.time < b.time - time {
            a.time
        } else {
            b.time
        }),
        (Some(a), None) => Some(a.time),
        (None, Some(b)) => Some(b.time),
        (None, None) => None,
    }
}
fn top_consumers(consumers: &[Consumer], value: impl Fn(&Consumer) -> f64) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..consumers.len()).collect();
    let count = indices.len().min(6);
    if count == 0 {
        return indices;
    }
    let order = |a: &usize, b: &usize| {
        value(&consumers[*b])
            .total_cmp(&value(&consumers[*a]))
            .then(consumers[*a].name.cmp(&consumers[*b].name))
    };
    if count < indices.len() {
        indices.select_nth_unstable_by(count - 1, order);
        indices.truncate(count);
    }
    indices.sort_unstable_by(order);
    indices
}
fn consumer_rows(
    processes: &[Process],
    consumers: &[Consumer],
    query: &str,
    sort: Sort,
    descending: bool,
) -> Vec<usize> {
    let query = query.to_lowercase();
    let mut rows: Vec<usize> = (0..consumers.len())
        .filter(|&i| {
            let consumer = &consumers[i];
            let lead = &processes[consumer.lead];
            query.is_empty()
                || consumer.name.to_lowercase().contains(&query)
                || lead.name.to_lowercase().contains(&query)
                || lead.user.to_lowercase().contains(&query)
                || lead.key.pid.to_string().contains(&query)
        })
        .collect();
    let names = (sort == Sort::Name).then(|| {
        consumers
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect::<Vec<_>>()
    });
    rows.sort_unstable_by(|&a, &b| {
        let (x, y) = (&consumers[a], &consumers[b]);
        let order = match sort {
            Sort::Cpu => x.cpu.total_cmp(&y.cpu),
            Sort::Memory => x.memory.cmp(&y.memory),
            Sort::Io => x.io.total_cmp(&y.io),
            Sort::Pid => processes[x.lead].key.pid.cmp(&processes[y.lead].key.pid),
            Sort::Name => names
                .as_ref()
                .map(|n| n[a].cmp(&n[b]))
                .unwrap_or(std::cmp::Ordering::Equal),
        };
        (if descending { order.reverse() } else { order }).then(a.cmp(&b))
    });
    rows
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
    let names = (sort == Sort::Name).then(|| {
        processes
            .iter()
            .map(|p| p.name.to_lowercase())
            .collect::<Vec<_>>()
    });
    let compare = |a: &usize, b: &usize| {
        let name_order = names.as_ref().map(|names| names[*a].cmp(&names[*b]));
        let (a, b) = (&processes[*a], &processes[*b]);
        let order = match sort {
            Sort::Cpu => a.cpu_percent.total_cmp(&b.cpu_percent),
            Sort::Memory => a.memory.cmp(&b.memory),
            Sort::Name => name_order.unwrap(),
            Sort::Pid => a.key.pid.cmp(&b.key.pid),
            Sort::Io => {
                (a.read_per_sec + a.write_per_sec).total_cmp(&(b.read_per_sec + b.write_per_sec))
            }
        };
        (if descending { order.reverse() } else { order }).then(a.key.pid.cmp(&b.key.pid))
    };
    if !tree {
        let mut rows: Vec<_> = (0..processes.len())
            .filter(|&i| matches(&processes[i]))
            .map(|i| (i, 0))
            .collect();
        rows.sort_unstable_by(|a, b| compare(&a.0, &b.0));
        return rows;
    }
    let by_pid: HashMap<_, _> = processes
        .iter()
        .enumerate()
        .map(|(i, p)| (p.key.pid, i))
        .collect();
    let mut keep = HashSet::new();
    for (i, _) in processes.iter().enumerate().filter(|(_, p)| matches(p)) {
        let mut current = Some(i);
        while let Some(index) = current {
            if !keep.insert(index) {
                break;
            }
            current = processes[index]
                .parent
                .and_then(|pid| by_pid.get(&pid).copied());
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
        siblings.sort_unstable_by(compare);
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
    remainder.sort_unstable_by(compare);
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
#[derive(Clone, Copy)]
enum TraceSeries {
    Cpu,
    Memory,
    Gpu,
    Received,
    Sent,
    DiskRead,
    DiskWrite,
    Power,
}
impl TraceSeries {
    fn sample(self, history: &History) -> (f64, Option<f32>, bool) {
        let value = match self {
            Self::Cpu => Some(history.cpu),
            Self::Memory => Some(history.memory),
            Self::Gpu => history.gpu,
            Self::Received => history.received.map(|v| v as f32),
            Self::Sent => history.sent.map(|v| v as f32),
            Self::DiskRead => history.disk_read.map(|v| v as f32),
            Self::DiskWrite => history.disk_write.map(|v| v as f32),
            Self::Power => history.power,
        };
        (history.time, value, history.break_before)
    }
}

fn for_each_clipped_trace_point(
    values: impl IntoIterator<Item = (f64, Option<f32>, bool)>,
    viewport: TraceViewport,
    mut visit: impl FnMut(bool, (f64, f32)),
) {
    let left = viewport.end - viewport.span;
    let mut previous: Option<(f64, f32)> = None;
    let mut has_pair = false;
    let mut last_visible = None;
    for (time, value, gap) in values.into_iter().chain([(0., None, false)]) {
        if gap || value.is_none() {
            if !has_pair
                && let Some(point) = previous
                && point.0 >= left
                && point.0 <= viewport.end
            {
                visit(true, point);
            }
            previous = None;
            has_pair = false;
            last_visible = None;
        }
        let Some(value) = value else { continue };
        let end = (time, value);
        if let Some(start) = previous {
            has_pair = true;
            if end.0 >= left && start.0 <= viewport.end && end.0 > start.0 {
                let interpolate = |time: f64| {
                    (
                        time,
                        start.1 + (end.1 - start.1) * ((time - start.0) / (end.0 - start.0)) as f32,
                    )
                };
                for point in [
                    interpolate(start.0.max(left)),
                    interpolate(end.0.min(viewport.end)),
                ] {
                    if last_visible != Some(point) {
                        visit(last_visible.is_none(), point);
                        last_visible = Some(point);
                    }
                }
            }
        }
        previous = Some(end);
    }
}

#[cfg(test)]
fn clipped_trace_segments(
    values: &[(f64, Option<f32>, bool)],
    viewport: TraceViewport,
) -> Vec<Vec<(f64, f32)>> {
    let mut segments: Vec<Vec<(f64, f32)>> = Vec::new();
    for_each_clipped_trace_point(values.iter().copied(), viewport, |start, point| {
        if start {
            segments.push(Vec::new());
        }
        segments.last_mut().unwrap().push(point);
    });
    segments
}

#[cfg(test)]
fn network_maximum(history: &VecDeque<History>, viewport: TraceViewport) -> f64 {
    series_maximum(
        history,
        viewport,
        &[TraceSeries::Received, TraceSeries::Sent],
        1024.,
    )
}

/// The tallest visible value of a set of series that share a scale.
fn series_maximum(
    history: &VecDeque<History>,
    viewport: TraceViewport,
    series: &[TraceSeries],
    floor: f64,
) -> f64 {
    let mut maximum = floor;
    for series in series.iter().copied() {
        for_each_clipped_trace_point(
            history.iter().map(|h| series.sample(h)),
            viewport,
            |_, (_, value)| {
                maximum = f64::max(maximum, value as f64);
            },
        );
    }
    maximum
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
        timeline.advance_to(12., 2., true, now);
        assert_eq!(timeline.end_at(now), 10.);
        assert_eq!(timeline.end_at(now + Duration::from_millis(500)), 10.5);
        let next = now + Duration::from_secs(1);
        assert_eq!(timeline.end_at(next), 11.);
        timeline.advance_to(13., 1., true, next);
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
        timeline.advance_to(12., 2., true, now);
        timeline.freeze(now + Duration::from_secs(1));
        assert_eq!(timeline.end_at(now + Duration::from_secs(30)), 11.);
        assert!(!timeline.is_moving(now + Duration::from_secs(30)));
        timeline.snap(40., now + Duration::from_secs(30));
        assert_eq!(timeline.end_at(now + Duration::from_secs(30)), 40.);
        timeline.advance_to(42., 2., true, now + Duration::from_secs(32));
        assert_eq!(timeline.end_at(now + Duration::from_secs(33)), 41.);
    }

    #[test]
    fn sample_updates_stay_fixed_and_smoothing_resumes_from_latest_sample() {
        let now = Instant::now();
        let mut timeline = GraphTimeline::new(now);
        timeline.snap(10., now);
        timeline.advance_to(12., 2., false, now);
        assert_eq!(timeline.end_at(now), 12.);
        assert_eq!(timeline.end_at(now + Duration::from_secs(1)), 12.);
        assert!(!timeline.is_moving(now));

        let next = now + Duration::from_secs(2);
        timeline.advance_to(14., 2., true, next);
        assert_eq!(timeline.end_at(next), 12.);
        assert_eq!(timeline.end_at(next + Duration::from_secs(1)), 13.);
        // Disabling smoothing completes an in-progress shift, rather than freezing it.
        let disabled = next + Duration::from_secs(1);
        timeline.snap(timeline.to, disabled);
        assert_eq!(timeline.end_at(disabled), 14.);
        assert!(!timeline.is_moving(disabled));
        timeline.advance_to(16., 2., false, next + Duration::from_secs(2));
        assert_eq!(timeline.end_at(next + Duration::from_secs(3)), 16.);

        let resumed = next + Duration::from_secs(4);
        timeline.advance_to(18., 2., true, resumed);
        assert_eq!(timeline.end_at(resumed), 16.);
        assert_eq!(timeline.end_at(resumed + Duration::from_secs(1)), 17.);
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
                ..History::default()
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
    fn consumers_rank_by_value_and_keep_only_the_leaders() {
        let mut rows = insight::consumers(&[
            process(1, None, "idle", 0.),
            process(2, None, "first busy", 90.),
            process(3, None, "middle", 20.),
            process(4, None, "second busy", 90.),
            process(5, None, "third busy", 80.),
            process(6, None, "fourth busy", 70.),
            process(7, None, "fifth busy", 60.),
            process(8, None, "sixth busy", 50.),
        ]);
        let top = top_consumers(&rows, |c| c.cpu as f64);
        let names: Vec<_> = top.iter().map(|&i| rows[i].name.as_str()).collect();
        assert_eq!(
            names,
            [
                "first busy",
                "second busy",
                "third busy",
                "fourth busy",
                "fifth busy",
                "sixth busy"
            ]
        );
        rows.truncate(2);
        assert_eq!(top_consumers(&rows, |c| c.cpu as f64), vec![1, 0]);
        assert!(top_consumers(&[], |c| c.cpu as f64).is_empty());
    }

    #[test]
    fn charts_fill_up_while_history_is_short() {
        assert_eq!(effective_span(300, 0.), 60);
        assert_eq!(effective_span(300, 59.), 60);
        assert_eq!(effective_span(300, 61.), 300);
        assert_eq!(effective_span(900, 400.), 900);
        assert_eq!(effective_span(60, 400.), 60);
        assert_eq!(effective_span(300, 800.), 300);
    }

    #[test]
    fn scrubbing_snaps_to_the_closest_sample() {
        let history: VecDeque<_> = [10., 12., 14.]
            .into_iter()
            .map(|time| History {
                time,
                ..History::default()
            })
            .collect();
        assert_eq!(nearest_sample(&history, 9.), Some(10.));
        assert_eq!(nearest_sample(&history, 11.4), Some(12.));
        assert_eq!(nearest_sample(&history, 10.9), Some(10.));
        assert_eq!(nearest_sample(&history, 99.), Some(14.));
        assert_eq!(nearest_sample(&VecDeque::new(), 1.), None);
    }

    #[test]
    fn storage_uses_finder_units_and_memory_uses_binary_units() {
        assert_eq!(decimal_bytes(994_000_000_000), "994 GB");
        assert_eq!(decimal_bytes(1_500_000_000_000), "1.50 TB");
        assert_eq!(bytes(16 * 1024 * 1024 * 1024), "16.0 GB");
    }

    #[test]
    fn name_sort_and_search_preserve_unicode_case_and_pid_ties() {
        let mut processes = vec![
            process(30, None, "Éclair", 0.),
            process(20, None, "alpha", 0.),
            process(10, None, "ALPHA", 0.),
        ];
        processes[0].user = "Åsa".into();
        assert_eq!(
            process_rows(&processes, "", Sort::Name, false, false),
            vec![(2, 0), (1, 0), (0, 0)]
        );
        assert_eq!(
            process_rows(&processes, "", Sort::Name, true, false),
            vec![(0, 0), (2, 0), (1, 0)]
        );
        assert_eq!(
            process_rows(&processes, "åS", Sort::Name, false, false),
            vec![(0, 0)]
        );
        assert_eq!(
            process_rows(&processes, "éC", Sort::Name, false, false),
            vec![(0, 0)]
        );
    }

    #[test]
    fn shared_history_keeps_a_render_frame_consistent_during_collection() {
        let mut history = Arc::new(VecDeque::from([History {
            time: 1.,
            cpu: 25.,
            memory: 50.,
            gpu: None,
            received: Some(2048.),
            sent: None,
            cores: Arc::from([10., 40.]),
            ..History::default()
        }]));
        let frame = history.clone();
        let mut next = history[0].clone();
        next.time = 2.;
        next.cpu = 75.;
        Arc::make_mut(&mut history).push_back(next);
        assert_eq!(frame.len(), 1);
        assert_eq!(history.len(), 2);
        assert_eq!(TraceSeries::Cpu.sample(&frame[0]), (1., Some(25.), false));
        assert_eq!(TraceSeries::Cpu.sample(&history[1]), (2., Some(75.), false));
        assert!(Arc::ptr_eq(&frame[0].cores, &history[0].cores));
    }

    #[test]
    fn clipped_trace_preserves_singletons_and_duplicate_time_samples() {
        assert_eq!(
            clipped_trace_segments(
                &[
                    (0., None, false),
                    (1., Some(10.), false),
                    (1., Some(20.), false),
                    (2., Some(40.), false),
                    (3., Some(60.), true),
                    (4., None, false),
                    (5., Some(80.), true),
                ],
                TraceViewport { end: 5., span: 4. }
            ),
            vec![vec![(1., 20.), (2., 40.)], vec![(3., 60.)], vec![(5., 80.)],]
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
            clipped_trace_segments(
                &[
                    (0., Some(25.), false),
                    (1., Some(50.), false),
                    (2., Some(75.), true),
                    (3., Some(90.), false),
                    (4., None, false),
                    (5., Some(10.), false)
                ],
                TraceViewport { end: 5., span: 5. }
            ),
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

use crate::model::Snapshot;
use block2::RcBlock;
use objc2::{MainThreadOnly, rc::Retained};
use objc2_app_kit::{
    NSBox, NSBoxType, NSColor, NSFont, NSMenu, NSMenuItem, NSTextAlignment, NSTextField,
    NSTitlePosition, NSView,
};
use objc2_foundation::{
    MainThreadMarker, NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSString, NSTimer,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

pub enum TrayAction {
    Open,
    Pause,
    Quit,
}

#[derive(Debug, PartialEq)]
struct TraySummary {
    sequence: u64,
    cpu: Option<f32>,
    memory: Option<f32>,
    memory_detail: String,
    gpu: Option<f32>,
    download: String,
    upload: String,
}

impl TraySummary {
    fn from_snapshot(snapshot: &Snapshot, interface: &str) -> Self {
        let ready = snapshot.sequence > 0;
        let network = snapshot
            .networks
            .iter()
            .find(|n| n.name == interface)
            .or_else(|| {
                if interface.is_empty() {
                    snapshot
                        .networks
                        .iter()
                        .find(|n| n.name == "en0")
                        .or(snapshot.networks.first())
                } else {
                    None
                }
            });
        Self {
            sequence: snapshot.sequence,
            cpu: ready.then_some(snapshot.cpu_percent),
            memory: (ready && snapshot.memory.total > 0)
                .then(|| snapshot.memory.used as f32 / snapshot.memory.total as f32 * 100.),
            memory_detail: if ready && snapshot.memory.total > 0 {
                format!(
                    "{} of {}",
                    bytes(snapshot.memory.used as f64),
                    bytes(snapshot.memory.total as f64)
                )
            } else {
                "Waiting for memory data".into()
            },
            gpu: snapshot.hardware.gpu_percent,
            download: network
                .filter(|_| ready)
                .map(|n| format!("{}/s", bytes(n.received_per_sec)))
                .unwrap_or_else(|| "Not available".into()),
            upload: network
                .filter(|_| ready)
                .map(|n| format!("{}/s", bytes(n.sent_per_sec)))
                .unwrap_or_else(|| "Not available".into()),
        }
    }
}

fn bytes(value: f64) -> String {
    if value >= 1_073_741_824. {
        format!("{:.1} GB", value / 1_073_741_824.)
    } else if value >= 1_048_576. {
        format!("{:.1} MB", value / 1_048_576.)
    } else if value >= 1024. {
        format!("{:.0} KB", value / 1024.)
    } else {
        format!("{value:.0} B")
    }
}

#[derive(Default)]
struct TrayState {
    interface: String,
    paused: bool,
}

pub struct Tray {
    _icon: TrayIcon,
    title: RefCell<Option<String>>,
    pause: MenuItem,
    summary: Rc<SummaryView>,
    state: Rc<RefCell<TrayState>>,
    timer: Retained<NSTimer>,
    events: Receiver<TrayAction>,
}

impl Tray {
    pub fn new(read_snapshot: impl Fn() -> Arc<Snapshot> + 'static) -> Result<Self, String> {
        let mtm = MainThreadMarker::new().ok_or("Menu bar must initialize on the main thread")?;
        let menu = Menu::new();
        let open = MenuItem::new("Open Sysly", true, None);
        let pause = MenuItem::new("Pause Sampling", true, None);
        let quit = MenuItem::new("Quit Sysly", true, None);
        menu.append_items(&[
            &PredefinedMenuItem::separator(),
            &open,
            &pause,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|e| e.to_string())?;
        let summary = Rc::new(SummaryView::new(mtm));
        unsafe {
            let native_menu = &*menu.ns_menu().cast::<NSMenu>();
            let item = NSMenuItem::new(mtm);
            item.setView(Some(&summary.view));
            native_menu.insertItem_atIndex(&item, 0);
        }
        let open_id = open.id().clone();
        let pause_id = pause.id().clone();
        let quit_id = quit.id().clone();
        let (sender, events) = mpsc::channel();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = if event.id == open_id {
                Some(TrayAction::Open)
            } else if event.id == pause_id {
                Some(TrayAction::Pause)
            } else if event.id == quit_id {
                Some(TrayAction::Quit)
            } else {
                None
            };
            if let Some(action) = action {
                let _ = sender.send(action);
            }
        }));
        let icon = TrayIconBuilder::new()
            .with_icon(
                Icon::from_rgba(
                    include_bytes!("../assets/brand/sysly-tray.rgba").to_vec(),
                    36,
                    36,
                )
                .map_err(|e| e.to_string())?,
            )
            .with_icon_as_template(true)
            .with_tooltip("Sysly system monitor")
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| e.to_string())?;
        if let Some(button) = icon.ns_status_item().and_then(|item| item.button(mtm)) {
            use objc2_app_kit::NSAccessibility;
            button.setAccessibilityLabel(Some(&NSString::from_str("Sysly system monitor")));
        }
        let state = Rc::new(RefCell::new(TrayState::default()));
        let timer_summary = summary.clone();
        let timer_state = state.clone();
        let block = RcBlock::new(move |_| {
            if !timer_summary
                .view
                .window()
                .is_some_and(|window| window.isVisible())
            {
                return;
            }
            let state = timer_state.borrow();
            if state.paused {
                return;
            }
            timer_summary.update(
                &TraySummary::from_snapshot(&read_snapshot(), &state.interface),
                state.paused,
            );
        });
        // AppKit fires this block only on the main run loop, which owns its views.
        let timer = unsafe {
            let timer = NSTimer::timerWithTimeInterval_repeats_block(0.5, true, &block);
            NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
            timer
        };
        summary.update(&TraySummary::from_snapshot(&Snapshot::default(), ""), false);
        Ok(Self {
            _icon: icon,
            title: RefCell::new(None),
            pause,
            summary,
            state,
            timer,
            events,
        })
    }

    pub fn update(
        &self,
        snapshot: &Snapshot,
        interface: &str,
        paused: bool,
        reading: Option<String>,
        verdict: (&str, &str),
    ) {
        self.summary.set_verdict(verdict.0, verdict.1);
        if *self.title.borrow() != reading {
            self._icon.set_title(reading.as_deref());
            *self.title.borrow_mut() = reading;
        }
        *self.state.borrow_mut() = TrayState {
            interface: interface.into(),
            paused,
        };
        self.summary
            .update(&TraySummary::from_snapshot(snapshot, interface), paused);
        self.pause.set_text(if paused {
            "Resume Sampling"
        } else {
            "Pause Sampling"
        });
    }

    pub fn poll(&self) -> Option<TrayAction> {
        self.events.try_recv().ok()
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.timer.invalidate();
    }
}

struct MetricView {
    value: Retained<NSTextField>,
    detail: Retained<NSTextField>,
    fill: Retained<NSBox>,
    y: f64,
}

const WIDTH: f64 = 320.;
const INSET: f64 = 14.;
const CONTENT: f64 = WIDTH - INSET * 2.;

fn srgb(red: u8, green: u8, blue: u8) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        red as f64 / 255.,
        green as f64 / 255.,
        blue as f64 / 255.,
        1.,
    )
}

struct SummaryView {
    view: Retained<NSView>,
    headline: Retained<NSTextField>,
    explanation: Retained<NSTextField>,
    status: Retained<NSTextField>,
    cpu: MetricView,
    memory: MetricView,
    gpu: MetricView,
    network: Retained<NSTextField>,
    sequence: Cell<u64>,
    updated: RefCell<String>,
}

impl SummaryView {
    fn new(mtm: MainThreadMarker) -> Self {
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., WIDTH, 262.));
        let headline = label(
            &view,
            "Checking your Mac…",
            rect(INSET, 228., CONTENT, 24.),
            16.,
            false,
            false,
            mtm,
        );
        if let Some(font) = NSFont::fontWithName_size(&NSString::from_str("New York"), 16.) {
            headline.setFont(Some(&font));
        }
        let explanation = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
        explanation.setFrame(rect(INSET, 188., CONTENT, 38.));
        explanation.setFont(Some(&NSFont::systemFontOfSize(12.)));
        explanation.setTextColor(Some(&NSColor::secondaryLabelColor()));
        explanation.setSelectable(false);
        view.addSubview(&explanation);
        let cpu = MetricView::new(&view, "Processor", 144., srgb(64, 159, 227), mtm);
        let memory = MetricView::new(&view, "Memory", 104., srgb(33, 178, 131), mtm);
        let gpu = MetricView::new(&view, "Graphics", 64., srgb(210, 149, 42), mtm);
        label(
            &view,
            "Network",
            rect(INSET, 34., 90., 17.),
            12.,
            false,
            false,
            mtm,
        );
        let network = label(&view, "", rect(100., 34., 202., 17.), 12., true, false, mtm);
        network.setAlignment(NSTextAlignment::Right);
        let status = label(
            &view,
            "Collecting first sample…",
            rect(INSET, 10., CONTENT, 15.),
            11.,
            false,
            true,
            mtm,
        );
        Self {
            view,
            headline,
            explanation,
            status,
            cpu,
            memory,
            gpu,
            network,
            sequence: Cell::new(0),
            updated: RefCell::new(String::new()),
        }
    }

    fn set_verdict(&self, headline: &str, detail: &str) {
        set_text(&self.headline, headline);
        set_text(&self.explanation, detail);
    }

    fn update(&self, summary: &TraySummary, paused: bool) {
        if summary.sequence != self.sequence.get() {
            self.sequence.set(summary.sequence);
            let now = unsafe { libc::time(std::ptr::null_mut()) };
            let mut local = std::mem::MaybeUninit::uninit();
            let local = unsafe {
                libc::localtime_r(&now, local.as_mut_ptr());
                local.assume_init()
            };
            *self.updated.borrow_mut() = format!(
                "{:02}:{:02}:{:02}",
                local.tm_hour, local.tm_min, local.tm_sec
            );
        }
        let status = if paused {
            format!("Paused, showing readings from {}", self.updated.borrow())
        } else if summary.sequence == 0 {
            "Collecting first sample…".into()
        } else {
            format!("Updated {}", self.updated.borrow())
        };
        set_text(&self.status, &status);
        self.cpu.update(summary.cpu, "");
        self.memory.update(summary.memory, &summary.memory_detail);
        self.gpu.update(summary.gpu, "");
        set_text(
            &self.network,
            &format!("↓ {}   ↑ {}", summary.download, summary.upload),
        );
    }
}

impl MetricView {
    fn new(
        parent: &NSView,
        name: &str,
        y: f64,
        color: Retained<NSColor>,
        mtm: MainThreadMarker,
    ) -> Self {
        label(
            parent,
            name,
            rect(INSET, y + 12., 90., 17.),
            12.,
            false,
            false,
            mtm,
        );
        let value = label(
            parent,
            "",
            rect(WIDTH - INSET - 90., y + 12., 90., 17.),
            12.,
            true,
            false,
            mtm,
        );
        value.setAlignment(NSTextAlignment::Right);
        let detail = label(
            parent,
            "",
            rect(100., y + 12., 150., 17.),
            11.,
            false,
            true,
            mtm,
        );
        detail.setAlignment(NSTextAlignment::Right);
        meter(
            parent,
            rect(INSET, y + 3., CONTENT, 4.),
            &NSColor::quaternaryLabelColor(),
            mtm,
        );
        let fill = meter(parent, rect(INSET, y + 3., 0., 4.), &color, mtm);
        Self {
            value,
            detail,
            fill,
            y,
        }
    }

    fn update(&self, percent: Option<f32>, detail: &str) {
        set_text(
            &self.value,
            &percent
                .map(|v| format!("{v:.0}%"))
                .unwrap_or_else(|| "Not available".into()),
        );
        set_text(&self.detail, detail);
        self.fill.setFrame(rect(
            INSET,
            self.y + 3.,
            percent.unwrap_or(0.).clamp(0., 100.) as f64 / 100. * CONTENT,
            4.,
        ));
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}
fn set_text(label: &NSTextField, text: &str) {
    label.setStringValue(&NSString::from_str(text));
}
fn label(
    parent: &NSView,
    text: &str,
    frame: NSRect,
    size: f64,
    bold: bool,
    muted: bool,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFrame(frame);
    label.setFont(Some(&NSFont::systemFontOfSize_weight(
        size,
        if bold { 0.23 } else { 0. },
    )));
    let text_color = if muted {
        NSColor::labelColor().colorWithAlphaComponent(0.68)
    } else {
        NSColor::labelColor()
    };
    label.setTextColor(Some(&text_color));
    parent.addSubview(&label);
    label
}
fn meter(
    parent: &NSView,
    frame: NSRect,
    color: &NSColor,
    mtm: MainThreadMarker,
) -> Retained<NSBox> {
    let bar = NSBox::initWithFrame(NSBox::alloc(mtm), frame);
    bar.setBoxType(NSBoxType::Custom);
    bar.setTitlePosition(NSTitlePosition::NoTitle);
    bar.setBorderWidth(0.);
    bar.setCornerRadius(2.);
    bar.setFillColor(color);
    parent.addSubview(&bar);
    bar
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Memory, Network};
    #[test]
    fn summary_preserves_missing_telemetry_and_selected_interface() {
        let mut snapshot = Snapshot::default();
        let waiting = TraySummary::from_snapshot(&snapshot, "");
        assert_eq!(waiting.cpu, None);
        assert_eq!(waiting.memory, None);
        assert_eq!(waiting.gpu, None);
        assert_eq!(waiting.download, "Not available");
        snapshot.sequence = 1;
        snapshot.cpu_percent = 12.;
        snapshot.memory = Memory {
            used: 4 * 1_073_741_824,
            total: 16 * 1_073_741_824,
            ..Memory::default()
        };
        snapshot.networks = vec![
            Network {
                name: "en0".into(),
                received_per_sec: 2048.,
                sent_per_sec: 1024.,
                ..Network::default()
            },
            Network {
                name: "en1".into(),
                received_per_sec: 0.,
                sent_per_sec: 0.,
                ..Network::default()
            },
        ];
        let summary = TraySummary::from_snapshot(&snapshot, "en0");
        assert_eq!(summary.cpu, Some(12.));
        assert_eq!(summary.memory, Some(25.));
        assert_eq!(summary.memory_detail, "4.0 GB of 16.0 GB");
        assert_eq!(summary.gpu, None);
        assert_eq!(summary.download, "2 KB/s");
        assert_eq!(summary.upload, "1 KB/s");
        assert_eq!(
            TraySummary::from_snapshot(&snapshot, "en1").download,
            "0 B/s"
        );
        assert_eq!(
            TraySummary::from_snapshot(&snapshot, "gone").download,
            "Not available"
        );
    }
}

//! Settings, laid out as grouped rows in the manner of System Settings.

use super::{App, Edit, FileAction, Undo, preferences, style::*, theme_colors};
use crate::preferences::{Density, FontChoice, HISTORY_RANGES, MenuBarReading, Theme};
use gpui::{
    AnyElement, Context, Div, IntoElement, SharedString, Window, div, prelude::*, px, relative,
};
use gpui_component::{Icon, IconName, switch::Switch};

const COLUMN_WIDTH: f32 = 680.;

fn sample_theme(name: &str) -> Option<Theme> {
    match name {
        "Black" => Some(Theme::black()),
        "Graphite" => Some(Theme::dark()),
        "Paper" => Some(Theme::light()),
        "Midnight" => Some(Theme::midnight()),
        "Ember" => Some(Theme::ember()),
        _ => None,
    }
}

impl App {
    fn group(&self, title: &'static str, rows: Vec<AnyElement>) -> Div {
        let ui = self.ui();
        let count = rows.len();
        let mut panel = ui.panel().overflow_hidden();
        for (index, row) in rows.into_iter().enumerate() {
            panel = panel.child(
                div()
                    .when(index + 1 < count, |s| {
                        s.border_b_1().border_color(ui.p.line.opacity(0.6))
                    })
                    .child(row),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(ui.strong(title).px(px(4.)))
            .child(panel)
    }

    fn setting(
        &self,
        title: impl Into<SharedString>,
        hint: Option<&'static str>,
        control: impl IntoElement,
    ) -> AnyElement {
        let ui = self.ui();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_6()
            .min_h(px(44.))
            .px(px(14.))
            .py(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(ui.text(13., ui.p.text, title))
                    .children(hint.map(|hint| {
                        ui.text(12., ui.p.muted, hint)
                            .mt(px(2.))
                            .line_height(relative(1.35))
                    })),
            )
            .child(div().flex_shrink_0().child(control))
            .into_any_element()
    }

    fn tabs(&self, cx: &mut Context<Self>) -> Div {
        let ui = self.ui();
        let mut tabs = ui.segments();
        for (index, title) in ["General", "Appearance", "Dashboard"]
            .into_iter()
            .enumerate()
        {
            tabs = tabs.child(
                ui.segment(("settings-tab", index), title, self.settings_tab == index)
                    .px(px(16.))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings_tab = index;
                        cx.notify();
                    })),
            );
        }
        tabs
    }

    fn general(&self, cx: &mut Context<Self>) -> Div {
        let ui = self.ui();
        let prefs = &self.preferences;
        let mut samples = ui.segments();
        for value in [1u64, 2, 3, 5, 10] {
            samples = samples.child(
                ui.segment(
                    ("sample", value as usize),
                    format!("{value} s"),
                    prefs.refresh_secs == value,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.preferences.refresh_secs = value;
                    this.save(cx);
                })),
            );
        }
        let mut range = ui.segments();
        for value in HISTORY_RANGES {
            range = range.child(
                ui.segment(
                    ("default-range", value as usize),
                    format!("{} min", value / 60),
                    prefs.history_secs == value,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.preferences.history_secs = value;
                    this.save(cx);
                })),
            );
        }
        let mut density = ui.segments();
        for (value, name) in [
            (Density::Comfortable, "Comfortable"),
            (Density::Compact, "Compact"),
        ] {
            density = density.child(ui.segment(name, name, prefs.density == value).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.preferences.density = value;
                    this.save(cx);
                }),
            ));
        }
        let mut reading = ui.segments();
        for (value, name) in [
            (MenuBarReading::Icon, "Icon only"),
            (MenuBarReading::Cpu, "Processor"),
            (MenuBarReading::Memory, "Memory"),
        ] {
            reading = reading.child(
                ui.segment(name, name, prefs.menu_bar_reading == value)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.preferences.menu_bar_reading = value;
                        this.save(cx);
                    })),
            );
        }
        let s = &self.snapshot;
        let mut session = vec![
            self.setting(
                "Samples in memory",
                None,
                ui.figure(12., ui.p.muted, self.history.len().to_string()),
            ),
            self.setting(
                "Last collection",
                None,
                ui.figure(12., ui.p.muted, format!("{:.0} ms", s.collected_ms)),
            ),
            self.setting(
                "Hardware readings",
                None,
                ui.text(12., ui.p.muted, s.hardware.status.clone()),
            ),
        ];
        for warning in s.warnings.iter().take(3) {
            session.push(
                ui.muted(warning.clone())
                    .px(px(14.))
                    .py(px(10.))
                    .line_height(relative(1.35))
                    .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(self.group(
                "Sampling",
                vec![
                    self.setting(
                        "Sample every",
                        Some("Longer intervals use less energy."),
                        samples,
                    ),
                    self.setting("Recorder range", Some("Up to 15 minutes are kept."), range),
                    self.setting(
                        "Smooth chart scrolling",
                        Some("Moves charts continuously, at the cost of more CPU."),
                        Switch::new("smooth-charts")
                            .checked(prefs.smooth_charts)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.preferences.smooth_charts = !this.preferences.smooth_charts;
                                if !this.preferences.smooth_charts {
                                    this.graph_timeline
                                        .snap(this.graph_timeline.to, std::time::Instant::now());
                                }
                                this.save(cx);
                            })),
                    ),
                ],
            ))
            .child(self.group("Window", vec![self.setting("Density", None, density)]))
            .child(self.group(
                "Menu bar",
                vec![self.setting(
                    "Show in the menu bar",
                    Some("Sysly stays there when you close the window."),
                    reading,
                )],
            ))
            .child(self.group("This session", session))
    }

    fn theme_card(
        &self,
        name: &'static str,
        theme: Option<Theme>,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ui = self.ui();
        let p = ui.p;
        let (left, right) = match &theme {
            Some(theme) => (theme.clone(), theme.clone()),
            None => (Theme::dark(), Theme::light()),
        };
        let swatches = |t: &Theme| {
            div()
                .flex_1()
                .h_full()
                .bg(color(t.background))
                .p(px(7.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .children(
                    [t.cpu, t.memory, t.gpu, t.network]
                        .into_iter()
                        .map(|c| div().h(px(3.)).rounded_full().bg(color(c))),
                )
        };
        div()
            .id(name)
            .tab_index(0)
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .min_w(px(96.))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .h(px(60.))
                    .rounded(px(9.))
                    .overflow_hidden()
                    .border_2()
                    .border_color(if selected { p.accent } else { p.line })
                    .child(swatches(&left))
                    .when(theme.is_none(), |s| s.child(swatches(&right))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(ui.text(12., if selected { p.text } else { p.muted }, name))
                    .when(selected, |s| {
                        s.child(
                            Icon::new(IconName::Check)
                                .size(px(12.))
                                .text_color(p.accent),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.apply_theme(name, window, cx);
            }))
    }

    fn apply_theme(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let font = self.preferences.theme.font;
        let font_size = self.preferences.theme.font_size;
        match sample_theme(name) {
            Some(theme) => {
                self.preferences.follow_system = false;
                self.preferences.theme = Theme {
                    font,
                    font_size,
                    ..theme
                };
            }
            None => {
                self.preferences.follow_system = true;
                self.follow_system_appearance(window);
            }
        }
        self.create_inputs(window, cx);
        self.save(cx);
    }

    fn swatch_row(&self, index: usize, name: &'static str, value: [u8; 3]) -> Div {
        let ui = self.ui();
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(32.))
            .child(
                div()
                    .size(px(16.))
                    .flex_shrink_0()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui.p.line)
                    .bg(color(value)),
            )
            .child(ui.text(12., ui.p.text, name).flex_1().min_w_0().truncate())
            .child(
                div()
                    .w(px(96.))
                    .flex_shrink_0()
                    .child(self.input(Edit::Color(index))),
            )
    }

    fn appearance_tab(&self, cx: &mut Context<Self>) -> Div {
        let ui = self.ui();
        let prefs = &self.preferences;
        let t = &prefs.theme;
        let auto = prefs.follow_system;
        let mut cards = div().flex().gap(px(12.)).p(px(14.)).child(self.theme_card(
            "Automatic",
            None,
            auto,
            cx,
        ));
        for name in ["Black", "Graphite", "Paper", "Midnight", "Ember"] {
            let selected = !auto && t.name == name;
            cards = cards.child(self.theme_card(name, sample_theme(name), selected, cx));
        }
        let colors = theme_colors(t);
        let mut left = div().flex().flex_col().flex_1().min_w_0();
        let mut right = div().flex().flex_col().flex_1().min_w_0();
        for (index, (name, value)) in colors.iter().enumerate() {
            if index < 6 {
                left = left.child(self.swatch_row(index, name, *value));
            } else {
                right = right.child(self.swatch_row(index, name, *value));
            }
        }
        let custom = prefs.custom_font_path.is_some();
        let family = |value: FontChoice| !custom && t.font == value;
        let fonts = ui
            .segments()
            .child(
                ui.segment("system-font", "System", family(FontChoice::System))
                    .on_click(cx.listener(|this, _, _, cx| this.set_font(FontChoice::System, cx))),
            )
            .child(
                ui.segment("mono-font", "Monospace", family(FontChoice::Monospace))
                    .on_click(
                        cx.listener(|this, _, _, cx| this.set_font(FontChoice::Monospace, cx)),
                    ),
            )
            .child(
                ui.segment("load-font", "Choose font…", custom)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Fonts", &["ttf", "otf", "ttc"])
                            .pick_file()
                        {
                            this.preferences.custom_font_path = Some(path);
                            this.load_font(cx);
                            this.save(cx);
                        }
                    })),
            );
        let size = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                ui.button("font-minus", "−")
                    .w(px(30.))
                    .px(px(0.))
                    .on_click(cx.listener(|this, _, _, cx| this.step_font(-1., cx))),
            )
            .child(
                ui.figure(12., ui.p.text, format!("{:.0} pt", t.font_size))
                    .w(px(40.))
                    .text_center(),
            )
            .child(
                ui.button("font-plus", "+")
                    .w(px(30.))
                    .px(px(0.))
                    .on_click(cx.listener(|this, _, _, cx| this.step_font(1., cx))),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(ui.strong("Theme").px(px(4.)))
                    .child(ui.panel().child(cards)),
            )
            .child(self.group(
                "Text",
                vec![
                    self.setting(
                        "Typeface",
                        custom.then_some("A chosen font stays on this Mac and is not exported."),
                        fonts,
                    ),
                    self.setting("Text size", None, size),
                ],
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(ui.strong("Colors").px(px(4.)))
                    .child(
                        ui.panel()
                            .p(px(14.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .mb(px(10.))
                                    .child(ui.text(12., ui.p.text, "Theme name"))
                                    .child(div().flex_1().child(self.input(Edit::ThemeName)))
                                    .child(ui.button("import-theme", "Import…").on_click(
                                        cx.listener(|this, _, window, cx| {
                                            this.file_action(FileAction::ImportTheme, window, cx)
                                        }),
                                    ))
                                    .child(ui.button("export-theme", "Export…").on_click(
                                        cx.listener(|this, _, window, cx| {
                                            this.file_action(FileAction::ExportTheme, window, cx)
                                        }),
                                    )),
                            )
                            .child(div().flex().gap_6().child(left).child(right)),
                    ),
            )
    }

    fn set_font(&mut self, font: FontChoice, cx: &mut Context<Self>) {
        self.preferences.theme.font = font;
        self.preferences.custom_font_path = None;
        self.load_font(cx);
        self.save(cx);
    }

    fn step_font(&mut self, delta: f32, cx: &mut Context<Self>) {
        let size = &mut self.preferences.theme.font_size;
        *size = (*size + delta).clamp(11., 20.);
        self.save(cx);
    }

    fn dashboard_tab(&self, cx: &mut Context<Self>) -> Div {
        let ui = self.ui();
        let mut saved: Vec<AnyElement> = vec![
            div()
                .flex()
                .items_center()
                .gap_2()
                .px(px(14.))
                .py(px(10.))
                .child(div().flex_1().child(self.input(Edit::LayoutName)))
                .child(
                    ui.button("save-layout", "Save current layout")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let name = this.inputs[&Edit::LayoutName].read(cx).value();
                            match this.preferences.save_named_layout(name.as_ref()) {
                                Ok(()) => this.show_toast(format!("Saved {name}"), None),
                                Err(error) => this.show_error(error),
                            }
                            this.save(cx);
                        })),
                )
                .into_any_element(),
        ];
        for (index, layout) in self.preferences.saved_layouts.iter().enumerate() {
            saved.push(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px(px(14.))
                    .h(px(44.))
                    .child(
                        ui.text(13., ui.p.text, layout.name.clone())
                            .flex_1()
                            .min_w_0()
                            .truncate(),
                    )
                    .child(
                        ui.button(("load-layout", index), "Apply")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.preferences.load_named_layout(index);
                                this.create_inputs(window, cx);
                                this.show_toast("Applied layout", Some(Undo::Layout));
                                this.save(cx);
                            })),
                    )
                    .child(
                        ui.quiet(("delete-layout", index), "Delete")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let layout = this.preferences.saved_layouts.remove(index);
                                let message = format!("Deleted {}", layout.name);
                                this.deleted_layout = Some(layout);
                                this.show_toast(message, Some(Undo::SavedLayout));
                                this.save(cx);
                            })),
                    )
                    .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(self.group(
                "Overview",
                vec![
                    div()
                        .flex()
                        .items_center()
                        .gap_6()
                        .px(px(14.))
                        .min_h(px(44.))
                        .child(ui.text(13., ui.p.text, "Title").flex_shrink_0())
                        .child(div().flex_1().child(self.input(Edit::Title)))
                        .into_any_element(),
                    self.setting(
                        "Arrange sections",
                        Some("Move, resize, rename or hide sections right on the Overview."),
                        ui.button("open-customize", "Customize Overview").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.navigate(super::Page::Overview, cx);
                                this.editing = true;
                                cx.notify();
                            },
                        )),
                    ),
                    self.setting(
                        "Reset layout",
                        Some("Go back to the sections and order Sysly started with."),
                        ui.button("reset-layout", "Reset").on_click(cx.listener(
                            |this, _, window, cx| {
                                this.preferences.reset_layout();
                                this.create_inputs(window, cx);
                                this.show_toast("Reset layout", Some(Undo::Layout));
                                this.save(cx);
                            },
                        )),
                    ),
                ],
            ))
            .child(self.group("Saved layouts", saved))
            .child(self.group(
                "Share",
                vec![self.setting(
                    "Layout file",
                    Some("Moves your sections, sizes and labels between Macs."),
                    div()
                        .flex()
                        .gap_2()
                        .child(ui.button("import-layout", "Import…").on_click(cx.listener(
                            |this, _, window, cx| {
                                this.file_action(FileAction::ImportLayout, window, cx)
                            },
                        )))
                        .child(ui.button("export-layout", "Export…").on_click(cx.listener(
                            |this, _, window, cx| {
                                this.file_action(FileAction::ExportLayout, window, cx)
                            },
                        ))),
                )],
            ))
    }

    pub(super) fn settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let body = match self.settings_tab {
            0 => self.general(cx),
            1 => self.appearance_tab(cx),
            _ => self.dashboard_tab(cx),
        };
        div()
            .flex()
            .justify_center()
            .px(px(GUTTER))
            .pt(px(18.))
            .pb(px(32.))
            .child(
                div()
                    .w_full()
                    .max_w(px(COLUMN_WIDTH))
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .child(div().flex().justify_center().child(self.tabs(cx)))
                    .child(body),
            )
            .into_any_element()
    }

    pub(super) fn file_action(
        &mut self,
        action: FileAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let (result, done) = match action {
            FileAction::ImportTheme => (
                preferences::import_theme(&path).map(|theme| {
                    self.preferences.follow_system = false;
                    self.preferences.theme = theme;
                }),
                "Imported theme",
            ),
            FileAction::ImportLayout => (
                preferences::import_layout(&path).map(|sections| {
                    self.preferences.remember_layout();
                    self.preferences.sections = sections;
                }),
                "Imported layout",
            ),
            FileAction::ExportTheme => (
                preferences::export_theme(&path, &self.preferences.theme),
                "Exported theme",
            ),
            FileAction::ExportLayout => (
                preferences::export_layout(&path, &self.preferences.sections),
                "Exported layout",
            ),
        };
        match result {
            Ok(()) => self.show_toast(done, None),
            Err(error) => self.show_error(error),
        }
        self.create_inputs(window, cx);
        self.load_font(cx);
        self.save(cx);
    }
}

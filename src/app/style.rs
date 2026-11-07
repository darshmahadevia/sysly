//! Design tokens and the small set of primitives every screen is built from.

use crate::preferences::{Density, Theme};
use gpui::{
    Div, Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Hsla, Pixels, SharedString,
    Stateful, div, prelude::*, px, rgb,
};
use std::sync::Arc;

pub(super) const TOOLBAR_HEIGHT: f32 = 52.;
pub(super) const GUTTER: f32 = 24.;
pub(super) const RADIUS_PANEL: f32 = 12.;
pub(super) const RADIUS_CONTROL: f32 = 7.;

pub(super) fn color(c: [u8; 3]) -> Hsla {
    rgb((c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).into()
}

fn mix(a: [u8; 3], b: [u8; 3], amount: f32) -> [u8; 3] {
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * amount).round() as u8;
    [
        channel(a[0], b[0]),
        channel(a[1], b[1]),
        channel(a[2], b[2]),
    ]
}

/// Whether a theme's background is dark. Decides which system controls match it.
pub(super) fn is_dark(theme: &Theme) -> bool {
    let [r, g, b] = theme.background.map(|c| c as f32);
    0.2126 * r + 0.7152 * g + 0.0722 * b < 128.
}

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub bg: Hsla,
    pub sidebar: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub line: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
    pub accent: Hsla,
    pub on_accent: Hsla,
    pub danger: Hsla,
    pub warning: Hsla,
    pub cpu: Hsla,
    pub memory: Hsla,
    pub gpu: Hsla,
    pub network: Hsla,
    pub storage: Hsla,
    pub battery: Hsla,
    pub dark: bool,
}

impl Palette {
    pub fn new(t: &Theme) -> Self {
        let dark = is_dark(t);
        let text = color(t.text);
        let [r, g, b] = t.accent.map(|c| c as f32);
        let on_accent = if 0.2126 * r + 0.7152 * g + 0.0722 * b > 140. {
            color([18, 20, 22])
        } else {
            color([255, 255, 255])
        };
        Self {
            bg: color(t.background),
            sidebar: color(mix(t.background, t.text, if dark { 0.025 } else { -0.03 })),
            surface: color(t.surface),
            raised: color(mix(t.surface, t.text, 0.07)),
            text,
            muted: color(t.muted),
            line: color(t.divider),
            hover: text.opacity(0.06),
            active: text.opacity(0.1),
            accent: color(t.accent),
            on_accent,
            danger: color(t.danger),
            warning: color(t.gpu),
            cpu: color(t.cpu),
            memory: color(t.memory),
            gpu: color(t.gpu),
            network: color(t.network),
            storage: color(t.storage),
            battery: color(t.battery),
            dark,
        }
    }
}

/// Everything a render function needs, cheap to clone so closures can own it.
#[derive(Clone)]
pub(super) struct Ui {
    pub p: Palette,
    family: SharedString,
    numeric: Font,
    scale: f32,
    pub compact: bool,
}

impl Ui {
    pub fn new(theme: &Theme, density: Density, family: SharedString) -> Self {
        let numeric = Font {
            family: family.clone(),
            features: FontFeatures(Arc::new(vec![("tnum".into(), 1)])),
            fallbacks: None::<FontFallbacks>,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        };
        Self {
            p: Palette::new(theme),
            family,
            numeric,
            scale: theme.font_size / 13.,
            compact: density == Density::Compact,
        }
    }

    /// A type size, following the text size setting.
    pub fn fs(&self, size: f32) -> Pixels {
        px((size * self.scale).round())
    }

    pub fn text(&self, size: f32, c: Hsla, value: impl Into<SharedString>) -> Div {
        div()
            .text_size(self.fs(size))
            .text_color(c)
            .child(value.into())
    }

    pub fn muted(&self, value: impl Into<SharedString>) -> Div {
        self.text(12., self.p.muted, value)
    }

    pub fn strong(&self, value: impl Into<SharedString>) -> Div {
        self.text(13., self.p.text, value)
            .font_weight(FontWeight::SEMIBOLD)
    }

    /// Figures use tabular digits so a changing value never shifts the layout.
    pub fn figure(&self, size: f32, c: Hsla, value: impl Into<SharedString>) -> Div {
        div()
            .font(self.numeric.clone())
            .text_size(self.fs(size))
            .text_color(c)
            .child(value.into())
    }

    /// The one voice in the app that speaks in sentences.
    pub fn verdict(&self, value: impl Into<SharedString>) -> Div {
        let serif = self.family.as_ref() == ".AppleSystemUIFont";
        div()
            .when(serif, |s| s.font_family("New York"))
            .text_size(self.fs(if serif { 30. } else { 27. }))
            .font_weight(FontWeight::MEDIUM)
            .text_color(self.p.text)
            .child(value.into())
    }

    pub fn sidebar_width(&self) -> f32 {
        224. * self.scale.clamp(1., 1.3)
    }

    /// Whether there is room for sparklines beside sidebar readings.
    pub fn roomy_sidebar(&self) -> bool {
        self.scale <= 1.1
    }

    pub fn lane_label_width(&self) -> f32 {
        190. * self.scale.max(1.)
    }

    /// Scales a fixed column with the text set inside it.
    pub fn scaled(&self, width: f32) -> f32 {
        width * self.scale.max(1.)
    }

    pub fn panel(&self) -> Div {
        div()
            .flex()
            .flex_col()
            .rounded(px(RADIUS_PANEL))
            .bg(self.p.surface)
            .border_1()
            .border_color(self.p.line.opacity(0.7))
    }

    pub fn inset(&self) -> f32 {
        if self.compact { 12. } else { 16. }
    }

    pub fn row_height(&self) -> f32 {
        (if self.compact { 28. } else { 32. }) * self.scale.max(1.)
    }

    fn control(&self, id: impl Into<gpui::ElementId>) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .tab_index(0)
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .h(px(28.))
            .rounded(px(RADIUS_CONTROL))
            .border_1()
            .border_color(gpui::transparent_black())
            .focus(move |s| s.border_color(p.accent))
            .cursor_pointer()
            .text_size(self.fs(12.))
            .font_weight(FontWeight::MEDIUM)
    }

    pub fn button(
        &self,
        id: impl Into<gpui::ElementId>,
        value: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let p = self.p;
        self.control(id)
            .px(px(11.))
            .bg(p.raised)
            .text_color(p.text)
            .hover(move |s| s.bg(p.active))
            .active(move |s| s.opacity(0.8))
            .child(value.into())
    }

    pub fn primary(
        &self,
        id: impl Into<gpui::ElementId>,
        value: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let p = self.p;
        self.control(id)
            .px(px(12.))
            .bg(p.accent)
            .text_color(p.on_accent)
            .hover(|s| s.opacity(0.9))
            .active(|s| s.opacity(0.8))
            .child(value.into())
    }

    pub fn danger(
        &self,
        id: impl Into<gpui::ElementId>,
        value: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let p = self.p;
        self.control(id)
            .px(px(11.))
            .bg(p.danger.opacity(0.14))
            .text_color(p.danger)
            .hover(move |s| s.bg(p.danger.opacity(0.22)))
            .active(|s| s.opacity(0.8))
            .child(value.into())
    }

    pub fn quiet(
        &self,
        id: impl Into<gpui::ElementId>,
        value: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let p = self.p;
        self.control(id)
            .px(px(8.))
            .text_color(p.muted)
            .hover(move |s| s.bg(p.hover).text_color(p.text))
            .child(value.into())
    }

    pub fn icon_button(&self, id: impl Into<gpui::ElementId>) -> Stateful<Div> {
        let p = self.p;
        self.control(id)
            .w(px(28.))
            .text_color(p.muted)
            .hover(move |s| s.bg(p.hover).text_color(p.text))
    }

    /// One option in a segmented control. Wrap a run of these in `segments`.
    pub fn segment(
        &self,
        id: impl Into<gpui::ElementId>,
        value: impl Into<SharedString>,
        selected: bool,
    ) -> Stateful<Div> {
        let p = self.p;
        self.control(id)
            .h(px(24.))
            .px(px(10.))
            .rounded(px(5.))
            .text_color(if selected { p.text } else { p.muted })
            .when(selected, |s| {
                s.bg(if p.dark { p.active } else { p.surface })
            })
            .when(!selected, |s| s.hover(move |s| s.text_color(p.text)))
            .child(value.into())
    }

    pub fn segments(&self) -> Div {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(1.))
            .p(px(2.))
            .rounded(px(RADIUS_CONTROL))
            .bg(self.p.hover)
    }

    /// A share of something: storage used, charge left, a core's load.
    pub fn meter(&self, fraction: f32, c: Hsla, height: f32) -> Div {
        div()
            .w_full()
            .h(px(height))
            .rounded_full()
            .overflow_hidden()
            .bg(self.p.text.opacity(0.09))
            .child(
                div()
                    .h_full()
                    .w(gpui::relative(fraction.clamp(0., 1.)))
                    .rounded_full()
                    .bg(c),
            )
    }

    /// The same measure as `meter`, for ranked rows where a track on every row
    /// would only add noise.
    pub fn bar(&self, fraction: f32, c: Hsla) -> Div {
        div().w_full().h(px(3.)).child(
            div()
                .h_full()
                .w(gpui::relative(fraction.clamp(0., 1.)))
                .rounded_full()
                .bg(c),
        )
    }

    /// A label and its value in a facts list.
    pub fn fact(&self, name: impl Into<SharedString>, value: impl Into<SharedString>) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .min_h(px(self.row_height()))
            .py(px(4.))
            .border_b_1()
            .border_color(self.p.line.opacity(0.5))
            .child(self.muted(name).flex_shrink_0())
            .child(self.figure(12., self.p.text, value).text_right())
    }
}

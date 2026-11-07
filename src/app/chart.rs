//! The recorder's drawing code. Every chart shares one time axis, so a moment
//! under the pointer lines up across all of them.

use super::{History, TraceSeries, TraceViewport, for_each_clipped_trace_point, style::Ui};
use gpui::{
    AnyElement, Bounds, Hsla, PathBuilder, Pixels, Window, canvas, fill, point, prelude::*, px,
    quad, size, transparent_black,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::Arc};

pub(super) type ChartRegistry = Rc<RefCell<Vec<Bounds<Pixels>>>>;

#[derive(Clone)]
pub(super) struct Plot {
    pub history: Arc<VecDeque<History>>,
    pub viewport: TraceViewport,
    pub series: TraceSeries,
    /// Drawn below a center line, for pairs such as download and upload.
    pub mirror: Option<TraceSeries>,
    pub maximum: f32,
    pub color: Hsla,
    pub divisions: u32,
    pub cursor: Option<f64>,
    pub registry: Option<ChartRegistry>,
    pub stroke: f32,
    pub decorated: bool,
}

impl Plot {
    pub fn element(self, ui: &Ui, height: f32) -> AnyElement {
        let grid = ui.p.line;
        let surface = ui.p.surface;
        let text = ui.p.text;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| self.paint(bounds, window, grid, surface, text),
        )
        .w_full()
        .h(px(height))
        .into_any_element()
    }

    fn x(&self, bounds: Bounds<Pixels>, time: f64) -> Pixels {
        let v = self.viewport;
        bounds.left() + bounds.size.width * ((time - v.end + v.span) / v.span) as f32
    }

    fn paint(
        &self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        grid: Hsla,
        surface: Hsla,
        text: Hsla,
    ) {
        if let Some(registry) = &self.registry {
            registry.borrow_mut().push(bounds);
        }
        let height = bounds.size.height;
        let center = bounds.top() + height / 2.;
        if self.decorated {
            for i in 1..self.divisions {
                let x = bounds.left() + bounds.size.width * (i as f32 / self.divisions as f32);
                window.paint_quad(fill(
                    Bounds::new(point(x.floor(), bounds.top()), size(px(1.), height)),
                    grid.opacity(0.45),
                ));
            }
            let base = if self.mirror.is_some() {
                center
            } else {
                bounds.bottom() - px(1.)
            };
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left(), base.floor()),
                    size(bounds.size.width, px(1.)),
                ),
                grid,
            ));
        }
        let span = if self.mirror.is_some() {
            height / 2.
        } else {
            height
        };
        let base = if self.mirror.is_some() {
            center
        } else {
            bounds.bottom()
        };
        self.trace(
            window,
            bounds,
            self.series,
            base,
            -span,
            self.color,
            surface,
        );
        if let Some(mirror) = self.mirror {
            self.trace(
                window,
                bounds,
                mirror,
                base,
                span,
                self.color.opacity(0.62),
                surface,
            );
        }
        if let Some(time) = self.cursor {
            let x = self.x(bounds, time);
            window.paint_quad(fill(
                Bounds::new(point(x.floor(), bounds.top()), size(px(1.), height)),
                text.opacity(0.45),
            ));
            let sample = self.history.iter().find(|h| h.time == time).map(|h| {
                (
                    self.series.sample(h).1,
                    self.mirror.and_then(|m| m.sample(h).1),
                )
            });
            if let Some((value, mirrored)) = sample {
                for (value, direction) in [(value, -span), (mirrored, span)] {
                    if let Some(value) = value {
                        let y = base + direction * (value / self.maximum).clamp(0., 1.);
                        self.dot(window, point(x, y), 3.5, self.color, surface);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn trace(
        &self,
        window: &mut Window,
        bounds: Bounds<Pixels>,
        series: TraceSeries,
        base: Pixels,
        span: Pixels,
        color: Hsla,
        surface: Hsla,
    ) {
        let mut line = PathBuilder::stroke(px(self.stroke));
        let mut area = PathBuilder::fill();
        let mut last = None;
        let maximum = self.maximum;
        for_each_clipped_trace_point(
            self.history.iter().map(|h| series.sample(h)),
            self.viewport,
            |start, (time, value)| {
                let at = point(
                    self.x(bounds, time),
                    base + span * (value / maximum).clamp(0., 1.),
                );
                if start {
                    if let Some(end) = last {
                        close_area(&mut area, end, base);
                    }
                    area.move_to(point(at.x, base));
                    area.line_to(at);
                    line.move_to(at);
                } else {
                    area.line_to(at);
                    line.line_to(at);
                }
                last = Some(at);
            },
        );
        let Some(end) = last else { return };
        close_area(&mut area, end, base);
        if let Ok(path) = area.build() {
            window.paint_path(path, color.opacity(0.14));
        }
        if let Ok(path) = line.build() {
            window.paint_path(path, color);
        }
        if self.decorated && self.cursor.is_none() {
            self.dot(window, end, 2.5, color, surface);
        }
    }

    fn dot(
        &self,
        window: &mut Window,
        at: gpui::Point<Pixels>,
        radius: f32,
        color: Hsla,
        surface: Hsla,
    ) {
        let outer = radius + 1.5;
        window.paint_quad(quad(
            Bounds::new(
                point(at.x - px(outer), at.y - px(outer)),
                size(px(outer * 2.), px(outer * 2.)),
            ),
            px(outer),
            surface,
            px(0.),
            transparent_black(),
            Default::default(),
        ));
        window.paint_quad(quad(
            Bounds::new(
                point(at.x - px(radius), at.y - px(radius)),
                size(px(radius * 2.), px(radius * 2.)),
            ),
            px(radius),
            color,
            px(0.),
            transparent_black(),
            Default::default(),
        ));
    }
}

fn close_area(area: &mut PathBuilder, end: gpui::Point<Pixels>, base: Pixels) {
    area.line_to(point(end.x, base));
    area.close();
}

/// One row per core and one column per moment, so a load that hops between
/// cores stays visible after the line chart has averaged it away.
pub(super) fn core_strip(
    history: Arc<VecDeque<History>>,
    viewport: TraceViewport,
    cores: usize,
    color: Hsla,
    empty: Hsla,
    row: f32,
) -> AnyElement {
    let height = (row + 2.) * cores as f32;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for core in 0..cores {
                let y = bounds.top() + px((row + 2.) * core as f32);
                window.paint_quad(fill(
                    Bounds::new(point(bounds.left(), y), size(bounds.size.width, px(row))),
                    empty,
                ));
            }
            let left = viewport.end - viewport.span;
            let mut previous: Option<&History> = None;
            for sample in history.iter() {
                if let Some(start) =
                    previous.filter(|_| sample.time >= left && !sample.break_before)
                {
                    let from = (start.time.max(left) - left) / viewport.span;
                    let to = (sample.time - left) / viewport.span;
                    let x = bounds.left() + bounds.size.width * from as f32;
                    let width = (bounds.size.width * (to - from) as f32).max(px(1.));
                    for (core, load) in sample.cores.iter().enumerate().take(cores) {
                        let y = bounds.top() + px((row + 2.) * core as f32);
                        window.paint_quad(fill(
                            Bounds::new(point(x, y), size(width, px(row))),
                            color.opacity(0.1 + 0.9 * (load / 100.).clamp(0., 1.)),
                        ));
                    }
                }
                previous = Some(sample);
            }
        },
    )
    .w_full()
    .h(px(height))
    .into_any_element()
}

/// A small trace of plain values, for readings that have no shared timeline.
pub(super) fn sparkline(values: Vec<f32>, maximum: f32, color: Hsla) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left(), bounds.bottom() - px(1.)),
                    size(bounds.size.width, px(1.)),
                ),
                color.opacity(0.25),
            ));
            if values.len() < 2 {
                return;
            }
            let mut line = PathBuilder::stroke(px(1.25));
            let step = bounds.size.width / 119.;
            let start = bounds.right() - step * (values.len() - 1) as f32;
            for (index, value) in values.iter().enumerate() {
                let at = point(
                    start + step * index as f32,
                    bounds.bottom() - bounds.size.height * (value / maximum).clamp(0., 1.),
                );
                if index == 0 {
                    line.move_to(at);
                } else {
                    line.line_to(at);
                }
            }
            if let Ok(path) = line.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w_full()
    .h(px(26.))
    .into_any_element()
}

/// Rounds a scale up to a value that reads cleanly on an axis label.
pub(super) fn nice_ceiling(value: f64) -> f64 {
    if value <= 0. {
        return 1.;
    }
    let magnitude = 10f64.powf(value.log10().floor());
    for step in [1., 2., 5., 10.] {
        if value <= step * magnitude {
            return step * magnitude;
        }
    }
    10. * magnitude
}

/// `nice_ceiling` for byte rates, stepping in the same binary units they display in.
pub(super) fn nice_rate(value: f64) -> f64 {
    let mut unit = 1.;
    while value / unit >= 1024. {
        unit *= 1024.;
    }
    nice_ceiling(value / unit) * unit
}

#[cfg(test)]
mod tests {
    use super::{nice_ceiling, nice_rate};

    #[test]
    fn scale_rounds_up_to_clean_steps() {
        assert_eq!(nice_ceiling(175_000.), 200_000.);
        assert_eq!(nice_ceiling(2_100.), 5_000.);
        assert_eq!(nice_ceiling(1_000.), 1_000.);
        assert_eq!(nice_ceiling(0.), 1.);
    }

    #[test]
    fn rate_scale_rounds_in_display_units() {
        assert_eq!(nice_rate(180. * 1024. * 1024.), 200. * 1024. * 1024.);
        assert_eq!(nice_rate(1024.), 1024.);
        assert_eq!(nice_rate(700.), 1000.);
    }
}

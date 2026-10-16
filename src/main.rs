mod app;
mod collector;
mod model;
mod monitor;
mod preferences;
mod tray;

use std::{
    io::{self, Write},
    time::Duration,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        if let Err(error) = sample_cli(&args) {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return;
    }
    use gpui::*;
    Application::new()
        .with_assets(gpui_component_assets::Assets)
        .run(|cx| {
            gpui_component::init(cx);
            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
            app::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1320.), px(900.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(1000.), px(700.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Sysly".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| app::App::new(window, cx));
                    cx.new(|cx| gpui_component::Root::new(view, window, cx))
                },
            )
            .expect("open Sysly window");
            cx.activate(true);
        });
}

fn sample_cli(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("--sample") || args.len() > 3 {
        return Err("Usage: sysly [--sample [count=5] [interval-seconds=1]]".into());
    }
    let count = args
        .get(1)
        .map(|s| s.parse::<usize>())
        .transpose()
        .map_err(|_| "Invalid sample count")?
        .unwrap_or(5);
    let seconds = args
        .get(2)
        .map(|s| s.parse::<u64>())
        .transpose()
        .map_err(|_| "Invalid interval")?
        .unwrap_or(1);
    if !(1..=3600).contains(&count) || !(1..=10).contains(&seconds) {
        return Err("Sample count must be 1..3600 and interval 1..10 seconds".into());
    }
    let mut collector = collector::Collector::new();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for _ in 0..count {
        let sample = collector.sample(Duration::from_secs(seconds));
        serde_json::to_writer(&mut out, &sample).map_err(|e| e.to_string())?;
        writeln!(out).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

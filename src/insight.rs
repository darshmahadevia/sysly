//! Turns raw readings into plain language. Everything here is pure so the
//! wording and thresholds can be tested without a running Mac.

use crate::model::{Pressure, Process, ProcessKey, Thermal};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Calm,
    Busy,
    Strained,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    pub level: Level,
    pub headline: String,
    pub detail: String,
    /// The process worth opening when the verdict names one.
    pub culprit: Option<(ProcessKey, String)>,
}

/// One row of "what is using my Mac": an app with its helpers folded in, or a
/// lone background process.
#[derive(Clone, Debug, PartialEq)]
pub struct Consumer {
    pub name: String,
    /// The `.app` bundle, when the process belongs to one.
    pub bundle: Option<String>,
    /// The process that stands for the whole row: signals go here.
    pub lead: usize,
    pub count: usize,
    pub cpu: f32,
    pub memory: u64,
    pub io: f64,
}

/// The outermost `.app` bundle in a command line, if there is one.
pub fn app_bundle(command: &str) -> Option<&str> {
    if !command.starts_with('/') {
        return None;
    }
    let end = command.find(".app/")? + 4;
    Some(&command[..end])
}

fn bundle_name(bundle: &str) -> &str {
    let file = bundle.rsplit('/').next().unwrap_or(bundle);
    file.strip_suffix(".app").unwrap_or(file)
}

/// Folds helpers into their app. A row's lead is the process closest to the
/// app's main executable, which is the one that should receive a quit request.
pub fn consumers(processes: &[Process]) -> Vec<Consumer> {
    let mut rows: Vec<Consumer> = Vec::with_capacity(processes.len());
    let mut by_bundle: HashMap<&str, usize> = HashMap::new();
    for (index, process) in processes.iter().enumerate() {
        let io = process.read_per_sec + process.write_per_sec;
        let Some(bundle) = app_bundle(&process.command) else {
            rows.push(Consumer {
                name: process.name.clone(),
                bundle: None,
                lead: index,
                count: 1,
                cpu: process.cpu_percent,
                memory: process.memory,
                io,
            });
            continue;
        };
        match by_bundle.get(bundle) {
            Some(&row) => {
                let row = &mut rows[row];
                row.count += 1;
                row.cpu += process.cpu_percent;
                row.memory += process.memory;
                row.io += io;
                let lead = &processes[row.lead];
                if process.command.len() < lead.command.len() {
                    row.lead = index;
                }
            }
            None => {
                by_bundle.insert(bundle, rows.len());
                rows.push(Consumer {
                    name: bundle_name(bundle).into(),
                    bundle: Some(bundle.into()),
                    lead: index,
                    count: 1,
                    cpu: process.cpu_percent,
                    memory: process.memory,
                    io,
                });
            }
        }
    }
    rows
}

#[derive(Clone, Debug, Default)]
pub struct Signals {
    pub ready: bool,
    /// Mean total CPU over the last 30 seconds, so one spike cannot flip the sentence.
    pub cpu_recent: f32,
    pub pressure: Pressure,
    pub thermal: Thermal,
    pub battery_percent: Option<f32>,
    pub on_battery: bool,
    pub startup_free_ratio: Option<f32>,
    pub startup_free: Option<String>,
    pub top_cpu: Option<(ProcessKey, String, f32)>,
    pub top_memory: Option<(ProcessKey, String, String)>,
}

pub fn assess(s: &Signals) -> Verdict {
    if !s.ready {
        return Verdict {
            level: Level::Calm,
            headline: "Checking your Mac…".into(),
            detail: "The first readings arrive in a moment.".into(),
            culprit: None,
        };
    }
    let cpu_culprit = || {
        s.top_cpu
            .as_ref()
            .filter(|(_, _, cpu)| *cpu >= 40.)
            .map(|(key, name, _)| (*key, name.clone()))
    };
    let memory_culprit = || s.top_memory.as_ref().map(|(k, n, _)| (*k, n.clone()));
    if matches!(s.thermal, Thermal::Serious | Thermal::Critical) {
        let culprit = cpu_culprit();
        return Verdict {
            level: Level::Strained,
            headline: "Your Mac is running hot.".into(),
            detail: match &culprit {
                Some((_, name)) => {
                    format!(
                        "macOS is limiting performance to cool down. {name} is working the hardest."
                    )
                }
                None => "macOS is limiting performance to cool down.".into(),
            },
            culprit,
        };
    }
    if s.pressure == Pressure::Critical {
        return Verdict {
            level: Level::Strained,
            headline: "Your Mac is out of memory.".into(),
            detail: match &s.top_memory {
                Some((_, name, amount)) => format!(
                    "Apps may slow down or freeze. {name} is using the most, at {amount}. Quitting it frees the most room."
                ),
                None => "Apps may slow down or freeze. Quit apps you are not using.".into(),
            },
            culprit: memory_culprit(),
        };
    }
    if s.startup_free_ratio.is_some_and(|free| free < 0.05) {
        return Verdict {
            level: Level::Strained,
            headline: "Your startup disk is almost full.".into(),
            detail: format!(
                "Only {} is left. macOS needs free space to run smoothly.",
                s.startup_free.as_deref().unwrap_or("a little")
            ),
            culprit: None,
        };
    }
    if s.on_battery && s.battery_percent.is_some_and(|percent| percent < 10.) {
        return Verdict {
            level: Level::Strained,
            headline: "Your battery is nearly empty.".into(),
            detail: format!(
                "{:.0}% left. Plug in soon to keep working.",
                s.battery_percent.unwrap_or_default()
            ),
            culprit: None,
        };
    }
    if s.pressure == Pressure::Warning {
        return Verdict {
            level: Level::Busy,
            headline: "Memory is getting tight.".into(),
            detail: match &s.top_memory {
                Some((_, name, amount)) => format!(
                    "macOS is compressing memory to keep up. {name} is using the most, at {amount}."
                ),
                None => "macOS is compressing memory to keep up.".into(),
            },
            culprit: memory_culprit(),
        };
    }
    if s.cpu_recent >= 70. {
        let culprit = cpu_culprit();
        return Verdict {
            level: Level::Busy,
            headline: "Your Mac is working hard.".into(),
            detail: match &s.top_cpu {
                Some((_, name, _)) if culprit.is_some() => format!(
                    "The processor has been {:.0}% busy for the last half minute, mostly because of {name}.",
                    s.cpu_recent
                ),
                _ => format!(
                    "The processor has been {:.0}% busy for the last half minute, spread across many tasks.",
                    s.cpu_recent
                ),
            },
            culprit,
        };
    }
    if let Some((key, name, cpu)) = s.top_cpu.as_ref().filter(|(_, _, cpu)| *cpu >= 100.) {
        return Verdict {
            level: Level::Busy,
            headline: format!("{name} is busy."),
            detail: format!(
                "It is using {:.1} cores' worth of processing. The rest of your Mac has room to spare.",
                cpu / 100.
            ),
            culprit: Some((*key, name.clone())),
        };
    }
    if s.startup_free_ratio.is_some_and(|free| free < 0.10) {
        return Verdict {
            level: Level::Busy,
            headline: "Your startup disk is filling up.".into(),
            detail: format!(
                "{} is left. Clear some space before it affects your Mac.",
                s.startup_free.as_deref().unwrap_or("Little space")
            ),
            culprit: None,
        };
    }
    let mut facts = vec![if s.cpu_recent < 20. {
        "The processor is mostly idle".to_string()
    } else {
        "The processor has plenty of headroom".to_string()
    }];
    if s.pressure == Pressure::Normal {
        facts.push("memory has room".into());
    }
    if let Some(free) = &s.startup_free {
        facts.push(format!("{free} of storage is free"));
    }
    let detail = match facts.as_slice() {
        [one] => format!("{one}."),
        [one, two] => format!("{one} and {two}."),
        [one, two, three] => format!("{one}, {two}, and {three}."),
        _ => String::new(),
    };
    Verdict {
        level: Level::Calm,
        headline: "Your Mac is running well.".into(),
        detail,
        culprit: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, name: &str, command: &str, cpu: f32, memory: u64) -> Process {
        Process {
            key: ProcessKey { pid, started_at: 1 },
            parent: None,
            name: name.into(),
            user: "tester".into(),
            command: command.into(),
            status: "Running".into(),
            cpu_percent: cpu,
            memory,
            virtual_memory: 0,
            read_per_sec: 1.,
            write_per_sec: 2.,
            run_secs: 1,
        }
    }

    fn calm() -> Signals {
        Signals {
            ready: true,
            cpu_recent: 12.,
            pressure: Pressure::Normal,
            thermal: Thermal::Nominal,
            startup_free_ratio: Some(0.5),
            startup_free: Some("400 GB".into()),
            ..Signals::default()
        }
    }

    fn key(pid: u32) -> ProcessKey {
        ProcessKey { pid, started_at: 1 }
    }

    #[test]
    fn finds_the_outermost_bundle_only_for_absolute_paths() {
        assert_eq!(
            app_bundle(
                "/Applications/Brave Browser.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper --type=gpu"
            ),
            Some("/Applications/Brave Browser.app")
        );
        assert_eq!(app_bundle("/usr/libexec/trustd"), None);
        assert_eq!(app_bundle("node /tmp/x.app/run.js"), None);
        assert_eq!(app_bundle(""), None);
    }

    #[test]
    fn folds_helpers_and_leads_with_the_main_executable() {
        let processes = vec![
            process(
                1,
                "Helper",
                "/Applications/Web.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper",
                30.,
                100,
            ),
            process(
                2,
                "Web",
                "/Applications/Web.app/Contents/MacOS/Web",
                10.,
                50,
            ),
            process(3, "trustd", "/usr/libexec/trustd", 5., 7),
        ];
        let rows = consumers(&processes);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Web");
        assert_eq!(rows[0].count, 2);
        assert_eq!(rows[0].cpu, 40.);
        assert_eq!(rows[0].memory, 150);
        assert_eq!(rows[0].io, 6.);
        assert_eq!(rows[0].lead, 1);
        assert_eq!(rows[1].name, "trustd");
        assert_eq!(rows[1].bundle, None);
    }

    #[test]
    fn says_nothing_definite_before_the_first_sample() {
        let verdict = assess(&Signals::default());
        assert_eq!(verdict.level, Level::Calm);
        assert_eq!(verdict.headline, "Checking your Mac…");
    }

    #[test]
    fn calm_lists_only_what_is_known() {
        assert_eq!(
            assess(&calm()).detail,
            "The processor is mostly idle, memory has room, and 400 GB of storage is free."
        );
        let mut unknown = calm();
        unknown.pressure = Pressure::Unknown;
        unknown.startup_free = None;
        assert_eq!(assess(&unknown).detail, "The processor is mostly idle.");
    }

    #[test]
    fn pressure_and_heat_outrank_cpu() {
        let mut s = calm();
        s.cpu_recent = 95.;
        s.pressure = Pressure::Critical;
        s.top_memory = Some((key(9), "Photos".into(), "6.1 GB".into()));
        let verdict = assess(&s);
        assert_eq!(verdict.level, Level::Strained);
        assert_eq!(verdict.culprit, Some((key(9), "Photos".into())));
        s.thermal = Thermal::Serious;
        assert_eq!(assess(&s).headline, "Your Mac is running hot.");
    }

    #[test]
    fn busy_names_a_culprit_only_when_one_stands_out() {
        let mut s = calm();
        s.cpu_recent = 80.;
        s.top_cpu = Some((key(4), "Xcode".into(), 300.));
        assert!(assess(&s).detail.contains("Xcode"));
        s.top_cpu = Some((key(4), "Xcode".into(), 12.));
        let verdict = assess(&s);
        assert!(verdict.detail.contains("many tasks"));
        assert_eq!(verdict.culprit, None);
    }

    #[test]
    fn one_busy_process_is_named_even_when_the_mac_is_fine() {
        let mut s = calm();
        s.top_cpu = Some((key(4), "fileproviderd".into(), 130.));
        let verdict = assess(&s);
        assert_eq!(verdict.headline, "fileproviderd is busy.");
        assert_eq!(verdict.level, Level::Busy);
    }

    #[test]
    fn low_battery_only_matters_while_discharging() {
        let mut s = calm();
        s.battery_percent = Some(6.);
        assert_eq!(assess(&s).level, Level::Calm);
        s.on_battery = true;
        assert_eq!(assess(&s).headline, "Your battery is nearly empty.");
    }

    #[test]
    fn storage_escalates_as_it_runs_out() {
        let mut s = calm();
        s.startup_free = Some("30 GB".into());
        s.startup_free_ratio = Some(0.08);
        assert_eq!(assess(&s).level, Level::Busy);
        s.startup_free_ratio = Some(0.03);
        assert_eq!(assess(&s).level, Level::Strained);
    }
}

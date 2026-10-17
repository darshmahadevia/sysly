use crate::{
    collector::{Collector, signal_process},
    model::{ProcessAction, ProcessKey, Snapshot},
};
use std::{
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

struct Policy {
    interval: u64,
    paused: bool,
    visible: bool,
    stopped: bool,
}

pub struct Monitor {
    latest: Arc<Mutex<Arc<Snapshot>>>,
    policy: Arc<(Mutex<Policy>, Condvar)>,
    actions: mpsc::SyncSender<(ProcessKey, ProcessAction)>,
    results: mpsc::Receiver<Result<(), String>>,
}

impl Monitor {
    pub fn start() -> Self {
        let latest = Arc::new(Mutex::new(Arc::new(Snapshot::default())));
        let policy = Arc::new((
            Mutex::new(Policy {
                interval: 1,
                paused: false,
                visible: true,
                stopped: false,
            }),
            Condvar::new(),
        ));
        let (actions, commands) = mpsc::sync_channel(8);
        let (result_sender, results) = mpsc::sync_channel(8);
        thread::Builder::new()
            .name("sysly-actions".into())
            .spawn(move || {
                for (key, action) in commands {
                    if result_sender.send(signal_process(key, action)).is_err() {
                        break;
                    }
                }
            })
            .expect("start process action worker");
        let worker_latest = latest.clone();
        let worker_policy = policy.clone();
        thread::Builder::new()
            .name("sysly-sampler".into())
            .spawn(move || {
                let mut collector = Collector::new();
                let mut last_started = None;
                loop {
                    let (lock, wake) = &*worker_policy;
                    let mut state = lock.lock().unwrap();
                    while !state.stopped {
                        if state.paused {
                            state = wake.wait(state).unwrap();
                            continue;
                        }
                        let interval = Duration::from_secs(if state.visible {
                            state.interval
                        } else {
                            state.interval.max(5)
                        });
                        let remaining = last_started
                            .map(|started: Instant| interval.saturating_sub(started.elapsed()))
                            .unwrap_or_default();
                        if remaining.is_zero() {
                            break;
                        }
                        state = wake.wait_timeout(state, remaining).unwrap().0;
                    }
                    if state.stopped {
                        break;
                    }
                    last_started = Some(Instant::now());
                    drop(state);
                    let snapshot = collector.sample_now();
                    let state = lock.lock().unwrap();
                    if state.stopped {
                        break;
                    }
                    if state.paused {
                        continue;
                    }
                    *worker_latest.lock().unwrap() = Arc::new(snapshot);
                }
            })
            .expect("start metrics worker");
        Self {
            latest,
            policy,
            actions,
            results,
        }
    }

    pub fn latest(&self) -> Arc<Snapshot> {
        self.latest.lock().unwrap().clone()
    }

    pub fn snapshot_reader(&self) -> impl Fn() -> Arc<Snapshot> + 'static {
        let latest = self.latest.clone();
        move || latest.lock().unwrap().clone()
    }

    pub fn set_interval(&self, seconds: u64) {
        self.policy.0.lock().unwrap().interval = seconds.clamp(1, 10);
        self.policy.1.notify_one();
    }

    pub fn set_paused(&self, paused: bool) {
        self.policy.0.lock().unwrap().paused = paused;
        self.policy.1.notify_one();
    }

    pub fn set_visible(&self, visible: bool) {
        self.policy.0.lock().unwrap().visible = visible;
        self.policy.1.notify_one();
    }

    pub fn act(&self, key: ProcessKey, action: ProcessAction) -> Result<(), String> {
        self.actions
            .try_send((key, action))
            .map_err(|e| format!("Cannot queue process action: {e}"))?;
        Ok(())
    }

    pub fn take_action_result(&self) -> Option<Result<(), String>> {
        self.results.try_recv().ok()
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.policy.0.lock().unwrap().stopped = true;
        self.policy.1.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    #[test]
    fn hidden_and_long_intervals_wait_without_delaying_visibility_changes() {
        fn wait_for_sample(monitor: &Monitor, after: u64) -> u64 {
            let deadline = Instant::now() + Duration::from_secs(12);
            loop {
                let sequence = monitor.latest().sequence;
                if sequence > after {
                    return sequence;
                }
                assert!(Instant::now() < deadline, "sampler did not publish");
                thread::sleep(Duration::from_millis(20));
            }
        }

        let monitor = Monitor::start();
        wait_for_sample(&monitor, 0);
        monitor.set_visible(false);
        let hidden_sequence = monitor.latest().sequence;
        thread::sleep(Duration::from_millis(1400));
        assert_eq!(monitor.latest().sequence, hidden_sequence);
        monitor.set_visible(true);
        let deadline = Instant::now() + Duration::from_secs(2);
        let visible_sequence = wait_for_sample(&monitor, hidden_sequence);
        assert!(Instant::now() < deadline, "show waited for hidden interval");

        monitor.set_interval(10);
        thread::sleep(Duration::from_millis(1400));
        assert_eq!(monitor.latest().sequence, visible_sequence);
        monitor.set_interval(1);
        let deadline = Instant::now() + Duration::from_secs(2);
        assert_eq!(
            wait_for_sample(&monitor, visible_sequence),
            visible_sequence + 1
        );
        assert!(
            Instant::now() < deadline,
            "shorter interval did not wake sampler"
        );

        let worker = Arc::downgrade(&monitor.latest);
        drop(monitor);
        let deadline = Instant::now() + Duration::from_secs(2);
        while worker.upgrade().is_some() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            worker.upgrade().is_none(),
            "sampler kept running after drop"
        );
    }

    #[test]
    fn pause_freezes_samples_and_long_sampling_does_not_block_signals() {
        let monitor = Monitor::start();
        let deadline = Instant::now() + Duration::from_secs(12);
        while monitor.latest().sequence == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            monitor.latest().sequence > 0,
            "real sampler did not publish"
        );
        monitor.set_paused(true);
        let sequence = monitor.latest().sequence;
        thread::sleep(Duration::from_millis(1200));
        assert_eq!(
            monitor.latest().sequence,
            sequence,
            "pause published an in-flight sample"
        );
        monitor.set_interval(10);
        monitor.set_paused(false);
        thread::sleep(Duration::from_millis(200));

        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[Pid::from_u32(child.id())]),
            true,
            ProcessRefreshKind::nothing(),
        );
        let key = ProcessKey {
            pid: child.id(),
            started_at: system
                .process(Pid::from_u32(child.id()))
                .unwrap()
                .start_time(),
        };
        monitor.act(key, ProcessAction::Terminate).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut result = None;
        while Instant::now() < deadline {
            if let Some(next) = monitor.take_action_result() {
                result = Some(next);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if result.is_none() {
            let _ = child.kill();
        }
        let status = child.wait().unwrap();
        assert_eq!(
            result,
            Some(Ok(())),
            "signal waited behind hardware sampling"
        );
        assert!(
            !status.success(),
            "owned child did not terminate from signal"
        );
    }
}

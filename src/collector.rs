use crate::model::{
    Battery, Disk, Hardware, Memory, Network, Process, ProcessAction, ProcessKey, Snapshot,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use sysinfo::{
    DiskRefreshKind, Disks, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, System, Uid,
    UpdateKind, Users,
};

#[derive(Default)]
struct CounterRates<K> {
    previous: HashMap<K, (u64, u64, Instant)>,
}

impl<K: Eq + std::hash::Hash> CounterRates<K> {
    fn observe(&mut self, key: K, read: u64, write: u64, now: Instant) -> Option<(f64, f64)> {
        let (old_read, old_write, before) = self.previous.insert(key, (read, write, now))?;
        let seconds = now.duration_since(before).as_secs_f64();
        if seconds <= 0.0 {
            return Some((0.0, 0.0));
        }
        let rate = |current: u64, old| current.saturating_sub(old) as f64 / seconds;
        Some((rate(read, old_read), rate(write, old_write)))
    }
}

pub struct Collector {
    system: System,
    networks: Networks,
    disks: Disks,
    users: HashMap<Uid, String>,
    sampler: Result<macmon::Sampler, String>,
    battery: Option<Battery>,
    battery_warning: Option<String>,
    network_rates: CounterRates<String>,
    process_rates: CounterRates<(u32, u64)>,
    disk_rates: CounterRates<String>,
    started: Instant,
    slow_refresh: Option<Instant>,
    users_refresh: Option<Instant>,
    sequence: u64,
}

impl Collector {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_usage();
        Self {
            system,
            networks: Networks::new(),
            disks: Disks::new(),
            users: HashMap::new(),
            sampler: macmon::Sampler::new().map_err(|error| error.to_string()),
            battery: None,
            battery_warning: None,
            network_rates: CounterRates::default(),
            process_rates: CounterRates::default(),
            disk_rates: CounterRates::default(),
            started: Instant::now(),
            slow_refresh: None,
            users_refresh: None,
            sequence: 0,
        }
    }

    pub fn sample(&mut self, interval: Duration) -> Snapshot {
        let wait_started = Instant::now();
        let hardware = self.sample_hardware(interval);
        if let Some(remaining) = interval.checked_sub(wait_started.elapsed()) {
            std::thread::sleep(remaining);
        }
        self.collect(hardware)
    }

    pub fn sample_now(&mut self) -> Snapshot {
        // IOReport retains its baseline across the monitor's interruptible wait.
        let hardware = self.sample_hardware(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        self.collect(hardware)
    }

    fn sample_hardware(&mut self, interval: Duration) -> Hardware {
        match &mut self.sampler {
            Ok(sampler) => match sampler
                .get_metrics(interval.as_millis().clamp(1, u32::MAX as u128) as u32)
            {
                Ok(metrics) => {
                    let positive = |value: f32| (value.is_finite() && value > 0.0).then_some(value);
                    Hardware {
                        status: "IOReport · GPU active residency · power estimates".into(),
                        cpu_temp: metrics.temp.cpu_temp_avg,
                        gpu_temp: metrics.temp.gpu_temp_avg,
                        gpu_percent: (sampler.get_soc_info().gpu_freqs.len() > 1
                            && metrics.gpu_freq_mhz > 0)
                            .then_some(metrics.gpu_active_ratio * 100.0),
                        gpu_mhz: (metrics.gpu_freq_mhz > 0).then_some(metrics.gpu_freq_mhz),
                        cpu_watts: positive(metrics.cpu_power),
                        gpu_watts: positive(metrics.gpu_power),
                        system_watts: positive(metrics.sys_power),
                        tiers: metrics
                            .cpu_tiers
                            .into_iter()
                            .map(|tier| (tier.label, tier.freq_mhz))
                            .collect(),
                        fans: metrics
                            .fans
                            .into_iter()
                            .map(|fan| (fan.name, fan.rpm))
                            .collect(),
                    }
                }
                Err(error) => Hardware {
                    status: format!("Hardware sampling unavailable: {error}"),
                    ..Hardware::default()
                },
            },
            Err(error) => Hardware {
                status: format!("Hardware sampling unavailable: {error}"),
                ..Hardware::default()
            },
        }
    }

    fn collect(&mut self, hardware: Hardware) -> Snapshot {
        let collecting = Instant::now();
        let slow = self
            .slow_refresh
            .is_none_or(|last| collecting.duration_since(last) >= Duration::from_secs(5));
        if slow {
            self.disks.refresh(true);
            self.refresh_battery();
            self.slow_refresh = Some(collecting);
        } else {
            for disk in self.disks.list_mut() {
                disk.refresh_specifics(DiskRefreshKind::nothing().with_io_usage());
            }
        }
        if self
            .users_refresh
            .is_none_or(|last| collecting.duration_since(last) >= Duration::from_secs(60))
        {
            self.users = Users::new_with_refreshed_list()
                .iter()
                .map(|user| (user.id().clone(), user.name().to_owned()))
                .collect();
            self.users_refresh = Some(collecting);
        }
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh());
        self.networks.refresh(true);
        let now = Instant::now();
        let mut warnings = Vec::new();
        if hardware.gpu_percent.is_none() {
            warnings.push(format!(
                "GPU active residency unavailable. {}",
                hardware.status
            ));
        }
        if let Some(warning) = &self.battery_warning {
            warnings.push(warning.clone());
        }
        let networks = self
            .networks
            .iter()
            .map(|(name, network)| {
                let (received_per_sec, sent_per_sec) = self
                    .network_rates
                    .observe(
                        name.clone(),
                        network.total_received(),
                        network.total_transmitted(),
                        now,
                    )
                    .unwrap_or_default();
                Network {
                    name: name.clone(),
                    addresses: network
                        .ip_networks()
                        .iter()
                        .map(|address| address.addr.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    received_per_sec,
                    sent_per_sec,
                    total_received: network.total_received(),
                    total_sent: network.total_transmitted(),
                    errors: network
                        .total_errors_on_received()
                        .saturating_add(network.total_errors_on_transmitted()),
                }
            })
            .collect::<Vec<_>>();
        self.network_rates
            .previous
            .retain(|name, _| self.networks.contains_key(name));
        let disks = self
            .disks
            .iter()
            .map(|disk| {
                let mount = disk.mount_point().to_string_lossy().into_owned();
                let io = disk.usage();
                // sysinfo cannot distinguish unsupported IO counters from lifetime zero counters.
                let available = io.total_read_bytes > 0 || io.total_written_bytes > 0;
                let rates = self
                    .disk_rates
                    .observe(
                        mount.clone(),
                        io.total_read_bytes,
                        io.total_written_bytes,
                        now,
                    )
                    .unwrap_or_default();
                if !available {
                    warnings.push(format!("Disk IO counters unavailable for {mount}"));
                }
                Disk {
                    name: disk.name().to_string_lossy().into_owned(),
                    mount,
                    total: disk.total_space(),
                    available: disk.available_space(),
                    read_per_sec: available.then_some(rates.0),
                    write_per_sec: available.then_some(rates.1),
                }
            })
            .collect::<Vec<_>>();
        if self
            .disks
            .iter()
            .filter(|disk| disk.file_system() == "apfs")
            .count()
            > 1
        {
            warnings.push(
                "APFS volumes may share physical disk IO counters; do not sum volume throughput"
                    .into(),
            );
        }
        self.disk_rates
            .previous
            .retain(|mount, _| disks.iter().any(|disk| &disk.mount == mount));
        let processes = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| {
                let key = ProcessKey {
                    pid: pid.as_u32(),
                    started_at: process.start_time(),
                };
                let io = process.disk_usage();
                let identity = (key.pid, key.started_at);
                let rates = self.process_rates.observe(
                    identity,
                    io.total_read_bytes,
                    io.total_written_bytes,
                    now,
                );
                let known = rates.is_some();
                let rates = rates.unwrap_or_default();
                Process {
                    key,
                    parent: process.parent().map(|pid| pid.as_u32()),
                    name: process.name().to_string_lossy().into_owned(),
                    user: process
                        .user_id()
                        .and_then(|id| self.users.get(id))
                        .cloned()
                        .unwrap_or_else(|| "Unknown".into()),
                    command: command_line(process.cmd()),
                    status: process.status().to_string(),
                    cpu_percent: if known { process.cpu_usage() } else { 0.0 },
                    memory: process.memory(),
                    virtual_memory: process.virtual_memory(),
                    read_per_sec: rates.0,
                    write_per_sec: rates.1,
                    run_secs: process.run_time(),
                }
            })
            .collect::<Vec<_>>();
        self.process_rates.previous.retain(|(pid, started_at), _| {
            self.system
                .process(Pid::from_u32(*pid))
                .is_some_and(|process| process.start_time() == *started_at)
        });
        self.sequence += 1;
        let load = System::load_average();
        Snapshot {
            sequence: self.sequence,
            elapsed_secs: self.started.elapsed().as_secs_f64(),
            collected_ms: collecting.elapsed().as_secs_f64() * 1000.0,
            cpu_name: self
                .system
                .cpus()
                .first()
                .map(|cpu| cpu.brand().to_owned())
                .unwrap_or_else(|| "CPU".into()),
            cpu_percent: self.system.global_cpu_usage(),
            cores: self
                .system
                .cpus()
                .iter()
                .map(|cpu| cpu.cpu_usage())
                .collect(),
            load: [load.one, load.five, load.fifteen],
            uptime_secs: System::uptime(),
            memory: Memory {
                total: self.system.total_memory(),
                used: self.system.used_memory(),
                available: self.system.available_memory(),
                swap_total: self.system.total_swap(),
                swap_used: self.system.used_swap(),
            },
            hardware,
            battery: self.battery.clone(),
            networks,
            disks,
            processes,
            warnings,
        }
    }

    fn refresh_battery(&mut self) {
        match read_battery() {
            Ok(battery) => {
                self.battery = battery;
                self.battery_warning = None;
            }
            Err(error) => {
                self.battery = None;
                self.battery_warning = Some(format!("Battery unavailable: {error}"));
            }
        }
    }
}

fn command_line(parts: &[std::ffi::OsString]) -> String {
    let capacity =
        parts.iter().map(|part| part.len()).sum::<usize>() + parts.len().saturating_sub(1);
    let mut command = String::with_capacity(capacity);
    for (index, part) in parts.iter().enumerate() {
        if index != 0 {
            command.push(' ');
        }
        command.push_str(&part.to_string_lossy());
    }
    command
}

fn process_refresh() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_user(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
}

pub fn signal_process(key: ProcessKey, action: ProcessAction) -> Result<(), String> {
    if key.pid <= 1 || key.pid == std::process::id() {
        return Err("System processes and Sysly cannot be targeted".into());
    }
    if key.started_at == 0 {
        return Err("Process start time unavailable; cannot safely target process".into());
    }
    let pid = i32::try_from(key.pid).map_err(|_| "Invalid process identifier")?;
    let mut fresh = System::new();
    fresh.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(key.pid)]),
        true,
        ProcessRefreshKind::nothing(),
    );
    let process = fresh
        .process(Pid::from_u32(key.pid))
        .ok_or("Process has exited")?;
    if process.start_time() != key.started_at {
        return Err("Process identity changed; select the process again".into());
    }
    let signal = match action {
        ProcessAction::Terminate => libc::SIGTERM,
        ProcessAction::Kill => libc::SIGKILL,
        ProcessAction::Stop => libc::SIGSTOP,
        ProcessAction::Continue => libc::SIGCONT,
    };
    if unsafe { libc::kill(pid, signal) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

fn read_battery() -> Result<Option<Battery>, String> {
    use core_foundation::{
        base::{CFType, CFTypeRef, TCFType, kCFAllocatorDefault},
        boolean::CFBoolean,
        dictionary::{CFDictionary, CFDictionaryRef},
        number::CFNumber,
        string::{CFString, CFStringRef},
    };
    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOServiceMatching(name: *const libc::c_char) -> CFDictionaryRef;
        fn IOServiceGetMatchingService(port: u32, matching: CFDictionaryRef) -> u32;
        fn IORegistryEntryCreateCFProperty(
            entry: u32,
            key: CFStringRef,
            allocator: *const libc::c_void,
            options: u32,
        ) -> CFTypeRef;
        fn IOObjectRelease(object: u32) -> i32;
    }
    let matching = unsafe { IOServiceMatching(c"AppleSmartBattery".as_ptr()) };
    if matching.is_null() {
        return Err("Battery registry matching failed".into());
    }
    let service = unsafe { IOServiceGetMatchingService(0, matching) };
    if service == 0 {
        return Ok(None);
    }
    let read = |key: &str| -> Option<CFType> {
        let key = CFString::new(key);
        let value = unsafe {
            IORegistryEntryCreateCFProperty(
                service,
                key.as_concrete_TypeRef(),
                kCFAllocatorDefault,
                0,
            )
        };
        (!value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
    };
    let number = |key: &str| read(key)?.downcast::<CFNumber>()?.to_i64();
    let flag = |key: &str| read(key)?.downcast::<CFBoolean>().map(bool::from);
    let result = (|| {
        let current = number("CurrentCapacity").ok_or("CurrentCapacity unavailable")?;
        let maximum = number("MaxCapacity")
            .filter(|value| *value > 0)
            .ok_or("MaxCapacity unavailable")?;
        let voltage = number("Voltage").ok_or("Battery voltage unavailable")?;
        let amperage = number("Amperage").ok_or("Battery current unavailable")?;
        let charging = flag("IsCharging").ok_or("Charging state unavailable")?;
        let external = flag("ExternalConnected").ok_or("Power source unavailable")?;
        let details = read("BatteryData").and_then(|data| data.downcast::<CFDictionary>());
        let detail_number = |key: &str| -> Option<i64> {
            let dictionary = details.as_ref()?;
            let key = CFString::new(key);
            let value = dictionary.find(key.as_CFTypeRef())?;
            let value = unsafe { CFType::wrap_under_get_rule(*value) };
            value.downcast::<CFNumber>()?.to_i64()
        };
        let design = number("DesignCapacity")
            .or_else(|| detail_number("DesignCapacity"))
            .filter(|value| *value > 0)
            .ok_or("DesignCapacity unavailable")?;
        let full = number("AppleRawMaxCapacity")
            .or_else(|| detail_number("FullChargeCapacity"))
            .ok_or("FullChargeCapacity unavailable")?;
        Ok(Some(Battery {
            percent: (current as f32 / maximum as f32 * 100.0).clamp(0.0, 100.0),
            state: if charging {
                "Charging"
            } else if external {
                "Plugged in"
            } else {
                "Discharging"
            }
            .into(),
            remaining_secs: number("TimeRemaining")
                .filter(|minutes| (1..65535).contains(minutes))
                .map(|minutes| minutes as u64 * 60),
            watts: (voltage as f64 * amperage as f64 / 1_000_000.0).abs() as f32,
            health_percent: full as f32 / design as f32 * 100.0,
            cycles: number("CycleCount").and_then(|value| u32::try_from(value).ok()),
        }))
    })();
    unsafe {
        IOObjectRelease(service);
    }
    result.map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_preserves_argument_spacing_and_lossy_text() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        assert_eq!(
            command_line(&[
                OsString::from("printf"),
                OsString::from(""),
                OsString::from("hello world"),
                OsString::from_vec(vec![b'f', 0xff]),
            ]),
            "printf  hello world f�"
        );
        assert_eq!(command_line(&[]), "");
    }

    #[test]
    fn samples_preserve_commands_and_reuse_user_names_until_refresh() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let mut collector = Collector::new();
        let first = collector.sample_now();
        let row = first
            .processes
            .iter()
            .find(|process| process.key.pid == child.id())
            .unwrap();
        assert_eq!(row.command, "/bin/sleep 30");
        assert_eq!(
            (row.cpu_percent, row.read_per_sec, row.write_per_sec),
            (0.0, 0.0, 0.0)
        );
        let owner = collector
            .system
            .process(Pid::from_u32(child.id()))
            .unwrap()
            .user_id()
            .unwrap()
            .clone();
        collector.users.insert(owner, "Cached user".into());
        let second = collector.sample_now();
        let row = second
            .processes
            .iter()
            .find(|process| process.key.pid == child.id())
            .unwrap();
        assert_eq!(row.user, "Cached user");
        assert_eq!(row.command, "/bin/sleep 30");
        child.kill().unwrap();
        child.wait().unwrap();
        let third = collector.sample_now();
        assert!(
            third
                .processes
                .iter()
                .all(|process| process.key.pid != child.id())
        );
        assert!(
            collector
                .process_rates
                .previous
                .keys()
                .all(|(pid, _)| *pid != child.id())
        );
    }

    #[test]
    fn rates_use_elapsed_time_and_reset_baselines() {
        let mut rates = CounterRates::default();
        let now = Instant::now();
        assert_eq!(rates.observe("en0", 100, 200, now), None);
        assert_eq!(
            rates.observe("en0", 300, 500, now + Duration::from_secs(2)),
            Some((100.0, 150.0))
        );
        assert_eq!(
            rates.observe("en0", 10, 20, now + Duration::from_secs(3)),
            Some((0.0, 0.0))
        );
        assert_eq!(
            rates.observe("en0", 30, 50, now + Duration::from_secs(4)),
            Some((20.0, 30.0))
        );
    }

    #[test]
    fn signals_revalidate_identity_and_terminate_owned_child() {
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
        let process = system.process(Pid::from_u32(child.id())).unwrap();
        let key = ProcessKey {
            pid: child.id(),
            started_at: process.start_time(),
        };
        assert!(
            signal_process(
                ProcessKey {
                    started_at: key.started_at + 1,
                    ..key
                },
                ProcessAction::Terminate
            )
            .unwrap_err()
            .contains("identity changed")
        );
        assert!(child.try_wait().unwrap().is_none());
        signal_process(key, ProcessAction::Terminate).unwrap();
        assert!(!child.wait().unwrap().success());
        assert!(
            signal_process(
                ProcessKey {
                    pid: std::process::id(),
                    started_at: 0
                },
                ProcessAction::Kill
            )
            .is_err()
        );
    }
}

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub sequence: u64,
    pub elapsed_secs: f64,
    pub collected_ms: f64,
    pub cpu_name: String,
    pub cpu_percent: f32,
    pub cores: Vec<f32>,
    pub load: [f64; 3],
    pub uptime_secs: u64,
    pub memory: Memory,
    pub hardware: Hardware,
    pub battery: Option<Battery>,
    pub networks: Vec<Network>,
    pub disks: Vec<Disk>,
    pub processes: Vec<Process>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Memory {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Hardware {
    pub status: String,
    pub cpu_temp: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub gpu_percent: Option<f32>,
    pub gpu_mhz: Option<u32>,
    pub cpu_watts: Option<f32>,
    pub gpu_watts: Option<f32>,
    pub system_watts: Option<f32>,
    pub tiers: Vec<(String, u32)>,
    pub fans: Vec<(String, u32)>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Battery {
    pub percent: f32,
    pub state: String,
    pub remaining_secs: Option<u64>,
    pub watts: f32,
    pub health_percent: f32,
    pub cycles: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Network {
    pub name: String,
    pub addresses: String,
    pub received_per_sec: f64,
    pub sent_per_sec: f64,
    pub total_received: u64,
    pub total_sent: u64,
    pub errors: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Disk {
    pub name: String,
    pub mount: String,
    pub total: u64,
    pub available: u64,
    pub read_per_sec: Option<f64>,
    pub write_per_sec: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessKey {
    pub pid: u32,
    pub started_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Process {
    pub key: ProcessKey,
    pub parent: Option<u32>,
    pub name: String,
    pub user: String,
    pub command: String,
    pub status: String,
    pub cpu_percent: f32,
    pub memory: u64,
    pub virtual_memory: u64,
    pub read_per_sec: f64,
    pub write_per_sec: f64,
    pub run_secs: u64,
}

#[derive(Clone, Copy, Debug)]
pub enum ProcessAction {
    Terminate,
    Kill,
    Stop,
    Continue,
}

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sysinfo::{Disks, Networks, System};
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub hostname: String,
    pub uptime_secs: u64,
    pub cpu_usage: f32,
    pub cpu_count: usize,
    pub cpu_model: String,
    pub ram_total: u64,
    pub ram_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub load_avg: [f64; 3],
    pub process_count: usize,
    pub os_name: String,
    pub kernel_version: String,
    pub disks: Vec<DiskInfo>,
    pub networks: Vec<NetworkInfo>,
    pub top_cpu_procs: Vec<ProcessInfo>,
    pub top_mem_procs: Vec<ProcessInfo>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub fs_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
}

pub type MetricsBroadcaster = broadcast::Sender<MetricsSnapshot>;

pub struct MetricsCollector {
    sys: Arc<RwLock<System>>,
    broadcaster: MetricsBroadcaster,
    prev_rx: std::collections::HashMap<String, u64>,
    prev_tx: std::collections::HashMap<String, u64>,
    prev_time: std::time::Instant,
}

impl MetricsCollector {
    pub fn new(broadcaster: MetricsBroadcaster) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys: Arc::new(RwLock::new(sys)),
            broadcaster,
            prev_rx: std::collections::HashMap::new(),
            prev_tx: std::collections::HashMap::new(),
            prev_time: std::time::Instant::now(),
        }
    }

    pub async fn collect(&mut self) -> MetricsSnapshot {
        let mut sys = self.sys.write().await;
        sys.refresh_all();

        let elapsed = self.prev_time.elapsed().as_secs_f64().max(0.1);
        self.prev_time = std::time::Instant::now();

        let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());
        let uptime_secs = System::uptime();
        let cpu_usage = sys.global_cpu_info().cpu_usage();
        let cpu_count = sys.cpus().len();
        let cpu_model = sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_default();
        let ram_total = sys.total_memory();
        let ram_used = sys.used_memory();
        let swap_total = sys.total_swap();
        let swap_used = sys.used_swap();
        let process_count = sys.processes().len();
        let os_name = System::long_os_version().unwrap_or_else(|| System::name().unwrap_or_else(|| "Linux".to_string()));
        let kernel_version = System::kernel_version().unwrap_or_default();

        let load_avg = System::load_average();
        let load = [load_avg.one, load_avg.five, load_avg.fifteen];

        // Disks
        let disks_info = Disks::new_with_refreshed_list();
        let disks: Vec<DiskInfo> = disks_info
            .iter()
            .map(|d| DiskInfo {
                name: d.name().to_string_lossy().to_string(),
                mount_point: d.mount_point().to_string_lossy().to_string(),
                total: d.total_space(),
                used: d.total_space().saturating_sub(d.available_space()),
                available: d.available_space(),
                fs_type: d.file_system().to_string_lossy().to_string(),
            })
            .collect();

        // Networks with per-second rates
        let networks_info = Networks::new_with_refreshed_list();
        let mut networks = Vec::new();
        for (name, data) in networks_info.iter() {
            let rx = data.total_received();
            let tx = data.total_transmitted();
            let prev_rx = self.prev_rx.get(name).copied().unwrap_or(rx);
            let prev_tx = self.prev_tx.get(name).copied().unwrap_or(tx);
            let rx_per_sec = (rx.saturating_sub(prev_rx)) as f64 / elapsed;
            let tx_per_sec = (tx.saturating_sub(prev_tx)) as f64 / elapsed;
            self.prev_rx.insert(name.clone(), rx);
            self.prev_tx.insert(name.clone(), tx);
            networks.push(NetworkInfo {
                name: name.clone(),
                rx_bytes: rx,
                tx_bytes: tx,
                rx_bytes_per_sec: rx_per_sec,
                tx_bytes_per_sec: tx_per_sec,
            });
        }

        // Top processes by CPU
        let mut procs: Vec<_> = sys.processes().values().collect();
        procs.sort_by(|a, b| b.cpu_usage().partial_cmp(&a.cpu_usage()).unwrap_or(std::cmp::Ordering::Equal));
        let top_cpu_procs: Vec<ProcessInfo> = procs.iter().take(5).map(|p| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string(),
            cpu_usage: p.cpu_usage(),
            memory_bytes: p.memory(),
        }).collect();

        // Top processes by memory
        procs.sort_by_key(|p| std::cmp::Reverse(p.memory()));
        let top_mem_procs: Vec<ProcessInfo> = procs.iter().take(5).map(|p| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string(),
            cpu_usage: p.cpu_usage(),
            memory_bytes: p.memory(),
        }).collect();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        MetricsSnapshot {
            hostname,
            uptime_secs,
            cpu_usage,
            cpu_count,
            cpu_model,
            ram_total,
            ram_used,
            swap_total,
            swap_used,
            load_avg: load,
            process_count,
            os_name,
            kernel_version,
            disks,
            networks,
            top_cpu_procs,
            top_mem_procs,
            timestamp,
        }
    }

    pub async fn run_loop(mut self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));
        loop {
            interval.tick().await;
            let snapshot = self.collect().await;
            // Ignore send errors (no subscribers is fine)
            let _ = self.broadcaster.send(snapshot);
        }
    }
}

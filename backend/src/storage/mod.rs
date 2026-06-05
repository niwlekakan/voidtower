use serde::Serialize;
use tokio::process::Command;

// ─── Data structures ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct BlockDevice {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub device_type: String,
    pub mountpoint: Option<String>,
    pub fstype: Option<String>,
    pub label: Option<String>,
    pub uuid: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub vendor: Option<String>,
    pub removable: bool,
    pub read_only: bool,
    pub state: Option<String>,
    pub children: Vec<BlockDevice>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MountInfo {
    pub device: String,
    pub mountpoint: String,
    pub fstype: String,
    pub options: String,
    pub size_bytes: u64,
    pub used_bytes: u64,
    pub avail_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FstabEntry {
    pub device: String,
    pub mountpoint: String,
    pub fstype: String,
    pub options: String,
    pub dump: i32,
    pub pass: i32,
    pub raw_line: String,
    pub line_idx: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RaidArray {
    pub name: String,
    pub path: String,
    pub level: String,
    pub state: String,
    pub size_bytes: u64,
    pub devices: Vec<String>,
    pub failed_devices: i32,
    pub spare_devices: i32,
    pub active_devices: i32,
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmartInfo {
    pub device: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub capacity_bytes: u64,
    pub temperature_c: Option<i32>,
    pub health: String,
    pub power_on_hours: Option<u64>,
    pub reallocated_sectors: Option<u64>,
    pub available: bool,
}

// ─── Helper: which ────────────────────────────────────────────────────────────

pub fn which_cmd(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ─── list_block_devices ───────────────────────────────────────────────────────

fn parse_lsblk_device(v: &serde_json::Value) -> BlockDevice {
    let str_field = |key: &str| -> Option<String> {
        v.get(key)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    };
    let bool_field = |key: &str| -> bool {
        match v.get(key) {
            Some(serde_json::Value::Bool(b)) => *b,
            Some(serde_json::Value::String(s)) => s == "1" || s == "true",
            Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0) == 1,
            _ => false,
        }
    };
    let size_bytes: u64 = v
        .get("size")
        .and_then(|x| x.as_u64())
        .or_else(|| v.get("size").and_then(|x| x.as_str()).and_then(|s| s.parse().ok()))
        .unwrap_or(0);

    let name = str_field("name").unwrap_or_default();
    let path = str_field("path").unwrap_or_else(|| format!("/dev/{name}"));

    let children = v
        .get("children")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().map(parse_lsblk_device).collect())
        .unwrap_or_default();

    BlockDevice {
        name,
        path,
        size_bytes,
        device_type: str_field("type").unwrap_or_else(|| "disk".into()),
        mountpoint: str_field("mountpoint"),
        fstype: str_field("fstype"),
        label: str_field("label"),
        uuid: str_field("uuid"),
        model: str_field("model"),
        serial: str_field("serial"),
        vendor: str_field("vendor"),
        removable: bool_field("rm"),
        read_only: bool_field("ro"),
        state: str_field("state"),
        children,
    }
}

pub async fn list_block_devices() -> Vec<BlockDevice> {
    let output = Command::new("lsblk")
        .args([
            "-J", "-b",
            "-o", "NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,LABEL,UUID,MODEL,SERIAL,VENDOR,RM,RO,STATE,PATH",
        ])
        .output()
        .await;

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    parsed
        .get("blockdevices")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().map(parse_lsblk_device).collect())
        .unwrap_or_default()
}

// ─── list_mounts ─────────────────────────────────────────────────────────────

fn is_pseudo_fs(fstype: &str, mountpoint: &str) -> bool {
    matches!(
        fstype,
        "proc" | "sysfs" | "devtmpfs" | "devpts" | "securityfs"
            | "pstore" | "efivarfs" | "bpf" | "autofs" | "mqueue"
            | "hugetlbfs" | "debugfs" | "tracefs" | "configfs"
            | "fusectl" | "fuse.portal" | "ramfs" | "overlay"
    ) || fstype.starts_with("cgroup")
        || fstype.starts_with("fuse.")
        || mountpoint.starts_with("/sys/")
        || mountpoint.starts_with("/proc/")
        || mountpoint.starts_with("/dev/")
        || mountpoint.starts_with("/run/")
        || (fstype == "tmpfs"
            && (mountpoint.starts_with("/sys/")
                || mountpoint.starts_with("/dev/")
                || mountpoint == "/dev"
                || mountpoint.starts_with("/run/")))
}

pub async fn list_mounts() -> Vec<MountInfo> {
    // Parse /proc/mounts
    let proc_mounts = tokio::fs::read_to_string("/proc/mounts").await.unwrap_or_default();
    let mut raw: Vec<(String, String, String, String)> = Vec::new(); // device, mp, fstype, opts

    for line in proc_mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let device = parts[0].to_string();
        let mountpoint = parts[1].to_string();
        let fstype = parts[2].to_string();
        let options = parts[3].to_string();

        if is_pseudo_fs(&fstype, &mountpoint) {
            continue;
        }
        // Skip loop devices without a real size
        if device.starts_with("/dev/loop") {
            continue;
        }
        raw.push((device, mountpoint, fstype, options));
    }

    // Run df -B1 to get usage numbers
    let df_out = Command::new("df")
        .args(["-B1", "--output=source,target,size,used,avail"])
        .output()
        .await
        .ok();

    // Build a lookup: mountpoint -> (size, used, avail)
    let mut df_map: std::collections::HashMap<String, (u64, u64, u64)> = std::collections::HashMap::new();
    if let Some(out) = df_out {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 5 {
                continue;
            }
            let mp = cols[1].to_string();
            let size: u64 = cols[2].parse().unwrap_or(0);
            let used: u64 = cols[3].parse().unwrap_or(0);
            let avail: u64 = cols[4].parse().unwrap_or(0);
            df_map.insert(mp, (size, used, avail));
        }
    }

    raw.into_iter()
        .map(|(device, mountpoint, fstype, options)| {
            let (size_bytes, used_bytes, avail_bytes) =
                df_map.get(&mountpoint).copied().unwrap_or((0, 0, 0));
            MountInfo {
                device,
                mountpoint,
                fstype,
                options,
                size_bytes,
                used_bytes,
                avail_bytes,
            }
        })
        .collect()
}

// ─── read_fstab ──────────────────────────────────────────────────────────────

pub async fn read_fstab() -> Vec<FstabEntry> {
    let content = tokio::fs::read_to_string("/etc/fstab")
        .await
        .unwrap_or_default();
    let mut entries = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let dump: i32 = parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
        let pass: i32 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);

        entries.push(FstabEntry {
            device: parts[0].to_string(),
            mountpoint: parts[1].to_string(),
            fstype: parts[2].to_string(),
            options: parts[3].to_string(),
            dump,
            pass,
            raw_line: line.to_string(),
            line_idx: idx,
        });
    }

    entries
}

// ─── list_raid ────────────────────────────────────────────────────────────────

pub async fn list_raid() -> Vec<RaidArray> {
    if !which_cmd("mdadm") {
        return Vec::new();
    }

    // mdadm --detail --scan gives lines like: ARRAY /dev/md0 metadata=... UUID=... name=...
    let scan_out = Command::new("mdadm")
        .args(["--detail", "--scan"])
        .output()
        .await
        .ok();

    let scan_out = match scan_out {
        Some(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let scan_text = String::from_utf8_lossy(&scan_out.stdout);
    let mut paths: Vec<String> = Vec::new();

    for line in scan_text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.first() == Some(&"ARRAY") {
            if let Some(path) = parts.get(1) {
                paths.push(path.to_string());
            }
        }
    }

    let mut arrays = Vec::new();

    for path in paths {
        let detail_out = Command::new("mdadm")
            .args(["--detail", &path])
            .output()
            .await
            .ok();

        let detail_out = match detail_out {
            Some(o) if o.status.success() => o,
            _ => continue,
        };

        let detail = String::from_utf8_lossy(&detail_out.stdout);
        let mut array = RaidArray {
            name: path
                .split('/')
                .next_back()
                .unwrap_or("md0")
                .to_string(),
            path: path.clone(),
            level: String::new(),
            state: String::new(),
            size_bytes: 0,
            devices: Vec::new(),
            failed_devices: 0,
            spare_devices: 0,
            active_devices: 0,
            uuid: None,
        };

        for line in detail.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("Raid Level :") {
                array.level = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("State :") {
                array.state = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("Array Size :") {
                // "1953382400 (1862.89 GiB)" — extract the numeric part
                if let Some(num_str) = val.split_whitespace().next() {
                    // Array Size is in kibibytes
                    array.size_bytes = num_str.parse::<u64>().unwrap_or(0) * 1024;
                }
            } else if let Some(val) = line.strip_prefix("Failed Devices :") {
                array.failed_devices = val.trim().parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("Spare Devices :") {
                array.spare_devices = val.trim().parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("Active Devices :") {
                array.active_devices = val.trim().parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("UUID :") {
                array.uuid = Some(val.trim().to_string());
            } else {
                // Device lines look like: "   0       8       1        0      active sync   /dev/sda1"
                // The last token of a line with /dev/ in it is a device path
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    if let Some(dev) = parts.last() {
                        if dev.starts_with("/dev/") {
                            array.devices.push(dev.to_string());
                        }
                    }
                }
            }
        }

        arrays.push(array);
    }

    arrays
}

// ─── smart_info ───────────────────────────────────────────────────────────────

pub fn sanitize_dev(dev: &str) -> Option<String> {
    // Strip any leading /dev/ prefix, then allow only alphanum + hyphen + underscore
    let base = dev.trim_start_matches('/');
    let base = base.trim_start_matches("dev/");
    if base.is_empty() {
        return None;
    }
    let clean: String = base
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

pub async fn smart_info(dev: &str) -> SmartInfo {
    let dev_clean = match sanitize_dev(dev) {
        Some(d) => d,
        None => {
            return SmartInfo {
                device: dev.to_string(),
                model: None,
                serial: None,
                capacity_bytes: 0,
                temperature_c: None,
                health: "unknown".into(),
                power_on_hours: None,
                reallocated_sectors: None,
                available: false,
            }
        }
    };

    if !which_cmd("smartctl") {
        return SmartInfo {
            device: dev_clean,
            model: None,
            serial: None,
            capacity_bytes: 0,
            temperature_c: None,
            health: "unavailable".into(),
            power_on_hours: None,
            reallocated_sectors: None,
            available: false,
        };
    }

    let dev_path = format!("/dev/{dev_clean}");

    // Try direct first, then sudo -n
    let output = {
        let direct = Command::new("smartctl")
            .args(["-a", &dev_path])
            .output()
            .await;
        match direct {
            Ok(o) if o.status.success() || !o.stdout.is_empty() => o,
            _ => {
                Command::new("sudo")
                    .args(["-n", "smartctl", "-a", &dev_path])
                    .output()
                    .await
                    .unwrap_or_else(|_| std::process::Output {
                        status: std::process::ExitStatus::default(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    })
            }
        }
    };

    let text = String::from_utf8_lossy(&output.stdout);

    let mut info = SmartInfo {
        device: dev_clean.clone(),
        model: None,
        serial: None,
        capacity_bytes: 0,
        temperature_c: None,
        health: "unknown".into(),
        power_on_hours: None,
        reallocated_sectors: None,
        available: !text.is_empty(),
    };

    for line in text.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("Device Model:") {
            info.model = Some(val.trim().to_string());
        } else if let Some(val) = line.strip_prefix("Model Number:") {
            // NVMe
            if info.model.is_none() {
                info.model = Some(val.trim().to_string());
            }
        } else if let Some(val) = line.strip_prefix("Serial Number:") {
            info.serial = Some(val.trim().to_string());
        } else if line.starts_with("User Capacity:") || line.starts_with("Total NVM Capacity:") {
            // "User Capacity:  500,107,862,016 bytes [500 GB]"
            let digits: String = line
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit() || *c == ',')
                .filter(|c| c.is_ascii_digit())
                .collect();
            info.capacity_bytes = digits.parse().unwrap_or(0);
        } else if line.contains("SMART overall-health") {
            // "SMART overall-health self-assessment test result: PASSED"
            info.health = if line.contains("PASSED") || line.contains("passed") {
                "healthy".into()
            } else {
                "failing".into()
            };
        } else if line.starts_with("SMART Health Status:") {
            // NVMe: "SMART Health Status: OK"
            if info.health == "unknown" {
                info.health = if line.contains("OK") { "healthy".into() } else { "failing".into() };
            }
        } else if line.starts_with("190 ") || line.starts_with("194 ") {
            // Temperature attribute — field 10 (0-indexed) is the raw value
            let cols: Vec<&str> = line.split_whitespace().collect();
            if let Some(raw) = cols.get(9) {
                info.temperature_c = raw.parse().ok();
            }
        } else if line.starts_with("Temperature:") {
            // NVMe: "Temperature:  38 Celsius"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(t) = parts.get(1) {
                info.temperature_c = t.parse().ok();
            }
        } else if line.starts_with("  9 ") || line.starts_with("9 ") {
            // Power_On_Hours attribute
            if line.contains("Power_On_Hours") {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if let Some(raw) = cols.get(9) {
                    info.power_on_hours = raw.parse().ok();
                }
            }
        } else if line.starts_with("Power On Hours:") {
            // NVMe
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(h) = parts.get(3) {
                info.power_on_hours = h.replace(',', "").parse().ok();
            }
        } else if (line.starts_with("  5 ") || line.starts_with("5 "))
            && line.contains("Reallocated_Sector") {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if let Some(raw) = cols.get(9) {
                    info.reallocated_sectors = raw.parse().ok();
                }
            }
    }

    info
}

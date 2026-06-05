use axum::{extract::State, Json};
use serde::Serialize;

use crate::AppState;

#[derive(Serialize, Clone)]
pub struct Capability {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub detected: bool,
    pub version: Option<String>,
    pub description: &'static str,
    pub required_dep: &'static str,
    pub how_to_enable: &'static str,
}

fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cmd_version(bin: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(bin)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.lines().next().map(|l| l.trim().to_string())
        })
}

fn path_exists(p: &str) -> bool {
    std::path::Path::new(p).exists()
}

fn read_file_contains(path: &str, needle: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|s| s.contains(needle))
        .unwrap_or(false)
}

fn detect_docker() -> Capability {
    let detected = path_exists("/var/run/docker.sock");
    let version = if detected {
        cmd_version("docker", &["version", "--format", "{{.Server.Version}}"])
    } else {
        None
    };
    Capability {
        id: "docker",
        name: "Docker",
        category: "Containers",
        detected,
        version,
        description: "Container runtime for managing Docker containers and images.",
        required_dep: "docker / docker-ce",
        how_to_enable: "Install Docker: curl -fsSL https://get.docker.com | sh",
    }
}

fn detect_docker_compose() -> Capability {
    let docker = ["/usr/bin/docker", "/usr/local/bin/docker", "docker"]
        .iter()
        .find(|p| std::path::Path::new(p).exists() || **p == "docker")
        .copied()
        .unwrap_or("docker");
    let v2 = std::process::Command::new(docker)
        .args(["compose", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let v1 = which("docker-compose");
    let detected = v2 || v1;
    let version = if v2 {
        cmd_version("docker", &["compose", "version", "--short"])
    } else if v1 {
        cmd_version("docker-compose", &["--version"])
    } else {
        None
    };
    Capability {
        id: "docker_compose",
        name: "Docker Compose",
        category: "Containers",
        detected,
        version,
        description: "Multi-container app orchestration. Required by App Vault.",
        required_dep: "docker-compose-plugin (v2) or docker-compose (v1)",
        how_to_enable: "Install Docker Compose plugin: apt install docker-compose-plugin",
    }
}

fn detect_lxc() -> Capability {
    let detected = which("lxc-ls") || which("lxc");
    let version = if detected {
        cmd_version("lxc-ls", &["--version"])
    } else {
        None
    };
    Capability {
        id: "lxc",
        name: "LXC Containers",
        category: "Containers",
        detected,
        version,
        description: "Linux Containers for lightweight OS-level virtualisation.",
        required_dep: "lxc",
        how_to_enable: "Install LXC: apt install lxc",
    }
}

fn detect_systemd() -> Capability {
    let detected = which("systemctl")
        && std::process::Command::new("systemctl")
            .arg("status")
            .output()
            .map(|o| o.status.code() != Some(1))
            .unwrap_or(false);
    let version = if detected {
        cmd_version("systemctl", &["--version"])
            .as_deref()
            .and_then(|s| s.split_whitespace().nth(1))
            .map(|v| v.to_string())
    } else {
        None
    };
    Capability {
        id: "systemd",
        name: "systemd",
        category: "Services",
        detected,
        version,
        description: "Service manager for starting, stopping, and monitoring Linux services.",
        required_dep: "systemd",
        how_to_enable: "Use a systemd-based Linux distribution (Debian, Ubuntu, Fedora, Arch…)",
    }
}

fn detect_kvm() -> Capability {
    let kvm_dev = path_exists("/dev/kvm");
    let vmx = read_file_contains("/proc/cpuinfo", "vmx");
    let svm = read_file_contains("/proc/cpuinfo", "svm");
    let detected = kvm_dev && (vmx || svm);
    let version = if detected {
        cmd_version("kvm", &["--version"]).or_else(|| Some("available".into()))
    } else {
        None
    };
    Capability {
        id: "kvm",
        name: "KVM",
        category: "Virtualisation",
        detected,
        version,
        description: "Kernel-based Virtual Machine. Required for running VMs directly.",
        required_dep: "kvm kernel module + Intel VT-x or AMD SVM CPU support",
        how_to_enable: "Enable VT-x/AMD-V in BIOS and load the kvm_intel or kvm_amd module",
    }
}

fn detect_libvirt() -> Capability {
    let detected = path_exists("/var/run/libvirt/libvirt-sock") || which("virsh");
    let version = if detected {
        cmd_version("virsh", &["--version"])
    } else {
        None
    };
    Capability {
        id: "libvirt",
        name: "libvirt / virsh",
        category: "Virtualisation",
        detected,
        version,
        description: "Virtualisation management layer. Required for VM control via VoidTower.",
        required_dep: "libvirt-daemon + libvirt-clients",
        how_to_enable: "apt install libvirt-daemon-system libvirt-clients && systemctl enable --now libvirtd",
    }
}

fn detect_zfs() -> Capability {
    let detected = which("zfs") && which("zpool");
    let version = if detected {
        cmd_version("zfs", &["version"])
    } else {
        None
    };
    Capability {
        id: "zfs",
        name: "ZFS",
        category: "Storage",
        detected,
        version,
        description: "Advanced filesystem with snapshots, RAID-Z, and deduplication.",
        required_dep: "zfsutils-linux / openzfs",
        how_to_enable: "apt install zfsutils-linux",
    }
}

fn detect_btrfs() -> Capability {
    let detected = which("btrfs")
        && std::fs::read_to_string("/proc/filesystems")
            .map(|s| s.contains("btrfs"))
            .unwrap_or(false);
    let version = if detected {
        cmd_version("btrfs", &["--version"])
    } else {
        None
    };
    Capability {
        id: "btrfs",
        name: "Btrfs",
        category: "Storage",
        detected,
        version,
        description: "Modern Linux filesystem with snapshots and subvolumes.",
        required_dep: "btrfs-progs",
        how_to_enable: "apt install btrfs-progs",
    }
}

fn detect_nfs() -> Capability {
    let detected = which("mount.nfs")
        || which("nfs-common")
        || std::fs::read_to_string("/proc/filesystems")
            .map(|s| s.contains("nfs"))
            .unwrap_or(false);
    Capability {
        id: "nfs",
        name: "NFS",
        category: "Storage",
        detected,
        version: None,
        description: "Network File System for mounting remote directories.",
        required_dep: "nfs-common (client) or nfs-kernel-server (server)",
        how_to_enable: "apt install nfs-common",
    }
}

fn detect_smb() -> Capability {
    let detected = which("smbclient") || which("mount.cifs");
    let version = if detected {
        cmd_version("smbclient", &["--version"])
    } else {
        None
    };
    Capability {
        id: "smb",
        name: "SMB / CIFS",
        category: "Storage",
        detected,
        version,
        description: "Windows-compatible file sharing for mounting Samba/NAS shares.",
        required_dep: "cifs-utils + smbclient",
        how_to_enable: "apt install cifs-utils smbclient",
    }
}

fn detect_restic() -> Capability {
    let detected = which("restic");
    let version = if detected {
        cmd_version("restic", &["version"])
    } else {
        None
    };
    Capability {
        id: "restic",
        name: "Restic",
        category: "Backups",
        detected,
        version,
        description: "Encrypted, deduplicated backup tool. Required by VoidTower backups.",
        required_dep: "restic",
        how_to_enable: "apt install restic   or   curl -fsSL https://raw.githubusercontent.com/restic/restic/master/doc/installation.rst",
    }
}

fn detect_nginx() -> Capability {
    let detected = which("nginx")
        || path_exists("/usr/sbin/nginx")
        || path_exists("/usr/bin/nginx")
        || path_exists("/usr/local/sbin/nginx");
    let version = if detected {
        cmd_version("nginx", &["-v"])
            .or_else(|| {
                std::process::Command::new("nginx")
                    .arg("-v")
                    .stderr(std::process::Stdio::piped())
                    .output()
                    .ok()
                    .and_then(|o| {
                        String::from_utf8(o.stderr).ok()
                            .map(|s| s.trim().to_string())
                    })
            })
    } else {
        None
    };
    Capability {
        id: "nginx",
        name: "Nginx",
        category: "Networking",
        detected,
        version,
        description: "Reverse proxy server. Required for the VoidTower Proxy Manager.",
        required_dep: "nginx",
        how_to_enable: "apt install nginx && systemctl enable --now nginx",
    }
}

fn detect_avahi() -> Capability {
    let detected = which("avahi-daemon")
        || path_exists("/var/run/avahi-daemon/pid")
        || path_exists("/run/avahi-daemon/pid");
    Capability {
        id: "avahi",
        name: "Avahi / mDNS",
        category: "Networking",
        detected,
        version: None,
        description: "mDNS/DNS-SD for zero-config local hostname resolution (e.g. hostname.local).",
        required_dep: "avahi-daemon + libnss-mdns",
        how_to_enable: "apt install avahi-daemon libnss-mdns && systemctl enable --now avahi-daemon",
    }
}

fn detect_nvidia() -> Capability {
    let detected = which("nvidia-smi")
        && std::process::Command::new("nvidia-smi")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    let version = if detected {
        cmd_version("nvidia-smi", &["--query-gpu=driver_version", "--format=csv,noheader"])
            .map(|v| format!("driver {v}"))
    } else {
        None
    };
    Capability {
        id: "nvidia_gpu",
        name: "NVIDIA GPU",
        category: "GPU",
        detected,
        version,
        description: "NVIDIA GPU with drivers installed. Enables GPU workloads in Docker/Ollama/Odysseus.",
        required_dep: "nvidia-driver + nvidia-container-toolkit",
        how_to_enable: "Install NVIDIA driver: ubuntu-drivers install  or  nvidia-driver packages",
    }
}

fn detect_rocm() -> Capability {
    let detected = which("rocm-smi") || path_exists("/opt/rocm");
    let version = if detected {
        cmd_version("rocm-smi", &["--version"])
    } else {
        None
    };
    Capability {
        id: "amd_gpu",
        name: "AMD GPU / ROCm",
        category: "GPU",
        detected,
        version,
        description: "AMD GPU with ROCm drivers. Enables GPU compute for AI/ML workloads.",
        required_dep: "rocm-opencl-runtime or amdgpu-pro",
        how_to_enable: "Install ROCm: see https://rocm.docs.amd.com/en/latest/deploy/linux/",
    }
}

fn detect_apt() -> Capability {
    let detected = which("apt") || which("apt-get");
    Capability {
        id: "apt",
        name: "APT",
        category: "Package Manager",
        detected,
        version: cmd_version("apt", &["--version"]),
        description: "Debian/Ubuntu package manager.",
        required_dep: "apt",
        how_to_enable: "Use a Debian/Ubuntu-based distribution.",
    }
}

fn detect_dnf() -> Capability {
    let detected = which("dnf") || which("yum");
    let version = if detected {
        cmd_version("dnf", &["--version"]).or_else(|| cmd_version("yum", &["--version"]))
    } else {
        None
    };
    Capability {
        id: "dnf",
        name: "DNF / YUM",
        category: "Package Manager",
        detected,
        version,
        description: "Fedora/RHEL/CentOS package manager.",
        required_dep: "dnf or yum",
        how_to_enable: "Use a Fedora/RHEL-based distribution.",
    }
}

fn detect_pacman() -> Capability {
    let detected = which("pacman");
    let version = if detected {
        cmd_version("pacman", &["--version"])
            .as_deref()
            .and_then(|s| s.lines().next().map(|l| l.to_string()))
    } else {
        None
    };
    Capability {
        id: "pacman",
        name: "Pacman",
        category: "Package Manager",
        detected,
        version,
        description: "Arch Linux package manager.",
        required_dep: "pacman",
        how_to_enable: "Use an Arch-based distribution.",
    }
}

fn detect_wireguard() -> Capability {
    let detected = std::process::Command::new("which").arg("wg").output()
        .map(|o| o.status.success()).unwrap_or(false);
    let version = if detected {
        std::process::Command::new("wg").arg("--version").output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(|l| l.to_string()).unwrap_or_default())
            .filter(|s| !s.is_empty())
    } else { None };
    Capability {
        id: "wireguard",
        name: "WireGuard",
        category: "Networking",
        detected,
        version,
        description: "WireGuard VPN kernel module and tools.",
        required_dep: "wireguard-tools",
        how_to_enable: "Install wireguard-tools (apt/pacman/dnf) and ensure the kernel module is loaded.",
    }
}

fn detect_ufw() -> Capability {
    let detected = std::process::Command::new("which").arg("ufw").output()
        .map(|o| o.status.success()).unwrap_or(false);
    Capability {
        id: "ufw",
        name: "UFW Firewall",
        category: "Networking",
        detected,
        version: None,
        description: "Uncomplicated Firewall (UFW) for managing iptables rules.",
        required_dep: "ufw",
        how_to_enable: "Install ufw (apt install ufw) and enable it with: sudo ufw enable",
    }
}

pub async fn get_capabilities(_state: State<AppState>) -> Json<serde_json::Value> {
    let capabilities: Vec<Capability> = vec![
        detect_docker(),
        detect_docker_compose(),
        detect_lxc(),
        detect_systemd(),
        detect_kvm(),
        detect_libvirt(),
        detect_zfs(),
        detect_btrfs(),
        detect_nfs(),
        detect_smb(),
        detect_restic(),
        detect_nginx(),
        detect_avahi(),
        detect_nvidia(),
        detect_rocm(),
        detect_apt(),
        detect_dnf(),
        detect_pacman(),
        detect_wireguard(),
        detect_ufw(),
    ];

    let total = capabilities.len();
    let detected_count = capabilities.iter().filter(|c| c.detected).count();

    Json(serde_json::json!({
        "capabilities": capabilities,
        "summary": {
            "total": total,
            "detected": detected_count,
            "missing": total - detected_count,
        }
    }))
}

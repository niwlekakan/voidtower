import { useEffect, useState } from 'react'
import { CheckCircle, XCircle, RefreshCw, ChevronDown, ChevronRight } from 'lucide-react'
import { api } from '@/api/client'
import type { Capability } from '@/api/types'

interface CapabilityMeta {
  unlocks: string[]
  install: { apt?: string; pacman?: string; dnf?: string }
}

const CAPABILITY_META: Record<string, CapabilityMeta> = {
  docker: {
    unlocks: ['Containers page', 'App Vault', 'Container log viewer', 'GPU workloads (with NVIDIA toolkit)'],
    install: {
      apt:    'curl -fsSL https://get.docker.com | sh',
      pacman: 'sudo pacman -S docker && sudo systemctl enable --now docker',
      dnf:    'sudo dnf install docker-ce && sudo systemctl enable --now docker',
    },
  },
  docker_compose: {
    unlocks: ['App Vault deployments', 'Multi-container app management'],
    install: {
      apt:    'sudo apt install docker-compose-plugin',
      pacman: 'sudo pacman -S docker-compose',
      dnf:    'sudo dnf install docker-compose-plugin',
    },
  },
  lxc: {
    unlocks: ['LXC container management'],
    install: {
      apt:    'sudo apt install lxc',
      pacman: 'sudo pacman -S lxc',
      dnf:    'sudo dnf install lxc',
    },
  },
  systemd: {
    unlocks: ['Services page', 'Service start/stop/restart', 'Service enable/disable'],
    install: { apt: 'Use a systemd-based distribution (Debian, Ubuntu, Fedora, Arch…)' },
  },
  kvm: {
    unlocks: ['VMs page (local KVM)', 'VM creation and lifecycle management'],
    install: {
      apt:    'Enable VT-x/AMD-V in BIOS, then: sudo modprobe kvm_intel  # or kvm_amd',
      pacman: 'Enable VT-x/AMD-V in BIOS, then: sudo modprobe kvm_intel  # or kvm_amd',
      dnf:    'Enable VT-x/AMD-V in BIOS, then: sudo modprobe kvm_intel  # or kvm_amd',
    },
  },
  libvirt: {
    unlocks: ['VMs page (local KVM)', 'VM control via virsh/libvirt'],
    install: {
      apt:    'sudo apt install libvirt-daemon-system libvirt-clients && sudo systemctl enable --now libvirtd',
      pacman: 'sudo pacman -S libvirt && sudo systemctl enable --now libvirtd',
      dnf:    'sudo dnf install libvirt libvirt-client && sudo systemctl enable --now libvirtd',
    },
  },
  zfs: {
    unlocks: ['ZFS pool/dataset management', 'Snapshot browser'],
    install: {
      apt:    'sudo apt install zfsutils-linux',
      pacman: 'sudo pacman -S zfs-utils  # requires AUR or archzfs repo',
      dnf:    'sudo dnf install zfs  # requires ZFS on Linux repo',
    },
  },
  btrfs: {
    unlocks: ['Btrfs subvolume and snapshot management'],
    install: {
      apt:    'sudo apt install btrfs-progs',
      pacman: 'sudo pacman -S btrfs-progs',
      dnf:    'sudo dnf install btrfs-progs',
    },
  },
  nfs: {
    unlocks: ['NFS remote mount management'],
    install: {
      apt:    'sudo apt install nfs-common',
      pacman: 'sudo pacman -S nfs-utils',
      dnf:    'sudo dnf install nfs-utils',
    },
  },
  smb: {
    unlocks: ['SMB/CIFS network share mounting'],
    install: {
      apt:    'sudo apt install cifs-utils smbclient',
      pacman: 'sudo pacman -S cifs-utils smbclient',
      dnf:    'sudo dnf install cifs-utils samba-client',
    },
  },
  restic: {
    unlocks: ['Backups page', 'Encrypted backup creation and restore'],
    install: {
      apt:    'sudo apt install restic',
      pacman: 'sudo pacman -S restic',
      dnf:    'sudo dnf install restic',
    },
  },
  nginx: {
    unlocks: ['Proxy Manager page', 'Reverse proxy configuration', 'SSL termination'],
    install: {
      apt:    'sudo apt install nginx && sudo systemctl enable --now nginx',
      pacman: 'sudo pacman -S nginx && sudo systemctl enable --now nginx',
      dnf:    'sudo dnf install nginx && sudo systemctl enable --now nginx',
    },
  },
  avahi: {
    unlocks: ['Local hostname resolution (hostname.local)', 'Zero-config service discovery'],
    install: {
      apt:    'sudo apt install avahi-daemon libnss-mdns && sudo systemctl enable --now avahi-daemon',
      pacman: 'sudo pacman -S avahi nss-mdns && sudo systemctl enable --now avahi-daemon',
      dnf:    'sudo dnf install avahi nss-mdns && sudo systemctl enable --now avahi',
    },
  },
  nvidia_gpu: {
    unlocks: ['GPU metrics on Dashboard', 'NVIDIA GPU workloads in Docker', 'Ollama GPU acceleration', 'Odysseus AI acceleration'],
    install: {
      apt:    'sudo ubuntu-drivers install  # Ubuntu\nsudo apt install nvidia-driver-535  # Debian',
      pacman: 'sudo pacman -S nvidia nvidia-container-toolkit',
      dnf:    'sudo dnf install akmod-nvidia xorg-x11-drv-nvidia',
    },
  },
  amd_gpu: {
    unlocks: ['AMD GPU metrics', 'ROCm compute workloads', 'Ollama AMD GPU acceleration'],
    install: {
      apt:    'See https://rocm.docs.amd.com/en/latest/deploy/linux/',
      pacman: 'sudo pacman -S rocm-opencl-runtime rocm-hip-sdk  # or from AUR',
      dnf:    'See https://rocm.docs.amd.com/en/latest/deploy/linux/',
    },
  },
  wireguard: {
    unlocks: ['WireGuard VPN page', 'VPN peer management', 'Tunnel monitoring'],
    install: {
      apt:    'sudo apt install wireguard-tools',
      pacman: 'sudo pacman -S wireguard-tools',
      dnf:    'sudo dnf install wireguard-tools',
    },
  },
  ufw: {
    unlocks: ['Firewall page', 'Rule management via UFW'],
    install: {
      apt:    'sudo apt install ufw && sudo ufw enable',
      pacman: 'sudo pacman -S ufw && sudo systemctl enable --now ufw && sudo ufw enable',
      dnf:    'sudo dnf install ufw && sudo systemctl enable --now ufw && sudo ufw enable',
    },
  },
  apt:    { unlocks: ['Package install hints for Debian/Ubuntu'], install: {} },
  dnf:    { unlocks: ['Package install hints for Fedora/RHEL'],   install: {} },
  pacman: { unlocks: ['Package install hints for Arch Linux'],    install: {} },
}

const CATEGORY_ORDER = [
  'Containers',
  'Services',
  'Virtualisation',
  'Storage',
  'Backups',
  'Networking',
  'GPU',
  'Package Manager',
]

function CapabilityCard({ cap }: { cap: Capability }) {
  const [expanded, setExpanded] = useState(false)
  const meta = CAPABILITY_META[cap.id]

  const distroCommands = meta
    ? ([
        meta.install.apt    ? { label: 'apt',    cmd: meta.install.apt    } : null,
        meta.install.pacman ? { label: 'pacman', cmd: meta.install.pacman } : null,
        meta.install.dnf    ? { label: 'dnf',    cmd: meta.install.dnf    } : null,
      ] as ({ label: string; cmd: string } | null)[]).filter(Boolean) as { label: string; cmd: string }[]
    : []

  return (
    <div
      className="rounded border p-3 text-sm"
      style={{
        background: 'var(--bg-panel)',
        borderColor: cap.detected ? 'var(--border-subtle)' : 'color-mix(in srgb, var(--accent-error) 30%, transparent)',
      }}
    >
      <div className="flex items-start gap-3">
        {cap.detected
          ? <CheckCircle size={16} style={{ color: 'var(--accent-success)', flexShrink: 0, marginTop: 1 }} />
          : <XCircle    size={16} style={{ color: 'var(--accent-error)',   flexShrink: 0, marginTop: 1 }} />
        }

        <div className="flex-1 min-w-0">
          <div className="flex items-baseline gap-2 flex-wrap">
            <span className="font-medium" style={{ color: 'var(--text-primary)' }}>
              {cap.name}
            </span>
            {cap.version && (
              <span className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>
                {cap.version}
              </span>
            )}
            {!cap.detected && (
              <span
                className="text-xs px-1.5 py-0.5 rounded"
                style={{ background: 'color-mix(in srgb, var(--accent-error) 15%, transparent)', color: 'var(--accent-error)' }}
              >
                not found
              </span>
            )}
          </div>

          <p className="mt-0.5 text-xs leading-relaxed" style={{ color: 'var(--text-secondary)' }}>
            {cap.description}
          </p>

          {/* Unlocks — shown for detected caps when metadata is present */}
          {cap.detected && meta && meta.unlocks.length > 0 && (
            <div className="mt-1.5 flex flex-wrap gap-1">
              {meta.unlocks.map(feature => (
                <span
                  key={feature}
                  className="text-xs px-1.5 py-0.5 rounded"
                  style={{
                    background: 'color-mix(in srgb, var(--accent-success) 12%, transparent)',
                    color: 'var(--accent-success)',
                    border: '1px solid color-mix(in srgb, var(--accent-success) 25%, transparent)',
                  }}
                >
                  {feature}
                </span>
              ))}
            </div>
          )}

          {/* How to enable — shown for missing caps */}
          {!cap.detected && (
            <div className="mt-2">
              <button
                onClick={() => setExpanded(e => !e)}
                className="flex items-center gap-1 text-xs hover:opacity-80 transition-opacity"
                style={{ color: 'var(--accent-primary)' }}
              >
                {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                How to enable
              </button>

              {expanded && (
                <div
                  className="mt-1.5 rounded p-2.5 text-xs space-y-2"
                  style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)' }}
                >
                  <div>
                    <span style={{ color: 'var(--text-muted)' }}>Requires: </span>
                    <code style={{ color: 'var(--accent-secondary)' }}>{cap.required_dep}</code>
                  </div>

                  {distroCommands.length > 0 ? (
                    <div className="space-y-1.5">
                      {distroCommands.map(({ label, cmd }) => (
                        <div key={label}>
                          <div
                            className="text-xs font-semibold mb-0.5 uppercase tracking-wide"
                            style={{ color: 'var(--text-muted)' }}
                          >
                            {label}
                          </div>
                          <code
                            className="block whitespace-pre-wrap break-all leading-relaxed"
                            style={{ color: 'var(--text-secondary)' }}
                          >
                            {cmd}
                          </code>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div>
                      <span style={{ color: 'var(--text-muted)' }}>Install: </span>
                      <code className="break-all" style={{ color: 'var(--text-secondary)' }}>{cap.how_to_enable}</code>
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

function CategorySection({ name, caps }: { name: string; caps: Capability[] }) {
  const detected = caps.filter(c => c.detected).length
  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>
          {name}
        </h2>
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
          {detected}/{caps.length}
        </span>
      </div>
      <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
        {caps.map(cap => (
          <CapabilityCard key={cap.id} cap={cap} />
        ))}
      </div>
    </div>
  )
}

export default function CapabilitiesPage() {
  const [data, setData] = useState<{ capabilities: Capability[]; summary: { total: number; detected: number; missing: number } } | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = () => {
    setLoading(true)
    setError(null)
    api.capabilities.list()
      .then(setData)
      .catch(e => setError(e.message ?? 'Failed to load capabilities'))
      .finally(() => setLoading(false))
  }

  useEffect(() => { load() }, [])

  const grouped = data
    ? CATEGORY_ORDER.reduce<Record<string, Capability[]>>((acc, cat) => {
        const caps = data.capabilities.filter(c => c.category === cat)
        if (caps.length) acc[cat] = caps
        return acc
      }, {})
    : {}

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Capabilities</h1>
          {data && (
            <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
              {data.summary.detected} of {data.summary.total} detected
              {data.summary.missing > 0 && ` · ${data.summary.missing} missing`}
            </p>
          )}
        </div>
        <button
          onClick={load}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-sm transition-opacity hover:opacity-80 disabled:opacity-50"
          style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
        >
          <RefreshCw size={13} className={loading ? 'animate-spin' : ''} />
          Refresh
        </button>
      </div>

      {data && (
        <div className="flex gap-3 flex-wrap">
          {[
            { label: 'Detected',    value: data.summary.detected, color: 'var(--accent-success)' },
            { label: 'Missing',     value: data.summary.missing,  color: data.summary.missing > 0 ? 'var(--accent-warning)' : 'var(--text-muted)' },
            { label: 'Total',       value: data.summary.total,    color: 'var(--text-secondary)' },
          ].map(({ label, value, color }) => (
            <div
              key={label}
              className="flex flex-col items-center justify-center rounded px-4 py-2 min-w-[80px]"
              style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}
            >
              <span className="text-xl font-bold tabular-nums" style={{ color }}>{value}</span>
              <span className="text-xs uppercase tracking-wide" style={{ color: 'var(--text-muted)' }}>{label}</span>
            </div>
          ))}
        </div>
      )}

      {loading && !data && (
        <div className="flex items-center justify-center h-40 text-sm" style={{ color: 'var(--text-muted)' }}>
          Scanning system…
        </div>
      )}

      {error && (
        <div className="rounded p-3 text-sm" style={{ background: 'color-mix(in srgb, var(--accent-error) 10%, transparent)', color: 'var(--accent-error)', border: '1px solid color-mix(in srgb, var(--accent-error) 30%, transparent)' }}>
          {error}
        </div>
      )}

      {Object.entries(grouped).map(([cat, caps]) => (
        <CategorySection key={cat} name={cat} caps={caps} />
      ))}
    </div>
  )
}

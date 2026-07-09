// Ported from frontend/src/api/types.ts (subset actually used by the mobile
// app's phase-one screens), plus the "guest"/"demo" roles added in Part A —
// the web types.ts hasn't been updated for those yet.

export type Role = 'owner' | 'admin' | 'operator' | 'viewer' | 'guest' | 'demo'

export interface User {
  id: string
  username: string
  role: Role
  force_password_change: boolean
  totp_enabled: boolean
  expires_at: number | null
}

export interface ApiError {
  error: { code: string; message: string }
}

export interface DiskInfo { name: string; mount_point: string; total: number; used: number; available: number; fs_type: string }
export interface NetworkInfo { name: string; rx_bytes: number; tx_bytes: number; rx_bytes_per_sec: number; tx_bytes_per_sec: number }
export interface GpuInfo {
  name: string
  temp_c: number
  util_pct: number
  mem_util_pct: number
  mem_used_mb: number
  mem_total_mb: number
  power_w: number
  power_limit_w: number
}

export interface MetricsSnapshot {
  hostname: string
  uptime_secs: number
  cpu_usage: number
  cpu_count: number
  cpu_model: string
  ram_total: number
  ram_used: number
  swap_total: number
  swap_used: number
  load_avg: [number, number, number]
  process_count: number
  os_name: string
  kernel_version: string
  disks: DiskInfo[]
  networks: NetworkInfo[]
  gpu: GpuInfo[]
  timestamp: number
}

export interface ServiceInfo {
  name: string
  description: string
  load_state: string
  active_state: string
  sub_state: string
  enabled: boolean
}
export interface ServicesResponse { services: ServiceInfo[]; systemd_available: boolean }

export interface ContainerInfo {
  id: string
  short_id: string
  name: string
  image: string
  status: string
  state: string
  created: number
}
export interface ContainersResponse { containers: ContainerInfo[]; docker_available: boolean }

export interface DeployedApp {
  project_name: string
  app_name?: string
  status?: string
}
export interface DeployedResponse { apps: DeployedApp[]; docker_available: boolean }

export interface Alert {
  id: string
  title: string
  message: string
  severity: 'info' | 'warning' | 'critical'
  category: string
  state: 'active' | 'acknowledged' | 'resolved'
  created_at: number
}
export interface AlertsResponse { alerts: Alert[]; total: number }

export interface WgPeer { id: string; name: string; public_key: string; allocated_ip: string }
export interface WgInterface { name: string; address: string; listen_port: number }
export interface WireguardResponse {
  available: boolean
  error: string | null
  interfaces: WgInterface[]
  peers: WgPeer[]
}

// Part B — node enrollment. Shape reconciled against the actual
// backend/src/api/node_enroll.rs (NodeRow/EnrollRequest/EnrollResponse).
export interface NodeRecord {
  id: string
  display_name: string
  device_type: string
  owner_user_id: string
  last_seen: number | null
  last_telemetry: string | null
  agent_capable: boolean
  approved: boolean
  created_at: number
}
export interface PairingCodeResponse { code: string; expires_at: number }
export interface EnrollResponse {
  node_id: string
  /** Scoped to POST /api/nodes/:id/heartbeat only — not a general API token. */
  heartbeat_token: string
  wg_client_config: string
  warnings: string[]
}

export interface User {
  id: string
  username: string
  role: 'owner' | 'admin' | 'operator' | 'viewer'
  force_password_change: boolean
}

export interface UserRecord {
  id: string
  username: string
  role: 'owner' | 'admin' | 'operator' | 'viewer'
  force_password_change: boolean
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
  top_cpu_procs: ProcessInfo[]
  top_mem_procs: ProcessInfo[]
  timestamp: number
}

export interface DiskInfo {
  name: string
  mount_point: string
  total: number
  used: number
  available: number
  fs_type: string
}

export interface NetworkInfo {
  name: string
  rx_bytes: number
  tx_bytes: number
  rx_bytes_per_sec: number
  tx_bytes_per_sec: number
}

export interface ProcessInfo {
  pid: number
  name: string
  cpu_usage: number
  memory_bytes: number
}

export interface ServiceInfo {
  name: string
  description: string
  load_state: string
  active_state: string
  sub_state: string
  enabled: boolean
}

export interface ServicesResponse {
  services: ServiceInfo[]
  systemd_available: boolean
}

export interface AuditEntry {
  id: string
  timestamp: number
  user_id: string | null
  actor_type: string
  action: string
  resource_type: string | null
  resource_id: string | null
  outcome: string
  ip_address: string | null
  details: string | null
}

export interface ApiError {
  error: { code: string; message: string }
}

export type ServiceAction = 'start' | 'stop' | 'restart' | 'enable' | 'disable'

// Containers
export interface ContainerInfo {
  id: string
  short_id: string
  name: string
  image: string
  status: string
  state: string
  created: number
  ports: PortMapping[]
}

export interface PortMapping {
  host_port: number | null
  container_port: number
  protocol: string
}

export interface ImageInfo {
  id: string
  tags: string[]
  size: number
  created: number
}

export interface ContainersResponse {
  containers: ContainerInfo[]
  docker_available: boolean
}

export type ContainerAction = 'start' | 'stop' | 'restart' | 'remove'

// App Vault
export interface AppDef {
  id: string
  name: string
  description: string
  category: string
  icon: string
  version_hint: string
  links: Record<string, string>
}

export interface DeployedApp {
  id: string
  app_id: string
  app_name: string
  project_name: string
  status: string
  deployed_at: number
  compose_path: string
  primary_port: number | null
}

export interface DeployedResponse {
  apps: DeployedApp[]
  docker_available: boolean
}

export interface ComposeContainer {
  name: string
  service: string
  image: string
  state: string
  status: string
  ports: string[]
}

// Alerts
export interface Alert {
  id: string
  title: string
  message: string
  severity: 'info' | 'warning' | 'critical'
  category: string
  node_id: string | null
  resource_type: string | null
  resource_id: string | null
  state: 'active' | 'acknowledged' | 'resolved'
  acknowledged_by: string | null
  acknowledged_at: number | null
  resolved_at: number | null
  created_at: number
  updated_at: number
}

export interface AlertsResponse {
  alerts: Alert[]
  total: number
}

// Proxy
export interface ProxyConfig {
  id: string
  domain: string
  upstream: string
  ssl: boolean
  enabled: boolean
  allow_embed: boolean
  created_at: number
}

// Files
export interface FileEntry {
  name: string
  path: string
  is_dir: boolean
  size: number
  modified: number
  permissions: string
  is_symlink: boolean
}

export interface FilesListResponse {
  path: string
  entries: FileEntry[]
  parent: string | null
}

export interface FsRoot {
  label: string
  path: string
}

// Security
export interface SessionInfo {
  id: string
  user_id: string
  expires_at: number
  created_at: number
  ip_address: string | null
  user_agent: string | null
}

export interface SessionsResponse {
  sessions: SessionInfo[]
  current_session_id: string
}

// Capabilities
export interface Capability {
  id: string
  name: string
  category: string
  detected: boolean
  version: string | null
  description: string
  required_dep: string
  how_to_enable: string
}

// Timeline
export type TimelineCategory = 'auth' | 'containers' | 'services' | 'backups' | 'secrets' | 'apps' | 'networking' | 'alerts' | 'files' | 'system'

export interface TimelineEvent {
  id: string
  timestamp: number
  category: TimelineCategory
  action: string
  actor: string
  actor_type: string
  resource_type: string | null
  resource_id: string | null
  outcome: string
  details: string | null
  ip_address: string | null
}

export interface TimelineResponse {
  events: TimelineEvent[]
  total: number
  limit: number
  offset: number
}

// Secrets
export interface SecretMeta {
  id: string
  name: string
  description: string | null
  created_at: number
  updated_at: number
  last_used_at: number | null
}

export interface SecretsResponse { secrets: SecretMeta[] }

export interface CapabilitiesResponse {
  capabilities: Capability[]
  summary: {
    total: number
    detected: number
    missing: number
  }
}

// Diagnostics
export type DiagStatus = 'pass' | 'warn' | 'fail' | 'info'

export interface DiagCheck {
  id: string
  name: string
  category: string
  status: DiagStatus
  message: string
  detail: string | null
}

// WireGuard
export interface WgInterface {
  name: string
  public_key: string
  listen_port: number
}

export interface WgPeer {
  id: string
  interface: string
  name: string
  public_key: string
  allocated_ip: string
  endpoint: string | null
  latest_handshake: number | null
  rx_bytes: number
  tx_bytes: number
  connected: boolean
  created_at: number
}

export interface WireguardResponse {
  available: boolean
  error: string | null
  interfaces: WgInterface[]
  peers: WgPeer[]
}

export interface SshSession {
  id: string
  label: string
  host: string
  port: number
  username: string
  key_path?: string
  created_at: number
  last_used?: number
}

export interface ModelFile {
  filename: string
  size_bytes: number
  modified: number
  active: boolean
  source: 'voidtower' | 'ollama'
}

export interface DownloadStatus {
  id: string
  filename: string
  total_bytes: number | null
  downloaded_bytes: number
  status: 'downloading' | 'done' | 'error'
  error: string | null
}

export interface OllamaPullStatus {
  id: string
  model: string
  status: 'pulling' | 'done' | 'error'
  current_layer: string | null
  total_bytes: number | null
  pulled_bytes: number | null
  error: string | null
}

export interface Tag {
  id: string
  name: string
  color: string
  created_at: number
}

export type TagMap = Record<string, Tag[]>

export interface LocalVm {
  name: string
  id: number | null
  state: string
}

export interface LocalVmsResponse {
  vms: LocalVm[]
  libvirt_available: boolean
}

export interface ProxmoxVm {
  vmid: number
  name: string
  kind: 'qemu' | 'lxc'
  node: string
  status: string
  mem: number
  maxmem: number
  cpu: number
  uptime: number
  cpus: number
}

export interface ProxmoxVmsResponse {
  vms: ProxmoxVm[]
  nodes: string[]
}

export interface ProxmoxConfig {
  host: string
  port: number
  token: string
  node: string
  verify_ssl: boolean
}

export interface DiagnosticsResponse {
  checks: DiagCheck[]
  summary: {
    pass: number
    warn: number
    fail: number
    info: number
    overall: 'pass' | 'warn' | 'fail'
  }
}

// Storage
export interface BlockDevice {
  name: string
  path: string
  size_bytes: number
  device_type: string
  mountpoint: string | null
  fstype: string | null
  label: string | null
  uuid: string | null
  model: string | null
  serial: string | null
  vendor: string | null
  removable: boolean
  read_only: boolean
  state: string | null
  children: BlockDevice[]
}

export interface MountInfo {
  device: string
  mountpoint: string
  fstype: string
  options: string
  size_bytes: number
  used_bytes: number
  avail_bytes: number
}

export interface FstabEntry {
  device: string
  mountpoint: string
  fstype: string
  options: string
  dump: number
  pass: number
  raw_line: string
  line_idx: number
}

export interface RaidArray {
  name: string
  path: string
  level: string
  state: string
  size_bytes: number
  devices: string[]
  failed_devices: number
  spare_devices: number
  active_devices: number
  uuid: string | null
}

export interface SmartInfo {
  device: string
  model: string | null
  serial: string | null
  capacity_bytes: number
  temperature_c: number | null
  health: string
  power_on_hours: number | null
  reallocated_sectors: number | null
  available: boolean
}

export interface StoragePaths {
  containers: string | null
  appvault: string | null
  vms: string | null
  backups: string | null
}

export interface ApiToken {
  id: string
  name: string
  scopes: string[]
  last_used_at: number | null
  expires_at: number | null
  created_at: number
}

export interface OdysseusConfig {
  enabled: boolean
  mcp_enabled: boolean
  allowed_url: string
  webhook_secret_hint: string
  emergency_disabled: boolean
}

export interface OdysseusManifest {
  voidtower_version: string
  integration_enabled: boolean
  tools: {
    name: string
    description: string
    required_scope: string
    risk: string
    destructive: boolean
    requires_confirmation?: boolean
  }[]
}

export interface AuditAction {
  id: string
  timestamp: number
  action: string
  resource_type: string | null
  resource_id: string | null
  outcome: string
  ip_address: string | null
  details: string | null
}

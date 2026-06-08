/// <reference types="vite/client" />
import type { ApiError } from './types'

const BASE = import.meta.env.VITE_API_BASE ?? ''

export class ApiClientError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly status: number,
  ) {
    super(message)
    this.name = 'ApiClientError'
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...init?.headers,
    },
  })

  if (!res.ok) {
    let body: ApiError | null = null
    try { body = await res.json() } catch { /* ignore */ }
    throw new ApiClientError(
      body?.error?.message ?? res.statusText,
      body?.error?.code ?? 'unknown',
      res.status,
    )
  }

  if (res.status === 204) return undefined as T
  return res.json() as Promise<T>
}

export const api = {
  auth: {
    login: (username: string, password: string, totp_code?: string) =>
      request<{ user: import('./types').User }>('/api/auth/login', {
        method: 'POST',
        body: JSON.stringify({ username, password, totp_code }),
      }),
    logout:    () => request('/api/auth/logout', { method: 'POST' }),
    me:        () => request<{ user: import('./types').User }>('/api/auth/me'),
    bootstrap: (token: string, username: string, password: string) =>
      request<{ user: import('./types').User }>('/api/auth/bootstrap', {
        method: 'POST',
        body: JSON.stringify({ token, username, password }),
      }),
  },

  totp: {
    setup:   () =>
      request<{ secret: string; uri: string }>('/api/auth/totp/setup', { method: 'POST' }),
    enable:  (code: string) =>
      request('/api/auth/totp/enable',  { method: 'POST', body: JSON.stringify({ code }) }),
    disable: (code: string) =>
      request('/api/auth/totp/disable', { method: 'POST', body: JSON.stringify({ code }) }),
  },

  metrics: {
    current: () => request<import('./types').MetricsSnapshot>('/api/metrics/current'),
    wsUrl: () => {
      const proto = location.protocol === 'https:' ? 'wss' : 'ws'
      return `${proto}://${location.host}/api/metrics/ws`
    },
  },

  services: {
    list:   () => request<import('./types').ServicesResponse>('/api/services'),
    get:    (name: string) => request<import('./types').ServiceInfo>(`/api/services/${name}`),
    action: (name: string, action: import('./types').ServiceAction) =>
      request<{ ok: boolean }>(`/api/services/${name}/action`, {
        method: 'POST',
        body: JSON.stringify({ action }),
      }),
    logs:   (name: string) =>
      request<{ lines: string[] }>(`/api/services/${name}/logs`),
  },

  audit: {
    list: (limit = 50, offset = 0) =>
      request<{ entries: import('./types').AuditEntry[]; limit: number; offset: number }>(
        `/api/audit?limit=${limit}&offset=${offset}`,
      ),
  },

  containers: {
    list: () => request<import('./types').ContainersResponse>('/api/containers'),
    action: (id: string, action: import('./types').ContainerAction) =>
      request<{ ok: boolean }>(`/api/containers/${id}/action`, {
        method: 'POST',
        body: JSON.stringify({ action }),
      }),
    actionPlan: (id: string, action: import('./types').ContainerAction) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(
        `/api/containers/${id}/action`, {
          method: 'POST',
          body: JSON.stringify({ action, dry_run: true }),
        }),
    logs: (id: string, tail = 200) =>
      request<{ lines: string[] }>(`/api/containers/${id}/logs?tail=${tail}`),
    images: () => request<{ images: import('./types').ImageInfo[] }>('/api/containers/images'),
    getCompose: (id: string) =>
      request<{ compose_path: string; content: string; diff?: string }>(`/api/containers/${id}/compose`),
    proposeCompose: (id: string, content: string) =>
      request<{ ok: boolean; diff: string; services: string[] }>(`/api/containers/${id}/compose/propose`, {
        method: 'POST', body: JSON.stringify({ content }),
      }),
    applyCompose: (id: string, content: string) =>
      request<{ ok: boolean }>(`/api/containers/${id}/compose/apply`, {
        method: 'POST', body: JSON.stringify({ content }),
      }),
  },

  apps: {
    catalog:  () => request<{ apps: import('./types').AppDef[] }>('/api/apps/catalog'),
    deployed: () => request<import('./types').DeployedResponse>('/api/apps/deployed'),
    deploy: (appId: string, projectName?: string, envOverrides?: Record<string, string>) =>
      request<{ ok: boolean; project_name: string }>('/api/apps/deploy', {
        method: 'POST',
        body: JSON.stringify({ app_id: appId, project_name: projectName, env_overrides: envOverrides }),
      }),
    start:   (p: string) => request<{ ok: boolean }>(`/api/apps/${p}/start`,   { method: 'POST' }),
    stop:    (p: string) => request<{ ok: boolean }>(`/api/apps/${p}/stop`,    { method: 'POST' }),
    restart:  (p: string) => request<{ ok: boolean }>(`/api/apps/${p}/restart`,  { method: 'POST' }),
    redeploy: (p: string) => request<{ ok: boolean }>(`/api/apps/${p}/redeploy`, { method: 'POST' }),
    remove:   (p: string) => request<{ ok: boolean }>(`/api/apps/${p}`,          { method: 'DELETE' }),
    logs:    (p: string) => request<{ lines: string[] }>(`/api/apps/${p}/logs`),
    status:  (p: string) => request<{ containers: import('./types').ComposeContainer[] }>(`/api/apps/${p}/status`),
    getCompose:    (p: string) => request<{ compose_path: string; content: string }>(`/api/apps/${p}/compose`),
    updateCompose: (p: string, content: string) =>
      request<{ ok: boolean }>(`/api/apps/${p}/compose`, { method: 'POST', body: JSON.stringify({ content }) }),
    deployCustom: (body: { name: string; image: string; ports: string[]; volumes: string[]; env: string[] }) =>
      request<{ ok: boolean; project_name: string }>('/api/apps/deploy-custom', {
        method: 'POST', body: JSON.stringify(body),
      }),
    openUi: (projectName: string, primaryPort: number) =>
      request<{ url: string; embed_url: string | null; proxy_created: boolean }>('/api/apps/open-ui', {
        method: 'POST',
        body: JSON.stringify({ project_name: projectName, primary_port: primaryPort }),
      }),
    detectExternal: () =>
      request<import('./types').ExternalStack[]>('/api/apps/detect-external'),
    adoptApp: (body: { project_name: string; app_name: string; compose_path?: string; primary_port?: number }) =>
      request<{ ok: boolean }>('/api/apps/adopt', { method: 'POST', body: JSON.stringify(body) }),
    convertApp: (projectName: string) =>
      request<{ ok: boolean }>(`/api/apps/${projectName}/convert`, { method: 'POST' }),
  },

  alerts: {
    list: (state = 'active', severity?: string) => {
      const params = new URLSearchParams({ state })
      if (severity) params.set('severity', severity)
      return request<import('./types').AlertsResponse>(`/api/alerts?${params}`)
    },
    acknowledge: (id: string) =>
      request<{ ok: boolean }>(`/api/alerts/${id}/acknowledge`, { method: 'POST' }),
    resolve: (id: string) =>
      request<{ ok: boolean }>(`/api/alerts/${id}/resolve`, { method: 'POST' }),
    delete: (id: string) =>
      request<{ ok: boolean }>(`/api/alerts/${id}`, { method: 'DELETE' }),
  },

  proxy: {
    list: () =>
      request<{ proxies: import('./types').ProxyConfig[]; nginx_available: boolean; nginx_backend: 'docker' | 'system' | 'none'; sites_dir: string }>('/api/proxy'),
    create: (domain: string, upstream: string, ssl: boolean, allow_embed = false) =>
      request<{ ok: boolean; id: string; nginx: string }>('/api/proxy', {
        method: 'POST',
        body: JSON.stringify({ domain, upstream, ssl, allow_embed }),
      }),
    delete: (id: string) =>
      request<{ ok: boolean; nginx: string }>(`/api/proxy/${id}`, { method: 'DELETE' }),
    update: (id: string, domain: string, upstream: string, ssl: boolean, allow_embed: boolean) =>
      request<{ ok: boolean; nginx: string }>(`/api/proxy/${id}`, {
        method: 'PUT',
        body: JSON.stringify({ domain, upstream, ssl, allow_embed }),
      }),
    toggle: (id: string) =>
      request<{ ok: boolean; enabled: boolean; nginx: string }>(`/api/proxy/${id}/toggle`, { method: 'POST' }),
    plan: (domain: string, upstream: string, ssl: boolean, allow_embed: boolean) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>('/api/proxy', {
        method: 'POST',
        body: JSON.stringify({ domain, upstream, ssl, allow_embed, dry_run: true }),
      }),
    planUpdate: (id: string, domain: string, upstream: string, ssl: boolean, allow_embed: boolean) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(`/api/proxy/${id}`, {
        method: 'PUT',
        body: JSON.stringify({ domain, upstream, ssl, allow_embed, dry_run: true }),
      }),
  },

  files: {
    roots: () => request<{ roots: import('./types').FsRoot[] }>('/api/files/roots'),
    list: (path: string) =>
      request<import('./types').FilesListResponse>(`/api/files/list?path=${encodeURIComponent(path)}`),
    read: (path: string) =>
      request<{ path: string; content: string; size: number; truncated: boolean }>(
        `/api/files/read?path=${encodeURIComponent(path)}`),
    write: (path: string, content: string) =>
      request<{ ok: boolean }>('/api/files/write', { method: 'POST', body: JSON.stringify({ path, content }) }),
    mkdir: (path: string) =>
      request<{ ok: boolean }>('/api/files/mkdir', { method: 'POST', body: JSON.stringify({ path }) }),
    delete: (path: string) =>
      request<{ ok: boolean }>(`/api/files/delete?path=${encodeURIComponent(path)}`, { method: 'DELETE' }),
    rename: (from: string, to: string) =>
      request<{ ok: boolean }>('/api/files/rename', { method: 'POST', body: JSON.stringify({ from, to }) }),
  },

  security: {
    sessions: () =>
      request<import('./types').SessionsResponse>('/api/security/sessions'),
    revokeSession: (id: string) =>
      request<{ ok: boolean }>(`/api/security/sessions/${id}`, { method: 'DELETE' }),
    revokeOthers: () =>
      request<{ ok: boolean; revoked: number }>('/api/security/sessions/revoke-others', { method: 'POST' }),
  },

  users: {
    list: () => request<{ users: import('./types').UserRecord[] }>('/api/users'),
    create: (username: string, password: string, role: string) =>
      request<{ user: import('./types').UserRecord }>('/api/users', {
        method: 'POST',
        body: JSON.stringify({ username, password, role }),
      }),
    delete: (id: string) =>
      request<{ ok: boolean }>(`/api/users/${id}`, { method: 'DELETE' }),
    changePassword: (password: string, username?: string) =>
      request<{ ok: boolean }>('/api/users/me/password', {
        method: 'POST',
        body: JSON.stringify({ password, username }),
      }),
  },

  terminal: {
    wsUrl: (sessionId?: string) => {
      const proto = location.protocol === 'https:' ? 'wss' : 'ws'
      const base = `${proto}://${location.host}/api/terminal/ws`
      return sessionId ? `${base}?session_id=${encodeURIComponent(sessionId)}` : base
    },
    sshWsUrl: (sessionId: string) => {
      const proto = location.protocol === 'https:' ? 'wss' : 'ws'
      return `${proto}://${location.host}/api/terminal/ssh/ws?session_id=${encodeURIComponent(sessionId)}`
    },
    listSshSessions: () => request<import('./types').SshSession[]>('/api/terminal/ssh/sessions'),
    createSshSession: (s: { label: string; host: string; port: number; username: string; key_path?: string; password?: string }) =>
      request<import('./types').SshSession>('/api/terminal/ssh/sessions', { method: 'POST', body: JSON.stringify(s) }),
    updateSshSession: (id: string, s: { label: string; host: string; port: number; username: string; key_path?: string; password?: string }) =>
      request<import('./types').SshSession>(`/api/terminal/ssh/sessions/${id}`, { method: 'PUT', body: JSON.stringify(s) }),
    deleteSshSession: (id: string) =>
      request<{ ok: boolean }>(`/api/terminal/ssh/sessions/${id}`, { method: 'DELETE' }),
    listLocalSessions: () => request<import('./types').LocalSession[]>('/api/terminal/local/sessions'),
    createLocalSession: (s: { label: string; category?: string }) =>
      request<import('./types').LocalSession>('/api/terminal/local/sessions', { method: 'POST', body: JSON.stringify(s) }),
    updateLocalSession: (id: string, s: { label: string; category?: string }) =>
      request<import('./types').LocalSession>(`/api/terminal/local/sessions/${id}`, { method: 'PUT', body: JSON.stringify(s) }),
    deleteLocalSession: (id: string) =>
      request<{ ok: boolean }>(`/api/terminal/local/sessions/${id}`, { method: 'DELETE' }),
  },

  timeline: {
    list: (params: { limit?: number; offset?: number; category?: string; outcome?: string; search?: string; from?: number; to?: number }) => {
      const p = new URLSearchParams()
      if (params.limit)    p.set('limit',    String(params.limit))
      if (params.offset)   p.set('offset',   String(params.offset))
      if (params.category) p.set('category', params.category)
      if (params.outcome)  p.set('outcome',  params.outcome)
      if (params.search)   p.set('search',   params.search)
      if (params.from)     p.set('from',     String(params.from))
      if (params.to)       p.set('to',       String(params.to))
      return request<import('./types').TimelineResponse>(`/api/timeline?${p}`)
    },
  },

  secrets: {
    list:   () => request<import('./types').SecretsResponse>('/api/secrets'),
    create: (name: string, description: string, value: string) =>
      request<{ id: string }>('/api/secrets', { method: 'POST', body: JSON.stringify({ name, description, value }) }),
    update: (id: string, patch: { name?: string; description?: string; value?: string }) =>
      request<{ ok: boolean }>(`/api/secrets/${id}`, { method: 'PATCH', body: JSON.stringify(patch) }),
    delete: (id: string) => request<{ ok: boolean }>(`/api/secrets/${id}`, { method: 'DELETE' }),
    reveal: (id: string) => request<{ value: string }>(`/api/secrets/${id}/reveal`),
    rotate: (id: string, new_value?: string) =>
      request<{ rotated: boolean; version: number }>(`/api/secrets/${id}/rotate`, {
        method: 'POST', body: JSON.stringify({ new_value }),
      }),
  },

  wireguard: {
    list: () => request<import('./types').WireguardResponse>('/api/wireguard'),
    addPeer: (name: string, iface: string, serverEndpoint?: string) =>
      request<{ id: string; public_key: string; allocated_ip: string; client_config: string; warnings: string[] }>(
        '/api/wireguard/peers',
        { method: 'POST', body: JSON.stringify({ name, interface: iface, server_endpoint: serverEndpoint }) },
      ),
    deletePeer: (id: string) =>
      request<{ ok: boolean; warnings: string[] }>(`/api/wireguard/peers/${id}`, { method: 'DELETE' }),
  },

  capabilities: {
    list: () => request<import('./types').CapabilitiesResponse>('/api/capabilities'),
  },

  diagnostics: {
    run: () => request<import('./types').DiagnosticsResponse>('/api/diagnostics'),
  },

  tags: {
    list: () => request<import('./types').Tag[]>('/api/tags'),
    create: (name: string, color: string) =>
      request<import('./types').Tag>('/api/tags', { method: 'POST', body: JSON.stringify({ name, color }) }),
    update: (id: string, patch: { name?: string; color?: string }) =>
      request<import('./types').Tag>(`/api/tags/${id}`, { method: 'PATCH', body: JSON.stringify(patch) }),
    delete: (id: string) => request<{ ok: boolean }>(`/api/tags/${id}`, { method: 'DELETE' }),
    map: (type: string) => request<import('./types').TagMap>(`/api/tags/map?type=${encodeURIComponent(type)}`),
    assign: (tag_id: string, resource_type: string, resource_id: string) =>
      request<{ ok: boolean }>('/api/tags/assign', { method: 'POST', body: JSON.stringify({ tag_id, resource_type, resource_id }) }),
    unassign: (tag_id: string, resource_type: string, resource_id: string) =>
      request<{ ok: boolean }>('/api/tags/unassign', { method: 'POST', body: JSON.stringify({ tag_id, resource_type, resource_id }) }),
  },

  storage: {
    devices: () => request<{ devices: import('./types').BlockDevice[] }>('/api/storage/devices'),
    mounts:  () => request<{ mounts: import('./types').MountInfo[] }>('/api/storage/mounts'),
    mount: (device: string, mountpoint: string, fstype: string, options?: string) =>
      request<{ ok: boolean }>('/api/storage/mount', {
        method: 'POST',
        body: JSON.stringify({ device, mountpoint, fstype, options }),
      }),
    umount: (mountpoint: string) =>
      request<{ ok: boolean }>('/api/storage/umount', {
        method: 'POST',
        body: JSON.stringify({ mountpoint }),
      }),
    fstab:     () => request<{ entries: import('./types').FstabEntry[] }>('/api/storage/fstab'),
    addFstab:  (e: { device: string; mountpoint: string; fstype: string; options: string; dump?: number; pass?: number }) =>
      request<{ ok: boolean }>('/api/storage/fstab', { method: 'POST', body: JSON.stringify(e) }),
    removeFstab: (idx: number) =>
      request<{ ok: boolean }>(`/api/storage/fstab/${idx}`, { method: 'DELETE' }),
    smart: (dev: string) => request<import('./types').SmartInfo>(`/api/storage/smart/${encodeURIComponent(dev)}`),
    raid:  () => request<{ available: boolean; arrays: import('./types').RaidArray[] }>('/api/storage/raid'),
    createRaid: (name: string, level: string, devices: string[]) =>
      request<{ ok: boolean; path: string }>('/api/storage/raid/create', {
        method: 'POST',
        body: JSON.stringify({ name, level, devices }),
      }),
    stopRaid: (path: string) =>
      request<{ ok: boolean }>('/api/storage/raid/stop', {
        method: 'POST',
        body: JSON.stringify({ path }),
      }),
    format: (device: string, fstype: string, label?: string) =>
      request<{ ok: boolean }>('/api/storage/format', {
        method: 'POST',
        body: JSON.stringify({ device, fstype, label }),
      }),
    getPaths: () => request<import('./types').StoragePaths>('/api/storage/paths'),
    setPaths: (paths: Partial<import('./types').StoragePaths>) =>
      request<{ ok: boolean }>('/api/storage/paths', { method: 'POST', body: JSON.stringify(paths) }),
  },

  models: {
    list:             () => request<import('./types').ModelFile[]>('/api/models'),
    startDownload:    (url: string, filename?: string) =>
      request<{ id: string }>('/api/models/download', { method: 'POST', body: JSON.stringify({ url, filename }) }),
    downloadStatus:   (id: string) => request<import('./types').DownloadStatus>(`/api/models/download/${id}`),
    deleteModel:      (filename: string) =>
      request<{ ok: boolean }>(`/api/models/${encodeURIComponent(filename)}`, { method: 'DELETE' }),
    loadModel:        (filename: string) =>
      request<{ ok: boolean }>('/api/models/load', { method: 'POST', body: JSON.stringify({ filename }) }),
    getActive:        () => request<{ filename: string | null }>('/api/models/active'),
    ollamaPull:         (model: string) =>
      request<{ id: string }>('/api/models/ollama/pull', { method: 'POST', body: JSON.stringify({ model }) }),
    ollamaPullStatus:   (id: string) => request<import('./types').OllamaPullStatus>(`/api/models/ollama/pull/${id}`),
    ollamaCreate:       (filename: string) =>
      request<{ id: string; model_name: string }>('/api/models/ollama/create', { method: 'POST', body: JSON.stringify({ filename }) }),
    ollamaCreateStatus: (id: string) => request<import('./types').OllamaPullStatus>(`/api/models/ollama/create/${id}`),
    ollamaTags:         () => request<import('./types').OllamaTagsResponse>('/api/models/ollama'),
  },

  vms: {
    listLocal: () => request<import('./types').LocalVmsResponse>('/api/vms/local'),
    localAction: (name: string, action: string) =>
      request<{ ok: boolean; message: string }>('/api/vms/local/action', {
        method: 'POST', body: JSON.stringify({ name, action }),
      }),
    getProxmoxConfig: () => request<import('./types').ProxmoxConfig | null>('/api/vms/proxmox/config'),
    setProxmoxConfig: (cfg: import('./types').ProxmoxConfig) =>
      request<{ ok: boolean }>('/api/vms/proxmox/config', {
        method: 'POST', body: JSON.stringify(cfg),
      }),
    listProxmox: () => request<import('./types').ProxmoxVmsResponse>('/api/vms/proxmox/vms'),
    proxmoxAction: (vmid: number, kind: string, node: string, action: string) =>
      request<{ ok: boolean; message?: string }>('/api/vms/proxmox/action', {
        method: 'POST', body: JSON.stringify({ vmid, kind, node, action }),
      }),
    testProxmox: () =>
      request<{ ok: boolean; nodes?: string[]; message?: string }>('/api/vms/proxmox/test', { method: 'POST' }),
  },

  mods: {
    getStatus: () => request<import('./types').ModStatus>('/api/mods'),
    saveConfig: (body: { url: string; branch: string }) =>
      request<{ ok: boolean }>('/api/mods/config', { method: 'POST', body: JSON.stringify(body) }),
    fetch: () => request<import('./types').ModFetchResult>('/api/mods/fetch', { method: 'POST' }),
    getDiff: () => request<{ diff: string }>('/api/mods/diff'),
    apply: () => request<{ ok: boolean; output: string }>('/api/mods/apply', { method: 'POST' }),
    rollback: () => request<{ ok: boolean }>('/api/mods/rollback', { method: 'POST' }),
  },

  integrations: {
    scopes: () => request<{ scopes: { name: string; description: string }[] }>('/api/integrations/scopes'),
    listTokens: () => request<{ tokens: import('./types').ApiToken[] }>('/api/integrations/tokens'),
    createToken: (name: string, scopes: string[], expires_days?: number, secret_ids?: string[]) =>
      request<{ id: string; token: string; name: string; scopes: string[]; created_at: number; secret_ids: string[] | null }>(
        '/api/integrations/tokens', { method: 'POST', body: JSON.stringify({ name, scopes, expires_days, secret_ids: secret_ids?.length ? secret_ids : undefined }) },
      ),
    revokeToken: (id: string) =>
      request<{ ok: boolean }>(`/api/integrations/tokens/${id}`, { method: 'DELETE' }),
    getOdysseusConfig: () => request<import('./types').OdysseusConfig>('/api/integrations/odysseus/config'),
    saveOdysseusConfig: (cfg: {
      enabled?: boolean; mcp_enabled?: boolean; allowed_url?: string;
      regenerate_webhook_secret?: boolean; emergency_disable?: boolean;
    }) => request<{ ok: boolean; webhook_secret?: string }>('/api/integrations/odysseus/config', { method: 'POST', body: JSON.stringify(cfg) }),
    manifest: () => request<import('./types').OdysseusManifest>('/api/integrations/odysseus/manifest'),
    recentActions: () => request<{ actions: import('./types').AuditAction[] }>('/api/integrations/actions'),
    eventsUrl: (token?: string) => {
      const base = (import.meta.env.VITE_API_BASE ?? '')
      return token ? `${base}/api/integrations/events?token=${encodeURIComponent(token)}` : `${base}/api/integrations/events`
    },
  },

  proxmox: {
    listHosts:  () => request<import('./types').ProxmoxHost[]>('/api/proxmox/hosts'),
    addHost:    (data: import('./types').AddHostRequest) =>
      request<{ ok: boolean }>('/api/proxmox/hosts', { method: 'POST', body: JSON.stringify(data) }),
    deleteHost: (id: string) =>
      request<{ ok: boolean }>(`/api/proxmox/hosts/${id}`, { method: 'DELETE' }),
    getNodes:   (hostId: string) => request<import('./types').PveNode[]>(`/api/proxmox/${hostId}/nodes`),
    getVms:     (hostId: string) => request<import('./types').PveVm[]>(`/api/proxmox/${hostId}/vms`),
    getStorage: (hostId: string) => request<import('./types').PveStorage[]>(`/api/proxmox/${hostId}/storage`),
    getTasks:   (hostId: string) => request<import('./types').PveTask[]>(`/api/proxmox/${hostId}/tasks`),
    vmAction:     (hostId: string, vmid: number, action: 'start' | 'stop' | 'reboot') =>
      request<{ ok: boolean; task: string }>(`/api/proxmox/${hostId}/vms/${vmid}/${action}`, { method: 'POST' }),
    getSnapshots: (hostId: string, vmid: number, kind: 'qemu' | 'lxc') =>
      request<import('./types').PveSnapshot[]>(`/api/proxmox/${hostId}/vms/${vmid}/snapshots?kind=${kind}`),
    createSnapshot: (hostId: string, vmid: number, name: string, desc: string) =>
      request<{ ok: boolean; task: string }>(`/api/proxmox/${hostId}/vms/${vmid}/snapshot`, { method: 'POST', body: JSON.stringify({ name, description: desc }) }),
    deleteSnapshot: (hostId: string, vmid: number, snapname: string) =>
      request<{ ok: boolean; task: string }>(`/api/proxmox/${hostId}/vms/${vmid}/snapshot/${snapname}`, { method: 'DELETE' }),
    rollbackSnapshot: (hostId: string, vmid: number, snapname: string) =>
      request<{ ok: boolean; task: string }>(`/api/proxmox/${hostId}/vms/${vmid}/rollback/${snapname}`, { method: 'POST' }),
    vncProxy: (hostId: string, vmid: number) =>
      request<{ ticket: string; port: number; proxmox_host: string; node: string; kind: string; vmid: number }>(
        `/api/proxmox/${hostId}/vms/${vmid}/vncproxy`, { method: 'POST' }),
    getBackupJobs: (hostId: string) =>
      request<{ jobs: import('./types').PveBackupJob[]; archives: import('./types').PveBackupArchive[] }>(`/api/proxmox/${hostId}/backup-jobs`),
  },

  policy: {
    list: () =>
      request<import('./types').PolicyRule[]>('/api/policy/rules'),
    create: (rule: { name: string; actor_type: string; action: string; resource_type: string; resource_tag?: string | null; effect: string; priority?: number }) =>
      request<import('./types').PolicyRule>('/api/policy/rules', { method: 'POST', body: JSON.stringify(rule) }),
    update: (id: string, patch: Partial<{ name: string; actor_type: string; action: string; resource_type: string; resource_tag: string | null; effect: string; priority: number; enabled: boolean }>) =>
      request<import('./types').PolicyRule>(`/api/policy/rules/${id}`, { method: 'PATCH', body: JSON.stringify(patch) }),
    delete: (id: string) =>
      request<{ ok: boolean }>(`/api/policy/rules/${id}`, { method: 'DELETE' }),
    check: (params: { actor_type: string; action: string; resource_type: string; resource_id: string }) =>
      request<import('./types').PolicyCheckResult>('/api/policy/check', { method: 'POST', body: JSON.stringify(params) }),
  },
}

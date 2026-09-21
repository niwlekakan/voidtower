/// <reference types="vite/client" />
import type { ApiError } from './types'
import { useAuthStore } from '@/store/auth'
import { API_V1_ENVELOPE_CONTRACT } from './generatedApiContract'
import {
  parseApprovalListEnvelope,
  parseApprovalReadEnvelope,
  parseApiErrorEnvelope,
  parseEventHistoryEnvelope,
  parseJobListEnvelope,
  parseJobReadEnvelope,
  parseJobSuccessEnvelope,
  parsePlanSuccessEnvelope,
  parseResourceCapabilitiesEnvelope,
  parseResourceListEnvelope,
  parseResourceReadEnvelope,
  parseVersionNegotiationErrorEnvelope,
} from './envelopeClient'

const BASE = import.meta.env.VITE_API_BASE ?? ''
const API_VERSION_HEADER = 'x-voidtower-api-version'
const MAX_CANONICAL_TARGET_LENGTH = 256
const MAX_PAGE_LIMIT = 500

export class ApiClientError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly status: number,
    public readonly supportedVersions: string[] | null = null,
  ) {
    super(message)
    this.name = 'ApiClientError'
  }
}

/**
 * The web app is served same-origin with the backend (through nginx), so the
 * browser's own fetch + cookie jar works fine. The desktop (Tauri) app talks
 * to a *remote* VoidTower instance from its own `tauri://` origin — from the
 * webview's perspective that makes every API call cross-site, and the
 * `vt_session` cookie (SameSite=Strict, backend/src/api/auth.rs) would never
 * be sent back. `@tauri-apps/plugin-http`'s fetch runs the request through
 * Rust instead of the webview's networking stack, so it isn't subject to
 * browser same-site/CORS enforcement and keeps its own per-host cookie jar —
 * swap to it only when actually running inside a Tauri window.
 */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window
}

let tauriFetchPromise: Promise<typeof fetch> | null = null
function resolveFetch(): Promise<typeof fetch> {
  if (!isTauri()) return Promise.resolve(fetch)
  if (!tauriFetchPromise) {
    tauriFetchPromise = import('@tauri-apps/plugin-http').then((m) => m.fetch as unknown as typeof fetch)
  }
  return tauriFetchPromise
}

async function request<T>(path: string, init?: RequestInit, parse?: (value: unknown) => T): Promise<T> {
  const requestSessionEpoch = useAuthStore.getState().sessionEpoch
  const doFetch = await resolveFetch()
  const res = await doFetch(`${BASE}${path}`, {
    ...init,
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      [API_VERSION_HEADER]: API_V1_ENVELOPE_CONTRACT.api_version,
      ...init?.headers,
    },
  })

  if (!res.ok) {
    if (res.status === 401 && useAuthStore.getState().sessionEpoch === requestSessionEpoch) {
      useAuthStore.getState().logout()
    }
    let body: ApiError | null = null
    try { body = await res.json() } catch { /* ignore */ }
    if (body && typeof body.error === 'object' && body.error !== null) {
      if (body.error.code === 'unsupported_api_version') {
        try {
          const error = parseVersionNegotiationErrorEnvelope(body)
          throw new ApiClientError(error.error.message, error.error.code, res.status, error.error.supported_versions)
        } catch (error) {
          if (error instanceof ApiClientError) throw error
        }
      }
      try {
        const error = parseApiErrorEnvelope(body)
        throw new ApiClientError(error.error.message, error.error.code, res.status)
      } catch {
        const errorBody = body.error as Record<string, unknown>
        const code = typeof errorBody.code === 'string' ? errorBody.code.slice(0, 128) : 'invalid_api_error'
        const message = typeof errorBody.message === 'string'
          ? errorBody.message.slice(0, 1024)
          : 'The API returned an invalid error envelope.'
        throw new ApiClientError(message, code, res.status)
      }
    }
    throw new ApiClientError(
      body?.error?.message ?? res.statusText,
      body?.error?.code ?? 'unknown',
      res.status,
    )
  }

  if (res.status === 204) return undefined as T
  const body = await res.json()
  return parse ? parse(body) : body as T
}

function requestJob(path: string, init?: RequestInit): Promise<import('./types').DurableJobResponse> {
  return request<import('./types').DurableJobResponse>(path, init, parseJobSuccessEnvelope)
}

type DurablePlanOrJobResponse =
  | { dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }
  | import('./types').DurableJobResponse

function parsePlanOrJobResponse(value: unknown): DurablePlanOrJobResponse {
  if (typeof value === 'object' && value !== null && (value as Record<string, unknown>).dry_run === true) {
    return value as DurablePlanOrJobResponse
  }
  return parseJobSuccessEnvelope<import('./types').DurableJob>(value)
}

function validateCanonicalTarget(value: string): void {
  if (typeof value !== 'string' || value.length === 0 || value === '.' || value === '..' || value.trim().length === 0 || value.length > MAX_CANONICAL_TARGET_LENGTH) {
    throw new ApiClientError('Invalid canonical action target.', 'invalid_action_target', 400)
  }
}

function validatePageLimit(value: number): void {
  if (!Number.isSafeInteger(value) || value < 1 || value > MAX_PAGE_LIMIT) {
    throw new ApiClientError('Invalid page limit.', 'invalid_pagination', 400)
  }
}

function validateEventCursor(value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new ApiClientError('Invalid event cursor.', 'invalid_pagination', 400)
  }
}

function proxyOptsBody(opts: import('./types').ProxyOptions) {
  return {
    custom_headers: opts.customHeaders ?? [],
    rate_limit_rpm: opts.rateLimitRpm ?? null,
    basic_auth_user: opts.basicAuthUser ?? null,
    basic_auth_secret_id: opts.basicAuthSecretId ?? null,
    websocket_extended: opts.websocketExtended ?? false,
    cache_static: opts.cacheStatic ?? false,
  }
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
    oidcStatus: () => request<import('./types').OidcStatus>('/api/auth/oidc/status'),
  },

  oidc: {
    get: () => request<import('./types').OidcConfig>('/api/oidc/config'),
    save: (cfg: import('./types').OidcConfigSaveRequest) =>
      request<{ ok: boolean }>('/api/oidc/config', { method: 'PUT', body: JSON.stringify(cfg) }),
    plan: (cfg: import('./types').OidcConfigSaveRequest) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>('/api/oidc/config', {
        method: 'PUT',
        body: JSON.stringify({ ...cfg, dry_run: true }),
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

  events: {
    history: (after = 0, limit = 100) => {
      validateEventCursor(after)
      validatePageLimit(limit)
      return request<import('./types').DurableEventHistoryResponse>(
        `/api/events?after=${encodeURIComponent(String(after))}&limit=${encodeURIComponent(String(limit))}`,
        undefined,
        parseEventHistoryEnvelope,
      )
    },
    streamUrl: (after?: number) => {
      if (after !== undefined) validateEventCursor(after)
      const query = after === undefined ? '' : `?after=${encodeURIComponent(String(after))}`
      return `${BASE}/api/events/stream${query}`
    },
  },

  resources: {
    list: (limit = 100) => {
      validatePageLimit(limit)
      return request<import('./types').DurableResourceListResponse>(
        `/api/resources?limit=${encodeURIComponent(String(limit))}`,
        undefined,
        parseResourceListEnvelope,
      )
    },
    get: (id: string) =>
      request<import('./types').DurableResourceReadResponse>(
        `/api/resources/${encodeURIComponent(id)}`,
        undefined,
        parseResourceReadEnvelope,
      ),
    capabilities: (id: string) =>
      request<import('./types').DurableResourceCapabilitiesResponse>(
        `/api/resources/${encodeURIComponent(id)}/capabilities`,
        undefined,
        parseResourceCapabilitiesEnvelope,
      ),
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
      requestJob(`/api/containers/${id}/action`, {
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
    previewCompose: (id: string, path: string, content: string) =>
      request<{ added: number; removed: number; current_lines: number; proposed_lines: number }>(`/api/containers/${id}/compose/propose`, {
        method: 'POST', body: JSON.stringify({ path, content }),
      }),
    applyCompose: (id: string, path: string, content: string) =>
      requestJob(`/api/containers/${id}/compose/apply`, {
        method: 'POST', body: JSON.stringify({ path, content }),
      }),
  },

  apps: {
    catalog:  () => request<{ apps: import('./types').AppDef[] }>('/api/apps/catalog'),
    deployed: () => request<import('./types').DeployedResponse>('/api/apps/deployed'),
    deploy: (
      appId: string,
      projectName?: string,
      envOverrides?: Record<string, string>,
      storageDriveId?: string,
      targetNodeId?: string,
    ) =>
      request<{ ok: boolean; project_name: string; generated_env?: Record<string, string> }>('/api/apps/deploy', {
        method: 'POST',
        body: JSON.stringify({
          app_id: appId, project_name: projectName, env_overrides: envOverrides,
          storage_drive_id: storageDriveId, target_node_id: targetNodeId,
        }),
      }),
    cancelDeploy: (p: string) =>
      request<{ ok: boolean; cancelled: boolean }>(`/api/apps/deploy/cancel/${p}`, { method: 'POST' }),
    start:   (p: string) => request<{ ok: boolean }>(`/api/apps/${p}/start`,   { method: 'POST' }),
    stop:    (p: string) => request<{ ok: boolean }>(`/api/apps/${p}/stop`,    { method: 'POST' }),
    restart:  (p: string) => request<{ ok: boolean }>(`/api/apps/${p}/restart`,  { method: 'POST' }),
    redeploy: (p: string) => request<{ ok: boolean }>(`/api/apps/${p}/redeploy`, { method: 'POST' }),
    remove:   (p: string) => request<{ ok: boolean }>(`/api/apps/${p}`,          { method: 'DELETE' }),
    logs:    (p: string) => request<{ lines: string[] }>(`/api/apps/${p}/logs`),
    status:  (p: string) => request<{ containers: import('./types').ComposeContainer[] }>(`/api/apps/${p}/status`),
    getCompose:    (p: string) => request<{ content: string }>(`/api/apps/${p}/compose`),
    updateCompose: (p: string, content: string) =>
      request<{ ok: boolean }>(`/api/apps/${p}/compose`, { method: 'POST', body: JSON.stringify({ content }) }),
    deployCustom: (body: {
      name: string; image: string; ports: string[]; volumes: string[]; env: string[]
      storage_drive_id?: string; target_node_id?: string
    }) =>
      request<{ ok: boolean; project_name: string }>('/api/apps/deploy-custom', {
        method: 'POST', body: JSON.stringify(body),
      }),
    openUi: (projectName: string, primaryPort: number) =>
      request<{ url: string; embed_url: string | null; proxy_created: false; proxy_available: boolean }>('/api/apps/open-ui', {
        method: 'POST',
        body: JSON.stringify({ project_name: projectName, primary_port: primaryPort }),
      }),
    expose: (projectName: string, body: { domain: string; ssl?: boolean; allow_embed?: boolean }) =>
      requestJob(`/api/apps/${projectName}/expose`, {
        method: 'POST', body: JSON.stringify(body),
      }),
    detectExternal: () =>
      request<import('./types').ExternalStack[]>('/api/apps/detect-external'),
    adoptApp: (body: { project_name: string; app_name: string; primary_port?: number }) =>
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
    create: (domain: string, upstream: string, ssl: boolean, allow_embed = false, sso_protect = false, opts: import('./types').ProxyOptions = {}) =>
      requestJob('/api/proxy', {
        method: 'POST',
        body: JSON.stringify({ domain, upstream, ssl, allow_embed, sso_protect, ...proxyOptsBody(opts) }),
      }),
    delete: (id: string) =>
      requestJob(`/api/proxy/${id}`, { method: 'DELETE' }),
    update: (id: string, domain: string, upstream: string, ssl: boolean, allow_embed: boolean, sso_protect = false, opts: import('./types').ProxyOptions = {}) =>
      requestJob(`/api/proxy/${id}`, {
        method: 'PUT',
        body: JSON.stringify({ domain, upstream, ssl, allow_embed, sso_protect, ...proxyOptsBody(opts) }),
      }),
    toggle: (id: string) =>
      requestJob(`/api/proxy/${id}/toggle`, { method: 'POST' }),
    health: (id: string) =>
      request<{ status: 'up' | 'down'; latency_ms: number; checked_at: number }>(`/api/proxy/${id}/health`),
    plan: (domain: string, upstream: string, ssl: boolean, allow_embed: boolean, sso_protect = false, opts: import('./types').ProxyOptions = {}) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>('/api/proxy', {
        method: 'POST',
        body: JSON.stringify({ domain, upstream, ssl, allow_embed, sso_protect, dry_run: true, ...proxyOptsBody(opts) }),
      }),
    planUpdate: (id: string, domain: string, upstream: string, ssl: boolean, allow_embed: boolean, sso_protect = false, opts: import('./types').ProxyOptions = {}) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(`/api/proxy/${id}`, {
        method: 'PUT',
        body: JSON.stringify({ domain, upstream, ssl, allow_embed, sso_protect, dry_run: true, ...proxyOptsBody(opts) }),
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

  members: {
    list: () => request<{ members: import('./types').MemberListEntry[] }>('/api/members'),
    myAccess: () => request<import('./types').MemberSelfAccessSummary>('/api/members/me/access'),
    myNodes: () => request<{ nodes: import('./types').MemberNodeOption[] }>('/api/members/me/nodes'),
    access: (userId: string) => request<import('./types').MemberAccessSummary>(`/api/members/${userId}/access`),
    grantAccess: (userId: string, appId: string) =>
      request<{ ok: boolean }>(`/api/members/${userId}/access`, {
        method: 'POST', body: JSON.stringify({ app_id: appId }),
      }),
    revokeAccess: (userId: string, appId: string) =>
      request<{ ok: boolean }>(`/api/members/${userId}/access/${appId}`, { method: 'DELETE' }),
    setCustomDeploy: (userId: string, enabled: boolean) =>
      request<{ ok: boolean; enabled: boolean }>(`/api/members/${userId}/custom-deploy`, {
        method: 'POST', body: JSON.stringify({ enabled }),
      }),
    setQuota: (userId: string, quotaBytes: number, maxApps: number) =>
      request<{ ok: boolean }>(`/api/members/${userId}/storage`, {
        method: 'POST', body: JSON.stringify({ quota_bytes: quotaBytes, max_apps: maxApps }),
      }),
    addDrive: (userId: string, label: string, hostPath: string) =>
      request<{ ok: boolean; id: string }>(`/api/members/${userId}/drives`, {
        method: 'POST', body: JSON.stringify({ label, host_path: hostPath }),
      }),
    removeDrive: (driveId: string) =>
      request<{ ok: boolean }>(`/api/members/drives/${driveId}`, { method: 'DELETE' }),
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
    getLlamaConfig:     () => request<import('./types').LlamaConfig>('/api/models/llama-config'),
    saveLlamaConfig:    (cfg: import('./types').LlamaConfig) =>
      request<{ ok: boolean }>('/api/models/llama-config', { method: 'POST', body: JSON.stringify(cfg) }),
    getOllamaConfig:    () => request<import('./types').OllamaConfig>('/api/models/ollama-config'),
    saveOllamaConfig:   (cfg: import('./types').OllamaConfig) =>
      request<{ ok: boolean }>('/api/models/ollama-config', { method: 'POST', body: JSON.stringify(cfg) }),
  },

  vms: {
    listLocal: () => request<import('./types').LocalVmsResponse>('/api/vms/local'),
    localAction: (name: string, action: string) =>
      request<{ ok: boolean; message: string }>('/api/vms/local/action', {
        method: 'POST', body: JSON.stringify({ name, action }),
      }),
    getProxmoxConfig: () => request<import('./types').ProxmoxConfig | null>('/api/vms/proxmox/config'),
    setProxmoxConfig: (cfg: import('./types').ProxmoxConfig) =>
      requestJob('/api/vms/proxmox/config', {
        method: 'POST', body: JSON.stringify(cfg),
      }),
    listProxmox: () => request<import('./types').ProxmoxVmsResponse>('/api/vms/proxmox/vms'),
    proxmoxAction: (vmid: number, kind: string, node: string, action: string) =>
      requestJob('/api/vms/proxmox/action', {
        method: 'POST', body: JSON.stringify({ vmid, kind, node, action }),
      }),
    testProxmox: () =>
      requestJob('/api/vms/proxmox/test', { method: 'POST' }),
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
      regenerate_webhook_secret?: boolean; revoke_webhook_secret?: boolean; emergency_disable?: boolean;
    }) => request<{ ok: boolean; webhook_secret?: string }>('/api/integrations/odysseus/config', { method: 'POST', body: JSON.stringify(cfg) }),
    manifest: () => request<import('./types').OdysseusManifest>('/api/integrations/odysseus/manifest'),
    recentActions: () => request<{ actions: import('./types').AuditAction[] }>('/api/integrations/actions'),
    eventsUrl: () => {
      const base = (import.meta.env.VITE_API_BASE ?? '')
      return `${base}/api/integrations/events`
    },
  },

  aiProviders: {
    list: () => request<import('./types').AiProviderConfig[]>('/api/ai/providers'),
    create: (req: import('./types').CreateAiProviderReq) =>
      request<{ ok: boolean; id: string }>('/api/ai/providers', { method: 'POST', body: JSON.stringify(req) }),
    update: (id: string, req: import('./types').UpdateAiProviderReq) =>
      request<{ ok: boolean }>(`/api/ai/providers/${id}`, { method: 'PUT', body: JSON.stringify(req) }),
    delete: (id: string) =>
      request<{ ok: boolean }>(`/api/ai/providers/${id}`, { method: 'DELETE' }),
    health: (id: string) =>
      request<import('./types').AiProviderHealthResult>(`/api/ai/providers/${id}/health`),
  },

  proxmox: {
    listHosts:  () => request<import('./types').ProxmoxHost[]>('/api/proxmox/hosts'),
    addHost:    (data: import('./types').AddHostRequest) =>
      requestJob('/api/proxmox/hosts', { method: 'POST', body: JSON.stringify(data) }),
    deleteHost: (id: string) =>
      requestJob(`/api/proxmox/hosts/${id}`, { method: 'DELETE' }),
    getNodes:   (hostId: string) => request<import('./types').PveNode[]>(`/api/proxmox/${hostId}/nodes`),
    getVms:     (hostId: string) => request<import('./types').PveVm[]>(`/api/proxmox/${hostId}/vms`),
    getStorage: (hostId: string) => request<import('./types').PveStorage[]>(`/api/proxmox/${hostId}/storage`),
    getTasks:   (hostId: string) => request<import('./types').PveTask[]>(`/api/proxmox/${hostId}/tasks`),
    vmAction:     (hostId: string, vmid: number, action: 'start' | 'stop' | 'shutdown' | 'reboot' | 'reset' | 'suspend' | 'resume') =>
      requestJob(`/api/proxmox/${hostId}/vms/${vmid}/${action}`, { method: 'POST' }),
    vmActionPlan: (hostId: string, vmid: number, action: 'start' | 'stop' | 'reboot' | 'reset' | 'suspend') =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(
        `/api/proxmox/${hostId}/vms/${vmid}/${action}`, { method: 'POST', body: JSON.stringify({ dry_run: true }) }),
    getSnapshots: (hostId: string, vmid: number, kind: 'qemu' | 'lxc') =>
      request<import('./types').PveSnapshot[]>(`/api/proxmox/${hostId}/vms/${vmid}/snapshots?kind=${kind}`),
    createSnapshot: (hostId: string, vmid: number, name: string, desc: string) =>
      requestJob(`/api/proxmox/${hostId}/vms/${vmid}/snapshot`, { method: 'POST', body: JSON.stringify({ name, description: desc }) }),
    createSnapshotPlan: (hostId: string, vmid: number, name: string, desc: string) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(
        `/api/proxmox/${hostId}/vms/${vmid}/snapshot`, { method: 'POST', body: JSON.stringify({ name, description: desc, dry_run: true }) }),
    deleteSnapshot: (hostId: string, vmid: number, snapname: string) =>
      requestJob(`/api/proxmox/${hostId}/vms/${vmid}/snapshot/${snapname}`, { method: 'DELETE' }),
    deleteSnapshotPlan: (hostId: string, vmid: number, snapname: string) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(
        `/api/proxmox/${hostId}/vms/${vmid}/snapshot/${snapname}`, { method: 'DELETE', body: JSON.stringify({ dry_run: true }) }),
    rollbackSnapshot: (hostId: string, vmid: number, snapname: string) =>
      requestJob(`/api/proxmox/${hostId}/vms/${vmid}/rollback/${snapname}`, { method: 'POST' }),
    rollbackSnapshotPlan: (hostId: string, vmid: number, snapname: string) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(
        `/api/proxmox/${hostId}/vms/${vmid}/rollback/${snapname}`, { method: 'POST', body: JSON.stringify({ dry_run: true }) }),
    vncProxy: (hostId: string, vmid: number) =>
      request<{ ticket: string; port: number; proxmox_host: string; node: string; kind: string; vmid: number }>(
        `/api/proxmox/${hostId}/vms/${vmid}/vncproxy`, { method: 'POST' }),
    getBackupJobs: (hostId: string) =>
      request<{ jobs: import('./types').PveBackupJob[]; archives: import('./types').PveBackupArchive[] }>(`/api/proxmox/${hostId}/backup-jobs`),
    diskPassthroughPlan: (hostId: string, vmid: number, diskPath: string, bus: string) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(
        `/api/proxmox/${hostId}/vms/${vmid}/disk-passthrough`, { method: 'POST', body: JSON.stringify({ disk_path: diskPath, bus, dry_run: true }) }),
    diskPassthrough: (hostId: string, vmid: number, diskPath: string, bus: string) =>
      requestJob(`/api/proxmox/${hostId}/vms/${vmid}/disk-passthrough`, { method: 'POST', body: JSON.stringify({ disk_path: diskPath, bus }) }),

    // Storage content browser
    getStorageContent: (hostId: string, node: string, storage: string) =>
      request<import('./types').PveStorageContent[]>(`/api/proxmox/${hostId}/nodes/${node}/storage/${storage}/content`),
    deleteStorageContentPlan: (hostId: string, node: string, storage: string, volid: string) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(
        `/api/proxmox/${hostId}/nodes/${node}/storage/${storage}/content?volid=${encodeURIComponent(volid)}`,
        { method: 'DELETE', body: JSON.stringify({ dry_run: true }) }),
    deleteStorageContent: (hostId: string, node: string, storage: string, volid: string) =>
      requestJob(
        `/api/proxmox/${hostId}/nodes/${node}/storage/${storage}/content?volid=${encodeURIComponent(volid)}`,
        { method: 'DELETE', body: JSON.stringify({ dry_run: false }) }),

    // Physical disks
    getDisks: (hostId: string, node: string) =>
      request<import('./types').PveDisk[]>(`/api/proxmox/${hostId}/nodes/${node}/disks`),
    getDiskSmart: (hostId: string, node: string, disk: string) =>
      request<Record<string, unknown>>(`/api/proxmox/${hostId}/nodes/${node}/disks/smart?disk=${encodeURIComponent(disk)}`),
    wipeDiskPlan: (hostId: string, node: string, disk: string) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(
        `/api/proxmox/${hostId}/nodes/${node}/disks/wipe`, { method: 'POST', body: JSON.stringify({ disk, dry_run: true }) }),
    wipeDisk: (hostId: string, node: string, disk: string) =>
      requestJob(
        `/api/proxmox/${hostId}/nodes/${node}/disks/wipe`, { method: 'POST', body: JSON.stringify({ disk }) }),
    initDiskPlan: (hostId: string, node: string, disk: string, fstype: string, name: string, raidlevel?: string) =>
      request<{ dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }>(
        `/api/proxmox/${hostId}/nodes/${node}/disks/init`, { method: 'POST', body: JSON.stringify({ disk, fstype, name, raidlevel, dry_run: true }) }),
    initDisk: (hostId: string, node: string, disk: string, fstype: string, name: string, raidlevel?: string) =>
      requestJob(
        `/api/proxmox/${hostId}/nodes/${node}/disks/init`, { method: 'POST', body: JSON.stringify({ disk, fstype, name, raidlevel }) }),
  },

  proxmoxDeploy: {
    deployToLxc: (hostId: string, req: {
      node: string; hostname: string; ostemplate: string; compose_yaml: string;
      cores?: number; memory?: number; storage?: string; disk_gb?: number;
    }) => requestJob(
      `/api/proxmox/${hostId}/lxc/deploy`, { method: 'POST', body: JSON.stringify(req) }),
  },

  plugins: {
    list:      () => request<import('./types').Plugin[]>('/api/plugins'),
    install:   (url: string) =>
      request<import('./types').Plugin>('/api/plugins', { method: 'POST', body: JSON.stringify({ url }) }),
    update:    (id: string, patch: { enabled?: boolean }) =>
      request<{ ok: boolean }>(`/api/plugins/${id}`, { method: 'PATCH', body: JSON.stringify(patch) }),
    uninstall: (id: string) =>
      request<{ ok: boolean }>(`/api/plugins/${id}`, { method: 'DELETE' }),
  },

  lxc: {
    list: () =>
      request<import('./types').LxcListResponse>('/api/lxc'),
    config: (vmid: number) =>
      request<import('./types').LxcConfig>(`/api/lxc/${vmid}/config`),
    action: (vmid: number, action: string) =>
      request<{ ok: boolean; message: string }>(`/api/lxc/${vmid}/action`, { method: 'POST', body: JSON.stringify({ action }) }),
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

  agents: {
    wsUrl: () => {
      const proto = location.protocol === 'https:' ? 'wss' : 'ws'
      return `${proto}://${location.host}/api/agents/ws`
    },
    list: () =>
      request<import('./types').AgentWithStatus[]>('/api/agents'),
    create: (req: import('./types').CreateAgentRequest) =>
      request<import('./types').AgentWithStatus>('/api/agents', { method: 'POST', body: JSON.stringify(req) }),
    update: (id: string, patch: import('./types').UpdateAgentRequest) =>
      request<{ ok: boolean }>(`/api/agents/${id}`, { method: 'PUT', body: JSON.stringify(patch) }),
    delete: (id: string) =>
      request<{ ok: boolean }>(`/api/agents/${id}`, { method: 'DELETE' }),
    getStatus: (id: string) =>
      request<import('./types').AgentStatusUpdate>(`/api/agents/${id}/status`),
    postStatus: (id: string, status: { state: string; activity?: string | null; task_id?: string | null }) =>
      request<{ ok: boolean }>(`/api/agents/${id}/status`, { method: 'POST', body: JSON.stringify(status) }),
    export: () =>
      request<import('./types').ExportedAgent[]>('/api/agents/export'),
    import: (agents: import('./types').ExportedAgent[]) =>
      request<{ ok: boolean; imported: number }>('/api/agents/import', { method: 'POST', body: JSON.stringify({ agents }) }),
  },

  tabs: {
    list: () =>
      request<import('./types').CustomTab[]>('/api/tabs'),
    create: (req: import('./types').CreateCustomTabRequest) =>
      request<import('./types').CustomTab>('/api/tabs', { method: 'POST', body: JSON.stringify(req) }),
    update: (id: string, patch: import('./types').UpdateCustomTabRequest) =>
      request<{ ok: boolean }>(`/api/tabs/${id}`, { method: 'PUT', body: JSON.stringify(patch) }),
    delete: (id: string) =>
      request<{ ok: boolean }>(`/api/tabs/${id}`, { method: 'DELETE' }),
    reorder: (ids: string[]) =>
      request<{ ok: boolean }>('/api/tabs/order', { method: 'PUT', body: JSON.stringify({ ids }) }),
    export: () =>
      request<import('./types').ExportedTab[]>('/api/tabs/export'),
    import: (tabs: import('./types').ExportedTab[]) =>
      request<{ ok: boolean; imported: number }>('/api/tabs/import', { method: 'POST', body: JSON.stringify({ tabs }) }),
  },

  updates: {
    infoVt: () =>
      request<import('./types').VoidTowerUpdateInfo>('/api/updates/voidtower'),
    checkVt: () =>
      requestJob('/api/updates/voidtower/check', { method: 'POST' }),
    applyVt: (dryRun: boolean) =>
      request<
        | { dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }
        | import('./types').DurableJobResponse
      >('/api/updates/voidtower/apply', { method: 'POST', body: JSON.stringify({ dry_run: dryRun }) }, parsePlanOrJobResponse),
    rollbackVt: (tag: string, dryRun: boolean) =>
      request<
        | { dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }
        | import('./types').DurableJobResponse
      >('/api/updates/voidtower/rollback', { method: 'POST', body: JSON.stringify({ tag, dry_run: dryRun }) }, parsePlanOrJobResponse),
    infoOdysseus: () =>
      request<import('./types').OdysseusUpdateInfo>('/api/updates/odysseus'),
    applyOdysseus: () =>
      requestJob('/api/updates/odysseus/apply', { method: 'POST' }),
    infoDocker: () =>
      request<import('./types').DockerUpdateRow[]>('/api/updates/docker'),
    checkDocker: () =>
      requestJob('/api/updates/docker/check', { method: 'POST' }),
    dockerApply: (id: string, dryRun: boolean) =>
      request<
        | { dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }
        | import('./types').DurableJobResponse
      >(`/api/updates/docker/${id}/apply`, { method: 'POST', body: JSON.stringify({ dry_run: dryRun }) }, parsePlanOrJobResponse),
    infoOs: () =>
      request<import('./types').OsUpdateInfo>('/api/updates/os'),
    applyOs: (dryRun: boolean) =>
      request<
        | { dry_run: true; plan: import('../components/ui/ChangePlanModal').ChangePlan }
        | import('./types').DurableJobResponse
      >('/api/updates/os/apply', { method: 'POST', body: JSON.stringify({ dry_run: dryRun }) }, parsePlanOrJobResponse),
  },

  operationJobs: {
    list: (limit = 50) =>
      request<import('./types').DurableJobListResponse>(
        `/api/jobs?limit=${encodeURIComponent(limit)}`,
        undefined,
        parseJobListEnvelope<import('./types').DurableJob>,
      ),
    get: (id: string) =>
      request<import('./types').DurableJobResponse>(`/api/jobs/${encodeURIComponent(id)}`, undefined, parseJobReadEnvelope),
    getByIdempotency: (key: string) =>
      (() => {
        if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(key)) {
          return Promise.reject(new ApiClientError('Invalid idempotency key.', 'invalid_idempotency_key', 400))
        }
        return request<import('./types').DurableJobResponse>(
          `/api/jobs/by-idempotency/${encodeURIComponent(key)}`,
          undefined,
          parseJobReadEnvelope,
        )
      })(),
    cancel: (id: string) =>
      requestJob(`/api/jobs/${encodeURIComponent(id)}/cancel`, { method: 'POST' }),
  },

  canonicalActions: {
    plan: async (resourceId: string, action: string, input: unknown = {}) => {
      validateCanonicalTarget(resourceId)
      validateCanonicalTarget(action)
      return request(`/api/resources/${encodeURIComponent(resourceId)}/actions/${encodeURIComponent(action)}/plan`, {
        method: 'POST',
        body: JSON.stringify({ input }),
      }, parsePlanSuccessEnvelope)
    },
    submit: async (resourceId: string, action: string, input: unknown, idempotencyKey: string) => {
      validateCanonicalTarget(resourceId)
      validateCanonicalTarget(action)
      if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(idempotencyKey)) {
        throw new ApiClientError('Invalid idempotency key.', 'invalid_idempotency_key', 400)
      }
      return request(`/api/resources/${encodeURIComponent(resourceId)}/actions/${encodeURIComponent(action)}`, {
        method: 'POST',
        headers: { 'Idempotency-Key': idempotencyKey },
        body: JSON.stringify({ input }),
      }, parseJobSuccessEnvelope)
    },
  },

  approvals: {
    list: (params: { status?: import('./types').DurableApprovalStatus; limit?: number } = {}) => {
      const query = new URLSearchParams()
      if (params.status) query.set('status', params.status)
      query.set('limit', String(params.limit ?? 50))
      return request<import('./types').DurableApprovalListResponse>(
        `/api/approvals?${query}`,
        undefined,
        parseApprovalListEnvelope<import('./types').DurableApproval>,
      )
    },
    get: (id: string) =>
      request<import('./types').DurableApprovalResponse>(
        `/api/approvals/${encodeURIComponent(id)}`,
        undefined,
        parseApprovalReadEnvelope<import('./types').DurableApproval>,
      ),
    approve: (id: string, comment?: string) =>
      requestJob(`/api/approvals/${encodeURIComponent(id)}/approve`, {
        method: 'POST', body: JSON.stringify({ comment: comment?.trim() || null }),
      }),
    reject: (id: string, comment?: string) =>
      requestJob(`/api/approvals/${encodeURIComponent(id)}/reject`, {
        method: 'POST', body: JSON.stringify({ comment: comment?.trim() || null }),
      }),
  },

  systemUpdate: {
    version: () => request<import('./types').SystemVersionInfo>('/api/system/version'),
    check: () => requestJob('/api/system/update-check'),
    apply: () => requestJob('/api/system/update', { method: 'POST' }),
  },
}

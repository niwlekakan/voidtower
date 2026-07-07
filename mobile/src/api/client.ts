// Ported from frontend/src/api/client.ts. Same endpoint paths/response shapes
// as the web client, but auth relies on RN's native cookie jar (see
// src/auth/AuthContext.tsx) instead of a browser's `credentials: 'include'` —
// RN's fetch (backed by NSURLSession/OkHttp) persists cookies across requests
// within the app automatically, so no bearer-token minting dance is needed.
import type {
  Alert, AlertsResponse, ApiError, ContainersResponse, DeployedResponse,
  EnrollResponse, MetricsSnapshot, NodeRecord, PairingCodeResponse,
  ServicesResponse, User, WireguardResponse,
} from './types'

let baseUrl = ''

export function setBaseUrl(url: string) {
  baseUrl = url.replace(/\/+$/, '')
}
export function getBaseUrl() {
  return baseUrl
}

export class ApiClientError extends Error {
  constructor(message: string, public readonly code: string, public readonly status: number) {
    super(message)
    this.name = 'ApiClientError'
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  if (!baseUrl) {
    throw new ApiClientError('No server configured', 'no_server', 0)
  }
  const res = await fetch(`${baseUrl}${path}`, {
    ...init,
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
      request<{ user: User }>('/api/auth/login', {
        method: 'POST',
        body: JSON.stringify({ username, password, totp_code }),
      }),
    logout: () => request('/api/auth/logout', { method: 'POST' }),
    me: () => request<{ user: User }>('/api/auth/me'),
  },

  users: {
    changePassword: (password: string, username?: string) =>
      request<{ ok: boolean }>('/api/users/me/password', {
        method: 'POST',
        body: JSON.stringify({ password, username }),
      }),
  },

  metrics: {
    current: () => request<MetricsSnapshot>('/api/metrics/current'),
  },

  services: {
    list: () => request<ServicesResponse>('/api/services'),
  },

  containers: {
    list: () => request<ContainersResponse>('/api/containers'),
  },

  apps: {
    deployed: () => request<DeployedResponse>('/api/apps/deployed'),
  },

  alerts: {
    list: (state = 'active') => request<AlertsResponse>(`/api/alerts?state=${state}`),
    acknowledge: (id: string) => request<{ ok: boolean }>(`/api/alerts/${id}/acknowledge`, { method: 'POST' }),
  },

  wireguard: {
    list: () => request<WireguardResponse>('/api/wireguard'),
  },

  aiProviders: {
    list: () => request<{ id: string; name: string; enabled: boolean }[]>('/api/ai/providers'),
  },

  // Part B — pairing-code enrollment (backend/src/api/node_enroll.rs).
  nodes: {
    list: () => request<{ nodes: NodeRecord[] }>('/api/nodes'),
    mintPairingCode: () => request<PairingCodeResponse>('/api/nodes/pairing-code', { method: 'POST' }),
    enroll: (body: { pairing_code: string; display_name: string; device_type: string; agent_capable?: boolean }) =>
      request<EnrollResponse>('/api/nodes/enroll', { method: 'POST', body: JSON.stringify(body) }),
    remove: (id: string) => request<{ ok: boolean }>(`/api/nodes/${id}`, { method: 'DELETE' }),
    heartbeat: (id: string, token: string, body: { battery?: number; storage_free_bytes?: number; online?: boolean }) =>
      request<{ ok: boolean }>(`/api/nodes/${id}/heartbeat`, {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}` },
        body: JSON.stringify(body),
      }),
  },

  // Read-only summary MVP for the remaining Tower module cards — each just
  // proxies its existing GET list endpoint. Module Drawer renders whatever
  // shape comes back generically (see ModuleDrawer.tsx), no per-module types.
  moduleSummary: (path: string) => request<Record<string, unknown>>(path),
}

/** GET path used for each Tower module's read-only drawer. */
export const MODULE_ENDPOINTS: Record<string, string> = {
  services: '/api/services',
  containers: '/api/containers',
  appvault: '/api/apps/deployed',
  vms: '/api/vms/local',
  files: '/api/files/roots',
  terminal: '/api/terminal/ssh/sessions',
  backups: '/api/backups',
  network: '/api/network/neighbors',
  storage: '/api/storage/devices',
  firewall: '/api/firewall',
  wireguard: '/api/wireguard',
  proxies: '/api/proxy',
  automation: '/api/automation',
  secrets: '/api/secrets',
  timeline: '/api/timeline',
  models: '/api/models',
  diagnostics: '/api/diagnostics',
  security: '/api/security/sessions',
  updates: '/api/updates/voidtower',
}

export type { Alert }

import { useState, useEffect, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { Globe, Trash2, Power, Plus, Lock, LockOpen, ExternalLink, CheckCircle, XCircle, ChevronDown, ChevronRight, AlertTriangle, Copy, Terminal } from 'lucide-react'
import MiniTerminal from '@/components/ui/MiniTerminal'
import { api, ApiClientError } from '@/api/client'
import { notify } from '@/store/notifications'
import type { ProxyConfig } from '@/api/types'
import Button from '@/components/ui/Button'

function fmt(ts: number) {
  return new Date(ts * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
}

// ─── Proxy backend info cards ─────────────────────────────────────────────────

interface BackendCardProps {
  name: string
  description: string
  badge: 'managed' | 'manual' | 'container'
  badgeColor: string
  children: React.ReactNode
}

function BackendCard({ name, description, badge, badgeColor, children }: BackendCardProps) {
  const [open, setOpen] = useState(false)
  return (
    <div className="rounded overflow-hidden" style={{ border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)' }}>
      <button
        className="w-full flex items-center justify-between px-4 py-3 text-left hover:opacity-80 transition-opacity"
        onClick={() => setOpen(v => !v)}
      >
        <div className="flex items-center gap-3">
          <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{name}</span>
          <span className="px-1.5 py-0.5 rounded text-xs font-medium" style={{ background: badgeColor + '22', color: badgeColor }}>
            {badge}
          </span>
          <span className="text-xs hidden sm:block" style={{ color: 'var(--text-muted)' }}>{description}</span>
        </div>
        {open ? <ChevronDown size={14} style={{ color: 'var(--text-muted)', flexShrink: 0 }} /> : <ChevronRight size={14} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />}
      </button>
      {open && (
        <div className="px-4 pb-4" style={{ borderTop: '1px solid var(--border-subtle)' }}>
          {children}
        </div>
      )}
    </div>
  )
}

function CodeBlock({ children }: { children: string }) {
  return (
    <pre className="text-xs p-3 rounded mt-2 overflow-x-auto font-mono"
         style={{ background: 'var(--bg-base)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
      {children}
    </pre>
  )
}

interface SetupStep { label: string; cmd: string | null; app_id?: string }
interface SetupStatus {
  ready: boolean
  mode: 'docker' | 'system' | 'none'
  shell: 'fish' | 'bash'
  checks: { conf_d_exists: boolean; conf_d_writable: boolean; has_include: boolean; can_reload: boolean }
  steps: SetupStep[]
  combined_cmd: string | null
}

function CopyableCmd({ cmd }: { cmd: string }) {
  const navigate = useNavigate()
  const [copied, setCopied] = useState(false)
  const copy = () => {
    navigator.clipboard.writeText(cmd)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }
  return (
    <div className="relative mt-2">
      <pre className="text-xs p-3 pr-20 rounded font-mono overflow-x-auto whitespace-pre-wrap"
           style={{ background: 'var(--bg-base)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
        {cmd}
      </pre>
      <div className="absolute top-2 right-2 flex items-center gap-1">
        <button onClick={copy}
          className="flex items-center gap-1 px-2 py-1 rounded text-xs hover:opacity-80"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: copied ? 'var(--accent-success)' : 'var(--text-muted)' }}>
          <Copy size={10} />{copied ? 'Copied' : 'Copy'}
        </button>
        <button onClick={() => navigate('/terminal')} title="Open terminal"
          className="flex items-center gap-1 px-2 py-1 rounded text-xs hover:opacity-80"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--accent-primary)' }}>
          <Terminal size={10} />
        </button>
      </div>
    </div>
  )
}

interface NginxStatus {
  active: boolean
  state: string
  pid: number | null
}

function NginxBackendCard({ available, nginxBackend, onInstalled }: {
  available: boolean
  nginxBackend: 'docker' | 'system' | 'none'
  onInstalled: () => void
}) {
  const [setup, setSetup] = useState<SetupStatus | null>(null)
  const [loadingSetup, setLoadingSetup] = useState(false)
  const [installCmd, setInstallCmd] = useState<string | null>(null)
  const [nginxStatus, setNginxStatus] = useState<NginxStatus | null>(null)
  const [actionLoading, setActionLoading] = useState<string | null>(null)
  const [testOutput, setTestOutput] = useState<string | null>(null)
  const [showLogs, setShowLogs] = useState(false)
  const [logLines, setLogLines] = useState<string[]>([])
  const [logPath, setLogPath] = useState('')
  const [loadingLogs, setLoadingLogs] = useState(false)
  const [showTerminal, setShowTerminal] = useState(false)

  const fetchSetup = useCallback(async () => {
    setLoadingSetup(true)
    try {
      const [setupRes, installRes] = await Promise.all([
        fetch('/api/proxy/nginx-setup', { credentials: 'include' }).then(r => r.json()),
        fetch('/api/proxy/nginx-install-cmd', { credentials: 'include' }).then(r => r.ok ? r.json() : null),
      ])
      setSetup(setupRes)
      if (installRes) setInstallCmd(installRes.cmd)
    } catch {
      setSetup(null)
    } finally {
      setLoadingSetup(false)
    }
  }, [])

  const fetchStatus = useCallback(async () => {
    try {
      const r = await fetch('/api/proxy/nginx/status', { credentials: 'include' }).then(r => r.json())
      setNginxStatus(r)
    } catch { /* ignore */ }
  }, [])

  useEffect(() => { fetchSetup() }, [fetchSetup])
  useEffect(() => {
    if (available && setup?.ready) fetchStatus()
  }, [available, setup, fetchStatus])

  const handleAction = async (action: string) => {
    setActionLoading(action)
    if (action !== 'test') setTestOutput(null)
    try {
      const r = await fetch('/api/proxy/nginx/action', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ action }),
      }).then(r => r.json())

      if (action === 'test') {
        setTestOutput(r.output ?? (r.ok ? 'Test passed' : 'Test failed'))
      } else {
        notify[r.ok ? 'success' : 'error'](r.message ?? (r.ok ? `nginx ${action} succeeded` : `nginx ${action} failed`))
        await fetchStatus()
      }
    } catch {
      notify.error(`Failed to ${action} nginx`)
    } finally {
      setActionLoading(null)
    }
  }

  const handleViewLogs = async () => {
    if (showLogs) { setShowLogs(false); return }
    setLoadingLogs(true)
    setShowLogs(true)
    try {
      const r = await fetch('/api/proxy/nginx/logs', { credentials: 'include' }).then(r => r.json())
      setLogLines(r.lines ?? [])
      setLogPath(r.path ?? '')
    } catch {
      setLogLines(['Failed to fetch logs'])
    } finally {
      setLoadingLogs(false)
    }
  }

  const needsSetup = available && setup && !setup.ready
  const borderColor = !available ? 'var(--border-default)'
    : needsSetup ? 'var(--accent-warning)'
    : 'var(--accent-success)'

  const statusText = !available ? 'Not detected'
    : needsSetup ? 'Needs setup'
    : 'Ready'

  const statusColor = !available ? 'var(--accent-danger)'
    : needsSetup ? 'var(--accent-warning)'
    : 'var(--accent-success)'

  return (
    <div className="rounded overflow-hidden" style={{ border: `1px solid ${borderColor}`, background: 'var(--bg-elevated)' }}>
      <div className="flex items-center justify-between px-4 py-3">
        <div className="flex items-center gap-3">
          {available && !needsSetup
            ? <CheckCircle size={15} style={{ color: 'var(--accent-success)', flexShrink: 0 }} />
            : <XCircle size={15} style={{ color: statusColor, flexShrink: 0 }} />}
          <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
            {nginxBackend === 'docker' ? 'nginx-proxy' : 'nginx'}
          </span>
          <span className="px-1.5 py-0.5 rounded text-xs font-medium"
                style={{ background: 'var(--accent-primary)22', color: 'var(--accent-primary)' }}>
            {nginxBackend === 'docker' ? 'Docker · managed' : 'fully managed'}
          </span>
          <span className="text-xs hidden sm:block" style={{ color: statusColor }}>
            {statusText}
          </span>
        </div>
        <button onClick={() => { onInstalled(); fetchSetup() }}
                className="text-xs hover:opacity-70 transition-opacity"
                style={{ color: 'var(--text-muted)' }}>
          Refresh
        </button>
      </div>

      {loadingSetup && (
        <div className="px-4 pb-3 text-xs" style={{ color: 'var(--text-muted)', borderTop: '1px solid var(--border-subtle)' }}>
          Checking setup…
        </div>
      )}

      {/* Not available — Docker: prompt to deploy from App Vault; System: show install cmd */}
      {!available && !loadingSetup && (
        <div className="px-4 pb-4" style={{ borderTop: '1px solid var(--border-subtle)' }}>
          {nginxBackend !== 'system' ? (
            <>
              <p className="text-xs mt-3 mb-2" style={{ color: 'var(--text-secondary)' }}>
                Deploy the <strong>Nginx Proxy</strong> app from the App Vault to enable Docker-managed reverse proxying:
              </p>
              <a href="/apps"
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80"
                style={{ background: 'var(--accent-primary)', color: '#fff' }}>
                Open App Vault →
              </a>
              {installCmd && (
                <>
                  <p className="text-xs mt-4 mb-1" style={{ color: 'var(--text-muted)' }}>
                    Or install system nginx as fallback:
                  </p>
                  <CopyableCmd cmd={installCmd} />
                </>
              )}
            </>
          ) : (
            <>
              <p className="text-xs mt-3 mb-1" style={{ color: 'var(--text-secondary)' }}>
                Install nginx, then click Refresh:
              </p>
              {installCmd
                ? <CopyableCmd cmd={installCmd} />
                : <CopyableCmd cmd="sudo pacman -S --noconfirm nginx && sudo systemctl enable --now nginx" />}
            </>
          )}
        </div>
      )}

      {/* nginx installed but setup incomplete */}
      {available && !loadingSetup && setup && !setup.ready && (
        <div className="px-4 pb-4" style={{ borderTop: '1px solid var(--border-subtle)' }}>
          <div className="flex items-start gap-2 mt-3 mb-3 p-2 rounded text-xs"
               style={{ background: 'var(--accent-warning-subtle)', color: 'var(--accent-warning)', border: '1px solid var(--accent-warning)' }}>
            <AlertTriangle size={13} style={{ flexShrink: 0, marginTop: 1 }} />
            nginx is running but VoidTower can't manage it yet. Run the commands below once to complete setup.
          </div>

          <div className="grid grid-cols-2 gap-1.5 mb-3">
            {[
              { label: 'conf.d directory', ok: setup.checks.conf_d_exists },
              { label: 'nginx.conf include', ok: setup.checks.has_include },
              { label: 'Write access', ok: setup.checks.conf_d_writable },
              { label: 'Can reload nginx', ok: setup.checks.can_reload },
            ].map(c => (
              <div key={c.label} className="flex items-center gap-1.5 text-xs">
                {c.ok
                  ? <CheckCircle size={11} style={{ color: 'var(--accent-success)', flexShrink: 0 }} />
                  : <XCircle size={11} style={{ color: 'var(--accent-danger)', flexShrink: 0 }} />}
                <span style={{ color: c.ok ? 'var(--text-secondary)' : 'var(--text-primary)' }}>{c.label}</span>
              </div>
            ))}
          </div>

          <p className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>
            Run all at once ({setup.shell ?? 'bash'}), then click Refresh:
          </p>
          {setup.combined_cmd && <CopyableCmd cmd={setup.combined_cmd} />}
          <button
            onClick={() => setShowTerminal(v => !v)}
            className="mt-2 flex items-center gap-1.5 text-xs px-2.5 py-1.5 rounded hover:opacity-80 transition-opacity"
            style={{ background: 'var(--bg-base)', border: '1px solid var(--border-subtle)', color: 'var(--accent-primary)' }}>
            <Terminal size={11} />{showTerminal ? 'Hide terminal' : 'Run in terminal'}
          </button>
          {showTerminal && (
            <div className="mt-2 rounded overflow-hidden" style={{ border: '1px solid var(--border-subtle)' }}>
              <MiniTerminal height={220} initialCommand={setup.combined_cmd ?? undefined} />
            </div>
          )}
        </div>
      )}

      {/* nginx ready — management panel */}
      {available && !loadingSetup && setup?.ready && (
        <div className="px-4 pb-4" style={{ borderTop: '1px solid var(--border-subtle)' }}>
          {/* Status row */}
          <div className="flex items-center gap-3 mt-3 mb-3">
            <span style={{
              width: 8, height: 8, borderRadius: '50%', flexShrink: 0,
              background: nginxStatus?.active ? 'var(--accent-success)' : 'var(--accent-danger)',
              boxShadow: nginxStatus?.active ? '0 0 6px var(--accent-success)' : undefined,
              display: 'inline-block',
            }} />
            <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              {nginxStatus ? nginxStatus.state : 'checking…'}
            </span>
            {nginxStatus?.pid ? (
              <span className="text-xs px-1.5 py-0.5 rounded font-mono"
                    style={{ background: 'var(--bg-base)', color: 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}>
                PID {nginxStatus.pid}
              </span>
            ) : nginxBackend === 'docker' ? (
              <span className="text-xs px-1.5 py-0.5 rounded font-mono"
                    style={{ background: 'var(--bg-base)', color: 'var(--accent-primary)', border: '1px solid var(--border-subtle)' }}>
                Docker container
              </span>
            ) : null}
          </div>

          {/* Action buttons */}
          <div className="flex flex-wrap gap-2 mb-3">
            {(['reload', 'restart'] as const).map(a => (
              <button key={a}
                className="px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
                style={{ background: 'var(--bg-base)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
                disabled={actionLoading !== null}
                onClick={() => handleAction(a)}>
                {actionLoading === a ? '…' : a.charAt(0).toUpperCase() + a.slice(1)}
              </button>
            ))}
            <button
              className="px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
              style={{
                background: 'var(--bg-base)', border: '1px solid var(--border-subtle)',
                color: nginxStatus?.active ? 'var(--accent-danger)' : 'var(--accent-success)',
              }}
              disabled={actionLoading !== null}
              onClick={() => handleAction(nginxStatus?.active ? 'stop' : 'start')}>
              {actionLoading === 'stop' || actionLoading === 'start'
                ? '…'
                : nginxStatus?.active ? 'Stop' : 'Start'}
            </button>
            <button
              className="px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
              style={{ background: 'var(--bg-base)', border: '1px solid var(--border-subtle)', color: 'var(--accent-secondary)' }}
              disabled={actionLoading !== null}
              onClick={() => handleAction('test')}>
              {actionLoading === 'test' ? '…' : 'Test Config'}
            </button>
            <button
              className="px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80"
              style={{ background: 'var(--bg-base)', border: '1px solid var(--border-subtle)', color: 'var(--text-muted)' }}
              onClick={handleViewLogs}>
              {showLogs ? 'Hide logs' : 'View logs'}
            </button>
          </div>

          {/* Test config output */}
          {testOutput !== null && (
            <pre className="text-xs p-3 rounded font-mono overflow-x-auto whitespace-pre-wrap mb-3"
                 style={{ background: 'var(--bg-base)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)', maxHeight: 160 }}>
              {testOutput}
            </pre>
          )}

          {/* Log viewer */}
          {showLogs && (
            <div>
              {logPath && (
                <p className="text-xs mb-1 font-mono" style={{ color: 'var(--text-muted)' }}>{logPath} (last 100 lines)</p>
              )}
              {loadingLogs
                ? <p className="text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</p>
                : (
                  <pre className="text-xs p-3 rounded font-mono overflow-auto whitespace-pre-wrap"
                       style={{ background: 'var(--bg-base)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)', maxHeight: 260 }}>
                    {logLines.join('\n') || 'No log entries'}
                  </pre>
                )}
            </div>
          )}
        </div>
      )}
    </div>
  )
}

// ─── Main page ────────────────────────────────────────────────────────────────

export default function ProxiesPage() {
  const [proxies, setProxies] = useState<ProxyConfig[]>([])
  const [nginxAvailable, setNginxAvailable] = useState(false)
  const [nginxBackend, setNginxBackend] = useState<'docker' | 'system' | 'none'>('none')
  const [sitesDir, setSitesDir] = useState('')
  const [loading, setLoading] = useState(true)
  const [showForm, setShowForm] = useState(false)

  const [domain, setDomain] = useState('')
  const [upstream, setUpstream] = useState('http://localhost:')
  const [ssl, setSsl] = useState(false)
  const [allowEmbed, setAllowEmbed] = useState(true)
  const [creating, setCreating] = useState(false)
  const [toggling, setToggling] = useState<string | null>(null)

  const refresh = useCallback(() => {
    setLoading(true)
    api.proxy.list()
      .then((r) => {
        setProxies(r.proxies)
        setNginxAvailable(r.nginx_available)
        setNginxBackend(r.nginx_backend ?? (r.nginx_available ? 'system' : 'none'))
        setSitesDir(r.sites_dir)
      })
      .catch(() => notify.error('Failed to load proxy configs'))
      .finally(() => setLoading(false))
  }, [])

  useEffect(() => { refresh() }, [refresh])

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault()
    setCreating(true)
    try {
      const r = await api.proxy.create(domain.trim(), upstream.trim(), ssl, allowEmbed)
      notify.success(`Proxy created — ${r.nginx}`)
      setDomain(''); setUpstream('http://localhost:'); setSsl(false); setAllowEmbed(true)
      setShowForm(false)
      refresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to create proxy')
    } finally {
      setCreating(false)
    }
  }

  const handleDelete = async (p: ProxyConfig) => {
    if (!confirm(`Remove proxy for "${p.domain}"? The nginx config will be deleted and nginx reloaded.`)) return
    try {
      const r = await api.proxy.delete(p.id)
      notify.success(`Removed — ${r.nginx}`)
      refresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to remove')
    }
  }

  const handleToggle = async (p: ProxyConfig) => {
    setToggling(p.id)
    try {
      const r = await api.proxy.toggle(p.id)
      notify.success(r.enabled ? 'Proxy enabled' : 'Proxy disabled')
      refresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Toggle failed')
    } finally {
      setToggling(null)
    }
  }

  return (
    <div className="space-y-5">
      <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Reverse Proxies</h1>

      {/* Proxy backend selection */}
      <div className="space-y-2">
        <p className="text-xs uppercase tracking-wider font-medium" style={{ color: 'var(--text-muted)' }}>
          Proxy backends
        </p>

        <NginxBackendCard available={nginxAvailable} nginxBackend={nginxBackend} onInstalled={refresh} />

        <BackendCard
          name="Caddy"
          description="Automatic HTTPS, simple Caddyfile syntax"
          badge="manual"
          badgeColor="var(--accent-secondary)"
        >
          <p className="text-xs mt-3 mb-1" style={{ color: 'var(--text-secondary)' }}>Install Caddy:</p>
          <CodeBlock>{`# Debian / Ubuntu
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https curl
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update && sudo apt install caddy

# Arch Linux
sudo pacman -S caddy

# Enable
sudo systemctl enable --now caddy`}</CodeBlock>
          <p className="text-xs mt-3 mb-1" style={{ color: 'var(--text-secondary)' }}>Example Caddyfile (<code className="font-mono">/etc/caddy/Caddyfile</code>):</p>
          <CodeBlock>{`app.example.com {
    reverse_proxy localhost:8080
}

# With automatic HTTPS (Caddy handles Let's Encrypt automatically)
secure.example.com {
    reverse_proxy localhost:9000
    header {
        X-Frame-Options ALLOWALL
        Content-Security-Policy "frame-ancestors *"
    }
}`}</CodeBlock>
          <div className="flex items-start gap-2 mt-3 p-2 rounded text-xs"
               style={{ background: 'var(--accent-warning-subtle)', color: 'var(--accent-warning)', border: '1px solid var(--accent-warning)' }}>
            <AlertTriangle size={13} style={{ flexShrink: 0, marginTop: 1 }} />
            Caddy is configured manually via Caddyfile. VoidTower cannot manage Caddy proxy rules directly — use the File Manager to edit <code className="font-mono">/etc/caddy/Caddyfile</code>.
          </div>
        </BackendCard>

        <BackendCard
          name="Traefik"
          description="Cloud-native, Docker-label driven, dashboard included"
          badge="container"
          badgeColor="var(--accent-primary)"
        >
          <p className="text-xs mt-3 mb-1" style={{ color: 'var(--text-secondary)' }}>Run Traefik via Docker Compose:</p>
          <CodeBlock>{`version: "3.8"
services:
  traefik:
    image: traefik:v3
    command:
      - --api.dashboard=true
      - --providers.docker=true
      - --providers.docker.exposedbydefault=false
      - --entrypoints.web.address=:80
      - --entrypoints.websecure.address=:443
      - --certificatesresolvers.le.acme.tlschallenge=true
      - --certificatesresolvers.le.acme.email=you@example.com
      - --certificatesresolvers.le.acme.storage=/letsencrypt/acme.json
    ports:
      - "80:80"
      - "443:443"
      - "8080:8080"   # Dashboard
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ./letsencrypt:/letsencrypt`}</CodeBlock>
          <p className="text-xs mt-3 mb-1" style={{ color: 'var(--text-secondary)' }}>Expose a service with labels:</p>
          <CodeBlock>{`services:
  myapp:
    image: myapp:latest
    labels:
      - traefik.enable=true
      - traefik.http.routers.myapp.rule=Host(\`app.example.com\`)
      - traefik.http.routers.myapp.entrypoints=websecure
      - traefik.http.routers.myapp.tls.certresolver=le
      - traefik.http.services.myapp.loadbalancer.server.port=3000`}</CodeBlock>
          <div className="flex items-start gap-2 mt-3 p-2 rounded text-xs"
               style={{ background: 'var(--accent-warning-subtle)', color: 'var(--accent-warning)', border: '1px solid var(--accent-warning)' }}>
            <AlertTriangle size={13} style={{ flexShrink: 0, marginTop: 1 }} />
            Traefik is configured via Docker labels and dynamic config files. VoidTower cannot manage Traefik routes directly.
          </div>
        </BackendCard>

        <BackendCard
          name="HAProxy"
          description="High-performance TCP/HTTP load balancer"
          badge="manual"
          badgeColor="var(--accent-secondary)"
        >
          <p className="text-xs mt-3 mb-1" style={{ color: 'var(--text-secondary)' }}>Install HAProxy:</p>
          <CodeBlock>{`# Debian / Ubuntu
sudo apt install haproxy

# Arch Linux
sudo pacman -S haproxy

# Fedora / RHEL
sudo dnf install haproxy

sudo systemctl enable --now haproxy`}</CodeBlock>
          <p className="text-xs mt-3 mb-1" style={{ color: 'var(--text-secondary)' }}>Example <code className="font-mono">/etc/haproxy/haproxy.cfg</code>:</p>
          <CodeBlock>{`frontend http_front
    bind *:80
    default_backend servers

frontend https_front
    bind *:443 ssl crt /etc/ssl/certs/example.pem
    default_backend servers

backend servers
    balance roundrobin
    server app1 127.0.0.1:8080 check
    server app2 127.0.0.1:8081 check`}</CodeBlock>
          <div className="flex items-start gap-2 mt-3 p-2 rounded text-xs"
               style={{ background: 'var(--accent-warning-subtle)', color: 'var(--accent-warning)', border: '1px solid var(--accent-warning)' }}>
            <AlertTriangle size={13} style={{ flexShrink: 0, marginTop: 1 }} />
            HAProxy is configured manually. Use the File Manager to edit <code className="font-mono">/etc/haproxy/haproxy.cfg</code>.
          </div>
        </BackendCard>
      </div>

      {/* nginx proxy rules — only shown when nginx is available */}
      {nginxAvailable && (
        <>
          <div className="flex items-center justify-between">
            <p className="text-xs uppercase tracking-wider font-medium" style={{ color: 'var(--text-muted)' }}>
              {nginxBackend === 'docker' ? 'nginx-proxy rules' : 'nginx proxy rules'}
            </p>
            <Button size="sm" variant="primary" onClick={() => setShowForm((v) => !v)}>
              <Plus size={13} className="mr-1" /> Add proxy
            </Button>
          </div>

          {showForm && (
            <form onSubmit={handleCreate} className="card space-y-4">
              <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>New proxy rule</h2>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Domain</label>
                  <input
                    value={domain}
                    onChange={(e) => setDomain(e.target.value)}
                    placeholder="app.example.com"
                    className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
                    style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
                    required
                  />
                </div>
                <div>
                  <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Upstream</label>
                  <input
                    value={upstream}
                    onChange={(e) => setUpstream(e.target.value)}
                    placeholder="http://localhost:8080"
                    className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
                    style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
                    required
                  />
                </div>
              </div>
              <label className="flex items-center gap-2.5 cursor-pointer">
                <input type="checkbox" checked={ssl} onChange={(e) => setSsl(e.target.checked)} className="w-3.5 h-3.5 rounded" />
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  SSL / HTTPS — requires cert at <code className="font-mono">/etc/letsencrypt/live/{domain || 'domain'}/</code>
                </span>
              </label>
              <label className="flex items-center gap-2.5 cursor-pointer">
                <input type="checkbox" checked={allowEmbed} onChange={(e) => setAllowEmbed(e.target.checked)} className="w-3.5 h-3.5 rounded" />
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  Allow iframe embedding — strips <code className="font-mono">X-Frame-Options</code> and adds <code className="font-mono">frame-ancestors *</code>
                </span>
              </label>
              <div className="flex gap-2">
                <Button size="sm" variant="primary" type="submit" loading={creating}>Create &amp; reload nginx</Button>
                <Button size="sm" variant="ghost" type="button" onClick={() => setShowForm(false)}>Cancel</Button>
              </div>
            </form>
          )}

          <div className="panel overflow-hidden">
            {loading ? (
              <p className="px-4 py-6 text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</p>
            ) : proxies.length === 0 ? (
              <div className="flex flex-col items-center gap-3 py-12">
                <Globe size={32} style={{ color: 'var(--text-muted)', opacity: 0.4 }} />
                <p className="text-sm" style={{ color: 'var(--text-muted)' }}>No proxy rules configured.</p>
                <Button size="sm" variant="ghost" onClick={() => setShowForm(true)}>
                  <Plus size={12} className="mr-1" /> Add your first proxy
                </Button>
              </div>
            ) : (
              <div className="overflow-x-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    {['Domain', 'Upstream', 'SSL', 'Embed', 'Added', 'Status', ''].map((h) => (
                      <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider"
                          style={{ color: 'var(--text-muted)' }}>{h}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {proxies.map((p) => (
                    <tr key={p.id} style={{ borderBottom: '1px solid var(--border-subtle)', opacity: p.enabled ? 1 : 0.5 }}>
                      <td className="px-4 py-3">
                        <div className="flex items-center gap-1.5">
                          <span className="font-mono font-medium" style={{ color: 'var(--text-primary)' }}>{p.domain}</span>
                          <a href={`${p.ssl ? 'https' : 'http'}://${p.domain}`} target="_blank" rel="noreferrer"
                             className="opacity-40 hover:opacity-100 transition-opacity" style={{ color: 'var(--accent-secondary)' }}>
                            <ExternalLink size={11} />
                          </a>
                        </div>
                      </td>
                      <td className="px-4 py-3 font-mono" style={{ color: 'var(--text-secondary)' }}>{p.upstream}</td>
                      <td className="px-4 py-3">
                        {p.ssl
                          ? <span title="SSL enabled"><Lock size={13} style={{ color: 'var(--accent-success)' }} /></span>
                          : <span title="No SSL"><LockOpen size={13} style={{ color: 'var(--text-muted)' }} /></span>}
                      </td>
                      <td className="px-4 py-3 text-xs"
                          style={{ color: p.allow_embed ? 'var(--accent-secondary)' : 'var(--text-disabled)' }}
                          title={p.allow_embed ? 'iframe embedding allowed' : 'iframe embedding blocked'}>
                        {p.allow_embed ? 'allowed' : '—'}
                      </td>
                      <td className="px-4 py-3" style={{ color: 'var(--text-muted)' }}>{fmt(p.created_at)}</td>
                      <td className="px-4 py-3">
                        <span className="px-1.5 py-0.5 rounded text-xs"
                              style={{
                                background: p.enabled ? 'var(--accent-success-subtle)' : 'var(--bg-elevated)',
                                color: p.enabled ? 'var(--accent-success)' : 'var(--text-muted)',
                              }}>
                          {p.enabled ? 'active' : 'disabled'}
                        </span>
                      </td>
                      <td className="px-4 py-3">
                        <div className="flex items-center gap-1">
                          <button onClick={() => handleToggle(p)} disabled={toggling === p.id}
                            title={p.enabled ? 'Disable' : 'Enable'}
                            className="p-1 rounded hover:opacity-80 disabled:opacity-40 transition-colors"
                            style={{ color: p.enabled ? 'var(--accent-warning)' : 'var(--accent-success)' }}>
                            <Power size={13} />
                          </button>
                          <button onClick={() => handleDelete(p)} title="Delete"
                            className="p-1 rounded hover:opacity-80 transition-colors"
                            style={{ color: 'var(--accent-danger)' }}>
                            <Trash2 size={13} />
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              </div>
            )}
          </div>

          {proxies.length > 0 && (
            <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
              Configs written to <code className="font-mono">{sitesDir}/</code>. Use the File Manager for advanced settings.
            </p>
          )}
        </>
      )}

      {!nginxAvailable && !loading && (
        <div className="p-4 rounded text-xs text-center" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-muted)' }}>
          Install nginx above to start managing proxy rules from VoidTower.
        </div>
      )}
    </div>
  )
}

import { useState, useEffect, useCallback, useRef } from 'react'
import {
  BrainCircuit, Send, ExternalLink, Loader2, Play, Square, RotateCw,
  Trash2, ChevronDown, ChevronUp, Terminal, FileCode, Layers, RefreshCw,
  Plus, X, Box, Tag as TagIcon, Search, Download, ArrowDownToLine,
} from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import type { AppDef, DeployedApp, ComposeContainer, Tag, TagMap, ExternalStack } from '@/api/types'
import { notify } from '@/store/notifications'
import { useEmbedStore } from '@/store/embedStore'
import { useFiltersStore } from '@/store/filters'
import { TagPill, TagPopover } from '@/components/ui/TagPill'
import DeployToProxmoxModal from './DeployToProxmoxModal'
import DeployConfigModal from './DeployConfigModal'
import Button from '@/components/ui/Button'
import AiBadge from '@/components/ui/AiBadge'

const CATEGORY_LABELS: Record<string, string> = {
  dev:         'Development',
  media:       'Media',
  storage:     'Storage',
  networking:  'Networking',
  monitoring:  'Monitoring',
  productivity:'Productivity',
  database:    'Databases',
  home:        'Home',
  ai:          'AI',
  security:    'Security',
  vm:          'Virtual Machines',
}

// ─── Custom Deploy Tab ────────────────────────────────────────────────────────

function ListInput({ label, placeholder, items, onChange }: {
  label: string; placeholder: string; items: string[]; onChange: (items: string[]) => void
}) {
  const update = (i: number, val: string) => {
    const next = [...items]; next[i] = val; onChange(next)
  }
  const remove = (i: number) => onChange(items.filter((_, idx) => idx !== i))
  const add    = () => onChange([...items, ''])
  return (
    <div>
      <div className="flex items-center justify-between mb-1">
        <label className="text-xs font-medium" style={{ color: 'var(--text-muted)' }}>{label}</label>
        <button onClick={add} className="flex items-center gap-1 text-xs hover:opacity-80" style={{ color: 'var(--accent-primary)' }}>
          <Plus size={11} /> Add
        </button>
      </div>
      {items.map((val, i) => (
        <div key={i} className="flex gap-1 mb-1">
          <input
            value={val} onChange={e => update(i, e.target.value)}
            placeholder={placeholder}
            className="flex-1 px-2 py-1 rounded text-xs font-mono"
            style={{ background: 'var(--bg-input)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
          />
          <button onClick={() => remove(i)} className="px-1 hover:opacity-70" style={{ color: 'var(--text-muted)' }}>
            <X size={12} />
          </button>
        </div>
      ))}
      {items.length === 0 && (
        <p className="text-xs italic" style={{ color: 'var(--text-muted)' }}>None — click Add to set one</p>
      )}
    </div>
  )
}

// Pair [host, container] — joined with ':' before submit
function PairInput({ label, hostPlaceholder, containerPlaceholder, pairs, onChange, validateHost }: {
  label: string
  hostPlaceholder: string
  containerPlaceholder: string
  pairs: [string, string][]
  onChange: (pairs: [string, string][]) => void
  validateHost?: (val: string) => string | null
}) {
  const update = (i: number, side: 0 | 1, val: string) => {
    const next = pairs.map((p) => [...p] as [string, string])
    next[i][side] = val
    onChange(next)
  }
  const remove = (i: number) => onChange(pairs.filter((_, idx) => idx !== i))
  const add    = () => onChange([...pairs, ['', '']])
  return (
    <div>
      <div className="flex items-center justify-between mb-1">
        <label className="text-xs font-medium" style={{ color: 'var(--text-muted)' }}>{label}</label>
        <button onClick={add} className="flex items-center gap-1 text-xs hover:opacity-80" style={{ color: 'var(--accent-primary)' }}>
          <Plus size={11} /> Add
        </button>
      </div>
      {pairs.map(([host, container], i) => {
        const hostErr = validateHost ? validateHost(host) : null
        return (
          <div key={i} className="mb-1 space-y-0.5">
            <div className="flex items-center gap-1">
              <input
                value={host} onChange={e => update(i, 0, e.target.value)}
                placeholder={hostPlaceholder}
                className="flex-1 px-2 py-1 rounded text-xs font-mono"
                style={{
                  background: 'var(--bg-input)',
                  border: `1px solid ${hostErr ? 'var(--accent-danger)' : 'var(--border-subtle)'}`,
                  color: 'var(--text-primary)',
                }}
              />
              <span className="text-xs" style={{ color: 'var(--text-muted)' }}>→</span>
              <input
                value={container} onChange={e => update(i, 1, e.target.value)}
                placeholder={containerPlaceholder}
                className="flex-1 px-2 py-1 rounded text-xs font-mono"
                style={{ background: 'var(--bg-input)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
              />
              <button onClick={() => remove(i)} className="px-1 hover:opacity-70" style={{ color: 'var(--text-muted)' }}>
                <X size={12} />
              </button>
            </div>
            {hostErr && <p className="text-xs pl-1" style={{ color: 'var(--accent-danger)' }}>{hostErr}</p>}
          </div>
        )
      })}
      {pairs.length === 0 && (
        <p className="text-xs italic" style={{ color: 'var(--text-muted)' }}>None — click Add to set one</p>
      )}
    </div>
  )
}

function validatePortNumber(val: string): string | null {
  if (val === '') return null
  const n = Number(val)
  if (!Number.isInteger(n) || n < 1 || n > 65535) return 'Port must be 1–65535'
  return null
}

function validateName(val: string): string | null {
  if (val === '') return null
  if (!/^[a-z0-9][a-z0-9-]*$/.test(val)) return 'Lowercase letters, numbers, hyphens only; must start with a letter or digit'
  return null
}

const RESTART_POLICIES = ['unless-stopped', 'always', 'on-failure', 'no'] as const
type RestartPolicy = (typeof RESTART_POLICIES)[number]

function buildYamlPreview(
  name: string,
  image: string,
  ports: [string, string][],
  volumes: [string, string][],
  env: string[],
  restartPolicy: RestartPolicy,
  joinVtProxy: boolean,
): string {
  const lines: string[] = ['services:']
  const svcName = name.trim() || '<name>'
  lines.push(`  ${svcName}:`)
  lines.push(`    image: ${image.trim() || '<image>'}`)
  lines.push(`    restart: ${restartPolicy}`)

  const validPorts = ports.filter(([h, c]) => h && c).map(([h, c]) => `${h}:${c}`)
  if (validPorts.length > 0) {
    lines.push('    ports:')
    validPorts.forEach(p => lines.push(`      - "${p}"`))
  }

  const validVols = volumes.filter(([h, c]) => h && c).map(([h, c]) => `${h}:${c}`)
  if (validVols.length > 0) {
    lines.push('    volumes:')
    validVols.forEach(v => lines.push(`      - ${v}`))
  }

  const validEnv = env.filter(Boolean)
  if (validEnv.length > 0) {
    lines.push('    environment:')
    validEnv.forEach(e => lines.push(`      - ${e}`))
  }

  if (joinVtProxy) {
    lines.push('    networks:')
    lines.push('      - vt-proxy')
  }

  if (joinVtProxy) {
    lines.push('')
    lines.push('networks:')
    lines.push('  vt-proxy:')
    lines.push('    external: true')
  }

  return lines.join('\n')
}

function CustomDeployTab({ onDeployed }: { onDeployed: () => void }) {
  const [image,         setImage]         = useState('')
  const [name,          setName]          = useState('')
  const [ports,         setPorts]         = useState<[string, string][]>([])
  const [volumes,       setVolumes]       = useState<[string, string][]>([])
  const [env,           setEnv]           = useState<string[]>([])
  const [restartPolicy, setRestartPolicy] = useState<RestartPolicy>('unless-stopped')
  const [joinVtProxy,   setJoinVtProxy]   = useState(true)
  const [busy,          setBusy]          = useState(false)
  const [err,           setErr]           = useState<string | null>(null)
  const [done,          setDone]          = useState<string | null>(null)

  const nameErr  = validateName(name)
  const portErrs = ports.map(([h]) => validatePortNumber(h))
  const hasValidationErrors = !!nameErr || portErrs.some(Boolean)

  const deploy = async () => {
    if (!image.trim() || !name.trim()) { setErr('Image and name are required'); return }
    if (hasValidationErrors) { setErr('Fix validation errors before deploying'); return }
    setBusy(true); setErr(null); setDone(null)
    try {
      const res = await api.apps.deployCustom({
        name: name.trim(), image: image.trim(),
        ports:   ports.filter(([h, c]) => h && c).map(([h, c]) => `${h}:${c}`),
        volumes: volumes.filter(([h, c]) => h && c).map(([h, c]) => `${h}:${c}`),
        env: env.filter(Boolean),
      })
      setDone(res.project_name)
      onDeployed()
    } catch (e: any) { setErr(e.message) }
    finally { setBusy(false) }
  }

  const yaml = buildYamlPreview(name, image, ports, volumes, env, restartPolicy, joinVtProxy)

  return (
    <div className="max-w-2xl space-y-4">
      <div>
        <h2 className="text-sm font-semibold mb-1" style={{ color: 'var(--text-primary)' }}>Deploy any Docker image</h2>
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
          Paste any Docker Hub image and configure ports, volumes, and env vars. VoidTower generates the compose file and deploys it instantly.
        </p>
      </div>

      <div className="space-y-3 p-4 rounded-lg" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        {/* Image */}
        <div>
          <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-muted)' }}>Image <span style={{ color: 'var(--accent-danger)' }}>*</span></label>
          <input value={image} onChange={e => setImage(e.target.value)}
            placeholder="nginx:latest"
            className="w-full px-3 py-2 rounded text-sm font-mono"
            style={{ background: 'var(--bg-input)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
          />
        </div>

        {/* Name */}
        <div>
          <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-muted)' }}>Name <span style={{ color: 'var(--accent-danger)' }}>*</span></label>
          <input value={name} onChange={e => setName(e.target.value)}
            placeholder="my-nginx"
            className="w-full px-3 py-2 rounded text-sm"
            style={{
              background: 'var(--bg-input)',
              border: `1px solid ${nameErr ? 'var(--accent-danger)' : 'var(--border-default)'}`,
              color: 'var(--text-primary)',
            }}
          />
          {nameErr && <p className="text-xs mt-0.5 pl-1" style={{ color: 'var(--accent-danger)' }}>{nameErr}</p>}
        </div>

        {/* Restart policy */}
        <div>
          <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-muted)' }}>Restart policy</label>
          <select
            value={restartPolicy}
            onChange={e => setRestartPolicy(e.target.value as RestartPolicy)}
            className="w-full px-3 py-2 rounded text-sm"
            style={{ background: 'var(--bg-input)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
          >
            {RESTART_POLICIES.map(p => (
              <option key={p} value={p}>{p}</option>
            ))}
          </select>
        </div>

        {/* vt-proxy checkbox */}
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={joinVtProxy}
            onChange={e => setJoinVtProxy(e.target.checked)}
            className="rounded"
            style={{ accentColor: 'var(--accent-primary)' }}
          />
          <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            Join <code className="font-mono text-xs" style={{ color: 'var(--accent-primary)' }}>vt-proxy</code> network
          </span>
          <span className="text-xs" style={{ color: 'var(--text-muted)' }}>(recommended for reverse-proxy access)</span>
        </label>

        {/* Ports */}
        <PairInput
          label="Ports"
          hostPlaceholder="8080"
          containerPlaceholder="80"
          pairs={ports}
          onChange={setPorts}
          validateHost={validatePortNumber}
        />

        {/* Volumes */}
        <PairInput
          label="Volumes"
          hostPlaceholder="/host/path"
          containerPlaceholder="/container/path"
          pairs={volumes}
          onChange={setVolumes}
        />

        <ListInput label="Env vars" placeholder="KEY=value" items={env} onChange={setEnv} />
      </div>

      {/* YAML preview */}
      <div>
        <p className="text-xs font-medium mb-1" style={{ color: 'var(--text-muted)' }}>Compose preview</p>
        <pre
          className="text-xs font-mono rounded p-3 overflow-x-auto whitespace-pre"
          style={{
            background: 'var(--terminal-bg, var(--bg-elevated))',
            color: 'var(--terminal-green, var(--text-secondary))',
            border: '1px solid var(--border-subtle)',
            maxHeight: 260,
            overflowY: 'auto',
          }}
        >
          {yaml}
        </pre>
      </div>

      {err  && <p className="text-xs px-3 py-2 rounded" style={{ background: 'var(--accent-danger)18', color: 'var(--accent-danger)', border: '1px solid var(--accent-danger)44' }}>{err}</p>}
      {done && <p className="text-xs px-3 py-2 rounded" style={{ background: '#22c55e18', color: '#22c55e', border: '1px solid #22c55e44' }}>Deployed as <code>{done}</code> — check the Deployed tab.</p>}

      <button onClick={deploy} disabled={busy || !image.trim() || !name.trim() || hasValidationErrors}
        className="flex items-center gap-2 px-4 py-2 rounded text-sm font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
        style={{ background: 'var(--accent-primary)', color: '#fff' }}>
        {busy ? <Loader2 size={14} className="animate-spin" /> : <Box size={14} />}
        Deploy
      </button>
    </div>
  )
}

// ─── AI Discover Tab ──────────────────────────────────────────────────────────

interface AiApp {
  name: string
  description: string
  category: string
  docker_image?: string
  website?: string
  voidtower_compatible: boolean
  notes?: string
}

const SYSTEM_PROMPT = `You are a homelab and self-hosted software expert integrated into VoidTower, a Linux infrastructure management platform.

When the user asks for app recommendations, respond with a JSON array of app objects. Each object must have exactly these fields:
- name: string (app name)
- description: string (one sentence)
- category: one of: dev, media, storage, networking, monitoring, productivity, database, home, ai, security
- docker_image: string (Docker Hub image e.g. "nginx:latest", omit if unavailable)
- website: string (homepage URL)
- voidtower_compatible: boolean (true if it can run in Docker with docker-compose)
- notes: string (optional — deployment tips, port info, resource requirements)

Respond ONLY with valid JSON array, no markdown, no prose. Example:
[{"name":"Gitea","description":"Self-hosted Git service.","category":"dev","docker_image":"gitea/gitea:latest","website":"https://gitea.io","voidtower_compatible":true,"notes":"Runs on port 3000"}]`

function DiscoverTab({ catalogApps, deployedIds, dockerAvailable, onDeploy, deploying }: {
  catalogApps: AppDef[]
  deployedIds: Set<string>
  dockerAvailable: boolean
  onDeploy: (app: AppDef) => void
  deploying: string | null
}) {
  const llmEndpoint = localStorage.getItem('vt-ai-llm-endpoint') || ''
  const [query, setQuery]       = useState('')
  const [results, setResults]   = useState<AiApp[]>([])
  const [thinking, setThinking] = useState(false)
  const [error, setError]       = useState<string | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const search = async () => {
    if (!query.trim() || !llmEndpoint) return
    setThinking(true)
    setError(null)
    setResults([])
    try {
      const res = await fetch(`${llmEndpoint}/v1/chat/completions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: 'default',
          messages: [
            { role: 'system', content: SYSTEM_PROMPT },
            { role: 'user',   content: query },
          ],
          temperature: 0.3,
          max_tokens: 1500,
        }),
      })
      if (!res.ok) throw new Error(`LLM returned ${res.status}`)
      const data = await res.json()
      const text: string = data.choices?.[0]?.message?.content ?? ''
      // Extract JSON array from response (tolerant of extra whitespace)
      const match = text.match(/\[[\s\S]*\]/)
      if (!match) throw new Error('No JSON array found in response')
      const parsed: AiApp[] = JSON.parse(match[0])
      setResults(parsed)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to get AI response')
    } finally {
      setThinking(false)
    }
  }

  const handleKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') search()
  }

  // Find matching catalog app by name (case-insensitive)
  const findCatalogApp = (name: string): AppDef | undefined =>
    catalogApps.find((a) => a.name.toLowerCase() === name.toLowerCase())

  if (!llmEndpoint) {
    return (
      <div className="flex flex-col items-center justify-center gap-4 py-16">
        <BrainCircuit size={40} style={{ color: 'var(--text-muted)', opacity: 0.4 }} />
        <p className="text-sm" style={{ color: 'var(--text-muted)' }}>No AI endpoint configured.</p>
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
          Go to <strong style={{ color: 'var(--text-secondary)' }}>Settings → AI Integrations</strong> and set your LLM endpoint to use this feature.
        </p>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Search bar */}
      <div className="flex gap-2">
        <div className="flex-1 flex items-center gap-2 px-3 py-2 rounded"
          style={{ background: 'var(--bg-card)', border: '1px solid var(--border-subtle)' }}>
          <BrainCircuit size={15} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKey}
            placeholder="Ask AI — e.g. 'I need a password manager' or 'best monitoring tools for homelab'"
            className="flex-1 bg-transparent outline-none text-sm"
            style={{ color: 'var(--text-primary)' }}
            autoFocus
          />
          {thinking && <Loader2 size={14} className="animate-spin" style={{ color: 'var(--text-muted)' }} />}
        </div>
        <Button size="sm" variant="primary" onClick={search} loading={thinking} disabled={!query.trim()}>
          <Send size={13} className="mr-1" /> Ask
        </Button>
      </div>

      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        AI searches for self-hosted apps that match your needs. Apps already in the catalog can be deployed directly.
      </p>

      {error && (
        <div className="p-3 rounded text-xs" style={{ background: 'var(--accent-danger-subtle)', color: 'var(--accent-danger)', border: '1px solid var(--accent-danger)' }}>
          {error}
        </div>
      )}

      {/* Results */}
      {results.length > 0 && (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {results.map((app, i) => {
            const catalogApp = findCatalogApp(app.name)
            const isDeployed = catalogApp ? deployedIds.has(catalogApp.id) : false
            return (
              <div key={i} className="card flex flex-col gap-2">
                <div className="flex items-start justify-between gap-2">
                  <div>
                    <div className="font-medium text-sm" style={{ color: 'var(--text-primary)' }}>{app.name}</div>
                    <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
                      {CATEGORY_LABELS[app.category] ?? app.category}
                    </div>
                  </div>
                  <div className="flex items-center gap-1 flex-shrink-0">
                    {app.voidtower_compatible && (
                      <span className="text-xs px-1.5 py-0.5 rounded"
                        style={{ background: 'var(--accent-success-subtle)', color: 'var(--accent-success)' }}>
                        Docker ✓
                      </span>
                    )}
                    {isDeployed && (
                      <span className="text-xs px-1.5 py-0.5 rounded"
                        style={{ background: 'var(--bg-elevated)', color: 'var(--accent-primary)' }}>
                        Running
                      </span>
                    )}
                  </div>
                </div>

                <p className="text-xs flex-1" style={{ color: 'var(--text-secondary)' }}>{app.description}</p>

                {app.notes && (
                  <p className="text-xs italic" style={{ color: 'var(--text-muted)' }}>{app.notes}</p>
                )}

                {app.docker_image && (
                  <div className="text-xs font-mono px-2 py-1 rounded" style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)' }}>
                    {app.docker_image}
                  </div>
                )}

                <div className="flex items-center gap-2 mt-auto pt-1">
                  {catalogApp ? (
                    <Button
                      size="sm"
                      disabled={isDeployed || !dockerAvailable}
                      loading={deploying === catalogApp.id}
                      onClick={() => onDeploy(catalogApp)}
                    >
                      {isDeployed ? 'Deployed' : 'Deploy'}
                    </Button>
                  ) : (
                    app.voidtower_compatible && (
                      <span className="text-xs" style={{ color: 'var(--text-muted)' }}>Not in catalog yet</span>
                    )
                  )}
                  {app.website && (
                    <a href={app.website} target="_blank" rel="noopener noreferrer"
                      className="flex items-center gap-1 text-xs"
                      style={{ color: 'var(--accent-secondary)' }}>
                      <ExternalLink size={11} /> Website
                    </a>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      )}

      {!thinking && results.length === 0 && !error && (
        <div className="grid gap-3 sm:grid-cols-3">
          {[
            'best monitoring tools for homelab',
            'I need a self-hosted password manager',
            'photo management apps like Google Photos',
            'home automation platform',
            'self-hosted email server',
            'collaborative document editing',
          ].map((suggestion) => (
            <button
              key={suggestion}
              onClick={() => { setQuery(suggestion); inputRef.current?.focus() }}
              className="text-left p-3 rounded text-xs transition-colors hover:opacity-80"
              style={{ background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
            >
              "{suggestion}"
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

// ─── Deployed App Panel ───────────────────────────────────────────────────────

type AppPanelTab = 'containers' | 'compose' | 'logs'

function DeployedAppPanel({ app, onRefresh }: { app: DeployedApp; onRefresh: () => void }) {
  const [tab, setTab] = useState<AppPanelTab>('containers')
  const [acting, setActing] = useState<string | null>(null)

  // Containers tab
  const [containers, setContainers] = useState<ComposeContainer[] | null>(null)
  const [containersLoading, setContainersLoading] = useState(false)

  // Compose tab
  const [compose, setCompose] = useState<string | null>(null)
  const [composeDirty, setComposeDirty] = useState(false)
  const [composeSaving, setComposeSaving] = useState(false)

  // Logs tab
  const [logs, setLogs] = useState<string[] | null>(null)
  const [logsLoading, setLogsLoading] = useState(false)
  const logsRef = useRef<HTMLPreElement>(null)

  const loadTab = useCallback(async (t: AppPanelTab) => {
    const p = app.project_name
    if (t === 'containers' && containers === null) {
      setContainersLoading(true)
      try { setContainers((await api.apps.status(p)).containers) }
      catch { setContainers([]) }
      finally { setContainersLoading(false) }
    }
    if (t === 'compose' && compose === null) {
      try { setCompose((await api.apps.getCompose(p)).content) }
      catch { setCompose('# Failed to load compose file') }
    }
    if (t === 'logs') {
      setLogsLoading(true)
      try { setLogs((await api.apps.logs(p)).lines) }
      catch { setLogs(['Failed to fetch logs']) }
      finally {
        setLogsLoading(false)
        setTimeout(() => logsRef.current?.scrollTo(0, logsRef.current.scrollHeight), 50)
      }
    }
  }, [app.project_name, containers, compose])

  useEffect(() => { loadTab(tab) }, [tab, loadTab])

  const action = async (kind: string) => {
    const p = app.project_name
    setActing(kind)
    try {
      if (kind === 'start')    await api.apps.start(p)
      if (kind === 'stop')     await api.apps.stop(p)
      if (kind === 'restart')  await api.apps.restart(p)
      if (kind === 'redeploy') await api.apps.redeploy(p)
      if (kind === 'remove') {
        if (!confirm(`Remove "${app.app_name}" and delete all its containers and volumes? This cannot be undone.`)) {
          setActing(null); return
        }
        await api.apps.remove(p)
        notify.success(`${app.app_name} removed`)
        onRefresh(); return
      }
      const label = kind === 'start' ? 'started' : kind === 'stop' ? 'stopped' : kind === 'redeploy' ? 'redeployed' : 'restarted'
      notify.success(`${app.app_name} ${label}`)
      onRefresh()
      // Reload containers after action
      if (tab === 'containers') {
        setContainers(null)
        setTimeout(() => loadTab('containers'), 1500)
      }
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : `${kind} failed`)
    } finally { setActing(null) }
  }

  const saveCompose = async () => {
    if (!compose) return
    setComposeSaving(true)
    try {
      await api.apps.updateCompose(app.project_name, compose)
      notify.success('Compose saved and applied')
      setComposeDirty(false)
      onRefresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Save failed')
    } finally { setComposeSaving(false) }
  }

  const running = app.status === 'running'

  return (
    <div className="mt-2 rounded-lg overflow-hidden" style={{ border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)' }}>
      {/* Action bar */}
      <div className="flex items-center gap-2 px-4 py-2.5" style={{ borderBottom: '1px solid var(--border-subtle)', background: 'var(--bg-panel)' }}>
        {!running && (
          <Button size="sm" variant="primary" loading={acting === 'start'} onClick={() => action('start')}>
            <Play size={11} className="mr-1" /> Start
          </Button>
        )}
        {running && (
          <Button size="sm" variant="ghost" loading={acting === 'stop'} onClick={() => action('stop')}>
            <Square size={11} className="mr-1" /> Stop
          </Button>
        )}
        {running && (
          <Button size="sm" variant="ghost" loading={acting === 'restart'} onClick={() => action('restart')}>
            <RotateCw size={11} className="mr-1" /> Restart
          </Button>
        )}
        <Button size="sm" variant="ghost" loading={acting === 'redeploy'} onClick={() => action('redeploy')}
          title="Re-read catalog YAML and run docker compose up --build (picks up config changes)">
          <RotateCw size={11} className="mr-1" /> Redeploy
        </Button>
        <Button size="sm" variant="ghost" loading={acting === 'remove'}
          onClick={() => action('remove')}
          style={{ color: 'var(--accent-danger)', marginLeft: 'auto' }}>
          <Trash2 size={11} className="mr-1" /> Remove
        </Button>
      </div>

      {/* Sub-tabs */}
      <div className="flex" style={{ borderBottom: '1px solid var(--border-subtle)' }}>
        {([
          { id: 'containers', label: 'Containers', icon: Layers },
          { id: 'compose',    label: 'Compose',    icon: FileCode },
          { id: 'logs',       label: 'Logs',       icon: Terminal },
        ] as const).map(({ id, label, icon: Icon }) => (
          <button key={id} onClick={() => setTab(id)}
            className="flex items-center gap-1.5 px-4 py-2 text-xs transition-colors border-b-2 -mb-px"
            style={{
              color: tab === id ? 'var(--accent-primary)' : 'var(--text-muted)',
              borderColor: tab === id ? 'var(--accent-primary)' : 'transparent',
            }}>
            <Icon size={11} />{label}
          </button>
        ))}
      </div>

      {/* Containers tab */}
      {tab === 'containers' && (
        <div className="p-3">
          <div className="flex justify-end mb-2">
            <button onClick={() => { setContainers(null); loadTab('containers') }}
              className="flex items-center gap-1 text-xs hover:opacity-80"
              style={{ color: 'var(--text-muted)' }}>
              <RefreshCw size={11} /> Refresh
            </button>
          </div>
          {containersLoading ? (
            <p className="text-xs py-4 text-center" style={{ color: 'var(--text-muted)' }}>Loading…</p>
          ) : !containers || containers.length === 0 ? (
            <p className="text-xs py-4 text-center" style={{ color: 'var(--text-muted)' }}>
              {running ? 'No container data — is Docker accessible?' : 'App is stopped.'}
            </p>
          ) : (
            <table className="w-full text-xs">
              <thead>
                <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  {['Container', 'Service', 'State', 'Ports'].map(h => (
                    <th key={h} className="px-3 py-1.5 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {containers.map((c, i) => (
                  <tr key={i} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    <td className="px-3 py-2 font-mono" style={{ color: 'var(--text-primary)' }}>{c.name}</td>
                    <td className="px-3 py-2" style={{ color: 'var(--text-secondary)' }}>{c.service}</td>
                    <td className="px-3 py-2">
                      <span className="px-1.5 py-0.5 rounded text-xs"
                        style={{
                          background: c.state === 'running' ? 'var(--accent-success-subtle)' : 'var(--bg-elevated)',
                          color: c.state === 'running' ? 'var(--accent-success)' : 'var(--text-muted)',
                        }}>
                        {c.state}
                      </span>
                    </td>
                    <td className="px-3 py-2 font-mono" style={{ color: 'var(--accent-secondary)' }}>
                      {c.ports.length ? c.ports.join(', ') : '—'}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}

      {/* Compose tab */}
      {tab === 'compose' && (
        <div className="p-3 space-y-2">
          <textarea
            value={compose ?? ''}
            onChange={e => { setCompose(e.target.value); setComposeDirty(true) }}
            spellCheck={false}
            className="w-full font-mono text-xs outline-none rounded p-3 resize-none"
            style={{
              background: 'var(--bg-panel)',
              border: `1px solid ${composeDirty ? 'var(--accent-warning)' : 'var(--border-subtle)'}`,
              color: 'var(--text-primary)',
              minHeight: 260,
            }}
          />
          <div className="flex items-center gap-2">
            <Button size="sm" variant="primary" loading={composeSaving}
              disabled={!composeDirty} onClick={saveCompose}>
              Save &amp; apply
            </Button>
            {composeDirty && (
              <span className="text-xs" style={{ color: 'var(--accent-warning)' }}>Unsaved changes</span>
            )}
            <span className="text-xs ml-auto" style={{ color: 'var(--text-muted)' }}>
              Saves the file and runs <code className="font-mono">docker compose up -d</code>
            </span>
          </div>
        </div>
      )}

      {/* Logs tab */}
      {tab === 'logs' && (
        <div className="p-3 space-y-2">
          <div className="flex justify-end">
            <button onClick={() => { setLogs(null); loadTab('logs') }}
              className="flex items-center gap-1 text-xs hover:opacity-80"
              style={{ color: 'var(--text-muted)' }}>
              <RefreshCw size={11} /> Refresh
            </button>
          </div>
          {logsLoading ? (
            <p className="text-xs py-4 text-center" style={{ color: 'var(--text-muted)' }}>Fetching logs…</p>
          ) : (
            <pre ref={logsRef}
              className="text-xs font-mono overflow-y-auto rounded p-3 whitespace-pre-wrap"
              style={{
                background: 'var(--terminal-bg)',
                color: 'var(--terminal-green)',
                maxHeight: 320,
                border: '1px solid var(--border-subtle)',
              }}>
              {logs?.join('\n') || '(no output)'}
            </pre>
          )}
        </div>
      )}
    </div>
  )
}

// ─── Deployed Tab ─────────────────────────────────────────────────────────────

// ── External App Detection ────────────────────────────────────────────────────

const PLATFORM_TEASERS = [
  { id: 'portainer', name: 'Portainer', desc: 'Detect stacks managed by Portainer via its REST API.' },
  { id: 'truenas',   name: 'TrueNAS SCALE', desc: 'Import apps managed by TrueNAS SCALE via its API.' },
]

function ExternalTab({ onAdopted }: { onAdopted: () => void }) {
  const [scanning, setScanning]   = useState(false)
  const [scanned, setScanned]     = useState(false)
  const [stacks, setStacks]       = useState<ExternalStack[]>([])
  const [adopting, setAdopting]   = useState<string | null>(null)
  const [converting, setConverting] = useState<string | null>(null)
  const [confirmConvert, setConfirmConvert] = useState<ExternalStack | null>(null)
  const [adopted, setAdopted]     = useState<Set<string>>(new Set())

  const scan = async () => {
    setScanning(true)
    try {
      const results = await api.apps.detectExternal()
      setStacks(results)
      setScanned(true)
    } catch { notify.error('Scan failed') }
    finally { setScanning(false) }
  }

  const adopt = async (stack: ExternalStack) => {
    setAdopting(stack.project_name)
    try {
      await api.apps.adoptApp({
        project_name: stack.project_name,
        app_name: stack.project_name,
        compose_path: stack.compose_path ?? undefined,
        primary_port: stack.primary_port ?? undefined,
      })
      setAdopted(prev => new Set([...prev, stack.project_name]))
      notify.success(`${stack.project_name} invited to VoidTower`)
      onAdopted()
    } catch (e) { notify.error(e instanceof ApiClientError ? e.message : 'Adopt failed') }
    finally { setAdopting(null) }
  }

  const convert = async (stack: ExternalStack) => {
    setConverting(stack.project_name)
    setConfirmConvert(null)
    try {
      await api.apps.convertApp(stack.project_name)
      setAdopted(prev => new Set([...prev, stack.project_name]))
      notify.success(`${stack.project_name} converted to VoidTower management`)
      onAdopted()
    } catch (e) { notify.error(e instanceof ApiClientError ? e.message : 'Convert failed') }
    finally { setConverting(null) }
  }

  return (
    <div className="space-y-6">
      {/* Scan section */}
      <div className="flex items-center gap-3">
        <Button onClick={scan} loading={scanning} size="sm">
          <Search size={13} /> Scan local Docker
        </Button>
        {scanned && !scanning && (
          <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
            {stacks.length === 0 ? 'No unmanaged stacks found.' : `${stacks.length} external stack${stacks.length !== 1 ? 's' : ''} detected`}
          </span>
        )}
        {!scanned && (
          <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
            Detects Docker Compose stacks and standalone containers not yet managed by VoidTower.
          </span>
        )}
      </div>

      {/* Results */}
      {stacks.map(stack => {
        const isAdopted = adopted.has(stack.project_name)
        const isBusy = adopting === stack.project_name || converting === stack.project_name
        return (
          <div key={stack.project_name} className="rounded-lg overflow-hidden"
            style={{ border: '1px solid var(--border-subtle)', background: 'var(--bg-card)' }}>
            <div className="flex items-center gap-3 px-4 py-3">
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="font-medium text-sm" style={{ color: 'var(--text-primary)' }}>
                    {stack.project_name}
                  </span>
                  <span className="text-xs px-1.5 py-0.5 rounded"
                    style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}>
                    local Docker
                  </span>
                  {stack.primary_port && (
                    <span className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>:{stack.primary_port}</span>
                  )}
                </div>
                <div className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>
                  {stack.compose_path ?? 'No compose file detected'}
                </div>
                <div className="flex flex-wrap gap-2 mt-2">
                  {stack.containers.map(c => (
                    <span key={c.id} className="text-xs px-2 py-0.5 rounded font-mono flex items-center gap-1"
                      style={{ background: 'var(--bg-elevated)', color: c.state === 'running' ? 'var(--accent-success)' : 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}>
                      <span className="w-1.5 h-1.5 rounded-full inline-block"
                        style={{ background: c.state === 'running' ? 'var(--accent-success)' : 'var(--text-disabled)' }} />
                      {c.name.replace(/^\//, '')}
                      {c.ports.length > 0 && <span style={{ color: 'var(--text-muted)' }}>·{c.ports[0].split(':')[0]}</span>}
                    </span>
                  ))}
                </div>
              </div>
              {isAdopted ? (
                <span className="text-xs px-2 py-1 rounded"
                  style={{ background: 'var(--accent-success-subtle)', color: 'var(--accent-success)', border: '1px solid var(--accent-success)' }}>
                  Invited ✓
                </span>
              ) : (
                <div className="flex gap-2 flex-shrink-0">
                  <Button size="sm" loading={adopting === stack.project_name} disabled={isBusy}
                    onClick={() => adopt(stack)} title="Register in VoidTower and connect to internal network">
                    <Download size={12} /> Invite
                  </Button>
                  {stack.compose_path && (
                    <Button size="sm" variant="ghost" loading={converting === stack.project_name} disabled={isBusy}
                      onClick={() => setConfirmConvert(stack)} title="Copy compose file to VoidTower and manage fully">
                      <ArrowDownToLine size={12} /> Convert
                    </Button>
                  )}
                </div>
              )}
            </div>
          </div>
        )
      })}

      {/* Platform teasers */}
      <div>
        <p className="text-xs font-semibold mb-3 uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>
          More platforms — coming soon
        </p>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {PLATFORM_TEASERS.map(p => (
            <div key={p.id} className="rounded-lg px-4 py-3 flex items-start gap-3 opacity-60"
              style={{ border: '1px dashed var(--border-subtle)', background: 'var(--bg-elevated)' }}>
              <div className="flex-1 min-w-0">
                <div className="font-medium text-sm" style={{ color: 'var(--text-primary)' }}>{p.name}</div>
                <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{p.desc}</div>
              </div>
              <span className="text-xs px-2 py-0.5 rounded flex-shrink-0"
                style={{ background: 'var(--bg-panel)', color: 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}>
                Soon
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* Convert confirmation modal */}
      {confirmConvert && (
        <div style={{ position: 'fixed', inset: 0, zIndex: 50, background: 'rgba(0,0,0,0.55)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setConfirmConvert(null)}>
          <div style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 10, padding: 24, maxWidth: 440, width: '100%' }}
            onClick={e => e.stopPropagation()}>
            <h2 className="text-sm font-semibold mb-3" style={{ color: 'var(--text-primary)' }}>Convert "{confirmConvert.project_name}"?</h2>
            <p className="text-xs mb-2" style={{ color: 'var(--text-secondary)' }}>
              VoidTower will copy the compose file to its app directory and take over management.
              Running containers are restarted in-place — named Docker volumes stay untouched.
            </p>
            <p className="text-xs mb-4" style={{ color: 'var(--accent-warning)' }}>
              Bind-mount host paths remain where they are. If your original compose file referenced
              relative paths, verify they resolve correctly from the new location.
            </p>
            <div className="flex gap-2">
              <Button size="sm" onClick={() => convert(confirmConvert)}>Convert</Button>
              <Button size="sm" variant="ghost" onClick={() => setConfirmConvert(null)}>Cancel</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function DeployedTab({ deployed, catalogApps, allTags, tagMap, globalTag, onRefresh, onTagsChanged }: {
  deployed: DeployedApp[]; catalogApps: AppDef[];
  allTags: Tag[]; tagMap: TagMap; globalTag: string | null;
  onRefresh: () => void; onTagsChanged: () => void
}) {
  const [expanded, setExpanded] = useState<string | null>(null)
  const [popover, setPopover] = useState<string | null>(null)
  const embedOpen  = useEmbedStore(s => s.open)
  const embedApp   = useEmbedStore(s => s.app)
  const listRef    = useRef<HTMLDivElement>(null)
  const savedScroll = useRef<number>(0)

  // Save scroll position before opening overlay; restore when overlay closes
  const openEmbed = (app: DeployedApp, def: ReturnType<typeof useEmbedStore.getState>['def']) => {
    savedScroll.current = listRef.current?.scrollTop ?? 0
    embedOpen(app, def!)
  }

  useEffect(() => {
    if (embedApp === null && savedScroll.current > 0 && listRef.current) {
      listRef.current.scrollTop = savedScroll.current
    }
  }, [embedApp])

  const displayed = globalTag ? deployed.filter(app => (tagMap[app.project_name] || []).some(t => t.id === globalTag)) : deployed

  if (deployed.length === 0) {
    return (
      <p className="py-8 text-center text-sm" style={{ color: 'var(--text-muted)' }}>
        No apps deployed yet. Deploy one from the Catalog tab.
      </p>
    )
  }

  if (displayed.length === 0) {
    return (
      <p className="py-8 text-center text-sm" style={{ color: 'var(--text-muted)' }}>
        No deployed apps with this tag.
      </p>
    )
  }

  return (
    <div ref={listRef} className="space-y-2" style={{ overflowY: 'auto' }}>
      {displayed.map(app => {
        const open = expanded === app.project_name
        const running = app.status === 'running'
        return (
          <div key={app.id} className="rounded-lg overflow-hidden"
            style={{ border: '1px solid var(--border-subtle)', background: 'var(--bg-card)' }}>
            <button
              className="w-full flex items-center gap-3 px-4 py-3 text-left hover:opacity-90 transition-opacity"
              onClick={() => setExpanded(open ? null : app.project_name)}
            >
              {/* Status dot */}
              <span className="w-2 h-2 rounded-full flex-shrink-0"
                style={{ background: running ? 'var(--accent-success)' : 'var(--text-disabled)' }} />

              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-sm" style={{ color: 'var(--text-primary)' }}>
                    {app.app_name}
                  </span>
                  <span className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>
                    {app.project_name}
                  </span>
                  {app.origin === 'adopted' && (
                    <span className="text-xs px-1.5 py-0.5 rounded"
                      style={{ background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)', opacity: 0.8 }}>
                      Adopted
                    </span>
                  )}
                  {app.primary_port && (() => {
                    const catalogDef = catalogApps.find(d => d.id === app.app_id)
                    const uiPort = catalogDef?.web_port ?? app.primary_port
                    const uiPath = catalogDef?.web_path ?? ''
                    return (
                      <a
                        href={`http://${window.location.hostname}:${uiPort}${uiPath}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        onClick={e => e.stopPropagation()}
                        className="flex items-center gap-1 text-xs px-1.5 py-0.5 rounded transition-opacity hover:opacity-80"
                        style={{ background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)' }}
                      >
                        <ExternalLink size={10} />
                        :{uiPort}
                      </a>
                    )
                  })()}
                </div>
                <div className="text-xs mt-0.5 flex items-center gap-3" style={{ color: 'var(--text-muted)' }}>
                  <span style={{ color: running ? 'var(--accent-success)' : 'var(--text-disabled)' }}>
                    {app.status}
                  </span>
                  <span>Deployed {new Date(app.deployed_at * 1000).toLocaleDateString()}</span>
                </div>
                <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 3, alignItems: 'center', position: 'relative' }} onClick={e => e.stopPropagation()}>
                  {(tagMap[app.project_name] || []).map(t => <TagPill key={t.id} tag={t} />)}
                  <button onClick={() => setPopover(popover === app.project_name ? null : app.project_name)} style={{
                    background: 'none', border: '1px dashed var(--border-subtle)', borderRadius: 10,
                    cursor: 'pointer', color: 'var(--text-muted)', fontSize: 11, padding: '0px 6px', lineHeight: '18px',
                  }}><TagIcon size={10} style={{ display: 'inline', verticalAlign: 'middle' }} /></button>
                  {popover === app.project_name && (
                    <TagPopover resourceType="app" resourceId={app.project_name} allTags={allTags} assigned={tagMap[app.project_name] || []} onClose={() => { setPopover(null); onTagsChanged() }} />
                  )}
                </div>
              </div>

              {running && app.primary_port !== null && !catalogApps.find(d => d.id === app.app_id)?.no_web_ui && (
                <button
                  onClick={e => {
                    e.stopPropagation()
                    const def = catalogApps.find(d => d.id === app.app_id) ?? {
                      id: app.app_id, name: app.app_name, description: '', category: '',
                      icon: '', version_hint: '', links: {},
                    }
                    openEmbed(app, def)
                  }}
                  className="flex items-center gap-1 text-xs px-2 py-1 rounded transition-opacity hover:opacity-80"
                  style={{
                    background: 'var(--bg-elevated)',
                    color: 'var(--text-secondary)',
                    border: '1px solid var(--border-subtle)',
                    flexShrink: 0,
                  }}
                  title="Open web UI inside VoidTower"
                >
                  <Box size={11} />
                  Open
                </button>
              )}

              <span style={{ color: 'var(--text-muted)', flexShrink: 0 }}>
                {open ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
              </span>
            </button>

            {open && <DeployedAppPanel app={app} onRefresh={() => { setExpanded(null); onRefresh() }} />}
          </div>
        )
      })}
    </div>
  )
}

// ─── Main Page ────────────────────────────────────────────────────────────────

export default function AppVaultPage() {
  const [apps, setApps]                 = useState<AppDef[]>([])
  const [deployed, setDeployed]         = useState<DeployedApp[]>([])
  const [dockerAvailable, setDockerAvailable] = useState(true)
  const [loading, setLoading]           = useState(true)
  const [deploying, setDeploying]       = useState<string | null>(null)
  const [deployError, setDeployError]   = useState<string | null>(null)
  const [search, setSearch]             = useState('')
  const [category, setCategory]         = useState<string>('all')
  const [tab, setTab]                   = useState<'catalog' | 'deployed' | 'discover' | 'custom' | 'external'>('deployed')
  const [proxmoxDeployApp, setProxmoxDeployApp] = useState<AppDef | null>(null)
  const [configModalApp, setConfigModalApp]     = useState<AppDef | null>(null)
  const [allTags, setAllTags]           = useState<Tag[]>([])
  const [tagMap, setTagMap]             = useState<TagMap>({})
  const globalTag = useFiltersStore((s) => s.globalTag)

  const loadTags = useCallback(async () => {
    try {
      const [tags, map] = await Promise.all([api.tags.list(), api.tags.map('app')])
      setAllTags(tags)
      setTagMap(map)
    } catch { /* empty */ }
  }, [])

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const [catData, depData] = await Promise.all([
        api.apps.catalog(),
        api.apps.deployed(),
      ])
      setApps(catData.apps)
      setDeployed(depData.apps)
      setDockerAvailable(depData.docker_available)
    } catch {
      notify.error('Failed to load App Vault')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { load(); loadTags() }, [load, loadTags])

  const deploy = async (app: AppDef) => {
    setDeploying(app.id)
    setDeployError(null)
    try {
      const result = await api.apps.deploy(app.id)
      notify.success(`${app.name} deployed as ${result.project_name}`)
      await load()
      setTab('deployed')
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Deploy failed'
      setDeployError(msg)
      notify.error('Deploy failed', msg.split('\n')[0])
    } finally {
      setDeploying(null)
    }
  }

  const categories = ['all', ...Array.from(new Set(apps.map((a) => a.category))).sort()]
  const filtered = apps.filter((a) => {
    const matchSearch = !search || a.name.toLowerCase().includes(search.toLowerCase()) || a.description.toLowerCase().includes(search.toLowerCase())
    const matchCat = category === 'all' || a.category === category
    return matchSearch && matchCat
  })
  const deployedIds = new Set(deployed.filter((d) => d.status === 'running').map((d) => d.app_id))

  return (
    <>
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>App Vault</h1>
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {apps.length} apps available · Deploy self-hosted software with one click
          </p>
        </div>
        <Button size="sm" onClick={load} loading={loading}>Refresh</Button>
      </div>

      {/* Tabs */}
      <div className="flex gap-1 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
        {([
          { id: 'deployed', label: `Deployed (${deployed.length})` },
          { id: 'catalog',  label: `Catalog (${apps.length})` },
          { id: 'external', label: '⇣ External' },
          { id: 'discover', label: '✦ AI Discover' },
          { id: 'custom',   label: '⊕ Custom Deploy' },
        ] as const).map(({ id, label }) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className="px-4 py-2 text-xs transition-colors border-b-2 -mb-px"
            style={{
              color: tab === id ? 'var(--accent-primary)' : 'var(--text-muted)',
              borderColor: tab === id ? 'var(--accent-primary)' : 'transparent',
            }}
          >
            {label}
          </button>
        ))}
      </div>

      {tab === 'catalog' && (
        <>
          {!dockerAvailable && (
            <div className="card text-xs" style={{ color: 'var(--accent-warning)', borderColor: 'var(--accent-warning)' }}>
              Docker is not available — deployment is disabled.
            </div>
          )}
          <div className="flex flex-wrap gap-3">
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search apps…"
              className="flex-1 px-3 py-1.5 rounded text-sm border outline-none"
              style={{ background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
            />
            <select
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              className="px-3 py-1.5 rounded text-sm border outline-none"
              style={{ background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
            >
              {categories.map((c) => (
                <option key={c} value={c}>{c === 'all' ? 'All categories' : (CATEGORY_LABELS[c] ?? c)}</option>
              ))}
            </select>
          </div>
          {deployError && (
            <div className="rounded-lg p-3 text-xs font-mono" style={{
              background: 'var(--accent-danger)18',
              border: '1px solid var(--accent-danger)44',
              color: 'var(--accent-danger)',
            }}>
              <div className="flex items-start justify-between gap-2 mb-1">
                <span className="font-semibold not-italic">Deploy error</span>
                <button onClick={() => setDeployError(null)} style={{ color: 'var(--text-muted)' }}>
                  <X size={12} />
                </button>
              </div>
              <pre className="whitespace-pre-wrap break-all overflow-auto" style={{ maxHeight: 200 }}>
                {deployError}
              </pre>
            </div>
          )}
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {filtered.map((app) => {
              const isDeployed = deployedIds.has(app.id)
              return (
                <div key={app.id} className="card flex flex-col gap-2">
                  <div className="flex items-start justify-between gap-2">
                    <div>
                      <div className="font-medium text-sm" style={{ color: 'var(--text-primary)' }}>{app.name}</div>
                      <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
                        {CATEGORY_LABELS[app.category] ?? app.category}
                      </div>
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      {app.ai_integration && (
                        <AiBadge level={app.ai_integration.level} description={app.ai_integration.description} />
                      )}
                      {isDeployed && (
                        <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: 'var(--bg-elevated)', color: 'var(--accent-success)' }}>
                          Running
                        </span>
                      )}
                    </div>
                  </div>
                  <p className="text-xs flex-1" style={{ color: 'var(--text-secondary)' }}>{app.description}</p>
                  <div className="flex items-center gap-2 mt-auto pt-1">
                    {isDeployed ? (
                      <Button
                        size="sm"
                        variant="ghost"
                        loading={deploying === app.id}
                        onClick={async () => {
                          const dep = deployed.find(d => d.app_id === app.id)
                          if (!dep) return
                          setDeploying(app.id)
                          try {
                            await api.apps.redeploy(dep.project_name)
                            notify.success(`${app.name} redeployed`)
                            await load()
                          } catch (e: unknown) {
                            notify.error(e instanceof Error ? e.message : 'Redeploy failed')
                          } finally { setDeploying(null) }
                        }}
                        title="Re-read catalog config and rebuild (docker compose up --build)"
                      >
                        <RotateCw size={11} className="mr-1" /> Redeploy
                      </Button>
                    ) : (
                      <Button
                        size="sm"
                        disabled={!dockerAvailable}
                        onClick={() => setConfigModalApp(app)}
                      >
                        Deploy
                      </Button>
                    )}
                    <button
                      onClick={() => setProxmoxDeployApp(app)}
                      className="text-xs px-2 py-1 rounded"
                      style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-muted)', cursor: 'pointer' }}
                      title="Deploy to Proxmox LXC"
                    >
                      → Proxmox
                    </button>
                    {app.links.home && (
                      <a href={app.links.home} target="_blank" rel="noopener noreferrer"
                        className="text-xs" style={{ color: 'var(--accent-secondary)' }}>
                        Website ↗
                      </a>
                    )}
                  </div>
                </div>
              )
            })}
            {filtered.length === 0 && !loading && (
              <div className="col-span-3 py-8 text-center text-sm" style={{ color: 'var(--text-muted)' }}>
                No apps match. Try <button onClick={() => setTab('discover')} className="underline" style={{ color: 'var(--accent-primary)' }}>AI Discover</button> to find more.
              </div>
            )}
          </div>
        </>
      )}

      {tab === 'deployed' && (
        <DeployedTab deployed={deployed} catalogApps={apps} allTags={allTags} tagMap={tagMap} globalTag={globalTag} onRefresh={load} onTagsChanged={loadTags} />
      )}

      {tab === 'external' && (
        <ExternalTab onAdopted={load} />
      )}

      {tab === 'custom' && (
        <CustomDeployTab onDeployed={() => { load(); setTab('deployed') }} />
      )}

      {tab === 'discover' && (
        <DiscoverTab
          catalogApps={apps}
          deployedIds={deployedIds}
          dockerAvailable={dockerAvailable}
          onDeploy={deploy}
          deploying={deploying}
        />
      )}
    </div>
    {proxmoxDeployApp && (
      <DeployToProxmoxModal app={proxmoxDeployApp} onClose={() => setProxmoxDeployApp(null)} />
    )}
    {configModalApp && (
      <DeployConfigModal
        app={configModalApp}
        onClose={() => setConfigModalApp(null)}
        onDeployed={async () => { setConfigModalApp(null); await load(); setTab('deployed') }}
      />
    )}
    </>
  )
}

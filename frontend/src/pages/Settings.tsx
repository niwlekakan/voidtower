import { useState, useEffect } from 'react'
import { useAuthStore } from '@/store/auth'
import { api, ApiClientError } from '@/api/client'
import type { UserRecord } from '@/api/types'
import Button from '@/components/ui/Button'
import { notify } from '@/store/notifications'
import { Trash2, UserPlus, Bell, Send, Key, Globe } from 'lucide-react'

export default function SettingsPage() {
  const currentUser = useAuthStore((s) => s.user)
  const isAdmin = currentUser?.role === 'owner' || currentUser?.role === 'admin'

  return (
    <div className="space-y-6 max-w-2xl">
      <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Settings</h1>

      {/* General / instance */}
      {isAdmin && <GeneralSection />}

      {/* Dashboard / Weather */}
      <WeatherLocationSection />

      {/* AI integrations */}
      <AIIntegrationsSection />

      {/* Notification webhooks — admin/owner only */}
      {isAdmin && <NotificationsSection />}

      {/* Change own password */}
      <AccountSection />

      {/* User management — admin/owner only */}
      {isAdmin && <UsersSection currentUserId={currentUser?.id ?? ''} />}
    </div>
  )
}

function GeneralSection() {
  const [name, setName] = useState('')
  const [saved, setSaved] = useState(false)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    fetch('/api/settings/general', { credentials: 'include' })
      .then(r => r.ok ? r.json() : null)
      .then((d: { instance_name: string } | null) => {
        if (d) setName(d.instance_name)
      })
      .finally(() => setLoading(false))
  }, [])

  const save = async () => {
    try {
      await fetch('/api/settings/general', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ instance_name: name.trim() || 'VoidTower' }),
      })
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
      document.title = name.trim() || 'VoidTower'
    } catch {
      notify.error('Failed to save')
    }
  }

  return (
    <div className="card space-y-3">
      <div className="flex items-center gap-2">
        <Globe size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>General</h2>
      </div>
      <div>
        <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Instance name</label>
        <input
          value={loading ? '' : name}
          onChange={e => setName(e.target.value)}
          placeholder="VoidTower"
          disabled={loading}
          className="w-full px-3 py-2 rounded text-sm outline-none"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
        />
        <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
          Shown in the browser tab title.
        </p>
      </div>
      <button onClick={save} className="px-3 py-1.5 rounded text-xs font-medium transition-colors"
        style={{ background: saved ? 'var(--accent-success-subtle)' : 'var(--accent-primary)', color: saved ? 'var(--accent-success)' : '#fff' }}>
        {saved ? 'Saved ✓' : 'Save'}
      </button>
    </div>
  )
}

function WeatherLocationSection() {
  const [lat, setLat] = useState(() => localStorage.getItem('vt-weather-lat') ?? '')
  const [lon, setLon] = useState(() => localStorage.getItem('vt-weather-lon') ?? '')
  const [saved, setSaved] = useState(false)

  const save = () => {
    if (lat && lon) {
      localStorage.setItem('vt-weather-lat', lat)
      localStorage.setItem('vt-weather-lon', lon)
    } else {
      localStorage.removeItem('vt-weather-lat')
      localStorage.removeItem('vt-weather-lon')
    }
    setSaved(true)
    setTimeout(() => setSaved(false), 2000)
  }

  const detect = () => {
    navigator.geolocation?.getCurrentPosition(({ coords }) => {
      setLat(coords.latitude.toFixed(4))
      setLon(coords.longitude.toFixed(4))
    })
  }

  return (
    <div className="card space-y-3">
      <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Weather Location</h2>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Override the coordinates used by the dashboard weather widget. Leave blank to use browser geolocation each time.
      </p>
      <div className="grid grid-cols-2 gap-3">
        {[['Latitude', lat, setLat], ['Longitude', lon, setLon]].map(([label, val, set]) => (
          <div key={label as string}>
            <label className="block text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>{label as string}</label>
            <input
              value={val as string}
              onChange={(e) => (set as (v: string) => void)(e.target.value)}
              placeholder={label === 'Latitude' ? '59.9139' : '10.7522'}
              className="w-full px-3 py-1.5 rounded text-sm font-mono outline-none"
              style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
            />
          </div>
        ))}
      </div>
      <div className="flex gap-2">
        <button onClick={save} className="px-3 py-1.5 rounded text-xs font-medium transition-colors"
          style={{ background: saved ? 'var(--accent-success-subtle)' : 'var(--accent-primary)', color: saved ? 'var(--accent-success)' : '#fff' }}>
          {saved ? 'Saved ✓' : 'Save'}
        </button>
        <button onClick={detect} className="px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
          Detect from browser
        </button>
        {(lat || lon) && (
          <button onClick={() => { setLat(''); setLon(''); localStorage.removeItem('vt-weather-lat'); localStorage.removeItem('vt-weather-lon') }}
            className="px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
            style={{ color: 'var(--text-muted)' }}>
            Clear
          </button>
        )}
      </div>
    </div>
  )
}

const AI_WORKSPACE_KEY = 'vt-ai-workspace-url'
const AI_LLM_KEY = 'vt-ai-llm-endpoint'

interface AiSettings {
  url: string | null
  port: number
  proxy_active: boolean
}

function AIIntegrationsSection() {
  const [workspaceUrl, setWorkspaceUrl] = useState(() => localStorage.getItem(AI_WORKSPACE_KEY) ?? '')
  const [llmEndpoint, setLlmEndpoint] = useState(() => localStorage.getItem(AI_LLM_KEY) ?? '')
  const [proxyPort, setProxyPort] = useState('7001')
  const [saving, setSaving] = useState(false)
  const [aiSettings, setAiSettings] = useState<AiSettings | null>(null)

  useEffect(() => {
    fetch('/api/settings/ai-url', { credentials: 'include' })
      .then(r => r.ok ? r.json() : null)
      .then((s: AiSettings | null) => {
        if (s) {
          setAiSettings(s)
          if (s.url) {
            setWorkspaceUrl(s.url)
            localStorage.setItem(AI_WORKSPACE_KEY, s.url)
          }
          setProxyPort(String(s.port))
        }
      })
      .catch(() => {})
  }, [])

  const [saveResult, setSaveResult] = useState<{ ok: boolean; message: string } | null>(null)

  const save = async () => {
    setSaving(true)
    setSaveResult(null)
    const url = workspaceUrl.trim() || null
    const port = parseInt(proxyPort, 10) || 7001

    url ? localStorage.setItem(AI_WORKSPACE_KEY, url) : localStorage.removeItem(AI_WORKSPACE_KEY)
    llmEndpoint.trim()
      ? localStorage.setItem(AI_LLM_KEY, llmEndpoint.trim())
      : localStorage.removeItem(AI_LLM_KEY)

    try {
      const r = await fetch('/api/settings/ai-url', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url, port }),
      }).then(res => res.json())

      setAiSettings({ url, port, proxy_active: r.proxy_active ?? false })

      if (r.proxy_active) {
        setSaveResult({ ok: true, message: `Proxy active on port ${port} — AI tab will use http://${window.location.hostname}:${port}/` })
      } else if (r.nginx_error) {
        setSaveResult({ ok: false, message: `URL saved, but proxy setup failed: ${r.nginx_error}` })
      } else {
        setSaveResult({ ok: true, message: 'Settings saved.' })
      }
    } catch (e) {
      setSaveResult({ ok: false, message: `Request failed: ${e}` })
    } finally {
      setSaving(false)
    }
  }

  const proxyUrl = aiSettings?.proxy_active
    ? `http://${window.location.hostname}:${aiSettings.port}`
    : null

  return (
    <div className="card space-y-4">
      <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Integrations — AI</h2>

      {proxyUrl && !saveResult && (
        <div className="flex items-start gap-2 p-2.5 rounded text-xs"
             style={{ background: 'var(--accent-success-subtle)', border: '1px solid var(--accent-success)', color: 'var(--accent-success)' }}>
          <span style={{ flexShrink: 0 }}>✓</span>
          <div>Embed proxy active — AI tab uses <span className="font-mono">{proxyUrl}</span></div>
        </div>
      )}

      {saveResult && (
        <div className="p-2.5 rounded text-xs"
             style={{
               background: saveResult.ok ? 'var(--accent-success-subtle)' : 'var(--accent-danger-subtle)',
               border: `1px solid ${saveResult.ok ? 'var(--accent-success)' : 'var(--accent-danger)'}`,
               color: saveResult.ok ? 'var(--accent-success)' : 'var(--accent-danger)',
             }}>
          {saveResult.message}
        </div>
      )}

      <div className="space-y-3">
        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>
            AI workspace URL
          </label>
          <input
            type="url"
            value={workspaceUrl}
            onChange={(e) => setWorkspaceUrl(e.target.value)}
            placeholder="http://localhost:7000"
            className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
            style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
          />
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
            Odysseus, Open WebUI, or any AI frontend. VoidTower creates an nginx proxy so it embeds cleanly.
          </p>
        </div>

        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>
            Proxy port <span style={{ color: 'var(--text-muted)' }}>(nginx listens here, must be reachable from your browser)</span>
          </label>
          <input
            type="number"
            min="1024"
            max="65535"
            value={proxyPort}
            onChange={(e) => setProxyPort(e.target.value)}
            className="w-32 px-3 py-2 rounded text-sm outline-none font-mono"
            style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
          />
        </div>

        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>
            LLM endpoint (OpenAI-compatible /v1)
          </label>
          <input
            type="url"
            value={llmEndpoint}
            onChange={(e) => setLlmEndpoint(e.target.value)}
            placeholder="http://127.0.0.1:8080/v1"
            className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
            style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
          />
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
            Used by VoidTower's built-in AI tools.
          </p>
        </div>
      </div>

      <Button size="sm" variant="primary" onClick={save} disabled={saving}>
        {saving ? 'Saving…' : 'Save'}
      </Button>
    </div>
  )
}

function NotificationsSection() {
  const [ntfy, setNtfy] = useState('')
  const [discord, setDiscord] = useState('')
  const [slack, setSlack] = useState('')
  const [saving, setSaving] = useState(false)
  const [testing, setTesting] = useState<string | null>(null)

  useEffect(() => {
    fetch('/api/settings/notifications', { credentials: 'include' })
      .then(r => r.ok ? r.json() : null)
      .then((d: { ntfy_url?: string; discord_webhook?: string; slack_webhook?: string } | null) => {
        if (!d) return
        setNtfy(d.ntfy_url ?? '')
        setDiscord(d.discord_webhook ?? '')
        setSlack(d.slack_webhook ?? '')
      })
  }, [])

  const save = async () => {
    setSaving(true)
    try {
      await fetch('/api/settings/notifications', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          ntfy_url:        ntfy.trim() || null,
          discord_webhook: discord.trim() || null,
          slack_webhook:   slack.trim() || null,
        }),
      })
      notify.success('Notification settings saved')
    } catch {
      notify.error('Failed to save')
    } finally {
      setSaving(false)
    }
  }

  const test = async (channel: string) => {
    setTesting(channel)
    try {
      const r = await fetch('/api/settings/notifications/test', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ channel }),
      }).then(res => res.json()) as { ok: boolean; error?: string }
      if (r.ok) notify.success(`Test sent to ${channel}`)
      else notify.error(`Test failed: ${r.error ?? 'unknown error'}`)
    } catch {
      notify.error('Request failed')
    } finally {
      setTesting(null)
    }
  }

  const fields: Array<{ label: string; key: string; val: string; set: (v: string) => void; placeholder: string; channel: string }> = [
    { label: 'ntfy.sh topic URL', key: 'ntfy', val: ntfy, set: setNtfy, placeholder: 'https://ntfy.sh/my-topic', channel: 'ntfy' },
    { label: 'Discord webhook URL', key: 'discord', val: discord, set: setDiscord, placeholder: 'https://discord.com/api/webhooks/…', channel: 'discord' },
    { label: 'Slack webhook URL', key: 'slack', val: slack, set: setSlack, placeholder: 'https://hooks.slack.com/services/…', channel: 'slack' },
  ]

  return (
    <div className="card space-y-4">
      <div className="flex items-center gap-2">
        <Bell size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Notification Webhooks</h2>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Configure one or more channels. Notifications fire when alerts are created or services go down.
      </p>
      <div className="space-y-3">
        {fields.map(({ label, key, val, set, placeholder, channel }) => (
          <div key={key}>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>{label}</label>
            <div className="flex gap-2">
              <input
                type="url"
                value={val}
                onChange={e => set(e.target.value)}
                placeholder={placeholder}
                className="flex-1 px-3 py-2 rounded text-sm outline-none font-mono min-w-0"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              />
              <button
                disabled={!val.trim() || testing === channel}
                onClick={() => test(channel)}
                className="flex items-center gap-1.5 px-2.5 py-1.5 rounded text-xs whitespace-nowrap transition-colors disabled:opacity-40"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
              >
                <Send size={11} />
                {testing === channel ? 'Sending…' : 'Test'}
              </button>
            </div>
          </div>
        ))}
      </div>
      <Button size="sm" variant="primary" onClick={save} disabled={saving}>
        {saving ? 'Saving…' : 'Save'}
      </Button>
    </div>
  )
}

function AccountSection() {
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [confirm, setConfirm] = useState('')
  const [saving, setSaving] = useState(false)

  const mismatch = next && confirm && next !== confirm
  const valid = current && next && next === confirm && next.length >= 8

  const save = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!valid) return
    setSaving(true)
    try {
      await api.users.changePassword(next)
      notify.success('Password changed')
      setCurrent(''); setNext(''); setConfirm('')
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to change password')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="card space-y-3">
      <div className="flex items-center gap-2">
        <Key size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Change Password</h2>
      </div>
      <form onSubmit={save} className="space-y-3">
        {[
          { label: 'Current password', val: current, set: setCurrent, placeholder: '' },
          { label: 'New password', val: next, set: setNext, placeholder: 'Min 8 characters' },
          { label: 'Confirm new password', val: confirm, set: setConfirm, placeholder: '' },
        ].map(({ label, val, set, placeholder }) => (
          <div key={label}>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>{label}</label>
            <input
              type="password"
              value={val}
              onChange={e => set(e.target.value)}
              placeholder={placeholder}
              className="w-full px-3 py-2 rounded text-sm outline-none"
              style={{
                background: 'var(--bg-elevated)',
                border: `1px solid ${mismatch && (label.includes('Confirm') || label.includes('New')) ? 'var(--accent-danger)' : 'var(--border-default)'}`,
                color: 'var(--text-primary)',
              }}
            />
          </div>
        ))}
        {mismatch && (
          <p className="text-xs" style={{ color: 'var(--accent-danger)' }}>Passwords do not match</p>
        )}
        <Button size="sm" variant="primary" type="submit" loading={saving} disabled={!valid || saving}>
          Update password
        </Button>
      </form>
    </div>
  )
}

function UsersSection({ currentUserId }: { currentUserId: string }) {
  const [users, setUsers] = useState<UserRecord[]>([])
  const [loading, setLoading] = useState(true)
  const [newUsername, setNewUsername] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [newRole, setNewRole] = useState('viewer')
  const [creating, setCreating] = useState(false)
  const [showForm, setShowForm] = useState(false)

  const refresh = () => {
    setLoading(true)
    api.users.list()
      .then((r) => setUsers(r.users))
      .catch(() => notify.error('Failed to load users'))
      .finally(() => setLoading(false))
  }

  useEffect(() => { refresh() }, [])

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault()
    setCreating(true)
    try {
      await api.users.create(newUsername, newPassword, newRole)
      notify.success(`User "${newUsername}" created`)
      setNewUsername(''); setNewPassword(''); setNewRole('viewer')
      setShowForm(false)
      refresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to create user')
    } finally {
      setCreating(false)
    }
  }

  const handleDelete = async (u: UserRecord) => {
    if (!confirm(`Delete user "${u.username}"? This cannot be undone.`)) return
    try {
      await api.users.delete(u.id)
      notify.success(`User "${u.username}" deleted`)
      refresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to delete user')
    }
  }

  const ROLE_COLORS: Record<string, string> = {
    owner: 'var(--accent-primary)',
    admin: 'var(--accent-warning)',
    operator: 'var(--accent-secondary)',
    viewer: 'var(--text-muted)',
  }

  return (
    <div className="card space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Users</h2>
        <button
          onClick={() => setShowForm((v) => !v)}
          className="flex items-center gap-1.5 px-2.5 py-1.5 rounded text-xs transition-colors hover:opacity-80"
          style={{ background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)' }}
        >
          <UserPlus size={12} />
          Add user
        </button>
      </div>

      {showForm && (
        <form onSubmit={handleCreate} className="space-y-2 p-3 rounded" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
          <div className="grid grid-cols-3 gap-2">
            <input
              placeholder="Username"
              value={newUsername}
              onChange={(e) => setNewUsername(e.target.value)}
              className="px-2 py-1.5 rounded text-xs outline-none font-mono"
              style={{ background: 'var(--bg-root)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              required
            />
            <input
              placeholder="Password"
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              className="px-2 py-1.5 rounded text-xs outline-none font-mono"
              style={{ background: 'var(--bg-root)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              required
            />
            <select
              value={newRole}
              onChange={(e) => setNewRole(e.target.value)}
              className="px-2 py-1.5 rounded text-xs outline-none"
              style={{ background: 'var(--bg-root)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
            >
              <option value="viewer">viewer</option>
              <option value="operator">operator</option>
              <option value="admin">admin</option>
            </select>
          </div>
          <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
            User will be prompted to change their password on first login.
          </p>
          <div className="flex gap-2">
            <Button size="sm" variant="primary" type="submit" loading={creating}>Create</Button>
            <Button size="sm" variant="ghost" type="button" onClick={() => setShowForm(false)}>Cancel</Button>
          </div>
        </form>
      )}

      {loading ? (
        <p className="text-xs py-2" style={{ color: 'var(--text-muted)' }}>Loading…</p>
      ) : (
        <div className="divide-y" style={{ borderColor: 'var(--border-subtle)' }}>
          {users.map((u) => (
            <div key={u.id} className="flex items-center gap-3 py-2.5">
              <span className="flex-1 text-sm font-mono" style={{ color: 'var(--text-primary)' }}>
                {u.username}
                {u.id === currentUserId && (
                  <span className="ml-1.5 text-xs" style={{ color: 'var(--text-muted)' }}>(you)</span>
                )}
              </span>
              <span className="text-xs" style={{ color: ROLE_COLORS[u.role] ?? 'var(--text-muted)' }}>
                {u.role}
              </span>
              {u.force_password_change && (
                <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: 'var(--accent-warning-subtle)', color: 'var(--accent-warning)' }}>
                  must change pw
                </span>
              )}
              {u.id !== currentUserId && (
                <button
                  onClick={() => handleDelete(u)}
                  className="p-1 rounded transition-colors hover:opacity-80"
                  style={{ color: 'var(--accent-danger)' }}
                  title={`Delete ${u.username}`}
                >
                  <Trash2 size={13} />
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

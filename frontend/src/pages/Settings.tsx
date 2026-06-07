import { useState, useEffect, useRef } from 'react'
import { useAuthStore } from '@/store/auth'
import { api, ApiClientError } from '@/api/client'
import type { UserRecord } from '@/api/types'
import Button from '@/components/ui/Button'
import { notify } from '@/store/notifications'
import { Trash2, UserPlus, Bell, Send, Key, Globe, RefreshCw, Download, GitBranch, Monitor, Plus, Webhook, ToggleLeft, ToggleRight, Cpu, Stethoscope, Palette, AlertTriangle, Upload, Copy, ShieldOff, Eye, EyeOff, Navigation } from 'lucide-react'
import { useThemeStore, type UiMode } from '@/store/theme'
import { setDeviceTierOverride, type DeviceTier } from '@/aios/hooks/useDeviceTier'
import { Accessibility } from 'lucide-react'
import { useNavConfigStore, DEFAULT_NAV_ITEMS, resolvedNavItems, type NavItem } from '@/store/navConfig'
import { ICON_MAP } from '@/aios/AiosDock'

function AppearanceSection() {
  const { uiMode, setUiMode } = useThemeStore()
  const [deviceOverride, setDeviceOverride] = useState<DeviceTier | ''>(() =>
    (localStorage.getItem('vt-device-tier') as DeviceTier | null) ?? ''
  )

  const handleDeviceOverride = (val: DeviceTier | '') => {
    setDeviceOverride(val)
    setDeviceTierOverride(val || null)
  }

  return (
    <div className="card space-y-4">
      <div className="flex items-center gap-2">
        <Monitor size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Appearance</h2>
      </div>

      {/* UI Mode toggle */}
      <div>
        <label className="block text-xs mb-2" style={{ color: 'var(--text-secondary)' }}>Interface mode</label>
        <div className="flex gap-2">
          {(['tower', 'void'] as UiMode[]).map((mode) => (
            <button
              key={mode}
              onClick={() => setUiMode(mode)}
              className="px-4 py-2 rounded text-xs font-medium transition-colors capitalize"
              style={{
                background: uiMode === mode ? 'var(--accent-primary)' : 'var(--bg-elevated)',
                color: uiMode === mode ? '#fff' : 'var(--text-secondary)',
                border: `1px solid ${uiMode === mode ? 'var(--accent-primary)' : 'var(--border-default)'}`,
              }}
            >
              {mode === 'void' ? 'Void Mode' : 'Tower Mode'}
            </button>
          ))}
        </div>
        <p className="mt-1.5 text-xs" style={{ color: 'var(--text-muted)' }}>
          {uiMode === 'void'
            ? 'Floating panels, spatial layout, always-on AI command bar.'
            : 'Standard sidebar navigation.'}
        </p>
      </div>

      {/* Device tier override */}
      <div>
        <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Device mode override</label>
        <select
          value={deviceOverride}
          onChange={(e) => handleDeviceOverride(e.target.value as DeviceTier | '')}
          className="px-3 py-2 rounded text-sm outline-none"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
        >
          <option value="">Auto-detect</option>
          <option value="phone">Phone</option>
          <option value="tablet">Tablet</option>
          <option value="desktop">Desktop</option>
          <option value="large">Large display</option>
          <option value="tv">TV</option>
          <option value="kiosk">Kiosk</option>
        </select>
        <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
          Override automatic tier detection. Takes effect after page reload.
        </p>
      </div>
    </div>
  )
}

function AccessibilitySection() {
  const { a11y, setA11y } = useThemeStore()

  const Row = ({ label, desc, field }: { label: string; desc: string; field: keyof typeof a11y }) => (
    <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 16 }}>
      <div style={{ flex: 1 }}>
        <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{label}</div>
        <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{desc}</div>
      </div>
      <button
        role="switch"
        aria-checked={a11y[field]}
        onClick={() => setA11y({ [field]: !a11y[field] })}
        style={{
          flexShrink: 0,
          width: 36, height: 20, borderRadius: 10, border: 'none', cursor: 'pointer',
          background: a11y[field] ? 'var(--accent-primary)' : 'var(--bg-elevated)',
          position: 'relative', transition: 'background 0.2s',
        }}
      >
        <span style={{
          position: 'absolute', top: 2,
          left: a11y[field] ? 18 : 2,
          width: 16, height: 16, borderRadius: '50%',
          background: '#fff', transition: 'left 0.2s',
        }} />
      </button>
    </div>
  )

  return (
    <div className="card space-y-4">
      <div className="flex items-center gap-2">
        <Accessibility size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Accessibility</h2>
      </div>
      <Row
        field="reduceMotion"
        label="Reduce motion"
        desc="Disable all transitions and animations throughout the UI."
      />
      <Row
        field="largeControls"
        label="Large controls"
        desc="Increase minimum tap target size and font size for buttons and inputs."
      />
      <Row
        field="preferStacked"
        label="Prefer stacked layout"
        desc="Force single-panel stacked layout in Void Mode (like phone tier)."
      />
    </div>
  )
}

function QuickLinksCard() {
  const links = [
    { href: '/capabilities', label: 'Capabilities', icon: Cpu,         desc: 'Manage system capabilities and feature flags' },
    { href: '/diagnostics',  label: 'Diagnostics',  icon: Stethoscope, desc: 'Run health checks and view diagnostics' },
    { href: '/themes',       label: 'Themes',       icon: Palette,     desc: 'Customize the appearance and color scheme' },
  ]
  return (
    <div className="card space-y-3">
      <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Quick Links</h2>
      <div className="grid grid-cols-1 gap-2" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))' }}>
        {links.map(({ href, label, icon: Icon, desc }) => (
          <a
            key={href}
            href={href}
            style={{
              display: 'flex', alignItems: 'flex-start', gap: 10,
              padding: '10px 12px', borderRadius: 8, textDecoration: 'none',
              background: 'var(--bg-elevated)',
              border: '1px solid var(--border-subtle)',
              transition: 'border-color 0.15s, background 0.15s',
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLElement).style.borderColor = 'var(--accent-primary)'
              ;(e.currentTarget as HTMLElement).style.background = 'var(--accent-primary-subtle, rgba(139,92,246,0.08))'
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.borderColor = 'var(--border-subtle)'
              ;(e.currentTarget as HTMLElement).style.background = 'var(--bg-elevated)'
            }}
          >
            <Icon size={15} style={{ color: 'var(--accent-primary)', flexShrink: 0, marginTop: 1 }} />
            <div>
              <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{label}</div>
              <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{desc}</div>
            </div>
          </a>
        ))}
      </div>
    </div>
  )
}

// ─── Preferences section ─────────────────────────────────────────────────────

function PreferencesSection() {
  const ls = (key: string, def: string) => localStorage.getItem(key) ?? def
  const [pollInterval, setPollInterval] = useState(() => ls('vt-poll-interval', '15'))
  const [clockFormat,  setClockFormat]  = useState(() => ls('vt-clock-format', '24h'))
  const [confirmDestructive, setConfirmDestructive] = useState(() => ls('vt-confirm-destructive', 'true') === 'true')
  const [saved, setSaved] = useState(false)

  const save = () => {
    localStorage.setItem('vt-poll-interval', pollInterval)
    localStorage.setItem('vt-clock-format', clockFormat)
    localStorage.setItem('vt-confirm-destructive', String(confirmDestructive))
    setSaved(true)
    setTimeout(() => setSaved(false), 2000)
  }

  const row = (label: string, desc: string, control: React.ReactNode) => (
    <div className="flex items-center justify-between gap-4">
      <div>
        <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{label}</div>
        <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{desc}</div>
      </div>
      {control}
    </div>
  )

  const sel = (value: string, onChange: (v: string) => void, opts: [string, string][]) => (
    <select value={value} onChange={e => onChange(e.target.value)}
      className="px-2 py-1.5 rounded text-xs outline-none"
      style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}>
      {opts.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
    </select>
  )

  const tog = (on: boolean, onChange: (v: boolean) => void) => (
    <button role="switch" aria-checked={on} onClick={() => onChange(!on)} style={{
      flexShrink: 0, width: 36, height: 20, borderRadius: 10, border: 'none', cursor: 'pointer', position: 'relative',
      background: on ? 'var(--accent-primary)' : 'var(--bg-elevated)',
    }}>
      <span style={{ position: 'absolute', top: 2, left: on ? 18 : 2, width: 16, height: 16, borderRadius: '50%', background: '#fff', transition: 'left 0.2s' }} />
    </button>
  )

  return (
    <div className="card space-y-4">
      <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Preferences</h2>
      {row('Panel refresh interval', 'How often live data panels poll the backend.',
        sel(pollInterval, setPollInterval, [['5','5 seconds'],['15','15 seconds'],['30','30 seconds'],['60','1 minute']]))}
      {row('Clock format', 'Time display in the status bar and widgets.',
        sel(clockFormat, setClockFormat, [['24h','24-hour'],['12h','12-hour']]))}
      {row('Confirm destructive actions', 'Show a confirmation prompt before deletes, stops, and resets.',
        tog(confirmDestructive, setConfirmDestructive))}
      <button onClick={save} className="px-3 py-1.5 rounded text-xs font-medium"
        style={{ background: saved ? 'var(--accent-success-subtle)' : 'var(--accent-primary)', color: saved ? 'var(--accent-success)' : '#fff' }}>
        {saved ? 'Saved ✓' : 'Save preferences'}
      </button>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>Preferences are stored locally in your browser.</p>
    </div>
  )
}

// ─── Developer section ────────────────────────────────────────────────────────

function DeveloperSection() {
  const ls = (k: string) => localStorage.getItem(k) === 'true'
  const [latencyBadge,   setLatencyBadge]   = useState(() => ls('vt-show-latency'))
  const [debugBorders,   setDebugBorders]   = useState(() => ls('vt-debug-panels'))
  const [forceMode,      setForceMode]      = useState(() => localStorage.getItem('vt-force-mode') ?? '')

  const apply = (key: string, val: boolean | string) => {
    if (typeof val === 'boolean') {
      if (val) localStorage.setItem(key, 'true'); else localStorage.removeItem(key)
    } else {
      if (val) localStorage.setItem(key, val); else localStorage.removeItem(key)
    }
    if (key === 'vt-debug-panels') document.documentElement.classList.toggle('debug-panel-borders', val as boolean)
  }

  const tog = (on: boolean, onChange: (v: boolean) => void, key: string) => (
    <button role="switch" aria-checked={on} onClick={() => { const next = !on; onChange(next); apply(key, next) }} style={{
      flexShrink: 0, width: 36, height: 20, borderRadius: 10, border: 'none', cursor: 'pointer', position: 'relative',
      background: on ? 'var(--accent-primary)' : 'var(--bg-elevated)',
    }}>
      <span style={{ position: 'absolute', top: 2, left: on ? 18 : 2, width: 16, height: 16, borderRadius: '50%', background: '#fff', transition: 'left 0.2s' }} />
    </button>
  )

  const row = (label: string, desc: string, control: React.ReactNode) => (
    <div className="flex items-center justify-between gap-4">
      <div>
        <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{label}</div>
        <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{desc}</div>
      </div>
      {control}
    </div>
  )

  return (
    <div className="card space-y-4" style={{ borderColor: 'var(--border-default)' }}>
      <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Developer</h2>
      {row('API latency badge', 'Show response time (ms) on panel headers.',
        tog(latencyBadge, setLatencyBadge, 'vt-show-latency'))}
      {row('Debug panel borders', 'Outline all panels in red for layout debugging.',
        tog(debugBorders, setDebugBorders, 'vt-debug-panels'))}
      {row('Force UI mode',
        'Override auto-detection. Takes effect after reload.',
        <select value={forceMode} onChange={e => { setForceMode(e.target.value); apply('vt-force-mode', e.target.value) }}
          className="px-2 py-1.5 rounded text-xs outline-none"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}>
          <option value="">Auto-detect</option>
          <option value="tower">Tower Mode</option>
          <option value="void">Void Mode</option>
        </select>
      )}
    </div>
  )
}

// ─── Disaster Recovery section ────────────────────────────────────────────────

function DisasterRecoverySection() {
  // Export
  const [exporting, setExporting] = useState(false)

  // Import
  const [importing, setImporting] = useState(false)
  const [importResult, setImportResult] = useState<{ proxies: number; automations: number; tags: number } | null>(null)

  // Emergency disable
  const [disableConfirm, setDisableConfirm] = useState(false)
  const [disabling, setDisabling] = useState(false)
  const [disableResult, setDisableResult] = useState<{ odysseus: boolean; automations: number } | null>(null)

  // Reset admin
  const [resetInput, setResetInput] = useState('')
  const [resetting, setResetting] = useState(false)
  const [tempPassword, setTempPassword] = useState<{ username: string; password: string } | null>(null)
  const [copied, setCopied] = useState(false)

  const doExport = async () => {
    setExporting(true)
    try {
      const res = await fetch('/api/disaster/export-config', {
        method: 'POST',
        credentials: 'include',
      })
      if (!res.ok) throw new Error(await res.text())
      const cd = res.headers.get('Content-Disposition') ?? ''
      const match = cd.match(/filename="?([^"]+)"?/)
      const filename = match?.[1] ?? 'voidtower-config.json'
      const blob = await res.blob()
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = filename
      a.click()
      URL.revokeObjectURL(url)
    } catch (err) {
      notify.error(err instanceof Error ? err.message : 'Export failed')
    } finally {
      setExporting(false)
    }
  }

  const doImport = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    setImporting(true)
    setImportResult(null)
    try {
      const text = await file.text()
      const payload = JSON.parse(text)
      const res = await fetch('/api/disaster/import-config', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      })
      const data = await res.json()
      if (!res.ok || !data.ok) throw new Error(data.error ?? 'Import failed')
      setImportResult(data.applied)
      notify.success('Config imported successfully')
    } catch (err) {
      notify.error(err instanceof Error ? err.message : 'Import failed')
    } finally {
      setImporting(false)
      e.target.value = ''
    }
  }

  const doEmergencyDisable = async () => {
    setDisabling(true)
    setDisableConfirm(false)
    try {
      const res = await fetch('/api/disaster/emergency-disable', {
        method: 'POST',
        credentials: 'include',
      })
      const data = await res.json()
      if (!res.ok || !data.ok) throw new Error(data.error ?? 'Failed')
      setDisableResult(data.disabled)
      notify.success('Emergency disable applied')
    } catch (err) {
      notify.error(err instanceof Error ? err.message : 'Emergency disable failed')
    } finally {
      setDisabling(false)
    }
  }

  const doResetAdmin = async () => {
    if (resetInput !== 'RESET') return
    setResetting(true)
    setTempPassword(null)
    try {
      const res = await fetch('/api/disaster/emergency-reset-admin', {
        method: 'POST',
        credentials: 'include',
      })
      const data = await res.json()
      if (!res.ok || !data.ok) throw new Error(data.error ?? 'Failed')
      setTempPassword({ username: data.username, password: data.temporary_password })
      setResetInput('')
      notify.success('Admin password reset')
    } catch (err) {
      notify.error(err instanceof Error ? err.message : 'Reset failed')
    } finally {
      setResetting(false)
    }
  }

  const copyPassword = async () => {
    if (!tempPassword) return
    await navigator.clipboard.writeText(tempPassword.password)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div
      className="card space-y-5"
      style={{ borderColor: 'var(--accent-danger, #ef4444)44' }}
    >
      <div className="flex items-center gap-2">
        <AlertTriangle size={15} style={{ color: 'var(--accent-danger, #ef4444)' }} />
        <h2 className="text-sm font-semibold" style={{ color: 'var(--accent-danger, #ef4444)' }}>
          Disaster Recovery
        </h2>
      </div>

      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Emergency controls for recovering a broken instance. Use with care — these actions are destructive and audit-logged.
      </p>

      {/* Export config */}
      <div className="space-y-1.5">
        <p className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Export configuration</p>
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
          Downloads a JSON snapshot of proxy rules, automation jobs, tags, and general settings. Does not include users, secrets, or session data.
        </p>
        <Button variant="secondary" size="sm" onClick={doExport} disabled={exporting}>
          {exporting
            ? <><RefreshCw size={13} className="animate-spin mr-1.5" />Exporting…</>
            : <><Download size={13} className="mr-1.5" />Export Config</>}
        </Button>
      </div>

      {/* Import config */}
      <div className="space-y-1.5">
        <p className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Import configuration</p>
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
          Load a previously exported JSON file. Upserts proxy rules, automation jobs, and tags. Alert rules are skipped for safety.
        </p>
        <label
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium cursor-pointer select-none"
          style={{
            background: 'var(--bg-elevated)',
            border: '1px solid var(--border-default)',
            color: importing ? 'var(--text-muted)' : 'var(--text-primary)',
            cursor: importing ? 'not-allowed' : 'pointer',
          }}
        >
          {importing
            ? <><RefreshCw size={13} className="animate-spin" />Importing…</>
            : <><Upload size={13} />Import Config</>}
          <input
            type="file"
            accept=".json"
            className="sr-only"
            onChange={doImport}
            disabled={importing}
          />
        </label>
        {importResult && (
          <div
            className="rounded px-3 py-2 text-xs"
            style={{
              background: 'var(--accent-success)18',
              border: '1px solid var(--accent-success)44',
              color: 'var(--accent-success)',
            }}
          >
            Applied: {importResult.proxies} {importResult.proxies === 1 ? 'proxy' : 'proxies'},{' '}
            {importResult.automations} {importResult.automations === 1 ? 'automation' : 'automations'},{' '}
            {importResult.tags} {importResult.tags === 1 ? 'tag' : 'tags'}
          </div>
        )}
      </div>

      {/* Emergency disable */}
      <div className="space-y-1.5">
        <p className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Emergency disable all AI &amp; automations</p>
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
          Immediately disables Odysseus AI access and turns off all enabled automation jobs.
        </p>
        {disableResult ? (
          <div
            className="rounded px-3 py-2 text-xs"
            style={{
              background: 'var(--accent-warning, #f59e0b)18',
              border: '1px solid var(--accent-warning, #f59e0b)44',
              color: 'var(--accent-warning, #f59e0b)',
            }}
          >
            Disabled: Odysseus AI access, {disableResult.automations}{' '}
            {disableResult.automations === 1 ? 'automation' : 'automations'}
          </div>
        ) : disableConfirm ? (
          <div className="flex gap-2">
            <Button variant="danger" size="sm" onClick={doEmergencyDisable} disabled={disabling}>
              {disabling ? <><RefreshCw size={13} className="animate-spin mr-1.5" />Disabling…</> : 'Confirm — Disable Now'}
            </Button>
            <Button variant="secondary" size="sm" onClick={() => setDisableConfirm(false)}>Cancel</Button>
          </div>
        ) : (
          <Button variant="danger" size="sm" onClick={() => setDisableConfirm(true)}>
            <ShieldOff size={13} className="mr-1.5" />Emergency Disable
          </Button>
        )}
      </div>

      {/* Reset admin password */}
      <div className="space-y-1.5">
        <p className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Reset admin password</p>
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
          Resets the oldest owner account to a random 16-character password. Type <code>RESET</code> to confirm.
        </p>
        {tempPassword ? (
          <div className="space-y-2">
            <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
              Temporary password for <strong style={{ color: 'var(--text-primary)' }}>{tempPassword.username}</strong> — shown once:
            </p>
            <div className="flex items-center gap-2">
              <code
                className="px-3 py-1.5 rounded text-sm font-mono select-all"
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border-default)',
                  color: 'var(--text-primary)',
                }}
              >
                {tempPassword.password}
              </code>
              <Button variant="secondary" size="sm" onClick={copyPassword}>
                <Copy size={13} className="mr-1" />
                {copied ? 'Copied!' : 'Copy'}
              </Button>
            </div>
            <p className="text-xs" style={{ color: 'var(--accent-warning, #f59e0b)' }}>
              Save this password now — it will not be shown again.
            </p>
          </div>
        ) : (
          <div className="flex gap-2 items-center">
            <input
              type="text"
              value={resetInput}
              onChange={e => setResetInput(e.target.value)}
              placeholder='Type RESET to confirm'
              className="px-3 py-1.5 rounded text-sm outline-none"
              style={{
                background: 'var(--bg-elevated)',
                border: '1px solid var(--border-default)',
                color: 'var(--text-primary)',
                width: '200px',
              }}
            />
            <Button
              variant="danger"
              size="sm"
              onClick={doResetAdmin}
              disabled={resetInput !== 'RESET' || resetting}
            >
              {resetting ? <><RefreshCw size={13} className="animate-spin mr-1.5" />Resetting…</> : 'Reset Password'}
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}

export default function SettingsPage() {
  const currentUser = useAuthStore((s) => s.user)
  const isAdmin = currentUser?.role === 'owner' || currentUser?.role === 'admin'

  return (
    <div className="space-y-6 max-w-2xl">
      <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Settings</h1>

      {/* Quick links to pages removed from top-level nav */}
      <QuickLinksCard />

      {/* General / instance */}
      {isAdmin && <GeneralSection />}

      {/* Navigation editor — admin only */}
      {isAdmin && <NavigationSection />}

      {/* Appearance — UI mode + device override */}
      <AppearanceSection />

      {/* Accessibility */}
      <AccessibilitySection />

      {/* Dashboard / Weather */}
      <WeatherLocationSection />

      {/* AI integrations */}
      <AIIntegrationsSection />

      {/* Notification webhooks (simple per-channel) — admin/owner only */}
      {isAdmin && <NotificationsSection />}

      {/* Advanced webhook configs — admin/owner only */}
      {isAdmin && <WebhooksSection />}

      {/* System — update + restart */}
      {isAdmin && <SystemSection />}

      {/* Preferences */}
      <PreferencesSection />

      {/* Developer */}
      {isAdmin && <DeveloperSection />}

      {/* Disaster Recovery — owner/admin only */}
      {isAdmin && <DisasterRecoverySection />}

      {/* Change own password */}
      <AccountSection />

      {/* User management — admin/owner only */}
      {isAdmin && <UsersSection currentUserId={currentUser?.id ?? ''} />}
    </div>
  )
}

function GeneralSection() {
  const [name, setName] = useState('')
  const [tagline, setTagline] = useState('')
  const [bgUrl, setBgUrl] = useState('')
  const [customCss, setCustomCss] = useState('')
  const [logo, setLogo] = useState('')
  const [saved, setSaved] = useState(false)
  const [loading, setLoading] = useState(true)
  const logoInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    fetch('/api/settings/general', { credentials: 'include' })
      .then(r => r.ok ? r.json() : null)
      .then((d: { instance_name: string; login_tagline: string; custom_css: string; login_bg_url: string; instance_logo: string } | null) => {
        if (d) {
          setName(d.instance_name ?? '')
          setTagline(d.login_tagline ?? '')
          setBgUrl(d.login_bg_url ?? '')
          setCustomCss(d.custom_css ?? '')
          setLogo(d.instance_logo ?? '')
        }
      })
      .finally(() => setLoading(false))
  }, [])

  const handleLogoFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => setLogo(reader.result as string)
    reader.readAsDataURL(file)
  }

  const save = async () => {
    try {
      await fetch('/api/settings/general', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          instance_name: name.trim() || 'VoidTower',
          login_tagline: tagline.trim() || null,
          login_bg_url:  bgUrl.trim() || null,
          custom_css:    customCss || null,
          instance_logo: logo || null,
        }),
      })
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
      const finalName = name.trim() || 'VoidTower'
      window.dispatchEvent(new CustomEvent('vt-settings-changed', { detail: { instance_name: finalName } }))
      let styleEl = document.getElementById('vt-custom-css') as HTMLStyleElement | null
      if (customCss) {
        if (!styleEl) { styleEl = document.createElement('style'); styleEl.id = 'vt-custom-css'; document.head.appendChild(styleEl) }
        styleEl.textContent = customCss
      } else if (styleEl) { styleEl.textContent = '' }
    } catch {
      notify.error('Failed to save')
    }
  }

  const inputStyle = { background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }

  return (
    <div className="card space-y-4">
      <div className="flex items-center gap-2">
        <Globe size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>General</h2>
      </div>

      {/* Instance name */}
      <div>
        <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Instance name</label>
        <input
          value={loading ? '' : name}
          onChange={e => setName(e.target.value)}
          placeholder="VoidTower"
          disabled={loading}
          className="w-full px-3 py-2 rounded text-sm outline-none"
          style={inputStyle}
        />
        <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Shown in the browser tab title and sidebar.</p>
      </div>

      {/* Logo */}
      <div>
        <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Instance logo</label>
        <div className="flex items-center gap-3">
          {logo && (
            <img src={logo} alt="logo preview" style={{ width: 48, height: 48, objectFit: 'contain', borderRadius: 6, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)' }} />
          )}
          <div className="flex gap-2">
            <button
              onClick={() => logoInputRef.current?.click()}
              className="px-3 py-1.5 rounded text-xs font-medium"
              style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
            >
              {logo ? 'Change' : 'Choose file'}
            </button>
            {logo && (
              <button
                onClick={() => { setLogo(''); if (logoInputRef.current) logoInputRef.current.value = '' }}
                className="px-3 py-1.5 rounded text-xs"
                style={{ color: 'var(--accent-danger)' }}
              >
                Clear
              </button>
            )}
          </div>
          <input ref={logoInputRef} type="file" accept="image/*" className="hidden" onChange={handleLogoFile} />
        </div>
        <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Used as favicon and on the login page. Max ~256 KB.</p>
      </div>

      {/* Login tagline */}
      <div>
        <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Login tagline</label>
        <input
          value={tagline}
          onChange={e => setTagline(e.target.value)}
          placeholder="Self-hosted infrastructure dashboard"
          className="w-full px-3 py-2 rounded text-sm outline-none"
          style={inputStyle}
        />
        <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Short description shown below the instance name on the login page.</p>
      </div>

      {/* Login background URL */}
      <div>
        <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Login background URL</label>
        <input
          value={bgUrl}
          onChange={e => setBgUrl(e.target.value)}
          placeholder="https://example.com/bg.jpg"
          className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
          style={inputStyle}
        />
        {bgUrl && (
          <div style={{ marginTop: 6, height: 80, borderRadius: 6, overflow: 'hidden', border: '1px solid var(--border-subtle)' }}>
            <img src={bgUrl} alt="bg preview" style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
          </div>
        )}
      </div>

      {/* Custom CSS */}
      <div>
        <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Custom CSS (injected globally)</label>
        <textarea
          value={customCss}
          onChange={e => setCustomCss(e.target.value)}
          rows={10}
          placeholder=":root { --accent-primary: #ff6b6b; }"
          className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
          style={{ ...inputStyle, resize: 'vertical' }}
        />
        <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Max 8192 characters. Applied immediately on save.</p>
      </div>

      <button onClick={save} className="px-3 py-1.5 rounded text-xs font-medium transition-colors"
        style={{ background: saved ? 'var(--accent-success-subtle)' : 'var(--accent-primary)', color: saved ? 'var(--accent-success)' : '#fff' }}>
        {saved ? 'Saved ✓' : 'Save'}
      </button>
    </div>
  )
}

// Sidebar groups in the exact order they appear in Tower Mode
const SIDEBAR_GROUPS: { label: string; ids: string[] }[] = [
  { label: 'Overview',  ids: ['dashboard', 'alerts', 'timeline'] },
  { label: 'Resources', ids: ['services', 'containers', 'vms', 'proxmox', 'apps'] },
  { label: 'AI',        ids: ['ai', 'models'] },
  { label: 'Network',   ids: ['network', 'proxies', 'wireguard', 'firewall'] },
  { label: 'Data',      ids: ['storage', 'backups', 'files'] },
  { label: 'Security',  ids: ['security', 'secrets', 'audit'] },
  { label: 'Ops',       ids: ['automation', 'terminal', 'tags'] },
  { label: 'System',    ids: ['integrations', 'updates', 'mods', 'themes', 'settings'] },
  { label: 'Void Mode dock only', ids: ['odysseus'] },
]

function NavigationSection() {
  const { items, setItems, resetItems } = useNavConfigStore()
  const resolved = resolvedNavItems(items)
  const [list, setList] = useState<NavItem[]>(resolved)
  const [editingId, setEditingId] = useState<string | null>(null)

  useEffect(() => { setList(resolvedNavItems(items)) }, [items])

  const toggleVisible = (id: string) => {
    const next = list.map(it => it.id === id ? { ...it, visible: !it.visible } : it)
    setList(next); setItems(next)
  }

  const updateLabel = (id: string, label: string) => {
    const next = list.map(it => it.id === id ? { ...it, label } : it)
    setList(next); setItems(next); setEditingId(null)
  }

  const navMap = Object.fromEntries(list.map(it => [it.id, it]))

  return (
    <div className="card space-y-4">
      <div className="flex items-center gap-2">
        <Navigation size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Navigation</h2>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Toggle visibility or rename items. Groups match the Tower Mode sidebar. Changes apply immediately.
      </p>

      {SIDEBAR_GROUPS.map(group => {
        const groupItems = group.ids.map(id => navMap[id]).filter(Boolean)
        if (groupItems.length === 0) return null
        return (
          <div key={group.label}>
            <p className="text-xs font-medium uppercase tracking-wider mb-1.5" style={{ color: 'var(--text-muted)' }}>{group.label}</p>
            <div className="space-y-1">
              {groupItems.map(item => {
                const Icon = ICON_MAP[item.id]
                const defaultLabel = DEFAULT_NAV_ITEMS.find(d => d.id === item.id)?.label ?? item.id
                return (
                  <div key={item.id} style={{
                    display: 'flex', alignItems: 'center', gap: 8,
                    padding: '5px 8px', borderRadius: 6,
                    background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)',
                    opacity: item.visible ? 1 : 0.45,
                  }}>
                    {Icon && <Icon size={13} style={{ color: 'var(--text-secondary)', flexShrink: 0 }} />}
                    <div style={{ flex: 1, minWidth: 0 }}>
                      {editingId === item.id ? (
                        <input autoFocus defaultValue={item.label}
                          onBlur={e => updateLabel(item.id, e.target.value.trim() || defaultLabel)}
                          onKeyDown={e => {
                            if (e.key === 'Enter') updateLabel(item.id, (e.target as HTMLInputElement).value.trim() || defaultLabel)
                            if (e.key === 'Escape') setEditingId(null)
                          }}
                          className="w-full text-xs outline-none px-1 rounded"
                          style={{ background: 'var(--bg-root)', border: '1px solid var(--accent-primary)', color: 'var(--text-primary)' }}
                          onClick={e => e.stopPropagation()}
                        />
                      ) : (
                        <span className="text-xs cursor-text" style={{ color: 'var(--text-primary)' }}
                          onClick={() => setEditingId(item.id)} title="Click to rename">
                          {item.label}
                          {item.label !== defaultLabel && <span style={{ color: 'var(--text-muted)', marginLeft: 4 }}>({defaultLabel})</span>}
                        </span>
                      )}
                    </div>
                    <button onClick={() => toggleVisible(item.id)} title={item.visible ? 'Hide' : 'Show'}
                      style={{ color: item.visible ? 'var(--text-secondary)' : 'var(--text-muted)', background: 'none', border: 'none', cursor: 'pointer', flexShrink: 0 }}>
                      {item.visible ? <Eye size={13} /> : <EyeOff size={13} />}
                    </button>
                  </div>
                )
              })}
            </div>
          </div>
        )
      })}

      <button onClick={() => { resetItems(); setList(DEFAULT_NAV_ITEMS) }}
        className="px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
        style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
        Reset to defaults
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
  const [nginxBackend, setNginxBackend] = useState<'docker' | 'system' | 'none'>('none')

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
    fetch('/api/proxy', { credentials: 'include' })
      .then(r => r.ok ? r.json() : null)
      .then((r: { nginx_backend?: string } | null) => {
        if (r?.nginx_backend) setNginxBackend(r.nginx_backend as 'docker' | 'system' | 'none')
      })
      .catch(() => {})
  }, [])

  const [saveResult, setSaveResult] = useState<{ ok: boolean; message: string } | null>(null)

  const save = async () => {
    setSaving(true)
    setSaveResult(null)
    const url = workspaceUrl.trim() || null
    const port = parseInt(proxyPort, 10) || 7001

    if (url) localStorage.setItem(AI_WORKSPACE_KEY, url); else localStorage.removeItem(AI_WORKSPACE_KEY)
    if (llmEndpoint.trim()) localStorage.setItem(AI_LLM_KEY, llmEndpoint.trim()); else localStorage.removeItem(AI_LLM_KEY)

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
            Odysseus, Open WebUI, or any AI frontend. VoidTower creates a reverse proxy
            {nginxBackend === 'docker' ? ' (via nginx-proxy Docker container)' : nginxBackend === 'system' ? ' (via system nginx)' : ''}{' '}
            so it embeds cleanly.
          </p>
        </div>

        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>
            Proxy port{' '}
            <span style={{ color: 'var(--text-muted)' }}>
              {nginxBackend === 'docker'
                ? '(nginx-proxy Docker container listens here — default 8080)'
                : nginxBackend === 'system'
                ? '(nginx listens here, must be reachable from your browser)'
                : '(deploy nginx-proxy from App Vault to enable proxy embedding)'}
            </span>
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
          {nginxBackend === 'none' && (
            <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
              No reverse proxy detected.{' '}
              <a href="/apps" style={{ color: 'var(--accent-primary)' }}>Deploy nginx-proxy from App Vault</a>{' '}
              or install system nginx to enable AI workspace embedding.
            </p>
          )}
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

// ─── System section ───────────────────────────────────────────────────────────

function SystemSection() {
  const [version, setVersion] = useState<{ commit: string; branch: string; commit_date: string; dirty: boolean } | null>(null)
  const [check, setCheck] = useState<{ behind: number; ahead: number; can_update: boolean; remote_commit: string } | null>(null)
  const [checking, setChecking] = useState(false)
  const [restarting, setRestarting] = useState(false)
  const [updating, setUpdating] = useState(false)

  useEffect(() => {
    fetch('/api/system/version', { credentials: 'include' })
      .then(r => r.ok ? r.json() : null).then(setVersion).catch(() => {})
  }, [])

  const checkUpdates = async () => {
    setChecking(true)
    try {
      const r = await fetch('/api/system/update-check', { credentials: 'include' })
      if (r.ok) setCheck(await r.json())
    } finally { setChecking(false) }
  }

  const doRestart = async () => {
    if (!confirm('Restart VoidTower now? You will be disconnected briefly.')) return
    setRestarting(true)
    try {
      await fetch('/api/system/restart', { method: 'POST', credentials: 'include' })
      // Poll until server comes back
      const poll = setInterval(async () => {
        try {
          const r = await fetch('/api/system/version', { credentials: 'include' })
          if (r.ok) { clearInterval(poll); setRestarting(false); notify.success('VoidTower restarted.') }
        } catch { /* empty */ }
      }, 1500)
    } catch { setRestarting(false) }
  }

  const doUpdate = async () => {
    if (!confirm('Pull latest from GitHub, rebuild, and restart? This may take a few minutes.')) return
    setUpdating(true)
    notify.info('Update started', 'VoidTower will restart when done.')
    try {
      await fetch('/api/system/update', { method: 'POST', credentials: 'include' })
      const poll = setInterval(async () => {
        try {
          const r = await fetch('/api/system/version', { credentials: 'include' })
          if (r.ok) { clearInterval(poll); setUpdating(false); setCheck(null); const v = await r.json(); setVersion(v); notify.success('Update complete.') }
        } catch { /* empty */ }
      }, 3000)
    } catch { setUpdating(false) }
  }

  return (
    <div className="card space-y-4">
      <div className="flex items-center gap-2">
        <GitBranch size={15} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>System</h2>
      </div>

      {version && (
        <div className="flex flex-wrap gap-3 text-xs" style={{ color: 'var(--text-muted)' }}>
          <span>Branch: <code style={{ color: 'var(--text-primary)' }}>{version.branch}</code></span>
          <span>Commit: <code style={{ color: 'var(--text-primary)' }}>{version.commit}</code></span>
          {version.dirty && <span style={{ color: 'var(--accent-warning, #f59e0b)' }}>● uncommitted changes</span>}
        </div>
      )}

      {check && (
        <div className="rounded px-3 py-2 text-xs" style={{
          background: check.can_update ? 'var(--accent-warning, #f59e0b)18' : 'var(--accent-success)18',
          border: `1px solid ${check.can_update ? 'var(--accent-warning, #f59e0b)44' : 'var(--accent-success)44'}`,
          color: check.can_update ? 'var(--accent-warning, #f59e0b)' : 'var(--accent-success)',
        }}>
          {check.can_update
            ? `${check.behind} new commit${check.behind !== 1 ? 's' : ''} available (${check.remote_commit})`
            : 'Up to date'}
          {check.ahead > 0 && ` · ${check.ahead} local commit${check.ahead !== 1 ? 's' : ''} ahead`}
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" size="sm" onClick={checkUpdates} disabled={checking}>
          {checking
            ? <><RefreshCw size={13} className="animate-spin mr-1.5" />Checking…</>
            : <><RefreshCw size={13} className="mr-1.5" />Check for updates</>}
        </Button>
        {check?.can_update && (
          <Button variant="secondary" size="sm" onClick={doUpdate} disabled={updating}>
            {updating
              ? <><Download size={13} className="animate-spin mr-1.5" />Updating…</>
              : <><Download size={13} className="mr-1.5" />Update &amp; restart</>}
          </Button>
        )}
        <Button variant="danger" size="sm" onClick={doRestart} disabled={restarting}>
          {restarting
            ? <><RefreshCw size={13} className="animate-spin mr-1.5" />Restarting…</>
            : <><RefreshCw size={13} className="mr-1.5" />Restart VoidTower</>}
        </Button>
      </div>
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

// ─── Webhooks section ─────────────────────────────────────────────────────────

const ALL_EVENTS = ['alert.created', 'alert.acked', 'alert.resolved', 'backup.failed', 'service.down'] as const
type WebhookEvent = typeof ALL_EVENTS[number]
type WebhookType = 'ntfy' | 'discord' | 'slack' | 'generic'

interface WebhookConfig {
  id: number
  name: string
  url: string
  type: WebhookType
  events: string  // JSON array
  enabled: boolean
  created_at: number
}

const TYPE_COLORS: Record<WebhookType, string> = {
  ntfy:    'var(--accent-primary)',
  discord: '#5865F2',
  slack:   '#4A154B',
  generic: 'var(--text-muted)',
}

function WebhooksSection() {
  const [webhooks, setWebhooks] = useState<WebhookConfig[]>([])
  const [loading, setLoading] = useState(true)
  const [showModal, setShowModal] = useState(false)
  const [testing, setTesting] = useState<number | null>(null)
  const [deleting, setDeleting] = useState<number | null>(null)

  // Form state
  const [formName, setFormName] = useState('')
  const [formUrl, setFormUrl] = useState('')
  const [formType, setFormType] = useState<WebhookType>('generic')
  const [formEvents, setFormEvents] = useState<WebhookEvent[]>(['alert.created'])
  const [formEnabled, setFormEnabled] = useState(true)
  const [formSaving, setFormSaving] = useState(false)

  const refresh = () => {
    setLoading(true)
    fetch('/api/webhooks', { credentials: 'include' })
      .then(r => r.ok ? r.json() : { webhooks: [] })
      .then((d: { webhooks: WebhookConfig[] }) => setWebhooks(d.webhooks))
      .catch(() => notify.error('Failed to load webhooks'))
      .finally(() => setLoading(false))
  }

  useEffect(() => { refresh() }, [])

  const openModal = () => {
    setFormName(''); setFormUrl(''); setFormType('generic')
    setFormEvents(['alert.created']); setFormEnabled(true)
    setShowModal(true)
  }

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault()
    setFormSaving(true)
    try {
      const r = await fetch('/api/webhooks', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: formName.trim(),
          url: formUrl.trim(),
          type: formType,
          events: formEvents,
          enabled: formEnabled,
        }),
      })
      if (!r.ok) {
        const d = await r.json() as { error?: { message?: string } }
        throw new Error(d.error?.message ?? 'Failed to create')
      }
      notify.success('Webhook created')
      setShowModal(false)
      refresh()
    } catch (err) {
      notify.error(err instanceof Error ? err.message : 'Failed to create webhook')
    } finally {
      setFormSaving(false)
    }
  }

  const toggleEnabled = async (wh: WebhookConfig) => {
    try {
      await fetch(`/api/webhooks/${wh.id}`, {
        method: 'PATCH',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: !wh.enabled }),
      })
      refresh()
    } catch {
      notify.error('Failed to update webhook')
    }
  }

  const handleTest = async (id: number) => {
    setTesting(id)
    try {
      const r = await fetch(`/api/webhooks/${id}/test`, {
        method: 'POST',
        credentials: 'include',
      }).then(res => res.json()) as { ok: boolean; error?: string }
      if (r.ok) notify.success('Test notification sent')
      else notify.error(`Test failed: ${r.error ?? 'unknown'}`)
    } catch {
      notify.error('Request failed')
    } finally {
      setTesting(null)
    }
  }

  const handleDelete = async (wh: WebhookConfig) => {
    if (!confirm(`Delete webhook "${wh.name}"?`)) return
    setDeleting(wh.id)
    try {
      await fetch(`/api/webhooks/${wh.id}`, { method: 'DELETE', credentials: 'include' })
      notify.success('Webhook deleted')
      refresh()
    } catch {
      notify.error('Failed to delete webhook')
    } finally {
      setDeleting(null)
    }
  }

  const toggleEvent = (ev: WebhookEvent) => {
    setFormEvents(prev =>
      prev.includes(ev) ? prev.filter(e => e !== ev) : [...prev, ev]
    )
  }

  return (
    <div className="card space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Webhook size={14} style={{ color: 'var(--accent-primary)' }} />
          <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Webhook Targets</h2>
        </div>
        <button
          onClick={openModal}
          className="flex items-center gap-1.5 px-2.5 py-1.5 rounded text-xs transition-colors hover:opacity-80"
          style={{ background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)' }}
        >
          <Plus size={12} />
          Add Webhook
        </button>
      </div>

      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Per-event webhook targets for ntfy, Discord, Slack, or any HTTP endpoint.
      </p>

      {loading ? (
        <p className="text-xs py-2" style={{ color: 'var(--text-muted)' }}>Loading…</p>
      ) : webhooks.length === 0 ? (
        <p className="text-xs py-2" style={{ color: 'var(--text-muted)' }}>No webhooks configured.</p>
      ) : (
        <div className="divide-y" style={{ borderColor: 'var(--border-subtle)' }}>
          {webhooks.map(wh => {
            const events: string[] = (() => { try { return JSON.parse(wh.events) } catch { return [] } })()
            return (
              <div key={wh.id} className="flex items-start gap-3 py-2.5">
                {/* Enable toggle */}
                <button
                  onClick={() => toggleEnabled(wh)}
                  className="mt-0.5 flex-shrink-0"
                  title={wh.enabled ? 'Disable' : 'Enable'}
                  style={{ color: wh.enabled ? 'var(--accent-success)' : 'var(--text-muted)' }}
                >
                  {wh.enabled ? <ToggleRight size={18} /> : <ToggleLeft size={18} />}
                </button>

                {/* Details */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{wh.name}</span>
                    <span className="text-xs px-1.5 py-0.5 rounded font-mono"
                      style={{ background: `${TYPE_COLORS[wh.type] ?? '#888'}22`, color: TYPE_COLORS[wh.type] ?? 'var(--text-muted)', border: `1px solid ${TYPE_COLORS[wh.type] ?? '#888'}44` }}>
                      {wh.type}
                    </span>
                  </div>
                  <div className="text-xs mt-0.5 truncate font-mono" style={{ color: 'var(--text-muted)' }}>{wh.url}</div>
                  <div className="flex flex-wrap gap-1 mt-1">
                    {events.map(ev => (
                      <span key={ev} className="text-xs px-1.5 py-0.5 rounded"
                        style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
                        {ev}
                      </span>
                    ))}
                  </div>
                </div>

                {/* Actions */}
                <div className="flex items-center gap-1.5 flex-shrink-0">
                  <button
                    disabled={testing === wh.id}
                    onClick={() => handleTest(wh.id)}
                    className="flex items-center gap-1 px-2 py-1 rounded text-xs transition-colors disabled:opacity-40"
                    style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
                  >
                    <Send size={11} />
                    {testing === wh.id ? '…' : 'Test'}
                  </button>
                  <button
                    disabled={deleting === wh.id}
                    onClick={() => handleDelete(wh)}
                    className="p-1 rounded transition-colors hover:opacity-80 disabled:opacity-40"
                    style={{ color: 'var(--accent-danger)' }}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              </div>
            )
          })}
        </div>
      )}

      {/* Add Webhook Modal */}
      {showModal && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4"
          style={{ background: 'rgba(0,0,0,0.6)' }}
          onClick={e => { if (e.target === e.currentTarget) setShowModal(false) }}
        >
          <form
            onSubmit={handleCreate}
            className="card w-full max-w-md space-y-4"
            style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-default)' }}
          >
            <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Add Webhook</h3>

            <div>
              <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Name</label>
              <input
                value={formName}
                onChange={e => setFormName(e.target.value)}
                placeholder="My Discord webhook"
                required
                className="w-full px-3 py-2 rounded text-sm outline-none"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              />
            </div>

            <div>
              <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>URL</label>
              <input
                type="url"
                value={formUrl}
                onChange={e => setFormUrl(e.target.value)}
                placeholder="https://discord.com/api/webhooks/…"
                required
                className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              />
            </div>

            <div>
              <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Type</label>
              <select
                value={formType}
                onChange={e => setFormType(e.target.value as WebhookType)}
                className="w-full px-3 py-2 rounded text-sm outline-none"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              >
                <option value="ntfy">ntfy</option>
                <option value="discord">Discord</option>
                <option value="slack">Slack</option>
                <option value="generic">Generic (JSON)</option>
              </select>
            </div>

            <div>
              <label className="block text-xs mb-2" style={{ color: 'var(--text-secondary)' }}>Events</label>
              <div className="space-y-1.5">
                {ALL_EVENTS.map(ev => (
                  <label key={ev} className="flex items-center gap-2 cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={formEvents.includes(ev)}
                      onChange={() => toggleEvent(ev)}
                      className="rounded"
                    />
                    <span className="text-xs font-mono" style={{ color: 'var(--text-primary)' }}>{ev}</span>
                  </label>
                ))}
              </div>
            </div>

            <label className="flex items-center gap-2 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={formEnabled}
                onChange={e => setFormEnabled(e.target.checked)}
              />
              <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Enabled</span>
            </label>

            <div className="flex gap-2 pt-1">
              <Button size="sm" variant="primary" type="submit" loading={formSaving} disabled={formSaving || formEvents.length === 0}>
                Create
              </Button>
              <Button size="sm" variant="ghost" type="button" onClick={() => setShowModal(false)}>
                Cancel
              </Button>
            </div>
          </form>
        </div>
      )}
    </div>
  )
}

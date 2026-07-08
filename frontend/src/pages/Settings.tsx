import { useState, useEffect } from 'react'
import { useAuthStore } from '@/store/auth'
import { api, ApiClientError, isTauri } from '@/api/client'
import type { UserRecord } from '@/api/types'
import Button from '@/components/ui/Button'
import ConfirmDialog from '@/components/ui/ConfirmDialog'
import { notify } from '@/store/notifications'
import { Trash2, UserPlus, Bell, Send, Key, RefreshCw, Download, GitBranch, Monitor, Plus, Webhook, ToggleLeft, ToggleRight, Cpu, Stethoscope, Palette, AlertTriangle, Upload, Copy, ShieldOff, Shield } from 'lucide-react'
import ChangePlanModal, { type ChangePlan } from '@/components/ui/ChangePlanModal'
import type { OidcConfigSaveRequest } from '@/api/types'
import { useThemeStore, type UiMode } from '@/store/theme'
import { setDeviceTierOverride, type DeviceTier } from '@/aios/hooks/useDeviceTier'
import { Accessibility } from 'lucide-react'

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

// ─── Desktop window section (Tauri app only) ─────────────────────────────────

function DesktopWindowSection() {
  const [ready, setReady] = useState(false)
  const [platform, setPlatform] = useState<string | null>(null)
  const [glass, setGlassOn] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    import('@tauri-apps/plugin-os').then(({ platform: getPlatform }) => {
      setPlatform(getPlatform())
      setReady(true)
    })
  }, [])

  if (!isTauri()) return null

  const supported = platform === 'macos' || platform === 'windows'

  const toggleGlass = async () => {
    const next = !glass
    setError(null)
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('set_glass', { enabled: next })
      setGlassOn(next)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  return (
    <div className="card space-y-4" style={{ borderColor: 'var(--border-default)' }}>
      <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Desktop window</h2>
      <div className="flex items-center justify-between gap-4">
        <div>
          <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>Glass window</div>
          <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {supported
              ? 'A translucent, blurred window background (macOS vibrancy / Windows Mica). Off by default.'
              : error ?? "Window transparency effects aren't supported on Linux yet — this app's in-app theme blur (Settings → Appearance) still works."}
          </div>
        </div>
        <button
          role="switch"
          aria-checked={glass}
          disabled={!ready || !supported}
          onClick={toggleGlass}
          style={{
            flexShrink: 0, width: 36, height: 20, borderRadius: 10, border: 'none',
            cursor: !ready || !supported ? 'not-allowed' : 'pointer', position: 'relative',
            opacity: !ready || !supported ? 0.4 : 1,
            background: glass ? 'var(--accent-primary)' : 'var(--bg-elevated)',
          }}
        >
          <span style={{ position: 'absolute', top: 2, left: glass ? 18 : 2, width: 16, height: 16, borderRadius: '50%', background: '#fff', transition: 'left 0.2s' }} />
        </button>
      </div>
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

      {/* Authentik SSO — admin/owner only */}
      {isAdmin && <AuthentikSsoSection />}

      {/* System — update + restart */}
      {isAdmin && <SystemSection />}

      {/* Preferences */}
      <PreferencesSection />

      {/* Developer */}
      {isAdmin && <DeveloperSection />}

      {/* Desktop window (Tauri app only — no-ops/hides everywhere else) */}
      <DesktopWindowSection />

      {/* Disaster Recovery — owner/admin only */}
      {isAdmin && <DisasterRecoverySection />}

      {/* Change own password */}
      <AccountSection />

      {/* User management — admin/owner only */}
      {isAdmin && <UsersSection currentUserId={currentUser?.id ?? ''} />}

      {/* Per-member app access / storage / custom-deploy — admin/owner only */}
      {isAdmin && <MembersSection />}
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
  tls_port: number
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

      const tlsPort: number = r.tls_port ?? port + 1
      setAiSettings({ url, port, tls_port: tlsPort, proxy_active: r.proxy_active ?? false })

      if (r.proxy_active) {
        setSaveResult({
          ok: true,
          message: `Proxy active on port ${port} (and ${tlsPort} for HTTPS) — AI tab will use ` +
            `http://${window.location.hostname}:${port}/ or https://${window.location.hostname}:${tlsPort}/ ` +
            `depending on how you reach VoidTower. The HTTPS listener uses a self-signed cert — the first time ` +
            `you load the AI tab remotely over HTTPS, you may need to open that URL directly once and accept ` +
            `the browser's certificate warning before the embedded tab will load.`,
        })
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
    ? (window.location.protocol === 'https:'
        ? `https://${window.location.hostname}:${aiSettings.tls_port}`
        : `http://${window.location.hostname}:${aiSettings.port}`)
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
  const [pendingAction, setPendingAction] = useState<'restart' | 'update' | null>(null)

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

  const confirmRestart = async () => {
    setPendingAction(null)
    setRestarting(true)
    try {
      await fetch('/api/system/restart', { method: 'POST', credentials: 'include' })
      let wentDown = false
      let elapsed = 0
      const poll = setInterval(async () => {
        elapsed += 1500
        if (elapsed > 60_000) {
          clearInterval(poll)
          setRestarting(false)
          notify.error('Restart is taking too long — check server logs.')
          return
        }
        try {
          const r = await fetch('/api/system/version', { credentials: 'include' })
          if (r.ok && wentDown) { clearInterval(poll); setRestarting(false); notify.success('VoidTower restarted.') }
          else if (!r.ok) { wentDown = true }
        } catch { wentDown = true }
      }, 1500)
    } catch {
      setRestarting(false)
      notify.error('Failed to send restart request')
    }
  }

  const confirmUpdate = async () => {
    setPendingAction(null)
    setUpdating(true)
    notify.info('Update started', 'VoidTower will restart when done — this may take a few minutes.')
    try {
      await fetch('/api/system/update', { method: 'POST', credentials: 'include' })
      let wentDown = false
      let elapsed = 0
      const poll = setInterval(async () => {
        elapsed += 3000
        if (elapsed > 20 * 60_000) {
          clearInterval(poll)
          setUpdating(false)
          notify.error('Update is taking too long — check server logs.')
          return
        }
        try {
          const r = await fetch('/api/system/version', { credentials: 'include' })
          if (r.ok && wentDown) {
            clearInterval(poll)
            setUpdating(false)
            setCheck(null)
            const v = await r.json()
            setVersion(v)
            notify.success('Update complete.')
          } else if (!r.ok) { wentDown = true }
        } catch { wentDown = true }
      }, 3000)
    } catch {
      setUpdating(false)
      notify.error('Failed to start update')
    }
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
          <Button variant="secondary" size="sm" onClick={() => setPendingAction('update')} disabled={updating}>
            {updating
              ? <><Download size={13} className="animate-spin mr-1.5" />Updating…</>
              : <><Download size={13} className="mr-1.5" />Update &amp; restart</>}
          </Button>
        )}
        <Button variant="danger" size="sm" onClick={() => setPendingAction('restart')} disabled={restarting}>
          {restarting
            ? <><RefreshCw size={13} className="animate-spin mr-1.5" />Restarting…</>
            : <><RefreshCw size={13} className="mr-1.5" />Restart VoidTower</>}
        </Button>
      </div>

      {pendingAction === 'restart' && (
        <ConfirmDialog
          title="Restart VoidTower"
          message="VoidTower will restart now. You will be disconnected briefly."
          confirmLabel="Restart"
          danger
          onConfirm={confirmRestart}
          onCancel={() => setPendingAction(null)}
        />
      )}
      {pendingAction === 'update' && (
        <ConfirmDialog
          title="Update & Restart"
          message="Pull latest code, rebuild, and restart VoidTower? This may take a few minutes."
          confirmLabel="Update & restart"
          onConfirm={confirmUpdate}
          onCancel={() => setPendingAction(null)}
        />
      )}
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
    member: 'var(--accent-success)',
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
              <option value="member">member</option>
            </select>
          </div>
          <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
            User will be prompted to change their password on first login.
            {newRole === 'member' && ' Grant them app access below, in Members.'}
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

// ─── Members section (per-member app access / storage / custom-deploy) ──────

function bytesLabel(n: number | null | undefined): string {
  if (n === null || n === undefined) return '—'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let v = n
  let i = 0
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i += 1 }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`
}

function MemberRow({ member, catalogApps, onChanged }: {
  member: import('@/api/types').MemberListEntry
  catalogApps: import('@/api/types').AppDef[]
  onChanged: () => void
}) {
  const [expanded, setExpanded] = useState(false)
  const [busyApp, setBusyApp] = useState<string | null>(null)
  const [busyCustom, setBusyCustom] = useState(false)
  const [quotaGb, setQuotaGb] = useState(String(Math.round(member.storage.quota_bytes / (1024 ** 3))))
  const [maxApps, setMaxApps] = useState(String(member.storage.max_apps))
  const [savingQuota, setSavingQuota] = useState(false)
  const [driveLabel, setDriveLabel] = useState('')
  const [drivePath, setDrivePath] = useState('')
  const [addingDrive, setAddingDrive] = useState(false)

  const toggleApp = async (appId: string, granted: boolean) => {
    setBusyApp(appId)
    try {
      if (granted) await api.members.revokeAccess(member.id, appId)
      else await api.members.grantAccess(member.id, appId)
      onChanged()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to update app access')
    } finally {
      setBusyApp(null)
    }
  }

  const toggleCustom = async () => {
    setBusyCustom(true)
    try {
      await api.members.setCustomDeploy(member.id, !member.can_deploy_custom)
      onChanged()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to update custom-deploy flag')
    } finally {
      setBusyCustom(false)
    }
  }

  const saveQuota = async () => {
    const gb = Number(quotaGb)
    const apps = Number(maxApps)
    if (!Number.isFinite(gb) || gb < 0 || !Number.isFinite(apps) || apps < 0) {
      notify.error('Enter valid numbers'); return
    }
    setSavingQuota(true)
    try {
      await api.members.setQuota(member.id, Math.round(gb * 1024 ** 3), Math.round(apps))
      notify.success('Quota updated')
      onChanged()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to update quota')
    } finally {
      setSavingQuota(false)
    }
  }

  const addDrive = async (e: React.FormEvent) => {
    e.preventDefault()
    setAddingDrive(true)
    try {
      await api.members.addDrive(member.id, driveLabel.trim(), drivePath.trim())
      notify.success('Drive added')
      setDriveLabel(''); setDrivePath('')
      onChanged()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to add drive')
    } finally {
      setAddingDrive(false)
    }
  }

  const removeDrive = async (driveId: string) => {
    try {
      await api.members.removeDrive(driveId)
      onChanged()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to remove drive')
    }
  }

  const usagePct = member.storage.quota_bytes > 0
    ? Math.min(100, Math.round((member.storage.used_bytes / member.storage.quota_bytes) * 100))
    : 0

  return (
    <div className="rounded" style={{ border: '1px solid var(--border-subtle)' }}>
      <button
        onClick={() => setExpanded((v) => !v)}
        className="w-full flex items-center justify-between px-3 py-2.5 text-left"
      >
        <span className="flex items-center gap-2">
          <span className="text-sm font-mono" style={{ color: 'var(--text-primary)' }}>{member.username}</span>
          <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
            {member.app_ids.length} app{member.app_ids.length === 1 ? '' : 's'} · {member.storage.app_count}/{member.storage.max_apps} deployed
          </span>
        </span>
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
          {bytesLabel(member.storage.used_bytes)} / {bytesLabel(member.storage.quota_bytes)} ({usagePct}%)
        </span>
      </button>

      {expanded && (
        <div className="px-3 pb-3 space-y-4" style={{ borderTop: '1px solid var(--border-subtle)' }}>
          <div className="pt-3">
            <p className="text-xs font-medium mb-1.5" style={{ color: 'var(--text-secondary)' }}>App access</p>
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5">
              {catalogApps.map((app) => {
                const granted = member.app_ids.includes(app.id)
                return (
                  <label key={app.id} className="flex items-center gap-1.5 text-xs cursor-pointer" style={{ color: 'var(--text-secondary)' }}>
                    <input
                      type="checkbox"
                      checked={granted}
                      disabled={busyApp === app.id}
                      onChange={() => toggleApp(app.id, granted)}
                    />
                    {app.name}
                  </label>
                )
              })}
              {catalogApps.length === 0 && (
                <span className="text-xs" style={{ color: 'var(--text-muted)' }}>No catalog apps found.</span>
              )}
            </div>
          </div>

          <label className="flex items-center gap-2 text-xs cursor-pointer" style={{ color: 'var(--text-secondary)' }}>
            <input type="checkbox" checked={member.can_deploy_custom} disabled={busyCustom} onChange={toggleCustom} />
            Allow custom (self-supplied image) deploys
          </label>

          <div>
            <p className="text-xs font-medium mb-1.5" style={{ color: 'var(--text-secondary)' }}>Quota</p>
            <div className="flex items-center gap-2">
              <input
                type="number" min={0} value={quotaGb} onChange={(e) => setQuotaGb(e.target.value)}
                className="w-20 px-2 py-1 rounded text-xs font-mono outline-none"
                style={{ background: 'var(--bg-root)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              />
              <span className="text-xs" style={{ color: 'var(--text-muted)' }}>GB storage ·</span>
              <input
                type="number" min={0} value={maxApps} onChange={(e) => setMaxApps(e.target.value)}
                className="w-16 px-2 py-1 rounded text-xs font-mono outline-none"
                style={{ background: 'var(--bg-root)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              />
              <span className="text-xs" style={{ color: 'var(--text-muted)' }}>max apps</span>
              <Button size="sm" variant="ghost" onClick={saveQuota} loading={savingQuota}>Save</Button>
            </div>
            {member.storage.last_check_at && (
              <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>
                Usage last checked {new Date(member.storage.last_check_at * 1000).toLocaleString()}
              </p>
            )}
          </div>

          <div>
            <p className="text-xs font-medium mb-1.5" style={{ color: 'var(--text-secondary)' }}>Assigned drives</p>
            <div className="space-y-1">
              {member.drives.map((d) => (
                <div key={d.id} className="flex items-center gap-2 text-xs">
                  <span className="font-mono" style={{ color: 'var(--text-primary)' }}>{d.label}</span>
                  <span style={{ color: 'var(--text-muted)' }}>{d.host_path}</span>
                  <span style={{ color: 'var(--text-muted)' }}>
                    {d.free_bytes !== null ? `${bytesLabel(d.free_bytes)} free / ${bytesLabel(d.total_bytes)}` : 'not yet checked'}
                  </span>
                  <button onClick={() => removeDrive(d.id)} style={{ color: 'var(--accent-danger)' }} title="Remove drive">
                    <Trash2 size={12} />
                  </button>
                </div>
              ))}
              {member.drives.length === 0 && (
                <p className="text-xs" style={{ color: 'var(--text-muted)' }}>No drives assigned — deploys use their quota directory.</p>
              )}
            </div>
            <form onSubmit={addDrive} className="flex items-center gap-2 mt-2">
              <input
                placeholder="Label" value={driveLabel} onChange={(e) => setDriveLabel(e.target.value)} required
                className="w-24 px-2 py-1 rounded text-xs outline-none"
                style={{ background: 'var(--bg-root)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              />
              <input
                placeholder="/mnt/host/path" value={drivePath} onChange={(e) => setDrivePath(e.target.value)} required
                className="flex-1 px-2 py-1 rounded text-xs font-mono outline-none"
                style={{ background: 'var(--bg-root)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              />
              <Button size="sm" variant="ghost" type="submit" loading={addingDrive}>Add</Button>
            </form>
            <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>
              Must already be mounted on this host — VoidTower only bind-mounts it, never formats/partitions.
            </p>
          </div>
        </div>
      )}
    </div>
  )
}

function MembersSection() {
  const [members, setMembers] = useState<import('@/api/types').MemberListEntry[]>([])
  const [catalogApps, setCatalogApps] = useState<import('@/api/types').AppDef[]>([])
  const [loading, setLoading] = useState(true)

  const refresh = () => {
    Promise.all([api.members.list(), api.apps.catalog()])
      .then(([m, c]) => { setMembers(m.members); setCatalogApps(c.apps) })
      .catch(() => notify.error('Failed to load members'))
      .finally(() => setLoading(false))
  }

  useEffect(() => { refresh() }, [])

  return (
    <div className="card space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Members</h2>
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>App access, custom-deploy, storage</span>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        A "member" only sees the apps you grant here, and can optionally deploy and manage their own —
        create one with the role "member" above, then configure their access below.
      </p>

      {loading ? (
        <p className="text-xs py-2" style={{ color: 'var(--text-muted)' }}>Loading…</p>
      ) : members.length === 0 ? (
        <p className="text-xs py-2" style={{ color: 'var(--text-muted)' }}>No members yet.</p>
      ) : (
        <div className="space-y-2">
          {members.map((m) => (
            <MemberRow key={m.id} member={m} catalogApps={catalogApps} onChanged={refresh} />
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

// ─── Authentik / OIDC SSO section ─────────────────────────────────────────────

function AuthentikSsoSection() {
  const [loading, setLoading] = useState(true)
  const [enabled, setEnabled] = useState(false)
  const [issuerUrl, setIssuerUrl] = useState('')
  const [clientId, setClientId] = useState('')
  const [clientSecret, setClientSecret] = useState('')
  const [hasClientSecret, setHasClientSecret] = useState(false)
  const [redirectUrl, setRedirectUrl] = useState('')
  const [scopes, setScopes] = useState('openid profile email groups')
  const [roleClaim, setRoleClaim] = useState('groups')
  const [roleRows, setRoleRows] = useState<Array<{ group: string; role: string }>>([])
  const [defaultRole, setDefaultRole] = useState('viewer')
  const [autoProvision, setAutoProvision] = useState(true)
  const [saving, setSaving] = useState(false)
  const [pendingPlan, setPendingPlan] = useState<ChangePlan | null>(null)
  const [executing, setExecuting] = useState(false)

  useEffect(() => {
    api.oidc.get()
      .then((cfg) => {
        setEnabled(cfg.enabled)
        setIssuerUrl(cfg.issuer_url ?? '')
        setClientId(cfg.client_id ?? '')
        setHasClientSecret(cfg.has_client_secret)
        setRedirectUrl(cfg.redirect_url ?? `${window.location.origin}/api/auth/oidc/callback`)
        setScopes(cfg.scopes)
        setRoleClaim(cfg.role_claim)
        setRoleRows(Object.entries(cfg.role_map).map(([group, role]) => ({ group, role })))
        setDefaultRole(cfg.default_role)
        setAutoProvision(cfg.auto_provision)
      })
      .catch(() => notify.error('Failed to load Authentik SSO settings'))
      .finally(() => setLoading(false))
  }, [])

  const buildPayload = (): OidcConfigSaveRequest => {
    const role_map: Record<string, string> = {}
    for (const row of roleRows) {
      if (row.group.trim()) role_map[row.group.trim()] = row.role
    }
    const payload: OidcConfigSaveRequest = {
      enabled,
      issuer_url: issuerUrl.trim(),
      client_id: clientId.trim(),
      redirect_url: redirectUrl.trim(),
      scopes: scopes.trim() || 'openid profile email groups',
      role_claim: roleClaim.trim() || 'groups',
      role_map,
      default_role: defaultRole,
      auto_provision: autoProvision,
    }
    if (clientSecret.trim()) payload.client_secret = clientSecret.trim()
    return payload
  }

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault()
    setSaving(true)
    try {
      const res = await api.oidc.plan(buildPayload())
      setPendingPlan(res.plan)
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to plan SSO config save')
    } finally {
      setSaving(false)
    }
  }

  const addRoleRow = () => setRoleRows([...roleRows, { group: '', role: 'viewer' }])
  const updateRoleRow = (i: number, field: 'group' | 'role', value: string) => {
    setRoleRows(roleRows.map((r, idx) => idx === i ? { ...r, [field]: value } : r))
  }
  const removeRoleRow = (i: number) => setRoleRows(roleRows.filter((_, idx) => idx !== i))

  const inputStyle = { background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }

  if (loading) {
    return (
      <div className="card space-y-3">
        <div className="flex items-center gap-2">
          <Shield size={14} style={{ color: 'var(--accent-primary)' }} />
          <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Authentik SSO</h2>
        </div>
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</p>
      </div>
    )
  }

  return (
    <div className="card space-y-4">
      <div className="flex items-center gap-2">
        <Shield size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Authentik SSO</h2>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Adds a "Login with Authentik" option to the login page (local username/password stays available as a fallback)
        and lets proxies in the Proxies tab opt into being gated behind Authentik's login + MFA.
      </p>

      <form onSubmit={handleSave} className="space-y-3">
        <label className="flex items-center gap-2.5 cursor-pointer">
          <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} className="w-3.5 h-3.5 rounded" />
          <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Enabled</span>
        </label>

        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Issuer URL</label>
          <input value={issuerUrl} onChange={(e) => setIssuerUrl(e.target.value)} placeholder="https://authentik.local/application/o/voidtower/"
            className="w-full px-3 py-2 rounded text-sm outline-none font-mono" style={inputStyle} required={enabled} />
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Client ID</label>
            <input value={clientId} onChange={(e) => setClientId(e.target.value)}
              className="w-full px-3 py-2 rounded text-sm outline-none font-mono" style={inputStyle} required={enabled} />
          </div>
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Client secret</label>
            <input type="password" value={clientSecret} onChange={(e) => setClientSecret(e.target.value)}
              placeholder={hasClientSecret ? 'Unchanged — leave blank to keep' : 'Required on first save'}
              className="w-full px-3 py-2 rounded text-sm outline-none font-mono" style={inputStyle} required={enabled && !hasClientSecret} />
          </div>
        </div>

        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Redirect URL</label>
          <input value={redirectUrl} onChange={(e) => setRedirectUrl(e.target.value)}
            className="w-full px-3 py-2 rounded text-sm outline-none font-mono" style={inputStyle} required={enabled} />
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
            Must match the redirect URI registered on the Authentik OAuth2/OIDC provider exactly.
          </p>
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Scopes</label>
            <input value={scopes} onChange={(e) => setScopes(e.target.value)}
              className="w-full px-3 py-2 rounded text-sm outline-none font-mono" style={inputStyle} />
          </div>
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Role claim</label>
            <input value={roleClaim} onChange={(e) => setRoleClaim(e.target.value)}
              className="w-full px-3 py-2 rounded text-sm outline-none font-mono" style={inputStyle} />
            <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Userinfo claim holding group names (default "groups").</p>
          </div>
        </div>

        <div>
          <label className="block text-xs mb-2" style={{ color: 'var(--text-secondary)' }}>Role mapping (Authentik group → VoidTower role)</label>
          <div className="space-y-2">
            {roleRows.map((row, i) => (
              <div key={i} className="flex items-center gap-2">
                <input value={row.group} onChange={(e) => updateRoleRow(i, 'group', e.target.value)}
                  placeholder="authentik-group-name"
                  className="flex-1 px-2.5 py-1.5 rounded text-xs outline-none font-mono" style={inputStyle} />
                <select value={row.role} onChange={(e) => updateRoleRow(i, 'role', e.target.value)}
                  className="px-2 py-1.5 rounded text-xs outline-none" style={inputStyle}>
                  <option value="owner">owner</option>
                  <option value="admin">admin</option>
                  <option value="operator">operator</option>
                  <option value="viewer">viewer</option>
                </select>
                <button type="button" onClick={() => removeRoleRow(i)} style={{ color: 'var(--accent-danger)' }}>
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
          <button type="button" onClick={addRoleRow}
            className="mt-2 flex items-center gap-1.5 px-2.5 py-1.5 rounded text-xs transition-colors hover:opacity-80"
            style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
            <Plus size={12} /> Add mapping
          </button>
        </div>

        <div className="grid grid-cols-2 gap-3 items-end">
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Default role</label>
            <select value={defaultRole} onChange={(e) => setDefaultRole(e.target.value)}
              className="w-full px-3 py-2 rounded text-sm outline-none" style={inputStyle}>
              <option value="viewer">viewer</option>
              <option value="operator">operator</option>
              <option value="admin">admin</option>
            </select>
            <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Used when no group mapping matches.</p>
          </div>
          <label className="flex items-center gap-2.5 cursor-pointer pb-2">
            <input type="checkbox" checked={autoProvision} onChange={(e) => setAutoProvision(e.target.checked)} className="w-3.5 h-3.5 rounded" />
            <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Auto-create users on first Authentik login</span>
          </label>
        </div>

        <Button size="sm" variant="primary" type="submit" loading={saving}>Save</Button>
      </form>

      {pendingPlan && (
        <ChangePlanModal
          plan={pendingPlan}
          confirming={executing}
          onConfirm={async () => {
            setExecuting(true)
            try {
              await api.oidc.save(buildPayload())
              setHasClientSecret(hasClientSecret || !!clientSecret.trim())
              setClientSecret('')
              notify.success('Authentik SSO settings saved')
            } catch (err) {
              notify.error(err instanceof ApiClientError ? err.message : 'Failed to save SSO settings')
            } finally {
              setExecuting(false)
              setPendingPlan(null)
            }
          }}
          onCancel={() => setPendingPlan(null)}
        />
      )}
    </div>
  )
}

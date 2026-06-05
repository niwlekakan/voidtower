import { useRef, useEffect, useState, useCallback } from 'react'
import { Settings2, X, Clock, CloudSun, Cpu, HardDrive, Network, Activity, Bell, Container, GripVertical } from 'lucide-react'
import { useMetricsStore } from '@/store/metrics'
import MetricCard from '@/components/ui/MetricCard'
import MetricChart from '@/components/ui/MetricChart'
import { api } from '@/api/client'

// ─── Types ────────────────────────────────────────────────────────────────────

type WidgetId = 'clock' | 'weather' | 'system' | 'charts' | 'containers' | 'alerts' | 'disks' | 'network' | 'processes'

interface WidgetDef { id: WidgetId; label: string; icon: React.ReactNode; defaultOn: boolean }

const WIDGET_DEFS: WidgetDef[] = [
  { id: 'clock',      label: 'Clock & Date',       icon: <Clock size={14} />,     defaultOn: true },
  { id: 'weather',    label: 'Weather',             icon: <CloudSun size={14} />,  defaultOn: true },
  { id: 'system',     label: 'System Overview',     icon: <Cpu size={14} />,       defaultOn: true },
  { id: 'charts',     label: 'CPU & RAM Charts',    icon: <Activity size={14} />,  defaultOn: true },
  { id: 'containers', label: 'Container Summary',   icon: <Container size={14} />, defaultOn: true },
  { id: 'alerts',     label: 'Alert Summary',       icon: <Bell size={14} />,      defaultOn: true },
  { id: 'disks',      label: 'Disk Usage',          icon: <HardDrive size={14} />, defaultOn: true },
  { id: 'network',    label: 'Network',             icon: <Network size={14} />,   defaultOn: false },
  { id: 'processes',  label: 'Top Processes',       icon: <Activity size={14} />,  defaultOn: false },
  { id: 'gpu',        label: 'GPU Stats',           icon: <Cpu size={14} />,       defaultOn: true },
]

function loadWidgetConfig(): Record<WidgetId, boolean> {
  try {
    const raw = localStorage.getItem('vt-dashboard-widgets')
    if (raw) return JSON.parse(raw)
  } catch { /* empty */ }
  return Object.fromEntries(WIDGET_DEFS.map((w) => [w.id, w.defaultOn])) as Record<WidgetId, boolean>
}

function saveWidgetConfig(cfg: Record<WidgetId, boolean>) {
  localStorage.setItem('vt-dashboard-widgets', JSON.stringify(cfg))
}

// ─── Section ordering ─────────────────────────────────────────────────────────

type SectionId = 'time' | 'system' | 'services' | 'charts' | 'disks' | 'network_widget' | 'processes' | 'gpu_widget'

const SECTION_DEFS: { id: SectionId; label: string; widgets: WidgetId[] }[] = [
  { id: 'time',           label: 'Clock & Weather',       widgets: ['clock', 'weather'] },
  { id: 'system',         label: 'System Overview',        widgets: ['system'] },
  { id: 'services',       label: 'Containers & Alerts',   widgets: ['containers', 'alerts'] },
  { id: 'charts',         label: 'CPU & RAM Charts',       widgets: ['charts'] },
  { id: 'disks',          label: 'Disk Usage',             widgets: ['disks'] },
  { id: 'network_widget', label: 'Network Rates',          widgets: ['network'] },
  { id: 'processes',      label: 'Top Processes',          widgets: ['processes'] },
  { id: 'gpu_widget',     label: 'GPU Stats',              widgets: ['gpu'] },
]

const DEFAULT_SECTION_ORDER: SectionId[] = SECTION_DEFS.map(s => s.id)

function loadSectionOrder(): SectionId[] {
  try {
    const raw = localStorage.getItem('vt-dashboard-sections')
    if (raw) {
      const saved: SectionId[] = JSON.parse(raw)
      // Merge: keep saved order, append any new sections
      const all = new Set(DEFAULT_SECTION_ORDER)
      const valid = saved.filter(id => all.has(id))
      DEFAULT_SECTION_ORDER.forEach(id => { if (!valid.includes(id)) valid.push(id) })
      return valid
    }
  } catch { /* empty */ }
  return [...DEFAULT_SECTION_ORDER]
}

function saveSectionOrder(order: SectionId[]) {
  localStorage.setItem('vt-dashboard-sections', JSON.stringify(order))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function fmt(bytes: number, decimals = 1): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(decimals)} ${sizes[i]}`
}

function uptime(secs: number): string {
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return d > 0 ? `${d}d ${h}h` : h > 0 ? `${h}h ${m}m` : `${m}m`
}

// ─── Clock Widget ─────────────────────────────────────────────────────────────

function ClockWidget() {
  const [now, setNow] = useState(new Date())
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000)
    return () => clearInterval(id)
  }, [])

  const timeStr = now.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' })
  const dateStr = now.toLocaleDateString(undefined, { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' })

  return (
    <div className="card flex flex-col items-center justify-center py-6 gap-1">
      <div className="font-mono font-semibold text-4xl tracking-tight" style={{ color: 'var(--text-primary)', fontVariantNumeric: 'tabular-nums' }}>
        {timeStr}
      </div>
      <div className="text-sm" style={{ color: 'var(--text-muted)' }}>{dateStr}</div>
    </div>
  )
}

// ─── Weather Widget ───────────────────────────────────────────────────────────

const WMO: Record<number, string> = {
  0: 'Clear sky', 1: 'Mainly clear', 2: 'Partly cloudy', 3: 'Overcast',
  45: 'Foggy', 48: 'Icy fog', 51: 'Light drizzle', 53: 'Drizzle', 55: 'Heavy drizzle',
  61: 'Slight rain', 63: 'Moderate rain', 65: 'Heavy rain',
  71: 'Slight snow', 73: 'Moderate snow', 75: 'Heavy snow',
  80: 'Slight showers', 81: 'Moderate showers', 82: 'Violent showers',
  95: 'Thunderstorm', 96: 'Thunderstorm + hail', 99: 'Thunderstorm + heavy hail',
}

function weatherIcon(code: number): string {
  if (code === 0) return '☀️'
  if (code <= 2) return '🌤️'
  if (code === 3) return '☁️'
  if (code <= 48) return '🌫️'
  if (code <= 67) return '🌧️'
  if (code <= 77) return '❄️'
  if (code <= 82) return '🌦️'
  return '⛈️'
}

interface WeatherData { temp: number; code: number; wind: number; unit: string }

function WeatherWidget() {
  const [weather, setWeather] = useState<WeatherData | null>(null)
  const [error, setError]     = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const fetchWeather = (lat: number, lon: number) => {
      fetch(
        `https://api.open-meteo.com/v1/forecast?latitude=${lat}&longitude=${lon}` +
        `&current=temperature_2m,weathercode,windspeed_10m&temperature_unit=celsius`
      )
        .then((r) => r.json())
        .then((d) => {
          const c = d.current
          setWeather({ temp: c.temperature_2m, code: c.weathercode, wind: c.windspeed_10m, unit: '°C' })
        })
        .catch(() => setError('Weather unavailable'))
        .finally(() => setLoading(false))
    }

    const storedLat = localStorage.getItem('vt-weather-lat')
    const storedLon = localStorage.getItem('vt-weather-lon')
    if (storedLat && storedLon) {
      fetchWeather(parseFloat(storedLat), parseFloat(storedLon))
      return
    }

    if (!navigator.geolocation) { setError('Set location in Settings → Weather'); setLoading(false); return }
    navigator.geolocation.getCurrentPosition(
      ({ coords }) => fetchWeather(coords.latitude, coords.longitude),
      () => { setError('Location denied — set manually in Settings'); setLoading(false) },
      { timeout: 8000 }
    )
  }, [])

  return (
    <div className="card flex flex-col items-center justify-center py-6 gap-2">
      {loading && <p className="text-sm" style={{ color: 'var(--text-muted)' }}>Loading weather…</p>}
      {error   && <p className="text-xs text-center" style={{ color: 'var(--text-muted)' }}>{error}</p>}
      {weather && (
        <>
          <div className="text-5xl leading-none">{weatherIcon(weather.code)}</div>
          <div className="text-3xl font-semibold font-mono" style={{ color: 'var(--text-primary)' }}>
            {weather.temp.toFixed(1)}{weather.unit}
          </div>
          <div className="text-sm" style={{ color: 'var(--text-secondary)' }}>
            {WMO[weather.code] ?? 'Unknown'} · {weather.wind} km/h
          </div>
        </>
      )}
    </div>
  )
}

// ─── Container Summary Widget ─────────────────────────────────────────────────

function ContainersWidget() {
  const [counts, setCounts] = useState<{ running: number; stopped: number; total: number } | null>(null)

  useEffect(() => {
    api.containers.list()
      .then((r) => {
        const running = r.containers.filter((c) => c.state === 'running').length
        setCounts({ running, stopped: r.containers.length - running, total: r.containers.length })
      })
      .catch(() => {})
  }, [])

  return (
    <div className="card">
      <div className="text-xs uppercase tracking-wider mb-3" style={{ color: 'var(--text-muted)' }}>Containers</div>
      {!counts ? (
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</p>
      ) : (
        <div className="flex items-center gap-6">
          <div className="text-center">
            <div className="text-2xl font-semibold font-mono" style={{ color: 'var(--accent-success)' }}>{counts.running}</div>
            <div className="text-xs" style={{ color: 'var(--text-muted)' }}>Running</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-semibold font-mono" style={{ color: 'var(--accent-danger)' }}>{counts.stopped}</div>
            <div className="text-xs" style={{ color: 'var(--text-muted)' }}>Stopped</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-semibold font-mono" style={{ color: 'var(--text-secondary)' }}>{counts.total}</div>
            <div className="text-xs" style={{ color: 'var(--text-muted)' }}>Total</div>
          </div>
        </div>
      )}
    </div>
  )
}

// ─── Alerts Widget ────────────────────────────────────────────────────────────

function AlertsWidget() {
  const [counts, setCounts] = useState<{ critical: number; warning: number; info: number } | null>(null)

  useEffect(() => {
    api.alerts.list('active')
      .then((r) => {
        const alerts = r.alerts
        setCounts({
          critical: alerts.filter((a) => a.severity === 'critical').length,
          warning:  alerts.filter((a) => a.severity === 'warning').length,
          info:     alerts.filter((a) => a.severity === 'info').length,
        })
      })
      .catch(() => {})
  }, [])

  const total = counts ? counts.critical + counts.warning + counts.info : 0

  return (
    <div className="card">
      <div className="text-xs uppercase tracking-wider mb-3" style={{ color: 'var(--text-muted)' }}>Active Alerts</div>
      {!counts ? (
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</p>
      ) : total === 0 ? (
        <div className="flex items-center gap-2 text-sm" style={{ color: 'var(--accent-success)' }}>
          <span>✓</span> All clear
        </div>
      ) : (
        <div className="flex items-center gap-6">
          {counts.critical > 0 && (
            <div className="text-center">
              <div className="text-2xl font-semibold font-mono" style={{ color: 'var(--accent-danger)' }}>{counts.critical}</div>
              <div className="text-xs" style={{ color: 'var(--text-muted)' }}>Critical</div>
            </div>
          )}
          {counts.warning > 0 && (
            <div className="text-center">
              <div className="text-2xl font-semibold font-mono" style={{ color: 'var(--accent-warning)' }}>{counts.warning}</div>
              <div className="text-xs" style={{ color: 'var(--text-muted)' }}>Warning</div>
            </div>
          )}
          {counts.info > 0 && (
            <div className="text-center">
              <div className="text-2xl font-semibold font-mono" style={{ color: 'var(--accent-secondary)' }}>{counts.info}</div>
              <div className="text-xs" style={{ color: 'var(--text-muted)' }}>Info</div>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

// ─── Customise Panel ──────────────────────────────────────────────────────────

function CustomisePanel({ config, onChange, sectionOrder, onReorder, onClose }: {
  config: Record<WidgetId, boolean>
  onChange: (id: WidgetId, val: boolean) => void
  sectionOrder: SectionId[]
  onReorder: (order: SectionId[]) => void
  onClose: () => void
}) {
  const dragId = useRef<SectionId | null>(null)
  const [dragOver, setDragOver] = useState<SectionId | null>(null)

  const handleDragStart = (id: SectionId) => { dragId.current = id }
  const handleDragOver  = (e: React.DragEvent, id: SectionId) => {
    e.preventDefault()
    if (dragId.current !== id) setDragOver(id)
  }
  const handleDrop = (targetId: SectionId) => {
    const from = dragId.current
    if (!from || from === targetId) { setDragOver(null); return }
    const next = [...sectionOrder]
    const fi = next.indexOf(from), ti = next.indexOf(targetId)
    next.splice(fi, 1); next.splice(ti, 0, from)
    onReorder(next)
    dragId.current = null; setDragOver(null)
  }

  return (
    <div className="fixed inset-0 z-40 flex justify-end" onClick={onClose}>
      <div className="h-full w-72 flex flex-col"
           style={{ background: 'var(--bg-panel)', borderLeft: '1px solid var(--border-subtle)' }}
           onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between px-4 py-3 border-b"
             style={{ borderColor: 'var(--border-subtle)' }}>
          <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Customize Dashboard</h2>
          <button onClick={onClose} style={{ color: 'var(--text-muted)' }}><X size={16} /></button>
        </div>
        <p className="text-xs px-4 pt-3 pb-1" style={{ color: 'var(--text-muted)' }}>
          Drag to reorder sections. Toggle individual widgets.
        </p>
        <div className="flex-1 overflow-y-auto px-4 py-2 space-y-2">
          {sectionOrder.map((sid) => {
            const section = SECTION_DEFS.find(s => s.id === sid)!
            const isOver = dragOver === sid
            const isDragging = dragId.current === sid
            return (
              <div key={sid}
                draggable
                onDragStart={() => handleDragStart(sid)}
                onDragOver={(e) => handleDragOver(e, sid)}
                onDrop={() => handleDrop(sid)}
                onDragEnd={() => { dragId.current = null; setDragOver(null) }}
                className="rounded overflow-hidden"
                style={{
                  border: `1px solid ${isOver ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
                  background: isOver ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
                  opacity: isDragging ? 0.45 : 1,
                  transition: 'opacity 0.15s, border-color 0.15s',
                }}>
                <div className="flex items-center gap-2 px-2 py-1.5 cursor-grab active:cursor-grabbing select-none"
                     style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  <GripVertical size={13} style={{ color: 'var(--text-disabled)', flexShrink: 0 }} />
                  <span className="text-xs font-medium flex-1" style={{ color: 'var(--text-secondary)' }}>
                    {section.label}
                  </span>
                </div>
                <div className="px-2 py-1">
                  {section.widgets.map(wid => {
                    const w = WIDGET_DEFS.find(d => d.id === wid)!
                    return (
                      <label key={wid} className="flex items-center gap-2.5 py-1 cursor-pointer hover:opacity-80">
                        <input type="checkbox" checked={config[wid]}
                          onChange={(e) => onChange(wid, e.target.checked)}
                          className="w-3 h-3 rounded" style={{ accentColor: 'var(--accent-primary)' }} />
                        <span style={{ color: 'var(--text-muted)', display: 'flex' }}>{w.icon}</span>
                        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{w.label}</span>
                      </label>
                    )
                  })}
                </div>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

// ─── Main Page ────────────────────────────────────────────────────────────────

type HistPoint = { t: number; v: number }

export default function DashboardPage() {
  const snapshot    = useMetricsStore((s) => s.snapshot)
  const cpuHistory  = useRef<HistPoint[]>([])
  const ramHistory  = useRef<HistPoint[]>([])
  const [, forceRender]     = useState(0)
  const [customise, setCustomise]     = useState(false)
  const [widgets, setWidgets]         = useState<Record<WidgetId, boolean>>(loadWidgetConfig)
  const [sectionOrder, setSectionOrder] = useState<SectionId[]>(loadSectionOrder)

  useEffect(() => {
    if (!snapshot) return
    const t = snapshot.timestamp
    cpuHistory.current = [...cpuHistory.current.slice(-59), { t, v: snapshot.cpu_usage }]
    ramHistory.current = [...ramHistory.current.slice(-59), { t, v: (snapshot.ram_used / snapshot.ram_total) * 100 }]
    forceRender((n) => n + 1)
  }, [snapshot])

  const toggleWidget = useCallback((id: WidgetId, val: boolean) => {
    setWidgets((prev) => { const next = { ...prev, [id]: val }; saveWidgetConfig(next); return next })
  }, [])

  const reorderSections = useCallback((order: SectionId[]) => {
    saveSectionOrder(order); setSectionOrder(order)
  }, [])

  const on = (id: WidgetId) => widgets[id]

  const renderSection = (sid: SectionId) => {
    switch (sid) {
      case 'time':
        if (!on('clock') && !on('weather')) return null
        return (
          <div key="time" className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {on('clock')   && <ClockWidget />}
            {on('weather') && <WeatherWidget />}
          </div>
        )
      case 'system':
        if (!on('system') || !snapshot) return null
        return (
          <div key="system" className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <MetricCard label="CPU"       value={`${snapshot.cpu_usage.toFixed(1)}%`}        sub={snapshot.cpu_model.slice(0,24)} accent />
            <MetricCard label="RAM"       value={`${((snapshot.ram_used/snapshot.ram_total)*100).toFixed(1)}%`} sub={`${fmt(snapshot.ram_used)} / ${fmt(snapshot.ram_total)}`} />
            <MetricCard label="Uptime"    value={uptime(snapshot.uptime_secs)} />
            <MetricCard label="Processes" value={snapshot.process_count} />
          </div>
        )
      case 'services':
        if (!on('containers') && !on('alerts')) return null
        return (
          <div key="services" className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {on('containers') && <ContainersWidget />}
            {on('alerts')     && <AlertsWidget />}
          </div>
        )
      case 'charts':
        if (!on('charts')) return null
        return (
          <div key="charts" className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div className="card">
              <div className="text-xs uppercase tracking-wider mb-2" style={{ color: 'var(--text-muted)' }}>CPU Usage</div>
              <MetricChart data={cpuHistory.current} unit="%" color="var(--accent-primary)" />
            </div>
            <div className="card">
              <div className="text-xs uppercase tracking-wider mb-2" style={{ color: 'var(--text-muted)' }}>RAM Usage</div>
              <MetricChart data={ramHistory.current} unit="%" color="var(--accent-secondary)" />
            </div>
          </div>
        )
      case 'disks':
        if (!on('disks') || !snapshot) return null
        return (
          <div key="disks" className="card">
            <div className="text-xs uppercase tracking-wider mb-3" style={{ color: 'var(--text-muted)' }}>Disks</div>
            <div className="space-y-3">
              {snapshot.disks.map((d) => {
                const pct = (d.used / d.total) * 100
                return (
                  <div key={d.mount_point}>
                    <div className="flex justify-between text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>
                      <span>{d.mount_point} <span style={{ color: 'var(--text-muted)' }}>({d.fs_type})</span></span>
                      <span>{fmt(d.used)} / {fmt(d.total)}</span>
                    </div>
                    <div className="h-1.5 rounded-full overflow-hidden" style={{ background: 'var(--bg-elevated)' }}>
                      <div className="h-full rounded-full transition-all" style={{
                        width: `${pct}%`,
                        background: pct > 90 ? 'var(--accent-danger)' : pct > 75 ? 'var(--accent-warning)' : 'var(--accent-primary)',
                      }} />
                    </div>
                  </div>
                )
              })}
            </div>
          </div>
        )
      case 'network_widget':
        if (!on('network') || !snapshot || snapshot.networks.length === 0) return null
        return (
          <div key="network_widget" className="card">
            <div className="text-xs uppercase tracking-wider mb-3" style={{ color: 'var(--text-muted)' }}>Network</div>
            <div className="space-y-2">
              {snapshot.networks.filter(n => n.rx_bytes > 0 || n.tx_bytes > 0).map(n => (
                <div key={n.name} className="flex items-center justify-between text-xs">
                  <span style={{ color: 'var(--text-primary)' }}>{n.name}</span>
                  <span style={{ color: 'var(--text-muted)' }}>↓ {fmt(n.rx_bytes_per_sec)}/s · ↑ {fmt(n.tx_bytes_per_sec)}/s</span>
                </div>
              ))}
            </div>
          </div>
        )
      case 'processes':
        if (!on('processes') || !snapshot) return null
        return (
          <div key="processes" className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {[
              { label: 'Top CPU Processes',    procs: snapshot.top_cpu_procs, key: 'cpu' as const },
              { label: 'Top Memory Processes', procs: snapshot.top_mem_procs, key: 'mem' as const },
            ].map(({ label, procs, key }) => (
              <div key={key} className="card">
                <div className="text-xs uppercase tracking-wider mb-3" style={{ color: 'var(--text-muted)' }}>{label}</div>
                <div className="space-y-1.5">
                  {procs.map(p => (
                    <div key={p.pid} className="flex items-center justify-between text-xs">
                      <span className="truncate max-w-32" style={{ color: 'var(--text-primary)' }}>{p.name}</span>
                      <span className="font-mono" style={{ color: 'var(--text-muted)' }}>
                        {key === 'cpu' ? `${p.cpu_usage.toFixed(1)}%` : fmt(p.memory_bytes)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )
      case 'gpu_widget':
        if (!on('gpu') || !snapshot || snapshot.gpu.length === 0) return null
        return (
          <div key="gpu_widget" className="card">
            <div className="text-xs uppercase tracking-wider mb-3" style={{ color: 'var(--text-muted)' }}>GPU</div>
            <div className="space-y-4">
              {snapshot.gpu.map((g, i) => {
                const memPct = g.mem_total_mb > 0 ? (g.mem_used_mb / g.mem_total_mb) * 100 : 0
                const pwrPct = g.power_limit_w > 0 ? (g.power_w / g.power_limit_w) * 100 : 0
                const tempColor = g.temp_c >= 85 ? 'var(--accent-danger)' : g.temp_c >= 70 ? 'var(--accent-warning)' : 'var(--accent-success)'
                return (
                  <div key={i} className="space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-medium truncate max-w-48" style={{ color: 'var(--text-primary)' }}>{g.name}</span>
                      <span className="text-xs font-mono" style={{ color: tempColor }}>{g.temp_c.toFixed(0)}°C</span>
                    </div>
                    {[
                      { label: 'GPU',  pct: g.util_pct,     right: `${g.util_pct.toFixed(0)}%`,       color: 'var(--accent-primary)' },
                      { label: 'VRAM', pct: memPct,          right: `${g.mem_used_mb} / ${g.mem_total_mb} MB`, color: 'var(--accent-secondary)' },
                      { label: 'PWR',  pct: pwrPct,          right: `${g.power_w.toFixed(0)}W`,        color: 'var(--accent-warning)' },
                    ].map(({ label, pct, right, color }) => (
                      <div key={label}>
                        <div className="flex justify-between text-xs mb-1" style={{ color: 'var(--text-muted)' }}>
                          <span>{label}</span><span>{right}</span>
                        </div>
                        <div className="h-1.5 rounded-full overflow-hidden" style={{ background: 'var(--bg-elevated)' }}>
                          <div className="h-full rounded-full transition-all duration-500" style={{ width: `${Math.min(pct, 100)}%`, background: color }} />
                        </div>
                      </div>
                    ))}
                  </div>
                )
              })}
            </div>
          </div>
        )
      default: return null
    }
  }

  return (
    <div className="space-y-5">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Dashboard</h1>
          {snapshot && (
            <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
              {snapshot.hostname} · {snapshot.os_name} · {snapshot.kernel_version}
            </p>
          )}
        </div>
        <button
          onClick={() => setCustomise(true)}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-muted)' }}
        >
          <Settings2 size={13} /> Customize
        </button>
      </div>

      {sectionOrder.map(sid => renderSection(sid))}

      {!snapshot && (
        <div className="flex items-center justify-center h-40" style={{ color: 'var(--text-muted)' }}>
          Waiting for metrics…
        </div>
      )}

      {/* Customise panel */}
      {customise && (
        <CustomisePanel
          config={widgets}
          onChange={toggleWidget}
          sectionOrder={sectionOrder}
          onReorder={reorderSections}
          onClose={() => setCustomise(false)}
        />
      )}
    </div>
  )
}

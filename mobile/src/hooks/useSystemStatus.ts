import { useCallback, useEffect, useState } from 'react'
import { api } from '../api/client'
import type { Alert, MetricsSnapshot } from '../api/types'
import { useAuth } from '../auth/AuthContext'

/** Tiers that already had real system visibility before this restructure. */
export const SYSTEM_ACCESS_ROLES = ['owner', 'admin', 'demo'] as const

const SEED_METRICS: MetricsSnapshot = {
  hostname: 'demo-tower', uptime_secs: 432000, cpu_usage: 23, cpu_count: 8, cpu_model: 'Demo CPU',
  ram_total: 32e9, ram_used: 11e9, swap_total: 0, swap_used: 0, load_avg: [0.5, 0.6, 0.4],
  process_count: 210, os_name: 'Linux', kernel_version: '6.x-demo',
  disks: [{ name: 'demo', mount_point: '/', total: 1e12, used: 4e11, available: 6e11, fs_type: 'ext4' }],
  networks: [],
  gpu: [{ name: 'Demo GPU', temp_c: 45, util_pct: 12, mem_util_pct: 20, mem_used_mb: 2000, mem_total_mb: 12000, power_w: 40, power_limit_w: 220 }],
  timestamp: Date.now() / 1000,
}
const SEED_ALERTS: Alert[] = [
  { id: 'demo-1', title: 'Demo alert', message: 'This is seed data — demo accounts never touch the real system.', severity: 'info', category: 'demo', state: 'active', created_at: Date.now() / 1000 },
]

/**
 * Shared metrics+alerts source for every screen that shows system status
 * (HomeScreen's plain-language summary, AdvancedScreen's full technical view).
 * Demo accounts get fixed seed data and never hit the real backend.
 */
export function useSystemStatus() {
  const { user } = useAuth()
  const isDemo = user?.role === 'demo'
  const hasSystemAccess = !!user && (SYSTEM_ACCESS_ROLES as readonly string[]).includes(user.role)

  const [metrics, setMetrics] = useState<MetricsSnapshot | null>(isDemo ? SEED_METRICS : null)
  const [alerts, setAlerts] = useState<Alert[]>(isDemo ? SEED_ALERTS : [])
  const [loading, setLoading] = useState(hasSystemAccess && !isDemo)

  const refresh = useCallback(async () => {
    if (!hasSystemAccess || isDemo) return
    setLoading(true)
    const [m, a] = await Promise.allSettled([api.metrics.current(), api.alerts.list()])
    if (m.status === 'fulfilled') setMetrics(m.value)
    if (a.status === 'fulfilled') setAlerts(a.value.alerts)
    setLoading(false)
  }, [hasSystemAccess, isDemo])

  useEffect(() => { refresh() }, [refresh])

  return { metrics, alerts, loading, refresh, isDemo, hasSystemAccess }
}

import { useCallback, useEffect, useState } from 'react'
import {
  ArrowUpCircle, CheckCircle, RefreshCw, RotateCcw, Server,
  Package, Loader2, ChevronDown, ChevronUp, AlertTriangle, Container, Box, Bot,
} from 'lucide-react'
import { api } from '@/api/client'
import type { DockerUpdateRow, OdysseusUpdateInfo, OsUpdateInfo, VoidTowerUpdateInfo } from '@/api/types'
import { useDurableJobTracker } from '@/hooks/useDurableJobTracker'
import { notify } from '@/store/notifications'
import ChangePlanModal, { type ChangePlan } from '@/components/ui/ChangePlanModal'
import DurableJobNotice from '@/components/ui/DurableJobNotice'

// ─── Shared ───────────────────────────────────────────────────────────────────

const card: React.CSSProperties = {
  background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)',
  borderRadius: 10, padding: '20px 24px',
}

function SectionHeader({ icon: Icon, title, badge }: { icon: React.ElementType; title: string; badge?: React.ReactNode }) {
  return (
    <div className="flex items-center gap-2 mb-4">
      <Icon size={16} style={{ color: 'var(--accent-primary)' }} />
      <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{title}</h2>
      {badge}
    </div>
  )
}

function Badge({ label, color }: { label: string; color: string }) {
  return (
    <span className="px-2 py-0.5 rounded text-xs font-medium" style={{ background: `${color}22`, color, border: `1px solid ${color}44` }}>
      {label}
    </span>
  )
}

function Btn({ onClick, disabled, variant = 'primary', children }: {
  onClick: () => void; disabled?: boolean; variant?: 'primary' | 'secondary' | 'danger'; children: React.ReactNode
}) {
  const bg = variant === 'primary' ? 'var(--accent-primary)' : variant === 'danger' ? 'var(--accent-danger)' : 'var(--bg-elevated)'
  const color = variant === 'secondary' ? 'var(--text-primary)' : '#fff'
  const border = variant === 'secondary' ? '1px solid var(--border-default)' : undefined
  return (
    <button onClick={onClick} disabled={disabled}
      className="flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
      style={{ background: bg, color, border }}>
      {children}
    </button>
  )
}

// ─── VoidTower — Docker mode ──────────────────────────────────────────────────

function VtDockerPanel({ info, onRefresh }: { info: VoidTowerUpdateInfo; onRefresh: () => void }) {
  const [applying, setApplying] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const [plan, setPlan] = useState<ChangePlan | null>(null)
  const { trackedJob, trackedLabel, tracking, track } = useDurableJobTracker()
  const checking = tracking && trackedJob?.action === 'update.voidtower.check'

  const check = async () => {
    try {
      const { job } = await api.updates.checkVt()
      track(job, { label: 'VoidTower update check', onSucceeded: onRefresh })
    } catch (error) {
      notify.error(error instanceof Error ? error.message : 'Failed to submit update check')
    }
  }

  const preview = async () => {
    setApplying(true)
    try {
      const res = await api.updates.applyVt(true)
      if ('plan' in res) setPlan(res.plan)
      else setApplying(false)
    } catch { setApplying(false) }
  }

  const confirmApply = async () => {
    setConfirming(true)
    try {
      const response = await api.updates.applyVt(false)
      if ('job' in response) {
        track(response.job, { label: 'VoidTower update', onSucceeded: onRefresh })
      }
    } catch (error) {
      notify.error(error instanceof Error ? error.message : 'Failed to submit VoidTower update')
    } finally {
      setConfirming(false)
      setPlan(null)
      setApplying(false)
    }
  }

  const statusColor = () => {
    switch (info.update_status) {
      case 'update-available': return 'var(--accent-warning, #f59e0b)'
      case 'up-to-date':       return 'var(--accent-success)'
      default:                 return 'var(--text-muted)'
    }
  }
  const statusLabel = () => {
    switch (info.update_status) {
      case 'update-available': return 'Update available'
      case 'up-to-date':       return 'Up to date'
      default:                 return 'Not checked'
    }
  }

  return (
    <div className="space-y-3">
      {plan && (
        <ChangePlanModal
          plan={plan}
          confirming={confirming}
          onConfirm={confirmApply}
          onCancel={() => { setPlan(null); setApplying(false) }}
        />
      )}

      <DurableJobNotice job={trackedJob} label={trackedLabel} tracking={tracking} />

      <div className="flex flex-wrap gap-4 text-xs" style={{ color: 'var(--text-muted)' }}>
        <span>Image: <code style={{ color: 'var(--text-primary)' }}>{info.current_image ?? 'unknown'}</code></span>
        <span style={{ color: statusColor() }}>
          {statusLabel()}
        </span>
      </div>

      {info.update_detail && (
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>{info.update_detail}</p>
      )}
      <div className="flex flex-wrap gap-2">
        <Btn onClick={onRefresh} variant="secondary"><RefreshCw size={12} />Refresh</Btn>
        <Btn onClick={check} variant="secondary" disabled={checking}>
          {checking ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
          {checking ? 'Checking…' : 'Check for update'}
        </Btn>
        {info.update_status === 'update-available' && (
          <Btn onClick={preview} disabled={applying}>
            {applying ? <Loader2 size={12} className="animate-spin" /> : <ArrowUpCircle size={12} />}
            {applying ? 'Updating…' : 'Apply update'}
          </Btn>
        )}
      </div>

      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Running in Docker. Check pulls the latest image manifest; Apply recreates the container with it.
        Requires <code>/var/run/docker.sock</code> to be mounted.
      </p>
    </div>
  )
}

// ─── VoidTower — git mode ─────────────────────────────────────────────────────

function VtGitPanel({ info, onRefresh }: { info: VoidTowerUpdateInfo; onRefresh: () => void }) {
  const [applying, setApplying] = useState(false)
  const [rollingBack, setRollingBack] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const [showLog, setShowLog] = useState(false)
  const [applyPlan, setApplyPlan] = useState<ChangePlan | null>(null)
  const [rollbackPlan, setRollbackPlan] = useState<{ plan: ChangePlan; tag: string } | null>(null)
  const { trackedJob, trackedLabel, tracking, track } = useDurableJobTracker()

  const previewApply = async () => {
    setApplying(true)
    try {
      const res = await api.updates.applyVt(true)
      if ('plan' in res) setApplyPlan(res.plan)
      else setApplying(false)
    } catch { setApplying(false) }
  }

  const confirmApply = async () => {
    setConfirming(true)
    try {
      const response = await api.updates.applyVt(false)
      if ('job' in response) {
        track(response.job, { label: 'VoidTower update', onSucceeded: onRefresh })
      }
    } catch (error) {
      notify.error(error instanceof Error ? error.message : 'Failed to submit VoidTower update')
    } finally {
      setConfirming(false)
      setApplyPlan(null)
      setApplying(false)
    }
  }

  const previewRollback = async (tag: string) => {
    setRollingBack(true)
    try {
      const res = await api.updates.rollbackVt(tag, true)
      if ('plan' in res) setRollbackPlan({ plan: res.plan, tag })
      else setRollingBack(false)
    } catch { setRollingBack(false) }
  }

  const confirmRollback = async () => {
    if (!rollbackPlan) return
    setConfirming(true)
    try {
      const response = await api.updates.rollbackVt(rollbackPlan.tag, false)
      if ('job' in response) {
        track(response.job, { label: `VoidTower rollback to ${rollbackPlan.tag}`, onSucceeded: onRefresh })
      }
    } catch (error) {
      notify.error(error instanceof Error ? error.message : 'Failed to submit VoidTower rollback')
    } finally {
      setConfirming(false)
      setRollbackPlan(null)
      setRollingBack(false)
    }
  }

  const riskColor = (behind: number) =>
    behind === 0 ? 'var(--accent-success)' : behind <= 3 ? 'var(--accent-warning, #f59e0b)' : 'var(--accent-danger)'

  return (
    <div className="space-y-3">
      {applyPlan && (
        <ChangePlanModal
          plan={applyPlan}
          confirming={confirming}
          onConfirm={confirmApply}
          onCancel={() => { setApplyPlan(null); setApplying(false) }}
        />
      )}
      {rollbackPlan && (
        <ChangePlanModal
          plan={rollbackPlan.plan}
          confirming={confirming}
          onConfirm={confirmRollback}
          onCancel={() => { setRollbackPlan(null); setRollingBack(false) }}
        />
      )}

      <DurableJobNotice job={trackedJob} label={trackedLabel} tracking={tracking} />

      <div className="flex flex-wrap gap-4 text-xs" style={{ color: 'var(--text-muted)' }}>
        <span>Current: <code style={{ color: 'var(--text-primary)' }}>{info.current_commit}</code></span>
        <span>Remote: <code style={{ color: 'var(--text-primary)' }}>{info.remote_commit}</code></span>
        {info.behind > 0 && <span style={{ color: riskColor(info.behind) }}>Risk: {info.behind <= 3 ? 'Low' : info.behind <= 10 ? 'Medium' : 'High'}</span>}
      </div>

      {info.fetch_error && (
        <div className="flex items-center gap-2 text-xs px-3 py-2 rounded" style={{ background: 'var(--accent-danger)18', color: 'var(--accent-danger)' }}>
          <AlertTriangle size={13} />git fetch failed: {info.fetch_error}
        </div>
      )}

      {info.commits.length > 0 && (
        <div>
          <button onClick={() => setShowLog(s => !s)} className="flex items-center gap-1 text-xs mb-2" style={{ color: 'var(--text-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>
            {showLog ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
            {showLog ? 'Hide' : 'Show'} changelog ({info.commits.length} commit{info.commits.length !== 1 ? 's' : ''})
          </button>
          {showLog && (
            <div className="rounded overflow-hidden" style={{ border: '1px solid var(--border-subtle)' }}>
              {info.commits.map(c => (
                <div key={c.hash} className="flex items-start gap-3 px-3 py-2" style={{ borderBottom: '1px solid var(--border-subtle)', fontSize: 12 }}>
                  <code style={{ color: 'var(--accent-primary)', flexShrink: 0 }}>{c.hash}</code>
                  <span style={{ color: 'var(--text-primary)', flex: 1 }}>{c.subject}</span>
                  <span style={{ color: 'var(--text-muted)', flexShrink: 0 }}>{c.author} · {c.date}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <Btn onClick={onRefresh} variant="secondary"><RefreshCw size={12} />Refresh</Btn>
        {info.behind > 0 && (
          <Btn onClick={previewApply} disabled={applying}>
            {applying ? <Loader2 size={12} className="animate-spin" /> : <ArrowUpCircle size={12} />}
            {applying ? 'Updating…' : 'Apply update'}
          </Btn>
        )}
      </div>

      {info.backup_tags.length > 0 && (
        <details className="mt-1">
          <summary className="text-xs cursor-pointer" style={{ color: 'var(--text-muted)' }}>
            Rollback points ({info.backup_tags.length})
          </summary>
          <div className="mt-2 space-y-1">
            {info.backup_tags.map(tag => (
              <div key={tag} className="flex items-center justify-between px-3 py-1.5 rounded" style={{ background: 'var(--bg-elevated)' }}>
                <code className="text-xs" style={{ color: 'var(--text-primary)' }}>{tag}</code>
                <Btn onClick={() => previewRollback(tag)} variant="danger" disabled={rollingBack}>
                  <RotateCcw size={11} />Roll back
                </Btn>
              </div>
            ))}
          </div>
        </details>
      )}
    </div>
  )
}

// ─── VoidTower section (mode-aware) ──────────────────────────────────────────

function VoidTowerSection() {
  const [info, setInfo] = useState<VoidTowerUpdateInfo | null>(null)
  const [loading, setLoading] = useState(true)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      setInfo(await api.updates.infoVt())
    } finally { setLoading(false) }
  }, [])

  useEffect(() => { load() }, [load])

  const statusBadge = () => {
    if (!info) return null
    if (info.mode === 'docker') {
      if (info.update_status === 'update-available') return <Badge label="Update available" color="var(--accent-warning, #f59e0b)" />
      if (info.update_status === 'up-to-date')       return <Badge label="Up to date" color="var(--accent-success)" />
      return null
    }
    if (info.behind > 0) return <Badge label={`${info.behind} update${info.behind !== 1 ? 's' : ''} available`} color="var(--accent-warning, #f59e0b)" />
    if (info.behind === 0 && !loading) return <Badge label="Up to date" color="var(--accent-success)" />
    return null
  }

  return (
    <div style={card} className="space-y-3">
      <SectionHeader
        icon={info?.mode === 'docker' ? Box : ArrowUpCircle}
        title="VoidTower Application"
        badge={!loading ? statusBadge() : null}
      />
      {loading && <div className="flex items-center gap-2 text-xs" style={{ color: 'var(--text-muted)' }}><Loader2 size={13} className="animate-spin" />Loading…</div>}
      {info && info.mode === 'docker' && <VtDockerPanel info={info} onRefresh={load} />}
      {info && info.mode === 'git'    && <VtGitPanel    info={info} onRefresh={load} />}
    </div>
  )
}

// ─── Docker images section ────────────────────────────────────────────────────

function DockerSection() {
  const [rows, setRows] = useState<DockerUpdateRow[]>([])
  const [loading, setLoading] = useState(true)
  const [applying, setApplying] = useState<string | null>(null)
  const [confirming, setConfirming] = useState(false)
  const [applyPlan, setApplyPlan] = useState<{ plan: ChangePlan; containerId: string } | null>(null)
  const { trackedJob, trackedLabel, tracking, track } = useDurableJobTracker()
  const checking = tracking && trackedJob?.action === 'update.docker.check'

  const load = useCallback(async () => {
    try {
      setRows(await api.updates.infoDocker())
    } finally { setLoading(false) }
  }, [])

  useEffect(() => { load() }, [load])

  const check = async () => {
    try {
      const { job } = await api.updates.checkDocker()
      track(job, { label: 'Docker image check', onSucceeded: load })
    } catch (error) {
      notify.error(error instanceof Error ? error.message : 'Failed to submit Docker image check')
    }
  }

  const previewApply = async (containerId: string) => {
    setApplying(containerId)
    try {
      const res = await api.updates.dockerApply(containerId, true)
      if ('plan' in res) setApplyPlan({ plan: res.plan, containerId })
      else setApplying(null)
    } catch { setApplying(null) }
  }

  const confirmApply = async () => {
    if (!applyPlan) return
    setConfirming(true)
    try {
      const response = await api.updates.dockerApply(applyPlan.containerId, false)
      if ('job' in response) {
        track(response.job, { label: 'Container image update', onSucceeded: load })
      }
    } catch (error) {
      notify.error(error instanceof Error ? error.message : 'Failed to submit container image update')
    } finally {
      setConfirming(false)
      setApplying(null)
      setApplyPlan(null)
    }
  }

  const statusIcon = (s: DockerUpdateRow['status']) => {
    if (s === 'up-to-date')       return <CheckCircle size={13} style={{ color: 'var(--accent-success)' }} />
    if (s === 'update-available') return <ArrowUpCircle size={13} style={{ color: 'var(--accent-warning, #f59e0b)' }} />
    return <span style={{ color: 'var(--text-disabled)', fontSize: 11 }}>—</span>
  }

  const updatesAvailable = rows.filter(r => r.status === 'update-available').length

  return (
    <div style={card} className="space-y-3">
      {applyPlan && (
        <ChangePlanModal
          plan={applyPlan.plan}
          confirming={confirming}
          onConfirm={confirmApply}
          onCancel={() => { setApplyPlan(null); setApplying(null) }}
        />
      )}
      <DurableJobNotice job={trackedJob} label={trackedLabel} tracking={tracking} />
      <SectionHeader icon={Container} title="Docker Images" badge={
        updatesAvailable > 0
          ? <Badge label={`${updatesAvailable} update${updatesAvailable !== 1 ? 's' : ''}`} color="var(--accent-warning, #f59e0b)" />
          : rows.length > 0 && rows.every(r => r.status === 'up-to-date')
            ? <Badge label="All up to date" color="var(--accent-success)" />
            : null
      } />

      {loading ? (
        <div className="flex items-center gap-2 text-xs" style={{ color: 'var(--text-muted)' }}><Loader2 size={13} className="animate-spin" />Loading…</div>
      ) : rows.length === 0 ? (
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>No running containers found.</p>
      ) : (
        <div className="rounded overflow-hidden" style={{ border: '1px solid var(--border-subtle)' }}>
          <table className="w-full text-xs" style={{ borderCollapse: 'collapse' }}>
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)' }}>
                {['Container', 'Image', 'Status', ''].map(h => (
                  <th key={h} className="px-3 py-2 text-left font-medium" style={{ color: 'var(--text-muted)' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map(row => (
                <tr key={row.container_id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  <td className="px-3 py-2 font-medium" style={{ color: 'var(--text-primary)' }}>{row.container_name}</td>
                  <td className="px-3 py-2 font-mono" style={{ color: 'var(--text-secondary)' }}>{row.image}</td>
                  <td className="px-3 py-2">
                    <div className="flex items-center gap-1.5">
                      {statusIcon(row.status)}
                      <span style={{ color: 'var(--text-muted)' }}>
                        {row.status === 'unknown' ? 'Not checked' : row.status.replace('-', ' ')}
                        {row.detail && ` — ${row.detail}`}
                      </span>
                    </div>
                  </td>
                  <td className="px-3 py-2">
                    {row.status === 'update-available' && (
                      <Btn onClick={() => previewApply(row.container_id)} disabled={applying === row.container_id}>
                        {applying === row.container_id ? <Loader2 size={11} className="animate-spin" /> : <ArrowUpCircle size={11} />}
                        {applying === row.container_id ? 'Updating…' : 'Pull & recreate'}
                      </Btn>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div className="flex gap-2">
        <Btn onClick={check} variant="secondary" disabled={checking}>
          <RefreshCw size={12} className={checking ? 'animate-spin' : ''} />
          {checking ? 'Checking…' : 'Check all images'}
        </Btn>
        <Btn onClick={load} variant="secondary"><RefreshCw size={12} />Refresh</Btn>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Checking images pulls the latest manifest from the registry. If a newer image is found it will be downloaded automatically.
      </p>
    </div>
  )
}

// ─── Odysseus section ────────────────────────────────────────────────────────

function OdysseusSection() {
  const [info, setInfo] = useState<OdysseusUpdateInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const { trackedJob, trackedLabel, tracking, track } = useDurableJobTracker()

  const load = useCallback(async () => {
    setLoading(true)
    try {
      setInfo(await api.updates.infoOdysseus())
    } catch {
      setInfo(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { load() }, [load])

  const apply = async () => {
    try {
      const { job } = await api.updates.applyOdysseus()
      track(job, { label: 'Odysseus update', onSucceeded: load })
    } catch (error) {
      notify.error(error instanceof Error ? error.message : 'Failed to submit Odysseus update')
    }
  }

  return (
    <div style={card} className="space-y-3">
      <SectionHeader
        icon={Bot}
        title="Odysseus"
        badge={info?.installed
          ? info.behind > 0
            ? <Badge label={`${info.behind} update${info.behind !== 1 ? 's' : ''} available`} color="var(--accent-warning, #f59e0b)" />
            : <Badge label="Up to date" color="var(--accent-success)" />
          : <Badge label="Not installed" color="var(--text-muted)" />}
      />
      <DurableJobNotice job={trackedJob} label={trackedLabel} tracking={tracking} />
      {loading ? (
        <div className="flex items-center gap-2 text-xs" style={{ color: 'var(--text-muted)' }}>
          <Loader2 size={13} className="animate-spin" />Loading…
        </div>
      ) : info?.installed ? (
        <>
          <div className="flex flex-wrap gap-4 text-xs" style={{ color: 'var(--text-muted)' }}>
            <span>Current: <code style={{ color: 'var(--text-primary)' }}>{info.current_commit}</code></span>
            <span>Remote: <code style={{ color: 'var(--text-primary)' }}>{info.remote_commit}</code></span>
          </div>
          <div className="flex gap-2">
            <Btn onClick={load} variant="secondary"><RefreshCw size={12} />Refresh</Btn>
            {info.behind > 0 && (
              <Btn onClick={apply} disabled={tracking}>
                {tracking ? <Loader2 size={12} className="animate-spin" /> : <ArrowUpCircle size={12} />}
                Submit update
              </Btn>
            )}
          </div>
        </>
      ) : (
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
          Odysseus is not installed at the configured update target.
        </p>
      )}
    </div>
  )
}

// ─── OS packages section ──────────────────────────────────────────────────────

function OsSection() {
  const [info, setInfo] = useState<OsUpdateInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [applying, setApplying] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const [showPkgs, setShowPkgs] = useState(false)
  const [plan, setPlan] = useState<ChangePlan | null>(null)
  const { trackedJob, trackedLabel, tracking, track } = useDurableJobTracker()

  const load = useCallback(async () => {
    setLoading(true)
    try {
      setInfo(await api.updates.infoOs())
    } finally { setLoading(false) }
  }, [])

  useEffect(() => { load() }, [load])

  const previewApply = async () => {
    setApplying(true)
    try {
      const res = await api.updates.applyOs(true)
      if ('plan' in res) setPlan(res.plan)
    } finally { setApplying(false) }
  }

  const confirmApply = async () => {
    setConfirming(true)
    try {
      const response = await api.updates.applyOs(false)
      if ('job' in response) {
        track(response.job, { label: 'Operating-system update', onSucceeded: load })
      }
    } catch (error) {
      notify.error(error instanceof Error ? error.message : 'Failed to submit operating-system update')
    } finally {
      setConfirming(false)
      setPlan(null)
    }
  }

  return (
    <div style={card} className="space-y-3">
      {plan && (
        <ChangePlanModal
          plan={plan}
          confirming={confirming}
          onConfirm={confirmApply}
          onCancel={() => setPlan(null)}
        />
      )}
      <DurableJobNotice job={trackedJob} label={trackedLabel} tracking={tracking} />
      <SectionHeader icon={Package} title="OS Packages" badge={
        !loading && info ? (
          info.error ? <Badge label="Error" color="var(--accent-danger)" />
          : info.available ? <Badge label={`${info.count} upgradable`} color="var(--accent-warning, #f59e0b)" />
          : <Badge label="Up to date" color="var(--accent-success)" />
        ) : null
      } />

      {loading && <div className="flex items-center gap-2 text-xs" style={{ color: 'var(--text-muted)' }}><Loader2 size={13} className="animate-spin" />Checking packages…</div>}

      {info && (
        <>
          <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
            Package manager: <code style={{ color: 'var(--text-primary)' }}>{info.package_manager}</code>
          </p>

          {info.error && (
            <div className="flex items-center gap-2 text-xs px-3 py-2 rounded" style={{ background: 'var(--accent-danger)18', color: 'var(--accent-danger)' }}>
              <AlertTriangle size={13} />{info.error}
            </div>
          )}

          {info.packages.length > 0 && (
            <div>
              <button onClick={() => setShowPkgs(s => !s)} className="flex items-center gap-1 text-xs mb-2" style={{ color: 'var(--text-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>
                {showPkgs ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                {showPkgs ? 'Hide' : 'Show'} packages ({info.count})
              </button>
              {showPkgs && (
                <div className="rounded p-3 font-mono text-xs overflow-auto max-h-48" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
                  {info.packages.map((p, i) => <div key={i}>{p}</div>)}
                </div>
              )}
            </div>
          )}

          <div className="flex flex-wrap gap-2">
            <Btn onClick={load} variant="secondary" disabled={loading}><RefreshCw size={12} />Refresh</Btn>
            {!info.error && (
              <>
                <Btn onClick={previewApply} variant="secondary" disabled={applying}>
                  {applying ? <Loader2 size={12} className="animate-spin" /> : <Server size={12} />}
                  Preview
                </Btn>
                {info.available && (
                  <Btn onClick={previewApply} disabled={applying}>
                    {applying ? <Loader2 size={12} className="animate-spin" /> : <ArrowUpCircle size={12} />}
                    {applying ? 'Loading…' : `Apply ${info.count} update${info.count !== 1 ? 's' : ''}`}
                  </Btn>
                )}
              </>
            )}
          </div>

        </>
      )}
    </div>
  )
}

// ─── Page ─────────────────────────────────────────────────────────────────────

export default function UpdatesPage() {
  return (
    <div className="p-6 space-y-6 max-w-4xl mx-auto">
      <div className="flex items-center gap-3">
        <ArrowUpCircle size={22} style={{ color: 'var(--accent-primary)' }} />
        <h1 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>Updates</h1>
      </div>
      <VoidTowerSection />
      <DockerSection />
      <OdysseusSection />
      <OsSection />
    </div>
  )
}

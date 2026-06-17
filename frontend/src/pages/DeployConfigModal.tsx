import { useRef, useState } from 'react'
import { X, Plus, Loader2, CheckCircle2, AlertCircle, KeyRound } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import type { AppDef } from '@/api/types'
import LogViewer from '@/components/ui/LogViewer'
import Button from '@/components/ui/Button'

interface Props {
  app: AppDef
  onClose: () => void
  onDeployed: (projectName: string) => void
}

type Phase = 'config' | 'deploying' | 'success' | 'error'

function composeToYaml(compose: Record<string, unknown>): string {
  return JSON.stringify(compose, null, 2)
}

export default function DeployConfigModal({ app, onClose, onDeployed }: Props) {
  const manualRequired = (app.required_env ?? []).filter(e => !e.generate)
  const autoRequired   = (app.required_env ?? []).filter(e => !!e.generate)

  const [envPairs, setEnvPairs] = useState<[string, string][]>(() =>
    manualRequired.map(e => [e.key, e.default ?? ''] as [string, string])
  )
  const [phase, setPhase]       = useState<Phase>('config')
  const [logs, setLogs]         = useState<string[]>([])
  const [errorMsg, setErrorMsg] = useState('')
  const [projectName, setProjectName] = useState('')
  const [generatedEnv, setGeneratedEnv] = useState<Record<string, string>>({})
  const [cancelling, setCancelling]   = useState(false)
  const cancellingRef = useRef(false)

  // Matches the backend's default project naming (DeployRequest.project_name is always
  // omitted here), so we know the name to cancel before the deploy call resolves.
  const pendingProjectName = `vt-${app.id}`

  const addPair = () => setEnvPairs(p => [...p, ['', '']])

  const updatePair = (i: number, side: 0 | 1, val: string) =>
    setEnvPairs(p => p.map((pair, idx) =>
      idx === i ? (side === 0 ? [val, pair[1]] : [pair[0], val]) : pair
    ))

  const removePair = (i: number) => setEnvPairs(p => p.filter((_, idx) => idx !== i))

  const missingRequired = manualRequired.filter(r =>
    !envPairs.find(([k, v]) => k === r.key && v.trim())
  )

  const deploy = async () => {
    setPhase('deploying')
    setCancelling(false)
    cancellingRef.current = false
    const envOverrides: Record<string, string> = {}
    for (const [k, v] of envPairs) {
      if (k.trim()) envOverrides[k.trim()] = v
    }
    const overrides = Object.keys(envOverrides).length ? envOverrides : undefined
    try {
      const result = await api.apps.deploy(app.id, undefined, overrides)
      setProjectName(result.project_name)
      setGeneratedEnv(result.generated_env ?? {})
      try {
        const logData = await api.apps.logs(result.project_name)
        setLogs(logData.lines)
      } catch {
        setLogs(['(no logs available)'])
      }
      setPhase('success')
    } catch (e) {
      setErrorMsg(cancellingRef.current ? 'Deployment cancelled' : (e instanceof ApiClientError ? e.message : 'Deploy failed'))
      setPhase('error')
    }
  }

  const cancelDeploy = async () => {
    setCancelling(true)
    cancellingRef.current = true
    try {
      await api.apps.cancelDeploy(pendingProjectName)
    } catch {
      // The in-flight deploy() call's own catch block will surface whatever error
      // results from the cancelled docker compose process.
    }
  }

  const dismiss = () => {
    if (phase === 'success') onDeployed(projectName)
    else onClose()
  }

  const inputStyle = (invalid?: boolean): React.CSSProperties => ({
    background: 'var(--bg-elevated)',
    border: `1px solid ${invalid ? 'var(--accent-warning)' : 'var(--border-subtle)'}`,
    borderRadius: 6,
    color: 'var(--text-primary)',
    padding: '5px 8px',
    fontSize: 12,
    fontFamily: 'monospace',
    width: '100%',
    boxSizing: 'border-box',
  })

  // Extra pairs (not from manualRequired)
  const extraPairs = envPairs.filter(([k]) => !manualRequired.find(r => r.key === k))

  return (
    <div
      style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.55)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000 }}
      onClick={e => e.target === e.currentTarget && dismiss()}
    >
      <div style={{
        background: 'var(--bg-card)',
        border: '1px solid var(--border)',
        borderRadius: 12,
        padding: 24,
        width: 600,
        maxHeight: '90vh',
        overflowY: 'auto',
        display: 'flex',
        flexDirection: 'column',
        gap: 16,
      }}>
        {/* Header */}
        <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 8 }}>
          <div>
            <div style={{ fontWeight: 600, fontSize: 15, color: 'var(--text-primary)' }}>Deploy {app.name}</div>
            <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>{app.description}</div>
          </div>
          <button onClick={dismiss} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: 2 }}>
            <X size={16} />
          </button>
        </div>

        {phase === 'config' && (
          <>
            {/* Auto-generated secrets notice */}
            {autoRequired.length > 0 && (
              <div style={{
                display: 'flex', alignItems: 'flex-start', gap: 8, padding: '8px 12px',
                borderRadius: 8, background: 'var(--accent-primary)12', border: '1px solid var(--accent-primary)33',
              }}>
                <KeyRound size={14} style={{ color: 'var(--accent-primary)', marginTop: 1, flexShrink: 0 }} />
                <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>
                  <span style={{ fontWeight: 500, color: 'var(--text-primary)' }}>Auto-generated secrets: </span>
                  {autoRequired.map(e => e.key).join(', ')} will be securely generated on deploy.
                </div>
              </div>
            )}

            {/* Compose preview */}
            {app.compose && (
              <div>
                <div style={{ fontSize: 11, fontWeight: 500, color: 'var(--text-muted)', marginBottom: 6, textTransform: 'uppercase', letterSpacing: '0.05em' }}>
                  Compose configuration
                </div>
                <pre style={{
                  fontSize: 11,
                  background: 'var(--terminal-bg)',
                  color: 'var(--terminal-green)',
                  border: '1px solid var(--border-subtle)',
                  borderRadius: 8,
                  padding: '10px 12px',
                  overflowX: 'auto',
                  maxHeight: 220,
                  margin: 0,
                  lineHeight: 1.5,
                }}>
                  {composeToYaml(app.compose)}
                </pre>
              </div>
            )}

            {/* Environment variables */}
            <div>
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                <div style={{ fontSize: 11, fontWeight: 500, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.05em' }}>
                  Environment overrides
                </div>
                <button
                  onClick={addPair}
                  style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 11, background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent-primary)' }}
                >
                  <Plus size={11} /> Add variable
                </button>
              </div>

              {/* Required fields that need manual entry */}
              {manualRequired.map(req => {
                const pairIdx = envPairs.findIndex(([k]) => k === req.key)
                const value   = pairIdx >= 0 ? envPairs[pairIdx][1] : ''
                const missing = !value.trim()
                return (
                  <div key={req.key} style={{ marginBottom: 10 }}>
                    <div style={{ fontSize: 11, marginBottom: 2, display: 'flex', gap: 6, alignItems: 'center' }}>
                      <code style={{ color: missing ? 'var(--accent-warning)' : 'var(--text-primary)' }}>{req.key}</code>
                      {missing && <span style={{ fontSize: 10, color: 'var(--accent-warning)' }}>required</span>}
                    </div>
                    <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: 4 }}>{req.description}</div>
                    <input
                      value={value}
                      onChange={e => {
                        if (pairIdx >= 0) updatePair(pairIdx, 1, e.target.value)
                        else setEnvPairs(p => [...p, [req.key, e.target.value]])
                      }}
                      placeholder={`Enter ${req.key}…`}
                      style={inputStyle(missing)}
                    />
                  </div>
                )
              })}

              {/* Extra / optional pairs */}
              {extraPairs.map((pair, i) => {
                const realIdx = envPairs.indexOf(pair)
                return (
                  <div key={i} style={{ display: 'flex', gap: 4, marginBottom: 4, alignItems: 'center' }}>
                    <input
                      value={pair[0]}
                      onChange={e => updatePair(realIdx, 0, e.target.value)}
                      placeholder="KEY"
                      style={{ ...inputStyle(), flex: 1 }}
                    />
                    <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>=</span>
                    <input
                      value={pair[1]}
                      onChange={e => updatePair(realIdx, 1, e.target.value)}
                      placeholder="value"
                      style={{ ...inputStyle(), flex: 2 }}
                    />
                    <button onClick={() => removePair(realIdx)} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: '0 4px', flexShrink: 0 }}>
                      <X size={12} />
                    </button>
                  </div>
                )
              })}

              {extraPairs.length === 0 && manualRequired.length === 0 && (
                <p style={{ fontSize: 12, color: 'var(--text-muted)', fontStyle: 'italic', margin: 0 }}>
                  No overrides — click "Add variable" to set env vars before deploying.
                </p>
              )}
            </div>

            {/* Footer buttons */}
            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
              <button onClick={onClose} style={{ padding: '6px 14px', borderRadius: 6, border: '1px solid var(--border)', background: 'transparent', color: 'var(--text-primary)', cursor: 'pointer', fontSize: 13 }}>
                Cancel
              </button>
              <Button
                onClick={deploy}
                disabled={missingRequired.length > 0}
                title={missingRequired.length > 0 ? `Fill in required fields: ${missingRequired.map(r => r.key).join(', ')}` : undefined}
              >
                Deploy {app.name}
              </Button>
            </div>
          </>
        )}

        {phase === 'deploying' && (
          <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 12, padding: '32px 0' }}>
            <Loader2 size={28} className="animate-spin" style={{ color: 'var(--accent-primary)' }} />
            <div style={{ fontSize: 13, color: 'var(--text-secondary)' }}>
              Running <code>docker compose up</code> for {app.name}…
            </div>
            <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
              This may take a minute while the image is pulled and containers start.
            </div>
            <button
              onClick={cancelDeploy}
              disabled={cancelling}
              style={{ marginTop: 4, padding: '6px 14px', borderRadius: 6, border: '1px solid var(--border)', background: 'transparent', color: 'var(--text-secondary)', cursor: cancelling ? 'default' : 'pointer', fontSize: 12, opacity: cancelling ? 0.6 : 1 }}
            >
              {cancelling ? 'Cancelling…' : 'Cancel deployment'}
            </button>
          </div>
        )}

        {phase === 'success' && (
          <>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 12px', borderRadius: 8, background: 'var(--accent-success)18', border: '1px solid var(--accent-success)44', color: 'var(--accent-success)', fontSize: 13 }}>
              <CheckCircle2 size={15} />
              <span>{app.name} deployed as <code>{projectName}</code></span>
            </div>
            {Object.keys(generatedEnv).length > 0 && (
              <div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 11, fontWeight: 500, color: 'var(--text-muted)', marginBottom: 6, textTransform: 'uppercase', letterSpacing: '0.05em' }}>
                  <KeyRound size={12} /> Generated secrets — save these now, they won't be shown again
                </div>
                <div style={{ background: 'var(--terminal-bg)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: '10px 12px', fontFamily: 'monospace', fontSize: 11 }}>
                  {Object.entries(generatedEnv).map(([k, v]) => (
                    <div key={k} style={{ color: 'var(--terminal-green)', wordBreak: 'break-all' }}>
                      <span style={{ color: 'var(--text-muted)' }}>{k}=</span>{v}
                    </div>
                  ))}
                </div>
              </div>
            )}
            <div>
              <div style={{ fontSize: 11, fontWeight: 500, color: 'var(--text-muted)', marginBottom: 6, textTransform: 'uppercase', letterSpacing: '0.05em' }}>
                Deployment output
              </div>
              <LogViewer lines={logs} maxHeight={280} />
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <Button onClick={dismiss}>View in Deployed tab</Button>
            </div>
          </>
        )}

        {phase === 'error' && (
          <>
            <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8, padding: '10px 12px', borderRadius: 8, background: 'var(--accent-danger)18', border: '1px solid var(--accent-danger)44' }}>
              <AlertCircle size={15} style={{ color: 'var(--accent-danger)', marginTop: 1, flexShrink: 0 }} />
              <pre style={{ fontSize: 12, color: 'var(--accent-danger)', margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                {errorMsg}
              </pre>
            </div>
            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
              <button onClick={() => setPhase('config')} style={{ padding: '6px 14px', borderRadius: 6, border: '1px solid var(--border)', background: 'transparent', color: 'var(--text-primary)', cursor: 'pointer', fontSize: 13 }}>
                Back
              </button>
              <button onClick={onClose} style={{ padding: '6px 14px', borderRadius: 6, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer', fontSize: 13 }}>
                Close
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}

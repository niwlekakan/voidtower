import { useState } from 'react'
import { useAiosStore } from '@/aios/store/aios'
import AiBadge from '@/components/ui/AiBadge'
import type { AiLevel } from '@/components/ui/AiBadge'

// ── AI level map (mirrors AiosPanel.tsx) ──────────────────────────────────────

const COMPONENT_AI_LEVELS: Record<string, AiLevel> = {
  ai:         'native',
  services:   'aware',
  containers: 'aware',
  terminal:   'aware',
}

// ── Tab type ──────────────────────────────────────────────────────────────────

type InspectorTab = 'overview' | 'actions' | 'info'

// ── Keyboard shortcuts reference ──────────────────────────────────────────────

const KEYBOARD_SHORTCUTS = [
  { keys: 'Esc',              description: 'Minimize focused panel'   },
  { keys: 'Ctrl+W',          description: 'Close focused panel'       },
  { keys: 'Ctrl+M',          description: 'Close all panels'          },
  { keys: 'Ctrl+Shift+←',   description: 'Snap focused panel left'   },
  { keys: 'Ctrl+Shift+→',   description: 'Snap focused panel right'  },
  { keys: 'Ctrl+Shift+↑',   description: 'Fullscreen focused panel'  },
  { keys: 'Ctrl+Shift+↓',   description: 'Restore focused panel'     },
  { keys: 'Alt+Tab',         description: 'Cycle panels'              },
  { keys: 'Ctrl+1–4',       description: 'Switch workspace'           },
]

// ── Small action button ───────────────────────────────────────────────────────

function ActionBtn({
  label,
  onClick,
  danger = false,
}: {
  label: string
  onClick: () => void
  danger?: boolean
}) {
  return (
    <button
      onClick={onClick}
      className="w-full text-left px-3 py-2 rounded text-xs transition-colors hover:opacity-80"
      style={{
        background: danger ? 'var(--accent-danger, #f8717118)' : 'var(--bg-elevated, rgba(255,255,255,0.06))',
        border: `1px solid ${danger ? 'var(--accent-danger, #f87171)44' : 'rgba(255,255,255,0.08)'}`,
        color: danger ? 'var(--accent-danger, #f87171)' : 'rgba(255,255,255,0.8)',
        cursor: 'pointer',
      }}
    >
      {label}
    </button>
  )
}

// ── Status dot ────────────────────────────────────────────────────────────────

function StatusDot({ status }: { status: 'running' | 'stopped' | 'unknown' }) {
  const color =
    status === 'running' ? 'var(--accent-success, #22c55e)' :
    status === 'stopped' ? 'var(--text-disabled, rgba(255,255,255,0.2))' :
    'var(--accent-warning, #f59e0b)'
  return (
    <span
      style={{
        display: 'inline-block',
        width: 8, height: 8,
        borderRadius: '50%',
        background: color,
        flexShrink: 0,
      }}
    />
  )
}

// ── Main component ────────────────────────────────────────────────────────────

export default function AiosInspectorPanel() {
  const [activeTab, setActiveTab] = useState<InspectorTab>('overview')

  const { panels, focusedId, minimizePanel, closePanel, snapPanel } = useAiosStore()

  // The inspector inspects the most-recently-focused non-inspector panel
  const inspectorPanel = panels.find((p) => p.component === 'inspector')
  const targetPanel = panels
    .filter((p) => p.component !== 'inspector')
    .find((p) => p.id === focusedId)
    ?? panels
      .filter((p) => p.component !== 'inspector')
      .sort((a, b) => b.zIndex - a.zIndex)[0]
    ?? null

  const aiLevel: AiLevel = targetPanel
    ? (COMPONENT_AI_LEVELS[targetPanel.component] ?? 'none')
    : 'none'

  // Classify panel type for action visibility
  const isAiPanel = aiLevel === 'native' || aiLevel === 'aware'
  const isAppOrContainer = targetPanel
    ? ['containers', 'services', 'apps'].includes(targetPanel.component)
    : false

  // Derive a rough running/stopped status
  const panelStatus: 'running' | 'stopped' | 'unknown' =
    targetPanel?.layoutMode === 'minimized' ? 'stopped' :
    targetPanel ? 'running' :
    'unknown'

  const tabs: { id: InspectorTab; label: string }[] = [
    { id: 'overview', label: 'Overview' },
    { id: 'actions',  label: 'Actions'  },
    { id: 'info',     label: 'Info'     },
  ]

  return (
    <div
      style={{
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        background: 'var(--bg-panel, rgba(0,0,0,0.6))',
        color: 'var(--text-primary)',
        fontSize: 12,
      }}
    >
      {/* Tab bar */}
      <div
        style={{
          display: 'flex',
          borderBottom: '1px solid rgba(255,255,255,0.08)',
          flexShrink: 0,
        }}
      >
        {tabs.map(({ id, label }) => (
          <button
            key={id}
            onClick={() => setActiveTab(id)}
            style={{
              flex: 1,
              padding: '8px 4px',
              fontSize: 11,
              fontWeight: 500,
              background: 'none',
              border: 'none',
              borderBottom: `2px solid ${activeTab === id ? 'var(--accent-primary, #8b5cf6)' : 'transparent'}`,
              color: activeTab === id ? 'var(--accent-primary, #8b5cf6)' : 'rgba(255,255,255,0.45)',
              cursor: 'pointer',
              transition: 'color 0.12s, border-color 0.12s',
            }}
          >
            {label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      <div style={{ flex: 1, overflowY: 'auto', padding: 12 }}>

        {/* ── Overview ─────────────────────────────────────────────────────── */}
        {activeTab === 'overview' && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {!targetPanel ? (
              <p style={{ color: 'rgba(255,255,255,0.35)', fontSize: 11, textAlign: 'center', marginTop: 24 }}>
                No panel focused
              </p>
            ) : (
              <>
                {/* Title + icon */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 2 }}>
                  <span style={{ fontSize: 18 }}>{targetPanel.icon}</span>
                  <span style={{ fontWeight: 600, fontSize: 13, color: 'rgba(255,255,255,0.9)' }}>
                    {targetPanel.title}
                  </span>
                </div>

                <Row label="Component" value={targetPanel.component} mono />
                <Row label="Type" value={targetPanel.type} />
                <Row label="Layout" value={targetPanel.layoutMode} />
                <Row label="Workspace" value={String(targetPanel.workspaceIndex + 1)} />
                <Row label="Z-Index" value={String(targetPanel.zIndex)} />

                {/* Status */}
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <span style={{ color: 'rgba(255,255,255,0.4)', fontSize: 11 }}>Status</span>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
                    <StatusDot status={panelStatus} />
                    <span style={{ color: 'rgba(255,255,255,0.7)', fontSize: 11 }}>{panelStatus}</span>
                  </div>
                </div>

                {/* AI level */}
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <span style={{ color: 'rgba(255,255,255,0.4)', fontSize: 11 }}>AI Integration</span>
                  {aiLevel !== 'none'
                    ? <AiBadge level={aiLevel} />
                    : <span style={{ color: 'rgba(255,255,255,0.25)', fontSize: 11 }}>None</span>
                  }
                </div>

                {/* Geometry (floating only) */}
                {targetPanel.layoutMode === 'floating' && (
                  <div
                    style={{
                      marginTop: 4,
                      padding: '8px 10px',
                      borderRadius: 6,
                      background: 'rgba(255,255,255,0.04)',
                      border: '1px solid rgba(255,255,255,0.08)',
                    }}
                  >
                    <p style={{ color: 'rgba(255,255,255,0.35)', fontSize: 10, marginBottom: 4 }}>GEOMETRY</p>
                    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '2px 12px' }}>
                      <Geo label="x" value={targetPanel.x} />
                      <Geo label="y" value={targetPanel.y} />
                      <Geo label="w" value={targetPanel.w} />
                      <Geo label="h" value={targetPanel.h} />
                    </div>
                  </div>
                )}
              </>
            )}
          </div>
        )}

        {/* ── Actions ──────────────────────────────────────────────────────── */}
        {activeTab === 'actions' && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {!targetPanel ? (
              <p style={{ color: 'rgba(255,255,255,0.35)', fontSize: 11, textAlign: 'center', marginTop: 24 }}>
                No panel focused
              </p>
            ) : (
              <>
                <p style={{ color: 'rgba(255,255,255,0.3)', fontSize: 10, marginBottom: 2 }}>GENERAL</p>
                <ActionBtn label="Minimize" onClick={() => minimizePanel(targetPanel.id)} />
                <ActionBtn label="Snap left"  onClick={() => snapPanel(targetPanel.id, 'left-half')} />
                <ActionBtn label="Snap right" onClick={() => snapPanel(targetPanel.id, 'right-half')} />
                <ActionBtn label="Close" danger onClick={() => closePanel(targetPanel.id)} />

                {isAppOrContainer && (
                  <>
                    <p style={{ color: 'rgba(255,255,255,0.3)', fontSize: 10, marginTop: 6, marginBottom: 2 }}>APP / CONTAINER</p>
                    <ActionBtn
                      label="Open in App Vault"
                      onClick={() => {
                        const { openPanel: op, activeWorkspace } = useAiosStore.getState()
                        const existing = panels.find((p) => p.component === 'apps')
                        if (existing) {
                          useAiosStore.getState().focusPanel(existing.id)
                        } else {
                          const vw = window.innerWidth
                          const vh = window.innerHeight
                          op({
                            type: 'app', component: 'apps', title: 'App Vault', icon: '',
                            layoutMode: 'floating',
                            x: 40, y: 40, w: Math.min(960, vw - 80), h: Math.min(680, vh - 80),
                            savedX: 40, savedY: 40, savedW: Math.min(960, vw - 80), savedH: Math.min(680, vh - 80),
                            pinned: false, workspaceIndex: activeWorkspace,
                          })
                        }
                        if (inspectorPanel) closePanel(inspectorPanel.id)
                      }}
                    />
                  </>
                )}

                {isAiPanel && (
                  <>
                    <p style={{ color: 'rgba(255,255,255,0.3)', fontSize: 10, marginTop: 6, marginBottom: 2 }}>AI</p>
                    <ActionBtn
                      label="Clear context"
                      onClick={() => {
                        // Post a message to any iframe inside the panel
                        const iframe = document.querySelector<HTMLIFrameElement>(
                          `[data-panel-id="${targetPanel.id}"] iframe`
                        )
                        iframe?.contentWindow?.postMessage({ type: 'clear-context' }, '*')
                      }}
                    />
                    <ActionBtn
                      label="New conversation"
                      onClick={() => {
                        const iframe = document.querySelector<HTMLIFrameElement>(
                          `[data-panel-id="${targetPanel.id}"] iframe`
                        )
                        iframe?.contentWindow?.postMessage({ type: 'new-conversation' }, '*')
                      }}
                    />
                  </>
                )}
              </>
            )}
          </div>
        )}

        {/* ── Info ─────────────────────────────────────────────────────────── */}
        {activeTab === 'info' && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            <p style={{ color: 'rgba(255,255,255,0.3)', fontSize: 10, marginBottom: 6 }}>KEYBOARD SHORTCUTS</p>
            {KEYBOARD_SHORTCUTS.map(({ keys, description }) => (
              <div
                key={keys}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: 8,
                  padding: '4px 0',
                  borderBottom: '1px solid rgba(255,255,255,0.05)',
                }}
              >
                <span style={{ color: 'rgba(255,255,255,0.5)', fontSize: 11 }}>{description}</span>
                <kbd
                  style={{
                    fontSize: 10,
                    fontFamily: 'monospace',
                    padding: '2px 5px',
                    borderRadius: 4,
                    background: 'rgba(255,255,255,0.08)',
                    border: '1px solid rgba(255,255,255,0.14)',
                    color: 'rgba(255,255,255,0.7)',
                    whiteSpace: 'nowrap',
                    flexShrink: 0,
                  }}
                >
                  {keys}
                </kbd>
              </div>
            ))}
          </div>
        )}

      </div>
    </div>
  )
}

// ── Small helpers ─────────────────────────────────────────────────────────────

function Row({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
      <span style={{ color: 'rgba(255,255,255,0.4)', fontSize: 11, flexShrink: 0 }}>{label}</span>
      <span style={{
        color: 'rgba(255,255,255,0.75)',
        fontSize: 11,
        fontFamily: mono ? 'monospace' : undefined,
        textAlign: 'right',
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        maxWidth: 160,
      }}>
        {value}
      </span>
    </div>
  )
}

function Geo({ label, value }: { label: string; value: number }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
      <span style={{ color: 'rgba(255,255,255,0.3)', fontSize: 10, width: 8 }}>{label}</span>
      <span style={{ color: 'rgba(255,255,255,0.65)', fontSize: 10, fontFamily: 'monospace' }}>
        {Math.round(value)}
      </span>
    </div>
  )
}

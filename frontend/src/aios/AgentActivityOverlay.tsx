import { useAgents } from '@/hooks/useAgents'
import { useAgentStatusStream } from '@/hooks/useAgentStatusStream'
import { useDeviceTier } from '@/aios/hooks/useDeviceTier'
import type { AgentState } from '@/api/types'

const STATE_COLOR: Record<AgentState, string> = {
  working: '#22c55e',
  idle: '#94a3b8',
  error: '#ef4444',
  offline: '#475569',
}

/**
 * Ambient rail showing live agent activity, visible across all AIOS panels.
 * Low device tiers render static dots only — no animation.
 */
export default function AgentActivityOverlay() {
  const { agents } = useAgents()
  const { statuses } = useAgentStatusStream()
  const tier = useDeviceTier()

  const enabled = agents.filter(a => a.enabled)
  if (enabled.length === 0) return null

  const lowTier = tier === 'phone' || tier === 'tv' || tier === 'kiosk'

  return (
    <div style={{
      position: 'fixed',
      top: 'calc(var(--aios-status-h, 28px) + 8px)',
      right: 12,
      zIndex: 40,
      display: 'flex',
      flexDirection: 'column',
      gap: 4,
      pointerEvents: 'none',
    }}>
      {enabled.map(a => {
        const live = statuses[a.id]
        const state = live?.state ?? a.state
        const activity = live?.activity ?? a.activity
        const working = state === 'working'
        return (
          <div key={a.id} title={`${a.name}: ${activity ?? state}`} style={{
            display: 'flex',
            alignItems: 'center',
            gap: 6,
            padding: '3px 8px',
            borderRadius: 999,
            background: 'rgba(10, 8, 20, 0.85)',
            backdropFilter: 'var(--vt-blur)',
            WebkitBackdropFilter: 'var(--vt-blur)',
            border: '1px solid rgba(139,92,246,0.12)',
            fontSize: 10,
            color: 'rgba(255,255,255,0.6)',
            maxWidth: 220,
            pointerEvents: 'auto',
          }}>
            <span style={{
              width: 6,
              height: 6,
              borderRadius: '50%',
              background: a.color || STATE_COLOR[state],
              flexShrink: 0,
              animation: !lowTier && working ? 'vt-agent-pulse 1.4s ease-in-out infinite' : undefined,
            }} />
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {a.name}{activity ? ` — ${activity}` : ''}
            </span>
          </div>
        )
      })}
      <style>{`
        @keyframes vt-agent-pulse {
          0%, 100% { opacity: 1; transform: scale(1); }
          50% { opacity: 0.4; transform: scale(1.4); }
        }
      `}</style>
    </div>
  )
}

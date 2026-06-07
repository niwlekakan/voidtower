import { Zap, Eye, CircleDot } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'

// ── Types ─────────────────────────────────────────────────────────────────────

export type AiLevel = 'native' | 'aware' | 'ready' | 'none'

export interface AiBadgeConfig {
  color: string
  background: string
  border: string
  label: string
  Icon: LucideIcon
}

// ── Badge style/label config ──────────────────────────────────────────────────

export function getAiBadgeConfig(level: AiLevel): AiBadgeConfig | null {
  switch (level) {
    case 'native':
      return {
        color:      '#06b6d4',
        background: '#06b6d418',
        border:     '#06b6d444',
        label:      'AI Native',
        Icon:       Zap,
      }
    case 'aware':
      return {
        color:      '#818cf8',
        background: '#818cf818',
        border:     '#818cf844',
        label:      'AI Aware',
        Icon:       Eye,
      }
    case 'ready':
      return {
        color:      'rgba(255,255,255,0.45)',
        background: 'rgba(255,255,255,0.06)',
        border:     'rgba(255,255,255,0.16)',
        label:      'AI Ready',
        Icon:       CircleDot,
      }
    case 'none':
      return null
  }
}

// ── Component ─────────────────────────────────────────────────────────────────

export interface AiBadgeProps {
  level: AiLevel
  description?: string
  /** When true, renders a more compact badge without the label text */
  compact?: boolean
}

export default function AiBadge({ level, description, compact = false }: AiBadgeProps) {
  const config = getAiBadgeConfig(level)
  if (!config) return null

  const { color, background, border, label, Icon } = config

  return (
    <span
      title={description ?? label}
      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-xs font-medium cursor-help"
      style={{
        background,
        color,
        border: `1px solid ${border}`,
        flexShrink: 0,
      }}
    >
      <Icon size={10} />
      {!compact && label}
    </span>
  )
}

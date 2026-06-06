import { BrainCircuit } from 'lucide-react'
import { notify } from '@/store/notifications'

// Module-level cache: avoid re-fetching on every click
let _cachedUrl: string | null | undefined = undefined  // undefined = not yet fetched

async function getOdysseusUrl(): Promise<string | null> {
  if (_cachedUrl !== undefined) return _cachedUrl
  try {
    const r = await fetch('/api/integrations/odysseus/config', { credentials: 'include' })
    if (!r.ok) { _cachedUrl = null; return null }
    const cfg = await r.json() as { allowed_url?: string; enabled?: boolean }
    _cachedUrl = cfg.allowed_url?.trim() || null
  } catch {
    _cachedUrl = null
  }
  return _cachedUrl
}

interface Props {
  context: string
  label?: string
  size?: number
}

export default function SendToOdysseus({ context, label, size = 13 }: Props) {
  const handleClick = async (e: React.MouseEvent) => {
    e.stopPropagation()
    const url = await getOdysseusUrl()
    if (!url) {
      notify.warning('Odysseus not configured — set it up in Settings → Integrations')
      return
    }
    // Odysseus doesn't expose a ?prompt= URL param, so copy to clipboard and open
    try {
      await navigator.clipboard.writeText(context)
      notify.success('Context copied — paste it into Odysseus')
    } catch {
      // clipboard unavailable — still open Odysseus
    }
    window.open(url, '_blank', 'noopener,noreferrer')
  }

  return (
    <button
      onClick={handleClick}
      title={label ?? 'Send to Odysseus'}
      className="flex items-center gap-1 px-1.5 py-1 rounded text-xs transition-colors hover:opacity-80"
      style={{
        background: 'var(--accent-primary-subtle, rgba(139,92,246,0.12))',
        color: 'var(--accent-primary, #7c3aed)',
        border: '1px solid var(--accent-primary, #7c3aed)44',
        lineHeight: 1,
        whiteSpace: 'nowrap',
      }}
    >
      <BrainCircuit size={size} />
      {label && <span>{label}</span>}
    </button>
  )
}

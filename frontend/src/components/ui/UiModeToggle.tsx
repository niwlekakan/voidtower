import { useEffect } from 'react'
import { LayoutDashboard, Hexagon } from 'lucide-react'
import { useThemeStore } from '@/store/theme'

interface Props {
  compact?: boolean // icon-only (for Void status bar where space is tight)
}

export default function UiModeToggle({ compact = false }: Props) {
  const { uiMode, setUiMode } = useThemeStore()
  const inVoid = uiMode === 'void'

  // Ctrl+Shift+V toggles anywhere in the app
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'V') {
        e.preventDefault()
        useThemeStore.getState().setUiMode(
          useThemeStore.getState().uiMode === 'void' ? 'tower' : 'void'
        )
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])

  const label = inVoid ? 'Tower Mode' : 'Void Mode'
  const Icon = inVoid ? LayoutDashboard : Hexagon

  return (
    <button
      onClick={() => setUiMode(inVoid ? 'tower' : 'void')}
      title={`Switch to ${label} (Ctrl+Shift+V)`}
      style={{
        display: 'flex', alignItems: 'center', gap: compact ? 0 : 6,
        padding: compact ? '3px 7px' : '4px 10px',
        borderRadius: 20,
        border: '1px solid var(--border-subtle)',
        background: inVoid ? 'var(--bg-elevated)' : 'var(--accent-primary-subtle)',
        color: inVoid ? 'var(--text-muted)' : 'var(--accent-primary)',
        cursor: 'pointer',
        fontSize: 11,
        fontWeight: 600,
        transition: 'all 0.2s',
        whiteSpace: 'nowrap',
        boxShadow: inVoid ? 'none' : '0 0 8px color-mix(in srgb, var(--accent-primary) 30%, transparent)',
      }}
    >
      <Icon size={12} style={{ flexShrink: 0 }} />
      {!compact && <span style={{ marginLeft: 5 }}>{label}</span>}
    </button>
  )
}

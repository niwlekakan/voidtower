import { useEffect } from 'react'
import { Hexagon } from 'lucide-react'
import { useThemeStore } from '@/store/theme'

interface Props {
  compact?: boolean // icon-only (for Void status bar where space is tight)
}

export default function UiModeToggle({ compact = false }: Props) {
  const { uiMode, setUiMode } = useThemeStore()
  const inVoid = uiMode === 'void'

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

  if (compact) {
    return (
      <button
        onClick={() => setUiMode(inVoid ? 'tower' : 'void')}
        title={`Void Mode ${inVoid ? 'on' : 'off'} (Ctrl+Shift+V)`}
        style={{
          display: 'flex', alignItems: 'center',
          padding: '3px 7px', borderRadius: 20,
          border: '1px solid var(--border-subtle)',
          background: inVoid ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
          color: inVoid ? 'var(--accent-primary)' : 'var(--text-muted)',
          cursor: 'pointer', transition: 'all 0.2s',
        }}
      >
        <Hexagon size={12} />
      </button>
    )
  }

  return (
    <button
      onClick={() => setUiMode(inVoid ? 'tower' : 'void')}
      title="Void Mode — experimental (Ctrl+Shift+V)"
      style={{
        display: 'flex', alignItems: 'center', gap: 7,
        padding: '4px 10px 4px 8px', borderRadius: 20,
        border: `1px solid ${inVoid ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
        background: inVoid ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
        color: inVoid ? 'var(--accent-primary)' : 'var(--text-muted)',
        cursor: 'pointer', fontSize: 11, fontWeight: 600,
        transition: 'all 0.2s', whiteSpace: 'nowrap',
        boxShadow: inVoid ? '0 0 8px color-mix(in srgb, var(--accent-primary) 30%, transparent)' : 'none',
      }}
    >
      <Hexagon size={12} style={{ flexShrink: 0 }} />
      <span>Void</span>
      <span style={{
        position: 'relative', width: 28, height: 16, borderRadius: 8, flexShrink: 0,
        background: inVoid ? 'var(--accent-primary)' : 'var(--bg-root)',
        border: `1px solid ${inVoid ? 'var(--accent-primary)' : 'var(--border-default)'}`,
        transition: 'background 0.2s, border-color 0.2s',
      }}>
        <span style={{
          position: 'absolute', top: 2, left: inVoid ? 14 : 2,
          width: 10, height: 10, borderRadius: '50%',
          background: inVoid ? '#fff' : 'var(--text-muted)',
          transition: 'left 0.2s',
        }} />
      </span>
    </button>
  )
}

import { Search } from 'lucide-react'

export interface NativePanelTab { id: string; label: string }

interface Props {
  children: React.ReactNode
  search?: string
  onSearch?: (v: string) => void
  searchPlaceholder?: string
  tabs?: NativePanelTab[]
  activeTab?: string
  onTabChange?: (id: string) => void
  actions?: React.ReactNode
}

export default function NativePanelShell({
  children, search, onSearch, searchPlaceholder = 'Search…',
  tabs, activeTab, onTabChange, actions,
}: Props) {
  const hasHeader = onSearch !== undefined || (tabs && tabs.length > 0)
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', fontSize: 12, color: 'var(--text-primary)' }}>
      {hasHeader && (
        <div style={{ flexShrink: 0, borderBottom: '1px solid var(--border-subtle)', background: 'var(--bg-panel)' }}>
          {tabs && tabs.length > 0 && (
            <div style={{ display: 'flex', gap: 2, padding: '6px 8px 0' }}>
              {tabs.map(t => (
                <button key={t.id} onClick={() => onTabChange?.(t.id)} style={{
                  padding: '4px 10px', fontSize: 11, fontWeight: 500, borderRadius: '4px 4px 0 0',
                  border: 'none', cursor: 'pointer', background: 'none',
                  color: activeTab === t.id ? 'var(--accent-primary)' : 'var(--text-muted)',
                  borderBottom: activeTab === t.id ? '2px solid var(--accent-primary)' : '2px solid transparent',
                  transition: 'color 0.12s',
                }}>{t.label}</button>
              ))}
            </div>
          )}
          {onSearch && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 8px' }}>
              <Search size={12} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
              <input
                value={search ?? ''}
                onChange={e => onSearch(e.target.value)}
                placeholder={searchPlaceholder}
                style={{
                  flex: 1, background: 'none', border: 'none', outline: 'none',
                  fontSize: 12, color: 'var(--text-primary)',
                }}
              />
            </div>
          )}
        </div>
      )}
      <div style={{ flex: 1, overflowY: 'auto', padding: '4px 0' }}>
        {children}
      </div>
      {actions && (
        <div style={{
          flexShrink: 0, borderTop: '1px solid var(--border-subtle)',
          padding: '6px 8px', display: 'flex', justifyContent: 'flex-end', gap: 6,
          background: 'var(--bg-panel)',
        }}>
          {actions}
        </div>
      )}
    </div>
  )
}

export function NativeRow({ children, style, onClick }: { children: React.ReactNode; style?: React.CSSProperties; onClick?: () => void }) {
  return (
    <div onClick={onClick} style={{
      display: 'flex', alignItems: 'center', gap: 6, padding: '5px 10px',
      borderBottom: '1px solid var(--border-subtle)', ...style,
    }}>{children}</div>
  )
}

export function StatusDot({ color }: { color: string }) {
  return <span style={{ width: 6, height: 6, borderRadius: '50%', background: color, flexShrink: 0, display: 'inline-block' }} />
}

export function IconBtn({ title, onClick, children, danger, disabled }: {
  title: string; onClick: () => void; children: React.ReactNode; danger?: boolean; disabled?: boolean
}) {
  return (
    <button title={title} onClick={onClick} disabled={disabled} style={{
      background: 'none', border: 'none', cursor: disabled ? 'default' : 'pointer', padding: 3, borderRadius: 4,
      color: danger ? 'var(--accent-error, #ef4444)' : 'var(--text-muted)',
      opacity: disabled ? 0.3 : 1,
      display: 'flex', alignItems: 'center',
    }}>{children}</button>
  )
}

export function EmptyState({ text }: { text: string }) {
  return <div style={{ textAlign: 'center', padding: '24px 8px', color: 'var(--text-muted)', fontSize: 12 }}>{text}</div>
}

export function LoadingState() {
  return <div style={{ textAlign: 'center', padding: '24px 8px', color: 'var(--text-muted)', fontSize: 12 }}>Loading…</div>
}

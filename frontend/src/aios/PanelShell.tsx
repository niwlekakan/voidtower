/**
 * Wraps a page component for display inside a Void Mode floating panel.
 * Provides a slim breadcrumb header and hides any full-page header the page
 * normally renders (via the `.panel-shell-active` CSS class).
 */
export function PanelShell({
  title,
  icon,
  children,
}: {
  title: string
  icon?: React.ReactNode
  children: React.ReactNode
}) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* 32px slim header — AI OS aesthetic: deep dark, purple-tinted */}
      <div style={{
        height: 32,
        minHeight: 32,
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        paddingInline: 12,
        background: 'rgba(10, 8, 20, 0.95)',
        backdropFilter: 'var(--vt-blur)',
        WebkitBackdropFilter: 'var(--vt-blur)',
        borderBottom: '1px solid rgba(139,92,246,0.12)',
        flexShrink: 0,
        overflow: 'hidden',
      }}>
        {icon && (
          <span style={{ display: 'flex', alignItems: 'center', flexShrink: 0, opacity: 0.55, color: 'var(--accent-primary, #8b5cf6)' }}>
            {icon}
          </span>
        )}
        <span style={{
          fontSize: 11,
          fontWeight: 500,
          color: 'rgba(255,255,255,0.45)',
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          letterSpacing: '0.04em',
          textTransform: 'uppercase',
        }}>
          {title}
        </span>
      </div>

      {/* Scrollable content area */}
      <div
        className="panel-shell-active panel-content-root"
        style={{ flex: 1, overflowY: 'auto', height: 'calc(100% - 32px)' }}
      >
        {children}
      </div>
    </div>
  )
}

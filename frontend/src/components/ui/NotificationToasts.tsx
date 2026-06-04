import { X, CheckCircle, AlertTriangle, XCircle, Info } from 'lucide-react'
import { useNotificationStore, type NotifLevel } from '@/store/notifications'

const icons: Record<NotifLevel, React.ReactNode> = {
  info:    <Info size={14} />,
  success: <CheckCircle size={14} />,
  warning: <AlertTriangle size={14} />,
  error:   <XCircle size={14} />,
}

const colors: Record<NotifLevel, string> = {
  info:    'var(--accent-secondary)',
  success: 'var(--accent-success)',
  warning: 'var(--accent-warning)',
  error:   'var(--accent-danger)',
}

export default function NotificationToasts() {
  const { notifications, remove } = useNotificationStore()
  if (!notifications.length) return null

  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 w-72">
      {notifications.map((n) => (
        <div
          key={n.id}
          className="flex items-start gap-2 p-3 rounded shadow-lg text-sm"
          style={{
            background: 'var(--bg-elevated)',
            border: '1px solid var(--border-default)',
            color: 'var(--text-primary)',
          }}
        >
          <span style={{ color: colors[n.level], flexShrink: 0, marginTop: 1 }}>
            {icons[n.level]}
          </span>
          <div className="flex-1 min-w-0">
            <div className="font-medium truncate">{n.title}</div>
            {n.message && <div className="text-xs mt-0.5" style={{ color: 'var(--text-secondary)' }}>{n.message}</div>}
          </div>
          <button onClick={() => remove(n.id)} style={{ color: 'var(--text-muted)', flexShrink: 0 }}>
            <X size={12} />
          </button>
        </div>
      ))}
    </div>
  )
}

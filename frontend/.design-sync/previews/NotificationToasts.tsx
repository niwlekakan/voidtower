import { NotificationToasts, useNotificationStore } from 'voidtower-frontend'
import { trapFixedAt } from './_trapFixed'

// NotificationToasts uses `fixed bottom-4 right-4` internally — see _trapFixed.ts.
trapFixedAt(360, 220)

// Single export only: notifications live in global Zustand state.
// duration: 0 disables auto-dismiss so the toasts survive the screenshot.
useNotificationStore.setState({
  notifications: [
    { id: 'n1', level: 'success', title: 'Backup completed', message: 'nightly-postgres finished in 42s', duration: 0 },
    { id: 'n2', level: 'warning', title: 'Disk usage above 85%', duration: 0 },
    { id: 'n3', level: 'error', title: 'Proxmox host unreachable', message: 'pve-01: connection refused', duration: 0 },
  ],
})

export function Stack() {
  return (
    <div style={{ position: 'relative', width: 360, height: 220 }}>
      <NotificationToasts />
    </div>
  )
}

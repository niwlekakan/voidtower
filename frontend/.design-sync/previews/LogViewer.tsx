import { LogViewer } from 'voidtower-frontend'

const SAMPLE_LINES = [
  '2026-06-25T09:14:02Z INFO  starting voidtower backend on 0.0.0.0:8743',
  '2026-06-25T09:14:02Z INFO  sqlite pool ready (16 connections)',
  '2026-06-25T09:14:03Z INFO  proxmox VM monitor: polling every 90s',
  '2026-06-25T09:14:05Z WARN  policy rule "deny-prod-delete" matched, action blocked',
  '2026-06-25T09:14:11Z INFO  backup job "nightly-postgres" completed in 42s',
  '2026-06-25T09:14:18Z ERROR failed to reach proxmox host pve-01: connection refused',
]

export function Default() {
  return (
    <div style={{ width: 480 }}>
      <LogViewer lines={SAMPLE_LINES} maxHeight={220} />
    </div>
  )
}

export function Empty() {
  return (
    <div style={{ width: 480 }}>
      <LogViewer lines={[]} maxHeight={120} />
    </div>
  )
}

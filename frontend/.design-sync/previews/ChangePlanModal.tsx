import { ChangePlanModal } from 'voidtower-frontend'
import { trapFixedAt } from './_trapFixed'

// ChangePlanModal uses `fixed inset-0` internally — see _trapFixed.ts.
trapFixedAt(560, 680)

export function LowRisk() {
  return (
    <ChangePlanModal
      plan={{
        title: 'Update Proxy',
        risk: 'low',
        changes: [
          { label: 'Domain', value: 'app.example.com' },
          { label: 'Upstream', value: 'http://192.168.1.42:8080' },
        ],
        preview: null,
      }}
      onConfirm={() => {}}
      onCancel={() => {}}
    />
  )
}

export function HighRisk() {
  return (
    <ChangePlanModal
      plan={{
        title: 'Delete Backup',
        risk: 'high',
        changes: [
          { label: 'Name', value: 'nightly-postgres' },
          { label: 'Effect', value: 'Permanent deletion' },
          { label: 'Repo', value: 's3:backup-bucket/nightly-postgres' },
        ],
        preview: 'restic forget --keep-daily 0 --prune\nrestic check',
      }}
      onConfirm={() => {}}
      onCancel={() => {}}
      confirming
    />
  )
}

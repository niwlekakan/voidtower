import { ConfirmDialog } from 'voidtower-frontend'
import { trapFixedAt } from './_trapFixed'

// ConfirmDialog uses `fixed inset-0` internally — see _trapFixed.ts.
trapFixedAt(460, 340)

export function Default() {
  return (
    <ConfirmDialog
      title="Restart service?"
      message="This will briefly interrupt traffic to all dependent containers."
      onConfirm={() => {}}
      onCancel={() => {}}
    />
  )
}

export function Danger() {
  return (
    <ConfirmDialog
      title="Delete container"
      message="This permanently removes the container and its volumes. This cannot be undone."
      confirmLabel="Delete"
      danger
      onConfirm={() => {}}
      onCancel={() => {}}
    />
  )
}

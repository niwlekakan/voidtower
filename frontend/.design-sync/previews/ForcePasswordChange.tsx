import { ForcePasswordChange } from 'voidtower-frontend'
import { trapFixedAt } from './_trapFixed'

// ForcePasswordChange uses `fixed inset-0` internally — see _trapFixed.ts.
trapFixedAt(420, 640)

export function Default() {
  return <ForcePasswordChange />
}

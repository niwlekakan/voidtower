import { Plus } from 'lucide-react'
import { Button } from 'voidtower-frontend'

export function Variants() {
  return (
    <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
      <Button variant="primary">Save &amp; continue</Button>
      <Button variant="secondary">Cancel</Button>
      <Button variant="danger">Unload from GPU</Button>
      <Button variant="ghost">Close</Button>
    </div>
  )
}

export function Sizes() {
  return (
    <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
      <Button size="sm">Refresh</Button>
      <Button size="md">Create</Button>
      <Button size="lg">Generate &amp; add</Button>
    </div>
  )
}

export function States() {
  return (
    <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
      <Button variant="primary" loading>Saving</Button>
      <Button variant="secondary" disabled>Disabled</Button>
    </div>
  )
}

export function WithIcon() {
  return (
    <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
      <Button size="sm" variant="primary">
        <Plus size={13} /> Add peer
      </Button>
    </div>
  )
}

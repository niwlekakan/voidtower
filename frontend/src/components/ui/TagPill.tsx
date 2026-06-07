import { useEffect, useRef } from 'react'
import { api } from '@/api/client'
import { notify } from '@/store/notifications'
import type { Tag } from '@/api/types'

interface TagPillProps {
  tag: Tag
  onRemove?: () => void
}

export function TagPill({ tag, onRemove }: TagPillProps) {
  return (
    <span style={{
      display: 'inline-flex', alignItems: 'center', gap: 4,
      padding: '1px 7px', borderRadius: 12, fontSize: 11, fontWeight: 600,
      background: tag.color + '22', color: tag.color,
      border: `1px solid ${tag.color}55`, whiteSpace: 'nowrap',
    }}>
      {tag.name}
      {onRemove && (
        <button
          onClick={e => { e.stopPropagation(); onRemove() }}
          style={{ background: 'none', border: 'none', cursor: 'pointer', color: tag.color, padding: 0, lineHeight: 1, fontSize: 13, opacity: 0.7 }}
        >×</button>
      )}
    </span>
  )
}

interface TagSelectorProps {
  allTags: Tag[]
  assigned: Tag[]
  onAssign: (tag: Tag) => void
  onUnassign: (tag: Tag) => void
}

interface TagPopoverProps {
  resourceType: string
  resourceId: string
  allTags: Tag[]
  assigned: Tag[]
  onClose: () => void
}

export function TagPopover({ resourceType, resourceId, allTags, assigned, onClose }: TagPopoverProps) {
  const ref = useRef<HTMLDivElement>(null)
  const assignedIds = new Set(assigned.map(t => t.id))

  useEffect(() => {
    const handler = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) onClose() }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [onClose])

  const toggle = async (tag: Tag) => {
    try {
      if (assignedIds.has(tag.id)) await api.tags.unassign(tag.id, resourceType, resourceId)
      else await api.tags.assign(tag.id, resourceType, resourceId)
    } catch { notify.error('Failed to update tag') }
    onClose()
  }

  if (allTags.length === 0) return (
    <div ref={ref} style={{ position: 'absolute', zIndex: 50, top: '100%', left: 0, background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: 10, minWidth: 160, boxShadow: '0 4px 16px rgba(0,0,0,0.3)' }}>
      <p style={{ fontSize: 12, color: 'var(--text-muted)' }}>No tags yet. Create some on the Tags page.</p>
    </div>
  )

  return (
    <div ref={ref} style={{ position: 'absolute', zIndex: 50, top: '100%', left: 0, background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: 8, minWidth: 160, boxShadow: '0 4px 16px rgba(0,0,0,0.3)', display: 'flex', flexDirection: 'column', gap: 2 }}>
      {allTags.map(tag => (
        <button key={tag.id} onClick={() => toggle(tag)} style={{
          display: 'flex', alignItems: 'center', gap: 8, padding: '4px 8px', borderRadius: 5,
          background: assignedIds.has(tag.id) ? 'var(--accent-primary-subtle)' : 'transparent',
          border: 'none', cursor: 'pointer', width: '100%', textAlign: 'left',
        }}>
          <span style={{ width: 10, height: 10, borderRadius: '50%', background: tag.color, flexShrink: 0 }} />
          <span style={{ fontSize: 12, color: 'var(--text-primary)' }}>{tag.name}</span>
          {assignedIds.has(tag.id) && <span style={{ marginLeft: 'auto', fontSize: 11, color: 'var(--accent-primary)' }}>✓</span>}
        </button>
      ))}
    </div>
  )
}

export function TagSelector({ allTags, assigned, onAssign, onUnassign }: TagSelectorProps) {
  const assignedIds = new Set(assigned.map(t => t.id))
  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
      {assigned.map(t => <TagPill key={t.id} tag={t} onRemove={() => onUnassign(t)} />)}
      {allTags.filter(t => !assignedIds.has(t.id)).map(t => (
        <button
          key={t.id}
          onClick={() => onAssign(t)}
          style={{
            display: 'inline-flex', alignItems: 'center', padding: '1px 7px',
            borderRadius: 12, fontSize: 11, fontWeight: 600, cursor: 'pointer',
            background: 'transparent', border: `1px dashed ${t.color}88`, color: t.color + '99',
            whiteSpace: 'nowrap',
          }}
        >+ {t.name}</button>
      ))}
    </div>
  )
}

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

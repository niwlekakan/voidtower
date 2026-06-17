import { useState, useEffect, useRef } from 'react'
import { Navigation, GripVertical, Pencil, Eye, EyeOff, RotateCcw, Trash2 } from 'lucide-react'
import { useNavConfigStore, DEFAULT_NAV_ITEMS, DEFAULT_NAV_GROUPS, resolvedNavItems, resolvedNavGroups, type NavItem, type StoredNavGroup } from '@/store/navConfig'
import { useSidebarPrefsStore, SIDEBAR_ANIMATION_OPTIONS, SIDEBAR_PLACEMENT_OPTIONS } from '@/store/sidebarPrefs'
import { ICON_MAP } from '@/aios/AiosDock'
import { ICON_REGISTRY, ICON_NAMES } from '@/components/ui/iconRegistry'
import { useAuthStore } from '@/store/auth'

function InstanceDefaultControl() {
  const currentUser = useAuthStore((s) => s.user)
  const { items, navGroups } = useNavConfigStore()
  const [status, setStatus] = useState<string | null>(null)
  if (currentUser?.role !== 'owner') return null

  const flash = (msg: string) => { setStatus(msg); setTimeout(() => setStatus(null), 2500) }

  const setDefault = async () => {
    try {
      const res = await fetch('/api/nav-config/default', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ items: resolvedNavItems(items), nav_groups: resolvedNavGroups(navGroups) }),
      })
      flash(res.ok ? 'Saved as instance default' : 'Failed to save')
    } catch {
      flash('Failed to save')
    }
  }

  const clearDefault = async () => {
    try {
      const res = await fetch('/api/nav-config/default', { method: 'DELETE', credentials: 'include' })
      flash(res.ok ? 'Instance default cleared' : 'Failed to clear')
    } catch {
      flash('Failed to clear')
    }
  }

  return (
    <div className="space-y-1.5">
      <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>Instance default (owner only)</div>
      <p className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
        New users without a saved layout of their own fall back to this configuration.
      </p>
      <div className="flex items-center gap-2">
        <button onClick={setDefault} className="px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
          Set current layout as default
        </button>
        <button onClick={clearDefault} className="px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
          style={{ color: 'var(--text-muted)' }}>
          Clear
        </button>
        {status && <span className="text-[11px]" style={{ color: 'var(--accent-success)' }}>{status}</span>}
      </div>
    </div>
  )
}

function SidebarPlacementPicker() {
  const placement = useSidebarPrefsStore((s) => s.placement)
  const setPlacement = useSidebarPrefsStore((s) => s.setPlacement)

  return (
    <div className="space-y-1.5">
      <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>Placement</div>
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-1.5">
        {SIDEBAR_PLACEMENT_OPTIONS.map(opt => (
          <button
            key={opt.value}
            onClick={() => setPlacement(opt.value)}
            className="text-left px-2 py-1.5 rounded text-xs transition-colors"
            style={{
              background: placement === opt.value ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
              border: `1px solid ${placement === opt.value ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
              color: placement === opt.value ? 'var(--accent-primary)' : 'var(--text-secondary)',
            }}
          >
            <div className="font-medium">{opt.label}</div>
            <div className="mt-0.5 text-[10px] leading-tight" style={{ color: 'var(--text-muted)' }}>{opt.description}</div>
          </button>
        ))}
      </div>
    </div>
  )
}

function SidebarAutoHideToggle() {
  const autoHide = useSidebarPrefsStore((s) => s.autoHide)
  const setAutoHide = useSidebarPrefsStore((s) => s.setAutoHide)

  return (
    <label className="flex items-center justify-between gap-3 px-2 py-1.5 rounded cursor-pointer"
      style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
      <div>
        <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>Auto-hide on scroll</div>
        <div className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
          Hides while scrolling down, reappears on scroll up.
        </div>
      </div>
      <input type="checkbox" checked={autoHide} onChange={e => setAutoHide(e.target.checked)} />
    </label>
  )
}

function SidebarAnimationPicker() {
  const animation = useSidebarPrefsStore((s) => s.animation)
  const setAnimation = useSidebarPrefsStore((s) => s.setAnimation)

  return (
    <div className="space-y-1.5">
      <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>Collapse animation</div>
      <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5">
        {SIDEBAR_ANIMATION_OPTIONS.map(opt => (
          <button
            key={opt.value}
            onClick={() => setAnimation(opt.value)}
            className="text-left px-2 py-1.5 rounded text-xs transition-colors"
            style={{
              background: animation === opt.value ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
              border: `1px solid ${animation === opt.value ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
              color: animation === opt.value ? 'var(--accent-primary)' : 'var(--text-secondary)',
            }}
          >
            <div className="font-medium">{opt.label}</div>
            <div className="mt-0.5 text-[10px] leading-tight" style={{ color: 'var(--text-muted)' }}>{opt.description}</div>
          </button>
        ))}
      </div>
    </div>
  )
}

export default function NavigationTab() {
  const { items, setItems, resetItems, navGroups, setNavGroups, resetNavGroups } = useNavConfigStore()
  const [list, setList] = useState<NavItem[]>(() => resolvedNavItems(items))
  const [groups, setGroups] = useState<StoredNavGroup[]>(() => resolvedNavGroups(navGroups))
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editingGroupId, setEditingGroupId] = useState<string | null>(null)
  const [iconPickerId, setIconPickerId] = useState<string | null>(null)
  const dragItemRef = useRef<{ groupId: string; idx: number; itemId: string } | null>(null)
  const dragGroupRef = useRef<number | null>(null)

  useEffect(() => { setList(resolvedNavItems(items)) }, [items])
  useEffect(() => { setGroups(resolvedNavGroups(navGroups)) }, [navGroups])

  const navMap = Object.fromEntries(list.map(it => [it.id, it]))

  const saveGroups = (next: StoredNavGroup[]) => { setGroups(next); setNavGroups(next) }

  const toggleVisible = (id: string) => {
    const next = list.map(it => it.id === id ? { ...it, visible: !it.visible } : it)
    setList(next); setItems(next)
  }

  const updateLabel = (id: string, label: string) => {
    const next = list.map(it => it.id === id ? { ...it, label } : it)
    setList(next); setItems(next); setEditingId(null)
  }

  const updateIcon = (id: string, icon: string | undefined) => {
    const next = list.map(it => it.id === id ? { ...it, icon } : it)
    setList(next); setItems(next); setIconPickerId(null)
  }

  const updateGroupLabel = (id: string, label: string) => {
    saveGroups(groups.map(g => g.id === id ? { ...g, label } : g))
    setEditingGroupId(null)
  }

  // Drag handlers for items — supports both reordering within a group and moving across groups
  const onItemDragStart = (groupId: string, idx: number, itemId: string) => {
    dragItemRef.current = { groupId, idx, itemId }
  }
  const onItemDragOver = (e: React.DragEvent, groupId: string, idx: number) => {
    e.preventDefault()
    const src = dragItemRef.current
    if (!src) return
    if (src.groupId === groupId && src.idx === idx) return
    const next = groups.map(g => {
      if (g.id === src.groupId) {
        const ids = g.itemIds.filter(id => id !== src.itemId)
        if (g.id === groupId) {
          const insertAt = idx > src.idx ? idx - 1 : idx
          ids.splice(insertAt, 0, src.itemId)
        }
        return { ...g, itemIds: ids }
      }
      if (g.id === groupId) {
        const ids = [...g.itemIds]
        ids.splice(idx, 0, src.itemId)
        return { ...g, itemIds: ids }
      }
      return g
    })
    dragItemRef.current = { groupId, idx, itemId: src.itemId }
    saveGroups(next)
  }

  // Drag handlers for groups
  const onGroupDragStart = (idx: number) => { dragGroupRef.current = idx }
  const onGroupDragOver = (e: React.DragEvent, idx: number) => {
    e.preventDefault()
    const src = dragGroupRef.current
    if (src === null || src === idx) return
    const next = [...groups]
    const [moved] = next.splice(src, 1)
    next.splice(idx, 0, moved)
    dragGroupRef.current = idx
    saveGroups(next)
  }

  const addGroup = () => {
    const id = `group-${Date.now().toString(36)}`
    saveGroups([...groups, { id, label: 'New Group', itemIds: [] }])
  }

  const deleteGroup = (id: string) => {
    const target = groups.find(g => g.id === id)
    if (!target) return
    const others = groups.filter(g => g.id !== id)
    if (others.length === 0) return
    if (target.itemIds.length > 0 && !window.confirm(`Delete "${target.label}"? Its items will move into "${others[0].label}".`)) return
    const next = others.map((g, i) => i === 0 ? { ...g, itemIds: [...g.itemIds, ...target.itemIds] } : g)
    saveGroups(next)
  }

  return (
    <div className="card space-y-4">
      <div className="flex items-center gap-2">
        <Navigation size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Navigation</h2>
      </div>
      <div className="space-y-3 pb-4 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
        <div className="text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Layout</div>
        <SidebarPlacementPicker />
        <SidebarAutoHideToggle />
        <SidebarAnimationPicker />
      </div>

      <div className="text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Items &amp; Groups</div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Drag to reorder groups and items. Click a group name to rename it. Toggle visibility, change the icon, or rename individual items.
      </p>

      <InstanceDefaultControl />

      {groups.map((group, gi) => {
        const groupItems = group.itemIds.map(id => navMap[id]).filter((it): it is NavItem => !!it)
        return (
          <div key={group.id}
            draggable
            onDragStart={() => onGroupDragStart(gi)}
            onDragOver={e => onGroupDragOver(e, gi)}
            style={{ cursor: 'grab' }}
          >
            <div className="flex items-center gap-1 mb-1.5">
              <GripVertical size={12} style={{ color: 'var(--text-disabled)', flexShrink: 0 }} />
              {editingGroupId === group.id ? (
                <input autoFocus defaultValue={group.label}
                  onBlur={e => updateGroupLabel(group.id, e.target.value.trim() || group.label)}
                  onKeyDown={e => {
                    if (e.key === 'Enter') updateGroupLabel(group.id, (e.target as HTMLInputElement).value.trim() || group.label)
                    if (e.key === 'Escape') setEditingGroupId(null)
                  }}
                  className="text-xs outline-none px-1 rounded"
                  style={{ background: 'var(--bg-root)', border: '1px solid var(--accent-primary)', color: 'var(--text-primary)', width: 100, cursor: 'text' }}
                  onClick={e => e.stopPropagation()}
                />
              ) : (
                <button
                  className="flex items-center gap-1 text-xs font-medium uppercase tracking-wider"
                  style={{ background: 'none', border: 'none', cursor: 'text', color: 'var(--text-muted)', padding: 0 }}
                  onClick={e => { e.stopPropagation(); setEditingGroupId(group.id) }}
                  title="Click to rename group"
                >
                  {group.label}
                  <Pencil size={10} style={{ opacity: 0.5 }} />
                </button>
              )}
              {groups.length > 1 && (
                <button
                  onClick={e => { e.stopPropagation(); deleteGroup(group.id) }}
                  title="Delete group"
                  style={{ marginLeft: 'auto', background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', display: 'flex' }}
                >
                  <Trash2 size={12} />
                </button>
              )}
            </div>
            <div className="space-y-1">
              {groupItems.map((item, ii) => {
                const DefaultIcon = ICON_MAP[item.id]
                const Icon = (item.icon && ICON_REGISTRY[item.icon]) || DefaultIcon
                const defaultLabel = DEFAULT_NAV_ITEMS.find(d => d.id === item.id)?.label ?? item.id
                return (
                  <div key={item.id}
                    draggable
                    onDragStart={e => { e.stopPropagation(); onItemDragStart(group.id, ii, item.id) }}
                    onDragOver={e => { e.stopPropagation(); onItemDragOver(e, group.id, ii) }}
                    style={{
                      position: 'relative',
                      display: 'flex', alignItems: 'center', gap: 8,
                      padding: '5px 8px', borderRadius: 6,
                      background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)',
                      opacity: item.visible ? 1 : 0.45,
                      cursor: 'grab',
                    }}
                  >
                    <GripVertical size={12} style={{ color: 'var(--text-disabled)', flexShrink: 0 }} />
                    <button
                      onClick={e => { e.stopPropagation(); setIconPickerId(iconPickerId === item.id ? null : item.id) }}
                      title="Change icon"
                      style={{ background: 'none', border: 'none', cursor: 'pointer', flexShrink: 0, display: 'flex', padding: 0 }}
                    >
                      {Icon && <Icon size={13} style={{ color: 'var(--text-secondary)' }} />}
                    </button>
                    {iconPickerId === item.id && (
                      <div
                        onClick={e => e.stopPropagation()}
                        style={{
                          position: 'absolute', top: '100%', left: 0, zIndex: 20,
                          marginTop: 4, padding: 6, borderRadius: 6,
                          background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)',
                          boxShadow: '0 4px 12px rgba(0,0,0,0.25)',
                          display: 'grid', gridTemplateColumns: 'repeat(7, 1fr)', gap: 4,
                          width: 196,
                        }}
                      >
                        {ICON_NAMES.map(name => {
                          const OptIcon = ICON_REGISTRY[name]
                          return (
                            <button key={name} onClick={() => updateIcon(item.id, name)} title={name}
                              style={{
                                display: 'flex', alignItems: 'center', justifyContent: 'center',
                                padding: 4, borderRadius: 4, cursor: 'pointer',
                                background: item.icon === name ? 'var(--accent-primary-subtle)' : 'transparent',
                                border: `1px solid ${item.icon === name ? 'var(--accent-primary)' : 'transparent'}`,
                              }}
                            >
                              <OptIcon size={13} style={{ color: 'var(--text-secondary)' }} />
                            </button>
                          )
                        })}
                        {item.icon && (
                          <button onClick={() => updateIcon(item.id, undefined)} title="Reset to default icon"
                            style={{
                              display: 'flex', alignItems: 'center', justifyContent: 'center',
                              padding: 4, borderRadius: 4, cursor: 'pointer',
                              background: 'transparent', border: '1px solid transparent',
                            }}
                          >
                            <RotateCcw size={13} style={{ color: 'var(--text-muted)' }} />
                          </button>
                        )}
                      </div>
                    )}
                    <div style={{ flex: 1, minWidth: 0 }}>
                      {editingId === item.id ? (
                        <input autoFocus defaultValue={item.label}
                          onBlur={e => updateLabel(item.id, e.target.value.trim() || defaultLabel)}
                          onKeyDown={e => {
                            if (e.key === 'Enter') updateLabel(item.id, (e.target as HTMLInputElement).value.trim() || defaultLabel)
                            if (e.key === 'Escape') setEditingId(null)
                          }}
                          className="w-full text-xs outline-none px-1 rounded"
                          style={{ background: 'var(--bg-root)', border: '1px solid var(--accent-primary)', color: 'var(--text-primary)' }}
                          onClick={e => e.stopPropagation()}
                        />
                      ) : (
                        <span className="text-xs cursor-text" style={{ color: 'var(--text-primary)' }}
                          onClick={e => { e.stopPropagation(); setEditingId(item.id) }} title="Click to rename">
                          {item.label}
                          {item.label !== defaultLabel && <span style={{ color: 'var(--text-muted)', marginLeft: 4 }}>({defaultLabel})</span>}
                        </span>
                      )}
                    </div>
                    <button onClick={e => { e.stopPropagation(); toggleVisible(item.id) }} title={item.visible ? 'Hide' : 'Show'}
                      style={{ color: item.visible ? 'var(--text-secondary)' : 'var(--text-muted)', background: 'none', border: 'none', cursor: 'pointer', flexShrink: 0 }}>
                      {item.visible ? <Eye size={13} /> : <EyeOff size={13} />}
                    </button>
                  </div>
                )
              })}
              <div
                onDragOver={e => { e.stopPropagation(); onItemDragOver(e, group.id, groupItems.length) }}
                style={{
                  minHeight: groupItems.length === 0 ? 28 : 6,
                  borderRadius: 6,
                  border: groupItems.length === 0 ? '1px dashed var(--border-subtle)' : 'none',
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                }}
              >
                {groupItems.length === 0 && (
                  <span className="text-[11px]" style={{ color: 'var(--text-muted)' }}>Drop items here</span>
                )}
              </div>
            </div>
          </div>
        )
      })}

      <button onClick={addGroup}
        className="px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
        style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
        + New group
      </button>

      <button onClick={() => { resetItems(); setList(DEFAULT_NAV_ITEMS); resetNavGroups(); setGroups(DEFAULT_NAV_GROUPS) }}
        className="px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
        style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
        Reset to defaults
      </button>
    </div>
  )
}

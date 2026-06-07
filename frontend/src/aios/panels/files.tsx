import { useEffect, useState } from 'react'
import { Folder, File, ArrowLeft } from 'lucide-react'
import NativePanelShell, { NativeRow, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface FsEntry { name: string; is_dir: boolean; size?: number; modified?: string }
interface Root { path: string; label: string }

function fmt(b?: number) {
  if (!b) return ''
  if (b > 1e9) return `${(b / 1e9).toFixed(1)}G`
  if (b > 1e6) return `${(b / 1e6).toFixed(1)}M`
  if (b > 1e3) return `${(b / 1e3).toFixed(1)}K`
  return `${b}B`
}

export default function NativeFilesPanel() {
  const [roots, setRoots] = useState<Root[]>([])
  const [entries, setEntries] = useState<FsEntry[]>([])
  const [path, setPath] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  async function loadRoots() {
    const r = await fetch('/api/files/roots', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setRoots(d.roots ?? []) }
    setLoading(false)
  }

  async function loadPath(p: string) {
    setLoading(true)
    setPath(p)
    const r = await fetch(`/api/files/list?path=${encodeURIComponent(p)}`, { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setEntries(d.entries ?? []) }
    setLoading(false)
  }

  function goUp() {
    if (!path) return
    const parent = path.split('/').slice(0, -1).join('/') || '/'
    const isRoot = roots.some(r => r.path === path)
    if (isRoot) { setPath(null); setLoading(false) }
    else loadPath(parent)
  }

  useEffect(() => { loadRoots() }, [])

  return (
    <NativePanelShell>
      {path !== null && (
        <NativeRow style={{ background: 'var(--bg-elevated)' }}>
          <IconBtn title="Up" onClick={goUp}><ArrowLeft size={11} /></IconBtn>
          <div style={{ fontSize: 10, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', flex: 1 }}>{path}</div>
        </NativeRow>
      )}
      {loading ? <LoadingState /> : path === null ? (
        roots.length === 0 ? <EmptyState text="No roots" /> :
        roots.map(r => (
          <NativeRow key={r.path} style={{ cursor: 'pointer' }} onClick={() => loadPath(r.path)}>
            <Folder size={13} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{r.label}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{r.path}</div>
            </div>
          </NativeRow>
        ))
      ) : (
        entries.length === 0 ? <EmptyState text="Empty directory" /> :
        entries.map(e => (
          <NativeRow key={e.name} style={{ cursor: e.is_dir ? 'pointer' : 'default' }} onClick={() => e.is_dir && loadPath(`${path}/${e.name}`)}>
            {e.is_dir
              ? <Folder size={13} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
              : <File size={13} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />}
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{e.name}</div>
              {!e.is_dir && e.size != null && <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{fmt(e.size)}</div>}
            </div>
          </NativeRow>
        ))
      )}
    </NativePanelShell>
  )
}

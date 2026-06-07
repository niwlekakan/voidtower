import { useEffect, useState } from 'react'
import { Trash2, Play } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface FirewallRule { id: string; chain: string; protocol: string; port?: string; action: string; enabled: boolean }

export default function NativeFirewallPanel() {
  const [rules, setRules] = useState<FirewallRule[]>([])
  const [loading, setLoading] = useState(true)

  async function load() {
    const r = await fetch('/api/firewall', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setRules(d.rules ?? []) }
    setLoading(false)
  }

  async function deleteRule(id: string) {
    await fetch('/api/firewall/rules/delete', {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id }),
    })
    load()
  }

  async function runAction(action: string) {
    await fetch('/api/firewall/action', {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action }),
    })
    load()
  }

  useEffect(() => { load() }, [])

  return (
    <NativePanelShell actions={
      <IconBtn title="Reload firewall" onClick={() => runAction('reload')}><Play size={11} /></IconBtn>
    }>
      {loading ? <LoadingState /> : rules.length === 0 ? <EmptyState text="No rules" /> :
        rules.map(rule => (
          <NativeRow key={rule.id}>
            <StatusDot color={rule.action === 'ACCEPT' ? '#22c55e' : rule.action === 'DROP' ? '#ef4444' : '#f59e0b'} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{rule.chain} {rule.protocol}{rule.port ? `:${rule.port}` : ''}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{rule.action}</div>
            </div>
            <IconBtn title="Delete rule" onClick={() => deleteRule(rule.id)} danger><Trash2 size={11} /></IconBtn>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}

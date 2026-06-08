import { useState, useEffect } from 'react'
import { Server, Copy, Check, Loader2 } from 'lucide-react'
import { api } from '@/api/client'
import type { AppDef } from '@/api/types'

interface Props { app: AppDef; onClose: () => void }

interface PveHost { id: string; name: string; node: string }

interface Result {
  vmid: string
  hostname: string
  node: string
  bootstrap_script: string
}

export default function DeployToProxmoxModal({ app, onClose }: Props) {
  const [hosts, setHosts]     = useState<PveHost[]>([])
  const [hostId, setHostId]   = useState('')
  const [node, setNode]       = useState('')
  const [hostname, setHostname] = useState(app.id.replace(/[^a-z0-9-]/g, '-').slice(0, 30))
  const [template, setTemplate] = useState('local:vztmpl/ubuntu-22.04-standard_22.04-1_amd64.tar.zst')
  const [storage, setStorage] = useState('local-lvm')
  const [cores, setCores]     = useState(2)
  const [memory, setMemory]   = useState(1024)
  const [diskGb, setDiskGb]   = useState(20)
  const [loading, setLoading] = useState(false)
  const [err, setErr]         = useState('')
  const [result, setResult]   = useState<Result | null>(null)
  const [copied, setCopied]   = useState(false)

  useEffect(() => {
    api.proxmox.listHosts().then(h => {
      setHosts(h)
      if (h.length > 0) { setHostId(h[0].id); setNode(h[0].node) }
    }).catch(() => {})
  }, [])

  const deploy = async () => {
    if (!hostId || !node || !hostname || !template) { setErr('Fill in all required fields'); return }
    setLoading(true); setErr('')
    try {
      const composeYaml = JSON.stringify(app.compose, null, 2)
      const res = await api.proxmoxDeploy.deployToLxc(hostId, {
        node, hostname, ostemplate: template,
        compose_yaml: composeYaml,
        cores, memory, storage, disk_gb: diskGb,
      })
      setResult(res)
    } catch (e: any) { setErr(e.message || 'Deploy failed') }
    finally { setLoading(false) }
  }

  const copyScript = async () => {
    if (!result) return
    await navigator.clipboard.writeText(result.bootstrap_script)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const inp = (value: string | number, onChange: (v: string) => void, placeholder?: string, type = 'text') => (
    <input type={type} value={value} onChange={e => onChange(e.target.value)} placeholder={placeholder}
      style={{ background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 6, color: 'var(--text-primary)', padding: '5px 8px', fontSize: 13, width: '100%' }} />
  )

  const lbl = (text: string) => (
    <label style={{ fontSize: 11, color: 'var(--text-muted)', fontWeight: 500 }}>{text}</label>
  )

  return (
    <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000 }}
      onClick={e => e.target === e.currentTarget && onClose()}>
      <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 12, padding: 24, width: 520, maxHeight: '85vh', overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 14 }}>

        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Server size={16} style={{ color: 'var(--accent-primary)' }} />
          <span style={{ fontWeight: 600, fontSize: 15 }}>Deploy {app.name} to Proxmox LXC</span>
        </div>

        {result ? (
          <>
            <div style={{ padding: '10px 14px', borderRadius: 8, background: 'var(--accent-success)18', border: '1px solid var(--accent-success)44', color: 'var(--accent-success)', fontSize: 13 }}>
              LXC <strong>{result.vmid}</strong> created on node <strong>{result.node}</strong>.
              SSH in and run the bootstrap script to install Docker and start the app.
            </div>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 6 }}>
                <span style={{ fontSize: 12, fontWeight: 500 }}>Bootstrap script</span>
                <button onClick={copyScript} style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 11, background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent-primary)' }}>
                  {copied ? <><Check size={12} /> Copied</> : <><Copy size={12} /> Copy</>}
                </button>
              </div>
              <pre style={{ fontSize: 11, background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 8, padding: '10px 12px', overflowX: 'auto', maxHeight: 220, color: 'var(--text-secondary)', margin: 0 }}>
                {result.bootstrap_script}
              </pre>
              <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 6 }}>
                Run as root inside LXC {result.vmid}: <code>pct enter {result.vmid}</code> then paste the script.
              </div>
            </div>
            <button onClick={onClose} style={{ alignSelf: 'flex-end', padding: '6px 16px', borderRadius: 6, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer', fontSize: 13 }}>
              Done
            </button>
          </>
        ) : (
          <>
            {hosts.length === 0 ? (
              <div style={{ fontSize: 13, color: 'var(--text-muted)' }}>
                No Proxmox hosts configured. Add one in <strong>Proxmox</strong> first.
              </div>
            ) : (
              <>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {lbl('Proxmox host')}
                    <select value={hostId} onChange={e => { setHostId(e.target.value); const h = hosts.find(x => x.id === e.target.value); if (h) setNode(h.node) }}
                      style={{ background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 6, color: 'var(--text-primary)', padding: '5px 8px', fontSize: 13 }}>
                      {hosts.map(h => <option key={h.id} value={h.id}>{h.name}</option>)}
                    </select>
                  </div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {lbl('Node')}
                    {inp(node, setNode, 'pve')}
                  </div>
                </div>

                <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                  {lbl('Container hostname')}
                  {inp(hostname, setHostname, 'my-app')}
                </div>

                <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                  {lbl('OS template (must exist in Proxmox storage)')}
                  {inp(template, setTemplate, 'local:vztmpl/ubuntu-22.04-standard_22.04-1_amd64.tar.zst')}
                </div>

                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr 1fr', gap: 8 }}>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {lbl('Cores')}
                    {inp(cores, v => setCores(+v || 2), '2', 'number')}
                  </div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {lbl('RAM (MB)')}
                    {inp(memory, v => setMemory(+v || 512), '1024', 'number')}
                  </div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {lbl('Disk (GB)')}
                    {inp(diskGb, v => setDiskGb(+v || 8), '20', 'number')}
                  </div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {lbl('Storage')}
                    {inp(storage, setStorage, 'local-lvm')}
                  </div>
                </div>

                <div style={{ fontSize: 11, color: 'var(--text-muted)', lineHeight: 1.5 }}>
                  Creates the LXC with <code>nesting=1</code> (required for Docker), starts it, and generates a bootstrap script to install Docker and start the app inside.
                </div>

                {err && <div style={{ color: 'var(--accent-danger)', fontSize: 12 }}>{err}</div>}

                <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                  <button onClick={onClose} style={{ padding: '6px 14px', borderRadius: 6, border: '1px solid var(--border)', background: 'transparent', color: 'var(--text-primary)', cursor: 'pointer', fontSize: 13 }}>Cancel</button>
                  <button onClick={deploy} disabled={loading} style={{ padding: '6px 14px', borderRadius: 6, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer', fontSize: 13, opacity: loading ? 0.7 : 1, display: 'flex', alignItems: 'center', gap: 6 }}>
                    {loading && <Loader2 size={13} className="animate-spin" />}
                    {loading ? 'Creating LXC…' : 'Create LXC'}
                  </button>
                </div>
              </>
            )}
          </>
        )}
      </div>
    </div>
  )
}

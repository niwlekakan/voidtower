import { useState, useEffect } from 'react'
import NativePanelShell from './NativePanelShell'
import { useThemeStore } from '@/store/theme'
import { BG_PRESETS, GLASS_LEVELS } from '@/theme/themes'
import { useNavConfigStore, resolvedNavItems } from '@/store/navConfig'

const TABS = [
  { id: 'appearance', label: 'Appearance' },
  { id: 'accessibility', label: 'Accessibility' },
  { id: 'desktop', label: 'Desktop' },
  { id: 'general', label: 'General' },
  { id: 'navigation', label: 'Navigation' },
]

function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button onClick={onClick} style={{
      padding: '4px 10px', borderRadius: 6, border: 'none', cursor: 'pointer', fontSize: 11,
      background: active ? 'var(--accent-primary)' : 'var(--bg-elevated)',
      color: active ? '#fff' : 'var(--text-secondary)',
    }}>{children}</button>
  )
}

function Toggle({ on, onChange, label, desc }: { on: boolean; onChange: (v: boolean) => void; label: string; desc: string }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '7px 10px', borderBottom: '1px solid var(--border-subtle)' }}>
      <div>
        <div style={{ fontSize: 12, color: 'var(--text-primary)' }}>{label}</div>
        <div style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 1 }}>{desc}</div>
      </div>
      <button role="switch" aria-checked={on} onClick={() => onChange(!on)} style={{
        width: 36, height: 20, borderRadius: 10, border: 'none', cursor: 'pointer', position: 'relative', flexShrink: 0,
        background: on ? 'var(--accent-primary)' : 'var(--bg-elevated)',
      }}>
        <span style={{ position: 'absolute', top: 2, left: on ? 18 : 2, width: 16, height: 16, borderRadius: '50%', background: '#fff', transition: 'left 0.15s' }} />
      </button>
    </div>
  )
}

function SL({ text }: { text: string }) {
  return <div style={{ padding: '8px 10px 4px', fontSize: 10, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>{text}</div>
}

const GENERAL_LABELS: Record<string, string> = {
  instance_name: 'Instance name',
  login_tagline: 'Login tagline',
  login_bg_url: 'Login background URL',
}

export default function NativeSettingsPanel() {
  const [tab, setTab] = useState('appearance')
  const { activeTheme, setTheme, glassLevel, setGlass, bgPreset, setBgPreset, randomize, a11y, setA11y } = useThemeStore()
  const allThemes = useThemeStore(s => s.allThemes())

  // General tab state
  const [general, setGeneral] = useState({ instance_name: '', login_tagline: '', login_bg_url: '' })
  const [generalLoading, setGeneralLoading] = useState(false)
  const [generalSaved, setGeneralSaved] = useState(false)

  useEffect(() => {
    if (tab !== 'general') return
    fetch('/api/settings/general', { credentials: 'include' })
      .then(r => r.json())
      .then(d => setGeneral({
        instance_name: d.instance_name ?? '',
        login_tagline: d.login_tagline ?? '',
        login_bg_url: d.login_bg_url ?? '',
      }))
  }, [tab])

  async function saveGeneral() {
    setGeneralLoading(true)
    await fetch('/api/settings/general', {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(general),
    })
    setGeneralLoading(false)
    setGeneralSaved(true)
    setTimeout(() => setGeneralSaved(false), 2000)
  }

  // Navigation tab state
  const navItems = useNavConfigStore(s => s.items)
  const setNavItems = useNavConfigStore(s => s.setItems)
  const navResolved = resolvedNavItems(navItems)

  function toggleNav(id: string) {
    setNavItems(navResolved.map(n => n.id === id ? { ...n, visible: !n.visible } : n))
  }
  function renameNav(id: string, label: string) {
    setNavItems(navResolved.map(n => n.id === id ? { ...n, label } : n))
  }
  function resetNav() {
    useNavConfigStore.getState().resetItems()
  }

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab}>
      {tab === 'appearance' && (
        <>
          <SL text="Theme" />
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, padding: '0 10px 8px' }}>
            {allThemes.map(t => (
              <Chip key={t.id} active={activeTheme.id === t.id} onClick={() => setTheme(t.id)}>{t.name}</Chip>
            ))}
            <button onClick={randomize} style={{ padding: '4px 10px', borderRadius: 6, border: '1px dashed var(--border-subtle)', cursor: 'pointer', fontSize: 11, background: 'none', color: 'var(--text-muted)' }}>✦ Random</button>
          </div>
          <SL text="Glass" />
          <div style={{ display: 'flex', gap: 4, padding: '0 10px 8px' }}>
            {GLASS_LEVELS.map(g => (
              <Chip key={g.id} active={glassLevel === g.id} onClick={() => setGlass(g.id)}>{g.label}</Chip>
            ))}
          </div>
          <SL text="Background" />
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, padding: '0 10px 8px' }}>
            {BG_PRESETS.map(p => (
              <Chip key={p.id} active={bgPreset === p.id} onClick={() => setBgPreset(p.id)}>{p.label}</Chip>
            ))}
          </div>
        </>
      )}
      {tab === 'accessibility' && (
        <>
          <Toggle on={a11y.reduceTransparency} onChange={v => setA11y({ reduceTransparency: v })} label="Reduce Transparency" desc="Remove glass blur from all panels" />
          <Toggle on={a11y.reduceMotion} onChange={v => setA11y({ reduceMotion: v })} label="Reduce Motion" desc="Disable transitions and animations" />
          <Toggle on={a11y.largeControls} onChange={v => setA11y({ largeControls: v })} label="Large Controls" desc="44px minimum tap targets" />
          <Toggle on={a11y.preferStacked} onChange={v => setA11y({ preferStacked: v })} label="Stacked Layout" desc="Single-column layout in panels" />
        </>
      )}
      {tab === 'desktop' && (
        <>
          <SL text="Snap Zones" />
          <div style={{ padding: '0 10px 8px', fontSize: 11, color: 'var(--text-secondary)', lineHeight: 1.6 }}>
            Drag a panel to a screen edge to snap. Corners → quarter screen. Top center → fullscreen. Hover screen edges to preview zones.
          </div>
          <SL text="Shortcuts" />
          {[['Ctrl+1/2/3/4','Switch workspace'],['Ctrl+Alt+O','Odysseus panel'],['Ctrl+Alt+T','Tiling mode'],['Ctrl+Alt+H/V','Split H/V'],['Ctrl+W','Close panel'],['Ctrl+Space','Command bar']].map(([k, v]) => (
            <div key={k} style={{ display: 'flex', justifyContent: 'space-between', padding: '5px 10px', borderBottom: '1px solid var(--border-subtle)' }}>
              <code style={{ fontSize: 10, color: 'var(--accent-primary)', background: 'var(--accent-primary-subtle)', padding: '2px 5px', borderRadius: 3 }}>{k}</code>
              <span style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{v}</span>
            </div>
          ))}
        </>
      )}
      {tab === 'general' && (
        <>
          <SL text="Instance" />
          {(['instance_name', 'login_tagline', 'login_bg_url'] as const).map(key => (
            <div key={key} style={{ padding: '4px 10px 6px' }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>
                {GENERAL_LABELS[key]}
              </div>
              <input
                value={general[key]}
                onChange={e => setGeneral(p => ({ ...p, [key]: e.target.value }))}
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 8px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }}
              />
            </div>
          ))}
          <div style={{ padding: '6px 10px' }}>
            <button onClick={saveGeneral} disabled={generalLoading} style={{ padding: '5px 14px', borderRadius: 6, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer', fontSize: 11 }}>
              {generalSaved ? 'Saved!' : generalLoading ? 'Saving…' : 'Save'}
            </button>
          </div>
        </>
      )}
      {tab === 'navigation' && (
        <>
          <SL text="Dock items" />
          <div style={{ padding: '0 10px 4px', fontSize: 10, color: 'var(--text-muted)' }}>
            Toggle or rename items in the dock. Reset to restore defaults.
          </div>
          {navResolved.map(item => (
            <div key={item.id} style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '4px 10px', borderBottom: '1px solid var(--border-subtle)' }}>
              <button
                onClick={() => toggleNav(item.id)}
                style={{
                  width: 28, height: 16, borderRadius: 8, border: 'none', cursor: 'pointer', position: 'relative', flexShrink: 0,
                  background: item.visible ? 'var(--accent-primary)' : 'var(--bg-elevated)',
                }}
              >
                <span style={{ position: 'absolute', top: 1, left: item.visible ? 14 : 1, width: 14, height: 14, borderRadius: '50%', background: '#fff', transition: 'left 0.12s' }} />
              </button>
              <input
                value={item.label}
                onChange={e => renameNav(item.id, e.target.value)}
                style={{ flex: 1, background: 'transparent', border: '1px solid transparent', borderRadius: 3, padding: '2px 4px', fontSize: 11, color: item.visible ? 'var(--text-primary)' : 'var(--text-muted)', outline: 'none' }}
                onFocus={e => (e.currentTarget.style.borderColor = 'var(--border-subtle)')}
                onBlur={e => (e.currentTarget.style.borderColor = 'transparent')}
              />
              <span style={{ fontSize: 9, color: 'var(--text-muted)', flexShrink: 0 }}>{item.id}</span>
            </div>
          ))}
          <div style={{ padding: '8px 10px' }}>
            <button onClick={resetNav} style={{ padding: '4px 10px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer', fontSize: 11 }}>
              Reset to defaults
            </button>
          </div>
        </>
      )}
    </NativePanelShell>
  )
}

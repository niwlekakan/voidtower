import { useState, useRef, useEffect } from 'react'
import { Globe } from 'lucide-react'
import { notify } from '@/store/notifications'

interface BrandingSettings {
  instance_name: string
  login_tagline: string
  custom_css: string
  login_bg_url: string
  instance_logo: string
}

export default function BrandingTab() {
  const [name, setName] = useState('')
  const [tagline, setTagline] = useState('')
  const [bgUrl, setBgUrl] = useState('')
  const [customCss, setCustomCss] = useState('')
  const [logo, setLogo] = useState('')
  const [saved, setSaved] = useState(false)
  const [loading, setLoading] = useState(true)
  const logoInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    fetch('/api/settings/general', { credentials: 'include' })
      .then(r => r.ok ? r.json() : null)
      .then((d: BrandingSettings | null) => {
        if (d) {
          setName(d.instance_name ?? '')
          setTagline(d.login_tagline ?? '')
          setBgUrl(d.login_bg_url ?? '')
          setCustomCss(d.custom_css ?? '')
          setLogo(d.instance_logo ?? '')
        }
      })
      .catch(() => notify.error('Failed to load branding settings'))
      .finally(() => setLoading(false))
  }, [])

  const handleLogoFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => setLogo(reader.result as string)
    reader.readAsDataURL(file)
  }

  const save = async () => {
    try {
      const response = await fetch('/api/settings/general', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          instance_name: name.trim() || 'VoidTower',
          login_tagline: tagline.trim() || null,
          login_bg_url: bgUrl.trim() || null,
          custom_css: customCss || null,
          instance_logo: logo || null,
        }),
      })

      if (!response.ok) {
        notify.error('Failed to save branding settings')
        return
      }

      setSaved(true)
      setTimeout(() => setSaved(false), 2000)

      const finalName = name.trim() || 'VoidTower'
      window.dispatchEvent(new CustomEvent('vt-settings-changed', { detail: { instance_name: finalName, instance_logo: logo } }))

      // Inject custom CSS
      let styleEl = document.getElementById('vt-custom-css') as HTMLStyleElement | null
      if (customCss) {
        if (!styleEl) {
          styleEl = document.createElement('style')
          styleEl.id = 'vt-custom-css'
          document.head.appendChild(styleEl)
        }
        styleEl.textContent = customCss
      } else if (styleEl) {
        styleEl.textContent = ''
      }

      notify.success('Branding settings saved')
    } catch {
      notify.error('Failed to save branding settings')
    }
  }

  const inputStyle = { background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }

  return (
    <div className="space-y-6 max-w-3xl py-6">
      <div className="card space-y-4">
        <div className="flex items-center gap-2">
          <Globe size={16} style={{ color: 'var(--accent-primary)' }} />
          <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Instance Branding</h2>
        </div>

        {/* Instance name */}
        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Instance name</label>
          <input
            value={loading ? '' : name}
            onChange={e => setName(e.target.value)}
            placeholder="VoidTower"
            disabled={loading}
            className="w-full px-3 py-2 rounded text-sm outline-none"
            style={inputStyle}
          />
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Shown in the browser tab title, sidebar, and login page.</p>
        </div>

        {/* Logo */}
        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Instance logo</label>
          <div className="flex items-center gap-3">
            {logo && (
              <img
                src={logo}
                alt="logo preview"
                style={{
                  width: 48,
                  height: 48,
                  objectFit: 'contain',
                  borderRadius: 6,
                  border: '1px solid var(--border-subtle)',
                  background: 'var(--bg-elevated)',
                }}
              />
            )}
            <div className="flex gap-2">
              <button
                onClick={() => logoInputRef.current?.click()}
                className="px-3 py-1.5 rounded text-xs font-medium"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              >
                {logo ? 'Change' : 'Choose file'}
              </button>
              {logo && (
                <button
                  onClick={() => {
                    setLogo('')
                    if (logoInputRef.current) logoInputRef.current.value = ''
                  }}
                  className="px-3 py-1.5 rounded text-xs"
                  style={{ color: 'var(--accent-danger)' }}
                >
                  Clear
                </button>
              )}
            </div>
            <input
              ref={logoInputRef}
              type="file"
              accept="image/*"
              className="hidden"
              onChange={handleLogoFile}
            />
          </div>
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
            PNG or SVG image. Used as favicon and on the login page. Max ~256 KB.
          </p>
        </div>

        {/* Login tagline */}
        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Login tagline</label>
          <input
            value={tagline}
            onChange={e => setTagline(e.target.value)}
            placeholder="Self-hosted infrastructure dashboard"
            className="w-full px-3 py-2 rounded text-sm outline-none"
            style={inputStyle}
          />
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Short description shown below the instance name on the login page.</p>
        </div>

        {/* Login background URL */}
        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Login background image URL</label>
          <input
            value={bgUrl}
            onChange={e => setBgUrl(e.target.value)}
            placeholder="https://example.com/bg.jpg"
            className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
            style={inputStyle}
          />
          {bgUrl && (
            <div
              style={{
                marginTop: 6,
                height: 80,
                borderRadius: 6,
                overflow: 'hidden',
                border: '1px solid var(--border-subtle)',
              }}
            >
              <img
                src={bgUrl}
                alt="bg preview"
                style={{ width: '100%', height: '100%', objectFit: 'cover' }}
                onError={() => notify.error('Failed to load background image')}
              />
            </div>
          )}
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Full URL to a publicly accessible image.</p>
        </div>

        {/* Custom CSS */}
        <div>
          <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Custom CSS (injected globally)</label>
          <textarea
            value={customCss}
            onChange={e => setCustomCss(e.target.value)}
            rows={10}
            placeholder={`:root {
  --accent-primary: #ff6b6b;
  --bg-root: #1a1a2e;
}

.vt-sidebar { border-radius: 12px; }`}
            className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
            style={{ ...inputStyle, resize: 'vertical' }}
          />
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
            Custom CSS rules. Max 8192 characters. Applied immediately on save. Use CSS variables or class selectors.
          </p>
        </div>

        {/* Save button */}
        <button
          onClick={save}
          className="px-4 py-2 rounded text-xs font-medium transition-colors"
          style={{
            background: saved ? 'var(--accent-success-subtle)' : 'var(--accent-primary)',
            color: saved ? 'var(--accent-success)' : '#fff',
          }}
        >
          {saved ? '✓ Saved' : 'Save branding'}
        </button>
      </div>

      {/* Preview section */}
      <div className="card space-y-3">
        <h3 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Login page preview</h3>
        <div
          className="rounded border overflow-hidden"
          style={{
            borderColor: 'var(--border-subtle)',
            background: 'var(--bg-root)',
            backgroundImage: bgUrl ? `url(${bgUrl})` : undefined,
            backgroundSize: 'cover',
            backgroundPosition: 'center',
            aspectRatio: '16 / 9',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            minHeight: 200,
          }}
        >
          <div style={{ background: 'var(--bg-panel)', padding: 32, borderRadius: 8, maxWidth: 320, width: '90%' }}>
            <div className="flex items-center gap-2 mb-4">
              {logo ? (
                <img
                  src={logo}
                  alt="logo"
                  style={{
                    width: 28,
                    height: 28,
                    objectFit: 'contain',
                    flexShrink: 0,
                  }}
                />
              ) : (
                <div
                  style={{
                    width: 28,
                    height: 28,
                    borderRadius: 4,
                    background: 'var(--accent-primary)',
                  }}
                />
              )}
              <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-primary)' }}>
                {name || 'VoidTower'}
              </span>
            </div>
            {tagline && (
              <p style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 16 }}>
                {tagline}
              </p>
            )}
            <div style={{ height: 40, background: 'var(--bg-elevated)', borderRadius: 4, marginBottom: 12 }} />
            <div style={{ height: 40, background: 'var(--bg-elevated)', borderRadius: 4 }} />
          </div>
        </div>
      </div>
    </div>
  )
}

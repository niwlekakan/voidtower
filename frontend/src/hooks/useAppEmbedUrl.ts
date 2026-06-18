import { useEffect, useRef, useState } from 'react'
import { api } from '@/api/client'
import type { AppDef } from '@/api/types'

interface AppEmbedUrl {
  iframeSrc: string | null
  embedUrl: string | null
  loading: boolean
  proxyCreated: boolean
}

/**
 * Resolves the iframe-embeddable URL for a deployed App Vault app.
 * Prefers the LAN nginx port-proxy (handles auth flows/POST, strips
 * X-Frame-Options) and falls back to the backend's GET-only proxy when
 * nginx isn't configured. `def.web_port`/`def.web_path` (or `def.links.web_ui`)
 * from the catalog take precedence over the raw deployed port/root path.
 */
export function useAppEmbedUrl(
  projectName: string | null,
  def: AppDef | null,
  primaryPort: number | null,
): AppEmbedUrl {
  const [iframeSrc, setIframeSrc] = useState<string | null>(null)
  const [embedUrl, setEmbedUrl] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [proxyCreated, setProxyCreated] = useState(false)

  const seqRef = useRef(0)
  useEffect(() => {
    if (!projectName || primaryPort === null) {
      setIframeSrc(null)
      setEmbedUrl(null)
      setLoading(false)
      setProxyCreated(false)
      return
    }

    const seq = ++seqRef.current
    setIframeSrc(null)
    setEmbedUrl(null)
    setLoading(true)
    setProxyCreated(false)

    const path = def?.links?.web_ui ?? def?.web_path ?? '/'
    const uiPort = def?.web_port ?? primaryPort
    const fullPath = path.startsWith('/') ? path : '/' + path
    const cleanPath = fullPath.slice(1)
    const backendProxy = `/api/apps/embed/${projectName}/${cleanPath}`

    api.apps.openUi(projectName, uiPort ?? 0).then(r => {
      if (seq !== seqRef.current) return
      const lanUrl = r.embed_url ? r.embed_url + fullPath : null
      setEmbedUrl(lanUrl)
      setProxyCreated(!!r.proxy_created)
      setIframeSrc(lanUrl ?? backendProxy)
      setLoading(false)
    }).catch(() => {
      if (seq !== seqRef.current) return
      setIframeSrc(backendProxy)
      setLoading(false)
    })
  }, [projectName, def, primaryPort])

  return { iframeSrc, embedUrl, loading, proxyCreated }
}

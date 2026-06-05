/// <reference lib="webworker" />

declare const self: ServiceWorkerGlobalScope

const CACHE_NAME = 'voidtower-shell-v1'

// Assets to pre-cache on install (shell resources)
const SHELL_URLS = [
  '/',
  '/index.html',
]

// ── Install: pre-cache the shell ─────────────────────────────────────────────

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(SHELL_URLS)),
  )
  // Activate immediately, don't wait for old tabs to close
  self.skipWaiting()
})

// ── Activate: remove old caches ───────────────────────────────────────────────

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys
          .filter((key) => key !== CACHE_NAME)
          .map((key) => caches.delete(key)),
      ),
    ),
  )
  // Take control of all open tabs immediately
  self.clients.claim()
})

// ── Fetch: routing strategy ───────────────────────────────────────────────────

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url)

  // Network-first for API calls — always want fresh data
  if (url.pathname.startsWith('/api/')) {
    event.respondWith(networkFirst(event.request))
    return
  }

  // Cache-first for JS/CSS assets (they are content-hashed by Vite)
  if (
    url.pathname.startsWith('/assets/') &&
    (url.pathname.endsWith('.js') || url.pathname.endsWith('.css'))
  ) {
    event.respondWith(cacheFirst(event.request))
    return
  }

  // Shell HTML navigation — cache-first with network fallback
  if (event.request.mode === 'navigate') {
    event.respondWith(cacheFirst(event.request, '/index.html'))
    return
  }

  // Everything else: network-first
  event.respondWith(networkFirst(event.request))
})

// ── Strategy helpers ──────────────────────────────────────────────────────────

/** Cache-first: return cached response immediately; update cache in background. */
async function cacheFirst(request: Request, fallbackUrl?: string): Promise<Response> {
  const cached = await caches.match(request)
  if (cached) return cached

  try {
    const response = await fetch(request)
    if (response.ok) {
      const cache = await caches.open(CACHE_NAME)
      cache.put(request, response.clone())
    }
    return response
  } catch {
    if (fallbackUrl) {
      const fallback = await caches.match(fallbackUrl)
      if (fallback) return fallback
    }
    return new Response('Offline', { status: 503, statusText: 'Service Unavailable' })
  }
}

/** Network-first: try network; fall back to cache if offline. */
async function networkFirst(request: Request): Promise<Response> {
  try {
    const response = await fetch(request)
    if (response.ok && request.method === 'GET') {
      const cache = await caches.open(CACHE_NAME)
      cache.put(request, response.clone())
    }
    return response
  } catch {
    const cached = await caches.match(request)
    if (cached) return cached
    return new Response('Offline', { status: 503, statusText: 'Service Unavailable' })
  }
}

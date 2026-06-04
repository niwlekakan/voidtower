import { create } from 'zustand'
import type { MetricsSnapshot } from '@/api/types'
import { api } from '@/api/client'

interface MetricsStore {
  snapshot: MetricsSnapshot | null
  connected: boolean
  error: string | null
  ws: WebSocket | null
  connect: () => void
  disconnect: () => void
}

export const useMetricsStore = create<MetricsStore>()((set, get) => ({
  snapshot: null,
  connected: false,
  error: null,
  ws: null,

  connect: () => {
    if (get().ws) return
    const ws = new WebSocket(api.metrics.wsUrl())

    ws.onopen = () => set({ connected: true, error: null })

    ws.onmessage = (e) => {
      try {
        const data = JSON.parse(e.data) as MetricsSnapshot
        set({ snapshot: data })
      } catch {}
    }

    ws.onerror = () => set({ error: 'WebSocket error', connected: false })

    ws.onclose = () => {
      set({ connected: false, ws: null })
      // Reconnect after 3s
      setTimeout(() => {
        if (!get().ws) get().connect()
      }, 3000)
    }

    set({ ws })
  },

  disconnect: () => {
    get().ws?.close()
    set({ ws: null, connected: false })
  },
}))

import { create } from 'zustand'
import type { AgentStatusUpdate } from '@/api/types'
import { api } from '@/api/client'

interface AgentsStore {
  statuses: Record<string, AgentStatusUpdate>
  connected: boolean
  error: string | null
  ws: WebSocket | null
  connect: () => void
  disconnect: () => void
}

export const useAgentsStore = create<AgentsStore>()((set, get) => ({
  statuses: {},
  connected: false,
  error: null,
  ws: null,

  connect: () => {
    if (get().ws) return
    const ws = new WebSocket(api.agents.wsUrl())

    ws.onopen = () => set({ connected: true, error: null })

    ws.onmessage = (e) => {
      try {
        const update = JSON.parse(e.data) as AgentStatusUpdate
        set((s) => ({ statuses: { ...s.statuses, [update.agent_id]: update } }))
      } catch { /* empty */ }
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

import { useCallback, useEffect, useState } from 'react'
import { api } from '@/api/client'
import type { AgentWithStatus, CreateAgentRequest, ExportedAgent, UpdateAgentRequest } from '@/api/types'

export function useAgents() {
  const [agents, setAgents] = useState<AgentWithStatus[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const reload = useCallback(async () => {
    try {
      const data = await api.agents.list()
      setAgents(data)
      setError(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load agents')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { reload() }, [reload])

  const create = useCallback(async (req: CreateAgentRequest) => {
    await api.agents.create(req)
    await reload()
  }, [reload])

  const update = useCallback(async (id: string, patch: UpdateAgentRequest) => {
    await api.agents.update(id, patch)
    await reload()
  }, [reload])

  const remove = useCallback(async (id: string) => {
    await api.agents.delete(id)
    await reload()
  }, [reload])

  const exportAgents = useCallback(() => api.agents.export(), [])

  const importAgents = useCallback(async (agents: ExportedAgent[]) => {
    const result = await api.agents.import(agents)
    await reload()
    return result
  }, [reload])

  return { agents, loading, error, reload, create, update, remove, exportAgents, importAgents }
}

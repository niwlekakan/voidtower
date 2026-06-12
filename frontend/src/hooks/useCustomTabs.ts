import { useCallback, useEffect, useState } from 'react'
import { api } from '@/api/client'
import type { CreateCustomTabRequest, CustomTab, ExportedTab, UpdateCustomTabRequest } from '@/api/types'

export function useCustomTabs() {
  const [tabs, setTabs] = useState<CustomTab[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const reload = useCallback(async () => {
    try {
      const data = await api.tabs.list()
      setTabs(data)
      setError(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load tabs')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { reload() }, [reload])

  const create = useCallback(async (req: CreateCustomTabRequest) => {
    await api.tabs.create(req)
    await reload()
  }, [reload])

  const update = useCallback(async (id: string, patch: UpdateCustomTabRequest) => {
    await api.tabs.update(id, patch)
    await reload()
  }, [reload])

  const remove = useCallback(async (id: string) => {
    await api.tabs.delete(id)
    await reload()
  }, [reload])

  const reorder = useCallback(async (ids: string[]) => {
    await api.tabs.reorder(ids)
    await reload()
  }, [reload])

  const exportTabs = useCallback(() => api.tabs.export(), [])

  const importTabs = useCallback(async (tabs: ExportedTab[]) => {
    const result = await api.tabs.import(tabs)
    await reload()
    return result
  }, [reload])

  return { tabs, loading, error, reload, create, update, remove, reorder, exportTabs, importTabs }
}

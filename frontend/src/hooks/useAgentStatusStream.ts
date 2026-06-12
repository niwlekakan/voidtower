import { useEffect } from 'react'
import { useAgentsStore } from '@/store/agents'
import { useAuthStore } from '@/store/auth'

/** Subscribes to live agent status updates over `/api/agents/ws`. */
export function useAgentStatusStream() {
  const { connect, disconnect, statuses, connected, error } = useAgentsStore()
  const status = useAuthStore((s) => s.status)

  useEffect(() => {
    if (status === 'authenticated') {
      connect()
      return () => disconnect()
    }
  }, [status, connect, disconnect])

  return { statuses, connected, error }
}

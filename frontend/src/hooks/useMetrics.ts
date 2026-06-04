import { useEffect } from 'react'
import { useMetricsStore } from '@/store/metrics'
import { useAuthStore } from '@/store/auth'

export function useMetrics() {
  const { connect, disconnect, snapshot, connected, error } = useMetricsStore()
  const status = useAuthStore((s) => s.status)

  useEffect(() => {
    if (status === 'authenticated') {
      connect()
      return () => disconnect()
    }
  }, [status, connect, disconnect])

  return { snapshot, connected, error }
}

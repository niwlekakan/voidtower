import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import * as SecureStore from 'expo-secure-store'
import { api, setBaseUrl, ApiClientError } from '../api/client'
import type { User } from '../api/types'

const SERVER_URL_KEY = 'vt_server_url'

interface AuthState {
  loading: boolean
  serverUrl: string | null
  user: User | null
  error: string | null
  totpRequired: boolean
  setServer: (url: string) => Promise<void>
  login: (username: string, password: string, totpCode?: string) => Promise<void>
  logout: () => Promise<void>
  refreshMe: () => Promise<void>
}

const AuthContext = createContext<AuthState | null>(null)

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [loading, setLoading] = useState(true)
  const [serverUrl, setServerUrl] = useState<string | null>(null)
  const [user, setUser] = useState<User | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [totpRequired, setTotpRequired] = useState(false)

  useEffect(() => {
    (async () => {
      const savedUrl = await SecureStore.getItemAsync(SERVER_URL_KEY)
      if (savedUrl) {
        setBaseUrl(savedUrl)
        setServerUrl(savedUrl)
        try {
          const { user } = await api.auth.me()
          setUser(user)
        } catch {
          // No valid session cookie (fresh install, expired session, etc.) —
          // fall through to the login screen.
        }
      }
      setLoading(false)
    })()
  }, [])

  const setServer = useCallback(async (url: string) => {
    const normalized = url.replace(/\/+$/, '')
    if (!normalized) {
      await SecureStore.deleteItemAsync(SERVER_URL_KEY)
      setBaseUrl('')
      setServerUrl(null)
      return
    }
    await SecureStore.setItemAsync(SERVER_URL_KEY, normalized)
    setBaseUrl(normalized)
    setServerUrl(normalized)
  }, [])

  const login = useCallback(async (username: string, password: string, totpCode?: string) => {
    setError(null)
    try {
      const { user } = await api.auth.login(username, password, totpCode)
      setTotpRequired(false)
      setUser(user)
    } catch (e) {
      if (e instanceof ApiClientError && e.code === 'totp_required') {
        setTotpRequired(true)
        return
      }
      setError(e instanceof ApiClientError ? e.message : 'Could not reach the server')
      throw e
    }
  }, [])

  const logout = useCallback(async () => {
    try { await api.auth.logout() } catch { /* best-effort */ }
    setUser(null)
  }, [])

  const refreshMe = useCallback(async () => {
    const { user } = await api.auth.me()
    setUser(user)
  }, [])

  const value = useMemo(
    () => ({ loading, serverUrl, user, error, totpRequired, setServer, login, logout, refreshMe }),
    [loading, serverUrl, user, error, totpRequired, setServer, login, logout, refreshMe],
  )

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be used within AuthProvider')
  return ctx
}

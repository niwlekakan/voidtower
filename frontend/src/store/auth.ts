import { create } from 'zustand'
import type { User } from '@/api/types'

type AuthStatus = 'idle' | 'loading' | 'authenticated' | 'unauthenticated'

interface AuthStore {
  user: User | null
  status: AuthStatus
  sessionEpoch: number
  setUser: (user: User | null) => void
  setStatus: (status: AuthStatus) => void
  logout: () => void
}

export const useAuthStore = create<AuthStore>()((set) => ({
  user: null,
  status: 'idle',
  sessionEpoch: 0,

  setUser: (user) => set((state) => ({ user, status: user ? 'authenticated' : 'unauthenticated', sessionEpoch: state.sessionEpoch + 1 })),
  setStatus: (status) => set({ status }),
  logout: () => set((state) => ({ user: null, status: 'unauthenticated', sessionEpoch: state.sessionEpoch + 1 })),
}))

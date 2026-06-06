import { create } from 'zustand'
import type { DeployedApp, AppDef } from '@/api/types'

interface EmbedState {
  app: DeployedApp | null
  def: AppDef | null
  open: (app: DeployedApp, def: AppDef) => void
  close: () => void
}

export const useEmbedStore = create<EmbedState>(set => ({
  app: null,
  def: null,
  open: (app, def) => set({ app, def }),
  close: () => set({ app: null, def: null }),
}))

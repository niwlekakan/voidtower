import { create } from 'zustand'

interface CmdPaletteStore {
  open: boolean
  query: string
  setOpen: (open: boolean) => void
  toggle: () => void
  setQuery: (query: string) => void
}

export const useCmdPaletteStore = create<CmdPaletteStore>()((set) => ({
  open: false,
  query: '',
  setOpen: (open) => set({ open, query: '' }),
  toggle: () => set((s) => ({ open: !s.open, query: '' })),
  setQuery: (query) => set({ query }),
}))

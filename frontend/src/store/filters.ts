import { create } from 'zustand'
import { persist } from 'zustand/middleware'

interface FiltersState {
  globalTag: string | null
  setGlobalTag: (tag: string | null) => void
}

export const useFiltersStore = create<FiltersState>()(
  persist(
    (set) => ({
      globalTag: null,
      setGlobalTag: (tag) => set({ globalTag: tag }),
    }),
    { name: 'vt-filters' },
  ),
)
